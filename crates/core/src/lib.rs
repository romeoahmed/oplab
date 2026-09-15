//! Guest addresses, memory policies, execution state and worker wire contracts.
//!
//! Domain constructors validate values before native allocation. The [`protocol`]
//! module owns serialization and framing; operation handlers validate command semantics.
//! This crate has no emulator, desktop or native toolchain dependency.
#![forbid(unsafe_code)]

pub mod address;
pub mod diagnostic;
pub mod execution;
pub mod memory;
pub mod policy;
pub mod protocol;
pub mod registers;
pub mod target;
