#include "qemu/osdep.h"

#include "internal.h"

#include "cpu.h"
#include "tcg/tcg.h"

#include "accel/tcg/cpu-ops.h"
#include "exec/cpu-common.h"
#include "exec/helper-head.h.inc"
#include "hw/core/cpu.h"
#include "qemu/rcu.h"
#include "tcg/helper-info.h"
#include "tcg/tcg-internal.h"

static OplabAbiType abi_type(unsigned code) {
  switch (code) {
  case dh_typecode_void:
    return (OplabAbiType){0};
  case dh_typecode_i32:
    return (OplabAbiType){.bits = 32};
  case dh_typecode_s32:
    return (OplabAbiType){.bits = 32, .sign_extend = true};
  case dh_typecode_i64:
    return (OplabAbiType){.bits = 64};
  case dh_typecode_s64:
    return (OplabAbiType){.bits = 64, .sign_extend = true};
  case dh_typecode_ptr:
    return (OplabAbiType){.bits = 64, .pointer = true};
  case dh_typecode_i128:
    return (OplabAbiType){.bits = 128};
  default:
    g_assert_not_reached();
  }
}

static const char *condition(TCGCond value) {
  switch (value) {
#define COND(name)                                                                                 \
  case TCG_COND_##name:                                                                            \
    return #name
    COND(NEVER);
    COND(ALWAYS);
    COND(EQ);
    COND(NE);
    COND(TSTEQ);
    COND(TSTNE);
    COND(LT);
    COND(GE);
    COND(LE);
    COND(GT);
    COND(LTU);
    COND(GEU);
    COND(LEU);
    COND(GTU);
#undef COND
  default:
    g_assert_not_reached();
  }
}

void oplab_block_free(OplabBlock *block) {
  g_free((void *)block->temps);
  g_free((void *)block->ops);
  *block = (OplabBlock){0};
}

static OplabTemp export_temp(const TCGTemp *source) {
  OplabTemp temp = {.bits = 8 * tcg_type_size(source->type)};
  switch (source->kind) {
  case TEMP_FIXED:
    temp.storage = OPLAB_ENV;
    break;
  case TEMP_GLOBAL:
    temp.storage = OPLAB_GLOBAL;
    temp.offset = source->mem_offset;
    temp.base = temp_idx(source->mem_base);
    break;
  case TEMP_CONST:
    temp.storage = OPLAB_CONSTANT;
    temp.value = source->val;
    break;
  default:
    temp.storage = OPLAB_LOCAL;
    break;
  }
  return temp;
}

