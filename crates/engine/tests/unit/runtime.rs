use super::*;
use oplab_core::{
    address::Address,
    protocol::{BuildIdentity, Request, VERSION, scalar::HexAddress},
    target::Target,
};
use std::time::Duration;

#[test]
fn pending_builds_resume_on_output_credit_without_another_request()
-> Result<(), Box<dyn std::error::Error>> {
    thread::scope(|scope| {
        let (events, receiver) = mpsc::sync_channel(8);
        let (output, outbound) = outbox::channel();
        let mut bulk_ids = Vec::new();
        while output.can_start_build()? {
            let id = 10 + u64::try_from(bulk_ids.len())?;
            output.send(reply(Counter::new(id), Reply::Decoded(Vec::new())), false)?;
            bulk_ids.push(id);
            assert!(bulk_ids.len() < 64, "unbounded fixture queue");
        }
        let mut dispatcher = Dispatcher::default();
        dispatcher.request(
            Request {
                id: Counter::new(1),
                command: Command::Hello { version: VERSION },
            }
            .into(),
            &output,
        )?;
        assert!(matches!(
            outbound
                .receive_reply()?
                .ok_or("missing hello")?
                .response
                .result,
            Reply::Hello(_)
        ));
        dispatcher.request(
            Request {
                id: Counter::new(2),
                command: Command::Assemble {
                    identity: BuildIdentity {
                        document: "backpressure".into(),
                        revision: Counter::new(0),
                        target: Target::X86_64,
                        base: HexAddress::new(Address::new(0x1000)),
                        assembler: crate::assembly::identity(),
                    },
                    source: "nop".into(),
                },
            }
            .into(),
            &output,
        )?;
        let (jobs, pending) = mpsc::sync_channel(1);
        let dispatcher = scope.spawn(move || dispatcher.run(&receiver, &output, &jobs));
        let (forward, replies) = mpsc::sync_channel(1);
        scope.spawn(move || {
            while let Ok(Some(message)) = outbound.receive_reply() {
                if forward.send(message).is_err() {
                    break;
                }
            }
        });
        // The dispatcher has no incoming request. Only returned output credit
        // wakes it to start the already-admitted assembly.
        let first = replies.recv_timeout(Duration::from_secs(5))?;
        assert!(bulk_ids.contains(&first.response.id.get()));
        events.send(Event::Writable)?;
        let job = pending.recv_timeout(Duration::from_secs(5))?;
        assert_eq!(job.id.get(), 2);
        events.send(Event::Build(builds::run(job)))?;
        events.send(Event::Request(Box::new(
            Request {
                id: Counter::new(4),
                command: Command::Shutdown,
            }
            .into(),
        )))?;
        let mut delivered = std::collections::BTreeSet::from([first.response.id.get()]);
        loop {
            let message = replies.recv_timeout(Duration::from_secs(5))?;
            assert!(
                delivered.insert(message.response.id.get()),
                "duplicate reply"
            );
            if message.response.result == Reply::Closed {
                assert_eq!(message.response.id.get(), 4);
                break;
            }
        }
        bulk_ids.extend([2, 4]);
        assert_eq!(delivered, bulk_ids.into_iter().collect());
        assert!(dispatcher.join().map_err(|_| "dispatcher failed")??);
        Ok(())
    })
}

#[test]
fn controls_and_cancellation_settle_while_the_native_job_is_outstanding()
-> Result<(), Box<dyn std::error::Error>> {
    thread::scope(|scope| {
        let (events, receiver) = mpsc::sync_channel(8);
        let (output, outbound) = outbox::channel();
        let (forward, replies) = mpsc::sync_channel(8);
        scope.spawn(move || {
            while let Ok(Some(message)) = outbound.receive_reply() {
                if forward.send(message).is_err() {
                    break;
                }
            }
        });
        let (jobs, pending) = mpsc::sync_channel(1);
        let dispatcher = scope.spawn(move || Dispatcher::default().run(&receiver, &output, &jobs));
        let send_request = |id, command| {
            events
                .send(Event::Request(Box::new(RequestMessage::from(Request {
                    id: Counter::new(id),
                    command,
                }))))
                .map_err(|_| "fixture event queue closed")
        };
        send_request(1, Command::Hello { version: VERSION })?;
        assert!(matches!(
            replies
                .recv_timeout(Duration::from_secs(2))?
                .response
                .result,
            Reply::Hello(_)
        ));
        send_request(
            2,
            Command::Assemble {
                identity: BuildIdentity {
                    document: "pending".into(),
                    revision: Counter::new(0),
                    target: Target::X86_64,
                    base: HexAddress::new(Address::new(0x1000)),
                    assembler: crate::assembly::identity(),
                },
                source: "nop".into(),
            },
        )?;
        let native_job = pending.recv_timeout(Duration::from_secs(2))?;
        // Hold the real job at the thread handoff. No fabricated native result
        // can unblock the controls below; the dispatcher must remain independent.
        send_request(
            3,
            Command::Decode {
                target: Target::X86_64,
                base: HexAddress::new(Address::new(0x1000)),
                bytes: vec![0x90],
                limit: 1,
            },
        )?;
        let decoded = replies.recv_timeout(Duration::from_secs(2))?;
        assert_eq!(decoded.response.id.get(), 3);
        assert!(matches!(decoded.response.result, Reply::Decoded(_)));
        send_request(
            4,
            Command::CancelAssembly {
                request: Counter::new(2),
            },
        )?;
        let mut cancellation = std::collections::BTreeMap::new();
        for _ in 0..2 {
            let message = replies.recv_timeout(Duration::from_secs(5))?;
            assert!(
                cancellation
                    .insert(message.response.id.get(), message.response.result)
                    .is_none()
            );
        }
        assert!(
            matches!(cancellation.get(&2), Some(Reply::Error(error)) if error.code == DiagnosticCode::Cancelled)
        );
        assert_eq!(
            cancellation.get(&4),
            Some(&Reply::AssemblyCancelled(Counter::new(2)))
        );
        send_request(5, Command::Shutdown)?;
        events.send(Event::Build(builds::run(native_job)))?;
        let closed = replies.recv_timeout(Duration::from_secs(2))?;
        assert_eq!(closed.response.id.get(), 5);
        assert_eq!(closed.response.result, Reply::Closed);
        assert!(dispatcher.join().map_err(|_| "dispatcher failed")??);
        assert!(replies.try_recv().is_err());
        Ok(())
    })
}
