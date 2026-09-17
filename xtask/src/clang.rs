//! Format native sources and analyze them with their actual compiler configuration.

use super::{
    Result,
    process::{build, cargo, query, run},
};
use cargo_metadata::{Message, MetadataCommand};
use serde::Deserialize;
use std::{collections::BTreeMap, env, fs, path::PathBuf, process::Command};

pub(super) fn format(check: bool) -> Result {
    let name = format!("llvm-config{}", env::consts::EXE_SUFFIX);
    let config = env::var_os("LLVM_SYS_231_PREFIX").map_or_else(
        || PathBuf::from(&name),
        |prefix| PathBuf::from(prefix).join("bin").join(&name),
    );
    let bin = PathBuf::from(query(config, &["--bindir"])?);
    let mut command = Command::new(bin.join(format!("clang-format{}", env::consts::EXE_SUFFIX)));
    if check {
        command.args(["--dry-run", "--Werror"]);
    } else {
        command.arg("-i");
    }
    let mut sources = Vec::new();
    let mut directories = vec![
        PathBuf::from("crates/toolchain/native"),
        PathBuf::from("crates/runtime/native"),
    ];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                directories.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext == "cpp" || ext == "hpp" || ext == "c" || ext == "h")
            {
                sources.push(path);
            }
        }
    }
    sources.sort();
    run(command.args(sources))
}

#[derive(Deserialize)]
struct NativeTools {
    bin: PathBuf,
    environment: BTreeMap<String, String>,
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
    let toolchain = metadata
        .packages
        .iter()
        .find(|package| package.name == "oplab-toolchain")
        .ok_or("toolchain package missing")?;
    let directory = build(cargo().args(["check", "--locked", "-p", "oplab-toolchain"]))?
        .into_iter()
        .find_map(|message| match message {
            Message::BuildScriptExecuted(script) if script.package_id == toolchain.id => {
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
    let tidy = toolchain
        .bin
        .join(format!("clang-tidy{}", env::consts::EXE_SUFFIX));
    run(Command::new(&tidy)
        .envs(toolchain.environment)
        .args(["--quiet", "-p"])
        .arg(directory)
        .args(sources))?;
    lint_runtime(&tidy)
}

fn lint_runtime(tidy: &std::path::Path) -> Result {
    let library = PathBuf::from(env::var_os("OPLAB_QEMU_DIR").ok_or("OPLAB_QEMU_DIR missing")?)
        .canonicalize()?;
    let sdk = library.parent().ok_or("QEMU SDK root missing")?;
    let native = sdk.join("sources/qemu/oplab");
    let mut sources = Vec::new();
    for entry in fs::read_dir("crates/runtime/native")? {
        let entry = entry?;
        let staged = native.join(entry.file_name());
        if fs::read(&staged)? != fs::read(entry.path())? {
            return Err("QEMU adapter sources changed; run cargo xtask sdk before checking".into());
        }
        if staged.extension().is_some_and(|extension| extension == "c") {
            sources.push(staged);
        }
    }
    sources.sort();
    run(Command::new(tidy)
        .args([
            "--quiet",
            "--config-file=crates/runtime/native/.clang-tidy",
            "-p",
        ])
        .arg(sdk.join("qemu-build"))
        .args(sources))
}
