//! Process I/O and assembly scheduling. Only the dispatcher owns reply decisions.

use super::{
    Worker, WorkerError, builds, outbox, stream,
    transport::{self, Message, RequestMessage},
};
use oplab_core::protocol::{
    Command, Diagnostic, DiagnosticCode, Reply, Response, execution::SessionAction, scalar::Counter,
};
use std::{
    io,
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::Instant,
};

enum Event {
    Request(Box<RequestMessage>),
    InputClosed,
    Writable,
    Build(Message),
    Failed(WorkerError),
}

pub(super) fn serve() -> Result<(), WorkerError> {
    let (events, receiver) = mpsc::sync_channel(16);
    let (output, replies) = outbox::channel();
    let (jobs, pending) = mpsc::sync_channel(1);
    let reading = {
        let events = events.clone();
        thread::Builder::new()
            .name("oplab-input".into())
            .spawn(move || read_input(&events))
            .map_err(|_| WorkerError::IoThread)?
    };
    let writing = {
        let events = events.clone();
        thread::Builder::new()
            .name("oplab-output".into())
            .spawn(move || write_output(replies, &events))
            .map_err(|_| WorkerError::IoThread)?
    };
    let building = thread::Builder::new()
        .name("oplab-assembly".into())
        .spawn(move || compile(&pending, &events))
        .map_err(|_| WorkerError::BuildLost)?;
    let result = Dispatcher::default().run(&receiver, &output, &jobs);
    // Wake blocked producers/receivers before joining. On fatal errors, standard
    // input or native code may still be blocked; the binary exits this process.
    drop(receiver);
    drop(jobs);
    drop(output);
    if result? {
        reading.join().map_err(|_| WorkerError::IoThread)?;
    }
    building.join().map_err(|_| WorkerError::BuildLost)?;
    writing.join().map_err(|_| WorkerError::IoThread)??;
    Ok(())
}

fn read_input(events: &SyncSender<Event>) {
    let mut input = io::stdin().lock();
    loop {
        match transport::read_request(&mut input) {
            Ok(Some(request)) => {
                // Shutdown is the final input request. Do not wait for a later
                // pipe close while the client waits for the shutdown reply.
                let shutdown = request.request.command == Command::Shutdown;
                if events.send(Event::Request(Box::new(request))).is_err() || shutdown {
                    return;
                }
            }
            Ok(None) => {
                let _ = events.send(Event::InputClosed);
                return;
            }
            Err(error) => {
                let _ = events.send(Event::Failed(error.into()));
                return;
            }
        }
    }
}

fn write_output(replies: outbox::Receiver, events: &SyncSender<Event>) -> Result<(), WorkerError> {
    let result = write_messages(&replies, events);
    // Dropping the receiver wakes a dispatcher waiting for control capacity,
    // including when the queue itself failed before any frame could be written.
    drop(replies);
    if result.is_err() {
        let _ = events.send(Event::Failed(WorkerError::IoThread));
    }
    result
}

fn write_messages(
    replies: &outbox::Receiver,
    events: &SyncSender<Event>,
) -> Result<(), WorkerError> {
    let mut output = io::stdout().lock();
    let mut encoder = stream::Encoder::default();
    while let Some(message) = replies.receive()? {
        // A full event queue already contains work that will recheck capacity.
        // Never block the writer merely to announce newly available output space.
        let _ = events.try_send(Event::Writable);
        match message {
            outbox::Delivery::Reply(message) => transport::write_message(&mut output, &message)?,
            outbox::Delivery::Observation(pending) => encoder.write(&mut output, pending)?,
        }
    }
    Ok(())
}

fn compile(jobs: &Receiver<builds::Job>, events: &SyncSender<Event>) {
    while let Ok(job) = jobs.recv() {
        let result = std::panic::catch_unwind(|| builds::run(job));
        let Ok(message) = result else {
            // Notify before the panic payload is dropped; its destructor can panic.
            let _ = events.send(Event::Failed(WorkerError::BuildLost));
            return;
        };
        let event = Event::Build(message);
        if events.send(event).is_err() {
            return;
        }
    }
}

enum Closing {
    InputComplete(Option<Counter>),
    HandshakeRejected,
}

#[derive(Default)]
struct Dispatcher {
    worker: Worker,
    builds: builds::Queue,
    closing: Option<Closing>,
    subscription: Option<stream::Subscription>,
}

