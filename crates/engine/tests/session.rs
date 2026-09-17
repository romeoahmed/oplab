//! Independent computation semantics and control boundaries on real guest machines.

use object::{Object, ObjectSymbol};
use oplab_core::{
    address::{Address, AddressRange},
    execution::{Access, ExecutionState, FaultKind, PauseReason, Termination},
    memory::Permissions,
    registers::MachineRegisters,
    target::Target,
};
use oplab_engine::{
    load::{Image, InitialMapping, MachineSetup},
    machine::Machine,
    session::Session,
};
use oplab_toolchain::assembly;
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if session.state() != ExecutionState::Running {
            return Ok(());
        }
        session.advance()?;
    }
    Err("session did not reach its expected boundary".into())
}

fn first_integer(session: &Session) -> TestResult<u64> {
    Ok(match session.read_registers()? {
        MachineRegisters::X86_64 { gpr, .. } => gpr[0],
        MachineRegisters::Aarch64 { x, .. } => x[0],
    })
}

#[test]
fn workbench_examples_match_scalar_pixel_processing_at_both_addresses() -> TestResult {
    for (target, source) in [
        (Target::X86_64, include_str!("../../../examples/x86_64.s")),
        (Target::Aarch64, include_str!("../../../examples/aarch64.s")),
    ] {
        let object =
            assembly::compile(target, source).map_err(|error| format!("{target:?}: {error:?}"))?;
        for base in [0x1000, 0x1234_5000] {
            let image = assembly::link(&object, Address::new(base))
                .map_err(|error| format!("{target:?}: {error:?}"))?;
            let done = symbol(&image, "done")?;
            let output = symbol(&image, "output")?;
            let checksum = symbol(&image, "checksum")?;
            let input = symbol(&image, "input")?;
            let machine = Machine::load(Image::Elf(&image), target, MachineSetup::default())?;
            let mut session = Session::new(machine, done, 10_000)?;
            let original = session.read_memory(input, 32)?;
            let expected: Vec<_> = original
                .iter()
                .enumerate()
                .map(|(index, &value)| {
                    if index % 4 == 3 {
                        value
                    } else {
                        value.saturating_add(32)
                    }
                })
                .collect();
            let sum: u64 = expected.iter().copied().map(u64::from).sum();
            for _ in 0..2 {
                assert_eq!(session.read_memory(output, 32)?, [0; 32]);
                assert_eq!(session.read_memory(checksum, 8)?, [0; 8]);
                session.start()?;
                settle(&mut session)?;
                assert_eq!(
                    session.state(),
                    ExecutionState::Terminated(Termination::Completed),
                    "{target:?} at {base:#x}: {:?}",
                    session.fault()
                );
                assert_eq!(session.read_registers()?.instruction_pointer(), done);
                assert_eq!(first_integer(&session)?, sum);
                assert_eq!(session.read_memory(output, 32)?, expected);
                assert_eq!(session.read_memory(checksum, 8)?, sum.to_le_bytes());
                assert_eq!(session.read_memory(input, 32)?, original);
                session.reset()?;
            }
        }
    }
    Ok(())
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
            MachineRegisters::X86_64 { rflags, .. } => assert_ne!(rflags & (1 << 6), 0),
            MachineRegisters::Aarch64 { nzcv, .. } => assert_ne!(nzcv & (1 << 30), 0),
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
    session.record_trace(true)?;
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
    assert_eq!(
        session
            .trace()
            .entries()
            .filter(|entry| entry.pc == repeat)
            .count(),
        1
    );
    assert_eq!(session.trace().entries().len(), 4);
    assert_eq!(session.read_memory(output, 8)?, [42; 8]);
    let MachineRegisters::X86_64 { gpr, .. } = session.read_registers()? else {
        return Err("wrong register bank".into());
    };
    assert_eq!(gpr[1], 0); // RCX exhausted; next INC has not executed.
    assert_eq!(gpr[3], 0);
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
        (Target::X86_64, "hlt", FaultKind::Exception(Some(13))),
        (
            Target::X86_64,
            "in eax, 0x80",
            FaultKind::Exception(Some(13)),
        ),
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
        let code = session.read_memory(Address::new(0x1000), 16)?;
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::GuestFault)
        );
        assert_eq!(
            session.fault().map(|fault| fault.kind),
            Some(expected),
            "{target:?}: {source}"
        );
        if expected == FaultKind::Protection(Access::Write) {
            assert_eq!(session.read_memory(Address::new(0x1000), 16)?, code);
        }
        if expected == FaultKind::Unmapped(Access::Write) {
            assert_eq!(
                session.fault().and_then(|fault| fault.address),
                Some(Address::new(0x0080_0000))
            );
        }
    }
    for (target, source) in [
        (Target::X86_64, "syscall"),
        (Target::Aarch64, "svc 0"),
        (Target::Aarch64, "wfi"),
    ] {
        let (mut session, _) = fixture(target, &format!("{source}\ndone: nop"), 100)?;
        session.step()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::UnsupportedEnvironment),
            "{target:?}: {source}: {:?}",
            session.fault()
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
        session.record_trace(true)?;
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
        assert_eq!(session.trace().entries().len(), 2);

        // An exhausted budget prevents the next fetch, but cannot erase a fault
        // produced by the last admitted instruction's own memory access.
        for (source, expected) in [
            (branch, Termination::Budget),
            (store, Termination::GuestFault),
        ] {
            let (mut session, _) = fixture(target, source, 2)?;
            session.record_trace(true)?;
            session.start()?;
            settle(&mut session)?;
            assert_eq!(session.state(), ExecutionState::Terminated(expected));
            assert_eq!(session.instructions(), 2);
            assert_eq!(session.trace().entries().len(), 2);
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
        assert!(session.instructions() > 0 && session.instructions() < 4096);
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
    fn generated_integer_programs_match_wrapping_arithmetic(
        initial in any::<u32>(),
        operations in prop::collection::vec((0_u8..3, any::<u32>()), 1..12),
    ) {
        let mut expected = initial;
        let mut x86 = format!("mov eax, {initial}\n");
        let mut arm = format!("movz w0, {}\nmovk w0, {}, lsl 16\n", initial & 0xffff, initial >> 16);
        for &(operation, value) in &operations {
            let (x86_op, arm_op) = match operation {
                0 => { expected = expected.wrapping_add(value); ("add", "add") }
                1 => { expected = expected.wrapping_sub(value); ("sub", "sub") }
                _ => { expected ^= value; ("xor", "eor") }
            };
            writeln!(x86, "mov ebx, {value}\n{x86_op} eax, ebx")?;
            writeln!(arm, "movz w1, {}\nmovk w1, {}, lsl 16\n{arm_op} w0, w0, w1", value & 0xffff, value >> 16)?;
        }
        x86.push_str("done: nop");
        arm.push_str("done: nop");
        for (target, source) in [(Target::X86_64, x86), (Target::Aarch64, arm)] {
            let (mut session, _) = fixture(target, &source, 100).map_err(|error| TestCaseError::fail(error.to_string()))?;
            session.start()?;
            settle(&mut session).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(session.state(), ExecutionState::Terminated(Termination::Completed));
            let actual = first_integer(&session).map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(actual, u64::from(expected));
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
            MachineRegisters::X86_64 { gpr, .. } => {
                assert_eq!(gpr, std::array::from_fn(|index| index as u64 + 1));
            }
            MachineRegisters::Aarch64 { x, sp, .. } => {
                assert_eq!(x, std::array::from_fn(|index| index as u64 + 1));
                assert_eq!(sp, 31);
            }
        }
    }
    Ok(())
}

