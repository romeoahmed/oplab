//! Stage Cargo's actual executable and analyze cc-rs's actual compiler invocation.

use super::{
    Result,
    process::{build, cargo, query, run},
};
use cargo_metadata::{Message, MetadataCommand};
use serde::Deserialize;
use std::{env, fs, path::PathBuf, process::Command};

pub(super) fn sidecar(release: bool) -> Result {
    let target = env::var("TAURI_ENV_TARGET_TRIPLE")
        .or_else(|_| env::var("CARGO_BUILD_TARGET"))
        .map_or_else(
            |_| {
                query(
                    env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()),
                    &["--print", "host-tuple"],
                )
            },
            Ok,
        )?;
    let mut command = cargo();
    command.args([
        "build",
        "--locked",
        "-p",
        "oplab-engine",
        "--bin",
        "oplab-worker",
        "--target",
        &target,
    ]);
    if release {
        command.arg("--release");
    }
    let executable = build(&mut command)?
        .into_iter()
        .find_map(|message| match message {
            Message::CompilerArtifact(artifact) if artifact.target.name == "oplab-worker" => {
                artifact.executable
            }
            _ => None,
        })
        .ok_or("Cargo did not report the worker executable")?;
    let directory = PathBuf::from("src-tauri/binaries");
    fs::create_dir_all(&directory)?;
    let suffix = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    fs::copy(
        executable,
        directory.join(format!("oplab-worker-{target}{suffix}")),
    )?;
    Ok(())
}

pub(super) fn format(check: bool) -> Result {
    let config = env::var_os("LLVM_CONFIG").unwrap_or_else(|| "llvm-config".into());
    let bin = PathBuf::from(query(config, &["--bindir"])?);
    let mut command = Command::new(bin.join(format!("clang-format{}", env::consts::EXE_SUFFIX)));
    if check {
        command.args(["--dry-run", "--Werror"]);
    } else {
        command.arg("-i");
    }
    let mut sources = fs::read_dir("crates/oplab-engine/native")?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    sources.retain(|path| {
        path.extension()
            .is_some_and(|ext| ext == "cpp" || ext == "hpp")
    });
    sources.sort();
    run(command.args(sources))
}

#[derive(Deserialize)]
struct NativeTools {
    bin: PathBuf,
}
#[derive(Deserialize)]
struct CompileCommand {
    directory: PathBuf,
    file: PathBuf,
}

pub(super) fn lint() -> Result {
    let metadata = MetadataCommand::new()
        .no_deps()
        .other_options(vec!["--locked".into()])
        .exec()?;
    let engine = metadata
        .packages
        .iter()
        .find(|package| package.name == "oplab-engine")
        .ok_or("engine package missing")?;
    let directory = build(cargo().args(["check", "--locked", "-p", "oplab-engine"]))?
        .into_iter()
        .find_map(|message| match message {
            Message::BuildScriptExecuted(script) if script.package_id == engine.id => {
                Some(script.out_dir)
            }
            _ => None,
        })
        .ok_or("Cargo did not report the native build directory")?;
    let toolchain: NativeTools =
        serde_json::from_slice(&fs::read(directory.join("native-tools.json"))?)?;
    let commands: Vec<CompileCommand> =
        serde_json::from_slice(&fs::read(directory.join("compile_commands.json"))?)?;
    let sources: Vec<_> = commands
        .into_iter()
        .filter(|command| command.file.starts_with("native"))
        .map(|command| command.directory.join(command.file))
        .collect();
    if sources.is_empty() {
        return Err("native compilation database contains no application sources".into());
    }
    run(Command::new(
        toolchain
            .bin
            .join(format!("clang-tidy{}", env::consts::EXE_SUFFIX)),
    )
    .args(["--quiet", "-p"])
    .arg(directory)
    .args(sources))
}
