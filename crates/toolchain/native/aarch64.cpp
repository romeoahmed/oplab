#include "decode_internal.hpp"
#include "oplab-toolchain/src/decode/ffi.rs.h"
#include "rust/cxx.h"
#include "target.hpp"
#include <cstddef>
#include <cstdint>
#include <llvm/ADT/ArrayRef.h>
#include <llvm/ADT/StringRef.h>
#include <llvm/MC/MCAsmInfo.h>
#include <llvm/MC/MCContext.h>
#include <llvm/MC/MCDisassembler/MCDisassembler.h>
#include <llvm/MC/MCInst.h>
#include <llvm/MC/MCInstPrinter.h>
#include <llvm/MC/MCInstrAnalysis.h>
#include <llvm/MC/MCInstrDesc.h>
#include <llvm/MC/MCInstrInfo.h>
#include <llvm/MC/MCRegisterInfo.h>
#include <llvm/MC/MCSubtargetInfo.h>
#include <llvm/MC/MCTargetOptions.h>
#include <llvm/MC/TargetRegistry.h>
#include <llvm/Support/raw_ostream.h>
#include <llvm/TargetParser/Triple.h>
#include <stdexcept>
#include <string>
#include <utility>

namespace oplab::decode {
namespace {
void analyze(const llvm::MCInst &inst, const llvm::MCInstrInfo &info,
             const llvm::MCRegisterInfo &registers, const llvm::MCInstrAnalysis &analysis,
             std::uint64_t pc, Instruction &out) {
  const auto &description = info.get(inst.getOpcode());
  for (unsigned i = 0; i < inst.getNumOperands(); ++i) {
    const auto &operand = inst.getOperand(i);
    if (!operand.isReg() || !operand.getReg()) {
      continue;
    }
    const auto name = llvm::StringRef(registers.getName(operand.getReg())).lower();
    // Zero registers are constants, not observable storage.
    if (name == "xzr" || name == "wzr") {
      continue;
    }
    add_register(out, name, i < description.getNumDefs() ? Access::Write : Access::Read);
    if (i < description.getNumOperands() &&
        description.getOperandConstraint(i, llvm::MCOI::TIED_TO) >= 0 &&
        (description.mayLoad() || description.mayStore())) {
      out.writeback = true;
    }
  }
  for (auto const reg : description.implicit_uses()) {
    add_register(out, llvm::StringRef(registers.getName(reg)).lower(), Access::Read);
  }
  for (auto const reg : description.implicit_defs()) {
    const auto name = llvm::StringRef(registers.getName(reg)).lower();
    out.updates_flags |= name == "nzcv";
    add_register(out, name, Access::Write);
  }
  if (description.mayLoad() || description.mayStore()) {
    const auto mode = description.mayLoad()
                          ? (description.mayStore() ? Access::ReadWrite : Access::Read)
                          : Access::Write;
    out.memory.push_back(Memory{.access = mode, .bytes = 0});
  }
  out.has_branch = analysis.evaluateBranch(inst, pc, out.size, out.branch);
  if (analysis.isCall(inst)) {
    out.flow = out.has_branch ? Flow::Call : Flow::IndirectCall;
    out.groups.push_back("call");
  } else if (analysis.isReturn(inst)) {
    out.flow = Flow::Return;
    out.groups.push_back("return");
  } else if (analysis.isConditionalBranch(inst)) {
    out.flow = Flow::ConditionalBranch;
    out.groups.push_back("branch");
  } else if (analysis.isBranch(inst)) {
    out.flow = out.has_branch ? Flow::Branch : Flow::IndirectBranch;
    out.groups.push_back("branch");
  }
}
} // namespace

Window aarch64_window(rust::Slice<const std::uint8_t> bytes, const DecodeOptions &options) {
  const auto &[base, limit, details] = options;
  initialize_targets();
  const llvm::Triple triple("aarch64-unknown-linux-gnu");
  std::string error;
  const auto *target = llvm::TargetRegistry::lookupTarget(triple, error);
  if (!target) {
    throw std::runtime_error("LLVM AArch64 target unavailable");
  }
  const auto registers = own(target->createMCRegInfo(triple));
  const auto info = own(target->createMCInstrInfo());
  const auto asm_info = own(target->createMCAsmInfo(*registers, triple, llvm::MCTargetOptions{}));
  const auto subtarget = own(target->createMCSubtargetInfo(triple, "generic", ""));
  // Inspection recognizes extensions independently of the selected execution profile.
  auto features = subtarget->getFeatureBits();
  for (const auto &feature : subtarget->getAllProcessorFeatures()) {
    features.set(feature.Value);
  }
  subtarget->setFeatureBits(features);
  llvm::MCContext context(triple, *asm_info, *registers, *subtarget);
  const auto decoder = own(target->createMCDisassembler(*subtarget, context));
  const auto printer = own(target->createMCInstPrinter(triple, 0, *asm_info, *info, *registers));
  const auto analysis = own(target->createMCInstrAnalysis(info.get()));
  printer->setPrintImmHex(true);
  Window out{};
  while (bytes.size() - out.consumed >= 4 && out.instructions.size() < limit) {
    llvm::MCInst inst;
    std::uint64_t size = 0;
    const auto pc = base + out.consumed;
    const auto status = decoder->getInstruction(
        inst, size, llvm::ArrayRef(bytes.data() + out.consumed, std::size_t{4}), pc, llvm::nulls());
    // SoftFail is architecturally invalid despite having a printable MCInst.
    if (status != llvm::MCDisassembler::Success || size != 4) {
      break;
    }
    Instruction instruction{};
    instruction.size = 4;
    std::string text;
    llvm::raw_string_ostream stream(text);
    printer->printInst(&inst, pc, "", *subtarget, stream);
    instruction.text = llvm::StringRef(text).trim().str();
    if (details) {
      analyze(inst, *info, *registers, *analysis, pc, instruction);
    }
    out.consumed += 4;
    out.instructions.push_back(std::move(instruction));
  }
  return out;
}
} // namespace oplab::decode