#[test]
fn grouped_breakpoints_stop_before_each_effect_and_survive_reset() -> TestResult {
    for (target, source) in [
        (
            Target::X86_64,
            "first: add rax, 1\nsecond: add rax, 2\ndone: nop",
        ),
        (
            Target::Aarch64,
            "first: add x0, x0, 1\nsecond: add x0, x0, 2\ndone: nop",
        ),
    ] {
        let (mut session, image) = fixture(target, source, 100)?;
        let first = symbol(&image, "first")?;
        let second = symbol(&image, "second")?;
        session.set_breakpoints(&[second, first, first], true)?;
        for (address, value) in [(first, 0), (second, 1)] {
            session.start()?;
            settle(&mut session)?;
            assert_eq!(
                session.state(),
                ExecutionState::Paused(PauseReason::Breakpoint(address))
            );
            assert_eq!(first_integer(&session)?, value);
        }
        session.reset()?;
        assert_eq!(session.breakpoints().collect::<Vec<_>>(), [first, second]);
        session.set_breakpoints(&[first, second], false)?;
        assert_eq!(session.breakpoints().count(), 0);
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
        assert_eq!(first_integer(&session)?, 3);
    }
    Ok(())
}

#[test]
fn grouped_breakpoint_limits_and_alignment_fail_without_partial_updates() -> TestResult {
    for target in [Target::X86_64, Target::Aarch64] {
        let (mut session, _) = fixture(target, "nop\ndone: nop", 100)?;
        let addresses = (0..257)
            .map(|index| Address::new(0x1000 + index * 4))
            .collect::<Vec<_>>();
        session.set_breakpoints(&addresses[..255], true)?;
        session.set_breakpoints(&[addresses[254], addresses[255]], true)?;
        assert_eq!(session.breakpoints().collect::<Vec<_>>(), addresses[..256]);
        // Existing entries and empty groups remain valid at capacity.
        session.set_breakpoints(&[addresses[0]; 256], true)?;
        session.set_breakpoints(&[], true)?;
        for enabled in [false, true] {
            assert!(session.set_breakpoints(&addresses, enabled).is_err());
            assert_eq!(session.breakpoints().collect::<Vec<_>>(), addresses[..256]);
            if target == Target::Aarch64 {
                assert!(
                    session
                        .set_breakpoints(&[addresses[0], Address::new(0x1001)], enabled)
                        .is_err()
                );
                assert_eq!(session.breakpoints().collect::<Vec<_>>(), addresses[..256]);
            }
        }
        assert!(session.set_breakpoints(&addresses[255..], true).is_err());
        assert_eq!(session.breakpoints().collect::<Vec<_>>(), addresses[..256]);
        session.set_breakpoints(&addresses[128..256], false)?;
        assert_eq!(session.breakpoints().collect::<Vec<_>>(), addresses[..128]);
        session.set_breakpoints(&addresses[128..256], true)?;
        assert_eq!(session.breakpoints().collect::<Vec<_>>(), addresses[..256]);
    }
    Ok(())
}

