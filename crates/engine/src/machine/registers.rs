//! Apply architectural aliases to canonical storage before native writes.
use super::{Machine, MachineError};
use oplab_core::registers::{
    InitialRegisters, MachineRegisters, RegisterEdit, RegisterStorage, RoundingMode, VectorBits,
    VectorEdit, VectorStorage,
};
use oplab_core::target::Target;
use oplab_runtime::{Register, Runtime};

pub(super) fn read<const N: usize>(
    native: &Runtime,
    register: Register,
) -> Result<[u8; N], MachineError> {
    native.register(register).map_err(|_| MachineError::Backend)
}
fn gpr(index: usize) -> Result<Register, MachineError> {
    Ok(Register::Gpr(
        u32::try_from(index).map_err(|_| MachineError::Backend)?,
    ))
}
impl Machine {
    pub(crate) fn write_register(&mut self, edit: RegisterEdit) -> Result<(), MachineError> {
        let register = match edit.storage() {
            RegisterStorage::Flags => Register::Flags,
            RegisterStorage::InstructionPointer => Register::Pc,
            RegisterStorage::Gpr(index) => gpr(index)?,
        };
        let value = edit.apply(u64::from_le_bytes(read(&self.native, register)?));
        self.native
            .set_register(register, &value.to_le_bytes())
            .map_err(|_| MachineError::Backend)?;
        if edit.storage() == RegisterStorage::InstructionPointer {
            self.pending_repeat = None;
            self.bypass = None;
        }
        Ok(())
    }
    pub(crate) fn write_vector(
        &mut self,
        name: &str,
        width: u16,
        lane: u16,
        value: VectorBits,
    ) -> Result<(), MachineError> {
        let (vl, maximum) = self
            .native
            .vector_lengths()
            .map_err(|_| MachineError::Backend)?;
        let edit = VectorEdit::new(
            self.initial.target(),
            usize::from(vl),
            name,
            width,
            lane,
            value,
        )?;
        let (register, size) = match edit.storage() {
            VectorStorage::Vector(index) => {
                (Register::Vector(u32::from(index)), usize::from(maximum))
            }
            VectorStorage::Predicate(index) => (
                Register::Predicate(u32::from(index)),
                usize::from(maximum) / 8,
            ),
        };
        let mut bytes = self
            .native
            .register_bytes(register, size)
            .map_err(|_| MachineError::Backend)?;
        edit.apply(&mut bytes)?;
        self.native
            .set_register(register, &bytes)
            .map_err(|_| MachineError::Backend)
    }
    pub(crate) fn set_rounding(&mut self, mode: RoundingMode) -> Result<(), MachineError> {
        let control = u32::from_le_bytes(read(&self.native, Register::Control)?);
        self.native
            .set_register(
                Register::Control,
                &mode.apply(self.initial.target(), control).to_le_bytes(),
            )
            .map_err(|_| MachineError::Backend)
    }
    fn vectors<const N: usize>(
        &self,
        size: usize,
        register: fn(u32) -> Register,
    ) -> Result<Box<[VectorBits; N]>, MachineError> {
        let values = (0..N)
            .map(|index| {
                let index = u32::try_from(index).map_err(|_| MachineError::Backend)?;
                let bytes = self
                    .native
                    .register_bytes(register(index), size)
                    .map_err(|_| MachineError::Backend)?;
                VectorBits::new(bytes).map_err(|_| MachineError::Backend)
            })
            .collect::<Result<Vec<_>, _>>()?;
        values
            .into_boxed_slice()
            .try_into()
            .map_err(|_| MachineError::Backend)
    }
    fn bank<const N: usize>(&self) -> Result<[u64; N], MachineError> {
        let mut values = [0; N];
        for (index, value) in values.iter_mut().enumerate() {
            *value = u64::from_le_bytes(read(&self.native, gpr(index)?)?);
        }
        Ok(values)
    }
    /// Read canonical integer, SIMD and floating-point state at one execution boundary.
    ///
    /// # Errors
    ///
    /// Returns a backend failure if any native register is unavailable.
    pub fn read_registers(&self) -> Result<MachineRegisters, MachineError> {
        let pc = self.pc()?;
        let flags = u64::from_le_bytes(read(&self.native, Register::Flags)?);
        let control = u32::from_le_bytes(read(&self.native, Register::Control)?);
        let (vl, max_vl) = self
            .native
            .vector_lengths()
            .map_err(|_| MachineError::Backend)?;
        match self.initial.target() {
            Target::X86_64 => Ok(MachineRegisters::X86_64 {
                gpr: self.bank()?,
                rip: pc,
                rflags: flags,
                ymm: self.vectors(32, Register::Vector)?,
                mxcsr: control,
            }),
            Target::Aarch64 => Ok(MachineRegisters::Aarch64 {
                x: self.bank()?,
                sp: u64::from_le_bytes(read(&self.native, Register::Gpr(31))?),
                pc,
                nzcv: u32::try_from(flags).map_err(|_| MachineError::Backend)?,
                z: self.vectors(usize::from(max_vl), Register::Vector)?,
                p: self.vectors(usize::from(max_vl) / 8, Register::Predicate)?,
                ffr: VectorBits::new(
                    self.native
                        .register_bytes(Register::Predicate(16), usize::from(max_vl) / 8)
                        .map_err(|_| MachineError::Backend)?,
                )
                .map_err(|_| MachineError::Backend)?,
                vl,
                max_vl,
                fpcr: control,
                fpsr: u32::from_le_bytes(read(&self.native, Register::Status)?),
            }),
        }
    }
}
pub(super) fn initialize(
    native: &mut Runtime,
    values: &InitialRegisters,
) -> Result<(), MachineError> {
    let values = match values {
        InitialRegisters::X86_64(values) => values.as_slice(),
        InitialRegisters::Aarch64(values) => values.as_slice(),
    };
    for (index, value) in values.iter().enumerate() {
        native
            .set_register(gpr(index)?, &value.to_le_bytes())
            .map_err(|_| MachineError::Backend)?;
    }
    Ok(())
}
