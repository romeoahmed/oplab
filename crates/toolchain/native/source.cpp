// Read linked DWARF through LLVM and retain exact locations in the original source.
#include "oplab-toolchain/native/assembly.hpp"
#include "oplab-toolchain/src/assembly/ffi.rs.h"
#include "rust/cxx.h"

#include <llvm/ADT/STLExtras.h>
#include <llvm/ADT/StringRef.h>
#include <llvm/DebugInfo/DWARF/DWARFContext.h>
#include <llvm/DebugInfo/DWARF/DWARFDebugLine.h>
#include <llvm/DebugInfo/DWARF/DWARFFormValue.h>
#include <llvm/Object/ObjectFile.h>
#include <llvm/Support/Error.h>
#include <llvm/Support/MD5.h>
#include <llvm/Support/MemoryBufferRef.h>

#include <algorithm>
#include <cstdint>
#include <set>
#include <utility>

namespace oplab {
SourcePoints source_points(rust::Slice<const std::uint8_t> image, rust::Str source) {
  const llvm::StringRef bytes(reinterpret_cast<const char *>(image.data()), image.size());
  auto object = llvm::object::ObjectFile::createObjectFile(llvm::MemoryBufferRef(bytes, "image"));
  if (!object) {
    llvm::consumeError(object.takeError());
    return {};
  }
  // Do not decompress source-provided sections during optional metadata inspection.
  if (llvm::any_of((*object)->sections(),
                   [](const auto &section) { return section.isCompressed(); })) {
    return {};
  }
  bool failed = false;
  const auto error = [&failed](llvm::Error problem) {
    failed = true;
    llvm::consumeError(std::move(problem));
  };
  const auto dwarf = llvm::DWARFContext::create(
      **object, llvm::DWARFContext::ProcessDebugRelocations::Ignore, nullptr, "", error, error);
  const llvm::StringRef text(source.data(), source.size());
  llvm::MD5 hash;
  hash.update(text);
  const auto checksum = hash.final();
  const auto lines = 1 + std::ranges::count(text, '\n');
  std::set<std::pair<std::uint64_t, std::uint32_t>> points;
  std::set<std::uint32_t> incomplete_lines;
  for (const auto &unit : dwarf->compile_units()) {
    auto table = dwarf->getLineTableForUnit(unit.get(), error);
    if (!table) {
      error(table.takeError());
      break;
    }
    if (*table == nullptr || !(*table)->Prologue.ContentTypes.HasMD5) {
      continue;
    }
    const auto &header = (*table)->Prologue;
    for (const auto &row : (*table)->Rows) {
      if (row.EndSequence || row.Line == 0 || std::cmp_greater(row.Line, lines) ||
          !header.hasFileAtIndex(row.File)) {
        continue;
      }
      const auto &file = header.getFileNameEntry(row.File);
      auto name = file.Name.getAsCString();
      if (!name) {
        error(name.takeError());
        break;
      }
      if (llvm::StringRef(*name) != "source.s" || file.Checksum != checksum) {
        continue;
      }
      const auto point = std::pair{row.Address.Address, row.Line};
      if (points.size() == 4096 && !points.contains(point)) {
        incomplete_lines.insert(row.Line);
        continue;
      }
      points.insert(point);
    }
  }
  if (failed) {
    return {}; // Debug metadata is optional; never expose a partially parsed table.
  }
  SourcePoints result{};
  result.truncated = !incomplete_lines.empty();
  for (const auto &[address, line] : points) {
    // A source breakpoint must never receive only part of a macro/repetition line.
    if (!incomplete_lines.contains(line)) {
      result.locations.push_back({.address = address, .line = line});
    }
  }
  return result;
}
} // namespace oplab
