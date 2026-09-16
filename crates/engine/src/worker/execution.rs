//! One machine-owning thread. Only commands and immutable observations cross it.

use super::{WorkerError, transport::Message};
use crate::{
    load::{Image, InitialMapping, MachineSetup},
    machine::{Machine, MachineError},
    session::Session,
};
use oplab_core::{
    address::AddressRange,
    diagnostic::ValidationError,
    execution::{ExecutionState, PauseReason},
    memory::{MAX_MAPPED_BYTES, MAX_REGIONS, Permissions},
    protocol::{
        Command, Diagnostic, DiagnosticCode, Reply, Response,
        execution::{
            InitialState, LoadImage, MemoryWindow, Observation, SessionAction, SessionKey, Status,
        },
        scalar::{Counter, HexAddress},
    },
    registers::InitialRegisters,
    target::Target,
};
use std::{
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
};

struct Job {
    id: Counter,
    command: Command,
    payload: Option<Vec<u8>>,
    reply: SyncSender<Message>,
}

/// One native owner with a bounded command queue and a reply slot per request.
/// Replies do not compete with unsolicited observations for capacity.
pub(super) struct Owner {
    sender: Option<SyncSender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl Owner {
    pub(super) fn spawn() -> Result<Self, WorkerError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("oplab-execution".into())
            .spawn(move || run(&receiver))
            .map_err(|_| WorkerError::ExecutionLost)?;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    pub(super) fn request(
        &self,
        id: Counter,
        command: Command,
        payload: Option<Vec<u8>>,
    ) -> Result<Message, WorkerError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.sender
            .as_ref()
            .ok_or(WorkerError::ExecutionLost)?
            .send(Job {
                id,
                command,
                payload,
                reply,
            })
            .map_err(|_| WorkerError::ExecutionLost)?;
        // Native hangs are handled by the process supervisor, not a detached
        // replacement thread that would leave an unknown mutation running.
        receiver.recv().map_err(|_| WorkerError::ExecutionLost)
    }

