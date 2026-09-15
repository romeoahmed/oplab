#pragma once

#include "rust/cxx.h"
#include <cstdint>

namespace oplab {
struct ObjectResult;
[[nodiscard]] ObjectResult assemble_object(rust::Str source, bool aarch64);
[[nodiscard]] bool link_object(rust::Str input, rust::Str output, rust::Str script,
                               std::uint64_t base);
} // namespace oplab
