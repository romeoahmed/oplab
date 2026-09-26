//! Real guest access semantics, including helper paths and resumable REP effects.
use object::{Object, ObjectSymbol};
use oplab_core::{
    address::Address,
    execution::{Access, ExecutionState, FaultKind, PauseReason, Termination},
    target::Target,
    watchpoint::{WatchAccess, Watchpoint, WatchpointHit},
};
use oplab_engine::session::Session;
use oplab_toolchain::assembly;
use proptest::prelude::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
fn symbol(image: &[u8], name: &str) -> Result<Address> {
    object::File::parse(image)?
        .symbols()
        .find(|s| s.name() == Ok(name))
        .map(|s| Address::new(s.address()))
        .ok_or_else(|| "missing symbol".into())
}
fn fixture(target: Target, source: &str) -> Result<(Session, Vec<u8>)> {
    let source = if target == Target::Aarch64 {
        format!(".arch armv9-a\n{source}")
    } else {
        source.into()
    };
    let object = assembly::compile(target, &source).map_err(|e| format!("{e:?}"))?;
    let image = assembly::link(&object, Address::new(0x1000)).map_err(|e| format!("{e:?}"))?;
    Ok((
        Session::from_elf(&image, target, symbol(&image, "done")?, 1000)?,
        image,
    ))
}
fn run(session: &mut Session) -> Result {
    session.start()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while session.state() == ExecutionState::Running {
        if std::time::Instant::now() >= deadline {
            return Err("execution deadline".into());
        }
        session.advance()?;
    }
    Ok(())
}
fn hit(session: &Session) -> Result<WatchpointHit> {
    if let ExecutionState::Paused(PauseReason::Watchpoint(hit)) = session.state() {
        Ok(hit)
    } else {
        Err(format!("expected watchpoint, got {:?}", session.state()).into())
    }
}

#[test]
fn stores_pause_after_effects_resume_without_replay_and_survive_reset() -> Result {
    for (target, source) in [
        (
            Target::X86_64,
            "lea rdi, [rip + data]\nstore: inc qword ptr [rdi]\ndone: nop\n.data\ndata: .quad 0",
        ),
        (
            Target::Aarch64,
            "adr x1, data\nmov x0, 1\nstore: str x0, [x1]\ndone: nop\n.data\ndata: .quad 0",
        ),
    ] {
        let (mut session, image) = fixture(target, source)?;
        let data = symbol(&image, "data")?;
        let point = Watchpoint::new(data, 8, WatchAccess::Write)?;
        session.set_watchpoints(&[point, point])?;
        assert_eq!(session.watchpoints(), &[point]);
        // Host inspection and patches do not count as guest data accesses.
        session.write_memory(data, &0_u64.to_le_bytes())?;
        for _ in 0..2 {
            run(&mut session)?;
            let stopped = hit(&session)?;
            assert_eq!(stopped.watchpoint, point);
            assert_eq!(stopped.pc, symbol(&image, "store")?);
            assert_eq!(stopped.address, data);
            assert_eq!(stopped.access, WatchAccess::Write);
            assert_eq!(session.read_memory(data, 8)?, 1_u64.to_le_bytes());
            assert_eq!(
                session.read_registers()?.instruction_pointer(),
                symbol(&image, "done")?
            );
            run(&mut session)?;
            assert_eq!(
                session.state(),
                ExecutionState::Terminated(Termination::Completed)
            );
            assert_eq!(session.read_memory(data, 8)?, 1_u64.to_le_bytes());
            session.reset()?;
            assert_eq!(session.watchpoints(), &[point]);
        }
    }
    Ok(())
}

#[test]
fn loads_at_page_edges_match_overlapping_ranges_and_unwatch_flushes_all_pages() -> Result {
    for (target, code) in [
        (
            Target::X86_64,
            "lea rdi, [rip + data]\nload: mov rax, [rdi]\ndone: nop",
        ),
        (
            Target::Aarch64,
            "adr x1, data\nload: ldr x0, [x1]\ndone: nop",
        ),
    ] {
        let (mut session, image) = fixture(
            target,
            &format!("{code}\n.data\n.balign 4096\n.zero 4092\ndata: .quad 42\n.zero 4096"),
        )?;
        let data = symbol(&image, "data")?;
        // Fetches and loads never match write-only watchpoints.
        let write = Watchpoint::new(Address::new(data.get() - 4092), 8192, WatchAccess::Write)?;
        session.set_watchpoints(&[write])?;
        run(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed),
            "{target:?}: {:?}",
            session.fault()
        );
        session.reset()?;
        let read = Watchpoint::new(data.checked_add(6)?, 1, WatchAccess::Read)?;
        let overlap = Watchpoint::new(data.checked_add(7)?, 1, WatchAccess::ReadWrite)?;
        session.set_watchpoints(&[overlap, write, read])?;
        run(&mut session)?;
        let stopped = hit(&session)?;
        assert_eq!(stopped.watchpoint, read);
        assert_eq!(stopped.address, read.address());
        assert_eq!(stopped.pc, symbol(&image, "load")?);
        session.set_watchpoints(&[])?;
        session.write_register(
            if target == Target::X86_64 {
                "rip"
            } else {
                "pc"
            },
            symbol(&image, "load")?.get(),
        )?;
        run(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed),
            "{target:?}: {:?}",
            session.fault()
        );
    }
    Ok(())
}