impl Dispatcher {
    fn run(
        mut self,
        events: &Receiver<Event>,
        output: &outbox::Sender,
        jobs: &SyncSender<builds::Job>,
    ) -> Result<bool, WorkerError> {
        loop {
            let event = if let Some(next) = self
                .subscription
                .as_ref()
                .and_then(|subscription| subscription.next)
            {
                match events.recv_timeout(next.saturating_duration_since(Instant::now())) {
                    Ok(event) => event,
                    Err(mpsc::RecvTimeoutError::Timeout) => Event::Writable,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return Err(WorkerError::IoThread),
                }
            } else {
                events.recv().map_err(|_| WorkerError::IoThread)?
            };
            match event {
                Event::Request(message) => self.request(*message, output)?,
                Event::InputClosed => {
                    self.closing = Some(Closing::InputComplete(None));
                    self.stop_observing(output)?;
                    self.worker.close()?;
                }
                Event::Failed(error) => return Err(error),
                Event::Writable => {}
                Event::Build(message) => {
                    if self.builds.finish(message.response.id)? {
                        send(output, message)?;
                    }
                }
            }
            if self
                .subscription
                .as_ref()
                .and_then(|subscription| subscription.next)
                .is_some_and(|next| next <= Instant::now())
            {
                self.sample(output)?;
            }
            if !self.builds.busy()
                && output.can_start_build()?
                && let Some(job) = self.builds.next()
            {
                jobs.try_send(job).map_err(|_| WorkerError::BuildLost)?;
            }
            if self.builds.idle()
                && let Some(closing) = &self.closing
            {
                if let Closing::InputComplete(Some(id)) = closing {
                    send(output, reply(*id, Reply::Closed))?;
                }
                return Ok(matches!(closing, Closing::InputComplete(_)));
            }
        }
    }

    fn request(
        &mut self,
        message: RequestMessage,
        output: &outbox::Sender,
    ) -> Result<(), WorkerError> {
        if self.closing.is_some() {
            return Err(WorkerError::Closed);
        }
        if let Some(response) = self.worker.accept(&message)? {
            if self.worker.closed {
                self.closing = Some(Closing::HandshakeRejected);
            }
            // An invalid initial handshake ends the process without waiting on a
            // reader that might already be blocked on the next input frame.
            send(output, response)?;
            return Ok(());
        }
        let id = message.request.id;
        match message.request.command {
            Command::Assemble { identity, source } => {
                match self.builds.push(id, identity, source) {
                    Ok(Some(previous)) => send(
                        output,
                        builds::error(previous, Diagnostic::new(DiagnosticCode::Superseded)),
                    )?,
                    Ok(None) => {}
                    Err(error) => send(output, builds::error(id, error))?,
                }
            }
            Command::CancelAssembly { request } => {
                if self.builds.cancel(request) {
                    send(
                        output,
                        builds::error(request, Diagnostic::new(DiagnosticCode::Cancelled)),
                    )?;
                    send(output, reply(id, Reply::AssemblyCancelled(request)))?;
                } else {
                    send(
                        output,
                        builds::error(id, Diagnostic::new(DiagnosticCode::InvalidInput)),
                    )?;
                }
            }
            Command::Subscribe { session, memory } => {
                let mut subscription = stream::Subscription::new(id, session, memory);
                match subscription.capture(&mut self.worker)? {
                    Ok(capture) => {
                        self.stop_observing(output)?;
                        send(output, reply(id, Reply::Subscribed(id)))?;
                        self.subscription = Some(subscription);
                        output.observe(Some(stream::Pending::Capture(capture)))?;
                    }
                    Err(error) => send(output, builds::error(id, error))?,
                }
            }
            Command::Unsubscribe { subscription } => {
                if self
                    .subscription
                    .as_ref()
                    .is_some_and(|active| active.id == subscription)
                {
                    self.stop_observing(output)?;
                    send(output, reply(id, Reply::Unsubscribed(subscription)))?;
                } else {
                    send(
                        output,
                        builds::error(id, Diagnostic::new(DiagnosticCode::InvalidInput)),
                    )?;
                }
            }
            Command::Shutdown => {
                self.stop_observing(output)?;
                self.closing = Some(Closing::InputComplete(Some(id)));
                self.worker.close()?;
            }
            _ => {
                let refresh = matches!(&message.request.command, Command::Execute { action, .. } if !matches!(action, SessionAction::Observe { .. }));
                let response = self.worker.perform(message)?;
                if self.subscription.as_ref().is_some_and(|subscription| {
                    match &response.response.result {
                        Reply::Observed(observation) => observation.key != subscription.key,
                        Reply::SessionClosed(key) => *key == subscription.key,
                        _ => false,
                    }
                }) {
                    self.stop_observing(output)?;
                }
                let succeeded = !matches!(response.response.result, Reply::Error(_));
                output.send(response, self.builds.busy())?;
                if refresh && succeeded {
                    self.sample(output)?;
                }
            }
        }
        Ok(())
    }

    fn stop_observing(&mut self, output: &outbox::Sender) -> Result<(), WorkerError> {
        self.subscription = None;
        output.observe(None)
    }

    fn sample(&mut self, output: &outbox::Sender) -> Result<(), WorkerError> {
        let Some(subscription) = &mut self.subscription else {
            return Ok(());
        };
        let pending = match subscription.capture(&mut self.worker)? {
            Ok(capture) => stream::Pending::Capture(capture),
            Err(error) => {
                let subscription = subscription.id;
                self.subscription = None;
                stream::Pending::Ended {
                    subscription,
                    error,
                }
            }
        };
        output.observe(Some(pending))
    }
}

fn send(output: &outbox::Sender, message: Message) -> Result<(), WorkerError> {
    output.send(message, false)
}

const fn reply(id: Counter, result: Reply) -> Message {
    Message {
        response: Response { id, result },
        payloads: Vec::new(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/runtime.rs"]
mod tests;
