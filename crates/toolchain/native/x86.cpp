#include "decode_internal.hpp"
#include "oplab-toolchain/src/decode/ffi.rs.h"
#include "rust/cxx.h"
#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <llvm/ADT/StringRef.h>
#include <mutex>
#include <stdexcept>
#include <utility>
extern "C" {
#include <xed/xed-interface.h>
}

namespace oplab::decode {
namespace {
Access access(xed_operand_action_enum_t action) {
  switch (action) {
  case XED_OPERAND_ACTION_R:
    return Access::Read;
  case XED_OPERAND_ACTION_W:
    return Access::Write;
  case XED_OPERAND_ACTION_RW:
    return Access::ReadWrite;
  case XED_OPERAND_ACTION_CR:
    return Access::ConditionalRead;
  case XED_OPERAND_ACTION_CW:
    return Access::ConditionalWrite;
  case XED_OPERAND_ACTION_RCW:
    return Access::ReadConditionalWrite;
  case XED_OPERAND_ACTION_CRW:
    return Access::ConditionalReadWrite;
  default:
    return Access::Unknown;
  }
}

void reg(Instruction &out, xed_reg_enum_t value, Access mode) {
  // XED models implicit stack updates with pseudo-registers. This decoder is long mode only.
  if (value == XED_REG_STACKPUSH || value == XED_REG_STACKPOP) {
    value = XED_REG_RSP;
  }
  if (value == XED_REG_INVALID || value == XED_REG_RFLAGS || value == XED_REG_EFLAGS ||
      value == XED_REG_FLAGS) {
    return;
  }
  add_register(out, llvm::StringRef(xed_reg_enum_t2str(value)).lower(), mode);
}

Flow flow(const xed_decoded_inst_t &inst) {
  const auto opcode = xed_decoded_inst_get_iclass(&inst);
  if (opcode == XED_ICLASS_XBEGIN || opcode == XED_ICLASS_XABORT || opcode == XED_ICLASS_XEND) {
    return Flow::Transaction;
  }
  if (opcode == XED_ICLASS_UD0 || opcode == XED_ICLASS_UD1 || opcode == XED_ICLASS_UD2) {
    return Flow::Exception;
  }
  const bool direct = xed_decoded_inst_get_branch_displacement_width(&inst) != 0;
  switch (xed_decoded_inst_get_category(&inst)) {
  case XED_CATEGORY_CALL:
    return direct ? Flow::Call : Flow::IndirectCall;
  case XED_CATEGORY_SYSCALL:
    return Flow::Call;
  case XED_CATEGORY_UNCOND_BR:
    return direct ? Flow::Branch : Flow::IndirectBranch;
  case XED_CATEGORY_COND_BR:
    return Flow::ConditionalBranch;
  case XED_CATEGORY_RET:
  case XED_CATEGORY_SYSRET:
    return Flow::Return;
  case XED_CATEGORY_INTERRUPT:
    return Flow::Interrupt;
  default:
    return Flow::Next;
  }
}

Flags flag_effects(const xed_decoded_inst_t &inst) {
  Flags out{};
  const auto *info = xed_decoded_inst_get_rflags_info(&inst);
  if (!info) {
    return out;
  }
  for (unsigned i = 0; i < xed_simple_flag_get_nflags(info); ++i) {
    const auto *flag = xed_simple_flag_get_flag_action(info, i);
    const auto name =
        llvm::StringRef(xed_flag_enum_t2str(xed_flag_action_get_flag_name(flag))).upper();
    switch (xed_flag_action_get_action(flag, 0)) {
    case XED_FLAG_ACTION_tst:
      out.read.push_back(name);
      break;
    case XED_FLAG_ACTION_u:
      out.undefined.push_back(name);
      break;
    case XED_FLAG_ACTION_0:
      out.cleared.push_back(name);
      break;
    case XED_FLAG_ACTION_1:
      out.set.push_back(name);
      break;
    case XED_FLAG_ACTION_mod:
    case XED_FLAG_ACTION_pop:
    case XED_FLAG_ACTION_ah:
      out.written.push_back(name);
      break;
    default:
      break;
    }
  }
  return out;
}

void analyze(const xed_decoded_inst_t &inst, std::uint64_t pc, Instruction &out) {
  out.flow = flow(inst);
  out.isa = xed_isa_set_enum_t2str(xed_decoded_inst_get_isa_set(&inst));
  out.privileged = xed_decoded_inst_get_attribute(&inst, XED_ATTRIBUTE_RING0) != 0;
  out.has_branch = xed_decoded_inst_get_branch_displacement_width(&inst) != 0;
  if (out.has_branch) {
    out.branch =
        pc + out.size + static_cast<std::uint64_t>(xed_decoded_inst_get_branch_displacement(&inst));
  }
  const auto *description = xed_decoded_inst_inst(&inst);
  for (unsigned i = 0; i < xed_inst_noperands(description); ++i) {
    const auto *operand = xed_inst_operand(description, i);
    const auto name = xed_operand_name(operand);
    const auto mode = access(xed_decoded_inst_operand_action(&inst, i));
    if (xed_operand_is_register(name)) {
      reg(out, xed_decoded_inst_get_reg(&inst, name), mode);
    }
    if (name == XED_OPERAND_MEM0 || name == XED_OPERAND_MEM1) {
      const unsigned index = name == XED_OPERAND_MEM0 ? 0 : 1;
      reg(out, xed_decoded_inst_get_base_reg(&inst, index), Access::Read);
      reg(out, xed_decoded_inst_get_index_reg(&inst, index), Access::Read);
      const auto segment = xed_decoded_inst_get_seg_reg(&inst, index);
      if (segment == XED_REG_FS || segment == XED_REG_GS) {
        reg(out, segment, Access::Read);
      }
      if ((xed_decoded_inst_mem_read(&inst, index) || xed_decoded_inst_mem_written(&inst, index)) &&
          !xed_decoded_inst_is_prefetch(&inst) &&
          xed_decoded_inst_get_iclass(&inst) != XED_ICLASS_INVLPG) {
        out.memory.push_back(Memory{
            .access = mode, .bytes = xed_decoded_inst_get_memory_operand_length(&inst, index)});
      }
    }
    if (name == XED_OPERAND_AGEN) {
      reg(out, xed_decoded_inst_get_base_reg(&inst, 0), Access::Read);
      reg(out, xed_decoded_inst_get_index_reg(&inst, 0), Access::Read);
    }
  }
  const auto category = xed_decoded_inst_get_category(&inst);
  const auto opcode = xed_decoded_inst_get_iclass(&inst);
  out.registers_incomplete = category == XED_CATEGORY_XSAVE || category == XED_CATEGORY_XSAVEOPT ||
                             opcode == XED_ICLASS_FXSAVE || opcode == XED_ICLASS_FXSAVE64 ||
                             opcode == XED_ICLASS_FXRSTOR || opcode == XED_ICLASS_FXRSTOR64;
  out.flags = flag_effects(inst);
}
} // namespace

Window x86_window(rust::Slice<const std::uint8_t> bytes, const DecodeOptions &options) {
  const auto &[base, limit, details] = options;
  static std::once_flag initialized;
  std::call_once(initialized, xed_tables_init);
  Window out{};
  while (out.consumed < bytes.size() && out.instructions.size() < limit) {
    xed_decoded_inst_t inst{};
    const xed_state_t state{.mmode = XED_MACHINE_MODE_LONG_64,
                            .stack_addr_width = XED_ADDRESS_WIDTH_64b};
    xed_decoded_inst_zero_set_mode(&inst, &state);
    const auto length = static_cast<unsigned>(
        std::min(bytes.size() - out.consumed, std::size_t{XED_MAX_INSTRUCTION_BYTES}));
    if (xed_decode(&inst, bytes.data() + out.consumed, length) != XED_ERROR_NONE) {
      break;
    }
    Instruction instruction{};
    instruction.size = xed_decoded_inst_get_length(&inst);
    std::array<char, 512> text{};
    if (!xed_format_context(XED_SYNTAX_INTEL, &inst, text.data(), static_cast<int>(text.size()),
                            base + out.consumed, nullptr, nullptr)) {
      throw std::runtime_error("XED formatting failed");
    }
    instruction.text = text.data();
    if (details) {
      analyze(inst, base + out.consumed, instruction);
    }
    out.consumed += instruction.size;
    out.instructions.push_back(std::move(instruction));
  }
  return out;
}
} // namespace oplab::decode
