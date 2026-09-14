//! Invoke installed tools without a shell or assumptions about Cargo's output directories.

use super::Result;
use cargo_metadata::Message;
use std::{
    env,
    ffi::OsStr,
    process::{Command, Stdio},
};

pub(super) fn cargo() -> Command {
    Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

pub(super) fn pnpm() -> Command {
    Command::new(if cfg!(windows) { "pnpm.cmd" } else { "pnpm" })
}

pub(super) fn run(command: &mut Command) -> Result {
    eprintln!("{command:?}");
    let status = command.status()?;
    if !status.success() {
        return Err(format!("{command:?} exited with {status}").into());
    }
    Ok(())
}

pub(super) fn query(program: impl AsRef<OsStr>, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Err(format!("tool query failed: {}", output.status).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

pub(super) fn build(command: &mut Command) -> Result<Vec<Message>> {
    command
        .arg("--message-format=json")
        .stderr(Stdio::inherit());
    let output = command.output()?;
    let mut messages = Vec::new();
    for message in Message::parse_stream(output.stdout.as_slice()) {
        let message = message?;
        if let Message::CompilerMessage(diagnostic) = &message
            && let Some(rendered) = &diagnostic.message.rendered
        {
            eprint!("{rendered}");
        }
        messages.push(message);
    }
    if !output.status.success() {
        return Err(format!("Cargo failed: {}", output.status).into());
    }
    Ok(messages)
}
