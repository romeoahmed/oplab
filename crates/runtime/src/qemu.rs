//! Load the private SDK and own CPU/block handles; QEMU layouts and exceptions stay native.
#![expect(
    unsafe_code,
    reason = "the SDK validates buffers and confines guest exceptions to C frames"
)]

use crate::Error;
use oplab_core::target::Target;
use std::{ffi::CStr, marker::PhantomData, path::PathBuf, ptr::NonNull, rc::Rc, sync::OnceLock};

#[expect(
    unreachable_pub,
    clippy::undocumented_unsafe_blocks,
    reason = "bindgen emits public ABI declarations and zero-initialization for C aggregates"
)]
pub(crate) mod abi {
    include!(concat!(env!("OUT_DIR"), "/qemu.rs"));
}

struct Backend {
    _library: libloading::Library,
    api: abi::OplabQemu,
}
// SAFETY: the API table is immutable and every CPU operation takes the SDK's lock.
unsafe impl Send for Backend {}
// SAFETY: QEMU state is never accessed through the table without its native lock.
unsafe impl Sync for Backend {}

impl Backend {
    fn load(target: Target) -> Result<Self, Error> {
        let guest = match target {
            Target::X86_64 => "x86_64",
            Target::Aarch64 => "aarch64",
        };
        let directory = std::env::var_os("OPLAB_QEMU_DIR")
            .map_or_else(
                || {
                    std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(PathBuf::from))
                },
                |value| Some(PathBuf::from(value)),
            )
            .ok_or_else(|| Error::Sdk("worker directory is unavailable".into()))?;
        let path = directory.join(libloading::library_filename(format!("oplab-qemu-{guest}")));
        // SAFETY: only the explicitly selected native SDK is loaded. Its table is
        // copied while the library is retained for the process lifetime below.
        unsafe {
            let library = libloading::Library::new(path).map_err(|e| Error::Sdk(e.to_string()))?;
            let get = library
                .get::<unsafe extern "C" fn() -> *const abi::OplabQemu>(b"oplab_qemu\0")
                .map_err(|e| Error::Sdk(e.to_string()))?;
            let api = get().as_ref().ok_or(Error::Native)?;
            if api.abi != 1 || CStr::from_ptr(api.version).to_bytes() != b"11.1.1" {
                return Err(Error::Sdk("SDK revision does not match the adapter".into()));
            }
            Ok(Self {
                api: *api,
                _library: library,
            })
        }
    }
}

pub(crate) struct Cpu {
    backend: &'static Backend,
    handle: NonNull<abi::OplabCpu>,
    _owner: PhantomData<Rc<()>>,
}

