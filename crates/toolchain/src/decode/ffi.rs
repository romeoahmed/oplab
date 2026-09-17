//! Owned decoder results; native handles and numeric enum values stay behind CXX.
#![expect(
    unsafe_code,
    unused_qualifications,
    unreachable_pub,
    reason = "CXX validates shared layouts and call-scoped borrows"
)]
#[cxx::bridge(namespace = "oplab::decode")]
pub(super) mod bridge {
    enum Architecture {
        X86_64,
        Aarch64,
    }
    enum Access {
        Unknown,
        Read,
        ConditionalRead,
        Write,
        ConditionalWrite,
        ReadWrite,
        ReadConditionalWrite,
        ConditionalReadWrite,
    }
    enum Flow {
        Next,
        Branch,
        IndirectBranch,
        ConditionalBranch,
        Call,
        IndirectCall,
        Return,
        Interrupt,
        Transaction,
        Exception,
    }
    struct Register {
        name: String,
        access: Access,
    }
    struct Memory {
        access: Access,
        bytes: u32,
    }
    struct Flags {
        read: Vec<String>,
        written: Vec<String>,
        cleared: Vec<String>,
        set: Vec<String>,
        undefined: Vec<String>,
    }
    struct Instruction {
        size: u32,
        text: String,
        registers: Vec<Register>,
        memory: Vec<Memory>,
        flags: Flags,
        isa: String,
        groups: Vec<String>,
        flow: Flow,
        branch: u64,
        has_branch: bool,
        registers_incomplete: bool,
        privileged: bool,
        updates_flags: bool,
        writeback: bool,
    }
    struct Window {
        instructions: Vec<Instruction>,
        consumed: usize,
    }
    // SAFETY: inputs are borrowed for the call; results own all strings and lists.
    unsafe extern "C++" {
        include!("oplab-toolchain/native/decode.hpp");
        fn decode_window(
            bytes: &[u8],
            base: u64,
            architecture: Architecture,
            limit: usize,
            details: bool,
        ) -> Result<Window>;
    }
}
