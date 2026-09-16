// LLVM owns parsing, target encodings, layout, relaxation, and ELF relocations.
// The bridge bounds emitted bytes and returns structured diagnostics.
#include "oplab-engine/native/assembly.hpp"
#include "oplab-engine/src/assembly/ffi.rs.h"
#include "rust/cxx.h"

#include <llvm/ADT/IntrusiveRefCntPtr.h>
#include <llvm/ADT/StringRef.h>
#include <llvm/Config/llvm-config.h>
#include <llvm/MC/MCAsmBackend.h>
#include <llvm/MC/MCAsmInfo.h>
#include <llvm/MC/MCCodeEmitter.h>
#include <llvm/MC/MCContext.h>
#include <llvm/MC/MCInstrInfo.h>
#include <llvm/MC/MCObjectFileInfo.h>
#include <llvm/MC/MCObjectWriter.h>
#include <llvm/MC/MCParser/MCAsmParser.h>
#include <llvm/MC/MCParser/MCAsmParserExtension.h>
#include <llvm/MC/MCParser/MCTargetAsmParser.h>
#include <llvm/MC/MCRegisterInfo.h>
#include <llvm/MC/MCSection.h>
#include <llvm/MC/MCStreamer.h>
#include <llvm/MC/MCSubtargetInfo.h>
#include <llvm/MC/MCTargetOptions.h>
#include <llvm/MC/TargetRegistry.h>
#include <llvm/Support/Alignment.h>
#include <llvm/Support/MemoryBuffer.h>
#include <llvm/Support/SourceMgr.h>
#include <llvm/Support/TargetSelect.h>
#include <llvm/Support/VirtualFileSystem.h>
#include <llvm/Support/raw_ostream.h>
#include <llvm/TargetParser/Triple.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <memory>
#include <mutex>
#include <optional>
#include <span>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace {
static_assert(LLVM_VERSION_MAJOR == 23, "the MC bridge supports LLVM 23");

constexpr std::size_t max_source = 256uz * 1024uz;
constexpr std::size_t max_object = 1024uz * 1024uz;
static_assert(max_source <= std::numeric_limits<std::uint32_t>::max());

// The object writer backpatches headers. Storage stops growing at the budget,
// while the stream's logical position continues to honor raw_pwrite_stream.
class BoundedOutput final : public llvm::raw_pwrite_stream {
public:
  BoundedOutput() : raw_pwrite_stream(true) {}

  [[nodiscard]] bool exceeded() const noexcept { return exceeded_; }
  [[nodiscard]] rust::Vec<std::uint8_t> take() && noexcept { return std::move(bytes_); }

private:
  rust::Vec<std::uint8_t> bytes_;
  std::uint64_t position_ = 0;
  bool exceeded_ = false;

  void write_impl(const char *data, std::size_t size) override {
    const auto remaining = std::numeric_limits<std::uint64_t>::max() - position_;
    position_ += std::min<std::uint64_t>(size, remaining);
    if (exceeded_ || size > remaining || size > max_object - bytes_.size()) {
      exceeded_ = true;
      return;
    }
    bytes_.reserve(bytes_.size() + size);
    for (const char byte : std::span{data, size}) {
      bytes_.push_back(static_cast<std::uint8_t>(byte));
    }
  }

  void pwrite_impl(const char *data, std::size_t size, std::uint64_t offset) override {
    if (exceeded_ || offset > bytes_.size() || size > bytes_.size() - offset) {
      exceeded_ = true;
      return;
    }
    const auto destination = std::span{bytes_.data(), bytes_.size()}.subspan(offset, size);
    std::ranges::transform(std::span{data, size}, destination.begin(),
                           [](char byte) { return static_cast<std::uint8_t>(byte); });
  }

  [[nodiscard]] std::uint64_t current_pos() const override { return position_; }
};

// Adopt owning factory pointers immediately; CXX translates failures to Rust errors.
template <typename T> [[nodiscard]] std::unique_ptr<T> own(T *value) {
  if (!value) {
    throw std::runtime_error("LLVM MC initialization failed");
  }
  return std::unique_ptr<T>{value};
}

// Reject .print through LLVM's directive parser: stdout carries the worker protocol.
class JobDirectives final : public llvm::MCAsmParserExtension {
public:
  void Initialize(llvm::MCAsmParser &parser) override {
    MCAsmParserExtension::Initialize(parser);
    parser.addDirectiveHandler(
        ".print", {this, HandleDirective<JobDirectives, &JobDirectives::rejectOutput>});
  }

private:
  bool rejectOutput(llvm::StringRef, llvm::SMLoc location) {
    return Error(location, "assembly jobs cannot write host output");
  }
};

[[nodiscard]] llvm::Triple guest_triple(oplab::Architecture architecture) {
  switch (architecture) {
  case oplab::Architecture::X86_64:
    return llvm::Triple("x86_64-unknown-linux-gnu");
  case oplab::Architecture::Aarch64:
    return llvm::Triple("aarch64-unknown-linux-gnu");
  }
  throw std::invalid_argument("unknown guest architecture");
}

