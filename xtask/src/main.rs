//! Cross-tool repository tasks; Cargo, Vite, and Tauri retain their native build lifecycles.

mod codegen;
mod native;
mod process;

use process::{cargo, pnpm, run};
use std::{env, path::Path};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Build, verify, and maintain the Oplab workspace.
#[derive(clap::Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(clap::Subcommand)]
enum Task {
    /// Check types, strict lints, unused code, generated contracts, and formatting.
    Check,
    /// Run Rust and Chromium behavioral tests.
    Test {
        /// Test optimized Rust builds.
        #[arg(long)]
        release: bool,
    },
    /// Format Rust, web sources, documentation, and C++.
    Fmt {
        /// Report differences without writing files.
        #[arg(long)]
        check: bool,
    },
    /// Export Rust-owned TypeScript contracts.
    Codegen {
        /// Reject generated declarations that differ from the Rust contract.
        #[arg(long)]
        check: bool,
    },
    /// Build and stage the worker for Tauri (inherits the Tauri hook profile).
    Sidecar {
        /// Build an optimized worker outside a Tauri hook.
        #[arg(long)]
        release: bool,
    },
}

fn main() -> Result {
    use clap::Parser;
    let cli = Cli::parse();
    env::set_current_dir(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("workspace root missing")?,
    )?;
    match cli.task {
        Task::Check => check(),
        Task::Test { release } => test(release),
        Task::Fmt { check } => format(check),
        Task::Codegen { check } => codegen::generate(check),
        Task::Sidecar { release } => native::sidecar(
            release
                || (env::var_os("TAURI_ENV_PLATFORM").is_some()
                    && env::var("TAURI_ENV_DEBUG").as_deref() != Ok("true")),
        ),
    }
}

fn check() -> Result {
    native::sidecar(false)?;
    run(cargo().args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ]))?;
    native::lint()?;
    run(pnpm().arg("check"))?;
    run(pnpm().arg("lint"))?;
    run(pnpm().args(["exec", "knip"]))?;
    codegen::generate(true)?;
    format(true)
}

fn test(release: bool) -> Result {
    native::sidecar(release)?;
    let mut command = cargo();
    command.args(["test", "--workspace", "--locked"]);
    if release {
        command.arg("--release");
    }
    run(&mut command)?;
    run(pnpm().args(["exec", "vitest", "run"]))
}

fn format(check: bool) -> Result {
    let mut rust = cargo();
    rust.args(["fmt", "--all"]);
    if check {
        rust.args(["--", "--check"]);
    }
    run(&mut rust)?;
    let mut web = pnpm();
    web.args(["exec", "oxfmt"]);
    if check {
        web.arg("--check");
    }
    run(web.arg("."))?;
    native::format(check)
}
