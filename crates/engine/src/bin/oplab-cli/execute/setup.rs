//! Human input is parsed once, then validated by the shared machine loader.

use oplab_core::{
    address::{Address, AddressRange},
    memory::{MAX_MAPPED_BYTES, Permissions},
    protocol::{Diagnostic, DiagnosticCode},
    registers::InitialRegisters,
    target::{CpuModel, Target},
};
use oplab_engine::load::{InitialMapping, MachineSetup};
use std::str::FromStr;

#[derive(clap::Args)]
pub(crate) struct Setup {
    /// Emulator profile. Defaults to haswell (`x86_64`) or cortex-a72 (`AArch64`).
    #[arg(long, value_enum)]
    cpu: Option<Cpu>,
    /// Initial canonical GPR or stack pointer; repeat for distinct registers.
    ///
    /// Lowercase names, decimal or 0x-prefixed unsigned 64-bit values. Unspecified
    /// GPRs are zero. Aliases, PC, flags and duplicate registers are rejected.
    #[arg(long = "register", value_name = "NAME=VALUE")]
    registers: Vec<Assignment>,
    /// Additional zero-filled, page-aligned memory; repeat for disjoint regions.
    ///
    /// Size accepts decimal or 0x hexadecimal. Permissions: r, w, x in that order,
    /// or - for a guard region. Cannot overlap image pages. Total memory <= 64 MiB.
    #[arg(long = "map", value_name = "ADDRESS:SIZE:PERMISSIONS", value_parser = mapping)]
    mappings: Vec<InitialMapping>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Cpu {
    Nehalem,
    Haswell,
    CortexA53,
    CortexA72,
}

impl From<Cpu> for CpuModel {
    fn from(cpu: Cpu) -> Self {
        match cpu {
            Cpu::Nehalem => Self::Nehalem,
            Cpu::Haswell => Self::Haswell,
            Cpu::CortexA53 => Self::CortexA53,
            Cpu::CortexA72 => Self::CortexA72,
        }
    }
}

#[derive(Clone)]
struct Assignment {
    name: String,
    value: u64,
}

impl FromStr for Assignment {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (name, value) = value.split_once('=').ok_or("expected NAME=VALUE")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        {
            return Err("expected a lowercase canonical register name");
        }
        Ok(Self {
            name: name.into(),
            value: number(value)?,
        })
    }
}

fn mapping(value: &str) -> Result<InitialMapping, &'static str> {
    let mut parts = value.split(':');
    let address: Address = parts
        .next()
        .ok_or("expected ADDRESS:SIZE:PERMISSIONS")?
        .parse()
        .map_err(|_| "expected a hexadecimal address")?;
    let length = number(parts.next().ok_or("missing mapping size")?)?;
    let mode = parts.next().ok_or("missing permissions")?;
    if parts.next().is_some() || !matches!(mode, "-" | "r" | "w" | "x" | "rw" | "rx" | "wx" | "rwx")
    {
        return Err("expected permissions r, w, x in order, or -");
    }
    let range = AddressRange::new(address, length, MAX_MAPPED_BYTES)
        .map_err(|_| "mapping must be nonempty, nonwrapping and at most 64 MiB")?;
    Ok(InitialMapping {
        range,
        permissions: Permissions {
            read: mode.contains('r'),
            write: mode.contains('w'),
            execute: mode.contains('x'),
        },
        bytes: Vec::new(),
    })
}

fn number(value: &str) -> Result<u64, &'static str> {
    let (digits, radix) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map_or((value, 10), |digits| (digits, 16));
    if digits.starts_with('+') {
        return Err("expected an unsigned decimal or 0x hexadecimal integer without a sign");
    }
    u64::from_str_radix(digits, radix).map_err(|_| "expected an unsigned 64-bit integer")
}

impl Setup {
    pub(super) fn build(self, target: Target) -> Result<MachineSetup, Diagnostic> {
        let registers = InitialRegisters::from_assignments(
            target,
            self.registers
                .iter()
                .map(|assignment| (assignment.name.as_str(), assignment.value)),
        )
        .map_err(|_| Diagnostic::new(DiagnosticCode::InvalidInput))?;
        Ok(MachineSetup {
            cpu: self.cpu.map(Into::into),
            registers: Some(registers),
            mappings: self.mappings,
        })
    }
}
