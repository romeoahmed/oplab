//! Validated guest mappings. Host initialization never widens guest permissions.

use crate::{address::AddressRange, diagnostic::ValidationError};

/// Maximum aggregate mapped memory per experiment, independent of viewport limits.
pub const MAX_MAPPED_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum number of independently mapped regions.
pub const MAX_REGIONS: usize = 64;

/// Guest access permissions. A region with no access is a valid guard region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    /// Guest data reads are allowed.
    pub read: bool,
    /// Guest data writes are allowed.
    pub write: bool,
    /// Guest instruction fetches are allowed.
    pub execute: bool,
}

impl Permissions {
    /// Return whether every requested permission is explicitly granted.
    #[must_use]
    pub const fn allows(self, required: Self) -> bool {
        (!required.read || self.read)
            && (!required.write || self.write)
            && (!required.execute || self.execute)
    }
}

/// One validated, page-aligned region and its original contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRegion {
    range: AddressRange,
    permissions: Permissions,
    initial: Vec<u8>,
}

impl MemoryRegion {
    /// Validate a map against backend granularity. Unspecified bytes initialize to zero.
    ///
    /// # Errors
    ///
    /// Rejects invalid page sizes, unaligned maps, oversized maps, and excess contents.
    pub fn new(
        range: AddressRange,
        permissions: Permissions,
        initial: Vec<u8>,
        page_size: u64,
    ) -> Result<Self, ValidationError> {
        if !page_size.is_power_of_two()
            || !range.start().get().is_multiple_of(page_size)
            || !range.length().is_multiple_of(page_size)
        {
            return Err(ValidationError::Alignment);
        }
        if range.length() > MAX_MAPPED_BYTES || initial.len() as u128 > u128::from(range.length()) {
            return Err(ValidationError::Length);
        }
        Ok(Self {
            range,
            permissions,
            initial,
        })
    }

    /// Validated range.
    #[must_use]
    pub const fn range(&self) -> AddressRange {
        self.range
    }

    /// Exact guest permissions.
    #[must_use]
    pub const fn permissions(&self) -> Permissions {
        self.permissions
    }

    /// Initial bytes at the beginning of the mapping; the remaining bytes are zero.
    #[must_use]
    pub fn initial(&self) -> &[u8] {
        &self.initial
    }
}

/// Ordered, non-overlapping mappings with an aggregate allocation budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryLayout(Vec<MemoryRegion>);

impl MemoryLayout {
    /// Sort and validate mappings before any native allocation takes place.
    ///
    /// # Errors
    ///
    /// Rejects empty/oversized layouts, overlap, and aggregate allocation overflow.
    pub fn new(mut regions: Vec<MemoryRegion>) -> Result<Self, ValidationError> {
        if regions.is_empty() || regions.len() > MAX_REGIONS {
            return Err(ValidationError::Length);
        }
        regions.sort_by_key(|region| region.range.start());
        let mut total = 0_u64;
        let mut previous: Option<AddressRange> = None;
        for region in &regions {
            if previous.is_some_and(|range| range.overlaps(region.range)) {
                return Err(ValidationError::Overlap);
            }
            total = total
                .checked_add(region.range.length())
                .ok_or(ValidationError::Length)?;
            if total > MAX_MAPPED_BYTES {
                return Err(ValidationError::Length);
            }
            previous = Some(region.range);
        }
        Ok(Self(regions))
    }

    /// Regions in ascending guest-address order.
    #[must_use]
    pub fn regions(&self) -> &[MemoryRegion] {
        &self.0
    }

    /// Consume the layout for revalidation with additional mappings, without copying bytes.
    #[must_use]
    pub fn into_regions(self) -> Vec<MemoryRegion> {
        self.0
    }

    /// Require one complete range to fit a single mapping with the requested access.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::Permission`] for an unmapped, crossing, or prohibited access.
    pub fn require(
        &self,
        range: AddressRange,
        access: Permissions,
    ) -> Result<&MemoryRegion, ValidationError> {
        self.0
            .iter()
            .find(|region| region.range.contains_range(range) && region.permissions.allows(access))
            .ok_or(ValidationError::Permission)
    }
}
