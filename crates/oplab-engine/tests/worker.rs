//! Worker framing and process behavior under fragmented, malformed, and bounded input.

use object::{Object, ObjectSection};
use oplab_core::address::Address;
use oplab_core::protocol::transport;
use oplab_core::protocol::{
    BuildIdentity, Command, Reply, Request, VERSION,
    frame::Kind,
    scalar::{Counter, HexAddress},
};
use oplab_core::target::Target;
use oplab_engine::worker::{Worker, WorkerError};
mod common;

use std::io::Cursor;

#[test]
fn protocol_negotiation_and_request_order_are_enforced() -> Result<(), WorkerError> {
    let mut worker = Worker::default();
    let request = Request {
        id: Counter::new(1),
        command: Command::Hello { version: VERSION },
    };
    assert!(matches!(
        worker.handle(request.clone().into())?.response.result,
        Reply::Hello(_)
    ));
    assert!(matches!(
        worker.handle(request.into()),
        Err(WorkerError::RequestOrder)
    ));
    let mut incompatible = Worker::default();
    assert!(matches!(
        incompatible
            .handle(
                Request {
                    id: Counter::new(1),
                    command: Command::Hello {
                        version: VERSION + 1
                    }
                }
                .into()
            )?
            .response
            .result,
        Reply::Error(_)
    ));
    assert!(matches!(
        incompatible.handle(
            Request {
                id: Counter::new(2),
                command: Command::Shutdown
            }
            .into()
        ),
        Err(WorkerError::Closed)
    ));
    Ok(())
}

#[test]
fn real_worker_process_handshakes_and_exits_on_shutdown() -> Result<(), Box<dyn std::error::Error>>
{
    let mut input = Vec::new();
    let assemble = |target| Command::Assemble {
        identity: BuildIdentity {
            document: "pipe-fixture".into(),
            revision: Counter::new(1),
            target,
            base: HexAddress::new(Address::new(0x1000)),
            assembler: oplab_engine::assembly::identity(),
        },
        source: if target == Target::X86_64 {
            ".section .debug_padding,\"\"\n.space 70000, 0x5a\n.text\nnop".into()
        } else {
            "nop".into()
        },
    };
    for (id, command) in [
        (1, Command::Hello { version: VERSION }),
        (2, assemble(Target::X86_64)),
        (3, assemble(Target::Aarch64)),
        (4, Command::Shutdown),
    ] {
        let body = serde_json::to_vec(&Request {
            id: Counter::new(id),
            command,
        })?;
        transport::write_frame(&mut input, Kind::Control, &body)?;
    }
    let output = common::run(env!("CARGO_BIN_EXE_oplab-worker"), &[], &input)?;
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let mut replies = Cursor::new(output.stdout);
    let hello = transport::read_message(&mut replies)?
        .ok_or("missing handshake")?
        .response;
    assert!(matches!(hello.result, Reply::Hello(_)));
    for (id, expected) in [(2, vec![0x90]), (3, vec![0x1f, 0x20, 0x03, 0xd5])] {
        let message = transport::read_message(&mut replies)?.ok_or("missing assembly reply")?;
        let response = message.response;
        assert_eq!(response.id, Counter::new(id));
        let Reply::Assembled(artifact) = response.result else {
            return Err("missing worker artifact".into());
        };
        assert_eq!(message.payloads[0].len(), artifact.object_bytes as usize);
        if id == 2 {
            let object = object::File::parse(message.payloads[0].as_slice())?;
            assert_eq!(
                object
                    .section_by_name(".debug_padding")
                    .ok_or("missing original debug section")?
                    .data()?,
                vec![0x5a; 70000]
            );
        }
        let image = object::File::parse(message.payloads[1].as_slice())?;
        assert_eq!(
            image
                .section_by_name(".text")
                .ok_or("missing text")?
                .data()?,
            expected
        );
    }
    let closed = transport::read_message(&mut replies)?
        .ok_or("missing shutdown")?
        .response;
    assert_eq!(closed.id, Counter::new(4));
    assert_eq!(closed.result, Reply::Closed);
    assert_eq!(transport::read_frame(&mut replies)?, None);
    Ok(())
}

#[test]
fn pipelined_cancellation_has_one_result_per_request_and_drains_before_shutdown()
-> Result<(), Box<dyn std::error::Error>> {
    use oplab_core::{address::Address, protocol::DiagnosticCode};
    let mut input = Vec::new();
    let base = HexAddress::new(Address::new(0x1000));
    for (id, command) in [
        (1, Command::Hello { version: VERSION }),
        (
            2,
            Command::Assemble {
                identity: BuildIdentity {
                    document: "pipeline".into(),
                    revision: Counter::new(1),
                    target: Target::X86_64,
                    base,
                    assembler: oplab_engine::assembly::identity(),
                },
                source: "nop".into(),
            },
        ),
        (
            3,
            Command::CancelAssembly {
                request: Counter::new(2),
            },
        ),
        (
            4,
            Command::Decode {
                target: Target::X86_64,
                base,
                bytes: vec![0x90],
                limit: 1,
            },
        ),
        (5, Command::Shutdown),
    ] {
        transport::write_request(
            &mut input,
            &Request {
                id: Counter::new(id),
                command,
            }
            .into(),
        )?;
    }
    let output = common::run(env!("CARGO_BIN_EXE_oplab-worker"), &[], &input)?;
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let mut reader = Cursor::new(output.stdout);
    let mut replies = std::collections::BTreeMap::new();
    let mut last = 0;
    while let Some(message) = transport::read_message(&mut reader)? {
        last = message.response.id.get();
        assert!(replies.insert(last, message.response.result).is_none());
    }
    assert_eq!(last, 5);
    assert_eq!(replies.len(), 5);
    assert!(matches!(replies.get(&1), Some(Reply::Hello(_))));
    match (replies.get(&2), replies.get(&3)) {
        (Some(Reply::Assembled(_)), Some(Reply::Error(error))) => {
            assert_eq!(error.code, DiagnosticCode::InvalidInput);
        }
        (Some(Reply::Error(error)), Some(Reply::AssemblyCancelled(id))) => {
            assert_eq!(error.code, DiagnosticCode::Cancelled);
            assert_eq!(id.get(), 2);
        }
        _ => return Err("invalid cancellation outcome pair".into()),
    }
    assert!(matches!(replies.get(&4), Some(Reply::Decoded(_))));
    assert_eq!(replies.get(&5), Some(&Reply::Closed));
    Ok(())
}

#[test]
fn rejected_handshake_exits_without_waiting_for_input_to_close()
-> Result<(), Box<dyn std::error::Error>> {
    use std::{
        process::{Command as ProcessCommand, Stdio},
        time::{Duration, Instant},
    };
    let mut process = common::ProcessGuard(
        ProcessCommand::new(env!("CARGO_BIN_EXE_oplab-worker"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let mut input = process.0.stdin.take().ok_or("missing stdin")?;
    transport::write_request(
        &mut input,
        &Request {
            id: Counter::new(1),
            command: Command::Hello {
                version: VERSION + 1,
            },
        }
        .into(),
    )?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = process.0.try_wait()? {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            return Err("handshake rejection waited for input EOF".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut output = process.0.stdout.take().ok_or("missing stdout")?;
    let message = transport::read_message(&mut output)?.ok_or("missing rejection")?;
    assert!(matches!(message.response.result, Reply::Error(ref error)
        if error.code == oplab_core::protocol::DiagnosticCode::Protocol));
    assert!(transport::read_message(&mut output)?.is_none());
    drop(input); // Deliberately retained until after the child exited.
    Ok(())
}