static OplabStop export_block(const TCGContext *s, OplabBlock *block) {
  if (s->nb_ops < 0 || s->nb_ops > 16384) {
    return OPLAB_INTERNAL;
  }
  OplabTemp *temps = g_new0(OplabTemp, s->nb_temps);
  block->temps = temps;
  block->temp_count = s->nb_temps;
  for (int i = 0; i < s->nb_temps; ++i) {
    temps[i] = export_temp(&s->temps[i]);
  }
  TCGOp *source;
  static_assert(INSN_START_WORDS <= G_N_ELEMENTS(block->restore));
  OplabOp *ops = g_new0(OplabOp, s->nb_ops);
  block->ops = ops;
  block->op_count = s->nb_ops;
  size_t index = 0;
  QTAILQ_FOREACH(source, &s->ops, link) {
    OplabOp *op = &ops[index++];
    static_assert(MAX_CALL_IARGS + 1 <= G_N_ELEMENTS(op->signature));
    static_assert(TCG_MAX_OP_ARGS <= G_N_ELEMENTS(op->args));
    const TCGOpDef *definition = &tcg_op_defs[source->opc];
    op->name = definition->name;
#if defined(__aarch64__)
    /* The AArch64 host expands shlv_vec to USHL, whose signed low-byte count
     * also represents right shifts. Preserve that host TCG operation's semantics. */
    if (source->opc == INDEX_op_shlv_vec) {
      op->name = "aa64_ushl_vec";
    }
#endif
    op->bits = definition->flags & (TCG_OPF_INT | TCG_OPF_VECTOR)
                   ? 8 * tcg_type_size(TCGOP_TYPE(source))
                   : 0;
    op->lane = 8 << TCGOP_VECE(source);
    op->outputs = definition->nb_oargs;
    op->inputs = definition->nb_iargs;
    op->constants = definition->nb_cargs;
    if (source->opc == INDEX_op_call) {
      const TCGHelperInfo *info = tcg_call_info(source);
      op->outputs = TCGOP_CALLO(source);
      op->inputs = TCGOP_CALLI(source);
      op->constants = 0;
      op->helper = (uintptr_t)tcg_call_func(source);
      for (unsigned i = 0; i < G_N_ELEMENTS(op->signature); ++i) {
        op->signature[i] = abi_type((info->typemask >> (3 * i)) & 7);
      }
#if defined(TARGET_X86_64)
      if (!strcmp(info->name, "syscall") || !strcmp(info->name, "sysenter")) {
        return OPLAB_ENVIRONMENT;
      }
#endif
    }
    unsigned args = op->outputs + op->inputs + op->constants;
    if (args > G_N_ELEMENTS(op->args)) {
      return OPLAB_INTERNAL;
    }
    for (unsigned i = 0; i < args; ++i) {
      op->args[i] =
          i < op->outputs + op->inputs ? temp_idx(arg_temp(source->args[i])) : source->args[i];
    }
    unsigned k = op->outputs + op->inputs;
    switch (source->opc) {
    case INDEX_op_brcond:
    case INDEX_op_setcond:
    case INDEX_op_negsetcond:
    case INDEX_op_movcond:
    case INDEX_op_cmp_vec:
    case INDEX_op_cmpsel_vec:
      op->condition = condition(source->args[k]);
      break;
    default:
      break;
    }
    if (source->opc == INDEX_op_br || source->opc == INDEX_op_set_label) {
      op->args[k] = arg_label(source->args[k])->id;
    }
    if (source->opc == INDEX_op_brcond) {
      op->args[k + 1] = arg_label(source->args[k + 1])->id;
    }
    if (source->opc == INDEX_op_bswap16 || source->opc == INDEX_op_bswap32) {
      op->args[k] = !!(source->args[k] & TCG_BSWAP_OS);
    }
    if (source->opc == INDEX_op_insn_start) {
      for (unsigned i = 0; i < INSN_START_WORDS; ++i) {
        block->restore[i] = tcg_get_insn_start_param(source, i);
      }
    }
  }
  return OPLAB_FINISHED;
}

static void exec_enter(OplabCpu *owner) {
  owner->exit = (OplabExit){.stop = OPLAB_FINISHED};
  owner->cpu->exception_index = -1;
  if (owner->cpu->cc->tcg_ops->cpu_exec_enter) {
    owner->cpu->cc->tcg_ops->cpu_exec_enter(owner->cpu);
  }
}

static OplabExit exec_leave(OplabCpu *owner) {
  if (owner->cpu->cc->tcg_ops->cpu_exec_exit) {
    owner->cpu->cc->tcg_ops->cpu_exec_exit(owner->cpu);
  }
  if (owner->exit.stop == OPLAB_FINISHED && owner->cpu->exception_index >= 0) {
    int exception = owner->cpu->exception_index;
    owner->exit.exception = exception;
    owner->exit.stop = OPLAB_EXCEPTION;
    if (exception == EXCP_HLT || exception == EXCP_HALTED) {
      owner->exit.stop = OPLAB_ENVIRONMENT;
    }
#if defined(TARGET_X86_64)
    if (exception == EXCP06_ILLOP) {
      owner->exit.stop = OPLAB_INVALID;
    }
    if (exception == EXCP03_INT3) {
      owner->exit.stop = OPLAB_ENVIRONMENT;
    }
#else
    if (exception == EXCP_UDEF) {
      owner->exit.stop = OPLAB_INVALID;
    }
    if (exception == EXCP_SWI || exception == EXCP_HVC || exception == EXCP_SMC ||
        exception == EXCP_BKPT) {
      owner->exit.stop = OPLAB_ENVIRONMENT;
    }
#endif
  }
  return owner->exit;
}

