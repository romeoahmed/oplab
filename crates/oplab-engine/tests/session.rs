//! Independent integer semantics and control boundaries on real guest machines.

use object::{Object, ObjectSymbol};
use oplab_core::{
    address::Address,
    execution::{Access, ExecutionState, FaultKind, PauseReason, Termination},
    registers::IntegerRegisters,
    target::Target,
};
use oplab_engine::{assembly, session::Session};
use proptest::prelude::*;
use std::fmt::Write;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn fixture(target: Target, source: &str, budget: u64) -> TestResult<(Session, Vec<u8>)> {
    let object = assembly::compile(target, source).map_err(|error| format!("{error:?}"))?;
    let image =
        assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
    let session = Session::from_elf(&image, target, symbol(&image, "done")?, budget)?;
    Ok((session, image))
}

fn symbol(image: &[u8], name: &str) -> TestResult<Address> {
    object::File::parse(image)?
        .symbols()
        .find(|symbol| symbol.name() == Ok(name))
        .map(|symbol| Address::new(symbol.address()))
        .ok_or_else(|| "missing fixture symbol".into())
}

fn settle(session: &mut Session) -> TestResult {
    // This is a test deadline in addition to the engine's own bounded slices.
    for _ in 0..1024 {
        if session.state() != ExecutionState::Running {
            return Ok(());
        }
        session.advance()?;
    }
    Err("session did not reach its expected boundary".into())
}

fn first_integer(session: &Session) -> TestResult<u64> {
    Ok(match session.read_registers()? {
        IntegerRegisters::X86_64 { gpr, .. } => gpr[0],
        IntegerRegisters::Aarch64 { x, .. } => x[0],
    })
}

#[test]
fn integer_aliases_flags_and_explicit_completion_follow_the_architecture() -> TestResult {
    for (target, source) in [
        (
            Target::X86_64,
            "movabs rax, -1\nmov eax, 42\nmov ah, 1\nadd eax, 1\ncmp eax, 299\ndone: nop",
        ),
        (
            Target::Aarch64,
            "mov x0, -1\nmov w0, 42\nadd w0, w0, 257\ncmp w0, 299\ndone: nop",
        ),
    ] {
        let (mut session, image) = fixture(target, source, 100)?;
        session.step()?;
        assert_eq!(session.state(), ExecutionState::Paused(PauseReason::Step));
        assert_eq!(first_integer(&session)?, u64::MAX);
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
        assert_eq!(first_integer(&session)?, 299);
        let registers = session.read_registers()?;
        assert_eq!(registers.instruction_pointer(), symbol(&image, "done")?);
        match registers {
            IntegerRegisters::X86_64 { rflags, .. } => assert_ne!(rflags & (1 << 6), 0),
            IntegerRegisters::Aarch64 { nzcv, .. } => assert_ne!(nzcv & (1 << 30), 0),
        }
        assert!(session.start().is_err());
    }
    Ok(())
}

#[test]
fn breakpoints_stop_before_effects_rearm_on_loops_and_survive_reset() -> TestResult {
    for (target, source) in [
        (Target::X86_64, "again: add rax, 1\njmp again\ndone: nop"),
        (Target::Aarch64, "again: add x0, x0, 1\nb again\ndone: nop"),
    ] {
        let (mut session, _) = fixture(target, source, 100)?;
        let address = Address::new(0x1000);
        session.set_breakpoint(address, true)?;
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Paused(PauseReason::Breakpoint(address))
        );
        assert_eq!(first_integer(&session)?, 0);
        assert_eq!(session.dispatches(), 0);
        session.step()?;
        assert_eq!(session.state(), ExecutionState::Paused(PauseReason::Step));
        assert_eq!(first_integer(&session)?, 1);
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Paused(PauseReason::Breakpoint(address))
        );
        assert_eq!(first_integer(&session)?, 1);
        session.start()?;
        settle(&mut session)?;
        assert_eq!(first_integer(&session)?, 2);
        session.reset()?;
        assert_eq!(session.generation().get(), 1);
        assert_eq!(session.dispatches(), 0);
        assert_eq!(first_integer(&session)?, 0);
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Paused(PauseReason::Breakpoint(address))
        );
        assert_eq!(first_integer(&session)?, 0);
    }
    Ok(())
}

