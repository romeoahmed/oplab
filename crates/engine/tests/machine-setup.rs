//! Initial conditions are checked against byte geometry and real guest effects.

use object::{Object, ObjectSymbol};
use oplab_core::{
    address::{Address, AddressRange},
    diagnostic::ValidationError,
    execution::{ExecutionState, Termination},
    memory::{MAX_MAPPED_BYTES, Permissions},
    registers::{InitialRegisters, MachineRegisters},
    target::Target,
};
use oplab_engine::{
    assembly,
    load::{Image, InitialMapping, LoadError, LoadPlan, MachineSetup},
    machine::Machine,
    session::Session,
};
use proptest::prelude::*;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const RW: Permissions = Permissions {
    read: true,
    write: true,
    execute: false,
};

fn mapping(start: u64, length: u64, bytes: Vec<u8>) -> TestResult<InitialMapping> {
    Ok(InitialMapping {
        range: AddressRange::new(Address::new(start), length, MAX_MAPPED_BYTES)?,
        permissions: RW,
        bytes,
    })
}

proptest! {
    #[test]
    fn raw_mapping_preserves_exact_bytes_and_zero_padding(
        base in prop_oneof![any::<u64>(), (0_u64..128).prop_map(|tail| u64::MAX - tail)],
        bytes in prop::collection::vec(any::<u8>(), 1..128),
        shift in 8_u32..=16,
    ) {
        let end = u128::from(base) + bytes.len() as u128;
        let page_size = 1_u64 << shift;
        let plan = LoadPlan::new(
            Image::Raw { bytes: &bytes, base: Address::new(base), entry: Address::new(base) },
            Target::X86_64, page_size, MachineSetup::default(),
        );
        if end > 1_u128 << 64 {
            prop_assert_eq!(plan, Err(LoadError::Mapping(ValidationError::AddressOverflow)));
        } else {
            let plan = plan?;
            let [region] = plan.memory().regions() else { return Err(TestCaseError::fail("expected one mapping")); };
            let prefix = usize::try_from(base - region.range().start().get())?;
            prop_assert!(prefix < usize::try_from(page_size)?);
            prop_assert!(region.range().end() >= end && region.range().end() - end < u128::from(page_size));
            prop_assert!(region.range().start().get().is_multiple_of(page_size));
            prop_assert!(region.range().length().is_multiple_of(page_size));
            prop_assert_eq!(region.permissions(), Permissions { read: true, write: false, execute: true });
            let actual = region.initial();
            prop_assert!(actual[..prefix].iter().all(|byte| *byte == 0));
            prop_assert_eq!(&actual[prefix..prefix + bytes.len()], bytes.as_slice());
            prop_assert!(actual[prefix + bytes.len()..].iter().all(|byte| *byte == 0));
        }
    }
}

#[test]
fn raw_entries_and_extra_mappings_reject_invalid_initial_conditions() -> TestResult {
    let raw = Image::Raw {
        bytes: &[0; 8],
        base: Address::new(0x1004),
        entry: Address::new(0x1004),
    };
    for mappings in [
        vec![mapping(0x1000, 4096, vec![])?], // Overlap includes page padding.
        vec![
            mapping(0x8000, 4096, vec![])?,
            mapping(0x8000, 4096, vec![])?,
        ],
        vec![mapping(0x8001, 4096, vec![])?],
        vec![mapping(0x8000, 4095, vec![])?],
        vec![mapping(0x8000, 4096, vec![0; 4097])?],
        vec![mapping(0x8000, MAX_MAPPED_BYTES, vec![])?],
        (0..64)
            .map(|index| mapping(0x8000 + index * 4096, 4096, vec![]))
            .collect::<TestResult<_>>()?,
    ] {
        assert!(
            LoadPlan::new(
                raw,
                Target::X86_64,
                4096,
                MachineSetup {
                    cpu: None,
                    mappings,
                    registers: None
                }
            )
            .is_err()
        );
    }
    for (target, base, entry, length) in [
        (Target::X86_64, 0x1004, 0x1003, 8),
        (Target::X86_64, 0x1004, 0x100c, 8),
        (Target::Aarch64, 0x1004, 0x1006, 8),
        (Target::Aarch64, 0x1004, 0x1008, 7),
        (Target::X86_64, 0x1004, 0x1004, 0),
    ] {
        let bytes = vec![0; length];
        assert!(
            LoadPlan::new(
                Image::Raw {
                    bytes: &bytes,
                    base: Address::new(base),
                    entry: Address::new(entry)
                },
                target,
                4096,
                MachineSetup::default()
            )
            .is_err()
        );
    }
    let wrong = MachineSetup {
        cpu: None,
        registers: Some(InitialRegisters::Aarch64([0; 32])),
        mappings: vec![],
    };
    assert_eq!(
        LoadPlan::new(raw, Target::X86_64, 4096, wrong),
        Err(LoadError::Mapping(ValidationError::Target))
    );
    // The final byte and exclusive 2^64 end are valid plan geometry; native support is separate.
    let last = Image::Raw {
        bytes: &[0x90],
        base: Address::new(u64::MAX),
        entry: Address::new(u64::MAX),
    };
    assert_eq!(
        LoadPlan::new(last, Target::X86_64, 4096, MachineSetup::default())?
            .memory()
            .regions()[0]
            .range()
            .end(),
        1_u128 << 64
    );
    Ok(())
}

