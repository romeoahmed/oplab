#include "qemu/osdep.h"

#include "internal.h"

#include "cpu.h"
#include "hw/core/cpu.h"
#include "qemu/bswap.h"

#if defined(TARGET_X86_64)
void oplab_state_init(CPUState *cpu) {
  CPUX86State *env = cpu_env(cpu);
  memset(env->regs, 0, sizeof(env->regs));
  cpu_x86_update_cr0(env, CR0_PE_MASK | CR0_ET_MASK);
  env->hflags |= HF_LMA_MASK;
  env->efer |= MSR_EFER_LMA | MSR_EFER_LME;
  cpu_x86_load_seg_cache(env, R_CS, 3, 0, UINT32_MAX,
                         DESC_P_MASK | DESC_S_MASK | DESC_CS_MASK | DESC_R_MASK | DESC_L_MASK |
                             DESC_DPL_MASK);
  cpu_x86_load_seg_cache(env, R_SS, 0x2b, 0, UINT32_MAX,
                         DESC_P_MASK | DESC_S_MASK | DESC_W_MASK | DESC_B_MASK | DESC_DPL_MASK);
  cpu_x86_update_cr4(env, CR4_OSFXSR_MASK | CR4_OSXSAVE_MASK);
  env->xcr0 = env->features[FEAT_XSAVE_XCR0_LO] | (env->features[FEAT_XSAVE_XCR0_HI] << 32);
  cpu_sync_avx_hflag(env);
  env->eflags = 2;
  env->eip = 0;
  cpu_set_mxcsr(env, 0x1f80);
}

bool oplab_state_read(OplabCpu *owner, OplabRegister bank, uint32_t index, uint8_t *bytes,
                      size_t size) {
  CPUX86State *env = cpu_env(owner->cpu);
  uint64_t value;
  switch (bank) {
  case OPLAB_GPR:
    if (index >= CPU_NB_REGS || size != 8) {
      return false;
    }
    value = env->regs[index];
    break;
  case OPLAB_PC:
    if (size != 8) {
      return false;
    }
    value = env->eip;
    break;
  case OPLAB_FLAGS:
    if (size != 8) {
      return false;
    }
    value = env->eflags;
    break;
  case OPLAB_CONTROL:
  case OPLAB_STATUS:
    if (size != 4) {
      return false;
    }
    update_mxcsr_from_sse_status(env);
    stl_le_p(bytes, bank == OPLAB_CONTROL ? env->mxcsr : env->mxcsr & 0x3f);
    return true;
  case OPLAB_VECTOR_LENGTH:
    if (size != 8) {
      return false;
    }
    stl_le_p(bytes, 32);
    stl_le_p(bytes + 4, 32);
    return true;
  case OPLAB_VECTOR:
    if (index >= CPU_NB_REGS || (size != 16 && size != 32)) {
      return false;
    }
    for (size_t i = 0; i < size / 8; ++i) {
      stq_le_p(bytes + i * 8, env->xmm_regs[index].ZMM_Q(i));
    }
    return true;
  default:
    return false;
  }
  stq_le_p(bytes, value);
  return true;
}

bool oplab_state_write(OplabCpu *owner, OplabRegister bank, uint32_t index, const uint8_t *bytes,
                       size_t size) {
  CPUX86State *env = cpu_env(owner->cpu);
  switch (bank) {
  case OPLAB_GPR:
    if (index >= CPU_NB_REGS || size != 8) {
      return false;
    }
    env->regs[index] = ldq_le_p(bytes);
    return true;
  case OPLAB_PC:
    if (size != 8) {
      return false;
    }
    env->eip = ldq_le_p(bytes);
    return true;
  case OPLAB_FLAGS:
    if (size != 8) {
      return false;
    }
    env->eflags = ldq_le_p(bytes) | 2;
    return true;
  case OPLAB_CONTROL:
    if (size != 4) {
      return false;
    }
    cpu_set_mxcsr(env, ldl_le_p(bytes));
    return true;
  case OPLAB_VECTOR:
    if (index >= CPU_NB_REGS || (size != 16 && size != 32)) {
      return false;
    }
    for (size_t i = 0; i < size / 8; ++i) {
      env->xmm_regs[index].ZMM_Q(i) = ldq_le_p(bytes + i * 8);
    }
    return true;
  default:
    return false;
  }
}

#elif defined(TARGET_AARCH64)
#include "hw/core/registerfields.h"
#include "internals.h"

