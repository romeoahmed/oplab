//! Keep native register identifiers inside the architecture adapter.

use super::hooks::Monitor;
use super::{Machine, MachineError};
use oplab_core::{
    address::Address,
    diagnostic::ValidationError,
    registers::{InitialRegisters, IntegerRegisters, RegisterEdit, RegisterStorage},
    target::Target,
};
use unicorn_engine::{RegisterARM64, RegisterX86, Unicorn};

impl Machine {
    pub(crate) fn write_register(&mut self, edit: RegisterEdit) -> Result<(), MachineError> {
        if edit.storage() == RegisterStorage::InstructionPointer {
            self.native
                .set_pc(edit.apply(0))
                .map_err(|_| MachineError::Backend)?;
            let monitor = self.native.get_data_mut();
            monitor.pending_repeat = None;
            monitor.bypass = None;
            return Ok(());
        }
        let register: i32 = match (self.initial.target(), edit.storage()) {
            (Target::X86_64, RegisterStorage::Gpr(index)) => {
                X86_GPR.get(index).copied().map(Into::into)
            }
            (Target::Aarch64, RegisterStorage::Gpr(31)) => Some(RegisterARM64::SP.into()),
            (Target::Aarch64, RegisterStorage::Gpr(index)) => {
                AARCH64_GPR.get(index).copied().map(Into::into)
            }
            (Target::X86_64, RegisterStorage::Flags) => Some(RegisterX86::RFLAGS.into()),
            (Target::Aarch64, RegisterStorage::Flags) => Some(RegisterARM64::NZCV.into()),
            (_, RegisterStorage::InstructionPointer) => None,
        }
        .ok_or(ValidationError::Target)?;
        // Debugger APIs need not implement instruction operand-width semantics.
        // Merge into canonical storage so every backend observes the same policy.
        let previous = self
            .native
            .reg_read(register)
            .map_err(|_| MachineError::Backend)?;
        self.native
            .reg_write(register, edit.apply(previous))
            .map_err(|_| MachineError::Backend)
    }

    /// Read canonical integer storage at one execution boundary.
    ///
    /// # Errors
    ///
    /// Returns a backend failure if any register cannot be observed consistently.
    pub fn read_registers(&self) -> Result<IntegerRegisters, MachineError> {
        let pc = Address::new(self.native.pc_read().map_err(|_| MachineError::Backend)?);
        match self.initial.target() {
            Target::X86_64 => {
                use RegisterX86 as R;
                let gpr = self.read_bank(X86_GPR)?;
                let rflags = self
                    .native
                    .reg_read(R::RFLAGS)
                    .map_err(|_| MachineError::Backend)?;
                Ok(IntegerRegisters::X86_64 {
                    gpr,
                    rip: pc,
                    rflags,
                })
            }
            Target::Aarch64 => {
                use RegisterARM64 as R;
                let x = self.read_bank(AARCH64_GPR)?;
                let sp = self
                    .native
                    .reg_read(R::SP)
                    .map_err(|_| MachineError::Backend)?;
                let nzcv = self
                    .native
                    .reg_read(R::NZCV)
                    .map_err(|_| MachineError::Backend)?;
                let nzcv = u32::try_from(nzcv).map_err(|_| MachineError::Backend)?;
                Ok(IntegerRegisters::Aarch64 { x, sp, pc, nzcv })
            }
        }
    }

    fn read_bank<R: Into<i32>, const N: usize>(
        &self,
        registers: [R; N],
    ) -> Result<[u64; N], MachineError> {
        let mut values = [0; N];
        for (value, register) in values.iter_mut().zip(registers) {
            *value = self
                .native
                .reg_read(register)
                .map_err(|_| MachineError::Backend)?;
        }
        Ok(values)
    }
}

const X86_GPR: [RegisterX86; 16] = [
    RegisterX86::RAX,
    RegisterX86::RCX,
    RegisterX86::RDX,
    RegisterX86::RBX,
    RegisterX86::RSP,
    RegisterX86::RBP,
    RegisterX86::RSI,
    RegisterX86::RDI,
    RegisterX86::R8,
    RegisterX86::R9,
    RegisterX86::R10,
    RegisterX86::R11,
    RegisterX86::R12,
    RegisterX86::R13,
    RegisterX86::R14,
    RegisterX86::R15,
];
const AARCH64_GPR: [RegisterARM64; 31] = [
    RegisterARM64::X0,
    RegisterARM64::X1,
    RegisterARM64::X2,
    RegisterARM64::X3,
    RegisterARM64::X4,
    RegisterARM64::X5,
    RegisterARM64::X6,
    RegisterARM64::X7,
    RegisterARM64::X8,
    RegisterARM64::X9,
    RegisterARM64::X10,
    RegisterARM64::X11,
    RegisterARM64::X12,
    RegisterARM64::X13,
    RegisterARM64::X14,
    RegisterARM64::X15,
    RegisterARM64::X16,
    RegisterARM64::X17,
    RegisterARM64::X18,
    RegisterARM64::X19,
    RegisterARM64::X20,
    RegisterARM64::X21,
    RegisterARM64::X22,
    RegisterARM64::X23,
    RegisterARM64::X24,
    RegisterARM64::X25,
    RegisterARM64::X26,
    RegisterARM64::X27,
    RegisterARM64::X28,
    RegisterARM64::X29,
    RegisterARM64::X30,
];

pub(super) fn initialize(
    native: &mut Unicorn<'_, Monitor>,
    values: &InitialRegisters,
) -> Result<(), MachineError> {
    match values {
        InitialRegisters::X86_64(values) => write_bank(native, X86_GPR, values),
        InitialRegisters::Aarch64(values) => {
            let [gpr @ .., sp] = values;
            write_bank(native, AARCH64_GPR, gpr)?;
            native
                .reg_write(RegisterARM64::SP, *sp)
                .map_err(|_| MachineError::Backend)
        }
    }
}

fn write_bank<R: Into<i32>, const N: usize>(
    native: &mut Unicorn<'_, Monitor>,
    registers: [R; N],
    values: &[u64; N],
) -> Result<(), MachineError> {
    for (register, value) in registers.into_iter().zip(values) {
        native
            .reg_write(register, *value)
            .map_err(|_| MachineError::Backend)?;
    }
    Ok(())
}