fn settle(session: &mut Session) -> TestResult {
    for _ in 0..1024 {
        if session.state() != ExecutionState::Running {
            return Ok(());
        }
        session.advance()?;
    }
    Err("machine did not stop".into())
}

fn check_initial_effects(value: u64, add: u64) -> TestResult {
    for (target, code, names) in [
        (
            Target::X86_64,
            &[0x48, 0x01, 0xc8, 0x48, 0x89, 0x04, 0x24][..],
            ["rax", "rcx", "rsp"],
        ),
        (
            Target::Aarch64,
            &[0x00, 0x00, 0x01, 0x8b, 0xe0, 0x03, 0x00, 0xf9][..],
            ["x0", "x1", "sp"],
        ),
    ] {
        // Fixed architectural encodings: add first two GPRs; store result at SP.
        let initial = [0xa5; 8];
        let setup = MachineSetup {
            cpu: None,
            registers: Some(InitialRegisters::from_assignments(
                target,
                [(names[0], value), (names[1], add), (names[2], 0x8000)],
            )?),
            mappings: vec![mapping(0x8000, 4096, initial.to_vec())?],
        };
        let image = Image::Raw {
            bytes: code,
            base: Address::new(0x1004),
            entry: Address::new(0x1004),
        };
        let machine = Machine::load(image, target, setup)?;
        assert_eq!(machine.read_memory(Address::new(0x1000), 4)?, [0; 4]);
        assert_eq!(
            machine.read_memory(Address::new(0x1004 + code.len() as u64), 8)?,
            [0; 8]
        );
        assert_eq!(machine.read_memory(Address::new(0x8008), 8)?, [0; 8]);
        let before = machine.read_registers()?;
        let mut session = Session::new(machine, Address::new(0x1004 + code.len() as u64), 2)?;
        for _ in 0..2 {
            assert_eq!(session.read_registers()?, before);
            assert_eq!(session.read_memory(Address::new(0x8000), 8)?, initial);
            session.start()?;
            settle(&mut session)?;
            assert_eq!(
                session.state(),
                ExecutionState::Terminated(Termination::Completed)
            );
            assert_eq!(session.instructions(), 2);
            assert_eq!(
                session.read_memory(Address::new(0x8000), 8)?,
                value.wrapping_add(add).to_le_bytes()
            );
            session.reset()?;
            assert_eq!(session.instructions(), 0);
            assert_eq!(session.dispatches(), 0);
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]
    #[test]
    fn configured_raw_sessions_preserve_full_width_inputs_and_reset(value in any::<u64>(), add in any::<u64>()) {
        check_initial_effects(value, add).map_err(|error| TestCaseError::fail(error.to_string()))?;
    }
}

#[test]
fn guest_stores_verify_every_named_register_independently_of_the_observation_adapter() -> TestResult
{
    for (target, names) in [
        (
            Target::X86_64,
            "rax rcx rdx rbx rsp rbp rsi rdi r8 r9 r10 r11 r12 r13 r14 r15".into(),
        ),
        (
            Target::Aarch64,
            (0..31)
                .map(|index| format!("x{index}"))
                .collect::<Vec<_>>()
                .join(" "),
        ),
    ] {
        let assignments: Vec<_> = names
            .split_whitespace()
            .enumerate()
            .map(|(index, name)| (name, u64::MAX - index as u64))
            .collect();
        let mut source = assignments
            .iter()
            .enumerate()
            .map(|(index, (name, _))| match target {
                Target::X86_64 => format!("mov [0x80000 + {}], {name}\n", index * 8),
                Target::Aarch64 => format!("str {name}, [sp, #{}]\n", index * 8),
            })
            .collect::<String>();
        let mut initial = assignments.clone();
        let mut expected: Vec<_> = assignments
            .iter()
            .flat_map(|(_, value)| value.to_le_bytes())
            .collect();
        if target == Target::Aarch64 {
            initial.push(("sp", 0x80000));
            source.push_str("mov x0, sp\nstr x0, [sp, #248]\n");
            expected.extend_from_slice(&0x80000_u64.to_le_bytes());
        }
        source.push_str("done: nop");
        let object = assembly::compile(target, &source).map_err(|error| format!("{error:?}"))?;
        let image =
            assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
        let completion = object::File::parse(image.as_slice())?
            .symbols()
            .find(|symbol| symbol.name() == Ok("done"))
            .ok_or("missing completion")?
            .address();
        let machine = Machine::load(
            Image::Elf(&image),
            target,
            MachineSetup {
                cpu: None,
                registers: Some(InitialRegisters::from_assignments(target, initial)?),
                mappings: vec![mapping(0x80000, 4096, vec![])?],
            },
        )?;
        let observed: Vec<_> = match machine.read_registers()? {
            MachineRegisters::X86_64 { gpr, .. } => {
                gpr.into_iter().flat_map(u64::to_le_bytes).collect()
            }
            MachineRegisters::Aarch64 { x, sp, .. } => x
                .into_iter()
                .chain([sp])
                .flat_map(u64::to_le_bytes)
                .collect(),
        };
        assert_eq!(observed, expected);
        let mut session = Session::new(machine, Address::new(completion), 64)?;
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
        assert_eq!(
            session.read_memory(Address::new(0x80000), u64::try_from(expected.len())?)?,
            expected
        );
    }
    Ok(())
}

#[test]
fn elf_setup_preserves_entry_permissions_and_runs_with_an_explicit_stack() -> TestResult {
    for (target, source, first, stack) in [
        (Target::X86_64, "push rax\npop rbx\ndone: nop", "rax", "rsp"),
        (
            Target::Aarch64,
            "str x0, [sp, #-16]!\nldr x1, [sp], #16\ndone: nop",
            "x0",
            "sp",
        ),
    ] {
        let object = assembly::compile(target, source).map_err(|error| format!("{error:?}"))?;
        let image =
            assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
        let setup = MachineSetup {
            cpu: None,
            registers: Some(InitialRegisters::from_assignments(
                target,
                [(first, 42), (stack, 0x9000)],
            )?),
            mappings: vec![mapping(0x8000, 4096, vec![])?],
        };
        let machine = Machine::load(Image::Elf(&image), target, setup)?;
        assert_eq!(machine.initial().entry(), Address::new(0x1000));
        let text = machine
            .initial()
            .memory()
            .regions()
            .iter()
            .find(|region| region.range().contains(Address::new(0x1000)))
            .ok_or("missing code")?;
        assert!(!text.permissions().write);
        let completion = if target == Target::X86_64 {
            0x1002
        } else {
            0x1008
        };
        let mut session = Session::new(machine, Address::new(completion), 2)?;
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
        match session.read_registers()? {
            MachineRegisters::X86_64 { gpr, .. } => {
                assert_eq!(gpr[3], 42); // RBX receives the popped value.
                assert_eq!(gpr[4], 0x9000); // RSP returns to the stack top.
            }
            MachineRegisters::Aarch64 { x, sp, .. } => {
                assert_eq!(x[1], 42);
                assert_eq!(sp, 0x9000);
            }
        }
    }
    Ok(())
}
