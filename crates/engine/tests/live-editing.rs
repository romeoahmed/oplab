//! Live writes must affect real guest execution without changing reset state or permissions.

use oplab_core::{
    address::Address,
    execution::{Access, ExecutionState, FaultKind, PauseReason, Termination},
    registers::MachineRegisters,
    target::Target,
};
use oplab_engine::{
    assembly,
    load::{Image, MachineSetup},
    machine::{Machine, MachineError},
    session::Session,
};
use proptest::prelude::*;
use std::fmt::Write;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn session(target: Target, code: &[u8]) -> Result<Session, MachineError> {
    Session::new(
        Machine::load(
            Image::Raw {
                bytes: code,
                base: Address::new(0x1000),
                entry: Address::new(0x1000),
            },
            target,
            MachineSetup::default(),
        )?,
        Address::new(0x1800),
        100,
    )
}

const fn integer(registers: &MachineRegisters) -> u64 {
    match registers {
        MachineRegisters::X86_64 { gpr, .. } => gpr[0],
        MachineRegisters::Aarch64 { x, .. } => x[0],
    }
}

fn step(session: &mut Session) -> TestResult {
    session.step()?;
    settle(session)
}

fn settle(session: &mut Session) -> TestResult {
    for _ in 0..1024 {
        if session.state() != ExecutionState::Running {
            return Ok(());
        }
        session.advance()?;
    }
    Err("session did not settle".into())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]
    #[test]
    fn gpr_writes_preserve_other_registers_and_reset(values in any::<[u64; 32]>()) {
        // Independent architectural names; never derive the oracle from the backend table.
        let x86 = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
            "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"].map(String::from);
        let arm: Vec<_> = (0..31).map(|index| format!("x{index}"))
            .chain(std::iter::once("sp".into())).collect();
        for (target, names) in [(Target::X86_64, x86.as_slice()), (Target::Aarch64, arm.as_slice())] {
            let mut machine = session(target, &[0; 16])?;
            let initial = machine.read_registers()?;
            let mut expected = initial.clone();
            for (index, (name, value)) in names.iter().zip(values).enumerate() {
                match &mut expected {
                    MachineRegisters::X86_64 { gpr, .. } => gpr[index] = value,
                    MachineRegisters::Aarch64 { x, sp, .. } => {
                        if index == 31 { *sp = value; } else { x[index] = value; }
                    }
                }
                machine.write_register(name, value)?;
                prop_assert_eq!(&machine.read_registers()?, &expected);
            }
            prop_assert_eq!(machine.instructions(), 0);
            prop_assert_eq!(machine.state(), ExecutionState::Ready);
            machine.reset()?;
            prop_assert_eq!(machine.read_registers()?, initial);
        }
    }

    #[test]
    fn memory_patches_preserve_surroundings_and_reset(
        original in prop::collection::vec(any::<u8>(), 256),
        (offset, bytes) in (0_usize..256).prop_flat_map(|offset|
            (Just(offset), prop::collection::vec(any::<u8>(), 1..=256 - offset))),
    ) {
        let mut expected = original.clone();
        expected[offset..offset + bytes.len()].copy_from_slice(&bytes);
        for target in [Target::X86_64, Target::Aarch64] {
            let mut machine = session(target, &original)?;
            let registers = machine.read_registers()?;
            machine.write_memory(Address::new(0x1000 + u64::try_from(offset)?), &bytes)?;
            prop_assert_eq!(&machine.read_memory(Address::new(0x1000), 256)?, &expected);
            prop_assert_eq!(machine.read_registers()?, registers);
            prop_assert_eq!(machine.state(), ExecutionState::Ready);
            machine.reset()?;
            prop_assert_eq!(&machine.read_memory(Address::new(0x1000), 256)?, &original);
        }
    }
}