fn call_fixture(target: Target, indirect: bool) -> TestResult<(Session, Vec<u8>)> {
    let source = match target {
        Target::X86_64 => {
            let call = if indirect {
                "lea r11, [rip + outer]\ncallsite: call r11"
            } else {
                "callsite: call outer"
            };
            format!(
                r".intel_syntax noprefix
.text
.globl _start
_start:
    lea rsp, [rip + stack_end]
    {call}
returned:
    add rax, 100
    jmp done
outer:
    inc rax
    call inner
    ret
inner:
    add rax, 2
    ret
done: nop
.bss
.balign 16
.space 4096
stack_end:"
            )
        }
        Target::Aarch64 => {
            let call = if indirect {
                "adr x10, outer\ncallsite: blr x10"
            } else {
                "callsite: bl outer"
            };
            format!(
                r".text
.globl _start
_start:
    adrp x9, stack_end
    add x9, x9, :lo12:stack_end
    mov sp, x9
    {call}
returned:
    add x0, x0, 100
    b done
outer:
    stp x29, x30, [sp, -16]!
    add x0, x0, 1
    bl inner
    ldp x29, x30, [sp], 16
    ret
inner:
    add x0, x0, 2
    ret
done: nop
.bss
.balign 16
.space 4096
stack_end:"
            )
        }
    };
    fixture(target, &source, 2000)
}

