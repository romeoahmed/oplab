//! Stable domain failures; presentation and localization belong at the boundary.

use thiserror::Error;

/// A rejected domain value or operation. Error text never includes source or memory.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    /// An address does not match the human-input or wire encoding required by its caller.
    #[error("invalid 0x-prefixed 64-bit hexadecimal address")]
    AddressFormat,
    /// A checked arithmetic operation would leave the address space.
    #[error("address range exceeds the 64-bit address space")]
    AddressOverflow,
    /// A bounded object is empty or exceeds its allowed size.
    #[error("resource length is outside its allowed range")]
    Length,
    /// A region does not meet the backend's page alignment requirement.
    #[error("memory region is not page aligned")]
    Alignment,
    /// Two mappings occupy the same guest byte.
    #[error("memory regions overlap")]
    Overlap,
    /// A register has more than one initial assignment.
    #[error("register is initialized more than once")]
    DuplicateRegister,
    /// A requested range is not contained by an appropriate mapping.
    #[error("memory range is unmapped or lacks required permissions")]
    Permission,
    /// A command is illegal for the current execution state.
    #[error("command is not allowed in the current execution state")]
    Transition,
    /// An identifier cannot advance without wrapping.
    #[error("monotonic counter exhausted")]
    CounterExhausted,
    /// Target-specific constraints are not satisfied.
    #[error("input does not satisfy the target profile")]
    Target,
}
