//! Exact wire scalars. JSON numbers are rejected for precision-sensitive values.

use crate::address::Address;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;
use ts_rs::TS;

/// An unsigned 64-bit integer serialized as decimal text, also used for register values.
///
/// Deserialization accepts ASCII digits without leading zeros, except `0` itself.
/// Signs, whitespace, JSON numbers and values above the unsigned 64-bit range fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, TS)]
#[ts(type = "string")]
pub struct Counter(u64);

impl Counter {
    /// Construct from an exact Rust integer.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the exact integer value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl Serialize for Counter {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Counter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = Counter;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a canonical unsigned 64-bit decimal string")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.is_empty()
                    || value.len() > 20
                    || (value.len() > 1 && value.starts_with('0'))
                    || !value.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return Err(E::custom("invalid counter encoding"));
                }
                value
                    .parse()
                    .map(Counter)
                    .map_err(|_| E::custom("counter exceeds 64 bits"))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

/// A guest address serialized as `0x` followed by sixteen lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, TS)]
#[ts(type = "string")]
pub struct HexAddress(Address);

impl HexAddress {
    /// Wrap a domain address for canonical wire serialization.
    #[must_use]
    pub const fn new(address: Address) -> Self {
        Self(address)
    }

    /// Validated domain address.
    #[must_use]
    pub const fn address(self) -> Address {
        self.0
    }
}

impl Serialize for HexAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for HexAddress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = HexAddress;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a canonical 64-bit hexadecimal address string")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let address: Address = value.parse().map_err(E::custom)?;
                if value != address.to_string() {
                    return Err(E::custom("noncanonical wire address"));
                }
                Ok(HexAddress(address))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

/// Raw 128-bit register bits: `0x` followed by exactly 32 lowercase hex digits.
///
/// The most significant digit comes first; lane zero occupies the low bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(type = "string")]
pub struct VectorBits(u128);

impl VectorBits {
    /// Preserve the complete register value without floating-point conversion.
    #[must_use]
    pub const fn new(value: u128) -> Self {
        Self(value)
    }
}

impl Serialize for VectorBits {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("0x{:032x}", self.0))
    }
}

impl<'de> Deserialize<'de> for VectorBits {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = VectorBits;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("0x followed by 32 lowercase hexadecimal digits")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let digits = value
                    .strip_prefix("0x")
                    .ok_or_else(|| E::custom("invalid vector encoding"))?;
                if digits.len() != 32
                    || !digits
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return Err(E::custom("invalid vector encoding"));
                }
                u128::from_str_radix(digits, 16)
                    .map(VectorBits)
                    .map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}
