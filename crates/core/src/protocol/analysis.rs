//! Decoder-reported static facts, independent of a loaded machine or source map.

use super::scalar::HexAddress;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Static metadata for exactly one instruction. Missing facts are not proof of no effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct InstructionAnalysis {
    /// Explicit and implicit register accesses reported by the decoder, retaining aliases.
    pub registers: Vec<RegisterAccess>,
    /// Reported memory accesses; addresses require machine state and are not evaluated.
    pub memory: Vec<MemoryAccess>,
    /// Direct near/relative control-transfer destination, when reported; never a fallthrough.
    pub branch_target: Option<HexAddress>,
    /// Architecture-specific facts retain the backend's distinct semantics.
    pub architecture: ArchitectureAnalysis,
}

/// Reported register or memory data access. Conditions are static, not observed outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DataAccess {
    /// Backend provides no access classification.
    Unknown,
    /// Read.
    Read,
    /// Read only under an instruction-dependent condition.
    ConditionalRead,
    /// Write.
    Write,
    /// Write only under an instruction-dependent condition.
    ConditionalWrite,
    /// Read and write.
    ReadWrite,
    /// Read, with a conditional write.
    ReadConditionalWrite,
    /// Conditional read, followed by an unconditional write.
    ConditionalReadWrite,
}

/// One backend-named register access; aliases are not expanded into machine registers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RegisterAccess {
    /// Architectural register spelling, without localization.
    pub name: String,
    /// Reported access semantics.
    pub access: DataAccess,
}

/// A reported memory access, including implicit stack or string accesses when available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MemoryAccess {
    /// Reported access semantics; unknown is distinct from no access.
    pub access: DataAccess,
    /// Decoder-reported operand size in bytes; not total REP traffic or cache-line size.
    pub bytes: Option<u32>,
}

/// Backend-specific metadata. Recognition and feature tags do not promise execution support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ArchitectureAnalysis {
    /// Intel XED instruction information.
    X86 {
        /// Control-flow classification.
        flow: FlowControl,
        /// Intel XED ISA-set identifier; not a complete CPUID predicate.
        isa: String,
        /// RFLAGS and x87 condition-code effects, retaining undefined and constant results.
        flags: FlagEffects,
        /// The decoder reports incomplete register effects, such as save/restore state.
        registers_incomplete: bool,
        /// Decoder identifies a privileged instruction; no OS environment is implied.
        privileged: bool,
    },
    /// LLVM MC `AArch64` detail; conservative static effects.
    Aarch64 {
        /// Control-flow groups; no architectural extension requirements are inferred.
        groups: Vec<String>,
        /// Decoder reports that the instruction updates condition flags.
        updates_flags: bool,
        /// Decoder reports base-register writeback.
        writeback: bool,
    },
}

/// x86 control-flow categories, independent of whether a branch is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FlowControl {
    /// Continue at the following instruction.
    Next,
    /// Direct unconditional branch.
    Branch,
    /// Indirect unconditional branch.
    IndirectBranch,
    /// Conditional branch.
    ConditionalBranch,
    /// Direct call, including special system transfers classified as calls by the backend.
    Call,
    /// Indirect call.
    IndirectCall,
    /// Return, including system returns.
    Return,
    /// Interrupt.
    Interrupt,
    /// Transaction begin/abort/end control.
    Transaction,
    /// Instruction always raises an exception.
    Exception,
}

/// Backend flag names, not numeric register masks or observed values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlagEffects {
    /// Flags read by the instruction.
    pub read: Vec<String>,
    /// Flags written with a computed value.
    pub written: Vec<String>,
    /// Flags forced to zero.
    pub cleared: Vec<String>,
    /// Flags forced to one.
    pub set: Vec<String>,
    /// Flags whose resulting values are architecturally undefined.
    pub undefined: Vec<String>,
}
