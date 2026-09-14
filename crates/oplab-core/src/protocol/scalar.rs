//! Exact wire scalars. JSON numbers are rejected for precision-sensitive values.

use crate::address::Address;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;
use ts_rs::TS;

/// A non-wrapping unsigned counter serialized as canonical decimal text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, TS)]
#[ts(type = "string")]
pub struct Counter(u64);

impl Counter {
    /// Construct from an exact Rust integer.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Recover the exact counter value after validated deserialization.
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
    /// Construct from a domain address without converting through floating point.
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
