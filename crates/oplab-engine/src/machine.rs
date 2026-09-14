//! Native machine ownership. Construct and use a machine on its execution thread.

use crate::load::{LoadError, LoadPlan};
use oplab_core::{
    address::{Address, AddressRange},
    diagnostic::ValidationError,
    memory::Permissions,
    target::Target,
};
use unicorn_engine::{
    Unicorn,
    unicorn_const::{Arch, Mode, Prot},
};

mod hooks;
mod registers;
mod run;
use hooks::Monitor;
pub(crate) use run::{SLICE_DISPATCHES, Slice, SliceStop};

#[cfg(test)]
#[path = "../tests/unit/machine.rs"]
mod tests;

/// Maximum bytes in one debugger memory observation.
pub const MAX_READ_BYTES: u64 = 64 * 1024;

/// Machine construction or observation failure without native logs or host paths.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MachineError {
    /// The input does not describe a supported initial image.
    #[error(transparent)]
    Load(#[from] LoadError),
    /// The requested observation violates a domain constraint.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// The native backend rejected an operation after input validation.
    #[error("native machine operation failed")]
    Backend,
}

/// One native address space and its validated initial image. This owner is neither
/// cloneable nor transferable between threads; dropping it releases native memory.
pub struct Machine {
    native: Unicorn<'static, Monitor>,
    initial: LoadPlan,
    exits: Vec<u64>,
}

impl Machine {
    /// Create a machine using the backend's actual page size and exact ELF permissions.
    /// Host initialization bypasses guest writes without widening the mapped access.
    /// The image does not implicitly provide a stack, OS services, or a completion address.
    ///
    /// # Errors
    /// Rejects invalid images and native setup failures. Partial native allocations
    /// are released before returning an error.
    pub fn from_elf(image: &[u8], target: Target) -> Result<Self, MachineError> {
        let native = open(target)?;
        let page_size = native
            .ctl_get_page_size()
            .map_err(|_| MachineError::Backend)?;
        let initial = LoadPlan::from_elf(image, target, u64::from(page_size))?;
        let native = initialize(native, &initial)?;
        Ok(Self {
            native,
            initial,
            exits: Vec::new(),
        })
    }

    pub(crate) fn reset(&mut self) -> Result<(), MachineError> {
        let mut replacement = initialize(open(self.initial.target())?, &self.initial)?;
        replacement.get_data_mut().breakpoints =
            std::mem::take(&mut self.native.get_data_mut().breakpoints);
        self.native = replacement;
        self.exits.clear();
        Ok(())
    }

    pub(crate) fn set_breakpoint(
        &mut self,
        address: Address,
        enabled: bool,
    ) -> Result<(), MachineError> {
        if !address
            .get()
            .is_multiple_of(self.initial.target().instruction_alignment())
        {
            return Err(ValidationError::Target.into());
        }
        let breakpoints = &mut self.native.get_data_mut().breakpoints;
        if enabled {
            if breakpoints.len() >= 256 && !breakpoints.contains(&address) {
                return Err(ValidationError::Length.into());
            }
            breakpoints.insert(address);
        } else {
            breakpoints.remove(&address);
        }
        Ok(())
    }

    /// Immutable initial contents and permissions, retained for reset planning.
    #[must_use]
    pub const fn initial(&self) -> &LoadPlan {
        &self.initial
    }

    /// Observe a bounded mapped range without granting guest read permissions.
    /// Debugger observations may inspect execute-only or guard pages.
    ///
    /// # Errors
    /// Rejects empty, oversized, unmapped, or cross-region ranges and native failures.
    pub fn read_memory(&self, address: Address, length: u64) -> Result<Vec<u8>, MachineError> {
        let range = AddressRange::new(address, length, MAX_READ_BYTES)?;
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
            .mem_read_as_vec(address.get(), length)
            .map_err(|_| MachineError::Backend)
    }
}

fn open(target: Target) -> Result<Unicorn<'static, Monitor>, MachineError> {
    let (architecture, mode) = match target {
        Target::X86_64 => (Arch::X86, Mode::MODE_64),
        Target::Aarch64 => (Arch::ARM64, Mode::LITTLE_ENDIAN),
    };
    Unicorn::new_with_data(architecture, mode, Monitor::new(target))
        .map_err(|_| MachineError::Backend)
}

fn initialize(
    mut native: Unicorn<'static, Monitor>,
    initial: &LoadPlan,
) -> Result<Unicorn<'static, Monitor>, MachineError> {
    let page_size = native
        .ctl_get_page_size()
        .map_err(|_| MachineError::Backend)?;
    if u64::from(page_size) != initial.page_size() {
        return Err(MachineError::Backend);
    }
    for region in initial.memory().regions() {
        native
            .mem_map(
                region.range().start().get(),
                region.range().length(),
                protection(region.permissions()),
            )
            .map_err(|_| MachineError::Backend)?;
        if !region.initial().is_empty() {
            native
                .mem_write(region.range().start().get(), region.initial())
                .map_err(|_| MachineError::Backend)?;
        }
    }
    native
        .set_pc(initial.entry().get())
        .map_err(|_| MachineError::Backend)?;
    hooks::install(&mut native).map_err(|_| MachineError::Backend)?;
    Ok(native)
}

fn protection(permissions: Permissions) -> Prot {
    let mut protection = Prot::NONE;
    for (enabled, flag) in [
        (permissions.read, Prot::READ),
        (permissions.write, Prot::WRITE),
        (permissions.execute, Prot::EXEC),
    ] {
        if enabled {
            protection |= flag;
        }
    }
    protection
}
