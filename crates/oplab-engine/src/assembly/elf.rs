//! Validate standard ELF geometry without projecting it into a custom section model.

use object::{Object, ObjectSection, ObjectSegment, SectionFlags};
use oplab_core::{
    address::{Address, AddressRange},
    protocol::{Diagnostic, DiagnosticCode, MAX_ALLOCATED_BYTES, MAX_OBJECT_BYTES},
    target::Target,
};

pub(super) fn validate(
    bytes: &[u8],
    target: Target,
    kind: object::ObjectKind,
) -> Result<object::File<'_>, Diagnostic> {
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err(Diagnostic::new(DiagnosticCode::ResourceLimit));
    }
    let file =
        object::File::parse(bytes).map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
    let architecture = match target {
        Target::X86_64 => object::Architecture::X86_64,
        Target::Aarch64 => object::Architecture::Aarch64,
    };
    if file.format() != object::BinaryFormat::Elf
        || !file.is_64()
        || !file.is_little_endian()
        || file.architecture() != architecture
        || file.kind() != kind
    {
        return Err(Diagnostic::new(DiagnosticCode::BackendFailure));
    }
    let mut allocated = 0_u64;
    let mut ranges = Vec::new();
    for section in file.sections() {
        // Inspect raw data without decompressing source-provided debug sections.
        section
            .data()
            .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
        let alignment = section.align().max(1);
        if !alignment.is_power_of_two() {
            return Err(Diagnostic::new(DiagnosticCode::BackendFailure));
        }
        if let SectionFlags::Elf { sh_flags, .. } = section.flags()
            && sh_flags.contains(object::elf::SHF_ALLOC)
            && section.size() != 0
        {
            allocated = allocated
                .checked_add(section.size())
                .filter(|size| *size <= MAX_ALLOCATED_BYTES as u64)
                .ok_or_else(|| Diagnostic::new(DiagnosticCode::ResourceLimit))?;
            if kind == object::ObjectKind::Executable {
                if !section.address().is_multiple_of(alignment) {
                    return Err(Diagnostic::new(DiagnosticCode::InvalidInput));
                }
                let range = AddressRange::new(
                    Address::new(section.address()),
                    section.size(),
                    MAX_ALLOCATED_BYTES as u64,
                )
                .map_err(|_| Diagnostic::new(DiagnosticCode::InvalidInput))?;
                // TLS zero-fill is instantiated per thread, not ordinary PT_LOAD
                // storage, and its section addresses may overlap other sections.
                if section.kind() != object::SectionKind::UninitializedTls {
                    ranges.push(range);
                }
            }
        }
    }
    ranges.sort_by_key(|range| range.start());
    if ranges
        .windows(2)
        .any(|pair| u128::from(pair[1].start().get()) < pair[0].end())
    {
        return Err(Diagnostic::new(DiagnosticCode::InvalidInput));
    }
    if kind == object::ObjectKind::Executable {
        validate_segments(&file)?;
    }
    Ok(file)
}

fn validate_segments(file: &object::File<'_>) -> Result<(), Diagnostic> {
    // PT_LOAD p_memsz includes zero-fill and segment padding, unlike section bytes.
    // Bound it independently while leaving actual page mapping to the loader.
    let mut total = 0_u64;
    let mut previous_end = 0_u128;
    for segment in file.segments() {
        let (offset, file_size) = segment.file_range();
        let alignment = segment.align().max(1);
        if !alignment.is_power_of_two()
            || file_size > segment.size()
            || offset % alignment != segment.address() % alignment
            || u128::from(segment.address()) < previous_end
        {
            return Err(Diagnostic::new(DiagnosticCode::InvalidInput));
        }
        segment
            .data()
            .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
        total = total
            .checked_add(segment.size())
            .filter(|size| *size <= MAX_OBJECT_BYTES as u64)
            .ok_or_else(|| Diagnostic::new(DiagnosticCode::ResourceLimit))?;
        previous_end = u128::from(segment.address()) + u128::from(segment.size());
        if previous_end > 1_u128 << 64 {
            return Err(Diagnostic::new(DiagnosticCode::InvalidInput));
        }
    }
    Ok(())
}
