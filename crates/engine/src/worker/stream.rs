//! Coalesce complete captures; form deltas only against the writer's delivered base.

use super::{Worker, WorkerError, transport::Message};
use oplab_core::protocol::{
    Command, Diagnostic, Reply, Request,
    execution::{MemoryWindow, Observation, SessionAction, SessionKey, Status},
    scalar::Counter,
    stream::{ObservationDelta, ObservationUpdate, RegisterUpdate, StreamEvent},
};
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_millis(33);

/// Immutable coherent values; only this complete form may replace a queued sample.
#[derive(Debug)]
pub(super) struct Capture {
    pub(super) subscription: Counter,
    pub(super) observation: Box<Observation>,
    pub(super) memory: Option<Vec<u8>>,
}

#[derive(Debug)]
pub(super) enum Pending {
    Capture(Capture),
    Ended {
        subscription: Counter,
        error: Diagnostic,
    },
}

impl Pending {
    pub(super) fn priority(&self) -> bool {
        match self {
            Self::Capture(capture) => {
                !matches!(capture.observation.status, Status::Ready | Status::Running)
            }
            Self::Ended { .. } => true,
        }
    }
}

pub(super) struct Subscription {
    pub(super) id: Counter,
    pub(super) key: SessionKey,
    memory: Option<MemoryWindow>,
    pub(super) next: Option<Instant>,
}

impl Subscription {
    pub(super) const fn new(id: Counter, key: SessionKey, memory: Option<MemoryWindow>) -> Self {
        Self {
            id,
            key,
            memory,
            next: None,
        }
    }

    pub(super) fn capture(
        &mut self,
        worker: &mut Worker,
    ) -> Result<Result<Capture, Diagnostic>, WorkerError> {
        // This internal read reuses the subscription identity only at the owner
        // handoff. Its response is consumed here and never settles a wire request.
        let Message {
            response,
            mut payloads,
        } = worker.perform(
            Request {
                id: self.id,
                command: Command::Execute {
                    session: self.key,
                    action: SessionAction::Observe {
                        memory: self.memory,
                    },
                },
            }
            .into(),
        )?;
        match response.result {
            Reply::Observed(observation) => {
                self.next =
                    (observation.status == Status::Running).then(|| Instant::now() + INTERVAL);
                Ok(Ok(Capture {
                    subscription: self.id,
                    observation,
                    memory: payloads.pop(),
                }))
            }
            Reply::Error(error) => Ok(Err(error)),
            _ => Err(WorkerError::ExecutionLost),
        }
    }
}

/// Owned exclusively by the writer. Advance only after a complete successful write.
#[derive(Default)]
pub(super) struct Encoder {
    base: Option<Capture>,
}

impl Encoder {
    pub(super) fn write(
        &mut self,
        output: &mut impl std::io::Write,
        pending: Pending,
    ) -> Result<(), WorkerError> {
        let capture = match pending {
            Pending::Capture(capture) => capture,
            Pending::Ended {
                subscription,
                error,
            } => {
                super::transport::write_stream(
                    output,
                    &StreamEvent {
                        subscription,
                        update: ObservationUpdate::Ended(error),
                    },
                    None,
                )?;
                self.base = None;
                return Ok(());
            }
        };
        let event = self.event(&capture);
        let memory = match &event.update {
            ObservationUpdate::Delta(delta) if delta.memory_bytes == 0 => None,
            _ => capture.memory.as_deref(),
        };
        super::transport::write_stream(output, &event, memory)?;
        self.base = Some(capture);
        Ok(())
    }

    fn event(&self, capture: &Capture) -> StreamEvent {
        let current = &capture.observation;
        let update = self
            .base
            .as_ref()
            .filter(|base| {
                base.subscription == capture.subscription
                    && base.observation.key == current.key
                    && base.observation.memory == current.memory
                    && base.observation.breakpoints == current.breakpoints
                    && base.observation.sequence < current.sequence
            })
            .map_or_else(
                || ObservationUpdate::Full(current.clone()),
                |base| {
                    ObservationUpdate::Delta(Box::new(ObservationDelta {
                        key: current.key,
                        base: base.observation.sequence,
                        sequence: current.sequence,
                        status: current.status,
                        instructions: current.instructions,
                        dispatches: current.dispatches,
                        registers: if base.observation.registers == current.registers {
                            RegisterUpdate::Unchanged
                        } else {
                            RegisterUpdate::Replace(current.registers.clone().map(Box::new))
                        },
                        fault: current.fault,
                        memory_bytes: if base.memory == capture.memory {
                            0
                        } else {
                            current.memory.map_or(0, |window| window.length)
                        },
                    }))
                },
            );
        StreamEvent {
            subscription: capture.subscription,
            update,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/stream.rs"]
mod tests;
