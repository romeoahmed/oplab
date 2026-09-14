//! Guest machine targets shared by validation, engines, and transport.

/// Initial little-endian, 64-bit guest profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// x86 long mode, independent of assembly display syntax.
    X86_64,
    /// Arm A64 with 64-bit addresses.
    Aarch64,
}

impl Target {
    /// Required instruction-start alignment, independent of mapping granularity.
    #[must_use]
    pub const fn instruction_alignment(self) -> u64 {
        match self {
            Self::X86_64 => 1,
            Self::Aarch64 => 4,
        }
    }
}
