use super::*;
use oplab_core::{
    address::Address,
    protocol::{BuildIdentity, Request, VERSION, scalar::HexAddress},
    target::Target,
};
use std::time::Duration;

#[test]
fn pending_builds_resume_when_bulk_output_space_becomes_available()
-> Result<(), Box<dyn std::error::Error>> {
    thread::scope(|scope| {
        let (events, receiver) = mpsc::sync_channel(8);
        let (output, replies) = outbox::channel();
        for id in 10..14 {
            output.send(reply(Counter::new(id), Reply::Decoded(Vec::new())), false)?;
        }
        let mut dispatcher = Dispatcher::default();
        for (id, command) in [
            (1, Command::Hello { version: VERSION }),
            (
                2,
                Command::Assemble {
                    identity: BuildIdentity {
                        document: "backpressure".into(),
                        revision: Counter::new(0),
                        target: Target::X86_64,
                        base: HexAddress::new(Address::new(0x1000)),
                        assembler: crate::assembly::identity(),
                    },
                    source: "nop".into(),
                },
            ),
        ] {
            dispatcher.request(
                Request {
                    id: Counter::new(id),
                    command,
                }
                .into(),
                &output,
            )?;
        }
        // Admit the prefix synchronously so the test does not assume how soon
        // the scheduler runs its background thread relative to the timeout.
        let (jobs, pending) = mpsc::sync_channel(1);
        let dispatcher = scope.spawn(move || dispatcher.run(&receiver, &output, &jobs));
        events.send(Event::Writable)?;
        // The native job cannot start while its result would have no slot.
        assert!(matches!(
            pending.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert_eq!(
            replies
                .receive_reply()?
                .ok_or("missing hello")?
                .response
                .id
                .get(),
            1
        );
        assert_eq!(
            replies
                .receive_reply()?
                .ok_or("missing bulk")?
                .response
                .id
                .get(),
            10
        );
        events.send(Event::Writable)?;
        let job = pending.recv_timeout(Duration::from_secs(2))?;
        assert_eq!(job.id.get(), 2);
        events.send(Event::Build(builds::run(job)))?;
        events.send(Event::Request(Box::new(RequestMessage::from(Request {
            id: Counter::new(3),
            command: Command::Shutdown,
        }))))?;
        assert!(dispatcher.join().map_err(|_| "dispatcher failed")??);
        let mut order = Vec::new();
        while let Some(message) = replies.receive_reply()? {
            order.push(message.response.id.get());
        }
        assert_eq!(order.last(), Some(&3), "shutdown preceded a pending reply");
        order.sort_unstable();
        assert_eq!(order, [2, 3, 11, 12, 13]);
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
        let cancelled = replies.recv_timeout(Duration::from_secs(2))?;
        assert_eq!(cancelled.response.id.get(), 2);
        assert!(
            matches!(cancelled.response.result, Reply::Error(ref error) if error.code == DiagnosticCode::Cancelled)
        );
        assert_eq!(
            replies
                .recv_timeout(Duration::from_secs(2))?
                .response
                .result,
            Reply::AssemblyCancelled(Counter::new(2))
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
