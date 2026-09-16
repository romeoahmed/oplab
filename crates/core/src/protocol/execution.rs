//! Exact session identities and coherent observations at the wire boundary.

use super::scalar::{Counter, HexAddress, VectorBits};
use crate::{
    execution::{FaultKind, GuestFault, Termination},
    registers::MachineRegisters,
    target::CpuModel,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Interpretation of the complete binary payload supplied with a load request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum LoadImage {
    /// Standard ELF owns addresses, entry and permissions.
    Elf,
    /// Exact machine code mapped RX without relocation or an implicit ABI.
    Raw {
        /// Address of the first payload byte; mapping pages round outward.
        base: HexAddress,
        /// Initial fetch address within the actual bytes, with target alignment.
        entry: HexAddress,
    },
}

/// Initial machine inputs applied only when loading, then retained for reset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct InitialState {
    /// Null selects Haswell for `x86_64` or Cortex-A72 for `AArch64`.
    pub cpu: Option<CpuModel>,
    /// Distinct canonical GPR names; omitted registers are zero. Maximum 32 entries.
    pub registers: Vec<RegisterValue>,
    /// Additional zero-filled regions, disjoint from image pages and each other.
    pub mappings: Vec<Mapping>,
}

/// One exact integer assignment; accepted names depend on the operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RegisterValue {
    /// Lowercase target name. Initial state accepts canonical GPRs only; live edits
    /// also accept subregisters, RIP/PC and individual application flag names.
    pub name: String,
    /// Exact unsigned value, limited to the selected register's width; flags use 0 or 1.
    pub value: Counter,
}

/// Additional guest memory; no stack or other ABI role is inferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    /// Backend-page-aligned starting address.
    pub address: HexAddress,
    /// Nonzero byte length, a multiple of backend page size. Total mapped memory is at most 64 MiB.
    pub length: u32,
    /// Standard ELF permission bits: `PF_R=4`, `PF_W=2`, `PF_X=1`; zero is a guard region.
    pub flags: u8,
}

/// A loaded session and reset generation, scoped to one worker connection.
///
/// Load request IDs are not reused within a connection; reset preserves the session ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionKey {
    /// ID of the successful load request on this connection.
    pub session: Counter,
    /// Current reset generation.
    pub generation: Counter,
}

/// A contiguous guest range for a memory observation or patch.
///
/// Observations capture this range together with registers; patches supply its bytes separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MemoryWindow {
    /// First guest byte.
    pub address: HexAddress,
    /// Nonzero byte count, limited to 64 KiB within one mapped region.
    pub length: u32,
}

/// Session operations applied between native execution slices.
///
/// Run and step acknowledge acceptance; the returned state may still be running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SessionAction {
    /// Begin continuous execution.
    Run,
    /// Begin one architectural step, including REP continuation.
    Step,
    /// Pause at the next ownership boundary.
    Pause,
    /// Terminate execution as cancelled.
    Cancel,
    /// Restore initial memory and registers, clear counters and advance the reset generation.
    Reset,
    /// Write an integer register, alias, RIP/PC or application flag while ready/paused.
    ///
    /// Reset restores initial state. PC writes rearm breakpoints and end REP continuation.
    WriteRegister(RegisterValue),
    /// Write 1–65,536 bytes from following binary frames while ready or paused.
    ///
    /// Guest permissions remain unchanged; executable translations are invalidated.
    WriteMemory(MemoryWindow),
    /// Set or remove a session address breakpoint; source provenance is not checked.
    Breakpoint {
        /// Guest address to test before instruction effects; target alignment is required.
        address: HexAddress,
        /// Whether the breakpoint should be retained.
        enabled: bool,
    },
    /// Capture control state and available registers, optionally with memory.
    Observe {
        /// Null requests registers and control state only.
        memory: Option<MemoryWindow>,
    },
    /// Release the machine, including a running session.
    Close,
}

/// Explicit wire state with address-bearing pauses encoded exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Status {
    /// Initial image is ready.
    Ready,
    /// Native slices may continue after this observation.
    Running,
    /// A step completed.
    Stepped,
    /// A requested pause completed.
    Paused,
    /// An address breakpoint stopped before effects.
    Breakpoint(HexAddress),
    /// Execution ended with an explicit outcome.
    Terminated(Termination),
    /// The native state is unusable; register observations are unavailable.
    Crashed,
}

