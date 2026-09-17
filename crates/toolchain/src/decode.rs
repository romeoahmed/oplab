//! Bounded decoding from a known instruction boundary.

mod analysis;
mod ffi;
pub use analysis::analyze;

use ffi::bridge;
use oplab_core::{
    address::{Address, AddressRange},
    protocol::MAX_DECODE_BYTES,
    target::Target,
};
use thiserror::Error;

/// A decoded instruction. Text is a projection, never authoritative source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Guest address of the instruction's first byte.
    pub address: Address,
    /// Exact current instruction bytes.
    pub bytes: Vec<u8>,
    /// Backend-formatted display text; spelling is not a stable encoding contract.
    pub text: String,
}

/// A bounded decode operation failed without reinterpreting invalid bytes as code.
#[derive(Debug, Error)]
pub enum DecodeError {
    /// Input was empty, oversized, unaligned, or outside the guest address space.
    #[error("invalid decode range")]
    Range,
    /// The decoder did not recognize a complete instruction at this address.
    #[error("invalid or incomplete instruction at {0}")]
    Invalid(Address),
    /// Native decoder initialization or operation failed.
    #[error("decoder backend failed: {0}")]
    Backend(String),
}

/// Decode a finite byte window, stopping after at most `limit` instructions.
///
/// `limit` must be `1..=4096`. Bytes after the instruction limit are not decoded.
/// Invalid instructions before that limit fail the whole call; no prefix is returned.
///
/// # Errors
///
/// Rejects invalid ranges/limits, incomplete or unknown instructions, and backend failures.
pub fn decode(
    target: Target,
    bytes: &[u8],
    base: Address,
    limit: usize,
) -> Result<Vec<Instruction>, DecodeError> {
    if bytes.is_empty()
        || bytes.len() > MAX_DECODE_BYTES
        || limit == 0
        || limit > 4096
        || !base.get().is_multiple_of(target.instruction_alignment())
    {
        return Err(DecodeError::Range);
    }
    let length = u64::try_from(bytes.len()).map_err(|_| DecodeError::Range)?;
    AddressRange::new(base, length, MAX_DECODE_BYTES as u64).map_err(|_| DecodeError::Range)?;
    let window = native(target, bytes, base, limit, false)?;
    if window.consumed < bytes.len() && window.instructions.len() < limit {
        return Err(DecodeError::Invalid(
            base.checked_add(u64::try_from(window.consumed).map_err(|_| DecodeError::Range)?)
                .map_err(|_| DecodeError::Range)?,
        ));
    }
    let mut offset = 0;
    window
        .instructions
        .into_iter()
        .map(|instruction| {
            let size = usize::try_from(instruction.size).map_err(|_| DecodeError::Range)?;
            let end = offset + size;
            let encoded = bytes.get(offset..end).ok_or(DecodeError::Range)?;
            let result = Instruction {
                address: base
                    .checked_add(u64::try_from(offset).map_err(|_| DecodeError::Range)?)
                    .map_err(|_| DecodeError::Range)?,
                bytes: encoded.to_vec(),
                text: instruction.text,
            };
            offset = end;
            Ok(result)
        })
        .collect()
}

fn native(
    target: Target,
    bytes: &[u8],
    base: Address,
    limit: usize,
    details: bool,
) -> Result<bridge::Window, DecodeError> {
    let architecture = match target {
        Target::X86_64 => bridge::Architecture::X86_64,
        Target::Aarch64 => bridge::Architecture::Aarch64,
    };
    bridge::decode_window(bytes, base.get(), architecture, limit, details)
        .map_err(|error| DecodeError::Backend(error.to_string()))
}
