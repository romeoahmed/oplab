//! Real worker sessions: identity, ordering, coherent observations, and owner lifetime.

#[path = "common/interactive.rs"]
mod interactive;
use interactive::{Client, TestResult};
use object::{Object, ObjectSymbol};
use oplab_core::protocol::transport::Message;
use oplab_core::{
    address::Address,
    execution::Termination,
    protocol::{
        Command, DiagnosticCode, Reply, VERSION,
        execution::{MemoryWindow, Observation, Registers, SessionAction, SessionKey, Status},
        scalar::{Counter, HexAddress},
    },
    target::Target,
};
use oplab_engine::assembly;

const fn address(value: u64) -> HexAddress {
    HexAddress::new(Address::new(value))
}

fn fixture(target: Target, source: &str) -> TestResult<(Vec<u8>, HexAddress, HexAddress)> {
    let object = assembly::compile(target, source).map_err(|error| format!("{error:?}"))?;
    let image =
        assembly::link(&object, Address::new(0x1000)).map_err(|error| format!("{error:?}"))?;
    let elf = object::File::parse(image.as_slice())?;
    let symbol = |name| {
        elf.symbols()
            .find(|symbol| symbol.name() == Ok(name))
            .map(|symbol| address(symbol.address()))
            .ok_or("missing symbol")
    };
    let done = symbol("done")?;
    let output = symbol("output")?;
    Ok((image, done, output))
}

fn observed(message: &Message) -> TestResult<&Observation> {
    if let Reply::Observed(observation) = &message.response.result {
        Ok(observation)
    } else {
        Err(format!(
            "expected observation, received {:?}",
            message.response.result
        )
        .into())
    }
}

fn execute(client: &mut Client, session: SessionKey, action: SessionAction) -> TestResult<Message> {
    client.request(Command::Execute { session, action }, None)
}

fn load(
    client: &mut Client,
    target: Target,
    image: &[u8],
    completion: HexAddress,
    budget: u64,
    replace: Option<SessionKey>,
) -> TestResult<Message> {
    client.request(
        Command::Load {
            replace,
            target,
            completion,
            instruction_budget: Counter::new(budget),
            image_bytes: u32::try_from(image.len())?,
        },
        Some(image.to_vec()),
    )
}

fn settle(client: &mut Client, key: SessionKey) -> TestResult<Message> {
    for _ in 0..1024 {
        let message = execute(client, key, SessionAction::Observe { memory: None })?;
        if observed(&message)?.status != Status::Running {
            return Ok(message);
        }
    }
    Err("execution did not settle".into())
}

fn integer(observation: &Observation) -> TestResult<u64> {
    Ok(
        match observation.registers.as_ref().ok_or("missing registers")? {
            Registers::X86_64 { gpr, .. } => gpr[0].get(),
            Registers::Aarch64 { x, .. } => x[0].get(),
        },
    )
}

