//! Guest machine targets shared by validation, engines, and transport.

/// Supported little-endian, 64-bit guest profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// x86 long mode, independent of assembly display syntax.
    X86_64,
    /// Arm A64 with 64-bit addresses.
    Aarch64,
}

impl Target {
    /// Required instruction-start alignment in bytes, independent of page size.
    #[must_use]
    pub const fn instruction_alignment(self) -> u64 {
        match self {
            Self::X86_64 => 1,
            Self::Aarch64 => 4,
        }
    }
}

/// Explicit emulator profiles, not guarantees of complete physical-CPU behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum CpuModel {
    /// Intel Nehalem profile.
    Nehalem,
    /// Intel Haswell profile; the default for `x86_64`.
    Haswell,
    /// Arm Cortex-A53 profile.
    CortexA53,
    /// Arm Cortex-A72 profile; the default for `AArch64`.
    CortexA72,
}

impl CpuModel {
    /// Architecture required by this profile.
    #[must_use]
    pub const fn target(self) -> Target {
        match self {
            Self::Nehalem | Self::Haswell => Target::X86_64,
            Self::CortexA53 | Self::CortexA72 => Target::Aarch64,
        }
    }

    /// Stable project default, selected explicitly instead of relying on the backend.
    #[must_use]
    pub const fn default_for(target: Target) -> Self {
        match target {
            Target::X86_64 => Self::Haswell,
            Target::Aarch64 => Self::CortexA72,
        }
    }
}
