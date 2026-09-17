#pragma once
#include <memory>
#include <stdexcept>

namespace oplab {
// LLVM target registration mutates global tables; assembly and inspection share this gate.
void initialize_targets();

// LLVM MC factories transfer ownership; adopt before another operation can throw.
template <typename T> [[nodiscard]] std::unique_ptr<T> own(T *pointer) {
  if (!pointer) {
    throw std::runtime_error("LLVM MC initialization failed");
  }
  return std::unique_ptr<T>{pointer};
}
} // namespace oplab
