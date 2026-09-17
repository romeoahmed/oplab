//! Standard assembly, ELF linking and instruction analysis for both guest architectures.

// Share LLVM discovery and native linkage with the execution runtime.
use llvm_sys as _;

pub mod assembly;
pub mod decode;
