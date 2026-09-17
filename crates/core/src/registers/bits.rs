//! Bounded raw register storage, independent of lane interpretation.

use crate::diagnostic::ValidationError;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;
use ts_rs::TS;

/// Exact register bytes, serialized as `0x` followed by 2–512 lowercase hex digits.
///
/// Most-significant digits come first. Byte zero in native storage is least significant.
/// The operation validates the architectural width; this value bounds allocation at 256 bytes.
#[derive(Debug, Clone, PartialEq, Eq, TS)]
#[ts(type = "string")]
pub struct VectorBits(Box<[u8]>);

impl VectorBits {
    /// Retain exact little-endian bytes, including leading zero bits.
    ///
    /// # Errors
    ///
    /// Rejects empty values and storage larger than a 2048-bit SVE register.
    pub fn new(bytes: impl Into<Box<[u8]>>) -> Result<Self, ValidationError> {
        let bytes = bytes.into();
        if !(1..=256).contains(&bytes.len()) {
            return Err(ValidationError::Target);
        }
        Ok(Self(bytes))
    }

    /// Exact little-endian storage, including any padding in the most significant byte.
    #[must_use]
    pub fn as_le_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for VectorBits {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("0x")?;
        for byte in self.0.iter().rev() {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}
impl Serialize for VectorBits {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for VectorBits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = VectorBits;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("0x followed by 1–256 lowercase hexadecimal bytes")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<VectorBits, E> {
                let digits = value
                    .strip_prefix("0x")
                    .ok_or_else(|| E::custom("invalid vector encoding"))?;
                if !(2..=512).contains(&digits.len())
                    || !digits.len().is_multiple_of(2)
                    || !digits
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err(E::custom("invalid vector encoding"));
                }
                let bytes = (0..digits.len())
                    .step_by(2)
                    .rev()
                    .map(|index| {
                        u8::from_str_radix(&digits[index..index + 2], 16).map_err(E::custom)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                VectorBits::new(bytes).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}