/// Coherent integer and 128-bit SIMD banks.
///
/// 64-bit register values use decimal strings, addresses and vectors use fixed-width
/// hex, and 32-bit controls use JSON numbers.
/// Raw bits do not imply flag definedness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Registers {
    /// `x86_64` integer and SIMD banks.
    X86_64 {
        /// ISA encoding order: RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8–R15.
        gpr: [Counter; 16],
        /// Observed instruction pointer; faults may leave it at the faulting instruction.
        rip: HexAddress,
        /// Raw flags and reserved bits from the processor model.
        rflags: Counter,
        /// XMM0–XMM15; excludes AVX upper halves and x87 state.
        xmm: Box<[VectorBits; 16]>,
        /// Raw backend MXCSR; accrued floating-point exception flags may be incomplete.
        mxcsr: u32,
    },
    /// A64 integer and SIMD banks, with SP stored independently.
    Aarch64 {
        /// X0–X30 in register-number order.
        x: [Counter; 31],
        /// Stack pointer value.
        sp: Counter,
        /// Observed instruction pointer; faults may leave it at the faulting instruction.
        pc: HexAddress,
        /// Raw NZCV representation, with flags in bits 31 through 28.
        nzcv: u32,
        /// V0–V31, including aliased scalar floating-point bits.
        v: Box<[VectorBits; 32]>,
        /// Raw floating-point control.
        fpcr: u32,
        /// Raw floating-point exception status.
        fpsr: u32,
    },
}

/// Native guest fault with exact optional memory metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    /// Architectural access or exception category.
    pub kind: FaultKind,
    /// Observed processor PC after the fault.
    pub pc: HexAddress,
    /// Access address, if reported.
    pub address: Option<HexAddress>,
    /// Access width in bytes, if reported.
    pub size: Option<Counter>,
}

/// A coherent machine observation captured between native slices.
///
/// Registers, faults, counters and optional binary memory share one sequence and generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    /// Resolved emulator profile, retained across reset.
    pub cpu: CpuModel,
    /// Session and reset generation that produced these values.
    pub key: SessionKey,
    /// Strictly increasing within the session, including across reset.
    pub sequence: Counter,
    /// Control state at capture time.
    pub status: Status,
    /// Observed architectural instruction starts, not retired instructions.
    pub instructions: Counter,
    /// Native dispatch work, including REP iterations.
    pub dispatches: Counter,
    /// Null only when native machine state is unusable (`crashed`).
    pub registers: Option<Registers>,
    /// Most recent terminal guest fault, if any.
    pub fault: Option<Fault>,
    /// Sorted, distinct address breakpoints retained across reset; at most 256.
    pub breakpoints: Vec<HexAddress>,
    /// Metadata for the binary memory payload following this response or full stream event.
    pub memory: Option<MemoryWindow>,
}

impl From<MachineRegisters> for Registers {
    fn from(registers: MachineRegisters) -> Self {
        match registers {
            MachineRegisters::X86_64 {
                gpr,
                rip,
                rflags,
                xmm,
                mxcsr,
            } => Self::X86_64 {
                gpr: gpr.map(Counter::new),
                rip: HexAddress::new(rip),
                rflags: Counter::new(rflags),
                xmm: Box::new(xmm.map(VectorBits::new)),
                mxcsr,
            },
            MachineRegisters::Aarch64 {
                x,
                sp,
                pc,
                nzcv,
                v,
                fpcr,
                fpsr,
            } => Self::Aarch64 {
                x: x.map(Counter::new),
                sp: Counter::new(sp),
                pc: HexAddress::new(pc),
                nzcv,
                v: Box::new(v.map(VectorBits::new)),
                fpcr,
                fpsr,
            },
        }
    }
}

impl From<GuestFault> for Fault {
    fn from(fault: GuestFault) -> Self {
        Self {
            kind: fault.kind,
            pc: HexAddress::new(fault.pc),
            address: fault.address.map(HexAddress::new),
            size: fault.size.map(Counter::new),
        }
    }
}
