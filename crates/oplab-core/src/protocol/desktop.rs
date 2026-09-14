//! Desktop ownership and failure metadata; native machine semantics remain shared.

use super::{Artifact, Capabilities, Command, execution::Observation, scalar::Counter};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Metadata in the first frame of a binary desktop command invocation.
#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DesktopCall {
    /// Worker process incarnation.
    pub connection: Counter,
    /// Current frontend attachment; replaced on reload or reattachment.
    pub view: Counter,
    /// Shared worker operation. The supervisor assigns its request ID.
    pub command: Command,
}

/// Current desktop attachment without replaying any mutation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConnectionInfo {
    /// Worker process incarnation, never reused by this desktop process.
    pub connection: Counter,
    /// Frontend attachment identity; obsolete views cannot submit new work.
    pub view: Counter,
    /// Verified startup handshake.
    pub capabilities: Capabilities,
    /// Last coherent machine status; a fresh subscription obtains current values.
    pub session: Option<Box<Observation>>,
    /// Latest successful build metadata, independent of the loaded session.
    pub artifact: Option<Artifact>,
}

/// Stable supervisor failures for localized presentation; no native logs or paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    /// The packaged executable or a process thread could not start.
    Unavailable,
    /// The command belongs to an obsolete connection or frontend attachment.
    StaleConnection,
    /// The bounded command queue has no capacity.
    Busy,
    /// A pipe closed or the child exited unexpectedly.
    Disconnected,
    /// A command or continuous run exceeded its wall-clock budget.
    Deadline,
    /// The child's observed resident memory exceeded the configured cutoff.
    MemoryLimit,
    /// Framing, correlation, or observation coherence failed.
    Protocol,
    /// The caller is not the authorized application window.
    Unauthorized,
}

/// A failed invocation never implies that an admitted mutation can be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DesktopFailure {
    /// Stable failure category.
    pub code: FailureCode,
    /// Worker request ID, if admitted.
    pub request: Option<Counter>,
    /// The request began writing; its effect may have occurred without a reply.
    pub outcome_unknown: bool,
}

impl DesktopFailure {
    /// Failure before a request has entered the worker's input pipe.
    #[must_use]
    pub const fn new(code: FailureCode) -> Self {
        Self {
            code,
            request: None,
            outcome_unknown: false,
        }
    }
}
