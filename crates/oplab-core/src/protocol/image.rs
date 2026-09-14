//! Bounded views of standard ELF metadata; the complete image remains authoritative.

use super::scalar::{Counter, HexAddress};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Entry, load segments, and defined symbols projected from a linked ELF image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImageInfo {
    /// ELF entry address; completion remains an explicit experiment setting.
    pub entry: HexAddress,
    /// Standard `PT_LOAD` program headers, in file order.
    pub segments: Vec<ImageSegment>,
    /// Bounded named, defined, non-file/non-section symbols.
    pub symbols: Vec<ImageSymbol>,
    /// More symbols exist than this bounded view includes.
    pub symbols_truncated: bool,
}

/// A standard load segment; file offsets refer to the complete executable payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImageSegment {
    /// Guest virtual start.
    pub address: HexAddress,
    /// ELF file offset, bounded by the one-MiB image limit.
    pub file_offset: u32,
    /// File-backed byte count.
    pub file_bytes: u32,
    /// Mapped byte count, including zero-filled storage.
    pub memory_bytes: Counter,
    /// ELF `PF_R/PF_W/PF_X` bits; no invented permission widening.
    pub flags: u32,
}

/// One named symbol at its linked address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImageSymbol {
    /// Original ELF symbol name.
    pub name: String,
    /// Linked symbol value.
    pub address: HexAddress,
    /// ELF symbol size, which may be zero for assembly labels.
    pub size: Counter,
}
