//! Bounded duplex process fixture. Killing the child precedes scoped I/O joins.

use oplab_core::protocol::transport::{self, Message, Output, RequestMessage, StreamMessage};
use oplab_core::protocol::{Command, Request, scalar::Counter};
use std::{
    io::Read,
    process::{Child, Command as ProcessCommand, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

pub(super) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) struct Client {
    sender: SyncSender<RequestMessage>,
    receiver: Receiver<Output>,
    events: std::collections::VecDeque<StreamMessage>,
    next: u64,
}

impl Client {
    pub(super) fn request(
        &mut self,
        command: Command,
        image: Option<Vec<u8>>,
    ) -> TestResult<Message> {
        let id = Counter::new(self.next);
        self.next = self.next.checked_add(1).ok_or("fixture request overflow")?;
        self.sender.send(RequestMessage {
            request: Request { id, command },
            image,
        })?;
        loop {
            match self.receiver.recv_timeout(Duration::from_secs(5))? {
                Output::Response(message) => {
                    assert_eq!(message.response.id, id);
                    return Ok(message);
                }
                Output::Observation(event) => {
                    if self.events.len() == 16 {
                        return Err("fixture observation overflow".into());
                    }
                    self.events.push_back(event);
                }
            }
        }
    }

    pub(super) fn event(&mut self) -> TestResult<StreamMessage> {
        if let Some(event) = self.events.pop_front() {
            return Ok(event);
        }
        match self.receiver.recv_timeout(Duration::from_secs(5))? {
            Output::Observation(event) => Ok(event),
            Output::Response(_) => Err("unexpected correlated reply".into()),
        }
    }
}

pub(super) fn worker(test: impl FnOnce(&mut Client) -> TestResult) -> TestResult {
    std::thread::scope(|scope| {
        let mut process = Guard(
            ProcessCommand::new(env!("CARGO_BIN_EXE_oplab-worker"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        let mut stdin = process.0.stdin.take().ok_or("missing stdin")?;
        let mut stdout = process.0.stdout.take().ok_or("missing stdout")?;
        let stderr = process.0.stderr.take().ok_or("missing stderr")?;
        let (sender, requests) = mpsc::sync_channel(1);
        let (replies, receiver) = mpsc::sync_channel(1);
        scope.spawn(move || {
            while let Ok(request) = requests.recv() {
                if transport::write_request(&mut stdin, &request).is_err() {
                    break;
                }
            }
        });
        scope.spawn(move || {
            while let Ok(Some(message)) = transport::read_output(&mut stdout) {
                if replies.send(message).is_err() {
                    break;
                }
            }
        });
        let errors = scope.spawn(move || {
            let mut bytes = Vec::new();
            stderr.take(4097).read_to_end(&mut bytes).map(|_| bytes)
        });
        let result = test(&mut Client {
            sender,
            receiver,
            next: 1,
            events: std::collections::VecDeque::new(),
        });
        if result.is_ok() {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = process.0.try_wait()? {
                    assert!(status.success(), "worker did not exit successfully");
                    break;
                }
                if Instant::now() >= deadline {
                    return Err("worker shutdown deadline exceeded".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        // A failed assertion or deadline also reaches Guard before the scoped joins.
        drop(process);
        let stderr = errors.join().map_err(|_| "stderr fixture failed")??;
        assert!(stderr.is_empty(), "worker emitted unexpected stderr");
        result
    })
}
