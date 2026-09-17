//! Observable process contracts: real ELF execution, attachment leases, and bounded delivery.

use crate::supervisor::{Service, Ticket};
use oplab_core::{
    address::Address,
    execution::Termination,
    protocol::{
        BuildIdentity, Command, Reply,
        desktop::{ConnectionInfo, FailureCode},
        execution::{MemoryWindow, SessionAction, Status},
        scalar::{Counter, HexAddress},
        stream::ObservationUpdate,
        transport::{self, Message, Output, StreamMessage},
    },
    target::Target,
};
use std::{io::Cursor, sync::mpsc, time::Duration};
use tauri::ipc::{Channel, InvokeResponseBody};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn channel() -> (Channel, mpsc::Receiver<InvokeResponseBody>) {
    let (sender, receiver) = mpsc::channel();
    (
        Channel::new(move |body| {
            let _ = sender.send(body);
            Ok(())
        }),
        receiver,
    )
}

fn request(
    service: &Service,
    info: &ConnectionInfo,
    command: Command,
    image: Option<Vec<u8>>,
) -> TestResult<Message> {
    service
        .request(info.connection, info.view, command, image)
        .and_then(Ticket::wait)
        .map_err(|error| format!("{error:?}").into())
}

fn event(receiver: &mpsc::Receiver<InvokeResponseBody>) -> TestResult<StreamMessage> {
    let InvokeResponseBody::Raw(bytes) = receiver.recv_timeout(Duration::from_secs(5))? else {
        return Err("worker failed".into());
    };
    let Some(Output::Observation(message)) = transport::read_output(&mut Cursor::new(bytes))?
    else {
        return Err("expected observation".into());
    };
    Ok(message)
}

#[test]
fn desktop_executes_standard_images_and_reattaches_without_replaying_mutations() -> TestResult {
    let executable = std::env::current_exe()?;
    let worker = executable
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or("test output directory unavailable")?
        .join(format!("oplab-worker{}", std::env::consts::EXE_SUFFIX));
    let runtime = std::path::PathBuf::from(
        std::env::var_os("OPLAB_QEMU_DIR").ok_or("QEMU SDK directory missing")?,
    );
    for (target, source) in [
        (
            Target::X86_64,
            "mov eax, 42\nmov [rip + output], rax\ndone: nop\n.bss\noutput: .skip 8",
        ),
        (
            Target::Aarch64,
            "mov x0, #42\nadr x1, output\nstr x0, [x1]\ndone: nop\n.bss\noutput: .skip 8",
        ),
    ] {
        let service = Service::default();
        let (events, receiver) = channel();
        let info = service
            .connect(&worker, &runtime, events, false)
            .map_err(|error| format!("{error:?}"))?;
        let (key, window) = load_example(&service, &info, target, source)?;
        request(
            &service,
            &info,
            Command::Subscribe {
                session: key,
                memory: Some(window),
            },
            None,
        )?;
        let first = event(&receiver)?;
        let ObservationUpdate::Full(initial) = first.event.update else {
            return Err("expected full capture".into());
        };
        assert_eq!(first.memory, Some(vec![0; 8]));
        // Withhold Channel credit. Execution and reset replies must remain independent.
        request(
            &service,
            &info,
            Command::Execute {
                session: key,
                action: SessionAction::Run,
            },
            None,
        )?;
        assert_completed(&service, &info, key, window)?;
        assert!(receiver.try_recv().is_err());
        service
            .acknowledge(
                info.connection,
                info.view,
                first.event.subscription,
                initial.sequence,
            )
            .map_err(|error| format!("{error:?}"))?;
        let latest = event(&receiver)?;
        assert!(matches!(latest.event.update, ObservationUpdate::Full(_)));
        service.detach(info.connection, info.view);
        assert!(
            matches!(service.request(info.connection, info.view, Command::Execute { session: key, action: SessionAction::Reset }, None), Err(error) if error.code == FailureCode::StaleConnection)
        );
        let (events, _) = channel();
        let attached = service
            .connect(&worker, &runtime, events, false)
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(attached.connection, info.connection);
        assert_ne!(attached.view, info.view);
        assert_eq!(
            attached
                .session
                .as_ref()
                .ok_or("missing restored session")?
                .key,
            key
        );
        assert!(
            matches!(service.request(info.connection, info.view, Command::Execute { session: key, action: SessionAction::Reset }, None),
            Err(error) if error.code == FailureCode::StaleConnection)
        );
        let reset = request(
            &service,
            &attached,
            Command::Execute {
                session: key,
                action: SessionAction::Reset,
            },
            None,
        )?;
        let Reply::Observed(reset) = reset.response.result else {
            return Err("reset failed".into());
        };
        assert_ne!(reset.key.generation, key.generation);
        assert_eq!(reset.instructions.get(), 0);
        service.shutdown();
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn unresponsive_child_is_killed_reaped_and_admitted_outcome_is_unknown() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let temporary = tempfile::tempdir()?;
    let executable = temporary.path().join("unresponsive-worker");
    std::fs::write(
        &executable,
        "#!/bin/sh\nprintf '%s' \"$$\" > \"$0.pid\"\nexec sleep 30\n",
    )?;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))?;
    let service = Service::default();
    let (events, _) = channel();
    let failure = service
        .connect(&executable, temporary.path(), events, false)
        .err()
        .ok_or("unresponsive worker connected")?;
    assert_eq!(failure.code, FailureCode::Deadline);
    assert!(failure.outcome_unknown);
    service.shutdown();
    let pid = std::fs::read_to_string(temporary.path().join("unresponsive-worker.pid"))?;
    let pid = sysinfo::Pid::from_u32(pid.parse()?);
    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    assert!(system.process(pid).is_none(), "worker survived shutdown");
    Ok(())
}

