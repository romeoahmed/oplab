//! LLD owns final section placement and relocation; host code never executes guest bytes.

use super::{ObjectArtifact, elf, ffi};
use object::{Object, ObjectSection};
use oplab_core::{
    address::{Address, AddressRange},
    protocol::{Diagnostic, DiagnosticCode, MAX_ALLOCATED_BYTES, MAX_OBJECT_BYTES},
};
use std::{fs, io::Read, path::Path};

pub(super) fn image(object: &ObjectArtifact, base: Address) -> Result<Vec<u8>, Diagnostic> {
    let parsed = object::File::parse(object.bytes())
        .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
    let text = parsed
        .section_by_name(".text")
        .ok_or_else(|| Diagnostic::new(DiagnosticCode::Assembly))?;
    if text.size() != 0
        && !base
            .get()
            .is_multiple_of(text.align().max(object.target.instruction_alignment()))
    {
        return Err(Diagnostic::new(DiagnosticCode::InvalidInput));
    }
    if text.size() != 0 {
        AddressRange::new(base, text.size(), MAX_ALLOCATED_BYTES as u64)
            .map_err(|_| Diagnostic::new(DiagnosticCode::InvalidInput))?;
    }
    let scratch =
        tempfile::tempdir().map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
    let input = scratch.path().join("input.o");
    let output = scratch.path().join("linked.elf");
    let script = scratch.path().join("layout.ld");
    fs::write(&input, object.bytes())
        .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
    // Anchor .text and separate following data at the target's maximum page size.
    // LLD places orphan sections using their standard ELF attributes; no source
    // section, symbol, note, or debugging data is discarded here.
    fs::write(
        &script,
        format!(
            "SECTIONS {{ .text 0x{:x} : {{ *(.text) }} . = ALIGN(CONSTANT(MAXPAGESIZE)); }}",
            base.get()
        ),
    )
    .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
    if !ffi::bridge::link_object(path(&input)?, path(&output)?, path(&script)?, base.get())
        .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?
    {
        return Err(Diagnostic::new(DiagnosticCode::Assembly));
    }
    let mut bytes = Vec::new();
    fs::File::open(output)
        .and_then(|file| {
            file.take((MAX_OBJECT_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
    let image = elf::validate(&bytes, object.target, object::ObjectKind::Executable)?;
    if image.section_by_name(".text").map_or_else(
        || text.size() != 0,
        |section| section.address() != base.get(),
    ) {
        return Err(Diagnostic::new(DiagnosticCode::BackendFailure));
    }
    Ok(bytes)
}

fn path(path: &Path) -> Result<&str, Diagnostic> {
    path.to_str()
        .ok_or_else(|| Diagnostic::new(DiagnosticCode::BackendFailure))
}
