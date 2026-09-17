#include "qemu/osdep.h"

#include "internal.h"

#include "tcg/tcg.h"

#include "accel/tcg/cpu-loop.h"
#include "exec/memattrs.h"
#include "exec/memopidx.h"
#include "exec/page-protection.h"
#include "exec/target_page.h"
#include "hw/core/cpu.h"
#include "qemu/int128.h"
#include "tcg/tcg-ldst.h"

OplabMap *oplab_mapping(OplabCpu *owner, uint64_t address, uint64_t size) {
  for (unsigned i = 0; i < owner->maps->len; ++i) {
    OplabMap *map = g_ptr_array_index(owner->maps, i);
    if (address >= map->base && address - map->base < map->size &&
        size <= map->size - (address - map->base)) {
      return map;
    }
  }
  return nullptr;
}

bool oplab_tlb_fill(CPUState *cpu, CPUTLBEntryFull *out, vaddr address, MMUAccessType access, int,
                    MemOp op, int size, bool probe, uintptr_t) {
  OplabCpu *owner = cpu->opaque;
  OplabMap *map = oplab_mapping(owner, address, 1);
  uint32_t permission = access == MMU_DATA_STORE ? 2 : access == MMU_INST_FETCH ? 4 : 1;
  OplabStop stop = !map                               ? OPLAB_UNMAPPED
                   : !(map->permissions & permission) ? OPLAB_PROTECTION
                                                      : OPLAB_FINISHED;
  if (stop == OPLAB_FINISHED && (address & ((1u << memop_alignment_bits(op)) - 1))) {
    stop = OPLAB_UNALIGNED;
  }
  if (stop != OPLAB_FINISHED) {
    if (probe) {
      return false;
    }
    owner->exit = (OplabExit){
        .stop = stop,
        .access = permission,
        .address = address,
        .size = size > 0 ? size : 1,
    };
    cpu_loop_exit(cpu);
  }
  *out = (CPUTLBEntryFull){
      .phys_addr = address & TARGET_PAGE_MASK,
      .prot = ((map->permissions & 1) ? PAGE_READ : 0) | ((map->permissions & 2) ? PAGE_WRITE : 0) |
              ((map->permissions & 4) ? PAGE_EXEC : 0),
      .lg_page_size = TARGET_PAGE_BITS,
      .attrs = MEMTXATTRS_UNSPECIFIED,
  };
  return true;
}

/* Keep MemOpIdx opaque; native helpers retain endianness, alignment and atomicity. */
uint64_t oplab_load(CPUArchState *env, uint64_t address, uint32_t op) {
  switch (get_memop(op) & MO_SSIZE) {
  case MO_UB:
    return helper_ldub_mmu(env, address, op, 0);
  case MO_SB:
    return helper_ldsb_mmu(env, address, op, 0);
  case MO_UW:
    return helper_lduw_mmu(env, address, op, 0);
  case MO_SW:
    return helper_ldsw_mmu(env, address, op, 0);
  case MO_UL:
    return helper_ldul_mmu(env, address, op, 0);
  case MO_SL:
    return helper_ldsl_mmu(env, address, op, 0);
  case MO_UQ:
    return helper_ldq_mmu(env, address, op, 0);
  default:
    g_assert_not_reached();
  }
}

void oplab_store(CPUArchState *env, uint64_t address, uint64_t value, uint32_t op) {
  switch (get_memop(op) & MO_SIZE) {
  case MO_8:
    helper_stb_mmu(env, address, value, op, 0);
    break;
  case MO_16:
    helper_stw_mmu(env, address, value, op, 0);
    break;
  case MO_32:
    helper_stl_mmu(env, address, value, op, 0);
    break;
  case MO_64:
    helper_stq_mmu(env, address, value, op, 0);
    break;
  default:
    g_assert_not_reached();
  }
}

void oplab_load_pair(CPUArchState *env, uint64_t address, uint32_t op, uint64_t *out) {
  Int128 value = helper_ld16_mmu(env, address, op, 0);
  out[0] = int128_getlo(value);
  out[1] = int128_gethi(value);
}

void oplab_store_pair(CPUArchState *env, uint64_t address, uint64_t low, uint64_t high,
                      uint32_t op) {
  helper_st16_mmu(env, address, int128_make128(low, high), op, 0);
}
