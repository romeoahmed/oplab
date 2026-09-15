//! The only C++ trust boundary; native objects never cross it.

// Apply expectations outside CXX's attribute parser, only to this bridge module.
#![expect(
    unsafe_code,
    unused_qualifications,
    unreachable_pub,
    reason = "CXX verifies signatures and generates qualified shared types; each call owns its MC state"
)]
#[cxx::bridge(namespace = "oplab")]
pub(super) mod bridge {
    /// Native outcomes are independent of localized LLVM diagnostic strings.
    enum Status {
        Success,
        Assembly,
        ResourceLimit,
        BackendFailure,
    }

    /// Bounded object output and a stable error category, without LLVM messages.
    struct ObjectResult {
        object: Vec<u8>,
        status: Status,
        source_offset: u32,
        has_source_offset: bool,
    }

    // SAFETY: Inputs are borrowed only for the call; output owns its bytes.
    // The bridge retains no Rust pointers and serializes LLD's global context.
    unsafe extern "C++" {
        include!("assembly.hpp");
        fn assemble_object(source: &str, aarch64: bool) -> Result<ObjectResult>;
        fn link_object(input: &str, output: &str, script: &str, base: u64) -> Result<bool>;
    }
}
