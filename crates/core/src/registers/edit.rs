//! Architectural write widths, independent of native debugger-register API behavior.

use super::{AARCH64_GPR_NAMES, X86_GPR_NAMES};
use crate::{diagnostic::ValidationError, target::Target};

/// Canonical storage affected by an integer edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterStorage {
    /// Index in the target's canonical GPR bank, including its stack pointer.
    Gpr(usize),
    /// RIP or PC; instruction alignment is checked before mutation.
    InstructionPointer,
    /// RFLAGS or NZCV; only individually named application flags are editable.
    Flags,
}

/// Validated replacement bits and the canonical bits that must survive a write.
#[derive(Debug, Clone, Copy)]
pub struct RegisterEdit {
    storage: RegisterStorage,
    preserve: u64,
    bits: u64,
}

impl RegisterEdit {
    /// Resolve a lowercase architectural name and validate its unsigned value.
    ///
    /// x86 byte/word writes preserve other bits; x86 dword and A64 W writes zero
    /// the upper half. Flags accept 0 or 1 and preserve every other flag bit.
    ///
    /// # Errors
    ///
    /// Rejects unsupported names, out-of-width values and misaligned A64 PC values.
    pub fn new(target: Target, name: &str, value: u64) -> Result<Self, ValidationError> {
        let (storage, width, shift) = resolve(target, name).ok_or(ValidationError::Target)?;
        let mask = u64::MAX >> (64 - width);
        if value > mask
            || (storage == RegisterStorage::InstructionPointer
                && !value.is_multiple_of(target.instruction_alignment()))
        {
            return Err(ValidationError::Target);
        }
        let preserve = if width >= 32 { 0 } else { !(mask << shift) };
        Ok(Self {
            storage,
            preserve,
            bits: value << shift,
        })
    }

    /// Native adapters map this storage to their own register identifiers.
    #[must_use]
    pub const fn storage(self) -> RegisterStorage {
        self.storage
    }

    /// Apply the validated edit without truncating the submitted value.
    #[must_use]
    pub const fn apply(self, previous: u64) -> u64 {
        (previous & self.preserve) | self.bits
    }
}

fn resolve(target: Target, name: &str) -> Option<(RegisterStorage, u32, u32)> {
    let names: &[&str] = match target {
        Target::X86_64 => &X86_GPR_NAMES,
        Target::Aarch64 => &AARCH64_GPR_NAMES,
    };
    if let Some(index) = names.iter().position(|candidate| *candidate == name) {
        return Some((RegisterStorage::Gpr(index), 64, 0));
    }
    match (target, name) {
        (Target::X86_64, "rip") | (Target::Aarch64, "pc") => {
            Some((RegisterStorage::InstructionPointer, 64, 0))
        }
        (Target::X86_64, _) => x86_alias(name),
        (Target::Aarch64, "fp") => Some((RegisterStorage::Gpr(29), 64, 0)),
        (Target::Aarch64, "lr") => Some((RegisterStorage::Gpr(30), 64, 0)),
        (Target::Aarch64, "wsp") => Some((RegisterStorage::Gpr(31), 32, 0)),
        (Target::Aarch64, _) => {
            if let Some(suffix) = name.strip_prefix('w') {
                let index = AARCH64_GPR_NAMES
                    .iter()
                    .position(|candidate| candidate.strip_prefix('x') == Some(suffix))?;
                Some((RegisterStorage::Gpr(index), 32, 0))
            } else {
                flag(name, &[("n", 31), ("z", 30), ("c", 29), ("v", 28)])
            }
        }
    }
}

fn x86_alias(name: &str) -> Option<(RegisterStorage, u32, u32)> {
    const ALIASES: [[&str; 3]; 16] = [
        ["eax", "ax", "al"],
        ["ecx", "cx", "cl"],
        ["edx", "dx", "dl"],
        ["ebx", "bx", "bl"],
        ["esp", "sp", "spl"],
        ["ebp", "bp", "bpl"],
        ["esi", "si", "sil"],
        ["edi", "di", "dil"],
        ["r8d", "r8w", "r8b"],
        ["r9d", "r9w", "r9b"],
        ["r10d", "r10w", "r10b"],
        ["r11d", "r11w", "r11b"],
        ["r12d", "r12w", "r12b"],
        ["r13d", "r13w", "r13b"],
        ["r14d", "r14w", "r14b"],
        ["r15d", "r15w", "r15b"],
    ];
    for (index, aliases) in ALIASES.iter().enumerate() {
        if let Some(column) = aliases.iter().position(|candidate| *candidate == name) {
            return Some((RegisterStorage::Gpr(index), [32, 16, 8][column], 0));
        }
    }
    if let Some(index) = ["ah", "ch", "dh", "bh"]
        .iter()
        .position(|candidate| *candidate == name)
    {
        return Some((RegisterStorage::Gpr(index), 8, 8));
    }
    flag(
        name,
        &[
            ("cf", 0),
            ("pf", 2),
            ("af", 4),
            ("zf", 6),
            ("sf", 7),
            ("df", 10),
            ("of", 11),
        ],
    )
}

fn flag(name: &str, flags: &[(&str, u32)]) -> Option<(RegisterStorage, u32, u32)> {
    flags
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, bit)| (RegisterStorage::Flags, 1, *bit))
}
