//! Exact session identities and coherent observations at the wire boundary.

use super::scalar::{Counter, HexAddress};
use crate::{
    execution::{FaultKind, GuestFault, Termination},
    registers::IntegerRegisters,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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

/// A bounded memory observation, captured together with the returned registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MemoryWindow {
    /// First guest byte.
    pub address: HexAddress,
    /// Byte count, limited to 64 KiB and one mapped region.
    pub length: u32,
}

/// Controls apply between native slices. Run and step acknowledge acceptance;
/// their observation may still be Running and is not a promise of completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
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
    /// Restore the bound initial image and advance its generation.
    Reset,
    /// Set or remove a session address breakpoint; source provenance is not checked.
    Breakpoint {
        /// Guest address to test before instruction effects; target alignment is required.
        address: HexAddress,
        /// Whether the breakpoint should be retained.
        enabled: bool,
    },
    /// Capture a full integer observation, optionally with one memory window.
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
    /// The native state is unusable; integer observations are unavailable.
    Crashed,
}

/// Complete canonical integer banks. Precision-sensitive values use decimal
/// strings; these are raw bit values, without inferred flag definedness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Registers {
    /// x86 ISA encoding order: RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8–R15.
    X86_64 {
        /// Canonical general-purpose values.
        gpr: [Counter; 16],
        /// Observed instruction pointer; faults may leave it at the faulting instruction.
        rip: HexAddress,
        /// Raw flags and reserved bits from the processor model.
        rflags: Counter,
    },
    /// A64 X0–X30, with SP stored independently.
    Aarch64 {
        /// Canonical general-purpose values.
        x: [Counter; 31],
        /// Stack pointer value.
        sp: Counter,
        /// Observed instruction pointer; faults may leave it at the faulting instruction.
        pc: HexAddress,
        /// Raw NZCV representation, with flags in bits 31 through 28.
        nzcv: u32,
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
    /// Access width, if reported.
    pub size: Option<Counter>,
}

/// A coherent machine observation captured between native slices.
///
/// Registers, faults, counters and optional binary memory share one sequence and generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Observation {
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
    /// Null only when the native machine has been lost.
    pub registers: Option<Registers>,
    /// Most recent terminal guest fault, if any.
    pub fault: Option<Fault>,
    /// Metadata for the binary memory payload immediately following the reply.
    pub memory: Option<MemoryWindow>,
}

impl From<IntegerRegisters> for Registers {
    fn from(registers: IntegerRegisters) -> Self {
        match registers {
            IntegerRegisters::X86_64 { gpr, rip, rflags } => Self::X86_64 {
                gpr: gpr.map(Counter::new),
                rip: HexAddress::new(rip),
                rflags: Counter::new(rflags),
            },
            IntegerRegisters::Aarch64 { x, sp, pc, nzcv } => Self::Aarch64 {
                x: x.map(Counter::new),
                sp: Counter::new(sp),
                pc: HexAddress::new(pc),
                nzcv,
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
