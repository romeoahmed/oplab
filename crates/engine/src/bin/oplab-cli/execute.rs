//! One-shot execution with caller-owned native state and bounded final output.

mod input;
mod setup;

pub(super) use input::Input;
use input::Prepared;
use oplab_core::{
    execution::{ExecutionState, Termination},
    protocol::{
        Diagnostic, DiagnosticCode,
        execution::{Fault, Registers},
        scalar::{Counter, HexAddress},
    },
    target::{CpuModel, Target},
};
use oplab_engine::machine::MachineError;
use serde::Serialize;
use std::{
    io,
    process::ExitCode,
    time::{Duration, Instant},
};

#[derive(Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum Output<'a> {
    Executed(&'a Report),
    Error(&'a Diagnostic),
}

#[derive(Serialize)]
struct Report {
    cpu: CpuModel,
    target: Target,
    completion: HexAddress,
    outcome: Outcome,
    instructions: Counter,
    dispatches: Counter,
    registers: Registers,
    fault: Option<Fault>,
    memory: Option<Memory>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Completed,
    Budget,
    GuestFault,
    UnsupportedEnvironment,
    Timeout,
}

#[derive(Serialize)]
struct Memory {
    address: HexAddress,
    bytes: Vec<u8>,
}

pub(super) fn run(input: Input) -> ExitCode {
    let result = input.prepare().and_then(execute);
    let success = result
        .as_ref()
        .is_ok_and(|report| report.outcome == Outcome::Completed);
    let output = match &result {
        Ok(report) => Output::Executed(report),
        Err(error) => Output::Error(error),
    };
    if super::write_json_line(io::stdout().lock(), &output) && success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn execute(prepared: Prepared) -> Result<Report, Diagnostic> {
    let Prepared {
        mut session,
        target,
        completion,
        policy,
    } = prepared;
    // Validate the entire requested window before starting a mutating operation.
    if let Some(address) = policy.memory {
        session
            .read_memory(address, u64::from(policy.memory_bytes))
            .map_err(|error| diagnostic(&error))?;
    }
    session.start().map_err(|error| diagnostic(&error))?;
    let started = Instant::now();
    let timeout = Duration::from_millis(policy.timeout_ms);
    let outcome = loop {
        match session.state() {
            ExecutionState::Running => {
                if started.elapsed() >= timeout {
                    session.cancel().map_err(|error| diagnostic(&error))?;
                    break Outcome::Timeout;
                }
                session.advance().map_err(|error| diagnostic(&error))?;
            }
            ExecutionState::Terminated(Termination::Completed) => break Outcome::Completed,
            ExecutionState::Terminated(Termination::Budget) => break Outcome::Budget,
            ExecutionState::Terminated(Termination::GuestFault) => break Outcome::GuestFault,
            ExecutionState::Terminated(Termination::UnsupportedEnvironment) => {
                break Outcome::UnsupportedEnvironment;
            }
            _ => return Err(Diagnostic::new(DiagnosticCode::BackendFailure)),
        }
    };
    let memory = policy
        .memory
        .map(|address| {
            session
                .read_memory(address, u64::from(policy.memory_bytes))
                .map(|bytes| Memory {
                    address: HexAddress::new(address),
                    bytes,
                })
        })
        .transpose()
        .map_err(|error| diagnostic(&error))?;
    Ok(Report {
        cpu: session.cpu(),
        target,
        completion: HexAddress::new(completion),
        outcome,
        instructions: Counter::new(session.instructions()),
        dispatches: Counter::new(session.dispatches()),
        registers: session
            .read_registers()
            .map_err(|error| diagnostic(&error))?
            .into(),
        fault: session.fault().map(Into::into),
        memory,
    })
}

const fn diagnostic(error: &MachineError) -> Diagnostic {
    Diagnostic::new(match error {
        MachineError::Backend => DiagnosticCode::BackendFailure,
        MachineError::Load(_) | MachineError::Validation(_) => DiagnosticCode::InvalidInput,
    })
}
