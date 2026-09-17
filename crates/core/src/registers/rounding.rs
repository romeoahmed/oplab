//! IEEE-754 rounding directions and their architectural control fields.

use crate::target::Target;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Rounding direction for instructions that consult MXCSR.RC or FPCR.RMode.
///
/// Explicit instruction rounding modes and x87 control are unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    /// Nearest representable value, with ties resolved to an even significand.
    NearestEven,
    /// Round toward negative infinity.
    Down,
    /// Round toward positive infinity.
    Up,
    /// Round toward zero.
    TowardZero,
}

impl RoundingMode {
    /// Replace only the architecture's two rounding bits; retain all other state.
    #[must_use]
    pub const fn apply(self, target: Target, previous: u32) -> u32 {
        let (shift, down, up) = match target {
            Target::X86_64 => (13, 1, 2),
            Target::Aarch64 => (22, 2, 1),
        };
        let bits = match self {
            Self::NearestEven => 0,
            Self::Down => down,
            Self::Up => up,
            Self::TowardZero => 3,
        };
        (previous & !(3 << shift)) | (bits << shift)
    }
}
