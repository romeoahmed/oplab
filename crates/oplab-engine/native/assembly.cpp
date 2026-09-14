// LLVM owns parsing, target encodings, layout, relaxation, and ELF relocations.
// Oplab supplies ownership, resource envelopes, and sanitized result values.
#include "oplab-engine/src/assembly/ffi.rs.h"

#include <lld/Common/Driver.h>
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
#include <llvm/Support/MemoryBuffer.h>
#include <llvm/Support/SourceMgr.h>
#include <llvm/Support/TargetSelect.h>
#include <llvm/Support/VirtualFileSystem.h>
#include <llvm/Support/raw_ostream.h>

#include <algorithm>
#include <array>
#include <cstdlib>
#include <limits>
#include <memory>
#include <mutex>
#include <span>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

LLD_HAS_DRIVER(elf)

namespace {
static_assert(LLVM_VERSION_MAJOR == 23, "the MC bridge supports LLVM 23");

constexpr std::size_t max_source = 256uz * 1024uz;
constexpr std::size_t max_object = 1024uz * 1024uz;

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

// LLVM's public factories transfer raw ownership. Adopt it immediately; CXX
// translates initialization exceptions into Rust errors without leaking handles.
template <typename T> [[nodiscard]] std::unique_ptr<T> own(T *value) {
  if (!value) {
    throw std::runtime_error("LLVM MC initialization failed");
  }
  return std::unique_ptr<T>{value};
}

// Standard syntax remains LLVM-owned; host output is outside an assembly job's
// capabilities because stdout carries the worker protocol.
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
ObjectResult assemble_object(rust::Str source, bool aarch64) {
  ObjectResult result{};
  if (source.size() > max_source) {
    result.status = Status::ResourceLimit;
    return result;
  }
  initialize_targets();
  const llvm::Triple triple(aarch64 ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu");
  std::string ignored;
  const auto *target = llvm::TargetRegistry::lookupTarget(triple, ignored);
  if (!target) {
    result.status = Status::BackendFailure;
    return result;
  }
  llvm::MCTargetOptions options;
  options.MCFatalWarnings = true;
  options.MCUseDwarfDirectory = llvm::MCTargetOptions::EnableDwarfDirectory;
  const auto registers = own(target->createMCRegInfo(triple));
  const auto instructions = own(target->createMCInstrInfo());
  const auto subtarget = own(target->createMCSubtargetInfo(triple, "generic", ""));
  const auto info = own(target->createMCAsmInfo(*registers, triple, options));

  // Assembly inputs cannot read host files via .include or .incbin.
  llvm::SourceMgr sources(llvm::makeIntrusiveRefCnt<llvm::vfs::InMemoryFileSystem>());
  const llvm::StringRef text(source.data(), source.size());
  const unsigned root = sources.AddNewSourceBuffer(
      llvm::MemoryBuffer::getMemBufferCopy(text, "source.s"), llvm::SMLoc());
  llvm::MCContext context(triple, *info, *registers, *subtarget, &sources);
  const auto object_info = own(target->createMCObjectFileInfo(context, false));
  context.setObjectFileInfo(object_info.get());
  context.setCompilationDir(".");
  context.setMainFileName("source.s");
  context.setGenDwarfForAssembly(true);
  context.setDwarfVersion(5);
  context.setGenDwarfRootFile("source.s", text);
  context.setDiagnosticHandler(
      [&sources, &result, root](const llvm::SMDiagnostic &diagnostic, bool, const llvm::SourceMgr &,
                                std::vector<const llvm::MDNode *> const &) {
        if (diagnostic.getKind() != llvm::SourceMgr::DK_Error || result.status != Status::Success) {
          return;
        }
        result.status = Status::Assembly;
        auto location = diagnostic.getLoc();
        unsigned buffer = sources.FindBufferContainingLoc(location);
        while (buffer && buffer != root) {
          location = sources.getParentIncludeLoc(buffer);
          buffer = sources.FindBufferContainingLoc(location);
        }
        if (buffer == root) {
          result.source_offset = static_cast<std::uint32_t>(
              location.getPointer() - sources.getMemoryBuffer(root)->getBufferStart());
          result.has_source_offset = true;
        }
      });
  // Installing a SourceMgr handler also suppresses the parser's raw include and
  // macro stacks. The MCContext handler above owns all published diagnostics.
  sources.setDiagHandler(
      [](const llvm::SMDiagnostic &diagnostic, void *owner) {
        static_cast<llvm::MCContext *>(owner)->diagnose(diagnostic);
      },
      &context);

  BoundedOutput output;
  auto backend = own(target->createMCAsmBackend(*subtarget, *registers, options));
  auto emitter = own(target->createMCCodeEmitter(*instructions, context));
  auto writer = backend->createObjectWriter(output);
  auto streamer = own(target->createMCObjectStreamer(
      triple, context, std::move(backend), std::move(writer), std::move(emitter), *subtarget));
  const auto parser = own(llvm::createMCAsmParser(sources, context, *streamer, *info));
  const auto target_parser = own(target->createMCAsmParser(*subtarget, *parser, *instructions));
  streamer->initSections(*subtarget);
  // Snippets start with the ISA's minimum alignment, not the compiler's preferred
  // function alignment. Source directives and literal pools may raise this bound.
  object_info->getTextSection()->setAlignment(llvm::Align(aarch64 ? 4 : 1));
  parser->setAssemblerDialect(1);
  parser->setTargetParser(*target_parser);
  JobDirectives directives;
  directives.Initialize(*parser);

  // Run the normal finalization exactly once: target literal pools, DWARF,
  // layout, fixups, relocations, then object emission. layout() is not a read-only
  // preflight and must not be called separately before this path.
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

bool link_object(rust::Str input, rust::Str output, rust::Str script, std::uint64_t base) {
  // LLD has a process-wide context. lldMain destroys it between successful runs;
  // the mutex also makes library callers safe before the worker queue is attached.
  static std::mutex mutex;
  const std::scoped_lock lock(mutex);
  const std::string input_path(input);
  const std::string output_path(output);
  const std::string script_path(script);
  const std::string entry = "--entry=" + std::to_string(base);
  // Objects cannot autoload host libraries. Rust checks section/segment ranges
  // with a wide exclusive end, including a valid last byte at UINT64_MAX.
  const std::array arguments = {"ld.lld",
                                "--no-relax",
                                "--build-id=none",
                                "--threads=1",
                                "--no-check-sections",
                                "--no-dependent-libraries",
                                "--fatal-warnings",
                                entry.c_str(),
                                "-T",
                                script_path.c_str(),
                                "-o",
                                output_path.c_str(),
                                input_path.c_str()};
  const std::array drivers = {lld::DriverDef{.f = lld::Gnu, .d = &lld::elf::link}};
  const auto result = lld::lldMain(arguments, llvm::nulls(), llvm::nulls(), drivers);
  if (!result.canRunAgain) {
    std::_Exit(70); // Discard an unusable native process, including its globals.
  }
  return result.retCode == 0;
}
} // namespace oplab
