//! Standard ELF views used by the workbench without reparsing binary formats in JavaScript.

use object::{Object, ObjectSegment, ObjectSymbol, SegmentFlags, SymbolKind};
use oplab_core::{
    address::Address,
    protocol::{
        Diagnostic, DiagnosticCode,
        image::{ImageInfo, ImageSegment, ImageSymbol},
        scalar::{Counter, HexAddress},
    },
    target::Target,
};

/// Inspect a linked image for bounded segment and symbol views.
///
/// # Errors
///
/// Rejects invalid ELF and metadata exceeding the wire limits.
pub fn describe(bytes: &[u8], target: Target) -> Result<ImageInfo, Diagnostic> {
    let file = super::elf::validate(bytes, target, object::ObjectKind::Executable)?;
    let segments = file
        .segments()
        .map(|segment| {
            let (offset, length) = segment.file_range();
            let SegmentFlags::Elf { p_flags, .. } = segment.flags() else {
                return Err(Diagnostic::new(DiagnosticCode::BackendFailure));
            };
            Ok(ImageSegment {
                address: HexAddress::new(Address::new(segment.address())),
                file_offset: u32::try_from(offset)
                    .map_err(|_| Diagnostic::new(DiagnosticCode::ResourceLimit))?,
                file_bytes: u32::try_from(length)
                    .map_err(|_| Diagnostic::new(DiagnosticCode::ResourceLimit))?,
                memory_bytes: Counter::new(segment.size()),
                flags: p_flags.0,
            })
        })
        .collect::<Result<Vec<_>, Diagnostic>>()?;
    let mut symbols = Vec::new();
    let mut symbols_truncated = false;
    let mut names = 0;
    for symbol in file.symbols() {
        // TLS values are thread-relative offsets, not linked virtual addresses.
        if symbol.is_undefined()
            || matches!(
                symbol.kind(),
                SymbolKind::File | SymbolKind::Section | SymbolKind::Tls
            )
        {
            continue;
        }
        let name = symbol
            .name()
            .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
        if name.is_empty() {
            continue;
        }
        if symbols.len() >= 4096 || names + name.len() > 64 * 1024 {
            symbols_truncated = true;
            continue;
        }
        names += name.len();
        symbols.push(ImageSymbol {
            name: name.into(),
            address: HexAddress::new(Address::new(symbol.address())),
            size: Counter::new(symbol.size()),
        });
    }
    Ok(ImageInfo {
        entry: HexAddress::new(Address::new(file.entry())),
        segments,
        symbols,
        symbols_truncated,
    })
}
