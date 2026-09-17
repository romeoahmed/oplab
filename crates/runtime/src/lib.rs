//! Thread-owned QEMU CPU state and LLVM execution.
//!
//! [`Runtime`] provides raw storage and instruction dispatch. Callers own register
//! edit policy, completion and instruction accounting; guest faults are [`Outcome`] values.
//! SDK and LLVM errors can contain native diagnostics and must be mapped before presentation.
#[cfg(not(all(target_pointer_width = "64", target_endian = "little")))]
compile_error!("the QEMU/LLVM runtime requires a 64-bit little-endian host");

mod jit;
mod qemu;

/// Runtime infrastructure failure; native diagnostics are unsuitable for ordinary UI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The selected SDK could not be loaded.
    #[error("QEMU SDK could not be loaded: {0}")]
    Sdk(String),
    /// A native operation violated its boundary contract.
    #[error("native runtime rejected the operation")]
    Native,
    /// The selected instruction requires an unsupported TCG operation.
    #[error("unsupported TCG operation: {0}")]
    Operation(String),
    /// LLVM rejected intermediate code or could not generate host code.
    #[error("LLVM compilation failed: {0}")]
    Llvm(String),
}

use oplab_core::{
    address::Address,
    execution::{Access, FaultKind, GuestFault},
    target::Target,
};
use qemu::{Cpu, abi};
use std::collections::BTreeMap;

