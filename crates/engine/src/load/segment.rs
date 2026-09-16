//! Borrow bounded file contents while validating segment and page geometry.

use super::LoadError;
use object::{
    LittleEndian, elf,
    read::elf::{FileHeader, ProgramHeader},
};
use oplab_core::{
    address::{Address, AddressRange},
    diagnostic::ValidationError,
    memory::{MAX_MAPPED_BYTES, Permissions},
};

pub(super) struct Segment<'a> {
    pub range: AddressRange,
    pub pages: AddressRange,
    pub permissions: Permissions,
    pub data: &'a [u8],
}

pub(super) fn read<'a>(
    header: &elf::FileHeader64<LittleEndian>,
    image: &'a [u8],
    page_size: u64,
) -> Result<Vec<Segment<'a>>, LoadError> {
    let headers = header
        .program_headers(LittleEndian, image)
        .map_err(|_| LoadError::Format)?;
    if headers.len() > 256 {
        return Err(ValidationError::Length.into());
    }
    let mut segments: Vec<Segment<'_>> = Vec::new();
    let mut previous_address = None;
    for header in headers {
        match header.p_type(LittleEndian) {
            elf::PT_LOAD => {
                let address = header.p_vaddr(LittleEndian);
                if previous_address.is_some_and(|previous| previous > address) {
                    return Err(LoadError::Geometry);
                }
                previous_address = Some(address);
                if let Some(segment) = load(header, image, page_size)? {
                    if segments
                        .last()
                        .is_some_and(|previous| previous.range.overlaps(segment.range))
                    {
                        return Err(LoadError::Geometry);
                    }
                    segments.push(segment);
                }
            }
            elf::PT_NULL
            | elf::PT_NOTE
            | elf::PT_PHDR
            | elf::PT_GNU_EH_FRAME
            | elf::PT_GNU_SFRAME => {}
            elf::PT_GNU_STACK
                if !header.p_flags(LittleEndian).contains(elf::PF_X)
                    && header.p_memsz(LittleEndian) == 0 => {}
            _ => return Err(LoadError::Runtime),
        }
    }
    Ok(segments)
}

fn load<'a>(
    header: &elf::ProgramHeader64<LittleEndian>,
    image: &'a [u8],
    page_size: u64,
) -> Result<Option<Segment<'a>>, LoadError> {
    let start = header.p_vaddr(LittleEndian);
    let offset = header.p_offset(LittleEndian);
    let size = header.p_memsz(LittleEndian);
    let file_size = header.p_filesz(LittleEndian);
    let alignment = header.p_align(LittleEndian).max(1);
    if file_size > size
        || !alignment.is_power_of_two()
        || start % alignment != offset % alignment
        || start % page_size != offset % page_size
    {
        return Err(LoadError::Geometry);
    }
    let flags = header.p_flags(LittleEndian);
    if flags.0 & !(elf::PF_R | elf::PF_W | elf::PF_X).0 != 0 {
        return Err(LoadError::Runtime);
    }
    if size == 0 {
        return Ok(None);
    }
    let range = AddressRange::new(Address::new(start), size, MAX_MAPPED_BYTES)?;
    let page_start = start - start % page_size;
    let page_end = range.end().next_multiple_of(u128::from(page_size));
    let page_length =
        u64::try_from(page_end - u128::from(page_start)).map_err(|_| ValidationError::Length)?;
    let pages = AddressRange::new(Address::new(page_start), page_length, MAX_MAPPED_BYTES)?;
    let data = if file_size == 0 {
        &[]
    } else {
        header
            .data(LittleEndian, image)
            .map_err(|()| LoadError::Geometry)?
    };
    Ok(Some(Segment {
        range,
        pages,
        data,
        permissions: Permissions {
            read: flags.contains(elf::PF_R),
            write: flags.contains(elf::PF_W),
            execute: flags.contains(elf::PF_X),
        },
    }))
}
