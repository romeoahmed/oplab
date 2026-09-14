//! Independent pipe owners and a watchdog; no blocking pipe can prevent termination.

use super::{CAPACITY, Connection, MEMORY_LIMIT, RUN_DEADLINE, Result, Shared, State};
use oplab_core::protocol::{
    Reply,
    desktop::{DesktopFailure, FailureCode},
    execution::Observation,
    scalar::Counter,
};
use oplab_core::protocol::{
    execution::Status,
    stream::ObservationUpdate,
    transport::{self, Output},
};
use std::{
    io::{BufReader, Read},
    process::{Command as ProcessCommand, Stdio},
    thread,
};
use std::{
    path::Path,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

pub(super) fn spawn(executable: &Path, id: Counter) -> Result<Connection> {
    let mut command = ProcessCommand::new(executable);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW, without a shell.
    }
    let mut child = command
        .spawn()
        .map_err(|_| DesktopFailure::new(FailureCode::Unavailable))?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (writer, requests) = mpsc::sync_channel(CAPACITY);
    let shared = Arc::new(Shared {
        state: Mutex::new(State::default()),
        child: Mutex::new(child),
    });
    // From this point every fallible setup path drops the connection, kills, and reaps.
    let mut connection = Connection {
        id,
        shared,
        writer: Some(writer),
        threads: Vec::new(),
    };
    let mut stdin = stdin.ok_or_else(|| DesktopFailure::new(FailureCode::Unavailable))?;
    let stdout = stdout.ok_or_else(|| DesktopFailure::new(FailureCode::Unavailable))?;
    let stderr = stderr.ok_or_else(|| DesktopFailure::new(FailureCode::Unavailable))?;
    connection.thread("oplab-writer", move |shared| {
        while let Ok(message) = requests.recv() {
            {
                let mut state = shared.state.lock().map_err(|_| FailureCode::Disconnected)?;
                if state.failure.is_some() {
                    return Ok(());
                }
                let pending = state
                    .pending
                    .get_mut(&message.request.id)
                    .ok_or(FailureCode::Protocol)?;
                pending.started = true;
                drop(state);
            }
            transport::write_request(&mut stdin, &message)
                .map_err(|_| FailureCode::Disconnected)?;
        }
        Ok(())
    })?;
    connection.thread("oplab-reader", move |shared| read(shared, stdout))?;
    connection.thread("oplab-stderr", move |_| {
        // Consume independently, retaining no source, environment, or filesystem text.
        let mut bounded = stderr.take(65_537);
        let count = std::io::copy(&mut bounded, &mut std::io::sink())
            .map_err(|_| FailureCode::Disconnected)?;
        if count > 65_536 {
            return Err(FailureCode::Protocol);
        }
        Ok(())
    })?;
    connection.thread("oplab-watchdog", watch)?;
    Ok(connection)
}

impl Connection {
    fn thread(
        &mut self,
        name: &str,
        task: impl FnOnce(&Shared) -> std::result::Result<(), FailureCode> + Send + 'static,
    ) -> Result<()> {
        let shared = Arc::clone(&self.shared);
        let thread = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| task(&shared)));
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(code)) => shared.fail(code),
                    Err(_) => shared.fail(FailureCode::Disconnected),
                }
            })
            .map_err(|_| DesktopFailure::new(FailureCode::Unavailable))?;
        self.threads.push(thread);
        Ok(())
    }
}

fn read(shared: &Shared, stdout: impl Read) -> std::result::Result<(), FailureCode> {
    let mut reader = BufReader::new(stdout);
    loop {
        let output = transport::read_output(&mut reader)
            .map_err(|_| FailureCode::Protocol)?
            .ok_or(FailureCode::Disconnected)?;
        let mut state = shared.state.lock().map_err(|_| FailureCode::Disconnected)?;
        if state.failure.is_some() {
            return Ok(());
        }
        match output {
            Output::Response(message) => {
                let pending = state
                    .pending
                    .remove(&message.response.id)
                    .ok_or(FailureCode::Protocol)?;
                match &message.response.result {
                    Reply::Observed(observation) => state.observe(observation),
                    Reply::Assembled(artifact) => state.artifact = Some(artifact.clone()),
                    Reply::SessionClosed(key) => {
                        state.closed_session = state.closed_session.max(key.session.get());
                        state.session = None;
                        state.running_since = None;
                        state.subscription = None;
                    }
                    Reply::Subscribed(id) if pending.view.get() == state.view => {
                        state.subscription = Some(*id);
                    }
                    Reply::Unsubscribed(id) if state.subscription == Some(*id) => {
                        state.subscription = None;
                    }
                    _ => {}
                }
                let _ = pending.result.send(Ok(message));
            }
            Output::Observation(message) => {
                let full = state
                    .cache
                    .apply(message)
                    .map_err(|_| FailureCode::Protocol)?;
                if let ObservationUpdate::Full(observation) = &full.event.update {
                    state.observe(observation);
                }
                if state.subscription == Some(full.event.subscription)
                    && let Some(delivery) = &mut state.delivery
                    && delivery.offer(full).is_err()
                {
                    // A detached WebView loses its subscription, never the guest.
                    state.delivery = None;
                    state.subscription = None;
                }
            }
        }
    }
}

impl State {
    fn observe(&mut self, observation: &Observation) {
        if observation.key.session.get() <= self.closed_session
            || self.session.as_ref().is_some_and(|current| {
                current.key.session > observation.key.session
                    || (current.key.session == observation.key.session
                        && current.sequence >= observation.sequence)
            })
        {
            return;
        }
        if observation.status == Status::Running {
            self.running_since.get_or_insert_with(Instant::now);
        } else {
            self.running_since = None;
        }
        if self
            .session
            .as_ref()
            .is_some_and(|current| current.key != observation.key)
        {
            self.subscription = None;
        }
        self.session = Some(Box::new(observation.clone()));
    }
}

fn watch(shared: &Shared) -> std::result::Result<(), FailureCode> {
    let pid = shared
        .child
        .lock()
        .map_err(|_| FailureCode::Disconnected)?
        .id();
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    loop {
        {
            let state = shared.state.lock().map_err(|_| FailureCode::Disconnected)?;
            if state.failure.is_some() {
                return Ok(());
            }
            let now = Instant::now();
            if state
                .pending
                .values()
                .any(|pending| pending.deadline <= now)
                || state
                    .running_since
                    .is_some_and(|start| now.duration_since(start) >= RUN_DEADLINE)
            {
                return Err(FailureCode::Deadline);
            }
        }
        if shared
            .child
            .lock()
            .map_err(|_| FailureCode::Disconnected)?
            .try_wait()
            .map_err(|_| FailureCode::Disconnected)?
            .is_some()
        {
            // Let the reader drain the final correlated replies before declaring EOF.
            return Ok(());
        }
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_memory().without_tasks(),
        );
        if system
            .process(pid)
            .is_some_and(|process| process.memory() > MEMORY_LIMIT)
        {
            return Err(FailureCode::MemoryLimit);
        }
        thread::sleep(Duration::from_millis(25));
    }
}
