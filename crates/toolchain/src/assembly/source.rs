//! Project linked source locations into bounded editor metadata.

use object::{Object, ObjectSegment, SegmentFlags};
use oplab_core::{
    address::Address,
    protocol::{
        Diagnostic, DiagnosticCode,
        image::{SourceLocation, SourceMap},
        scalar::HexAddress,
    },
};

pub(super) fn describe(image: &[u8], source: &str) -> Result<SourceMap, Diagnostic> {
    let file =
        object::File::parse(image).map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
    let points = super::ffi::bridge::source_points(image, source)
        .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
    let executable = file
        .segments()
        .filter(|segment| {
            matches!(segment.flags(), SegmentFlags::Elf {p_flags, ..}
        if p_flags.contains(object::elf::PF_X))
        })
        .map(|segment| (segment.address(), segment.file_range().1))
        .collect::<Vec<_>>();
    // LLVM SourceMgr counts LF; CodeMirror also splits bare CR. A raw LF line
    // containing an interior CR has no unambiguous editor location in DWARF.
    let lines = source
        .split('\n')
        .scan(1_usize, |editor_line, line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let extra = line.bytes().filter(|byte| *byte == b'\r').count();
            let mapped = (extra == 0).then_some(*editor_line);
            *editor_line += extra + 1;
            Some(mapped)
        })
        .collect::<Vec<_>>();
    Ok(SourceMap {
        locations: points
            .locations
            .into_iter()
            .filter(|point| {
                executable.iter().any(|&(start, size)| {
                    point
                        .address
                        .checked_sub(start)
                        .is_some_and(|offset| offset < size)
                })
            })
            .filter_map(|point| {
                let line = lines
                    .get(usize::try_from(point.line).ok()?.checked_sub(1)?)?
                    .as_ref()?;
                Some(SourceLocation {
                    address: HexAddress::new(Address::new(point.address)),
                    line: u32::try_from(*line).ok()?,
                })
            })
            .collect(),
        truncated: points.truncated,
    })
}
