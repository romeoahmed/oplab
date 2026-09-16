#pragma once

#include "rust/cxx.h"
#include <cstdint>

namespace oplab {
enum class Architecture : std::uint8_t;
struct ObjectResult;
struct LinkRequest;

[[nodiscard]] ObjectResult assemble_object(rust::Str source, Architecture architecture);
[[nodiscard]] bool link_object(const LinkRequest &request);
} // namespace oplab
