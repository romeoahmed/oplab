//! Keep compilation and analysis on the same cc-rs toolchain configuration.

use super::Result;
use std::{env, fs, path::PathBuf, process::Command};

pub(super) fn configure(build: &mut cc::Build, includes: &[PathBuf]) -> Result<()> {
    let compiler = build.try_get_compiler()?;
    for directory in includes {
        if compiler.is_like_msvc() {
            build
                .flag(format!("/external:I{}", directory.display()))
                .flag("/external:W0");
        } else {
            build.flag("-isystem").flag(directory.as_os_str());
        }
    }
    // cc-rs owns compiler/archiver selection, target flags, CXXFLAGS and stdlib.
    // Only make an implicit Apple SDK explicit for compilation-database consumers.
    if env::var("CARGO_CFG_TARGET_OS")? == "macos"
        && !compiler.args().iter().any(|arg| {
            let arg = arg.to_string_lossy();
            arg.starts_with("-isysroot") || arg.starts_with("--sysroot")
        })
    {
        println!("cargo::rerun-if-env-changed=SDKROOT");
        let sdk = if let Some(value) = env::var_os("SDKROOT") {
            PathBuf::from(value)
        } else {
            let result = Command::new("xcrun")
                .args(["--sdk", "macosx", "--show-sdk-path"])
                .output()?;
            if !result.status.success() {
                return Err("macOS SDK discovery failed; set SDKROOT".into());
            }
            PathBuf::from(String::from_utf8(result.stdout)?.trim())
        };
        if !sdk.is_dir() {
            return Err("SDKROOT must identify an existing macOS SDK directory".into());
        }
        build.flag("-isysroot").flag(sdk.as_os_str());
    }
    Ok(())
}

pub(super) fn write_compilation_database(
    build: &cc::Build,
    llvm: &super::llvm::Toolchain,
) -> Result<()> {
    let compiler = build.try_get_compiler()?;
    let directory = env::current_dir()?;
    let mut commands = Vec::new();
    for file in build.get_files() {
        let mut arguments = vec![compiler.path().to_str().ok_or("non-UTF-8 compiler path")?];
        for argument in compiler.args() {
            arguments.push(argument.to_str().ok_or("non-UTF-8 compiler argument")?);
        }
        arguments.push(if compiler.is_like_msvc() { "/c" } else { "-c" });
        arguments.push(file.to_str().ok_or("non-UTF-8 source path")?);
        commands.push(
            serde_json::json!({"directory": directory, "file": file, "arguments": arguments}),
        );
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo output directory is missing")?);
    fs::write(
        output.join("compile_commands.json"),
        serde_json::to_vec_pretty(&commands)?,
    )?;
    fs::write(
        output.join("native-tools.json"),
        serde_json::to_vec(&serde_json::json!({"bin": llvm.bin}))?,
    )?;
    Ok(())
}