#[test]
fn repeat_step_finishes_the_instruction_and_reset_restores_written_memory() -> TestResult {
    let source = "lea rdi, [rip + output]\nmov ecx, 8\nmov al, 42\nrepeat_here: rep stosb\nafter_repeat: inc ebx\ndone: nop\n.bss\noutput: .skip 8";
    let (mut session, image) = fixture(Target::X86_64, source, 100)?;
    let repeat = symbol(&image, "repeat_here")?;
    let output = symbol(&image, "output")?;
    session.set_breakpoint(repeat, true)?;
    session.start()?;
    settle(&mut session)?;
    assert_eq!(
        session.state(),
        ExecutionState::Paused(PauseReason::Breakpoint(repeat))
    );
    session.step()?;
    settle(&mut session)?;
    assert_eq!(session.state(), ExecutionState::Paused(PauseReason::Step));
    assert_eq!(
        session.read_registers()?.instruction_pointer(),
        symbol(&image, "after_repeat")?
    );
    assert_eq!(session.read_memory(output, 8)?, [42; 8]);
    if let IntegerRegisters::X86_64 { gpr, .. } = session.read_registers()? {
        assert_eq!(gpr[1], 0); // RCX exhausted; next INC has not executed.
        assert_eq!(gpr[3], 0);
    }
    session.reset()?;
    assert_eq!(session.read_memory(output, 8)?, [0; 8]);
    let (mut budgeted, _) = fixture(Target::X86_64, source, 4)?;
    budgeted.start()?;
    settle(&mut budgeted)?;
    assert_eq!(
        budgeted.state(),
        ExecutionState::Terminated(Termination::Budget)
    );
    assert_eq!(budgeted.instructions(), 4); // Three setup instructions and the complete REP.
    assert!(budgeted.dispatches() > budgeted.instructions());
    assert_eq!(budgeted.read_memory(output, 8)?, [42; 8]);
    Ok(())
}

#[test]
fn faults_and_environment_exits_cannot_masquerade_as_completion() -> TestResult {
    for (target, source, expected) in [
        (
            Target::X86_64,
            "movabs rax, 0x800000\nmov byte ptr [rax], 42",
            FaultKind::Unmapped(Access::Write),
        ),
        (
            Target::Aarch64,
            "mov x0, 0x800000\nstr x1, [x0]",
            FaultKind::Unmapped(Access::Write),
        ),
        (
            Target::X86_64,
            "mov byte ptr [rip - 7], 42",
            FaultKind::Protection(Access::Write),
        ),
        (
            Target::Aarch64,
            "adr x0, .\nstr x1, [x0]",
            FaultKind::Protection(Access::Write),
        ),
        (Target::X86_64, "ud2", FaultKind::InvalidInstruction),
        (
            Target::X86_64,
            "movabs rax, 0x800000\njmp rax",
            FaultKind::Unmapped(Access::Fetch),
        ),
        (
            Target::Aarch64,
            "mov x0, 0x800000\nbr x0",
            FaultKind::Unmapped(Access::Fetch),
        ),
    ] {
        let (mut session, _) = fixture(target, &format!("{source}\ndone: nop"), 100)?;
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::GuestFault)
        );
        assert_eq!(session.fault().map(|fault| fault.kind), Some(expected));
        if expected == FaultKind::Unmapped(Access::Write) {
            assert_eq!(
                session.fault().and_then(|fault| fault.address),
                Some(Address::new(0x0080_0000))
            );
        }
    }
    for (target, source) in [
        (Target::X86_64, "syscall"),
        (Target::X86_64, "hlt"),
        (Target::X86_64, "in eax, 0x80"),
        (Target::Aarch64, "svc 0"),
        (Target::Aarch64, "wfi"),
    ] {
        let (mut session, _) = fixture(target, &format!("{source}\ndone: nop"), 100)?;
        session.step()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::UnsupportedEnvironment)
        );
        assert!(session.fault().is_none());
    }
    Ok(())
}

#[test]
fn branch_steps_finish_before_the_next_fetch_but_data_faults_remain_immediate() -> TestResult {
    for (target, branch, store) in [
        (
            Target::X86_64,
            "mov eax, 0x800000\njmp rax\ndone: nop",
            "mov eax, 0x800000\nmov byte ptr [rax], 1\ndone: nop",
        ),
        (
            Target::Aarch64,
            "mov x0, 0x800000\nbr x0\ndone: nop",
            "mov x0, 0x800000\nstr x1, [x0]\ndone: nop",
        ),
    ] {
        let (mut session, _) = fixture(target, branch, 100)?;
        for _ in 0..2 {
            session.step()?;
            settle(&mut session)?;
            assert_eq!(session.state(), ExecutionState::Paused(PauseReason::Step));
        }
        assert_eq!(
            session.read_registers()?.instruction_pointer(),
            Address::new(0x0080_0000)
        );
        assert!(session.fault().is_none());
        session.step()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::GuestFault)
        );
        assert_eq!(
            session.fault().map(|fault| fault.kind),
            Some(FaultKind::Unmapped(Access::Fetch))
        );
        assert_eq!(session.instructions(), 2);

        // An exhausted budget prevents the next fetch, but cannot erase a fault
        // produced by the last admitted instruction's own memory access.
        for (source, expected) in [
            (branch, Termination::Budget),
            (store, Termination::GuestFault),
        ] {
            let (mut session, _) = fixture(target, source, 2)?;
            session.start()?;
            settle(&mut session)?;
            assert_eq!(session.state(), ExecutionState::Terminated(expected));
            assert_eq!(session.instructions(), 2);
            if expected == Termination::GuestFault {
                assert_eq!(
                    session.fault().map(|fault| fault.kind),
                    Some(FaultKind::Unmapped(Access::Write))
                );
            } else {
                assert!(session.fault().is_none());
            }
        }
    }
    Ok(())
}

