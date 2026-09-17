#pragma once
#include "rust/cxx.h"
#include <cstddef>
#include <cstdint>
#include <string>

namespace oplab::decode {
enum class Access : std::uint8_t;
struct Instruction;
struct Window;

struct DecodeOptions {
  std::uint64_t base;
  std::size_t limit;
  bool details;
};
[[nodiscard]] Window x86_window(rust::Slice<const std::uint8_t> bytes,
                                const DecodeOptions &options);
[[nodiscard]] Window aarch64_window(rust::Slice<const std::uint8_t> bytes,
                                    const DecodeOptions &options);
void add_register(Instruction &instruction, const std::string &name, Access access);
} // namespace oplab::decode