#[test]
fn rep_watchpoints_preserve_iteration_progress_and_count_one_instruction() -> Result {
    let (mut session, image) = fixture(
        Target::X86_64,
        "lea rdi, [rip + data]\nmov ecx, 3\nmov al, 7\nstore: rep stosb\ndone: nop\n.data\ndata: .zero 8",
    )?;
    let data = symbol(&image, "data")?;
    session.set_watchpoints(&[Watchpoint::new(data, 3, WatchAccess::Write)?])?;
    for index in 0..3 {
        run(&mut session)?;
        assert_eq!(hit(&session)?.address, data.checked_add(index)?);
        assert_eq!(hit(&session)?.pc, symbol(&image, "store")?);
        assert_eq!(session.instructions(), 4);
        let bytes = session.read_memory(data, 3)?;
        let mut expected = [0; 3];
        expected[..usize::try_from(index + 1)?].fill(7);
        assert_eq!(bytes, expected);
    }
    run(&mut session)?;
    assert_eq!(
        session.state(),
        ExecutionState::Terminated(Termination::Completed)
    );
    assert_eq!(session.instructions(), 4);
    Ok(())
}

#[test]
fn native_accesses_include_same_value_stores_vectors_and_atomics() -> Result {
    for (target, code, expected) in [
        (
            Target::X86_64,
            "lea rdi, [rip + data]\nstore: mov qword ptr [rdi], 0",
            0_u64.to_le_bytes().to_vec(),
        ),
        (
            Target::Aarch64,
            "adr x1, data\nstore: str xzr, [x1]",
            0_u64.to_le_bytes().to_vec(),
        ),
        (
            Target::X86_64,
            "lea rdi, [rip + data]\nvpcmpeqd ymm0, ymm0, ymm0\nstore: vmovdqu [rdi], ymm0",
            vec![0xff; 32],
        ),
        (
            Target::Aarch64,
            "adr x1, data\nptrue p0.b\nmov z0.b, -1\nstore: st1b {z0.b}, p0, [x1]",
            vec![0xff; 256],
        ),
        (
            Target::X86_64,
            "lea rdi, [rip + data]\nstore: lock inc qword ptr [rdi]",
            1_u64.to_le_bytes().to_vec(),
        ),
        (
            Target::Aarch64,
            "adr x1, data\nmov x0, 1\nstore: ldadd x0, x2, [x1]",
            1_u64.to_le_bytes().to_vec(),
        ),
    ] {
        let (mut session, image) = fixture(
            target,
            &format!("{code}\ndone: nop\n.data\n.balign 16\ndata: .zero 256"),
        )?;
        let data = symbol(&image, "data")?;
        let size = u64::try_from(expected.len())?;
        let last = data.checked_add(size - 1)?;
        session.set_watchpoints(&[Watchpoint::new(last, 1, WatchAccess::ReadWrite)?])?;
        run(&mut session)?;
        let stopped = hit(&session)?;
        assert_eq!(stopped.pc, symbol(&image, "store")?);
        assert_eq!(stopped.address, last);
        assert_eq!(session.read_memory(data, size)?, expected);
        assert_eq!(
            session.read_registers()?.instruction_pointer(),
            symbol(&image, "done")?
        );
    }
    Ok(())
}

#[test]
fn faults_override_prior_data_matches() -> Result {
    let (mut session, image) = fixture(
        Target::X86_64,
        "lea rsi, [rip + data]\nmov edi, 0xdead000\nmovsb\ndone: nop\n.data\ndata: .byte 42",
    )?;
    session.set_watchpoints(&[Watchpoint::new(
        symbol(&image, "data")?,
        1,
        WatchAccess::Read,
    )?])?;
    run(&mut session)?;
    assert_eq!(
        session.state(),
        ExecutionState::Terminated(Termination::GuestFault)
    );
    let fault = session.fault().ok_or("missing memory fault")?;
    assert_eq!(fault.kind, FaultKind::Unmapped(Access::Write));
    assert_eq!(fault.address, Some(Address::new(0x0dea_d000)));
    Ok(())
}

