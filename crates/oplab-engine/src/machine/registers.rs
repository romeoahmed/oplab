//! Keep native register identifiers inside the architecture adapter.

use super::{Machine, MachineError};
use oplab_core::{address::Address, registers::IntegerRegisters, target::Target};
use unicorn_engine::{RegisterARM64, RegisterX86};

impl Machine {
    /// Read canonical integer storage at one execution boundary.
    ///
    /// # Errors
    /// Returns a backend failure if any register cannot be observed consistently.
    pub fn read_registers(&self) -> Result<IntegerRegisters, MachineError> {
        let pc = Address::new(self.native.pc_read().map_err(|_| MachineError::Backend)?);
        match self.initial.target() {
            Target::X86_64 => {
                use RegisterX86 as R;
                let gpr = self.read_bank([
                    R::RAX,
                    R::RCX,
                    R::RDX,
                    R::RBX,
                    R::RSP,
                    R::RBP,
                    R::RSI,
                    R::RDI,
                    R::R8,
                    R::R9,
                    R::R10,
                    R::R11,
                    R::R12,
                    R::R13,
                    R::R14,
                    R::R15,
                ])?;
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
                let x = self.read_bank([
                    R::X0,
                    R::X1,
                    R::X2,
                    R::X3,
                    R::X4,
                    R::X5,
                    R::X6,
                    R::X7,
                    R::X8,
                    R::X9,
                    R::X10,
                    R::X11,
                    R::X12,
                    R::X13,
                    R::X14,
                    R::X15,
                    R::X16,
                    R::X17,
                    R::X18,
                    R::X19,
                    R::X20,
                    R::X21,
                    R::X22,
                    R::X23,
                    R::X24,
                    R::X25,
                    R::X26,
                    R::X27,
                    R::X28,
                    R::X29,
                    R::X30,
                ])?;
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
