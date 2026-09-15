//! Machine-readable assembly and instruction inspection through the worker's operations.

use clap::Parser;
use oplab_core::{
    address::Address,
    protocol::{
        BuildIdentity, Command, MAX_DECODE_BYTES, MAX_SOURCE_BYTES, Reply, Request, Response,
        VERSION,
        scalar::{Counter, HexAddress},
    },
};
use oplab_core::{protocol::transport, target::Target};
use oplab_engine::{assembly, worker::Worker};
use std::{
    io::{self, Read, Write},
    process::ExitCode,
};

/// Assemble ELF images, disassemble machine code or analyze an instruction from stdin.
#[derive(clap::Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    operation: Operation,
}

#[derive(clap::Subcommand)]
enum Operation {
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
    let binary = matches!(cli.operation, Operation::Assemble(_));
    let response = match run(cli.operation) {
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

fn write_json_line(mut writer: impl Write, response: &Response) -> bool {
    transport::encode_json(response).is_ok_and(|bytes| {
        writer
            .write_all(&bytes)
            .and_then(|()| writer.write_all(b"\n"))
            .and_then(|()| writer.flush())
            .is_ok()
    })
}

fn run(operation: Operation) -> Result<transport::Message, &'static str> {
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
        Operation::Capabilities => return Ok(hello),
        Operation::Assemble(input) => Command::Assemble {
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
        Operation::Analyze(input) => Command::Analyze {
            target: input.target.into(),
            base: HexAddress::new(input.base),
            bytes: read_input(15)?,
        },
        Operation::Decode(input) => Command::Decode {
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
        .map_err(|_| "worker request failed")
}

fn read_input(limit: usize) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "could not read input")?;
    if bytes.len() > limit {
        return Err("input budget exceeded");
    }
    Ok(bytes)
}
