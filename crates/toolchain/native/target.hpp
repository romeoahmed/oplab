#pragma once
namespace oplab {
// LLVM target registration mutates global tables; assembly and inspection share this gate.
void initialize_targets();
} // namespace oplab
