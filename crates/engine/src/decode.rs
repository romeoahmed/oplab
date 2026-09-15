//! Bounded decoding from a known instruction boundary.

mod analysis;
pub use analysis::analyze;

use capstone::prelude::*;
use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
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
    match target {
        Target::X86_64 => decode_x86(bytes, base, limit),
        Target::Aarch64 => decode_arm(bytes, base, limit),
    }
}

fn decode_x86(bytes: &[u8], base: Address, limit: usize) -> Result<Vec<Instruction>, DecodeError> {
    let mut decoder = Decoder::with_ip(64, bytes, base.get(), DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    let mut result = Vec::new();
    while decoder.can_decode() && result.len() < limit {
        let start = decoder.position();
        let instruction = decoder.decode();
        let address = Address::new(instruction.ip());
        if instruction.is_invalid() {
            return Err(DecodeError::Invalid(address));
        }
        let mut text = String::new();
        formatter.format(&instruction, &mut text);
        let encoded = bytes
            .get(start..decoder.position())
            .ok_or(DecodeError::Range)?;
        result.push(Instruction {
            address,
            bytes: encoded.to_vec(),
            text,
        });
    }
    Ok(result)
}

fn decode_arm(bytes: &[u8], base: Address, limit: usize) -> Result<Vec<Instruction>, DecodeError> {
    let decoder = Capstone::new()
        .arm64()
        .mode(arch::arm64::ArchMode::Arm)
        .build()
        .map_err(|error| DecodeError::Backend(error.to_string()))?;
    let instructions = decoder
        .disasm_count(bytes, base.get(), limit)
        .map_err(|error| DecodeError::Backend(error.to_string()))?;
    let mut result = Vec::new();
    let mut consumed = 0;
    for instruction in instructions.as_ref() {
        let mnemonic = instruction
            .mnemonic()
            .ok_or_else(|| DecodeError::Invalid(Address::new(instruction.address())))?;
        let operands = instruction.op_str().unwrap_or_default();
        result.push(Instruction {
            address: Address::new(instruction.address()),
            bytes: instruction.bytes().to_vec(),
            text: if operands.is_empty() {
                mnemonic.to_owned()
            } else {
                format!("{mnemonic} {operands}")
            },
        });
        consumed += instruction.bytes().len();
    }
    if consumed < bytes.len() && result.len() < limit {
        let offset = u64::try_from(consumed).map_err(|_| DecodeError::Range)?;
        return Err(DecodeError::Invalid(
            base.checked_add(offset).map_err(|_| DecodeError::Range)?,
        ));
    }
    Ok(result)
}
