//! Exact guest addresses and non-wrapping ranges.

use std::{fmt, str::FromStr};

use crate::diagnostic::ValidationError;

/// A 64-bit guest virtual address, independent of host pointer size.
///
/// Parsing accepts `0x` or `0X` followed by one to sixteen hexadecimal digits.
/// Display uses `0x` and sixteen lowercase digits. Neither permits address wraparound.
///
/// # Examples
///
/// ```
/// use oplab_core::address::{Address, AddressRange};
///
/// let last: Address = "0xFFFFFFFFFFFFFFFF".parse()?;
/// let range = AddressRange::new(last, 1, 4096)?;
/// assert_eq!(range.end(), 1_u128 << 64);
/// assert!(last.checked_add(1).is_err());
/// # Ok::<(), oplab_core::diagnostic::ValidationError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address(u64);

impl Address {
    /// Construct an address from an exact integer.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the exact guest address.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Add a byte offset without permitting address wraparound.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::AddressOverflow`] if the result exceeds `u64::MAX`.
    pub fn checked_add(self, offset: u64) -> Result<Self, ValidationError> {
        self.0
            .checked_add(offset)
            .map(Self)
            .ok_or(ValidationError::AddressOverflow)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:016x}", self.0)
    }
}

impl FromStr for Address {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let digits = value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
            .ok_or(ValidationError::AddressFormat)?;
        if digits.is_empty()
            || digits.len() > 16
            || !digits.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ValidationError::AddressFormat);
        }
        u64::from_str_radix(digits, 16)
            .map(Self)
            .map_err(|_| ValidationError::AddressFormat)
    }
}

/// A nonempty range whose exclusive end may equal 2^64.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressRange {
    start: Address,
    length: u64,
}

impl AddressRange {
    /// Construct a bounded range. The limit is supplied by the owning operation.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or overflowing ranges.
    pub fn new(start: Address, length: u64, limit: u64) -> Result<Self, ValidationError> {
        if length == 0 || length > limit {
            return Err(ValidationError::Length);
        }
        if u128::from(start.get()) + u128::from(length) > (1_u128 << 64) {
            return Err(ValidationError::AddressOverflow);
        }
        Ok(Self { start, length })
    }

    /// First included address.
    #[must_use]
    pub const fn start(self) -> Address {
        self.start
    }

    /// Number of bytes in the range.
    #[must_use]
    pub const fn length(self) -> u64 {
        self.length
    }

    /// Exclusive end, including the representable boundary just above `u64::MAX`.
    #[must_use]
    pub fn end(self) -> u128 {
        u128::from(self.start.get()) + u128::from(self.length)
    }

    /// Whether one guest address lies inside this range.
    #[must_use]
    pub fn contains(self, address: Address) -> bool {
        address >= self.start && u128::from(address.get()) < self.end()
    }

    /// Whether every byte in another range belongs to this range.
    #[must_use]
    pub fn contains_range(self, other: Self) -> bool {
        other.start >= self.start && other.end() <= self.end()
    }

    /// Whether the ranges share any byte; adjacent ranges do not overlap.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        u128::from(self.start.get()) < other.end() && u128::from(other.start.get()) < self.end()
    }
}
