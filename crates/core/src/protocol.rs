//! Versioned wire values. Deserialization validates scalars; operations validate semantics.

pub mod analysis;
pub mod desktop;
pub mod execution;
pub mod frame;
pub mod image;
pub mod scalar;
pub mod stream;
pub mod transport;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::target::Target;
use execution::{MemoryWindow, Observation, SessionAction, SessionKey};
use scalar::{Counter, HexAddress};

/// Development wire version; clients and workers must use the same application build.
pub const VERSION: u32 = 1;
/// Maximum UTF-8 source bytes, independent of expansion and output budgets.
pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
/// Maximum total allocated section size, including zero-fill but excluding ELF overhead.
pub const MAX_ALLOCATED_BYTES: usize = 64 * 1024;
/// Maximum input bytes for one decode operation.
pub const MAX_DECODE_BYTES: usize = 64 * 1024;
/// Maximum bytes in an ELF object, ELF executable or raw-code load payload.
pub const MAX_OBJECT_BYTES: usize = 1024 * 1024;

/// Exact settings that must still match before a build can replace a document result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BuildIdentity {
    /// Opaque document identity, never a filesystem path.
    pub document: String,
    /// Monotonic source revision, encoded as decimal text.
    pub revision: Counter,
    /// Guest machine architecture.
    pub target: Target,
    /// Link address of the `.text` section; independent of object compilation.
    pub base: HexAddress,
    /// Required backend and version from the handshake.
    pub assembler: AssemblerIdentity,
}

/// Toolchain identity for reproducible assembly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AssemblerIdentity {
    /// Backend package name.
    pub name: String,
    /// Exact release version.
    pub version: String,
}

/// A connection-scoped command and its correlation ID; mutations are not retryable by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Positive, strictly increasing identifier within one worker connection.
    pub id: Counter,
    /// Operation payload.
    pub command: Command,
}

/// Commands accepted by the framed worker after protocol negotiation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Command {
    /// Negotiate compatibility before sending any other command.
    Hello {
        /// The caller's required protocol version.
        version: u32,
    },
    /// Compile and link one complete source revision, retaining both ELF files.
    Assemble {
        /// Identity captured when the build was requested.
        identity: BuildIdentity,
        /// Original source; never included in routine logs.
        source: String,
    },
    /// Cancel an outstanding assembly result; native work stops at stage boundaries.
    CancelAssembly {
        /// Request to cancel on this connection.
        request: Counter,
    },
    /// Decode exact bytes from a known starting boundary.
    Decode {
        /// Guest architecture; static decoding is independent of runtime support.
        target: Target,
        /// First instruction address.
        base: HexAddress,
        /// Exact current bytes.
        bytes: Vec<u8>,
        /// Upper bound on returned instructions.
        limit: u32,
    },
    /// Analyze exactly one complete instruction without observing or changing a machine.
    Analyze {
        /// Guest instruction set.
        target: Target,
        /// Instruction address, used for relative destinations.
        base: HexAddress,
        /// Exact instruction bytes: 1–15 for `x86_64`, exactly four for `AArch64`.
        bytes: Vec<u8>,
    },
    /// Bind an ELF image or raw code, supplied in binary frames after this request.
    Load {
        /// ELF-defined layout or explicit raw-code placement.
        image: execution::LoadImage,
        /// Explicit initial GPRs and additional zero-filled mappings.
        initial: execution::InitialState,
        /// Current session to replace; null requires no active session.
        replace: Option<SessionKey>,
        /// Required guest architecture; ELF inputs must declare the same target.
        target: Target,
        /// Guest address at which execution completes before fetching an instruction.
        completion: HexAddress,
        /// Maximum architectural instruction starts.
        instruction_budget: Counter,
        /// Complete binary payload length, between one byte and one MiB.
        image_bytes: u32,
    },
    /// Operate on an exact session generation at its owning thread boundary.
    Execute {
        /// Reject stale commands before inspecting or changing the machine.
        session: SessionKey,
        /// Control or observation request.
        action: SessionAction,
    },
    /// Replace the connection's observation subscription after validating its window.
    Subscribe {
        /// Exact loaded generation to observe.
        session: SessionKey,
        /// Null observes registers and control state only.
        memory: Option<MemoryWindow>,
    },
    /// Stop an exact subscription without mutating the machine.
    Unsubscribe {
        /// ID of the successful Subscribe request.
        subscription: Counter,
    },
    /// Drain accepted work and output, then send the final reply and close.
    Shutdown,
}

