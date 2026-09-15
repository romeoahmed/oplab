//! Initial machine inputs share one validated memory layout before native allocation.

use super::{LoadError, LoadPlan};
use oplab_core::{
    address::{Address, AddressRange},
    diagnostic::ValidationError,
    memory::{MAX_MAPPED_BYTES, MAX_REGIONS, MemoryLayout, MemoryRegion, Permissions},
    protocol::MAX_OBJECT_BYTES,
    registers::InitialRegisters,
    target::Target,
};

/// Guest bytes retain their format; raw code needs explicit addresses.
#[derive(Debug, Clone, Copy)]
pub enum Image<'a> {
    /// A complete standard ELF64 executable, with authoritative entry and mappings.
    Elf(&'a [u8]),
    /// Exact bytes mapped read/execute. Padding is zero; no ELF or ABI is synthesized.
    Raw {
        /// Nonempty byte extent, at most 1 MiB.
        bytes: &'a [u8],
        /// Address of the first byte, independent of page alignment.
        base: Address,
        /// Aligned initial fetch inside the supplied bytes.
        entry: Address,
    },
}

/// An additional page-aligned mapping. Omitted tail bytes initialize to zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialMapping {
    /// Complete mapping extent; must fit backend page geometry and machine budgets.
    pub range: AddressRange,
    /// Exact guest permissions, including no-access guard regions.
    pub permissions: Permissions,
    /// Bytes at the mapping start; at most the mapping length.
    pub bytes: Vec<u8>,
}

/// Explicit initial conditions, retained by the machine for reset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineSetup {
    /// `None` retains native defaults; an explicit bank must match the guest.
    pub registers: Option<InitialRegisters>,
    /// Additional mappings; overlap with each other or image pages is rejected.
    pub mappings: Vec<InitialMapping>,
}

impl LoadPlan {
    /// Validate image, registers and additional mappings before native allocation.
    ///
    /// # Errors
    ///
    /// Rejects unsupported ELF, raw entry/geometry, target mismatch, overlapping or
    /// unaligned mappings, and aggregate memory limits. ELF permissions are never widened.
    pub fn new(
        image: Image<'_>,
        target: Target,
        page_size: u64,
        setup: MachineSetup,
    ) -> Result<Self, LoadError> {
        if setup
            .registers
            .as_ref()
            .is_some_and(|bank| bank.target() != target)
        {
            return Err(ValidationError::Target.into());
        }
        if setup.mappings.len() >= MAX_REGIONS {
            return Err(ValidationError::Length.into());
        }
        let mut plan = match image {
            Image::Elf(bytes) => Self::from_elf(bytes, target, page_size)?,
            Image::Raw { bytes, base, entry } => {
                Self::from_raw(bytes, target, base, entry, page_size)?
            }
        };
        let mut regions = plan.memory.into_regions();
        for mapping in setup.mappings {
            regions.push(MemoryRegion::new(
                mapping.range,
                mapping.permissions,
                mapping.bytes,
                page_size,
            )?);
        }
        plan.memory = MemoryLayout::new(regions)?;
        plan.registers = setup.registers;
        Ok(plan)
    }

    fn from_raw(
        bytes: &[u8],
        target: Target,
        base: Address,
        entry: Address,
        page_size: u64,
    ) -> Result<Self, LoadError> {
        if !page_size.is_power_of_two() || page_size > MAX_MAPPED_BYTES {
            return Err(ValidationError::Alignment.into());
        }
        let contents = AddressRange::new(base, bytes.len() as u64, MAX_OBJECT_BYTES as u64)?;
        let alignment = target.instruction_alignment();
        let fetch = AddressRange::new(entry, alignment, alignment).map_err(|_| LoadError::Entry)?;
        if !entry.get().is_multiple_of(alignment) || !contents.contains_range(fetch) {
            return Err(LoadError::Entry);
        }
        let start = base.get() & !(page_size - 1);
        let end = contents.end().next_multiple_of(u128::from(page_size));
        let length = u64::try_from(end - u128::from(start)).map_err(|_| ValidationError::Length)?;
        let range = AddressRange::new(Address::new(start), length, MAX_MAPPED_BYTES)?;
        let offset = usize::try_from(base.get() - start).map_err(|_| ValidationError::Length)?;
        let mut initial = Vec::with_capacity(offset + bytes.len());
        initial.resize(offset, 0);
        initial.extend_from_slice(bytes);
        let region = MemoryRegion::new(
            range,
            Permissions {
                read: true,
                write: false,
                execute: true,
            },
            initial,
            page_size,
        )?;
        Ok(Self {
            target,
            entry,
            page_size,
            memory: MemoryLayout::new(vec![region])?,
            registers: None,
        })
    }
}
