//! Reconstruct worker deltas before coalescing delivery to a slower `WebView`.

use oplab_core::protocol::{
    desktop::{DesktopFailure, FailureCode},
    execution::{Observation, Status},
    stream::{ObservationUpdate, RegisterUpdate, StreamEvent},
    transport::StreamMessage,
};

#[derive(Default)]
pub(super) struct Cache {
    current: Option<StreamMessage>,
}

impl Cache {
    pub(super) fn apply(
        &mut self,
        message: StreamMessage,
    ) -> Result<StreamMessage, DesktopFailure> {
        let StreamMessage { event, memory } = message;
        match event.update {
            ObservationUpdate::Full(observation) => {
                self.store(event.subscription, observation, memory)
            }
            ObservationUpdate::Ended(error) => {
                self.current = None;
                Ok(StreamMessage {
                    event: StreamEvent {
                        subscription: event.subscription,
                        update: ObservationUpdate::Ended(error),
                    },
                    memory: None,
                })
            }
            ObservationUpdate::Delta(delta) => {
                let base = self
                    .current
                    .as_ref()
                    .filter(|base| base.event.subscription == event.subscription)
                    .ok_or_else(protocol)?;
                let ObservationUpdate::Full(observation) = &base.event.update else {
                    return Err(protocol());
                };
                let matches_base = delta.base == observation.sequence;
                if delta.key != observation.key
                    || !matches_base
                    || delta.sequence <= delta.base
                    || delta.instructions < observation.instructions
                    || delta.dispatches < observation.dispatches
                    || (delta.memory_bytes != 0
                        && observation.memory.map(|window| window.length)
                            != Some(delta.memory_bytes))
                {
                    return Err(protocol());
                }
                let next = Box::new(Observation {
                    key: delta.key,
                    sequence: delta.sequence,
                    status: delta.status,
                    instructions: delta.instructions,
                    dispatches: delta.dispatches,
                    registers: match delta.registers {
                        RegisterUpdate::Unchanged => observation.registers.clone(),
                        RegisterUpdate::Replace(bank) => bank.map(|bank| *bank),
                    },
                    fault: delta.fault,
                    memory: observation.memory,
                });
                let memory = if delta.memory_bytes == 0 {
                    base.memory.clone()
                } else {
                    memory
                };
                self.store(event.subscription, next, memory)
            }
        }
    }

    fn store(
        &mut self,
        subscription: oplab_core::protocol::scalar::Counter,
        observation: Box<Observation>,
        memory: Option<Vec<u8>>,
    ) -> Result<StreamMessage, DesktopFailure> {
        if (observation.status == Status::Crashed) != observation.registers.is_none()
            || observation.memory.map(|window| window.length as usize)
                != memory.as_ref().map(Vec::len)
        {
            return Err(protocol());
        }
        let message = StreamMessage {
            event: StreamEvent {
                subscription,
                update: ObservationUpdate::Full(observation),
            },
            memory,
        };
        self.current = Some(StreamMessage {
            event: message.event.clone(),
            memory: message.memory.clone(),
        });
        Ok(message)
    }
}

const fn protocol() -> DesktopFailure {
    DesktopFailure::new(FailureCode::Protocol)
}