/// One terminal reply for one accepted request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Response {
    /// Echoes the request identity without lossy numeric conversion.
    pub id: Counter,
    /// Explicit success or failure.
    pub result: Reply,
}

/// Tagged replies prevent an error from being confused with empty successful output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Reply {
    /// Negotiated worker capabilities.
    Hello(Capabilities),
    /// Build metadata followed by the complete object and executable binary payloads.
    Assembled(Artifact),
    /// Cancellation was accepted; the original request receives its own error reply.
    AssemblyCancelled(Counter),
    /// A bounded sequence of instructions.
    Decoded(Vec<DecodedInstruction>),
    /// Static decoder facts for the requested instruction.
    Analyzed(Box<analysis::InstructionAnalysis>),
    /// One coherent observation; optional memory follows as binary frames.
    Observed(Box<Observation>),
    /// The selected session was dropped on its owning thread.
    SessionClosed(SessionKey),
    /// Observation events will follow; the ID is the Subscribe request ID.
    Subscribed(Counter),
    /// The selected subscription ended; no later events will be written for it.
    Unsubscribed(Counter),
    /// Accepted work and output have drained; the connection is closing.
    Closed,
    /// The request failed without a successful result.
    Error(Diagnostic),
}

/// Operations and targets available on the negotiated worker connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    /// Negotiated protocol version.
    pub version: u32,
    /// Guest targets supported by this worker.
    pub targets: Vec<Target>,
    /// Assembler used for this worker.
    pub assembler: AssemblerIdentity,
    /// Whether verified source-to-instruction mapping is available.
    pub source_mapping: bool,
    /// Whether this worker currently supports machine sessions.
    pub execution: bool,
}

/// Metadata for two complete ELF files transferred after the control frame.
///
/// Payload order is relocatable object, then linked executable image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// Full identity echoed from the request.
    pub identity: BuildIdentity,
    /// Exact object file length, bounded to one MiB.
    pub object_bytes: u32,
    /// Exact executable file length, bounded to one MiB.
    pub image_bytes: u32,
    /// Validated entry, segments, and bounded symbol view of the linked ELF.
    pub image: image::ImageInfo,
}

/// A decoded instruction with its original bytes and display text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DecodedInstruction {
    /// Address of the instruction's first byte.
    pub address: HexAddress,
    /// Exact instruction bytes.
    pub bytes: Vec<u8>,
    /// Backend display text; never used to reconstruct machine code.
    pub text: String,
}

/// Stable error codes for localized presentation. Raw source and paths are excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    /// The connection has not negotiated a compatible protocol.
    Protocol,
    /// Invalid input or target-specific settings.
    InvalidInput,
    /// The requested assembler settings do not match the worker.
    BackendMismatch,
    /// The backend failed without producing a trustworthy result.
    BackendFailure,
    /// Assembly rejected the supplied source.
    Assembly,
    /// A configured input or output budget was exceeded.
    ResourceLimit,
    /// Bytes could not be decoded as complete instructions.
    Decode,
    /// The selected session or generation is no longer current.
    StaleSession,
    /// The operation is illegal in the current execution state.
    InvalidState,
    /// An accepted operation was explicitly cancelled before publishing its result.
    Cancelled,
    /// A newer request replaced pending work for the same document.
    Superseded,
}

/// Structured failure details. Optional fields serialize as explicit null values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    /// Stable, locale-independent failure category.
    pub code: DiagnosticCode,
    /// Original UTF-8 source byte position, when supplied by the assembler.
    pub source_offset: Option<u32>,
    /// The failing guest address, when known.
    pub address: Option<HexAddress>,
}

impl Diagnostic {
    /// Construct a failure without inventing a location.
    #[must_use]
    pub const fn new(code: DiagnosticCode) -> Self {
        Self {
            code,
            address: None,
            source_offset: None,
        }
    }
}
