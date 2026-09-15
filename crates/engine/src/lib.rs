//! Assembly, decoding and isolated guest execution for `x86_64` and `AArch64`.
//!
//! [`assembly`] compiles and links standard ELF; [`load`] validates its mappings.
//! [`session::Session`] owns a machine on its execution thread. Desktop callers use
//! the framed [`worker`] process so native failures can be supervised.

pub mod assembly;
pub mod decode;
pub mod load;
pub mod machine;
pub mod session;
pub mod worker;
