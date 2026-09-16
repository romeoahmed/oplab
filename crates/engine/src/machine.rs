//! Native machine ownership. Construct and use a machine on its execution thread.

use crate::load::{Image, LoadError, LoadPlan, MachineSetup};
use oplab_core::{
    address::{Address, AddressRange},
    diagnostic::ValidationError,
    memory::Permissions,
    target::{CpuModel, Target},
};
use unicorn_engine::{
    Arm64CpuModel, Unicorn, X86CpuModel,
    unicorn_const::{Arch, Mode, Prot},
};

mod hooks;
mod registers;
mod run;
use hooks::Monitor;
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

/// One native address space and its validated initial image.
///
/// Construct and use it on the same thread. It is neither cloneable nor transferable;
/// dropping it releases native memory.
pub struct Machine {
    native: Unicorn<'static, Monitor>,
    initial: LoadPlan,
    exits: Vec<u64>,
}

impl Machine {
    /// Create a machine using the backend's actual page size and exact ELF permissions.
    ///
    /// Host initialization writes memory without granting guest write permission.
    /// The image does not implicitly provide a stack, OS services, or a completion address.
    ///
    /// # Errors
    ///
    /// Rejects invalid images and native setup failures. Partial native allocations
    /// are released before returning an error.
    pub fn from_elf(image: &[u8], target: Target) -> Result<Self, MachineError> {
        Self::load(Image::Elf(image), target, MachineSetup::default())
    }

    /// Load ELF or raw code with explicit initial registers and additional mappings.
    ///
    /// # Errors
    ///
    /// Rejects invalid initial conditions before mapping guest memory. A failed native
    /// setup drops the entire new machine; no partially configured state escapes.
    pub fn load(
        image: Image<'_>,
        target: Target,
        setup: MachineSetup,
    ) -> Result<Self, MachineError> {
        let cpu = setup.cpu.unwrap_or_else(|| CpuModel::default_for(target));
        if cpu.target() != target {
            return Err(ValidationError::Target.into());
        }
        let native = open(cpu)?;
        let page_size = native
            .ctl_get_page_size()
            .map_err(|_| MachineError::Backend)?;
        let initial = LoadPlan::new(image, target, u64::from(page_size), setup)?;
        let native = initialize(native, &initial)?;
        Ok(Self {
            native,
            initial,
            exits: Vec::new(),
        })
    }

    pub(crate) fn reset(&mut self) -> Result<(), MachineError> {
        let mut replacement = initialize(open(self.initial.cpu())?, &self.initial)?;
        replacement.get_data_mut().breakpoints =
            std::mem::take(&mut self.native.get_data_mut().breakpoints);
        self.native = replacement;
        self.exits.clear();
        Ok(())
    }

    pub(crate) fn breakpoints(&self) -> impl Iterator<Item = Address> + '_ {
        self.native.get_data().breakpoints.iter().copied()
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

    /// Immutable initial memory, registers and permissions retained for reset.
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
            .mem_write(address.get(), bytes)
            .map_err(|_| MachineError::Backend)?;
        if executable {
            // Unicorn's range invalidation takes an exclusive u64 end. The last
            // address-space byte instead requires a full translation-cache flush.
            let result = match u64::try_from(range.end()) {
                Ok(end) => self.native.ctl_remove_cache(address.get(), end),
                Err(_) => self.native.ctl_flush_tb(),
            };
            result.map_err(|_| MachineError::Backend)?;
        }
        Ok(())
    }

    /// Observe a bounded mapped range without granting guest read permissions.
    ///
    /// Debugger observations may inspect execute-only or guard pages.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, unmapped, or cross-region ranges and native failures.
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
            .mem_read_as_vec(address.get(), length)
            .map_err(|_| MachineError::Backend)
    }
}

fn open(cpu: CpuModel) -> Result<Unicorn<'static, Monitor>, MachineError> {
    let (architecture, mode) = match cpu.target() {
        Target::X86_64 => (Arch::X86, Mode::MODE_64),
        Target::Aarch64 => (Arch::ARM64, Mode::LITTLE_ENDIAN),
    };
    let mut native = Unicorn::new_with_data(architecture, mode, Monitor::new(cpu.target()))
        .map_err(|_| MachineError::Backend)?;
    let model = match cpu {
        CpuModel::Nehalem => X86CpuModel::NEHALEM as i32,
        CpuModel::Haswell => X86CpuModel::HASWELL as i32,
        CpuModel::CortexA53 => Arm64CpuModel::A53 as i32,
        CpuModel::CortexA72 => Arm64CpuModel::A72 as i32,
    };
    // Set before the first query/register/memory operation initializes Unicorn's CPU.
    native
        .ctl_set_cpu_model(model)
        .map_err(|_| MachineError::Backend)?;
    Ok(native)
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
    registers::configure_simd(&mut native, initial.target())?;
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
    if let Some(values) = initial.registers() {
        registers::initialize(&mut native, values)?;
    }
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
