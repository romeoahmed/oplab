//! Native machine ownership. Construct and use a machine on its execution thread.
use crate::load::{Image, LoadError, LoadPlan, MachineSetup};
use oplab_core::{
    address::{Address, AddressRange},
    diagnostic::ValidationError,
    memory::Permissions,
    target::Target,
};
use oplab_runtime::{Register, Runtime};
use std::collections::BTreeSet;
mod registers;
mod run;
pub(crate) use run::{SLICE_DISPATCHES, Slice, SliceStop};

/// Maximum bytes in one debugger memory read or write.
pub const MAX_MEMORY_BYTES: u64 = 64 * 1024;
/// Machine operation failure without native logs or host paths.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MachineError {
    /// The input does not describe a supported initial image.
    #[error(transparent)]
    Load(#[from] LoadError),
    /// The requested operation violates a domain constraint.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// The native backend rejected an operation after input validation.
    #[error("native machine operation failed")]
    Backend,
}
/// One thread-owned native address space and its retained initial image.
pub struct Machine {
    native: Runtime,
    initial: LoadPlan,
    breakpoints: BTreeSet<Address>,
    pending_repeat: Option<Address>,
    bypass: Option<Address>,
}
impl Machine {
    /// Validate and load static ELF with exact guest permissions and default initial state.
    ///
    /// # Errors
    ///
    /// Rejects invalid images and native setup failures.
    pub fn from_elf(image: &[u8], target: Target) -> Result<Self, MachineError> {
        Self::load(Image::Elf(image), target, MachineSetup::default())
    }
    /// Load ELF or raw code with explicit initial registers and additional mappings.
    ///
    /// # Errors
    ///
    /// Rejects invalid setup before allocation; partial owners are discarded on failure.
    pub fn load(
        image: Image<'_>,
        target: Target,
        setup: MachineSetup,
    ) -> Result<Self, MachineError> {
        let initial = LoadPlan::new(image, target, 4096, setup)?;
        let native = initialize(&initial)?;
        Ok(Self {
            native,
            initial,
            breakpoints: BTreeSet::new(),
            pending_repeat: None,
            bypass: None,
        })
    }
    pub(crate) fn reset(&mut self) -> Result<(), MachineError> {
        self.native = initialize(&self.initial)?;
        self.pending_repeat = None;
        self.bypass = None;
        Ok(())
    }
    pub(crate) fn breakpoints(&self) -> impl Iterator<Item = Address> + '_ {
        self.breakpoints.iter().copied()
    }
    pub(crate) fn set_breakpoints(
        &mut self,
        addresses: &[Address],
        enabled: bool,
    ) -> Result<(), MachineError> {
        if addresses.len() > 256 {
            return Err(ValidationError::Length.into());
        }
        if addresses.iter().any(|address| {
            !address
                .get()
                .is_multiple_of(self.initial.target().instruction_alignment())
        }) {
            return Err(ValidationError::Target.into());
        }
        let mut next = self.breakpoints.clone();
        for address in addresses {
            if enabled {
                next.insert(*address);
            } else {
                next.remove(address);
            }
        }
        if next.len() > 256 {
            return Err(ValidationError::Length.into());
        }
        self.breakpoints = next;
        Ok(())
    }
    /// Initial memory, registers and permissions retained for reset.
    #[must_use]
    pub const fn initial(&self) -> &LoadPlan {
        &self.initial
    }
    pub(crate) fn write_memory(
        &mut self,
        address: Address,
        bytes: &[u8],
    ) -> Result<(), MachineError> {
        let length = u64::try_from(bytes.len()).map_err(|_| ValidationError::Length)?;
        let range = AddressRange::new(address, length, MAX_MEMORY_BYTES)?;
        let region = self.initial.memory().require(
            range,
            Permissions {
                read: false,
                write: false,
                execute: false,
            },
        )?;
        let executable = region.permissions().execute;
        self.native
            .write(address.get(), bytes)
            .map_err(|_| MachineError::Backend)?;
        if executable {
            self.pending_repeat = None;
        }
        Ok(())
    }
    /// Observe a bounded mapped range without granting guest read permissions.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, unmapped or cross-region ranges and native failures.
    pub fn read_memory(&self, address: Address, length: u64) -> Result<Vec<u8>, MachineError> {
        let range = AddressRange::new(address, length, MAX_MEMORY_BYTES)?;
        self.initial.memory().require(
            range,
            Permissions {
                read: false,
                write: false,
                execute: false,
            },
        )?;
        let length = usize::try_from(length).map_err(|_| ValidationError::Length)?;
        self.native
            .read(address.get(), length)
            .map_err(|_| MachineError::Backend)
    }
    fn pc(&self) -> Result<Address, MachineError> {
        registers::read(&self.native, Register::Pc)
            .map(u64::from_le_bytes)
            .map(Address::new)
    }
}
fn initialize(initial: &LoadPlan) -> Result<Runtime, MachineError> {
    let mut native = Runtime::new(initial.target()).map_err(|_| MachineError::Backend)?;
    for region in initial.memory().regions() {
        let permissions = region.permissions();
        let mask = u8::from(permissions.read)
            | (u8::from(permissions.write) << 1)
            | (u8::from(permissions.execute) << 2);
        native
            .map(
                region.range().start().get(),
                region.range().length(),
                mask,
                region.initial(),
            )
            .map_err(|_| MachineError::Backend)?;
    }
    native
        .set_register(Register::Pc, &initial.entry().get().to_le_bytes())
        .map_err(|_| MachineError::Backend)?;
    if let Some(values) = initial.registers() {
        registers::initialize(&mut native, values)?;
    }
    Ok(native)
}