#[test]
fn temporary_targets_and_step_over_preserve_breakpoints_and_stop_before_effects() -> TestResult {
    for (target, indirect) in [
        (Target::X86_64, false),
        (Target::X86_64, true),
        (Target::Aarch64, false),
        (Target::Aarch64, true),
    ] {
        let (mut session, image) = call_fixture(target, indirect)?;
        let call = symbol(&image, "callsite")?;
        let returned = symbol(&image, "returned")?;
        let inner = symbol(&image, "inner")?;
        session.record_trace(true)?;
        session.run_until(&[call; 256])?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Paused(PauseReason::Target(call))
        );
        let count = session.instructions();
        session.run_until(&[call])?;
        settle(&mut session)?;
        assert_eq!(session.instructions(), count);
        assert!(session.run_until(&[]).is_err());
        assert!(session.run_until(&[call; 257]).is_err());
        assert_eq!(
            session.state(),
            ExecutionState::Paused(PauseReason::Target(call))
        );
        session.step_over()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Paused(PauseReason::Target(returned))
        );
        assert_eq!(first_integer(&session)?, 3);
        assert_eq!(session.breakpoints().count(), 0);
        assert!(session.trace().entries().any(|entry| entry.pc == inner));
        assert!(!session.trace().entries().any(|entry| entry.pc == returned));
        session.step_over()?;
        settle(&mut session)?;
        assert_eq!(first_integer(&session)?, 103);
        assert_eq!(session.state(), ExecutionState::Paused(PauseReason::Step));

        session.reset()?;
        assert!(session.trace().enabled());
        assert_eq!(session.trace().entries().len(), 0);
        session.run_until(&[call])?;
        settle(&mut session)?;
        session.set_breakpoint(inner, true)?;
        session.step_over()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Paused(PauseReason::Breakpoint(inner))
        );
        assert_eq!(first_integer(&session)?, 1);
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
        assert_eq!(first_integer(&session)?, 103);
        assert_eq!(session.breakpoints().collect::<Vec<_>>(), [inner]);
    }
    Ok(())
}