void oplab_state_init(CPUState *cpu) {
  CPUARMState *env = cpu_env(cpu);
  env->aarch64 = 1;
  pstate_write(env, PSTATE_MODE_EL0t);
  env->cp15.scr_el3 |= SCR_NS | SCR_RW;
  env->cp15.hcr_el2 |= HCR_RW;
  env->cp15.cpacr_el1 = FIELD_DP64(0, CPACR_EL1, FPEN, 3);
  env->cp15.cpacr_el1 = FIELD_DP64(env->cp15.cpacr_el1, CPACR_EL1, ZEN, 3);
  env->cp15.cpacr_el1 = FIELD_DP64(env->cp15.cpacr_el1, CPACR_EL1, SMEN, 3);
  env->cp15.cptr_el[2] = 0;
  env->cp15.cptr_el[3] = R_CPTR_EL3_EZ_MASK | R_CPTR_EL3_ESM_MASK;
  for (unsigned i = 1; i <= 3; ++i) {
    env->vfp.zcr_el[i] = ARM_MAX_VQ - 1;
  }
  vfp_set_fpcr(env, 0);
  vfp_set_fpsr(env, 0);
  arm_rebuild_hflags(env);
}

bool oplab_state_read(OplabCpu *owner, OplabRegister bank, uint32_t index, uint8_t *bytes,
                      size_t size) {
  CPUARMState *env = cpu_env(owner->cpu);
  uint64_t value;
  switch (bank) {
  case OPLAB_GPR:
    if (index >= G_N_ELEMENTS(env->xregs) || size != 8) {
      return false;
    }
    value = env->xregs[index];
    break;
  case OPLAB_PC:
    if (size != 8) {
      return false;
    }
    value = env->pc;
    break;
  case OPLAB_FLAGS:
    if (size != 8) {
      return false;
    }
    value = pstate_read(env) & PSTATE_NZCV;
    break;
  case OPLAB_CONTROL:
  case OPLAB_STATUS:
    if (size != 4) {
      return false;
    }
    stl_le_p(bytes, bank == OPLAB_CONTROL ? vfp_get_fpcr(env) : vfp_get_fpsr(env));
    return true;
  case OPLAB_VECTOR_LENGTH:
    if (size != 8) {
      return false;
    }
    stl_le_p(bytes, (sve_vqm1_for_el(env, arm_current_el(env)) + 1) * 16);
    stl_le_p(bytes + 4, arm_max_vq(ARM_CPU(owner->cpu)) * 16);
    return true;
  case OPLAB_PREDICATE:
    if (index >= G_N_ELEMENTS(env->vfp.pregs) ||
        size != (size_t)arm_max_vq(ARM_CPU(owner->cpu)) * 2) {
      return false;
    }
    /* The SDK requires a little-endian host; predicate bit zero is byte zero's low bit. */
    memcpy(bytes, env->vfp.pregs[index].p, size);
    return true;
  case OPLAB_VECTOR:
    if (index >= G_N_ELEMENTS(env->vfp.zregs) || size == 0 || size % 16 != 0 ||
        size > (size_t)arm_max_vq(ARM_CPU(owner->cpu)) * 16) {
      return false;
    }
    for (size_t i = 0; i < size / 8; ++i) {
      stq_le_p(bytes + i * 8, env->vfp.zregs[index].d[i]);
    }
    return true;
  default:
    return false;
  }
  stq_le_p(bytes, value);
  return true;
}

bool oplab_state_write(OplabCpu *owner, OplabRegister bank, uint32_t index, const uint8_t *bytes,
                       size_t size) {
  CPUARMState *env = cpu_env(owner->cpu);
  switch (bank) {
  case OPLAB_GPR:
    if (index >= G_N_ELEMENTS(env->xregs) || size != 8) {
      return false;
    }
    env->xregs[index] = ldq_le_p(bytes);
    return true;
  case OPLAB_PC:
    if (size != 8) {
      return false;
    }
    env->pc = ldq_le_p(bytes);
    return true;
  case OPLAB_FLAGS:
    if (size != 8) {
      return false;
    }
    pstate_write(env, (pstate_read(env) & ~PSTATE_NZCV) | (ldq_le_p(bytes) & PSTATE_NZCV));
    return true;
  case OPLAB_CONTROL:
    if (size != 4) {
      return false;
    }
    vfp_set_fpcr(env, ldl_le_p(bytes));
    return true;
  case OPLAB_PREDICATE:
    if (index >= G_N_ELEMENTS(env->vfp.pregs) ||
        size != (size_t)arm_max_vq(ARM_CPU(owner->cpu)) * 2) {
      return false;
    }
    memcpy(env->vfp.pregs[index].p, bytes, size);
    return true;
  case OPLAB_VECTOR:
    if (index >= G_N_ELEMENTS(env->vfp.zregs) || size == 0 || size % 16 != 0 ||
        size > (size_t)arm_max_vq(ARM_CPU(owner->cpu)) * 16) {
      return false;
    }
    for (size_t i = 0; i < size / 8; ++i) {
      env->vfp.zregs[index].d[i] = ldq_le_p(bytes + i * 8);
    }
    return true;
  default:
    return false;
  }
}
#endif