#[test]
fn cooperative_controls_and_budget_limits_remain_distinct_from_success() -> TestResult {
    for (target, source) in [
        (Target::X86_64, "again: jmp again\ndone: nop"),
        (Target::Aarch64, "again: b again\ndone: nop"),
    ] {
        let (mut session, _) = fixture(target, source, 4096)?;
        session.start()?;
        session.advance()?;
        assert_eq!(session.state(), ExecutionState::Running);
        assert!(session.dispatches() <= 1024);
        assert!(session.reset().is_err());
        assert!(session.set_breakpoint(Address::new(0x1000), true).is_err());
        session.pause()?;
        assert_eq!(
            session.state(),
            ExecutionState::Paused(PauseReason::Requested)
        );
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Budget)
        );
        assert_eq!(session.dispatches(), 4096);
        session.reset()?;
        session.cancel()?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Cancelled)
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]
    #[test]
    fn generated_integer_programs_match_wrapping_arithmetic(a in any::<u32>(), b in any::<u32>()) {
        let x86 = format!("mov eax, {a}\nmov ebx, {b}\nadd eax, ebx\ndone: nop");
        let arm = format!("movz w0, {}\nmovk w0, {}, lsl 16\nmovz w1, {}\nmovk w1, {}, lsl 16\nadd w0, w0, w1\ndone: nop", a & 0xffff, a >> 16, b & 0xffff, b >> 16);
        for (target, source, budget) in [(Target::X86_64, x86, 3), (Target::Aarch64, arm, 5)] {
            let (mut session, _) = fixture(target, &source, budget).map_err(|error| TestCaseError::fail(error.to_string()))?;
            session.start()?;
            settle(&mut session).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(session.state(), ExecutionState::Terminated(Termination::Completed));
            let actual = first_integer(&session).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(actual, u64::from(a.wrapping_add(b)));
        }
    }
}

#[test]
fn explicit_completion_precedes_an_unmapped_fetch() -> TestResult {
    for (target, count) in [(Target::X86_64, 4096), (Target::Aarch64, 1024)] {
        let source = format!(".rept {count}\nnop\n.endr\ndone:");
        let (mut session, mut image) = fixture(target, &source, count)?;
        let completion = symbol(&image, "done")?;
        assert!(session.read_memory(completion, 1).is_err());
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
        assert_eq!(session.dispatches(), count);
        // ELF64 e_entry may select the final instruction inside an existing segment.
        let entry = completion.get() - target.instruction_alignment();
        image[24..32].copy_from_slice(&entry.to_le_bytes());
        for (completion, expected) in [
            (0x2000, Termination::Completed),
            (0x3000, Termination::Budget),
        ] {
            let mut session = Session::from_elf(&image, target, Address::new(completion), 1)?;
            session.start()?;
            settle(&mut session)?;
            assert_eq!(
                session.state(),
                ExecutionState::Terminated(expected),
                "{target:?}: {:?}, {} starts, {} dispatches",
                session.fault(),
                session.instructions(),
                session.dispatches()
            );
            assert_eq!(session.instructions(), 1);
            assert!(session.fault().is_none());
        }
    }
    Ok(())
}

#[test]
fn canonical_banks_preserve_every_integer_register_and_distinct_stack_storage() -> TestResult {
    for target in [Target::X86_64, Target::Aarch64] {
        let mut source = String::new();
        match target {
            Target::X86_64 => {
                for (index, name) in [
                    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10",
                    "r11", "r12", "r13", "r14", "r15",
                ]
                .into_iter()
                .enumerate()
                {
                    writeln!(source, "mov {name}, {}", index + 1)?;
                }
            }
            Target::Aarch64 => {
                for index in 0..31 {
                    writeln!(source, "mov x{index}, {}", index + 1)?;
                }
                source.push_str("mov sp, x30\n");
            }
        }
        source.push_str("done: nop");
        let (mut session, _) = fixture(target, &source, 100)?;
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
        match session.read_registers()? {
            IntegerRegisters::X86_64 { gpr, .. } => {
                assert_eq!(gpr, std::array::from_fn(|index| index as u64 + 1));
            }
            IntegerRegisters::Aarch64 { x, sp, .. } => {
                assert_eq!(x, std::array::from_fn(|index| index as u64 + 1));
                assert_eq!(sp, 31);
            }
        }
    }
    Ok(())
}
