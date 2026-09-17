#pragma once
#include "rust/cxx.h"
#include <cstddef>
#include <cstdint>
namespace oplab::decode {
enum class Architecture : std::uint8_t;
struct Window;
// Rust validates byte/instruction limits, alignment and address ranges before entering CXX.
[[nodiscard]] Window decode_window(rust::Slice<const std::uint8_t> bytes, std::uint64_t base,
                                   Architecture architecture, std::size_t limit, bool details);
} // namespace oplab::decode
