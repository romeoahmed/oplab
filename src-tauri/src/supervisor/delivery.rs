//! One Channel observation awaiting acknowledgement and one latest pending sample.

use oplab_core::protocol::{
    desktop::{DesktopFailure, FailureCode},
    scalar::Counter,
    stream::ObservationUpdate,
    transport::{self, StreamMessage},
};
use tauri::ipc::{Channel, InvokeResponseBody};

pub(super) struct Delivery {
    channel: Channel,
    waiting: Option<(Counter, Counter)>,
    latest: Option<StreamMessage>,
}

impl Delivery {
    pub(super) const fn new(channel: Channel) -> Self {
        Self {
            channel,
            waiting: None,
            latest: None,
        }
    }

    pub(super) fn offer(&mut self, message: StreamMessage) -> Result<(), DesktopFailure> {
        self.latest = Some(message);
        self.flush()
    }

    pub(super) fn acknowledge(
        &mut self,
        subscription: Counter,
        sequence: Counter,
    ) -> Result<(), DesktopFailure> {
        if self.waiting != Some((subscription, sequence)) {
            return Err(DesktopFailure::new(FailureCode::Protocol));
        }
        self.waiting = None;
        self.flush()
    }

    fn flush(&mut self) -> Result<(), DesktopFailure> {
        if self.waiting.is_some() {
            return Ok(());
        }
        let Some(message) = self.latest.take() else {
            return Ok(());
        };
        let sequence = match &message.event.update {
            ObservationUpdate::Full(observation) => observation.sequence,
            ObservationUpdate::Ended(_) => Counter::new(0),
            ObservationUpdate::Delta(_) => return Err(DesktopFailure::new(FailureCode::Protocol)),
        };
        let mut bytes = Vec::new();
        transport::write_stream(&mut bytes, &message.event, message.memory.as_deref())
            .map_err(|_| DesktopFailure::new(FailureCode::Protocol))?;
        self.channel
            .send(InvokeResponseBody::Raw(bytes))
            .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?;
        self.waiting = Some((message.event.subscription, sequence));
        Ok(())
    }

    pub(super) fn failed(&self, code: FailureCode) {
        // A single small terminal notification is independent of observation credit.
        if let Ok(json) = serde_json::to_string(&DesktopFailure::new(code)) {
            let _ = self.channel.send(InvokeResponseBody::Json(json));
        }
    }
}