    pub(super) fn shutdown(mut self) -> Result<(), WorkerError> {
        self.sender.take();
        self.thread
            .take()
            .ok_or(WorkerError::ExecutionLost)?
            .join()
            .map_err(|_| WorkerError::ExecutionLost)
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // Disconnect wakes an idle owner and is checked between running slices.
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct BoundSession {
    id: Counter,
    machine: Session,
    sequence: u64,
}

impl BoundSession {
    const fn key(&self) -> SessionKey {
        SessionKey {
            session: self.id,
            generation: Counter::new(self.machine.generation().get()),
        }
    }

    fn observe(
        &mut self,
        memory: Option<MemoryWindow>,
    ) -> Result<(Reply, Vec<Vec<u8>>), Diagnostic> {
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| Diagnostic::new(DiagnosticCode::ResourceLimit))?;
        let payloads = memory
            .map(|window| {
                self.machine
                    .read_memory(window.address.address(), u64::from(window.length))
            })
            .transpose()
            .map_err(|error| diagnostic(&error))?
            .into_iter()
            .collect();
        let registers = if self.machine.state() == ExecutionState::Crashed {
            None
        } else {
            Some(
                self.machine
                    .read_registers()
                    .map_err(|error| diagnostic(&error))?
                    .into(),
            )
        };
        let fault = self.machine.fault().map(Into::into);
        let observation = Observation {
            key: self.key(),
            sequence: Counter::new(sequence),
            status: status(self.machine.state()),
            instructions: Counter::new(self.machine.instructions()),
            dispatches: Counter::new(self.machine.dispatches()),
            registers,
            fault,
            memory,
            breakpoints: self.machine.breakpoints().map(HexAddress::new).collect(),
        };
        self.sequence = sequence;
        Ok((Reply::Observed(Box::new(observation)), payloads))
    }
}

fn run(receiver: &Receiver<Job>) {
    // Construct and destroy every native handle inside this thread.
    let mut active: Option<BoundSession> = None;
    loop {
        let job = if active
            .as_ref()
            .is_some_and(|bound| bound.machine.state() == ExecutionState::Running)
        {
            match receiver.try_recv() {
                Ok(job) => Some(job),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        } else {
            match receiver.recv() {
                Ok(job) => Some(job),
                Err(_) => return,
            }
        };
        let advanced = if let Some(job) = job {
            let (result, payloads) = dispatch(&mut active, job.id, &job.command, job.payload)
                .unwrap_or_else(|error| (Reply::Error(error), Vec::new()));
            // Step already executes one slice. Poll queued controls before any
            // continuation, including when that slice made no instruction progress.
            let advanced = matches!(
                job.command,
                Command::Execute {
                    action: SessionAction::Step,
                    ..
                }
            ) && matches!(result, Reply::Observed(_));
            if job
                .reply
                .send(Message {
                    response: Response { id: job.id, result },
                    payloads,
                })
                .is_err()
            {
                return; // No retry or orphan machine after its owner loses correlation.
            }
            advanced
        } else {
            false
        };
        if !advanced
            && let Some(bound) = active.as_mut()
            && bound.machine.state() == ExecutionState::Running
        {
            // Session marks native failure as Crashed. Observations then omit registers.
            let _ = bound.machine.advance();
        }
    }
}

fn dispatch(
    active: &mut Option<BoundSession>,
    id: Counter,
    command: &Command,
    payload: Option<Vec<u8>>,
) -> Result<(Reply, Vec<Vec<u8>>), Diagnostic> {
    match command {
        Command::Load {
            image: format,
            initial,
            replace,
            target,
            completion,
            instruction_budget,
            ..
        } => {
            if active.as_ref().map(BoundSession::key) != *replace {
                return Err(Diagnostic::new(DiagnosticCode::StaleSession));
            }
            if active
                .as_ref()
                .is_some_and(|bound| bound.machine.state() == ExecutionState::Running)
            {
                return Err(Diagnostic::new(DiagnosticCode::InvalidState));
            }
            let image = payload.ok_or_else(|| Diagnostic::new(DiagnosticCode::InvalidInput))?;
            let setup = setup(*target, initial).map_err(|error| diagnostic(&error))?;
            let input = match format {
                LoadImage::Elf => Image::Elf(&image),
                LoadImage::Raw { base, entry } => Image::Raw {
                    bytes: &image,
                    base: base.address(),
                    entry: entry.address(),
                },
            };
            let machine = Machine::load(input, *target, setup)
                .and_then(|machine| {
                    Session::new(machine, completion.address(), instruction_budget.get())
                })
                .map_err(|error| diagnostic(&error))?;
            let mut replacement = BoundSession {
                id,
                machine,
                sequence: 0,
            };
            let observation = replacement.observe(None)?;
            *active = Some(replacement);
            Ok(observation)
        }
        Command::Execute { session, action } => {
            let bound = active
                .as_mut()
                .filter(|bound| bound.key() == *session)
                .ok_or_else(|| Diagnostic::new(DiagnosticCode::StaleSession))?;
            if *action == SessionAction::Close {
                *active = None;
                return Ok((Reply::SessionClosed(*session), Vec::new()));
            }
            if bound.sequence == u64::MAX {
                return Err(Diagnostic::new(DiagnosticCode::ResourceLimit));
            }
            let memory = match apply(&mut bound.machine, action, payload.as_deref()) {
                Ok(memory) => memory,
                Err(MachineError::Backend) if bound.machine.state() == ExecutionState::Crashed => {
                    return bound.observe(None);
                }
                Err(error) => return Err(diagnostic(&error)),
            };
            bound.observe(memory)
        }
        _ => Err(Diagnostic::new(DiagnosticCode::InvalidInput)),
    }
}

fn setup(target: Target, initial: &InitialState) -> Result<MachineSetup, MachineError> {
    if initial.registers.len() > 32 || initial.mappings.len() >= MAX_REGIONS {
        return Err(ValidationError::Length.into());
    }
    let registers = InitialRegisters::from_assignments(
        target,
        initial
            .registers
            .iter()
            .map(|register| (register.name.as_str(), register.value.get())),
    )?;
    let mappings = initial
        .mappings
        .iter()
        .map(|mapping| {
            if mapping.flags > 7 {
                return Err(ValidationError::Permission);
            }
            Ok(InitialMapping {
                range: AddressRange::new(
                    mapping.address.address(),
                    u64::from(mapping.length),
                    MAX_MAPPED_BYTES,
                )?,
                permissions: Permissions {
                    read: mapping.flags & 4 != 0,
                    write: mapping.flags & 2 != 0,
                    execute: mapping.flags & 1 != 0,
                },
                bytes: Vec::new(),
            })
        })
        .collect::<Result<_, ValidationError>>()?;
    Ok(MachineSetup {
        registers: Some(registers),
        mappings,
    })
}

fn apply(
    session: &mut Session,
    action: &SessionAction,
    payload: Option<&[u8]>,
) -> Result<Option<MemoryWindow>, MachineError> {
    match action {
        SessionAction::Run => session.start()?,
        SessionAction::Step => session.step()?,
        SessionAction::Pause => session.pause()?,
        SessionAction::Cancel => session.cancel()?,
        SessionAction::Reset => session.reset()?,
        SessionAction::WriteRegister(register) => {
            session.write_register(&register.name, register.value.get())?;
        }
        SessionAction::WriteMemory(window) => {
            session.write_memory(
                window.address.address(),
                payload.ok_or(ValidationError::Length)?,
            )?;
        }
        SessionAction::Breakpoint { address, enabled } => {
            session.set_breakpoint(address.address(), *enabled)?;
        }
        SessionAction::Observe { memory } => return Ok(*memory),
        SessionAction::Close => return Err(ValidationError::Transition.into()),
    }
    Ok(None)
}

const fn diagnostic(error: &MachineError) -> Diagnostic {
    Diagnostic::new(match error {
        MachineError::Backend => DiagnosticCode::BackendFailure,
        MachineError::Validation(ValidationError::Transition) => DiagnosticCode::InvalidState,
        MachineError::Validation(ValidationError::CounterExhausted) => {
            DiagnosticCode::ResourceLimit
        }
        MachineError::Load(_) | MachineError::Validation(_) => DiagnosticCode::InvalidInput,
    })
}

const fn status(state: ExecutionState) -> Status {
    match state {
        ExecutionState::Ready => Status::Ready,
        ExecutionState::Running => Status::Running,
        ExecutionState::Paused(PauseReason::Step) => Status::Stepped,
        ExecutionState::Paused(PauseReason::Requested) => Status::Paused,
        ExecutionState::Paused(PauseReason::Breakpoint(address)) => {
            Status::Breakpoint(HexAddress::new(address))
        }
        ExecutionState::Terminated(reason) => Status::Terminated(reason),
        ExecutionState::Crashed => Status::Crashed,
    }
}