fn load_example(
    service: &Service,
    info: &ConnectionInfo,
    target: Target,
    source: &str,
) -> TestResult<(oplab_core::protocol::execution::SessionKey, MemoryWindow)> {
    let assembled = request(
        service,
        info,
        Command::Assemble {
            identity: BuildIdentity {
                document: "desktop-test".into(),
                revision: Counter::new(1),
                target,
                base: HexAddress::new(Address::new(0x1000)),
                assembler: info.capabilities.assembler.clone(),
            },
            source: source.into(),
        },
        None,
    )?;
    let Reply::Assembled(artifact) = assembled.response.result else {
        return Err("assembly failed".into());
    };
    let symbol = |name| {
        artifact
            .image
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .map(|symbol| symbol.address)
            .ok_or("missing ELF symbol")
    };
    let window = MemoryWindow {
        address: symbol("output")?,
        length: 8,
    };
    let loaded = request(
        service,
        info,
        Command::Load {
            image: oplab_core::protocol::execution::LoadImage::Elf,
            initial: oplab_core::protocol::execution::InitialState::default(),
            replace: None,
            target,
            completion: symbol("done")?,
            instruction_budget: Counter::new(100),
            image_bytes: artifact.image_bytes,
        },
        assembled.payloads.into_iter().nth(1),
    )?;
    let Reply::Observed(initial) = loaded.response.result else {
        return Err("load failed".into());
    };
    Ok((initial.key, window))
}

fn assert_completed(
    service: &Service,
    info: &ConnectionInfo,
    key: oplab_core::protocol::execution::SessionKey,
    window: MemoryWindow,
) -> TestResult {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let observed = request(
            service,
            info,
            Command::Execute {
                session: key,
                action: SessionAction::Observe {
                    memory: Some(window),
                },
            },
            None,
        )?;
        let Reply::Observed(observation) = observed.response.result else {
            return Err("observe failed".into());
        };
        if observation.status != Status::Running {
            assert_eq!(
                observation.status,
                Status::Terminated(Termination::Completed)
            );
            assert_eq!(observed.payloads, [vec![42, 0, 0, 0, 0, 0, 0, 0]]);
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("execution did not complete".into());
        }
    }
    Ok(())
}