static OplabExit translate_cpu(OplabCpu *owner, OplabBlock *block) {
  exec_enter(owner);
  *block = (OplabBlock){0};
  const TCGCPUOps *ops = owner->cpu->cc->tcg_ops;
  TCGTBCPUState state = ops->get_tb_cpu_state(owner->cpu);
  owner->block = (TranslationBlock){
      .pc = state.pc,
      .cs_base = state.cs_base,
      .flags = state.flags,
      .cflags = CF_NO_GOTO_TB | CF_NO_GOTO_PTR | CF_NOIRQ | CF_SINGLE_STEP,
  };
  block->pc = state.pc;
  block->cs_base = state.cs_base;
  block->flags = state.flags;
  if (!sigsetjmp(owner->cpu->jmp_env, 0)) {
    CPUTLBEntryFull page;
    oplab_tlb_fill(owner->cpu, &page, state.pc, MMU_INST_FETCH, 0, 0, 1, false, 0);
    OplabMap *map = oplab_mapping(owner, state.pc, 1);
    if (sigsetjmp(tcg_ctx->jmp_trans, 0)) {
      owner->exit.stop = OPLAB_INTERNAL;
    } else {
      int count = 1;
      tcg_func_start(tcg_ctx);
      tcg_ctx->addr_type = TCG_TYPE_I64;
      tcg_ctx->cpu = owner->cpu;
      tcg_ctx->gen_tb = &owner->block;
      ops->translate_code(owner->cpu, &owner->block, &count, state.pc,
                          memory_region_get_ram_ptr(&map->memory) + state.pc - map->base);
      block->size = owner->block.size;
      owner->exit.stop = export_block(tcg_ctx, block);
      if (block->size > sizeof(block->code)) {
        owner->exit.stop = OPLAB_INTERNAL;
      } else {
        for (unsigned i = 0; i < block->size; ++i) {
          OplabMap *part = oplab_mapping(owner, state.pc + i, 1);
          if (!part) {
            owner->exit.stop = OPLAB_INTERNAL;
            break;
          }
          block->code[i] =
              ((uint8_t *)memory_region_get_ram_ptr(&part->memory))[state.pc + i - part->base];
        }
      }
    }
  }
  OplabExit result = exec_leave(owner);
  if (result.stop != OPLAB_FINISHED) {
    oplab_block_free(block);
  }
  return result;
}

static OplabExit execute_cpu(OplabCpu *owner, const OplabBlock *block, uintptr_t entry) {
  exec_enter(owner);
  owner->block.pc = block->pc;
  owner->block.cs_base = block->cs_base;
  owner->block.flags = block->flags;
  if (sigsetjmp(owner->cpu->jmp_env, 0)) {
    owner->cpu->cc->tcg_ops->restore_state_to_opc(owner->cpu, &owner->block, block->restore);
  } else {
    ((void (*)(void *))entry)(cpu_env(owner->cpu));
  }
  OplabExit result = exec_leave(owner);
#if defined(TARGET_X86_64)
  CPUX86State *env = cpu_env(owner->cpu);
  result.repeated =
      result.stop == OPLAB_FINISHED && env->eip == block->pc && (env->eflags & RF_MASK);
#endif
  return result;
}

typedef struct {
  TCGContext *context;
  OplabBlock block;
  uintptr_t entry;
  OplabExit result;
} Work;

static void dispatch(CPUState *cpu, run_on_cpu_data data) {
  Work *work = data.host_ptr;
  tcg_ctx = work->context;
  rcu_read_lock();
  work->result = work->entry ? execute_cpu(cpu->opaque, &work->block, work->entry)
                             : translate_cpu(cpu->opaque, &work->block);
  rcu_read_unlock();
}

OplabExit oplab_translate(OplabCpu *owner, OplabBlock *block) {
  oplab_enter();
  Work work = {.context = tcg_ctx};
  run_on_cpu(owner->cpu, dispatch, RUN_ON_CPU_HOST_PTR(&work));
  oplab_leave();
  *block = work.block;
  return work.result;
}

OplabExit oplab_execute(OplabCpu *owner, const OplabBlock *block, uintptr_t entry) {
  oplab_enter();
  Work work = {.context = tcg_ctx, .block = *block, .entry = entry};
  run_on_cpu(owner->cpu, dispatch, RUN_ON_CPU_HOST_PTR(&work));
  oplab_leave();
  return work.result;
}
