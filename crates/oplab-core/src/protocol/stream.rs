//! Subscription events are independent of correlated request outcomes.

use super::{
    Diagnostic,
    execution::{Fault, Observation, Registers, SessionKey, Status},
    scalar::Counter,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One subscription event, followed by any declared binary memory bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct StreamEvent {
    /// ID of the successful Subscribe request; scoped to this connection.
    pub subscription: Counter,
    /// Complete state, a delta from a delivered state, or a capture failure.
    pub update: ObservationUpdate,
}

/// A delta never depends on an observation that was coalesced before delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ObservationUpdate {
    /// Establish a baseline; optional memory follows in binary frames.
    Full(Box<Observation>),
    /// Update the exact baseline within the same subscription and memory window.
    Delta(Box<ObservationDelta>),
    /// The subscription ended because a coherent capture was unavailable.
    Ended(Diagnostic),
}

/// Register banks are replaced atomically, preserving their architecture and exact bits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RegisterUpdate {
    /// Retain the complete bank from the baseline.
    Unchanged,
    /// Replace the complete bank, or clear it after native state loss.
    Replace(Option<Box<Registers>>),
}

/// State at a new coherent boundary. Status, counters, and fault are always current;
/// unchanged registers and memory are inherited only from the named baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ObservationDelta {
    /// Must exactly match the baseline's session and generation.
    pub key: SessionKey,
    /// Sequence of the previously delivered stream observation.
    pub base: Counter,
    /// New sequence, strictly greater than base; gaps from coalescing are legal.
    pub sequence: Counter,
    /// Current execution state.
    pub status: Status,
    /// Current architectural instruction starts.
    pub instructions: Counter,
    /// Current native dispatch work.
    pub dispatches: Counter,
    /// Atomic bank change, including explicit clearing on a crash.
    pub registers: RegisterUpdate,
    /// Current fault; null clears the preceding fault.
    pub fault: Option<Fault>,
    /// Zero retains memory. Otherwise the complete baseline window follows as
    /// binary bytes, with this exact length. Window changes require a full event.
    pub memory_bytes: u32,
}
