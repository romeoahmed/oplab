//! Architecture-shaped integer initialization and observations, independent of a native backend.

use crate::{address::Address, diagnostic::ValidationError, target::Target};

/// Canonical x86 general-purpose names in ISA encoding order.
pub const X86_GPR_NAMES: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15",
];

/// Canonical A64 general-purpose names, followed by the separate stack pointer.
pub const AARCH64_GPR_NAMES: [&str; 32] = [
    "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13", "x14",
    "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27",
    "x28", "x29", "x30", "sp",
];

/// Initial general-purpose values. PC comes from the image; flags retain backend defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitialRegisters {
    /// RAX through R15 in [`X86_GPR_NAMES`] order, including RSP.
    X86_64([u64; 16]),
    /// X0 through X30, followed by SP, in [`AARCH64_GPR_NAMES`] order.
    Aarch64([u64; 32]),
}

impl InitialRegisters {
    /// Build an initial bank from canonical lowercase names; unspecified values are zero.
    ///
    /// # Errors
    ///
    /// Rejects unknown, alias, wrong-target or duplicate names. PC and flags are not GPRs.
    pub fn from_assignments<'a>(
        target: Target,
        assignments: impl IntoIterator<Item = (&'a str, u64)>,
    ) -> Result<Self, ValidationError> {
        let mut bank = match target {
            Target::X86_64 => Self::X86_64([0; 16]),
            Target::Aarch64 => Self::Aarch64([0; 32]),
        };
        let (names, values): (&[&str], &mut [u64]) = match &mut bank {
            Self::X86_64(values) => (&X86_GPR_NAMES, values),
            Self::Aarch64(values) => (&AARCH64_GPR_NAMES, values),
        };
        let mut assigned = 0_u32;
        for (name, value) in assignments {
            let index = names
                .iter()
                .position(|candidate| *candidate == name)
                .ok_or(ValidationError::Target)?;
            let bit = 1 << index;
            if assigned & bit != 0 {
                return Err(ValidationError::DuplicateRegister);
            }
            assigned |= bit;
            values[index] = value;
        }
        Ok(bank)
    }

    /// Architecture whose register storage is initialized.
    #[must_use]
    pub const fn target(&self) -> Target {
        match self {
            Self::X86_64(_) => Target::X86_64,
            Self::Aarch64(_) => Target::Aarch64,
        }
    }
}

/// Raw integer registers captured between native execution slices.
///
/// Flag values do not imply that every bit is architecturally defined by prior code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegerRegisters {
    /// x86 long-mode registers; subregister aliases refer to this canonical storage.
    X86_64 {
        /// ISA encoding order: RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8 through R15.
        gpr: [u64; 16],
        /// Observed instruction pointer; a fault may leave it at the faulting instruction.
        rip: Address,
        /// Raw RFLAGS, including reserved bits supplied by the processor model.
        rflags: u64,
    },
    /// A64 integer registers; SP and the zero register are distinct architectural roles.
    Aarch64 {
        /// X0 through X30. W-register writes zero-extend into their X register.
        x: [u64; 31],
        /// Stack pointer; this is not X31.
        sp: u64,
        /// Observed instruction pointer; a fault may leave it at the faulting instruction.
        pc: Address,
        /// Raw NZCV representation, with flags in bits 31 through 28.
        nzcv: u32,
    },
}

impl IntegerRegisters {
    /// Architecture-independent instruction pointer without reinterpreting GPR indices.
    #[must_use]
    pub const fn instruction_pointer(&self) -> Address {
        match self {
            Self::X86_64 { rip, .. } => *rip,
            Self::Aarch64 { pc, .. } => *pc,
        }
    }
}
