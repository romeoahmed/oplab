use super::*;
use oplab_core::protocol::{Response, scalar::Counter};
use std::{collections::BTreeSet, sync::mpsc, thread, time::Duration};

fn message(id: u64, result: Reply) -> Message {
    Message {
        response: Response {
            id: Counter::new(id),
            result,
        },
        payloads: Vec::new(),
    }
}

fn bulk(id: u64) -> Message {
    message(id, Reply::Decoded(Vec::new()))
}

fn control(id: u64) -> Message {
    builds::error(
        Counter::new(id),
        Diagnostic::new(DiagnosticCode::InvalidInput),
    )
}

#[test]
fn controls_overtake_bulk_and_shutdown_drains_every_reply_once() -> Result<(), WorkerError> {
    let (sender, receiver) = channel();
    let capacity = BULK_CAPACITY as u64;
    for id in 0..capacity {
        sender.send(bulk(id), false)?;
    }
    sender.send(control(capacity), false)?;
    sender.send(bulk(capacity + 1), false)?;
    sender.send(message(capacity + 2, Reply::Closed), false)?;
    let mut delivered = BTreeSet::new();
    let mut bulk_started = false;
    while let Some(message) = receiver.receive_reply()? {
        let id = message.response.id.get();
        assert!(delivered.insert(id), "duplicate reply");
        match message.response.result {
            Reply::Error(error) => {
                assert!(!bulk_started, "bulk delivery delayed a control");
                if id == capacity + 1 {
                    assert_eq!(error.code, DiagnosticCode::ResourceLimit);
                    assert!(message.payloads.is_empty());
                }
            }
            Reply::Decoded(_) => bulk_started = true,
            Reply::Closed => assert_eq!(delivered.len(), BULK_CAPACITY + 3),
            reply => panic!("unexpected reply: {reply:?}"),
        }
    }
    assert_eq!(delivered, (0..capacity + 3).collect());
    assert!(sender.send(control(capacity + 3), false).is_err());
    Ok(())
}

#[test]
fn reads_cannot_consume_an_active_builds_reserved_slot() -> Result<(), WorkerError> {
    let (sender, receiver) = channel();
    let reserved = BULK_CAPACITY as u64 - 1;
    for id in 0..reserved {
        sender.send(bulk(id), false)?;
    }
    assert!(sender.can_start_build()?);
    sender.send(bulk(reserved), true)?;
    sender.send(bulk(reserved + 1), false)?;
    assert!(!sender.can_start_build()?);
    drop(sender);
    let mut delivered = BTreeSet::new();
    while let Some(message) = receiver.receive_reply()? {
        let id = message.response.id.get();
        if id == reserved {
            assert!(
                matches!(message.response.result, Reply::Error(error) if error.code == DiagnosticCode::ResourceLimit)
            );
        } else {
            assert!(matches!(message.response.result, Reply::Decoded(_)));
        }
        assert!(delivered.insert(id));
    }
    assert_eq!(delivered, (0..reserved + 2).collect());
    Ok(())
}

#[test]
fn a_lost_writer_releases_a_saturated_control_sender() -> Result<(), Box<dyn std::error::Error>> {
    thread::scope(|scope| {
        let (sender, receiver) = channel();
        for id in 0..CONTROL_CAPACITY as u64 {
            sender.send(control(id), false)?;
        }
        let (finished, result) = mpsc::sync_channel(1);
        scope.spawn(move || {
            let _ = finished.send(sender.send(control(CONTROL_CAPACITY as u64), false));
        });
        drop(receiver);
        assert!(result.recv_timeout(Duration::from_secs(2))?.is_err());
        Ok(())
    })
}

impl Receiver {
    pub(in crate::worker) fn receive_reply(&self) -> Result<Option<Message>, WorkerError> {
        self.receive()?
            .map(|delivery| match delivery {
                Delivery::Reply(message) => Ok(message),
                Delivery::Observation(_) => Err(WorkerError::IoThread),
            })
            .transpose()
    }
}
