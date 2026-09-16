//! Keep compilation and analysis on the same cc-rs toolchain configuration.

use super::Result;
use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    fs,
    path::PathBuf,
};

pub(super) fn configure(build: &mut cc::Build, includes: &[PathBuf]) -> Result<()> {
    let compiler = build.try_get_compiler()?;
    if compiler.is_like_msvc() {
        // CXX Result translates exceptions; MSVC must unwind native RAII owners.
        build.flag("/EHsc").flag("/external:W0");
    }
    for directory in includes {
        if compiler.is_like_msvc() {
            let mut flag = OsString::from("/external:I");
            flag.push(directory);
            build.flag(flag);
        } else {
            build.flag("-isystem").flag(directory.as_os_str());
        }
    }
    // cc-rs may omit an implicit Apple SDK; clang-tidy needs its explicit path.
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
            PathBuf::from(super::llvm::query(
                OsStr::new("xcrun"),
                &["--sdk", "macosx", "--show-sdk-path"],
            )?)
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
        let mut arguments = vec![utf8(compiler.path().as_os_str())?];
        for argument in compiler.args() {
            arguments.push(utf8(argument)?);
        }
        arguments.push(if compiler.is_like_msvc() { "/c" } else { "-c" });
        arguments.push(utf8(file.as_os_str())?);
        commands.push(
            serde_json::json!({"directory": directory, "file": file, "arguments": arguments}),
        );
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("Cargo output directory is missing")?);
    fs::write(
        output.join("compile_commands.json"),
        serde_json::to_vec_pretty(&commands)?,
    )?;
    // Compilation databases have no environment field. Preserve cc-rs's SDK/MSVC
    // environment separately so clang-tidy sees the same toolchain headers.
    let environment = compiler
        .env()
        .iter()
        .map(|(key, value)| Ok((utf8(key)?, utf8(value)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    fs::write(
        output.join("native-tools.json"),
        serde_json::to_vec(&serde_json::json!({"bin": llvm.bin, "environment": environment}))?,
    )?;
    Ok(())
}

fn utf8(value: &OsStr) -> Result<&str> {
    value
        .to_str()
        .ok_or_else(|| "native analysis metadata requires UTF-8 paths and arguments".into())
}
