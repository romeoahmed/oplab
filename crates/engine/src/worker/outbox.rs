//! Bounded reply lanes. Priority changes message order, never binary frame order.

use super::{WorkerError, builds, stream::Pending, transport::Message};
use oplab_core::protocol::{Diagnostic, DiagnosticCode, Reply, execution::Status};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
};

const CONTROL_CAPACITY: usize = 4;
const BULK_CAPACITY: usize = 4;

#[derive(Default)]
struct State {
    control: VecDeque<Message>,
    bulk: VecDeque<Message>,
    final_reply: Option<Message>,
    observation: Option<Pending>,
    closed: bool,
    failed: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

/// The dispatcher is the sole producer and owns the active-build reservation.
pub(super) struct Sender(Arc<Shared>);
pub(super) struct Receiver(Arc<Shared>);

pub(super) enum Delivery {
    Reply(Message),
    Observation(Pending),
}

pub(super) fn channel() -> (Sender, Receiver) {
    let shared = Arc::new(Shared::default());
    (Sender(Arc::clone(&shared)), Receiver(shared))
}

impl Sender {
    /// A native job starts only when its future artifact has a reserved bulk slot.
    pub(super) fn can_start_build(&self) -> Result<bool, WorkerError> {
        let state = self.0.state.lock().map_err(|_| WorkerError::IoThread)?;
        if state.failed || state.closed {
            return Err(WorkerError::IoThread);
        }
        Ok(state.bulk.len() < BULK_CAPACITY)
    }

    /// Replace the pending observation; `None` clears it without dropping correlated replies.
    pub(super) fn observe(&self, pending: Option<Pending>) -> Result<(), WorkerError> {
        let mut state = self.0.state.lock().map_err(|_| WorkerError::IoThread)?;
        if state.failed || state.closed {
            return Err(WorkerError::IoThread);
        }
        state.observation = pending;
        drop(state);
        self.0.changed.notify_all();
        Ok(())
    }

    /// Preserve every request outcome. A saturated bulk lane becomes an explicit
    /// resource error; control saturation applies backpressure instead of dropping it.
    pub(super) fn send(
        &self,
        mut message: Message,
        reserve_build: bool,
    ) -> Result<(), WorkerError> {
        let mut state = self.0.state.lock().map_err(|_| WorkerError::IoThread)?;
        if state.failed || state.closed {
            return Err(WorkerError::IoThread);
        }
        if matches!(message.response.result, Reply::Closed) {
            state.final_reply = Some(message);
            state.closed = true;
        } else if is_bulk(&message) && state.bulk.len() + usize::from(reserve_build) < BULK_CAPACITY
        {
            state.bulk.push_back(message);
        } else {
            if is_bulk(&message) {
                message = builds::error(
                    message.response.id,
                    Diagnostic::new(DiagnosticCode::ResourceLimit),
                );
            }
            state = self
                .0
                .changed
                .wait_while(state, |state| {
                    state.control.len() == CONTROL_CAPACITY && !state.failed && !state.closed
                })
                .map_err(|_| WorkerError::IoThread)?;
            if state.failed || state.closed {
                return Err(WorkerError::IoThread);
            }
            state.control.push_back(message);
        }
        drop(state);
        self.0.changed.notify_all();
        Ok(())
    }
}

impl Receiver {
    pub(super) fn receive(&self) -> Result<Option<Delivery>, WorkerError> {
        let mut state = self.0.state.lock().map_err(|_| WorkerError::IoThread)?;
        loop {
            if state.failed {
                return Err(WorkerError::IoThread);
            }
            let delivery = state.control.pop_front().map(Delivery::Reply).or_else(|| {
                if state.observation.as_ref().is_some_and(Pending::priority) {
                    state.observation.take().map(Delivery::Observation)
                } else {
                    state
                        .bulk
                        .pop_front()
                        .map(Delivery::Reply)
                        .or_else(|| state.observation.take().map(Delivery::Observation))
                }
            });
            if let Some(message) = delivery {
                drop(state);
                self.0.changed.notify_all();
                return Ok(Some(message));
            }
            if state.closed {
                return Ok(state.final_reply.take().map(Delivery::Reply));
            }
            state = self
                .0
                .changed
                .wait(state)
                .map_err(|_| WorkerError::IoThread)?;
        }
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.closed = true;
        }
        self.0.changed.notify_all();
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.failed = true;
            state.control.clear();
            state.bulk.clear();
            state.final_reply = None;
            state.observation = None;
        }
        self.0.changed.notify_all();
    }
}

fn is_bulk(message: &Message) -> bool {
    match &message.response.result {
        Reply::Assembled(_) | Reply::Decoded(_) | Reply::Analyzed(_) => true,
        Reply::Observed(observation) => {
            observation.memory.is_some()
                && matches!(observation.status, Status::Ready | Status::Running)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/outbox.rs"]
mod tests;
