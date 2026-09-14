//! Native semantic probes remain bounded even when the guest faults.

use super::Machine;
use crate::assembly;
use oplab_core::{address::Address, target::Target};
use unicorn_engine::{RegisterARM64, RegisterX86, unicorn_const::uc_error};

#[test]
fn guest_stores_cannot_modify_initialized_rx_pages() -> Result<(), Box<dyn std::error::Error>> {
    for (target, source) in [
        (Target::X86_64, "mov byte ptr [rip - 7], 42"),
        (Target::Aarch64, "str w0, [x1]"),
    ] {
        let object = assembly::compile(target, source).map_err(|error| format!("{error:?}"))?;
        let bytes =
            assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
        let mut machine = Machine::from_elf(&bytes, target)?;
        let mapped = machine.native.mem_regions()?;
        let code = mapped
            .iter()
            .find(|region| region.begin <= 0x1000 && region.end >= 0x1000)
            .ok_or("missing code mapping")?;
        assert_eq!(code.perms, 5); // Unicorn R | X, with no guest write access.
        let pc: i32 = match target {
            Target::X86_64 => RegisterX86::RIP.into(),
            Target::Aarch64 => RegisterARM64::PC.into(),
        };
        assert_eq!(machine.native.reg_read(pc)?, 0x1000);
        if target == Target::Aarch64 {
            machine.native.reg_write(RegisterARM64::X0, 42)?;
            machine.native.reg_write(RegisterARM64::X1, 0x1000)?;
        }
        let before = machine.read_memory(Address::new(0x1000), 8)?;
        assert_eq!(
            machine.native.emu_start(0x1000, 0x2000, 100_000, 1),
            Err(uc_error::WRITE_PROT)
        );
        assert_eq!(machine.read_memory(Address::new(0x1000), 8)?, before);
    }
    Ok(())
}
