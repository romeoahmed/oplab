//! Use llvm-sys discovery for LLVM headers; link matching LLD and static XED libraries.

use super::Result;
use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Command,
};

pub(super) struct Toolchain {
    pub(super) includes: Vec<PathBuf>,
    pub(super) bin: PathBuf,
    version: String,
    xed_prefix: PathBuf,
    lld_prefix: PathBuf,
    lld_kind: &'static str,
}

impl Toolchain {
    pub(super) fn discover() -> Result<Self> {
        let config = env::var_os("DEP_LLVM_23_CONFIG_PATH")
            .ok_or("llvm-sys did not report its LLVM configuration")?;
        println!("cargo::rerun-if-changed={}", config.display());
        let version = query(&config, &["--version"])?;
        if version.split('.').next() != Some("23") {
            return Err(
                "the MC bridge supports LLVM 23; select a compatible LLVM_SYS_231_PREFIX".into(),
            );
        }
        let targets = query(&config, &["--targets-built"])?;
        if !["X86", "AArch64"]
            .iter()
            .all(|required| targets.split_whitespace().any(|target| target == *required))
        {
            return Err("LLVM must include both X86 and AArch64 targets".into());
        }
        let bin = PathBuf::from(query(&config, &["--bindir"])?);
        let lld_prefix = configured("LLD_PREFIX")?.map_or_else(
            || query(&config, &["--prefix"]).map(PathBuf::from),
            |value| Ok(value.into()),
        )?;
        let xed_prefix = configured("XED_PREFIX")?
            .map(PathBuf::from)
            .ok_or("XED_PREFIX must select an Intel XED SDK")?;
        if !xed_prefix.join("include/xed/xed-interface.h").is_file() {
            return Err("XED development headers are missing".into());
        }
        let includes = vec![
            PathBuf::from(query(&config, &["--includedir"])?),
            lld_prefix.join("include"),
            xed_prefix.join("include"),
        ];
        if !includes[1].join("lld/Common/Driver.h").is_file() {
            return Err("LLD development headers are missing; select LLD_PREFIX".into());
        }
        // LLD's C++ API requires a matching LLVM release and compatible ABI.
        let linker = lld_prefix
            .join("bin")
            .join(format!("ld.lld{}", env::consts::EXE_SUFFIX));
        println!("cargo::rerun-if-changed={}", linker.display());
        println!(
            "cargo::rerun-if-changed={}",
            includes[0].join("llvm/Config/llvm-config.h").display()
        );
        let reported = query(linker.as_os_str(), &["--version"])?;
        let mut words = reported.split_whitespace();
        let lld_version = words.find(|word| *word == "LLD").and_then(|_| words.next());
        if lld_version != Some(version.as_str()) {
            return Err("LLVM and LLD must come from the same release".into());
        }
        // llvm-sys owns LLVM linkage with the workspace's prefer-dynamic policy.
        // Its metadata does not expose link kind; the same config probe tells us
        // whether a shared LLD can safely use that LLVM installation.
        let shared_llvm = query(&config, &["--link-shared", "--libnames"]).is_ok();
        let lld_kind = match configured("LLD_LINK_KIND")?.as_deref() {
            Some(kind) if kind == "static" => "static",
            Some(kind) if kind == "dylib" && shared_llvm => "dylib",
            Some(kind) if kind == "dylib" => return Err("shared LLD requires shared LLVM".into()),
            Some(_) => return Err("LLD_LINK_KIND must be 'static' or 'dylib'".into()),
            None if shared_llvm && has_shared_lld(&lld_prefix)? => "dylib",
            None => "static",
        };
        Ok(Self {
            includes,
            bin,
            version,
            xed_prefix,
            lld_prefix,
            lld_kind,
        })
    }

    pub(super) fn emit_link_metadata(&self) {
        println!(
            "cargo::rustc-link-search=native={}",
            self.xed_prefix.join("lib").display()
        );
        println!("cargo::rustc-link-lib=static=xed");
        println!("cargo::rerun-if-changed={}", self.xed_prefix.display());
        println!(
            "cargo::rustc-link-search=native={}",
            self.lld_prefix.join("lib").display()
        );
        for name in ["lldELF", "lldCommon"] {
            println!("cargo::rustc-link-lib={}={name}", self.lld_kind);
        }
        println!("cargo::rustc-env=OPLAB_LLVM_VERSION={}", self.version);
    }
}

/// Match cc-rs's documented target-specific environment precedence.
fn configured(name: &str) -> Result<Option<OsString>> {
    let target = env::var("TARGET")?;
    let scope = if env::var("HOST")? == target {
        "HOST"
    } else {
        "TARGET"
    };
    let keys = [
        format!("{name}_{target}"),
        format!("{name}_{}", target.replace(['-', '.'], "_")),
        format!("{scope}_{name}"),
        name.into(),
    ];
    for key in keys {
        println!("cargo::rerun-if-env-changed={key}");
        if let Some(value) = env::var_os(&key) {
            if value.is_empty() {
                return Err(format!("{key} must not be empty").into());
            }
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub(super) fn query(program: &OsStr, arguments: &[&str]) -> Result<String> {
    let result = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| {
            format!(
                "could not run {} {}: {error}",
                program.display(),
                arguments.join(" ")
            )
        })?;
    if !result.status.success() {
        return Err(format!(
            "{} {} failed: {}",
            program.display(),
            arguments.join(" "),
            String::from_utf8_lossy(&result.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(result.stdout)?.trim().into())
}

fn has_shared_lld(prefix: &Path) -> Result<bool> {
    let os = env::var("CARGO_CFG_TARGET_OS")?;
    Ok(["lldELF", "lldCommon"]
        .iter()
        .all(|name| match os.as_str() {
            "windows" => {
                prefix.join("bin").join(format!("{name}.dll")).is_file()
                    && prefix.join("lib").join(format!("{name}.lib")).is_file()
            }
            "macos" => prefix
                .join("lib")
                .join(format!("lib{name}.dylib"))
                .is_file(),
            _ => prefix.join("lib").join(format!("lib{name}.so")).is_file(),
        }))
}