#[test]
fn patching_previously_executed_code_invalidates_translations_on_both_guests() -> TestResult {
    for (target, code, replacement) in [
        // ADD RAX,1; JMP back / ADD X0,X0,#1; B back.
        (
            Target::X86_64,
            vec![0x48, 0x83, 0xc0, 1, 0xeb, 0xfa],
            vec![0x48, 0x83, 0xc0, 2],
        ),
        (
            Target::Aarch64,
            vec![0x00, 0x04, 0x00, 0x91, 0xff, 0xff, 0xff, 0x17],
            vec![0x00, 0x08, 0x00, 0x91],
        ),
    ] {
        let mut machine = session(target, &code)?;
        step(&mut machine)?;
        assert_eq!(integer(&machine.read_registers()?), 1);
        step(&mut machine)?;
        assert_eq!(
            machine.read_registers()?.instruction_pointer(),
            Address::new(0x1000)
        );
        let name = if target == Target::X86_64 {
            "rax"
        } else {
            "x0"
        };
        machine.write_register(name, 40)?;
        machine.write_memory(Address::new(0x1000), &replacement)?;
        machine.set_breakpoint(Address::new(0x1004), true)?;
        machine.start()?;
        settle(&mut machine)?;
        assert_eq!(
            machine.state(),
            ExecutionState::Paused(PauseReason::Breakpoint(Address::new(0x1004)))
        );
        assert_eq!(integer(&machine.read_registers()?), 42);
        assert_eq!(machine.instructions(), 3);
        machine.reset()?;
        assert_eq!(
            machine.read_memory(Address::new(0x1000), u64::try_from(code.len())?)?,
            code
        );
        assert_eq!(
            machine.breakpoints().collect::<Vec<_>>(),
            vec![Address::new(0x1004)]
        );
        step(&mut machine)?;
        assert_eq!(integer(&machine.read_registers()?), 1);
    }
    Ok(())
}

#[test]
fn rejected_writes_preserve_state_and_host_patches_do_not_grant_guest_write_access() -> TestResult {
    for (target, code, canonical, invalid_names) in [
        // Store the accumulator into its RX code page.
        (
            Target::X86_64,
            vec![0x48, 0x89, 0x04, 0x25, 0x00, 0x10, 0x00, 0x00],
            "rax",
            [
                ("x0", 42),
                ("rflags", 42),
                ("al", 256),
                ("eax", 1 << 32),
                ("zf", 2),
            ],
        ),
        (
            Target::Aarch64,
            vec![0x20, 0x00, 0x00, 0xf9],
            "x0",
            [
                ("x31", 42),
                ("nzcv", 42),
                ("w0", 1 << 32),
                ("pc", 0x1001),
                ("z", 2),
            ],
        ),
    ] {
        let mut machine = session(target, &code)?;
        let initial = machine.read_registers()?;
        for (name, value) in invalid_names {
            assert!(machine.write_register(name, value).is_err());
            assert_eq!(machine.read_registers()?, initial, "{name}={value}");
            assert_eq!(machine.state(), ExecutionState::Ready);
            assert_eq!(machine.instructions(), 0);
        }
        for (address, bytes) in [
            (0x1000, vec![]),
            (0x1fff, vec![0, 0]),
            (0x2000, vec![0]),
            (u64::MAX, vec![0, 0]),
            (0x1000, vec![0; 65537]),
        ] {
            assert!(machine.write_memory(Address::new(address), &bytes).is_err());
        }
        assert_eq!(
            machine.read_memory(Address::new(0x1000), u64::try_from(code.len())?)?,
            code
        );
        machine.write_memory(Address::new(0x1100), &[42])?;
        if target == Target::Aarch64 {
            machine.write_register("x1", 0x1000)?;
        }
        machine.start()?;
        assert!(machine.write_register(canonical, 42).is_err());
        assert!(machine.write_memory(Address::new(0x1000), &[0]).is_err());
        settle(&mut machine)?;
        assert_eq!(
            machine.state(),
            ExecutionState::Terminated(Termination::GuestFault)
        );
        assert_eq!(
            machine.fault().map(|fault| fault.kind),
            Some(FaultKind::Protection(Access::Write))
        );
        assert!(machine.write_register(canonical, 42).is_err());
        assert!(machine.write_memory(Address::new(0x1000), &[0]).is_err());
    }
    Ok(())
}

