//! CXX bridge carrying owned bytes and call-scoped borrows, never LLVM/LLD handles.

// Apply expectations outside CXX's attribute parser, only to this bridge module.
#![expect(
    unsafe_code,
    unused_qualifications,
    unreachable_pub,
    reason = "CXX verifies signatures and generates qualified shared types; each call owns its MC state"
)]
#[cxx::bridge(namespace = "oplab")]
pub(super) mod bridge {
    /// Guest ISA; independent of the host compiler target.
    enum Architecture {
        X86_64,
        Aarch64,
    }

    /// Assembly outcomes independent of LLVM diagnostic wording.
    enum Status {
        Success,
        Assembly,
        ResourceLimit,
    }

    /// Bounded ELF output or a diagnostic with an optional original-source byte offset.
    struct ObjectResult {
        object: Vec<u8>,
        status: Status,
        source_offset: u32,
        has_source_offset: bool,
    }

    /// Paths are owned by the calling Rust frame and borrowed for one LLD call.
    struct LinkRequest<'a> {
        input: &'a str,
        output: &'a str,
        script: &'a str,
        base: u64,
    }

    // SAFETY: C++ retains no input borrows; returned bytes own their storage.
    // LLVM handles remain call-local, and LLD's process-wide context is serialized.
    unsafe extern "C++" {
        include!("oplab-toolchain/native/assembly.hpp");
        fn assemble_object(source: &str, architecture: Architecture) -> Result<ObjectResult>;
        fn link_object(request: &LinkRequest) -> Result<bool>;
    }
}
