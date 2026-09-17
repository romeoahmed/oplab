#pragma once
#include <stddef.h>
#include <stdint.h>

/* Private in-process ABI. CPU layouts and QEMU enum values stay inside the SDK. */
typedef enum { OPLAB_LOCAL, OPLAB_ENV, OPLAB_GLOBAL, OPLAB_CONSTANT } OplabStorage;
typedef struct {
  uint32_t bits;
  OplabStorage storage;
  uint32_t base;
  int64_t offset;
  uint64_t value;
} OplabTemp;

typedef struct {
  uint32_t bits;
  bool pointer;
  bool sign_extend;
} OplabAbiType;

/* Names have SDK lifetime. Arguments contain output/input temp indices, then constants;
 * bits and lane are widths in bits. Helper signatures start with the return type. */
typedef struct {
  const char *name;
  const char *condition;
  uint32_t bits;
  uint32_t lane;
  uint32_t outputs;
  uint32_t inputs;
  uint32_t constants;
  uint64_t args[16];
  uintptr_t helper;
  OplabAbiType signature[8];
} OplabOp;

/* translate transfers both arrays to the caller; release frees them and clears the block.
 * Keep the block alive through compilation and execution, including fault restoration. */
typedef struct {
  const OplabTemp *temps;
  size_t temp_count;
  const OplabOp *ops;
  size_t op_count;
  uint64_t pc;
  uint64_t cs_base;
  uint32_t flags;
  uint32_t size;
  uint64_t restore[3];
  uint8_t code[16];
} OplabBlock;

typedef enum {
  OPLAB_FINISHED,
  OPLAB_INVALID,
  OPLAB_ENVIRONMENT,
  OPLAB_EXCEPTION,
  OPLAB_UNMAPPED,
  OPLAB_PROTECTION,
  OPLAB_UNALIGNED,
  OPLAB_INTERNAL
} OplabStop;

typedef struct {
  OplabStop stop;
  uint32_t access;
  uint64_t address;
  uint64_t size;
  int32_t exception;
  bool repeated;
} OplabExit;

typedef enum {
  OPLAB_GPR,
  OPLAB_PC,
  OPLAB_FLAGS,
  OPLAB_VECTOR,
  OPLAB_CONTROL,
  OPLAB_STATUS,
  OPLAB_PREDICATE,
  OPLAB_VECTOR_LENGTH
} OplabRegister;
typedef struct OplabCpu OplabCpu;

/* One SDK per guest, retained for the process lifetime. Buffers are borrowed only for
 * each call. Memory permissions use R=1, W=2, X=4, unlike ELF permission bits. */
typedef struct {
  uint32_t abi;
  const char *version;
  OplabCpu *(*create)(void);
  void (*destroy)(OplabCpu *);
  bool (*map)(OplabCpu *, uint64_t base, uint64_t size, uint32_t permissions, const uint8_t *,
              size_t);
  bool (*read)(OplabCpu *, uint64_t address, uint8_t *, size_t);
  bool (*write)(OplabCpu *, uint64_t address, const uint8_t *, size_t);
  bool (*register_read)(OplabCpu *, OplabRegister, uint32_t index, uint8_t *, size_t);
  bool (*register_write)(OplabCpu *, OplabRegister, uint32_t index, const uint8_t *, size_t);
  OplabExit (*translate)(OplabCpu *, OplabBlock *);
  void (*release)(OplabBlock *);
  OplabExit (*execute)(OplabCpu *, const OplabBlock *, uintptr_t entry);
  uintptr_t load;
  uintptr_t store;
  uintptr_t load_pair;
  uintptr_t store_pair;
} OplabQemu;

const OplabQemu *oplab_qemu(void);