impl Cpu {
    pub(crate) fn new(target: Target) -> Result<Self, Error> {
        static X86: OnceLock<Result<Backend, String>> = OnceLock::new();
        static ARM: OnceLock<Result<Backend, String>> = OnceLock::new();
        let slot = match target {
            Target::X86_64 => &X86,
            Target::Aarch64 => &ARM,
        };
        // SDKs remain loaded: QEMU registers process hooks and thread destructors.
        let backend = slot
            .get_or_init(|| Backend::load(target).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| Error::Sdk(e.clone()))?;
        // SAFETY: the loaded table has the generated ABI and returns an owned handle.
        let handle = unsafe { backend.api.create.ok_or(Error::Native)?() };
        Ok(Self {
            backend,
            handle: NonNull::new(handle).ok_or(Error::Native)?,
            _owner: PhantomData,
        })
    }
    pub(crate) fn map(
        &mut self,
        base: u64,
        size: u64,
        permissions: u32,
        bytes: &[u8],
    ) -> Result<(), Error> {
        // SAFETY: bytes remains borrowed for the call; the SDK copies into owned RAM.
        let ok = unsafe {
            self.backend.api.map.ok_or(Error::Native)?(
                self.handle.as_ptr(),
                base,
                size,
                permissions,
                bytes.as_ptr(),
                bytes.len(),
            )
        };
        ok.then_some(()).ok_or(Error::Native)
    }
    pub(crate) fn read(&self, address: u64, size: usize) -> Result<Vec<u8>, Error> {
        if size == 0 || size > 65536 {
            return Err(Error::Native);
        }
        let mut bytes = vec![0; size];
        // SAFETY: the destination has exactly size writable bytes and does not escape.
        let ok = unsafe {
            self.backend.api.read.ok_or(Error::Native)?(
                self.handle.as_ptr(),
                address,
                bytes.as_mut_ptr(),
                size,
            )
        };
        ok.then_some(bytes).ok_or(Error::Native)
    }
    pub(crate) fn write(&mut self, address: u64, bytes: &[u8]) -> Result<(), Error> {
        // SAFETY: the SDK copies this call-scoped immutable slice after validating its mapping.
        let ok = unsafe {
            self.backend.api.write.ok_or(Error::Native)?(
                self.handle.as_ptr(),
                address,
                bytes.as_ptr(),
                bytes.len(),
            )
        };
        ok.then_some(()).ok_or(Error::Native)
    }
    pub(crate) fn register(
        &self,
        bank: abi::OplabRegister,
        index: u32,
        bytes: &mut [u8],
    ) -> Result<(), Error> {
        if !(1..=256).contains(&bytes.len()) {
            return Err(Error::Native);
        }
        // SAFETY: the SDK validates the register width before writing this slice.
        let ok = unsafe {
            self.backend.api.register_read.ok_or(Error::Native)?(
                self.handle.as_ptr(),
                bank,
                index,
                bytes.as_mut_ptr(),
                bytes.len(),
            )
        };
        ok.then_some(()).ok_or(Error::Native)
    }
    pub(crate) fn set_register(
        &mut self,
        bank: abi::OplabRegister,
        index: u32,
        bytes: &[u8],
    ) -> Result<(), Error> {
        // SAFETY: the register width and index are validated natively; no borrow escapes.
        let ok = unsafe {
            self.backend.api.register_write.ok_or(Error::Native)?(
                self.handle.as_ptr(),
                bank,
                index,
                bytes.as_ptr(),
                bytes.len(),
            )
        };
        ok.then_some(()).ok_or(Error::Native)
    }
    pub(crate) fn translate(&mut self) -> Result<(Block, abi::OplabExit), Error> {
        let mut raw = abi::OplabBlock::default();
        // SAFETY: the SDK initializes every field and transfers ownership of both arrays.
        let exit = unsafe {
            self.backend.api.translate.ok_or(Error::Native)?(self.handle.as_ptr(), &raw mut raw)
        };
        Ok((
            Block {
                raw,
                backend: self.backend,
            },
            exit,
        ))
    }
    pub(crate) fn execute(&mut self, block: &Block, entry: usize) -> Result<abi::OplabExit, Error> {
        // SAFETY: entry is compiled for this SDK's CPU layout and retained by the caller.
        // C establishes its jump boundary before calling it; no Rust frame is skipped.
        Ok(unsafe {
            self.backend.api.execute.ok_or(Error::Native)?(
                self.handle.as_ptr(),
                &raw const block.raw,
                entry,
            )
        })
    }
    pub(crate) const fn memory_helpers(&self) -> [usize; 4] {
        let api = &self.backend.api;
        [api.load, api.store, api.load_pair, api.store_pair]
    }
}
impl Drop for Cpu {
    fn drop(&mut self) {
        if let Some(destroy) = self.backend.api.destroy {
            // SAFETY: this is the sole owner and the SDK remains loaded for the process.
            unsafe {
                destroy(self.handle.as_ptr());
            }
        }
    }
}

pub(crate) struct Block {
    pub(crate) raw: abi::OplabBlock,
    backend: &'static Backend,
}
impl Block {
    pub(crate) const fn temps(&self) -> &[abi::OplabTemp] {
        if self.raw.temp_count == 0 {
            return &[];
        }
        // SAFETY: these arrays were allocated by translate and live until release.
        unsafe { std::slice::from_raw_parts(self.raw.temps, self.raw.temp_count) }
    }
    pub(crate) const fn ops(&self) -> &[abi::OplabOp] {
        if self.raw.op_count == 0 {
            return &[];
        }
        // SAFETY: translate exports complete initialized records owned by this block.
        unsafe { std::slice::from_raw_parts(self.raw.ops, self.raw.op_count) }
    }
}
impl Drop for Block {
    fn drop(&mut self) {
        if let Some(release) = self.backend.api.release {
            // SAFETY: this is the sole owner; release also clears the native descriptor.
            unsafe {
                release(&raw mut self.raw);
            }
        }
    }
}
