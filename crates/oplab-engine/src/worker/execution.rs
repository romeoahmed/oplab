//! One machine-owning thread. Only commands and immutable observations cross it.

use super::{WorkerError, transport::Message};
use crate::{machine::MachineError, session::Session};
use oplab_core::{
    diagnostic::ValidationError,
    execution::{ExecutionState, PauseReason},
    protocol::{
        Command, Diagnostic, DiagnosticCode, Reply, Response,
        execution::{
            Fault, MemoryWindow, Observation, Registers, SessionAction, SessionKey, Status,
        },
        scalar::{Counter, HexAddress},
    },
    registers::IntegerRegisters,
};
use std::{
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
};

struct Job {
    id: Counter,
    command: Command,
    image: Option<Vec<u8>>,
    reply: SyncSender<Message>,
}

/// The synchronous dispatcher admits one queued execution command. The reply slot
/// never competes with observations: each command captures a full snapshot on demand.
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
        image: Option<Vec<u8>>,
    ) -> Result<Message, WorkerError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.sender
            .as_ref()
            .ok_or(WorkerError::ExecutionLost)?
            .send(Job {
                id,
                command,
                image,
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
            Some(registers(
                &self
                    .machine
                    .read_registers()
                    .map_err(|error| diagnostic(&error))?,
            ))
        };
        let fault = self.machine.fault().map(|fault| Fault {
            kind: fault.kind,
            pc: HexAddress::new(fault.pc),
            address: fault.address.map(HexAddress::new),
            size: fault.size.map(Counter::new),
        });
        let observation = Observation {
            key: self.key(),
            sequence: Counter::new(sequence),
            status: status(self.machine.state()),
            instructions: Counter::new(self.machine.instructions()),
            dispatches: Counter::new(self.machine.dispatches()),
            registers,
            fault,
            memory,
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
            let (result, payloads) = dispatch(&mut active, job.id, &job.command, job.image)
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
            // Session turns a native operation failure into Crashed. The next
            // observation reports the lost state without attempting register reads.
            let _ = bound.machine.advance();
        }
    }
}

fn dispatch(
    active: &mut Option<BoundSession>,
    id: Counter,
    command: &Command,
    image: Option<Vec<u8>>,
) -> Result<(Reply, Vec<Vec<u8>>), Diagnostic> {
    match command {
        Command::Load {
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
            let image = image.ok_or_else(|| Diagnostic::new(DiagnosticCode::InvalidInput))?;
            let machine = Session::from_elf(
                &image,
                *target,
                completion.address(),
                instruction_budget.get(),
            )
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
            let memory = apply(&mut bound.machine, *action).map_err(|error| diagnostic(&error))?;
            bound.observe(memory)
        }
        _ => Err(Diagnostic::new(DiagnosticCode::InvalidInput)),
    }
}

fn apply(
    session: &mut Session,
    action: SessionAction,
) -> Result<Option<MemoryWindow>, MachineError> {
    match action {
        SessionAction::Run => session.start()?,
        SessionAction::Step => session.step()?,
        SessionAction::Pause => session.pause()?,
        SessionAction::Cancel => session.cancel()?,
        SessionAction::Reset => session.reset()?,
        SessionAction::Breakpoint { address, enabled } => {
            session.set_breakpoint(address.address(), enabled)?;
        }
        SessionAction::Observe { memory } => return Ok(memory),
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

fn registers(registers: &IntegerRegisters) -> Registers {
    match registers {
        IntegerRegisters::X86_64 { gpr, rip, rflags } => Registers::X86_64 {
            gpr: gpr.map(Counter::new),
            rip: HexAddress::new(*rip),
            rflags: Counter::new(*rflags),
        },
        IntegerRegisters::Aarch64 { x, sp, pc, nzcv } => Registers::Aarch64 {
            x: x.map(Counter::new),
            sp: Counter::new(*sp),
            pc: HexAddress::new(*pc),
            nzcv: *nzcv,
        },
    }
}
