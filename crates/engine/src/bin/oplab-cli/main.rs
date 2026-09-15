//! Machine-readable assembly, instruction inspection and bounded guest execution.

mod execute;

use clap::Parser;
use oplab_core::{
    address::Address,
    protocol::{
        BuildIdentity, Command, MAX_DECODE_BYTES, MAX_SOURCE_BYTES, Reply, Request, VERSION,
        scalar::{Counter, HexAddress},
    },
};
use oplab_core::{protocol::transport, target::Target};
use oplab_engine::{assembly, worker::Worker};
use std::{
    io::{self, Read, Write},
    process::ExitCode,
};

/// Assemble, inspect or execute `x86_64` and `AArch64` code from stdin.
#[derive(clap::Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    operation: Operation,
}

#[derive(clap::Subcommand)]
enum Operation {
    #[command(flatten)]
    Inspect(Inspection),
    /// Execute source, ELF or raw code and report final machine state.
    #[command(subcommand)]
    Run(execute::Input),
}

#[derive(clap::Subcommand)]
enum Inspection {
    /// Print engine capabilities as JSON.
    Capabilities,
    /// Read UTF-8 assembly and write an executable ELF image to stdout.
    Assemble(Input),
    /// Read machine code and write decoded instructions as JSON.
    Decode(Input),
    /// Read exactly one instruction and write its static analysis as JSON.
    Analyze(Input),
}

#[derive(clap::Args)]
struct Input {
    /// Guest instruction set.
    #[arg(value_enum)]
    target: Guest,
    /// Guest base address in hexadecimal, for example 0x1000.
    base: Address,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Guest {
    #[value(name = "x86_64")]
    X86_64,
    Aarch64,
}

impl From<Guest> for Target {
    fn from(guest: Guest) -> Self {
        match guest {
            Guest::X86_64 => Self::X86_64,
            Guest::Aarch64 => Self::Aarch64,
        }
    }
}

fn main() -> ExitCode {
    // Keep dependency panic payloads and build-machine paths out of routine stderr.
    std::panic::set_hook(Box::new(|_| eprintln!("engine_backend_panic")));
    let cli = Cli::parse();
    match cli.operation {
        Operation::Run(input) => execute::run(input),
        Operation::Inspect(operation) => inspect(operation),
    }
}

fn inspect(operation: Inspection) -> ExitCode {
    let binary = matches!(operation, Inspection::Assemble(_));
    let response = match dispatch(operation) {
        Ok(response) => response,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let success = !matches!(response.response.result, Reply::Error(_));
    let written = if binary {
        if success {
            response.payloads.get(1).is_some_and(|image| {
                let mut stdout = io::stdout().lock();
                stdout
                    .write_all(image)
                    .and_then(|()| stdout.flush())
                    .is_ok()
            })
        } else {
            write_json_line(io::stderr().lock(), &response.response)
        }
    } else {
        write_json_line(io::stdout().lock(), &response.response)
    };
    if written && success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn write_json_line(writer: impl Write, response: &impl serde::Serialize) -> bool {
    let mut writer = io::BufWriter::new(writer);
    serde_json::to_writer(&mut writer, response).is_ok()
        && writer
            .write_all(b"\n")
            .and_then(|()| writer.flush())
            .is_ok()
}

fn dispatch(operation: Inspection) -> Result<transport::Message, Box<dyn std::error::Error>> {
    let mut worker = Worker::default();
    let hello = worker
        .handle(
            Request {
                id: Counter::new(1),
                command: Command::Hello { version: VERSION },
            }
            .into(),
        )
        .map_err(|_| "worker initialization failed")?;
    let command = match operation {
        Inspection::Capabilities => return Ok(hello),
        Inspection::Assemble(input) => Command::Assemble {
            identity: BuildIdentity {
                document: "stdin".into(),
                revision: Counter::new(0),
                target: input.target.into(),
                base: HexAddress::new(input.base),
                assembler: assembly::identity(),
            },
            source: String::from_utf8(read_input(MAX_SOURCE_BYTES)?)
                .map_err(|_| "source must be UTF-8")?,
        },
        Inspection::Analyze(input) => Command::Analyze {
            target: input.target.into(),
            base: HexAddress::new(input.base),
            bytes: read_input(15)?,
        },
        Inspection::Decode(input) => Command::Decode {
            target: input.target.into(),
            base: HexAddress::new(input.base),
            bytes: read_input(MAX_DECODE_BYTES)?,
            limit: 4096,
        },
    };
    worker
        .handle(
            Request {
                id: Counter::new(2),
                command,
            }
            .into(),
        )
        .map_err(|_| "worker request failed".into())
}

fn read_input(limit: usize) -> Result<Vec<u8>, InputError> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| InputError::Read)?;
    if bytes.len() > limit {
        return Err(InputError::Budget);
    }
    Ok(bytes)
}

#[derive(Debug, thiserror::Error)]
enum InputError {
    #[error("could not read input")]
    Read,
    #[error("input budget exceeded")]
    Budget,
}

impl From<InputError> for oplab_core::protocol::Diagnostic {
    fn from(error: InputError) -> Self {
        use oplab_core::protocol::DiagnosticCode;
        Self::new(match error {
            InputError::Read => DiagnosticCode::InvalidInput,
            InputError::Budget => DiagnosticCode::ResourceLimit,
        })
    }
}