#[test]
fn trace_capacity_clear_and_reset_preserve_execution_ownership() -> TestResult {
    for target in [Target::X86_64, Target::Aarch64] {
        let (mut session, _) = fixture(target, ".rept 514\nnop\n.endr\ndone:", 1000)?;
        assert!(!session.trace().enabled());
        session.record_trace(true)?;
        let width = if target == Target::X86_64 { 1 } else { 4 };
        for count in [511, 512, 513] {
            session.run_until(&[Address::new(0x1000 + count * width)])?;
            settle(&mut session)?;
            assert_eq!(session.instructions(), count);
            assert_eq!(session.trace().discarded(), count.saturating_sub(512));
            assert_eq!(
                session
                    .trace()
                    .entries()
                    .map(|entry| entry.instruction)
                    .collect::<Vec<_>>(),
                (count.saturating_sub(512) + 1..=count).collect::<Vec<_>>()
            );
        }
        session.clear_trace()?;
        assert_eq!(session.instructions(), 513);
        assert_eq!(session.trace().entries().len(), 0);
        assert_eq!(session.trace().discarded(), 0);
        assert!(session.trace().enabled());
        session.step()?;
        settle(&mut session)?;
        assert_eq!(
            session
                .trace()
                .entries()
                .next()
                .map(|entry| entry.instruction),
            Some(514)
        );
        session.reset()?;
        assert_eq!(session.instructions(), 0);
        assert_eq!(session.trace().entries().len(), 0);
        assert_eq!(session.trace().discarded(), 0);
        assert!(session.trace().enabled());
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]
    #[test]
    fn trace_matches_recorded_intervals_after_clears(
        intervals in prop::collection::vec((any::<bool>(), any::<bool>(), 1_u16..160), 1..7),
    ) {
        let total: u64 = intervals.iter().map(|(_, _, count)| u64::from(*count)).sum();
        for target in [Target::X86_64, Target::Aarch64] {
            let (mut session, _) = fixture(target, &format!(".rept {total}\nnop\n.endr\ndone:"), total)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let width = if target == Target::X86_64 { 1 } else { 4 };
            let mut starts = 0;
            let mut recorded = Vec::new();
            for &(enabled, clear, count) in &intervals {
                if clear {
                    session.clear_trace()?;
                    recorded.clear();
                }
                session.record_trace(enabled)?;
                let end = starts + u64::from(count);
                if enabled { recorded.extend(starts + 1..=end); }
                session.run_until(&[Address::new(0x1000 + end * width)])?;
                settle(&mut session).map_err(|error| TestCaseError::fail(error.to_string()))?;
                starts = end;
                let discarded = recorded.len().saturating_sub(512);
                let expected: Vec<_> = recorded[discarded..].iter()
                    .map(|&instruction| (instruction, 0x1000 + (instruction - 1) * width)).collect();
                let actual: Vec<_> = session.trace().entries()
                    .map(|entry| (entry.instruction, entry.pc.get())).collect();
                prop_assert_eq!(actual, expected);
                prop_assert_eq!(session.trace().discarded(), u64::try_from(discarded)?);
                prop_assert_eq!(session.trace().enabled(), enabled);
                prop_assert_eq!(session.instructions(), starts);
            }
            prop_assert_eq!(session.state(), ExecutionState::Terminated(Termination::Completed));
        }
    }
}

#[test]
fn temporary_runs_remain_interruptible_and_budgeted() -> TestResult {
    for target in [Target::X86_64, Target::Aarch64] {
        let source = if target == Target::X86_64 {
            ".text\n.globl _start\n_start: jmp _start\ndone: nop\n"
        } else {
            ".text\n.globl _start\n_start: b _start\ndone: nop\n"
        };
        let (mut session, _) = fixture(target, source, 30)?;
        session.run_until(&[Address::new(0x8000)])?;
        assert!(session.record_trace(true).is_err());
        assert!(session.clear_trace().is_err());
        session.pause()?;
        session.record_trace(true)?;
        session.step()?;
        settle(&mut session)?;
        assert_eq!(session.state(), ExecutionState::Paused(PauseReason::Step));
        session.run_until(&[Address::new(0x8000)])?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Budget)
        );
        assert_eq!(session.instructions(), 30);
        assert_eq!(session.trace().entries().len(), 30);
    }
    Ok(())
}

#[test]
fn step_over_distinguishes_recursive_visits_to_the_same_return_address() -> TestResult {
    for (target, source) in [
        (
            Target::X86_64,
            ".intel_syntax noprefix\n.text\n.globl _start\n_start: lea rsp, [rip + stack_end]\ncall recur\njmp done\nrecur: cmp eax, 3\nje base\ninc eax\ncallsite: call recur\nreturned: nop\nbase: ret\ndone: nop\n.bss\n.balign 16\n.space 4096\nstack_end:\n",
        ),
        (
            Target::Aarch64,
            ".text\n.globl _start\n_start: adrp x9, stack_end\nadd x9, x9, :lo12:stack_end\nmov sp, x9\nbl recur\nb done\nrecur: stp x29, x30, [sp, -16]!\ncmp x0, 3\nb.eq base\nadd x0, x0, 1\ncallsite: bl recur\nreturned: nop\nbase: ldp x29, x30, [sp], 16\nret\ndone: nop\n.bss\n.balign 16\n.space 4096\nstack_end:\n",
        ),
    ] {
        let (mut session, image) = fixture(target, source, 100)?;
        let call = symbol(&image, "callsite")?;
        let returned = symbol(&image, "returned")?;
        session.run_until(&[call])?;
        settle(&mut session)?;
        assert_eq!(first_integer(&session)?, 1);
        session.record_trace(true)?;
        session.step_over()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Paused(PauseReason::Target(returned))
        );
        assert_eq!(first_integer(&session)?, 3);
        assert_eq!(
            session
                .trace()
                .entries()
                .filter(|entry| entry.pc == returned)
                .count(),
            2
        );
        session.start()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
    }
    Ok(())
}