#[test]
fn both_guests_preserve_session_identity_observation_order_and_reset() -> TestResult {
    for (target, source) in [
        (
            Target::X86_64,
            "mov eax, 42\nmov byte ptr [rip + output], al\ndone: nop\n.bss\noutput: .skip 8",
        ),
        (
            Target::Aarch64,
            "mov x0, 42\nadr x1, output\nstrb w0, [x1]\ndone: nop\n.bss\noutput: .skip 8",
        ),
    ] {
        let (image, completion, output) = fixture(target, source)?;
        interactive::worker(|client| {
            let hello = client.request(Command::Hello { version: VERSION }, None)?;
            assert!(matches!(hello.response.result, Reply::Hello(ref caps) if caps.execution));
            let loaded = load(client, target, &image, completion, 100, None)?;
            let initial = observed(&loaded)?;
            let key = initial.key;
            assert_eq!(key.session, loaded.response.id);
            assert_eq!(key.generation.get(), 0);
            assert_eq!(initial.status, Status::Ready);
            execute(client, key, SessionAction::Step)?;
            let stepped = settle(client, key)?;
            assert_eq!(observed(&stepped)?.status, Status::Stepped);
            assert_eq!(integer(observed(&stepped)?)?, 42);
            execute(client, key, SessionAction::Run)?;
            let completed = settle(client, key)?;
            assert_eq!(
                observed(&completed)?.status,
                Status::Terminated(Termination::Completed)
            );
            let window = Some(MemoryWindow {
                address: output,
                length: 8,
            });
            let memory = execute(client, key, SessionAction::Observe { memory: window })?;
            assert_eq!(memory.payloads, [vec![42, 0, 0, 0, 0, 0, 0, 0]]);
            assert_eq!(integer(observed(&memory)?)?, 42);
            let reset = execute(client, key, SessionAction::Reset)?;
            let fresh = observed(&reset)?.key;
            assert_eq!(fresh.session, key.session);
            assert_eq!(fresh.generation.get(), 1);
            assert!(observed(&reset)?.sequence > observed(&memory)?.sequence);
            assert_eq!(integer(observed(&reset)?)?, 0);
            assert_eq!(observed(&reset)?.instructions.get(), 0);
            assert!(
                matches!(execute(client, key, SessionAction::Run)?.response.result,
                Reply::Error(ref error) if error.code == DiagnosticCode::StaleSession)
            );
            let memory = execute(client, fresh, SessionAction::Observe { memory: window })?;
            assert_eq!(observed(&memory)?.status, Status::Ready);
            assert_eq!(memory.payloads, [vec![0; 8]]);
            // Failed replacement retains both the initial machine and its identity.
            let invalid = load(client, target, &[0], completion, 100, Some(fresh))?;
            assert!(matches!(invalid.response.result, Reply::Error(_)));
            assert_eq!(
                observed(&execute(
                    client,
                    fresh,
                    SessionAction::Observe { memory: None }
                )?)?
                .key,
                fresh
            );
            assert_eq!(
                execute(client, fresh, SessionAction::Close)?
                    .response
                    .result,
                Reply::SessionClosed(fresh)
            );
            assert!(
                matches!(execute(client, fresh, SessionAction::Reset)?.response.result,
                Reply::Error(ref error) if error.code == DiagnosticCode::StaleSession)
            );
            let reloaded = load(client, target, &image, completion, 100, None)?;
            assert_ne!(observed(&reloaded)?.key.session, fresh.session);
            assert_eq!(
                client.request(Command::Shutdown, None)?.response.result,
                Reply::Closed
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn running_observations_are_coherent_and_controls_do_not_wait_for_completion() -> TestResult {
    for (target, source) in [
        (
            Target::X86_64,
            "again: inc rax\nstore: mov [rip + output], rax\njmp again\ndone: nop\n.bss\noutput: .skip 8",
        ),
        (
            Target::Aarch64,
            "adr x1, output\nagain: add x0, x0, 1\nstore: str x0, [x1]\nb again\ndone: nop\n.bss\noutput: .skip 8",
        ),
    ] {
        let (image, completion, output) = fixture(target, source)?;
        let elf = object::File::parse(image.as_slice())?;
        let store_address = elf
            .symbols()
            .find(|symbol| symbol.name() == Ok("store"))
            .map(|symbol| address(symbol.address()))
            .ok_or("missing store")?;
        interactive::worker(|client| {
            client.request(Command::Hello { version: VERSION }, None)?;
            let loaded = load(client, target, &image, completion, 100_000_000, None)?;
            let key = observed(&loaded)?.key;
            execute(
                client,
                key,
                SessionAction::Breakpoint {
                    address: store_address,
                    enabled: true,
                },
            )?;
            execute(client, key, SessionAction::Run)?;
            let stopped = settle(client, key)?;
            assert_eq!(
                observed(&stopped)?.status,
                Status::Breakpoint(store_address)
            );
            let memory = Some(MemoryWindow {
                address: output,
                length: 8,
            });
            let before = execute(client, key, SessionAction::Observe { memory })?;
            assert_eq!(integer(observed(&before)?)?, 1);
            assert_eq!(before.payloads, [vec![0; 8]]);
            execute(
                client,
                key,
                SessionAction::Breakpoint {
                    address: store_address,
                    enabled: false,
                },
            )?;
            execute(client, key, SessionAction::Run)?;
            let mut sequence = observed(&before)?.sequence;
            for _ in 0..4 {
                let message = execute(client, key, SessionAction::Observe { memory })?;
                let observation = observed(&message)?;
                assert_eq!(observation.status, Status::Running);
                assert!(observation.sequence > sequence);
                sequence = observation.sequence;
                let register = integer(observation)?;
                let written = u64::from_le_bytes(message.payloads[0].as_slice().try_into()?);
                let pc = match observation.registers.as_ref().ok_or("missing registers")? {
                    Registers::X86_64 { rip, .. } => *rip,
                    Registers::Aarch64 { pc, .. } => *pc,
                };
                assert_eq!(written, register - u64::from(pc == store_address));
            }
            let paused = execute(client, key, SessionAction::Pause)?;
            assert_eq!(observed(&paused)?.status, Status::Paused);
            let cancelled = execute(client, key, SessionAction::Cancel)?;
            assert_eq!(
                observed(&cancelled)?.status,
                Status::Terminated(Termination::Cancelled)
            );
            let reset = execute(client, key, SessionAction::Reset)?;
            execute(client, observed(&reset)?.key, SessionAction::Run)?;
            // Shutdown must disconnect and join an actively slicing native owner.
            assert_eq!(
                client.request(Command::Shutdown, None)?.response.result,
                Reply::Closed
            );
            Ok(())
        })?;
    }
    Ok(())
}

fn streamed_state(
    message: &oplab_core::protocol::transport::StreamMessage,
) -> TestResult<(Counter, Status)> {
    use oplab_core::protocol::stream::ObservationUpdate;
    match &message.event.update {
        ObservationUpdate::Full(observation) => Ok((observation.sequence, observation.status)),
        ObservationUpdate::Delta(delta) => Ok((delta.sequence, delta.status)),
        ObservationUpdate::Ended(error) => Err(format!("capture failed: {error:?}").into()),
    }
}

fn stream_until(
    client: &mut Client,
    subscription: Counter,
    mut sequence: Counter,
    status: Status,
) -> TestResult<Counter> {
    use oplab_core::protocol::stream::ObservationUpdate;
    for _ in 0..32 {
        let message = client.event()?;
        assert_eq!(message.event.subscription, subscription);
        if let ObservationUpdate::Delta(delta) = &message.event.update {
            assert_eq!(delta.base, sequence);
        }
        let (next, current) = streamed_state(&message)?;
        assert!(next > sequence);
        sequence = next;
        if current == status {
            return Ok(sequence);
        }
    }
    Err("stream did not reach expected state".into())
}

fn exercise_subscription(
    client: &mut Client,
    target: Target,
    image: &[u8],
    completion: HexAddress,
    output: HexAddress,
) -> TestResult {
    use oplab_core::protocol::stream::ObservationUpdate;
    client.request(Command::Hello { version: VERSION }, None)?;
    let loaded = load(client, target, image, completion, 100_000_000, None)?;
    let key = observed(&loaded)?.key;
    let subscribe = |session, length| Command::Subscribe {
        session,
        memory: Some(MemoryWindow {
            address: output,
            length,
        }),
    };
    let ack = client.request(subscribe(key, 8), None)?;
    let id = ack.response.id;
    assert_eq!(ack.response.result, Reply::Subscribed(id));
    let initial = client.event()?;
    assert!(
        matches!(&initial.event.update, ObservationUpdate::Full(observation) if observation.key == key)
    );
    assert_eq!(initial.memory.as_deref(), Some([0; 8].as_slice()));
    let mut sequence = streamed_state(&initial)?.0;
    execute(client, key, SessionAction::Run)?;
    // Receive an immediate running capture and a later timer-driven update without
    // requesting Observe. Memory stores trail the increment by at most one instruction.
    let mut registers = observed(&loaded)?.registers.clone();
    let mut memory = [0_u8; 8];
    for _ in 0..2 {
        let message = client.event()?;
        let ObservationUpdate::Delta(delta) = message.event.update else {
            return Err("missing delta".into());
        };
        assert_eq!(delta.base, sequence);
        assert!(delta.sequence > sequence);
        assert_eq!(delta.status, Status::Running);
        if let oplab_core::protocol::stream::RegisterUpdate::Replace(bank) = delta.registers {
            registers = bank.map(|bank| *bank);
        }
        if let Some(bytes) = message.memory {
            memory.copy_from_slice(&bytes);
        }
        let value = match registers.as_ref().ok_or("missing stream registers")? {
            Registers::X86_64 { gpr, .. } => gpr[0].get(),
            Registers::Aarch64 { x, .. } => x[0].get(),
        };
        let stored = u64::from_le_bytes(memory);
        assert!(value >= stored && value - stored <= 1);
        sequence = delta.sequence;
    }
    execute(client, key, SessionAction::Pause)?;
    stream_until(client, id, sequence, Status::Paused)?;
    let reset = execute(client, key, SessionAction::Reset)?;
    let next_key = observed(&reset)?.key;
    assert_ne!(next_key, key);
    let stale = client.request(subscribe(key, 8), None)?;
    assert!(
        matches!(stale.response.result, Reply::Error(error) if error.code == DiagnosticCode::StaleSession)
    );
    let ack = client.request(subscribe(next_key, 4), None)?;
    let next_id = ack.response.id;
    let initial = client.event()?;
    assert_eq!(initial.event.subscription, next_id);
    assert!(
        matches!(&initial.event.update, ObservationUpdate::Full(observation) if observation.key == next_key)
    );
    assert_eq!(initial.memory.as_deref(), Some([0; 4].as_slice()));
    let sequence = streamed_state(&initial)?.0;
    // Invalid replacement windows leave the current subscription alive.
    let invalid = client.request(subscribe(next_key, 0), None)?;
    assert!(
        matches!(invalid.response.result, Reply::Error(error) if error.code == DiagnosticCode::InvalidInput)
    );
    execute(client, next_key, SessionAction::Cancel)?;
    stream_until(
        client,
        next_id,
        sequence,
        Status::Terminated(Termination::Cancelled),
    )?;
    let stopped = client.request(
        Command::Unsubscribe {
            subscription: next_id,
        },
        None,
    )?;
    assert_eq!(stopped.response.result, Reply::Unsubscribed(next_id));
    let stale = client.request(Command::Unsubscribe { subscription: id }, None)?;
    assert!(
        matches!(stale.response.result, Reply::Error(error) if error.code == DiagnosticCode::InvalidInput)
    );
    execute(client, next_key, SessionAction::Close)?;
    assert_eq!(
        client.request(Command::Shutdown, None)?.response.result,
        Reply::Closed
    );
    Ok(())
}

#[test]
fn subscriptions_stream_both_guests_and_restart_cleanly_after_reset() -> TestResult {
    for (target, source) in [
        (
            Target::X86_64,
            "again: inc rax\nmov qword ptr [rip + output], rax\njmp again\ndone: nop\n.bss\noutput: .skip 8",
        ),
        (
            Target::Aarch64,
            "adr x1, output\nagain: add x0, x0, #1\nstr x0, [x1]\nb again\ndone: nop\n.bss\noutput: .skip 8",
        ),
    ] {
        let (image, completion, output) = fixture(target, source)?;
        interactive::worker(|client| {
            exercise_subscription(client, target, &image, completion, output)
        })?;
    }
    Ok(())
}
