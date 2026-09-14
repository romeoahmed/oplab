//! Own one isolated worker, bounded request admission, and explicit frontend leases.

mod delivery;
mod observations;
mod process;

use delivery::Delivery;
use oplab_core::protocol::{
    Artifact, Capabilities, Command, Reply, Request, VERSION,
    desktop::{ConnectionInfo, DesktopFailure, FailureCode},
    execution::Observation,
    scalar::Counter,
    transport::{Message, RequestMessage},
};
use std::{
    collections::BTreeMap,
    path::Path,
    process::Child,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tauri::ipc::Channel;

const CAPACITY: usize = 16;
const CONTROL_DEADLINE: Duration = Duration::from_secs(3);
const BUILD_DEADLINE: Duration = Duration::from_secs(15);
const RUN_DEADLINE: Duration = Duration::from_secs(60);
const MEMORY_LIMIT: u64 = 1024 * 1024 * 1024;

type Result<T> = std::result::Result<T, DesktopFailure>;

struct Pending {
    result: SyncSender<Result<Message>>,
    deadline: Instant,
    started: bool,
    view: Counter,
}

#[derive(Default)]
struct State {
    counter: u64,
    view: u64,
    pending: BTreeMap<Counter, Pending>,
    failure: Option<FailureCode>,
    capabilities: Option<Capabilities>,
    session: Option<Box<Observation>>,
    closed_session: u64,
    artifact: Option<Artifact>,
    subscription: Option<Counter>,
    cache: observations::Cache,
    delivery: Option<Delivery>,
    running_since: Option<Instant>,
}

struct Shared {
    state: Mutex<State>,
    child: Mutex<Child>,
}

impl Shared {
    fn fail(&self, code: FailureCode) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.failure.is_some() {
            return;
        }
        state.failure = Some(code);
        for (id, pending) in std::mem::take(&mut state.pending) {
            let _ = pending.result.send(Err(DesktopFailure {
                code,
                request: Some(id),
                outcome_unknown: pending.started,
            }));
        }
        let delivery = state.delivery.take();
        drop(state);
        if let Some(delivery) = delivery {
            delivery.failed(code);
        }
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// A request already admitted to the bounded process queue.
pub(crate) struct Ticket(Receiver<Result<Message>>);

impl Ticket {
    pub(crate) fn wait(self) -> Result<Message> {
        self.0
            .recv()
            .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?
    }
}

struct Connection {
    id: Counter,
    shared: Arc<Shared>,
    writer: Option<SyncSender<RequestMessage>>,
    threads: Vec<JoinHandle<()>>,
}

impl Connection {
    fn request(&self, view: Counter, command: Command, image: Option<Vec<u8>>) -> Result<Ticket> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?;
        if let Some(code) = state.failure {
            return Err(DesktopFailure::new(code));
        }
        if view.get() != 0 && (state.view != view.get() || state.delivery.is_none()) {
            return Err(DesktopFailure::new(FailureCode::StaleConnection));
        }
        if state.pending.len() == CAPACITY {
            return Err(DesktopFailure::new(FailureCode::Busy));
        }
        let id = state
            .counter
            .checked_add(1)
            .ok_or_else(|| DesktopFailure::new(FailureCode::Protocol))?;
        let id = Counter::new(id);
        let budget = if matches!(command, Command::Assemble { .. }) {
            BUILD_DEADLINE
        } else {
            CONTROL_DEADLINE
        };
        let (result, receiver) = mpsc::sync_channel(1);
        let message = RequestMessage {
            request: Request { id, command },
            image,
        };
        oplab_core::protocol::transport::validate_request(&message)
            .map_err(|_| DesktopFailure::new(FailureCode::Protocol))?;
        state.pending.insert(
            id,
            Pending {
                result,
                deadline: Instant::now() + budget,
                started: false,
                view,
            },
        );
        if self
            .writer
            .as_ref()
            .ok_or_else(|| DesktopFailure::new(FailureCode::Disconnected))?
            .try_send(message)
            .is_err()
        {
            state.pending.remove(&id);
            return Err(DesktopFailure::new(FailureCode::Busy));
        }
        state.counter = id.get();
        drop(state);
        Ok(Ticket(receiver))
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Ok(ticket) = self.request(Counter::new(0), Command::Shutdown, None) {
            self.writer.take();
            let _ = ticket.0.recv_timeout(Duration::from_secs(2));
        }
        self.writer.take();
        self.shared.fail(FailureCode::Disconnected);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
struct ServiceState {
    incarnation: u64,
    active: Option<Arc<Connection>>,
}

/// Window-scoped native integration. Reattachment preserves the worker; restart is explicit.
#[derive(Default)]
pub(crate) struct Service(Mutex<ServiceState>);

impl Service {
    pub(crate) fn connect(
        &self,
        executable: &Path,
        channel: Channel,
        restart: bool,
    ) -> Result<ConnectionInfo> {
        let mut service = self
            .0
            .lock()
            .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?;
        if restart {
            service.active = None;
        }
        if service.active.is_none() {
            let id = service
                .incarnation
                .checked_add(1)
                .ok_or_else(|| DesktopFailure::new(FailureCode::Protocol))?;
            service.incarnation = id;
            let connection = process::spawn(executable, Counter::new(id))?;
            let message = connection
                .request(Counter::new(0), Command::Hello { version: VERSION }, None)?
                .wait()?;
            let Reply::Hello(capabilities) = message.response.result else {
                return Err(DesktopFailure::new(FailureCode::Protocol));
            };
            if capabilities.version != VERSION || !capabilities.execution {
                return Err(DesktopFailure::new(FailureCode::Protocol));
            }
            connection
                .shared
                .state
                .lock()
                .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?
                .capabilities = Some(capabilities);
            service.active = Some(Arc::new(connection));
        }
        let connection = service
            .active
            .clone()
            .ok_or_else(|| DesktopFailure::new(FailureCode::Unavailable))?;
        drop(service);
        let mut state = connection
            .shared
            .state
            .lock()
            .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?;
        if let Some(code) = state.failure {
            return Err(DesktopFailure::new(code));
        }
        state.view = state
            .view
            .checked_add(1)
            .ok_or_else(|| DesktopFailure::new(FailureCode::Protocol))?;
        state.delivery = Some(Delivery::new(channel));
        state.subscription = None;
        Ok(ConnectionInfo {
            connection: connection.id,
            view: Counter::new(state.view),
            capabilities: state
                .capabilities
                .clone()
                .ok_or_else(|| DesktopFailure::new(FailureCode::Protocol))?,
            session: state.session.clone(),
            artifact: state.artifact.clone(),
        })
    }

    pub(crate) fn request(
        &self,
        connection: Counter,
        view: Counter,
        command: Command,
        image: Option<Vec<u8>>,
    ) -> Result<Ticket> {
        let service = self
            .0
            .lock()
            .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?;
        let active = service
            .active
            .as_ref()
            .filter(|active| active.id == connection)
            .cloned()
            .ok_or_else(|| DesktopFailure::new(FailureCode::StaleConnection))?;
        drop(service);
        if view.get() == 0 || matches!(command, Command::Hello { .. } | Command::Shutdown) {
            return Err(DesktopFailure::new(FailureCode::Protocol));
        }
        active.request(view, command, image)
    }

    pub(crate) fn acknowledge(
        &self,
        connection: Counter,
        view: Counter,
        subscription: Counter,
        sequence: Counter,
    ) -> Result<()> {
        let service = self
            .0
            .lock()
            .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?;
        let active = service
            .active
            .as_ref()
            .filter(|active| active.id == connection)
            .cloned()
            .ok_or_else(|| DesktopFailure::new(FailureCode::StaleConnection))?;
        drop(service);
        let mut state = active
            .shared
            .state
            .lock()
            .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?;
        if state.view != view.get() {
            return Err(DesktopFailure::new(FailureCode::StaleConnection));
        }
        state
            .delivery
            .as_mut()
            .ok_or_else(|| DesktopFailure::new(FailureCode::Disconnected))?
            .acknowledge(subscription, sequence)
    }

    pub(crate) fn detach(&self, connection: Counter, view: Counter) {
        if let Ok(service) = self.0.lock()
            && let Some(active) = service
                .active
                .as_ref()
                .filter(|active| active.id == connection)
            && let Ok(mut state) = active.shared.state.lock()
            && state.view == view.get()
        {
            state.delivery = None;
            state.subscription = None;
        }
    }

    pub(crate) fn shutdown(&self) {
        if let Ok(mut service) = self.0.lock() {
            service.active = None;
        }
    }
}