#[test]
fn step_over_at_mapping_edges_preserves_state_on_decode_failure() -> TestResult {
    for (target, offset, invalid, nop) in [
        (Target::X86_64, 4095, &[0x0f][..], &[0x90][..]),
        (
            Target::Aarch64,
            4092,
            &[0xff; 4][..],
            &[0x1f, 0x20, 0x03, 0xd5][..],
        ),
    ] {
        let mut bytes = vec![0; 4096];
        bytes[offset..].copy_from_slice(invalid);
        let entry = Address::new(0x1000 + u64::try_from(offset)?);
        let machine = Machine::load(
            Image::Raw {
                bytes: &bytes,
                base: Address::new(0x1000),
                entry,
            },
            target,
            MachineSetup::default(),
        )?;
        let mut session = Session::new(machine, Address::new(0x3000), 10)?;
        session.record_trace(true)?;
        assert!(session.step_over().is_err());
        assert_eq!(session.state(), ExecutionState::Ready);
        assert_eq!(session.instructions(), 0);
        assert_eq!(session.read_registers()?.instruction_pointer(), entry);
        assert_eq!(session.trace().entries().len(), 0);

        session.write_memory(entry, nop)?;
        session.step_over()?;
        settle(&mut session)?;
        assert_eq!(session.state(), ExecutionState::Paused(PauseReason::Step));
        assert_eq!(
            session.read_registers()?.instruction_pointer(),
            Address::new(0x2000)
        );
        assert_eq!(session.trace().entries().len(), 1);
        assert_eq!(
            session.trace().entries().next().map(|entry| entry.pc),
            Some(entry)
        );
        assert!(session.step_over().is_err());
        assert_eq!(session.state(), ExecutionState::Paused(PauseReason::Step));
        session.step()?;
        settle(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::GuestFault)
        );
        assert_eq!(session.instructions(), 1);
        assert_eq!(session.trace().entries().len(), 1);
    }
    Ok(())
}

#[test]
fn step_over_decodes_an_instruction_spanning_adjacent_mappings() -> TestResult {
    let mut bytes = vec![0; 4096];
    bytes[4095] = 0xb8; // MOV EAX, imm32; the immediate resides in the next mapping.
    let machine = Machine::load(
        Image::Raw {
            bytes: &bytes,
            base: Address::new(0x1000),
            entry: Address::new(0x1fff),
        },
        Target::X86_64,
        MachineSetup {
            registers: None,
            mappings: vec![InitialMapping {
                range: AddressRange::new(Address::new(0x2000), 4096, 4096)?,
                permissions: Permissions {
                    read: true,
                    write: false,
                    execute: true,
                },
                bytes: vec![42, 0, 0, 0],
            }],
        },
    )?;
    let mut session = Session::new(machine, Address::new(0x3000), 10)?;
    session.step_over()?;
    settle(&mut session)?;
    assert_eq!(session.state(), ExecutionState::Paused(PauseReason::Step));
    assert_eq!(first_integer(&session)?, 42);
    assert_eq!(
        session.read_registers()?.instruction_pointer(),
        Address::new(0x2004)
    );
    Ok(())
}
