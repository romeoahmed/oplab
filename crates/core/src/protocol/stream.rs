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

/// A full observation, a delta from a delivered baseline, or a capture failure.
///
/// Coalesced samples that were never delivered cannot become delta baselines.
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

/// A coherent update relative to the last delivered subscription sample.
///
/// Status, counters and fault are current; unchanged registers and memory come from `base`.
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
