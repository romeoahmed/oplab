//! Versioned native SDK builds with build-graph integration in owned checkouts.
use crate::{Result, process::run};
use cargo_metadata::MetadataCommand;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(clap::Args)]
pub(super) struct Options {
    /// SDK output directory (defaults to Cargo's target/native-sdk).
    #[arg(long)]
    prefix: Option<PathBuf>,
    /// Local QEMU Git repository to clone instead of downloading it.
    #[arg(long)]
    qemu_source: Option<PathBuf>,
    /// Local XED Git repository to clone instead of downloading it.
    #[arg(long)]
    xed_source: Option<PathBuf>,
    /// Local mbuild Git repository to clone instead of downloading it.
    #[arg(long)]
    mbuild_source: Option<PathBuf>,
    /// Instrument QEMU and the adapter with `AddressSanitizer`.
    #[arg(long)]
    asan: bool,
}

pub(super) fn build(options: Options) -> Result {
    let prefix = match options.prefix {
        Some(prefix) => prefix,
        None => MetadataCommand::new()
            .no_deps()
            .other_options(vec!["--locked".into()])
            .exec()?
            .target_directory
            .join("native-sdk")
            .into(),
    };
    fs::create_dir_all(&prefix)?;
    let prefix = prefix.canonicalize()?;
    let sources = prefix.join("sources");
    fs::create_dir_all(&sources)?;
    let qemu = checkout(&sources, "qemu", "v11.1.1", options.qemu_source.as_deref())?;
    let xed = checkout(
        &sources,
        "xed",
        "v2026.08.23",
        options.xed_source.as_deref(),
    )?;
    let mbuild = checkout(
        &sources,
        "mbuild",
        "v2026.08.23",
        options.mbuild_source.as_deref(),
    )?;
    build_xed(&prefix, &xed, &mbuild)?;
    build_qemu(&prefix, &qemu, options.asan)?;
    println!(
        "SDK ready: {}\nSet XED_PREFIX to its xed directory and OPLAB_QEMU_DIR to its lib directory.",
        prefix.display()
    );
    Ok(())
}

fn checkout(root: &Path, name: &str, tag: &str, local: Option<&Path>) -> Result<PathBuf> {
    let path = root.join(name);
    if !path.exists() {
        let remote = match name {
            "qemu" => "https://gitlab.com/qemu-project/qemu.git",
            "xed" => "https://github.com/intelxed/xed.git",
            _ => "https://github.com/intelxed/mbuild.git",
        };
        run(Command::new("git")
            .args(["clone", "--branch", tag, "--depth", "1"])
            .arg(local.map_or_else(|| Path::new(remote), |p| p))
            .arg(&path))?;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&path)
        .args(["describe", "--tags", "--exact-match"])
        .output()?;
    if !output.status.success() || String::from_utf8(output.stdout)?.trim() != tag {
        return Err(format!("{} must be checked out at {tag}", path.display()).into());
    }
    Ok(path)
}

fn build_xed(prefix: &Path, source: &Path, mbuild: &Path) -> Result {
    let python = env::var_os("PYTHON").unwrap_or_else(|| "python3".into());
    let mut command = Command::new(python);
    command
        .current_dir(source)
        .env("PYTHONPATH", mbuild)
        .args(["mfile.py", "--static", "install"])
        .arg(format!("--install-dir={}", prefix.join("xed").display()))
        .arg(format!("-j{}", std::thread::available_parallelism()?));
    for (variable, option) in [("CC", "--cc"), ("CXX", "--cxx"), ("AR", "--ar")] {
        if let Ok(value) = env::var(variable) {
            command.arg(format!("{option}={value}"));
        }
    }
    run(&mut command)
}

fn build_qemu(prefix: &Path, source: &Path, asan: bool) -> Result {
    let adapter = source.join("oplab");
    fs::create_dir_all(&adapter)?;
    for entry in fs::read_dir("crates/runtime/native")? {
        let entry = entry?;
        write_changed(&adapter.join(entry.file_name()), &fs::read(entry.path())?)?;
    }
    // QEMU has no public embedding target. Extend only its build graph, using
    // the original file each time so subsequent builds cannot duplicate targets.
    let original = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["show", "HEAD:meson.build"])
        .output()?;
    if !original.status.success() {
        return Err("cannot read QEMU's build graph".into());
    }
    let original = String::from_utf8(original.stdout)?;
    let marker = "  if target.endswith('-softmmu')\n    execs = [{";
    if original.matches(marker).count() != 1 {
        return Err("QEMU build integration point changed".into());
    }
    let target = fs::read_to_string(adapter.join("meson.build"))?;
    write_changed(
        &source.join("meson.build"),
        original
            .replacen(marker, &format!("{target}\n{marker}"), 1)
            .as_bytes(),
    )?;
    // MAX ARM's architectural helpers reference the GICv5 CPU interface.
    write_changed(
        &source.join("configs/devices/aarch64-softmmu/default.mak"),
        b"CONFIG_ARM_GICV5=y\n",
    )?;
    let build = prefix.join("qemu-build");
    fs::create_dir_all(&build)?;
    let mut configure = Command::new(source.join("configure"));
    configure.current_dir(&build).args([
        "--target-list=x86_64-softmmu,aarch64-softmmu",
        "--without-default-features",
        "--without-default-devices",
        "--enable-tcg",
        "--disable-docs",
        "--disable-tools",
        "--disable-guest-agent",
    ]);
    if asan {
        configure.arg("--enable-asan");
    }
    for (variable, option) in [("CC", "--cc"), ("CXX", "--cxx")] {
        if let Ok(value) = env::var(variable) {
            configure.arg(format!("{option}={value}"));
        }
    }
    if build.join("build.ninja").exists() {
        let meson = build.join(if cfg!(windows) {
            "pyvenv/Scripts/meson.exe"
        } else {
            "pyvenv/bin/meson"
        });
        run(Command::new(meson)
            .args(["setup", "--reconfigure", "--clearcache"])
            .arg(&build)
            .arg(source)
            .arg(format!("-Dasan={asan}")))?;
    } else {
        run(&mut configure)?;
    }
    let names = ["x86_64", "aarch64"].map(library);
    run(Command::new("ninja").arg("-C").arg(&build).args(&names))?;
    fs::create_dir_all(prefix.join("lib"))?;
    for name in names {
        fs::copy(build.join(&name), prefix.join("lib").join(name))?;
    }
    Ok(())
}

// Preserve timestamps so Ninja can reuse unchanged adapter objects and build rules.
fn write_changed(path: &Path, bytes: &[u8]) -> Result {
    if !fs::read(path).is_ok_and(|previous| previous == bytes) {
        fs::write(path, bytes)?;
    }
    Ok(())
}

pub(super) fn library(guest: &str) -> String {
    format!(
        "{}oplab-qemu-{guest}{}",
        env::consts::DLL_PREFIX,
        env::consts::DLL_SUFFIX
    )
}
