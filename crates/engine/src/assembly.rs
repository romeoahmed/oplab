//! Complete LLVM translation units and linked ELF images, without flattening sections.

mod elf;
mod ffi;
pub(crate) mod image;
mod link;

use oplab_core::{
    protocol::{AssemblerIdentity, BuildIdentity, Diagnostic, DiagnosticCode, MAX_SOURCE_BYTES},
    target::Target,
};

/// A validated relocatable ELF object. Construction is limited to the MC frontend.
#[derive(Debug)]
pub struct ObjectArtifact {
    target: Target,
    bytes: Vec<u8>,
}

impl ObjectArtifact {
    /// Complete ELF bytes, including symbols, relocations, and debugging sections.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Build identity and complete relocatable/executable ELF files.
#[derive(Debug)]
pub struct BuildArtifact {
    /// Document revision and assembly settings captured for this build.
    pub identity: BuildIdentity,
    /// Original relocatable ELF, before final address assignment.
    pub object: Vec<u8>,
    /// Executable ELF describing file-backed bytes and zero-filled load regions.
    pub image: Vec<u8>,
}

/// Assembler name and exact LLVM release used by this process.
#[must_use]
pub fn identity() -> AssemblerIdentity {
    AssemblerIdentity {
        name: "llvm-mc".into(),
        version: env!("OPLAB_LLVM_VERSION").into(),
    }
}

/// Compile one complete translation unit independently of its eventual load address.
///
/// Undefined symbols and standard relocations remain in the resulting ELF object.
///
/// # Errors
///
/// Rejects empty or oversized source, MC warnings/errors, native failures and invalid output.
pub fn compile(target: Target, source: &str) -> Result<ObjectArtifact, Diagnostic> {
    validate_source(source)?;
    let bytes = assemble_object(source, target)?;
    elf::validate(&bytes, target, object::ObjectKind::Relocatable)?;
    Ok(ObjectArtifact { target, bytes })
}

/// Link an object at the exact requested `.text` address without modifying the object.
///
/// The object can be linked repeatedly; loading is a separate operation.
///
/// # Examples
///
/// ```
/// use oplab_core::{address::Address, target::Target};
/// use oplab_engine::assembly;
///
/// let object = assembly::compile(Target::Aarch64, ".text\nmov x0, #42\n")?;
/// let first = assembly::link(&object, Address::new(0x1000))?;
/// let relocated = assembly::link(&object, Address::new(0x2000))?;
/// assert_ne!(first, relocated);
/// # Ok::<(), oplab_core::protocol::Diagnostic>(())
/// ```
///
/// # Errors
///
/// Rejects conflicting alignment, undefined symbols, failed relocations, address
/// overflow and output limits. Native or temporary-file failures return a backend diagnostic.
pub fn link(
    object: &ObjectArtifact,
    base: oplab_core::address::Address,
) -> Result<Vec<u8>, Diagnostic> {
    link::image(object, base)
}

/// Build one source revision by composing object generation and final ELF linking.
///
/// # Errors
///
/// Rejects stale backend identity, invalid settings, compilation or linking errors.
pub fn assemble(build: BuildIdentity, source: &str) -> Result<BuildArtifact, Diagnostic> {
    assemble_cancellable(build, source, || false)
}

/// Check cancellation between native stages; never unwind or interrupt an active
/// LLVM/LLD call. The scheduler separately prevents publication after cancellation.
pub(crate) fn assemble_cancellable(
    build: BuildIdentity,
    source: &str,
    cancelled: impl Fn() -> bool,
) -> Result<BuildArtifact, Diagnostic> {
    validate_build(&build, source)?;
    let checkpoint = || {
        if cancelled() {
            Err(Diagnostic::new(DiagnosticCode::Cancelled))
        } else {
            Ok(())
        }
    };
    checkpoint()?;
    let object = compile(build.target, source)?;
    checkpoint()?;
    let image = link(&object, build.base.address())?;
    checkpoint()?;
    Ok(BuildArtifact {
        identity: build,
        object: object.bytes,
        image,
    })
}

/// Reject invalid queued work before retaining it or superseding an accepted build.
pub(crate) fn validate_build(build: &BuildIdentity, source: &str) -> Result<(), Diagnostic> {
    validate_identity(build)?;
    validate_source(source)
}

const fn validate_source(source: &str) -> Result<(), Diagnostic> {
    if source.is_empty() {
        return Err(Diagnostic::new(DiagnosticCode::InvalidInput));
    }
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Diagnostic::new(DiagnosticCode::ResourceLimit));
    }
    Ok(())
}

fn assemble_object(source: &str, target: Target) -> Result<Vec<u8>, Diagnostic> {
    let architecture = match target {
        Target::X86_64 => ffi::bridge::Architecture::X86_64,
        Target::Aarch64 => ffi::bridge::Architecture::Aarch64,
    };
    let result = ffi::bridge::assemble_object(source, architecture)
        .map_err(|_| Diagnostic::new(DiagnosticCode::BackendFailure))?;
    match result.status {
        ffi::bridge::Status::Success => Ok(result.object),
        ffi::bridge::Status::Assembly => {
            let mut diagnostic = Diagnostic::new(DiagnosticCode::Assembly);
            if result.has_source_offset && source.get(..result.source_offset as usize).is_some() {
                diagnostic.source_offset = Some(result.source_offset);
            }
            Err(diagnostic)
        }
        ffi::bridge::Status::ResourceLimit => Err(Diagnostic::new(DiagnosticCode::ResourceLimit)),
        _ => Err(Diagnostic::new(DiagnosticCode::BackendFailure)),
    }
}

fn validate_identity(build: &BuildIdentity) -> Result<(), Diagnostic> {
    if build.assembler != identity() {
        return Err(Diagnostic::new(DiagnosticCode::BackendMismatch));
    }
    if build.document.is_empty()
        || build.document.len() > 64
        || !build
            .document
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(Diagnostic::new(DiagnosticCode::InvalidInput));
    }
    Ok(())
}
