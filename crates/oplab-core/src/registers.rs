//! Architecture-shaped integer observations, independent of a native backend.

use crate::address::Address;

/// Raw integer register observations at one stopped execution boundary.
/// Flag values do not imply that every bit is architecturally defined by prior code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegerRegisters {
    /// x86 long-mode registers; subregister aliases refer to this canonical storage.
    X86_64 {
        /// ISA encoding order: RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8 through R15.
        gpr: [u64; 16],
        /// Next instruction address.
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
        /// Next instruction address.
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