#[test]
fn editing_a_paused_rep_continuation_preserves_instruction_accounting() -> TestResult {
    use oplab_core::{address::AddressRange, memory::Permissions, registers::InitialRegisters};
    use oplab_engine::load::InitialMapping;
    for (restart, reverse) in [(false, false), (false, true), (true, false), (true, true)] {
        let machine = Machine::load(
            Image::Raw {
                bytes: &[0xf3, 0xaa, 0x90],
                base: Address::new(0x1000),
                entry: Address::new(0x1000),
            },
            Target::X86_64,
            MachineSetup {
                cpu: None,
                registers: Some(InitialRegisters::from_assignments(
                    Target::X86_64,
                    [("rax", 1), ("rcx", 5000), ("rdi", 0x8000)],
                )?),
                mappings: vec![InitialMapping {
                    range: AddressRange::new(Address::new(0x8000), 8192, 8192)?,
                    permissions: Permissions {
                        read: true,
                        write: true,
                        execute: false,
                    },
                    bytes: vec![],
                }],
            },
        )?;
        let mut machine = Session::new(machine, Address::new(0x1002), 100)?;
        machine.start()?;
        for _ in 0..1024 {
            machine.advance()?;
            if machine.instructions() != 0 {
                break;
            }
        }
        machine.pause()?;
        assert_eq!(machine.instructions(), 1);
        let MachineRegisters::X86_64 { gpr, rip, .. } = machine.read_registers()? else {
            return Err("wrong target".into());
        };
        assert_eq!(rip, Address::new(0x1000));
        assert!(gpr[1] > 0);
        let next = Address::new(gpr[7] - u64::from(reverse));
        machine.write_register("rcx", 2)?;
        machine.write_register("rax", 42)?;
        machine.write_register("df", u64::from(reverse))?;
        if restart {
            machine.write_register("rip", 0x1000)?;
        }
        step(&mut machine)?;
        assert_eq!(machine.instructions(), if restart { 2 } else { 1 });
        assert_eq!(machine.read_memory(next, 2)?, [42, 42]);
        assert_eq!(
            machine.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
    }
    Ok(())
}

#[test]
fn patching_the_last_address_byte_does_not_overflow_cache_invalidation() -> TestResult {
    for target in [Target::X86_64, Target::Aarch64] {
        let base = Address::new(u64::MAX - 3);
        let machine = Machine::load(
            Image::Raw {
                bytes: &[0; 4],
                base,
                entry: base,
            },
            target,
            MachineSetup::default(),
        )?;
        let mut machine = Session::new(machine, Address::new(0x1000), 100)?;
        machine.write_memory(Address::new(u64::MAX), &[42])?;
        assert_eq!(machine.read_memory(base, 4)?, [0, 0, 0, 42]);
        machine.reset()?;
        assert_eq!(machine.read_memory(base, 4)?, [0; 4]);
    }
    Ok(())
}

#[test]
fn maximum_patches_fit_one_mapping_and_cannot_span_adjacent_mappings() -> TestResult {
    use oplab_core::{address::AddressRange, memory::Permissions};
    use oplab_engine::load::InitialMapping;
    for target in [Target::X86_64, Target::Aarch64] {
        let original = vec![0x5a; 65_536];
        let machine = Machine::load(
            Image::Raw {
                bytes: &original,
                base: Address::new(0x1000),
                entry: Address::new(0x1000),
            },
            target,
            MachineSetup {
                cpu: None,
                registers: None,
                mappings: vec![InitialMapping {
                    range: AddressRange::new(Address::new(0x11000), 4096, 4096)?,
                    permissions: Permissions {
                        read: true,
                        write: true,
                        execute: false,
                    },
                    bytes: vec![0x33; 4096],
                }],
            },
        )?;
        let mut machine = Session::new(machine, Address::new(0x1800), 100)?;
        let patch = vec![0xa5; 65_536];
        machine.write_memory(Address::new(0x1000), &patch)?;
        assert_eq!(machine.read_memory(Address::new(0x1000), 65_536)?, patch);
        assert!(
            machine
                .write_memory(Address::new(0x10fff), &[1, 2])
                .is_err()
        );
        assert_eq!(machine.read_memory(Address::new(0x1000), 65_536)?, patch);
        assert_eq!(
            machine.read_memory(Address::new(0x11000), 4096)?,
            vec![0x33; 4096]
        );
        machine.reset()?;
        assert_eq!(machine.read_memory(Address::new(0x1000), 65_536)?, original);
    }
    Ok(())
}

#[test]
fn debugger_alias_writes_match_real_mov_instructions() -> TestResult {
    let x86 = [
        "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "ah", "ch", "dh", "bh", "r8b", "r9b",
        "r10b", "r11b", "r12b", "r13b", "r14b", "r15b", "ax", "cx", "dx", "bx", "sp", "bp", "si",
        "di", "r8w", "r9w", "r10w", "r11w", "r12w", "r13w", "r14w", "r15w", "eax", "ecx", "edx",
        "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d", "r12d", "r13d", "r14d",
        "r15d",
    ]
    .map(String::from);
    let arm: Vec<_> = (0..31)
        .map(|index| format!("w{index}"))
        .chain(["fp".into(), "lr".into(), "wsp".into()])
        .collect();
    for (target, names) in [
        (Target::X86_64, x86.as_slice()),
        (Target::Aarch64, arm.as_slice()),
    ] {
        let mut instructions = String::new();
        for name in names {
            if name == "wsp" {
                writeln!(instructions, "mov wsp, w0")?;
            } else {
                writeln!(instructions, "mov {name}, 42")?;
            }
        }
        let source = if target == Target::X86_64 {
            format!(".intel_syntax noprefix\n.text\n.global _start\n_start:\n{instructions}")
        } else {
            format!(".text\n.global _start\n_start:\n{instructions}")
        };
        let object = assembly::compile(target, &source).map_err(|error| format!("{error:?}"))?;
        let image =
            assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
        let mut edited = Session::from_elf(&image, target, Address::new(0x1800), 100)?;
        let mut executed = Session::from_elf(&image, target, Address::new(0x1800), 100)?;
        let canonical: Vec<_> = if target == Target::X86_64 {
            [
                "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11",
                "r12", "r13", "r14", "r15",
            ]
            .map(String::from)
            .to_vec()
        } else {
            (0..31)
                .map(|index| format!("x{index}"))
                .chain(["sp".into()])
                .collect()
        };
        for name in names {
            // Distinct fresh values expose wrong-bank writes and aliases that do nothing.
            for (index, register) in canonical.iter().enumerate() {
                let value = u64::MAX - u64::try_from(index)?;
                edited.write_register(register, value)?;
                executed.write_register(register, value)?;
            }
            if name == "wsp" {
                edited.write_register("x0", 42)?;
                executed.write_register("x0", 42)?;
            }
            let pc = edited.read_registers()?.instruction_pointer();
            edited.write_register(name, 42)?;
            step(&mut executed)?;
            let actual = edited.read_registers()?;
            assert_eq!(actual.instruction_pointer(), pc, "{name}");
            assert_eq!(edited.instructions(), 0);
            assert_eq!(edited.state(), ExecutionState::Ready);
            let mut expected = executed.read_registers()?;
            match &mut expected {
                MachineRegisters::X86_64 { rip, .. } => *rip = pc,
                MachineRegisters::Aarch64 { pc: address, .. } => *address = pc,
            }
            assert_eq!(actual, expected, "{name}");
        }
    }
    Ok(())
}

#[test]
fn flag_writes_drive_guest_conditions_and_preserve_other_bits() -> TestResult {
    for (target, code, flags) in [
        // SETZ AL / CSET W0,EQ: no preceding instruction overwrites the edited flags.
        (
            Target::X86_64,
            &[0x0f, 0x94, 0xc0][..],
            &[
                ("cf", 0),
                ("pf", 2),
                ("af", 4),
                ("zf", 6),
                ("sf", 7),
                ("df", 10),
                ("of", 11),
            ][..],
        ),
        (
            Target::Aarch64,
            &[0xe0, 0x17, 0x9f, 0x1a][..],
            &[("n", 31), ("z", 30), ("c", 29), ("v", 28)][..],
        ),
    ] {
        let mut machine = session(target, code)?;
        let initial = machine.read_registers()?;
        for set in [1, 0] {
            // Every checked write must change its bit, including the clearing pass.
            for &(name, _) in flags {
                machine.write_register(name, 1 - set)?;
            }
            for &(name, bit) in flags {
                let mut expected = machine.read_registers()?;
                match &mut expected {
                    MachineRegisters::X86_64 { rflags, .. } => {
                        *rflags = (*rflags & !(1 << bit)) | (set << bit);
                    }
                    MachineRegisters::Aarch64 { nzcv, .. } => {
                        *nzcv = (*nzcv & !(1 << bit)) | (u32::try_from(set)? << bit);
                    }
                }
                machine.write_register(name, set)?;
                assert_eq!(machine.read_registers()?, expected, "{name}={set}");
            }
            step(&mut machine)?;
            assert_eq!(integer(&machine.read_registers()?), set);
            machine.reset()?;
            assert_eq!(machine.read_registers()?, initial);
        }
    }
    Ok(())
}

#[test]
fn pc_edits_rearm_breakpoints_and_defer_completion_or_fetch_faults_until_run() -> TestResult {
    for (target, code, pc) in [
        (Target::X86_64, &[0x90; 8][..], "rip"),
        (
            Target::Aarch64,
            &[0x1f, 0x20, 0x03, 0xd5, 0x1f, 0x20, 0x03, 0xd5][..],
            "pc",
        ),
    ] {
        let mut machine = session(target, code)?;
        let initial = machine.read_registers()?;
        machine.set_breakpoint(Address::new(0x1000), true)?;
        machine.start()?;
        settle(&mut machine)?;
        for destination in [0x1004, 0x1000] {
            machine.set_breakpoint(Address::new(destination), true)?;
            machine.write_register(pc, destination)?;
            assert_eq!(
                machine.state(),
                ExecutionState::Paused(PauseReason::Requested)
            );
            assert_eq!(machine.instructions(), 0);
            machine.start()?;
            settle(&mut machine)?;
            assert_eq!(
                machine.state(),
                ExecutionState::Paused(PauseReason::Breakpoint(Address::new(destination)))
            );
            assert_eq!(machine.instructions(), 0);
        }
        // Writing the same PC is also an explicit restart, not breakpoint resume.
        machine.write_register(pc, 0x1000)?;
        machine.start()?;
        settle(&mut machine)?;
        assert_eq!(machine.instructions(), 0);
        step(&mut machine)?;
        assert_eq!(machine.instructions(), 1);
        machine.write_register(pc, 0x1800)?;
        assert!(matches!(machine.state(), ExecutionState::Paused(_)));
        machine.start()?;
        settle(&mut machine)?;
        assert_eq!(
            machine.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
        assert_eq!(machine.instructions(), 1);
        machine.reset()?;
        assert_eq!(machine.read_registers()?, initial);
        machine.write_register(pc, 0x3000)?;
        assert_eq!(machine.state(), ExecutionState::Ready);
        machine.start()?;
        settle(&mut machine)?;
        assert_eq!(
            machine.fault().map(|fault| fault.kind),
            Some(FaultKind::Unmapped(Access::Fetch))
        );
        assert_eq!(machine.instructions(), 0);
    }
    Ok(())
}