// Follow SourceMgr parent locations when available; never infer macro offsets.
[[nodiscard]] std::optional<std::uint32_t> source_offset(const llvm::SourceMgr &sources,
                                                         llvm::SMLoc location) {
  const auto root = sources.getMainFileID();
  auto buffer = sources.FindBufferContainingLoc(location);
  while (buffer && buffer != root) {
    location = sources.getParentIncludeLoc(buffer);
    buffer = sources.FindBufferContainingLoc(location);
  }
  if (buffer != root) {
    return std::nullopt;
  }
  // The source limit fits u32 and SourceMgr established that both pointers belong
  // to the root buffer (including its end-of-input location).
  return static_cast<std::uint32_t>(location.getPointer() -
                                    sources.getMemoryBuffer(root)->getBufferStart());
}

void capture_diagnostics(llvm::MCContext &context, llvm::SourceMgr &sources,
                         oplab::ObjectResult &result) {
  context.setDiagnosticHandler([&sources, &result](const llvm::SMDiagnostic &diagnostic, bool,
                                                   const llvm::SourceMgr &,
                                                   std::vector<const llvm::MDNode *> &) {
    if (diagnostic.getKind() != llvm::SourceMgr::DK_Error ||
        result.status != oplab::Status::Success) {
      return;
    }
    result.status = oplab::Status::Assembly;
    const auto offset = source_offset(sources, diagnostic.getLoc());
    result.source_offset = offset.value_or(0);
    result.has_source_offset = offset.has_value();
  });
  // Route parser include/macro diagnostics through the same first-error handler.
  sources.setDiagHandler(
      [](const llvm::SMDiagnostic &diagnostic, void *owner) {
        static_cast<llvm::MCContext *>(owner)->diagnose(diagnostic);
      },
      &context);
}

void initialize_targets() {
  static std::once_flag once;
  std::call_once(once, [] {
    LLVMInitializeX86TargetInfo();
    LLVMInitializeX86TargetMC();
    LLVMInitializeX86AsmParser();
    LLVMInitializeAArch64TargetInfo();
    LLVMInitializeAArch64TargetMC();
    LLVMInitializeAArch64AsmParser();
  });
}
} // namespace

namespace oplab {
ObjectResult assemble_object(rust::Str source, Architecture architecture) {
  ObjectResult result{};
  if (source.size() > max_source) {
    result.status = Status::ResourceLimit;
    return result;
  }
  initialize_targets();
  const auto triple = guest_triple(architecture);
  std::string ignored;
  const auto *target = llvm::TargetRegistry::lookupTarget(triple, ignored);
  if (!target) {
    throw std::runtime_error("LLVM MC target is unavailable");
  }
  llvm::MCTargetOptions options;
  options.MCFatalWarnings = true;
  options.MCUseDwarfDirectory = llvm::MCTargetOptions::EnableDwarfDirectory;
  const std::unique_ptr<llvm::MCRegisterInfo> registers = own(target->createMCRegInfo(triple));
  const std::unique_ptr<llvm::MCInstrInfo> instructions = own(target->createMCInstrInfo());
  const std::unique_ptr<llvm::MCSubtargetInfo> subtarget =
      own(target->createMCSubtargetInfo(triple, "generic", ""));
  const auto info = own(target->createMCAsmInfo(*registers, triple, options));

  // Assembly inputs cannot read host files via .include or .incbin.
  llvm::SourceMgr sources(llvm::makeIntrusiveRefCnt<llvm::vfs::InMemoryFileSystem>());
  const llvm::StringRef text(source.data(), source.size());
  sources.AddNewSourceBuffer(llvm::MemoryBuffer::getMemBufferCopy(text, "source.s"), llvm::SMLoc());
  llvm::MCContext context(triple, *info, *registers, *subtarget, &sources);
  const auto object_info = own(target->createMCObjectFileInfo(context, false));
  context.setObjectFileInfo(object_info.get());
  context.setCompilationDir(".");
  context.setMainFileName("source.s");
  context.setGenDwarfForAssembly(true);
  context.setDwarfVersion(5);
  context.setGenDwarfRootFile("source.s", text);
  capture_diagnostics(context, sources, result);

  BoundedOutput output;
  auto backend = own(target->createMCAsmBackend(*subtarget, *registers, options));
  std::unique_ptr<llvm::MCCodeEmitter> emitter =
      own(target->createMCCodeEmitter(*instructions, context));
  std::unique_ptr<llvm::MCObjectWriter> writer = backend->createObjectWriter(output);
  auto streamer = own(target->createMCObjectStreamer(
      triple, context, std::move(backend), std::move(writer), std::move(emitter), *subtarget));
  const auto parser = own(llvm::createMCAsmParser(sources, context, *streamer, *info));
  const auto target_parser = own(target->createMCAsmParser(*subtarget, *parser, *instructions));
  streamer->initSections(*subtarget);
  // Snippets start with the ISA's minimum alignment, not the compiler's preferred
  // function alignment. Source directives and literal pools may raise this bound.
  object_info->getTextSection()->setAlignment(llvm::Align(triple.isAArch64() ? 4 : 1));
  parser->setAssemblerDialect(1);
  parser->setTargetParser(*target_parser);
  JobDirectives directives;
  directives.Initialize(*parser);

  // Run finalizes pools, DWARF, layout, fixups and object emission exactly once.
  // A separate layout() preflight would mutate the assembler before finalization.
  const bool failed = parser->Run(true);
  if (output.exceeded()) {
    result.status = Status::ResourceLimit;
  } else if (failed || context.hadError() || result.status != Status::Success) {
    result.status = Status::Assembly;
  } else {
    result.object = std::move(output).take();
  }
  return result;
}
} // namespace oplab
