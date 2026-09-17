//! Exact vector and predicate edits, independent of instruction operand semantics.

use super::VectorBits;
use crate::{diagnostic::ValidationError, target::Target};

/// Canonical native storage shared by register aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorStorage {
    /// YMM or Z register index.
    Vector(u8),
    /// P0–P15, with FFR at index 16.
    Predicate(u8),
}

/// Validated replacement of one lane or complete architectural register.
#[derive(Debug, Clone)]
pub struct VectorEdit {
    storage: VectorStorage,
    offset: usize,
    width: usize,
    value: VectorBits,
}

impl VectorEdit {
    /// Validate an edit against the current architectural vector length in bytes.
    ///
    /// XMM/V alias the low 128 bits of YMM/Z. Vector lanes use 8, 16, 32, 64 or
    /// 128 bits; predicate lanes use one bit. A full-register edit uses its width
    /// and lane zero. Values must contain exactly ceil(width / 8) bytes.
    ///
    /// # Errors
    ///
    /// Rejects invalid lengths, names, widths, lanes and nonzero padding bits.
    pub fn new(
        target: Target,
        vector_bytes: usize,
        name: &str,
        width: u16,
        lane: u16,
        value: VectorBits,
    ) -> Result<Self, ValidationError> {
        if !valid_length(target, vector_bytes) {
            return Err(ValidationError::Target);
        }
        let (storage, bytes) = match target {
            Target::X86_64 => {
                if let Some(index) = index(name, "xmm", 16) {
                    (VectorStorage::Vector(index), 16)
                } else if let Some(index) = index(name, "ymm", 16) {
                    (VectorStorage::Vector(index), 32)
                } else {
                    return Err(ValidationError::Target);
                }
            }
            Target::Aarch64 => {
                if let Some(index) = index(name, "v", 32) {
                    (VectorStorage::Vector(index), 16)
                } else if let Some(index) = index(name, "z", 32) {
                    (VectorStorage::Vector(index), vector_bytes)
                } else if let Some(index) = index(name, "p", 16) {
                    (VectorStorage::Predicate(index), vector_bytes / 8)
                } else if name == "ffr" {
                    (VectorStorage::Predicate(16), vector_bytes / 8)
                } else {
                    return Err(ValidationError::Target);
                }
            }
        };
        let width = usize::from(width);
        let bits = bytes * 8;
        let lane_width = match storage {
            VectorStorage::Vector(_) => matches!(width, 8 | 16 | 32 | 64 | 128),
            VectorStorage::Predicate(_) => width == 1,
        };
        if width == 0
            || (!lane_width && width != bits)
            || usize::from(lane) >= bits / width
            || value.as_le_bytes().len() != width.div_ceil(8)
            || (width == 1 && value.as_le_bytes()[0] > 1)
        {
            return Err(ValidationError::Target);
        }
        Ok(Self {
            storage,
            offset: usize::from(lane) * width,
            width,
            value,
        })
    }

    /// Canonical storage; aliases never maintain separate state.
    #[must_use]
    pub const fn storage(&self) -> VectorStorage {
        self.storage
    }

    /// Merge on the native owner thread, retaining all bits outside the selected range.
    ///
    /// # Errors
    ///
    /// Rejects insufficient storage before mutation.
    pub fn apply(&self, previous: &mut [u8]) -> Result<(), ValidationError> {
        let start = self.offset / 8;
        let end = (self.offset + self.width).div_ceil(8);
        let destination = previous
            .get_mut(start..end)
            .ok_or(ValidationError::Target)?;
        if self.width == 1 {
            let bit = self.offset % 8;
            destination[0] = (destination[0] & !(1 << bit)) | (self.value.as_le_bytes()[0] << bit);
        } else {
            destination.copy_from_slice(self.value.as_le_bytes());
        }
        Ok(())
    }
}

fn valid_length(target: Target, bytes: usize) -> bool {
    match target {
        Target::X86_64 => bytes == 32,
        Target::Aarch64 => (16..=256).contains(&bytes) && bytes.is_multiple_of(16),
    }
}
fn index(name: &str, prefix: &str, count: u8) -> Option<u8> {
    let suffix = name.strip_prefix(prefix)?;
    let index = suffix.parse::<u8>().ok()?;
    (index < count && suffix == index.to_string()).then_some(index)
}
