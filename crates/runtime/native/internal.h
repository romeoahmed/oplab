#pragma once

/* QEMU translation units include qemu/osdep.h before this header. */
#include "exec/memop.h"
#include "exec/mmu-access-type.h"
#include "exec/translation-block.h"
#include "exec/vaddr.h"
#include "qemu.h"
#include "qom/object.h"
#include "system/memory.h"

static_assert(sizeof(uintptr_t) == 8 && HOST_BIG_ENDIAN == 0,
              "the QEMU/LLVM adapter requires a 64-bit little-endian host");

#define TYPE_OPLAB_CPU "oplab-cpu-owner"
OBJECT_DECLARE_SIMPLE_TYPE(OplabCpu, OPLAB_CPU)

typedef struct {
  MemoryRegion memory;
  uint64_t base;
  uint64_t size;
  uint32_t permissions;
} OplabMap;

struct OplabCpu {
  Object parent;
  CPUState *cpu;
  MemoryRegion memory;
  GPtrArray *maps;
  uint64_t mapped;
  OplabExit exit;
  TranslationBlock block;
};

void oplab_enter(void);
void oplab_leave(void);
void oplab_state_init(CPUState *);
OplabExit oplab_translate(OplabCpu *, OplabBlock *);
OplabExit oplab_execute(OplabCpu *, const OplabBlock *, uintptr_t);
void oplab_block_free(OplabBlock *);
bool oplab_state_read(OplabCpu *, OplabRegister, uint32_t, uint8_t *, size_t);
bool oplab_state_write(OplabCpu *, OplabRegister, uint32_t, const uint8_t *, size_t);
bool oplab_tlb_fill(CPUState *, CPUTLBEntryFull *, vaddr, MMUAccessType, int, MemOp, int, bool,
                    uintptr_t);
OplabMap *oplab_mapping(OplabCpu *, uint64_t, uint64_t);
uint64_t oplab_load(CPUArchState *, uint64_t, uint32_t);
void oplab_store(CPUArchState *, uint64_t, uint64_t, uint32_t);
void oplab_load_pair(CPUArchState *, uint64_t, uint32_t, uint64_t *);
void oplab_store_pair(CPUArchState *, uint64_t, uint64_t, uint64_t, uint32_t);
