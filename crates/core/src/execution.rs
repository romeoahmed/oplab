//! Pure command legality and explicit execution outcomes.

use crate::{address::Address, diagnostic::ValidationError};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Guest access that failed; debugger reads and writes are separate host operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Data read.
    Read,
    /// Data write.
    Write,
    /// Instruction fetch.
    Fetch,
}

/// Execution fault category, independent of native error representations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum FaultKind {
    /// A requested range is not mapped.
    Unmapped(Access),
    /// A mapping denies the guest access.
    Protection(Access),
    /// An access violates architectural alignment.
    Unaligned(Access),
    /// The guest reports an invalid or unavailable instruction in its current state.
    InvalidInstruction,
    /// The translated instruction requires an operation the runtime cannot lower.
    UnsupportedInstruction,
    /// A processor exception, with its reported vector when available.
    Exception(Option<u32>),
}

/// A terminal guest fault. Partial instruction effects are not rolled back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestFault {
    /// Execution failure category.
    pub kind: FaultKind,
    /// Address of the instruction whose translation or execution faulted.
    pub pc: Address,
    /// Failing memory address, when supplied by the runtime.
    pub address: Option<Address>,
    /// Access width in bytes reported by the backend, if available.
    pub size: Option<u64>,
}

/// The reason an otherwise live session stopped executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    /// One instruction completed.
    Step,
    /// A cooperative pause reached a safe boundary.
    Requested,
    /// Execution stopped before this instruction.
    Breakpoint(Address),
}

/// An explicit terminal outcome; faults and budget limits are never success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Termination {
    /// The experiment's declared completion condition was reached.
    Completed,
    /// An invalid guest operation stopped execution.
    GuestFault,
    /// The user cancelled the experiment.
    Cancelled,
    /// A resource budget was exhausted.
    Budget,
    /// The experiment requested an unimplemented environment interaction.
    UnsupportedEnvironment,
}

/// Worker-owned execution state, independent of editable document state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionState {
    /// An artifact is loaded and has not started.
    Ready,
    /// Execution owns the machine; direct patches are prohibited.
    Running,
    /// A live machine is stopped at a safe boundary.
    Paused(PauseReason),
    /// The experiment ended with an explicit outcome.
    Terminated(Termination),
    /// Native machine state is unusable; loading a replacement is required.
    Crashed,
}

/// A completed control event applied by the machine owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlEvent {
    /// The worker begins a step or a run.
    Start,
    /// Execution reached a safe pause point.
    Pause(PauseReason),
    /// The machine reached a terminal outcome.
    Terminate(Termination),
    /// The loaded artifact was successfully restored.
    Reset,
    /// A native failure invalidated this machine's state.
    Crash,
}

impl ExecutionState {
    /// Compute a control transition after the corresponding operation succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::Transition`] when the event is illegal in the current state.
    pub const fn transition(self, event: ControlEvent) -> Result<Self, ValidationError> {
        use ControlEvent as E;
        match (self, event) {
            (Self::Ready | Self::Paused(_), E::Start) => Ok(Self::Running),
            (Self::Running, E::Pause(reason)) => Ok(Self::Paused(reason)),
            (Self::Running | Self::Ready | Self::Paused(_), E::Terminate(reason)) => {
                Ok(Self::Terminated(reason))
            }
            (Self::Ready | Self::Paused(_) | Self::Terminated(_), E::Reset) => Ok(Self::Ready),
            (_, E::Crash) => Ok(Self::Crashed),
            _ => Err(ValidationError::Transition),
        }
    }

    /// Validate direct mutation; callers must pause running sessions explicitly.
    ///
    /// # Errors
    ///
    /// Rejects running, terminal, and crashed sessions.
    pub const fn require_patchable(self) -> Result<(), ValidationError> {
        match self {
            Self::Ready | Self::Paused(_) => Ok(()),
            _ => Err(ValidationError::Transition),
        }
    }
}

/// A monotonically increasing identifier; exhaustion is an error, never wraparound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Generation(u64);

impl Generation {
    /// The initial generation. The supervisor separately identifies its worker.
    pub const INITIAL: Self = Self(0);

    /// Compute the next generation for a reset; publish it only after replacement succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::CounterExhausted`] rather than reusing an identifier.
    pub fn next(self) -> Result<Self, ValidationError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(ValidationError::CounterExhausted)
    }

    /// Exact counter value for canonical decimal serialization.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}
