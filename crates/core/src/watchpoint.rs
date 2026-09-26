//! Bounded data-access observation, independent of instruction fetch and debugger I/O.

use crate::{
    address::{Address, AddressRange},
    diagnostic::ValidationError,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Maximum input count before de-duplication and retained range count per machine.
pub const MAX_WATCHPOINTS: usize = 32;

/// Guest data accesses to observe; writes include stores of the existing value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WatchAccess {
    /// Loads only.
    Read,
    /// Stores only.
    Write,
    /// Loads and stores, including read-modify-write operations.
    ReadWrite,
}

/// A validated non-wrapping data range, independent of current mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Watchpoint {
    address: Address,
    length: u32,
    access: WatchAccess,
}
impl Watchpoint {
    /// Watch 1–65,536 bytes. Ranges may span pages or mappings.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized or wrapping ranges.
    pub fn new(
        address: Address,
        length: u32,
        access: WatchAccess,
    ) -> Result<Self, ValidationError> {
        AddressRange::new(address, u64::from(length), 65536)?;
        Ok(Self {
            address,
            length,
            access,
        })
    }
    /// First watched byte.
    #[must_use]
    pub const fn address(self) -> Address {
        self.address
    }
    /// Range size in bytes.
    #[must_use]
    pub const fn length(self) -> u32 {
        self.length
    }
    /// Selected data-access types.
    #[must_use]
    pub const fn access(self) -> WatchAccess {
        self.access
    }
}

/// First QEMU-reported matching access in a completed native dispatch.
///
/// Pause occurs after instruction effects, or after one REP iteration. Faults take
/// precedence; this is an access observation, not a changed-value assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchpointHit {
    /// Retained range responsible for the pause.
    pub watchpoint: Watchpoint,
    /// Instruction that performed the access, distinct from the resulting PC.
    pub pc: Address,
    /// First overlapping byte reported by QEMU, not necessarily the access start.
    pub address: Address,
    /// Matching access bits reported by QEMU, restricted to the watched modes.
    pub access: WatchAccess,
}
