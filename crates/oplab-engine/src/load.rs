//! Plan static ELF64 mappings from program headers before allocating native memory.

mod segment;

use object::{LittleEndian, elf, read::elf::FileHeader};
use oplab_core::{
    address::{Address, AddressRange},
    diagnostic::ValidationError,
    memory::{MAX_MAPPED_BYTES, MAX_REGIONS, MemoryLayout, MemoryRegion, Permissions},
    protocol::MAX_OBJECT_BYTES,
    target::Target,
};
use segment::Segment;

/// A rejected image or mapping; messages never expose file contents or host paths.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    /// The file is not a well-formed supported ELF64 image.
    #[error("invalid ELF image")]
    Format,
    /// The image targets another architecture or encoding.
    #[error("ELF image does not match the guest target")]
    Target,
    /// The image needs an OS loader, dynamic linker, TLS, or unsupported segment policy.
    #[error("ELF image requires unsupported runtime services")]
    Runtime,
    /// Segment order, file bounds, or alignment is inconsistent.
    #[error("invalid ELF segment geometry")]
    Geometry,
    /// Two distinct segment permissions would have to occupy the same native page.
    #[error("ELF segments require conflicting permissions on one page")]
    PagePermissions,
    /// The entry is outside executable segment contents or is misaligned.
    #[error("ELF entry is not an executable instruction address")]
    Entry,
    /// A checked domain limit or mapping invariant was violated.
    #[error(transparent)]
    Mapping(#[from] ValidationError),
}

/// Validated initial image, independent of any emulator handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPlan {
    target: Target,
    entry: Address,
    page_size: u64,
    memory: MemoryLayout,
}

impl LoadPlan {
    /// Read a static executable without depending on section headers or symbol names.
    /// Page padding and storage beyond `p_filesz` initialize to zero.
    ///
    /// # Errors
    /// Rejects unsupported runtime requirements, invalid geometry, conflicting page
    /// permissions, invalid entry points, and excessive allocation before copying data.
    pub fn from_elf(image: &[u8], target: Target, page_size: u64) -> Result<Self, LoadError> {
        if image.len() > MAX_OBJECT_BYTES {
            return Err(ValidationError::Length.into());
        }
        if !page_size.is_power_of_two() || page_size > MAX_MAPPED_BYTES {
            return Err(ValidationError::Alignment.into());
        }
        let header = header(image, target)?;
        let segments = segment::read(header, image, page_size)?;
        let entry = Address::new(header.e_entry(LittleEndian));
        validate_entry(&segments, entry, target)?;
        let mappings = mappings(&segments)?;
        let regions = mappings
            .into_iter()
            .map(|mapping| mapping.initialize(&segments, page_size))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            target,
            entry,
            page_size,
            memory: MemoryLayout::new(regions)?,
        })
    }

    /// Architecture required by this image.
    #[must_use]
    pub const fn target(&self) -> Target {
        self.target
    }

    /// Initial instruction address recorded in the ELF header.
    #[must_use]
    pub const fn entry(&self) -> Address {
        self.entry
    }

    /// Mapping granularity used during validation.
    #[must_use]
    pub const fn page_size(&self) -> u64 {
        self.page_size
    }

    /// Exact initial mappings, including zero-filled tails.
    #[must_use]
    pub const fn memory(&self) -> &MemoryLayout {
        &self.memory
    }
}

fn header(image: &[u8], target: Target) -> Result<&elf::FileHeader64<LittleEndian>, LoadError> {
    let header = elf::FileHeader64::<LittleEndian>::parse(image).map_err(|_| LoadError::Format)?;
    let machine = match target {
        Target::X86_64 => elf::EM_X86_64,
        Target::Aarch64 => elf::EM_AARCH64,
    };
    if !header.is_little_endian() || header.e_machine(LittleEndian) != machine {
        return Err(LoadError::Target);
    }
    if header.e_version(LittleEndian) != u32::from(elf::EV_CURRENT.0)
        || usize::from(header.e_ehsize(LittleEndian)) != size_of_val(header)
    {
        return Err(LoadError::Format);
    }
    if header.e_type(LittleEndian) != elf::ET_EXEC || header.e_flags(LittleEndian).0 != 0 {
        return Err(LoadError::Runtime);
    }
    Ok(header)
}

fn validate_entry(
    segments: &[Segment<'_>],
    entry: Address,
    target: Target,
) -> Result<(), LoadError> {
    let alignment = target.instruction_alignment();
    let fetch = AddressRange::new(entry, alignment, alignment).map_err(|_| LoadError::Entry)?;
    if !entry.get().is_multiple_of(alignment)
        || !segments
            .iter()
            .any(|segment| segment.permissions.execute && segment.range.contains_range(fetch))
    {
        return Err(LoadError::Entry);
    }
    Ok(())
}

struct Mapping {
    range: AddressRange,
    permissions: Permissions,
}

impl Mapping {
    fn initialize(
        self,
        segments: &[Segment<'_>],
        page_size: u64,
    ) -> Result<MemoryRegion, LoadError> {
        // Keep the zero tail implicit. File bytes are copied only at their segment
        // addresses; unrelated bytes outside p_filesz never become guest contents.
        let included = || {
            segments
                .iter()
                .filter(|segment| self.range.contains_range(segment.range))
        };
        let length = included()
            .filter(|segment| !segment.data.is_empty())
            .map(|segment| {
                segment.range.start().get() - self.range.start().get() + segment.data.len() as u64
            })
            .max()
            .unwrap_or(0);
        let length = usize::try_from(length).map_err(|_| ValidationError::Length)?;
        let mut initial = vec![0; length];
        for segment in included().filter(|segment| !segment.data.is_empty()) {
            let offset = usize::try_from(segment.range.start().get() - self.range.start().get())
                .map_err(|_| ValidationError::Length)?;
            initial[offset..offset + segment.data.len()].copy_from_slice(segment.data);
        }
        Ok(MemoryRegion::new(
            self.range,
            self.permissions,
            initial,
            page_size,
        )?)
    }
}

fn mappings(segments: &[Segment<'_>]) -> Result<Vec<Mapping>, LoadError> {
    let mut mappings: Vec<Mapping> = Vec::new();
    for segment in segments {
        if let Some(previous) = mappings.last_mut() {
            let start = u128::from(segment.pages.start().get());
            if start < previous.range.end() && previous.permissions != segment.permissions {
                return Err(LoadError::PagePermissions);
            }
            if start <= previous.range.end() && previous.permissions == segment.permissions {
                let end = previous.range.end().max(segment.pages.end());
                let length = u64::try_from(end - u128::from(previous.range.start().get()))
                    .map_err(|_| ValidationError::Length)?;
                previous.range =
                    AddressRange::new(previous.range.start(), length, MAX_MAPPED_BYTES)?;
                continue;
            }
        }
        mappings.push(Mapping {
            range: segment.pages,
            permissions: segment.permissions,
        });
    }
    if mappings.len() > MAX_REGIONS
        || mappings
            .iter()
            .map(|mapping| mapping.range.length())
            .sum::<u64>()
            > MAX_MAPPED_BYTES
    {
        return Err(ValidationError::Length.into());
    }
    Ok(mappings)
}
