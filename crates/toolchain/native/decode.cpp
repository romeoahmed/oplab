#include "decode.hpp"
#include "decode_internal.hpp"
#include "oplab-toolchain/src/decode/ffi.rs.h"
#include "rust/cxx.h"
#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>

namespace oplab::decode {
namespace {
// Each effect is absent (0), conditional (1), or unconditional (2).
constexpr std::pair<unsigned, unsigned> effects(Access access) {
  switch (access) {
  case Access::Read:
    return {2, 0};
  case Access::ConditionalRead:
    return {1, 0};
  case Access::Write:
    return {0, 2};
  case Access::ConditionalWrite:
    return {0, 1};
  case Access::ReadWrite:
    return {2, 2};
  case Access::ReadConditionalWrite:
    return {2, 1};
  case Access::ConditionalReadWrite:
    return {1, 2};
  default:
    throw std::invalid_argument("unknown register access");
  }
}

constexpr Access merge_access(Access first, Access second) {
  if (first == second) {
    return first;
  }
  if (first == Access::Unknown || second == Access::Unknown) {
    return Access::Unknown;
  }
  const auto [first_read, first_write] = effects(first);
  const auto [second_read, second_write] = effects(second);
  // The wire has no conditional-read/conditional-write combination; leave that unknown.
  constexpr std::array combinations = {
      Access::Unknown,  Access::ConditionalWrite,     Access::Write, Access::ConditionalRead,
      Access::Unknown,  Access::ConditionalReadWrite, Access::Read,  Access::ReadConditionalWrite,
      Access::ReadWrite};
  return combinations[3 * std::max(first_read, second_read) + std::max(first_write, second_write)];
}
} // namespace

Window decode_window(rust::Slice<const std::uint8_t> bytes, std::uint64_t base,
                     Architecture architecture, std::size_t limit, bool details) {
  switch (architecture) {
  case Architecture::X86_64:
    return x86_window(bytes, {.base = base, .limit = limit, .details = details});
  case Architecture::Aarch64:
    return aarch64_window(bytes, {.base = base, .limit = limit, .details = details});
  }
  throw std::invalid_argument("unknown decoder architecture");
}

void add_register(Instruction &instruction, const std::string &name, Access access) {
  if (name.empty()) {
    return;
  }
  const auto reg =
      std::ranges::find(instruction.registers, std::string_view{name}, [](const Register &item) {
        return std::string_view{item.name.data(), item.name.size()};
      });
  if (reg != instruction.registers.end()) {
    reg->access = merge_access(reg->access, access);
  } else {
    instruction.registers.push_back(Register{.name = name, .access = access});
  }
}
} // namespace oplab::decode
