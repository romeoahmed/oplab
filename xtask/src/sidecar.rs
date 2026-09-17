//! Build the worker and stage its executable and guest libraries for Tauri.

use super::{
    Result,
    process::{build, cargo, query},
};
use cargo_metadata::Message;
use std::{env, fs, path::PathBuf};

pub(super) fn stage(release: bool) -> Result {
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
        "oplab-runner",
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
    stage_runtime()?;
    Ok(())
}

fn stage_runtime() -> Result {
    let source = env::var_os("OPLAB_QEMU_DIR")
        .ok_or("OPLAB_QEMU_DIR must name the directory containing both QEMU SDK libraries")?;
    let destination = PathBuf::from("src-tauri/runtime");
    fs::create_dir_all(&destination)?;
    for guest in ["x86_64", "aarch64"] {
        let name = super::sdk::library(guest);
        fs::copy(PathBuf::from(&source).join(&name), destination.join(name))?;
    }
    Ok(())
}
