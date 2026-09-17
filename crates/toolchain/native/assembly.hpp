#pragma once

#include "rust/cxx.h"
#include <cstdint>

namespace oplab {
enum class Architecture : std::uint8_t;
struct ObjectResult;
struct LinkRequest;
struct SourcePoints;

[[nodiscard]] ObjectResult assemble_object(rust::Str source, Architecture architecture);
[[nodiscard]] SourcePoints source_points(rust::Slice<const std::uint8_t> image, rust::Str source);
[[nodiscard]] bool link_object(const LinkRequest &request);
} // namespace oplab
