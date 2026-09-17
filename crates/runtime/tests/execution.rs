//! Native execution boundaries shared by both supported guests.
use oplab_core::{
    address::Address,
    execution::{Access, FaultKind},
    target::Target,
};
use oplab_runtime::{Outcome, Register, Runtime};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn machine(
    target: Target,
    code: &[u8],
) -> std::result::Result<Runtime, Box<dyn std::error::Error>> {
    let mut cpu = Runtime::new(target)?;
    cpu.map(0x1000, 4096, 5, code)?;
    cpu.set_register(Register::Pc, &0x1000u64.to_le_bytes())?;
    Ok(cpu)
}
#[test]
fn both_guests_execute_and_recreate() -> Result {
    for (target, code) in [
        (Target::X86_64, &[0xb8, 42, 0, 0, 0][..]),
        (Target::Aarch64, &[0x40, 5, 0x80, 0xd2][..]),
    ] {
        for _ in 0..64 {
            let mut cpu = machine(target, code)?;
            let step = cpu.execute(Address::new(0x1000), 1)?;
            assert!(matches!(step.outcome, Outcome::Finished));
            assert_eq!(cpu.register(Register::Gpr(0))?, 42u64.to_le_bytes());
            assert_eq!(
                cpu.register(Register::Pc)?,
                (0x1000 + code.len() as u64).to_le_bytes()
            );
        }
    }
    Ok(())
}
#[test]
fn guest_fetch_permissions_are_independent_of_debugger_access() -> Result {
    let mut cpu = Runtime::new(Target::X86_64)?;
    cpu.map(0x1000, 4096, 1, &[0x90])?;
    cpu.set_register(Register::Pc, &0x1000u64.to_le_bytes())?;
    assert_eq!(cpu.read(0x1000, 1)?, [0x90]);
    let step = cpu.execute(Address::new(0x1000), 1)?;
    let Outcome::Fault(fault) = step.outcome else {
        panic!("expected a fetch fault")
    };
    assert_eq!(fault.kind, FaultKind::Protection(Access::Fetch));
    assert!(!step.started);
    Ok(())
}
#[test]
fn rep_obeys_dispatch_budget_and_can_resume() -> Result {
    let mut cpu = machine(Target::X86_64, &[0xf3, 0xaa])?;
    cpu.map(0x2000, 4096, 3, &[])?;
    cpu.set_register(Register::Gpr(7), &0x2000u64.to_le_bytes())?;
    cpu.set_register(Register::Gpr(1), &20u64.to_le_bytes())?;
    cpu.set_register(Register::Gpr(0), &42u64.to_le_bytes())?;
    let step = cpu.execute(Address::new(0x1000), 4)?;
    assert!(matches!(step.outcome, Outcome::Yield));
    assert_eq!(step.dispatches, 4);
    assert_eq!(cpu.read(0x2000, 5)?, [42, 42, 42, 42, 0]);
    let step = cpu.execute(Address::new(0x1000), 32)?;
    assert!(matches!(step.outcome, Outcome::Finished));
    assert_eq!(cpu.read(0x2000, 20)?, [42; 20]);
    Ok(())
}

#[test]
fn guest_code_writes_change_previously_executed_instructions() -> Result {
    let mut cpu = Runtime::new(Target::X86_64)?;
    // MOV EAX,1; MOV byte ptr [RIP-11],42 changes the first immediate.
    cpu.map(
        0x1000,
        4096,
        7,
        &[0xb8, 1, 0, 0, 0, 0xc6, 0x05, 0xf5, 0xff, 0xff, 0xff, 42],
    )?;
    cpu.set_register(Register::Pc, &0x1000u64.to_le_bytes())?;
    for pc in [0x1000, 0x1005] {
        assert!(matches!(
            cpu.execute(Address::new(pc), 1)?.outcome,
            Outcome::Finished
        ));
    }
    cpu.set_register(Register::Pc, &0x1000u64.to_le_bytes())?;
    assert!(matches!(
        cpu.execute(Address::new(0x1000), 1)?.outcome,
        Outcome::Finished
    ));
    assert_eq!(cpu.register(Register::Gpr(0))?, 42u64.to_le_bytes());
    Ok(())
}

#[test]
fn repeated_byte_reversal_preserves_all_64_bits() -> Result {
    // BSWAP RAX and REV X0,X0 are independent encodings of byte reversal.
    for (target, code) in [
        (Target::X86_64, &[0x48, 0x0f, 0xc8][..]),
        (Target::Aarch64, &[0x00, 0x0c, 0xc0, 0xda][..]),
    ] {
        let mut cpu = machine(target, code)?;
        for value in [0, u64::MAX, 0x0123_4567_89ab_cdef, 0x8000_0000_0000_0001] {
            cpu.set_register(Register::Pc, &0x1000u64.to_le_bytes())?;
            cpu.set_register(Register::Gpr(0), &value.to_le_bytes())?;
            assert!(matches!(
                cpu.execute(Address::new(0x1000), 1)?.outcome,
                Outcome::Finished
            ));
            assert_eq!(cpu.register(Register::Gpr(0))?, value.to_be_bytes());
        }
    }
    Ok(())
}
