//! Framed worker dispatch with a dedicated native execution owner.

use oplab_core::target::Target;
mod builds;
mod dispatch;
mod execution;
mod outbox;
mod stream;
use oplab_core::protocol::transport;

use oplab_core::protocol::{
    Capabilities, Command, DecodedInstruction, Diagnostic, DiagnosticCode, Reply, Response,
    VERSION,
    scalar::{Counter, HexAddress},
};
use oplab_toolchain::assembly;
use oplab_toolchain::decode::{self, DecodeError};
use thiserror::Error;

/// Connection failures may leave request outcomes unknown; no request is retried internally.
#[derive(Debug, Error)]
pub enum WorkerError {
    /// Framing, decoding, or I/O failed.
    #[error(transparent)]
    Transport(#[from] transport::TransportError),
    /// A repeated or out-of-order request cannot receive another correlated reply.
    #[error("invalid request order")]
    RequestOrder,
    /// No work may follow an orderly shutdown.
    #[error("worker connection is closed")]
    Closed,
    /// An execution thread could not start or stopped without a correlated result.
    #[error("execution owner unavailable; operation outcome may be unknown")]
    ExecutionLost,
    /// The assembly owner failed without a valid completion notification.
    #[error("assembly owner unavailable")]
    BuildLost,
    /// A process I/O thread could not start or complete.
    #[error("worker I/O thread unavailable")]
    IoThread,
}

/// Owns protocol negotiation and correlation for one worker connection.
#[derive(Default)]
pub struct Worker {
    negotiated: bool,
    closed: bool,
    last_request: Option<Counter>,
    execution: Option<execution::Owner>,
}

impl Worker {
    /// Dispatch one request synchronously after a compatible initial handshake.
    ///
    /// Operation failures are returned as tagged replies. Assembly cancellation and
    /// subscriptions require [`serve`] and are rejected by this synchronous interface.
    ///
    /// # Errors
    ///
    /// Returns a connection error for invalid IDs, requests after shutdown, binary
    /// payload mismatches or execution-owner failure.
    pub fn handle(
        &mut self,
        message: transport::RequestMessage,
    ) -> Result<transport::Message, WorkerError> {
        if let Some(response) = self.accept(&message)? {
            return Ok(response);
        }
        self.perform(message)
    }

    fn accept(
        &mut self,
        message: &transport::RequestMessage,
    ) -> Result<Option<transport::Message>, WorkerError> {
        if self.closed {
            return Err(WorkerError::Closed);
        }
        let request = &message.request;
        if request.id.get() == 0 || self.last_request.is_some_and(|last| request.id <= last) {
            return Err(WorkerError::RequestOrder);
        }
        self.last_request = Some(request.id);
        transport::validate_request(message)?;
        if self.negotiated {
            return Ok(None);
        }
        let result = if matches!(request.command, Command::Hello { version: VERSION }) {
            self.negotiated = true;
            Reply::Hello(Capabilities {
                version: VERSION,
                targets: vec![Target::X86_64, Target::Aarch64],
                assembler: assembly::identity(),
                source_mapping: false,
                execution: true,
            })
        } else {
            self.closed = true;
            Reply::Error(Diagnostic::new(DiagnosticCode::Protocol))
        };
        Ok(Some(transport::Message {
            response: Response {
                id: request.id,
                result,
            },
            payloads: Vec::new(),
        }))
    }

    fn close(&mut self) -> Result<(), WorkerError> {
        self.closed = true;
        if let Some(owner) = self.execution.take() {
            owner.shutdown()?;
        }
        Ok(())
    }

    fn perform(
        &mut self,
        message: transport::RequestMessage,
    ) -> Result<transport::Message, WorkerError> {
        let transport::RequestMessage { request, payload } = message;
        if let Command::Assemble { identity, source } = request.command {
            return Ok(builds::build_message(
                request.id,
                assembly::assemble(identity, &source),
            ));
        }
        if matches!(
            request.command,
            Command::Load { .. } | Command::Execute { .. }
        ) {
            if self.execution.is_none() {
                self.execution = Some(execution::Owner::spawn()?);
            }
            return self
                .execution
                .as_ref()
                .ok_or(WorkerError::ExecutionLost)?
                .request(request.id, request.command, payload);
        }
        if request.command == Command::Shutdown {
            self.close()?;
        }
        Ok(transport::Message {
            response: Response {
                id: request.id,
                result: Self::dispatch(request.command),
            },
            payloads: Vec::new(),
        })
    }

    fn dispatch(command: Command) -> Reply {
        match command {
            Command::Hello { .. } => Reply::Error(Diagnostic::new(DiagnosticCode::Protocol)),
            Command::Shutdown => Reply::Closed,
            Command::Decode {
                target,
                base,
                bytes,
                limit,
            } => {
                let Ok(limit) = usize::try_from(limit) else {
                    return Reply::Error(Diagnostic::new(DiagnosticCode::InvalidInput));
                };
                match decode::decode(target, &bytes, base.address(), limit) {
                    Ok(instructions) => Reply::Decoded(
                        instructions
                            .into_iter()
                            .map(|instruction| DecodedInstruction {
                                address: HexAddress::new(instruction.address),
                                bytes: instruction.bytes,
                                text: instruction.text,
                            })
                            .collect(),
                    ),
                    Err(error) => Reply::Error(decode_diagnostic(&error)),
                }
            }
            Command::Analyze {
                target,
                base,
                bytes,
            } => match decode::analyze(target, &bytes, base.address()) {
                Ok(analysis) => Reply::Analyzed(Box::new(analysis)),
                Err(error) => Reply::Error(decode_diagnostic(&error)),
            },
            Command::CancelAssembly { .. }
            | Command::Subscribe { .. }
            | Command::Unsubscribe { .. } => {
                Reply::Error(Diagnostic::new(DiagnosticCode::InvalidInput))
            }
            Command::Assemble { .. } | Command::Load { .. } | Command::Execute { .. } => {
                Reply::Error(Diagnostic::new(DiagnosticCode::Protocol))
            }
        }
    }
}

/// Serve the worker process's standard pipes with independent assembly and execution.
///
/// This process entry owns background I/O; callers must exit the process on return.
/// Embedders use engine operations or [`Worker`] for synchronous dispatch instead.
///
/// # Errors
///
/// Stops on malformed input, failed I/O, native owner failure, or invalid correlation.
/// Native hangs require an external process supervisor deadline.
pub fn serve() -> Result<(), WorkerError> {
    dispatch::serve()
}

const fn decode_diagnostic(error: &DecodeError) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode::Decode,
        source_offset: None,
        address: if let DecodeError::Invalid(address) = error {
            Some(HexAddress::new(*address))
        } else {
            None
        },
    }
}