/// Canonical little-endian storage; integer aliases and write policy belong to the caller.
#[derive(Clone, Copy)]
pub enum Register {
    /// x86 ISA encoding order (RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8–R15);
    /// `AArch64` indices 0–30 are X0–X30 and index 31 is SP.
    Gpr(u32),
    /// Architectural instruction pointer.
    Pc,
    /// RFLAGS or NZCV.
    Flags,
    /// YMM or Z storage; shorter reads/writes address its low bits.
    Vector(u32),
    /// P0–P15 or FFR (index 16).
    Predicate(u32),
    /// Read-only current/maximum vector lengths: two little-endian `u32` byte counts.
    VectorLength,
    /// MXCSR or FPCR.
    Control,
    /// Read-only MXCSR exception flags or FPSR.
    Status,
}
impl Register {
    const fn location(self) -> (abi::OplabRegister, u32) {
        match self {
            Self::Gpr(index) => (abi::OPLAB_GPR, index),
            Self::Pc => (abi::OPLAB_PC, 0),
            Self::Flags => (abi::OPLAB_FLAGS, 0),
            Self::Vector(index) => (abi::OPLAB_VECTOR, index),
            Self::Predicate(index) => (abi::OPLAB_PREDICATE, index),
            Self::VectorLength => (abi::OPLAB_VECTOR_LENGTH, 0),
            Self::Control => (abi::OPLAB_CONTROL, 0),
            Self::Status => (abi::OPLAB_STATUS, 0),
        }
    }
}
/// Instruction outcome; experiment completion is a separate caller-owned condition.
pub enum Outcome {
    /// The instruction completed.
    Finished,
    /// A repeated instruction reached its dispatch budget.
    Yield,
    /// The instruction requires an unimplemented operating environment.
    Environment,
    /// An architectural or guest-memory fault occurred; partial effects remain.
    Fault(GuestFault),
}
/// Result of a bounded instruction dispatch.
pub struct Step {
    /// Native dispatches, including repeated-string iterations.
    pub dispatches: u64,
    /// Whether an architectural instruction was admitted.
    pub started: bool,
    /// Whether a repeated instruction remains in progress.
    pub repeated: bool,
    /// Guest-visible completion, suspension or failure.
    pub outcome: Outcome,
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    pc: u64,
    cs_base: u64,
    flags: u32,
    size: u32,
    bytes: [u8; 16],
}
impl From<&abi::OplabBlock> for Key {
    fn from(block: &abi::OplabBlock) -> Self {
        Self {
            pc: block.pc,
            cs_base: block.cs_base,
            flags: block.flags,
            size: block.size,
            bytes: block.code,
        }
    }
}
/// Thread-owned CPU and bounded ORC code cache. QEMU owns all architectural state.
pub struct Runtime {
    cpu: Cpu,
    compiler: jit::Compiler,
    cache: BTreeMap<Key, jit::Compiled>,
}
impl Runtime {
    /// Create the architecture's MAX guest in x86 CPL3 or `AArch64` EL0.
    ///
    /// No memory, stack or OS services are supplied. SIMD starts at nearest-even rounding.
    ///
    /// # Errors
    ///
    /// Fails when the matching SDK or host JIT cannot initialize.
    pub fn new(target: Target) -> Result<Self, Error> {
        Ok(Self {
            cpu: Cpu::new(target)?,
            compiler: jit::Compiler::new()?,
            cache: BTreeMap::new(),
        })
    }
    /// Map 4-KiB-aligned memory, copy `bytes` at its start and zero the remainder.
    ///
    /// Permissions use R=1, W=2, X=4 (not ELF flags); zero creates a guard region.
    /// The CPU accepts at most 64 mappings totaling 64 MiB.
    ///
    /// # Errors
    ///
    /// Rejects invalid alignment, overlap, overflow, permissions, excess initial bytes
    /// or mapping budgets; native allocation can also fail.
    pub fn map(
        &mut self,
        base: u64,
        size: u64,
        permissions: u8,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.cpu.map(base, size, u32::from(permissions), bytes)
    }
    /// Read a mapped range, independently of guest access permissions.
    ///
    /// # Errors
    ///
    /// Rejects empty, unmapped, cross-mapping or larger-than-64-KiB ranges.
    pub fn read(&self, address: u64, size: usize) -> Result<Vec<u8>, Error> {
        self.cpu.read(address, size)
    }
    /// Patch mapped storage and release cached executable code.
    ///
    /// # Errors
    ///
    /// Rejects empty, unmapped, cross-mapping or larger-than-64-KiB ranges.
    pub fn write(&mut self, address: u64, bytes: &[u8]) -> Result<(), Error> {
        self.cpu.write(address, bytes)?;
        self.cache.clear();
        Ok(())
    }
    /// Read canonical register storage or a supported low-vector view.
    ///
    /// # Errors
    ///
    /// Rejects an unavailable register or unsupported view width.
    pub fn register<const N: usize>(&self, register: Register) -> Result<[u8; N], Error> {
        let (bank, index) = register.location();
        let mut bytes = [0; N];
        self.cpu.register(bank, index, &mut bytes)?;
        Ok(bytes)
    }
    /// Read bounded variable-width architectural storage.
    ///
    /// # Errors
    ///
    /// Rejects sizes outside 1–256 bytes and widths unavailable in the native bank.
    pub fn register_bytes(&self, register: Register, size: usize) -> Result<Vec<u8>, Error> {
        if !(1..=256).contains(&size) {
            return Err(Error::Native);
        }
        let (bank, index) = register.location();
        let mut bytes = vec![0; size];
        self.cpu.register(bank, index, &mut bytes)?;
        Ok(bytes)
    }
    /// Effective and maximum vector length in bytes, reported by QEMU.
    ///
    /// # Errors
    ///
    /// Rejects unavailable or inconsistent architectural lengths.
    pub fn vector_lengths(&self) -> Result<(u16, u16), Error> {
        let bytes: [u8; 8] = self.register(Register::VectorLength)?;
        let current = u32::from_le_bytes(bytes[..4].try_into().map_err(|_| Error::Native)?);
        let maximum = u32::from_le_bytes(bytes[4..].try_into().map_err(|_| Error::Native)?);
        if !(16..=256).contains(&current)
            || !(current..=256).contains(&maximum)
            || !current.is_multiple_of(16)
            || !maximum.is_multiple_of(16)
        {
            return Err(Error::Native);
        }
        Ok((
            u16::try_from(current).map_err(|_| Error::Native)?,
            u16::try_from(maximum).map_err(|_| Error::Native)?,
        ))
    }
    /// Replace canonical register storage or a supported low-vector view.
    ///
    /// Low-vector writes preserve upper storage. This raw API does not apply integer
    /// alias rules, validate PC alignment or manage session breakpoint/REP accounting.
    ///
    /// # Errors
    ///
    /// Rejects unavailable registers and incorrect widths.
    pub fn set_register(&mut self, register: Register, bytes: &[u8]) -> Result<(), Error> {
        let (bank, index) = register.location();
        if !(1..=256).contains(&bytes.len()) {
            return Err(Error::Native);
        }
        self.cpu.set_register(bank, index, bytes)
    }
    /// Execute at the native PC, with at most `budget` REP dispatches (1–1,024).
    ///
    /// `pc` labels faults; it must match the native PC and does not set it. Guest
    /// faults are returned in [`Step::outcome`]. Translation and compilation are
    /// synchronous, so the dispatch budget is not a wall-clock deadline.
    ///
    /// # Errors
    ///
    /// Rejects invalid budgets and native infrastructure failures.
    pub fn execute(&mut self, pc: Address, budget: u64) -> Result<Step, Error> {
        if !(1..=1024).contains(&budget) {
            return Err(Error::Native);
        }
        let mut step = Step {
            dispatches: 0,
            started: false,
            repeated: false,
            outcome: Outcome::Finished,
        };
        loop {
            let (block, mut exit) = self.cpu.translate()?;
            if exit.stop == abi::OPLAB_FINISHED {
                step.started = true;
                let key = Key::from(&block.raw);
                if self.cache.len() == 128 && !self.cache.contains_key(&key) {
                    self.cache.clear();
                }
                let compiled = match self.cache.entry(key) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        match self.compiler.compile(&block, self.cpu.memory_helpers()) {
                            Ok(compiled) => entry.insert(compiled),
                            Err(Error::Operation(_)) => {
                                step.outcome = Outcome::Fault(GuestFault {
                                    kind: FaultKind::UnsupportedInstruction,
                                    pc,
                                    address: None,
                                    size: None,
                                });
                                return Ok(step);
                            }
                            Err(error) => return Err(error),
                        }
                    }
                };
                exit = self.cpu.execute(&block, compiled.entry)?;
                step.dispatches += 1;
            }
            step.repeated = exit.repeated;
            step.outcome = outcome(exit, pc)?;
            if !exit.repeated || !matches!(step.outcome, Outcome::Finished) {
                return Ok(step);
            }
            if step.dispatches == budget {
                step.outcome = Outcome::Yield;
                return Ok(step);
            }
        }
    }
}
fn outcome(exit: abi::OplabExit, pc: Address) -> Result<Outcome, Error> {
    let access = match exit.access {
        2 => Access::Write,
        4 => Access::Fetch,
        _ => Access::Read,
    };
    let kind = match exit.stop {
        abi::OPLAB_FINISHED => return Ok(Outcome::Finished),
        abi::OPLAB_ENVIRONMENT => return Ok(Outcome::Environment),
        abi::OPLAB_INVALID => FaultKind::InvalidInstruction,
        abi::OPLAB_EXCEPTION => FaultKind::Exception(u32::try_from(exit.exception).ok()),
        abi::OPLAB_UNMAPPED => FaultKind::Unmapped(access),
        abi::OPLAB_PROTECTION => FaultKind::Protection(access),
        abi::OPLAB_UNALIGNED => FaultKind::Unaligned(access),
        abi::OPLAB_INTERNAL => return Err(Error::Native),
        _ => return Err(Error::Sdk("unknown native exit".into())),
    };
    Ok(Outcome::Fault(GuestFault {
        kind,
        pc,
        address: (exit.size != 0).then_some(Address::new(exit.address)),
        size: (exit.size != 0).then_some(exit.size),
    }))
}