#[test]
fn full_watchpoint_sets_ignore_fetches_and_reject_overflow_atomically() -> Result {
    for target in [Target::X86_64, Target::Aarch64] {
        let (mut session, _) = fixture(target, "nop\nnop\ndone: nop")?;
        let points: Vec<_> = (0..32)
            .map(|offset| Watchpoint::new(Address::new(0x1000 + offset), 1, WatchAccess::ReadWrite))
            .collect::<std::result::Result<_, _>>()?;
        session.set_watchpoints(&points)?;
        let mut overflow = points.clone();
        overflow.push(Watchpoint::new(Address::new(0x1020), 1, WatchAccess::Read)?);
        assert!(session.set_watchpoints(&overflow).is_err());
        assert_eq!(session.watchpoints(), points);
        run(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
        session.reset()?;
        session.set_watchpoints(&[])?;
        assert!(session.watchpoints().is_empty());
        run(&mut session)?;
        assert_eq!(
            session.state(),
            ExecutionState::Terminated(Termination::Completed)
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]
    #[test]
    fn stores_match_range_intersections_and_preserve_exact_bytes(
        offset in 0_usize..32, start in 0_usize..48, length in 1_u32..17,
        access in prop::sample::select(vec![WatchAccess::Read, WatchAccess::Write, WatchAccess::ReadWrite]),
    ) {
        for (target, code) in [
            (Target::X86_64, format!("lea rdi, [rip + data]\nmov rax, 0x0101010101010101\nstore: mov [rdi + {offset}], rax")),
            (Target::Aarch64, format!("adr x1, data\nmov x0, 0x0101010101010101\nstore: stur x0, [x1, {offset}]")),
        ] {
            let (mut session, image) = fixture(target, &format!("{code}\ndone: nop\n.data\ndata: .zero 64")).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let address = symbol(&image, "data").map_err(|e| TestCaseError::fail(e.to_string()))?;
            let point = Watchpoint::new(address.checked_add(u64::try_from(start)?)?, length, access)?;
            session.set_watchpoints(&[point])?;
            run(&mut session).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let end = start + usize::try_from(length)?;
            let overlaps = (offset..offset + 8).any(|byte| (start..end).contains(&byte));
            if access != WatchAccess::Read && overlaps {
                let stopped = hit(&session).map_err(|e| TestCaseError::fail(e.to_string()))?;
                prop_assert_eq!(stopped.watchpoint, point);
                prop_assert_eq!(stopped.access, WatchAccess::Write);
                prop_assert_eq!(stopped.address, address.checked_add(u64::try_from(offset.max(start))?)?);
                prop_assert_eq!(stopped.pc, symbol(&image, "store").map_err(|e| TestCaseError::fail(e.to_string()))?);
            } else {
                prop_assert_eq!(session.state(), ExecutionState::Terminated(Termination::Completed));
            }
            let mut expected = [0; 64];
            expected[offset..offset + 8].fill(1);
            prop_assert_eq!(session.read_memory(address, 64)?, expected);
        }
    }
}

#[test]
fn inactive_sve_lanes_do_not_trigger_and_mandatory_alignment_still_faults() -> Result {
    for (code, termination) in [
        (
            "adr x1, data\npfalse p0.b\nld1b {z0.b}, p0/z, [x1]",
            Termination::Completed,
        ),
        (
            "adr x1, data\nadd x1, x1, 1\nldxp x0, x2, [x1]",
            Termination::GuestFault,
        ),
    ] {
        let (mut session, image) = fixture(
            Target::Aarch64,
            &format!("{code}\ndone: nop\n.data\n.balign 16\ndata: .zero 256"),
        )?;
        session.set_watchpoints(&[Watchpoint::new(
            symbol(&image, "data")?,
            256,
            WatchAccess::ReadWrite,
        )?])?;
        run(&mut session)?;
        assert_eq!(session.state(), ExecutionState::Terminated(termination));
        if termination == Termination::GuestFault {
            assert!(matches!(
                session.fault().map(|fault| fault.kind),
                Some(FaultKind::Unaligned(Access::Read))
            ));
        }
    }
    Ok(())
}
