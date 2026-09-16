// LLD owns ELF placement and relocations; calls share its process-wide context.
#include "oplab-engine/native/assembly.hpp"
#include "oplab-engine/src/assembly/ffi.rs.h"

#include <lld/Common/Driver.h>
#include <lld/Common/ErrorHandler.h>
#include <llvm/Support/raw_ostream.h>

#include <array>
#include <mutex>
#include <string>

LLD_HAS_DRIVER(elf)

namespace oplab {
bool link_object(const LinkRequest &request) {
  // Protect LLD's process-wide context across callers; internal parallelism stays enabled.
  // lldMain tears down the context after each recoverable invocation.
  static std::mutex mutex;
  const std::scoped_lock lock(mutex);
  const std::string input_path(request.input);
  const std::string output_path(request.output);
  const std::string script_path(request.script);
  const std::string entry = "--entry=" + std::to_string(request.base);
  // Objects cannot autoload host libraries. Rust checks section/segment ranges
  // with a wide exclusive end, including a valid last byte at UINT64_MAX.
  const std::array arguments = {"ld.lld",
                                "--no-relax",
                                "--build-id=none",
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
    lld::exitLld(70); // LLD owns crash cleanup; this process cannot accept more work.
  }
  return result.retCode == 0;
}
} // namespace oplab
