//! Discover native libraries through llvm-config; do not assume platform sonames.

use super::Result;
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) struct Toolchain {
    pub(super) includes: [PathBuf; 2],
    pub(super) bin: PathBuf,
    config: OsString,
    version: String,
    llvm_lib: PathBuf,
    lld_prefix: PathBuf,
    llvm_kind: &'static str,
    lld_kind: &'static str,
}

impl Toolchain {
    pub(super) fn discover() -> Result<Self> {
        let config = configured("LLVM_CONFIG")?.unwrap_or_else(|| "llvm-config".into());
        println!("cargo::rerun-if-env-changed=PATH");
        if Path::new(&config).is_file() {
            println!("cargo::rerun-if-changed={}", config.display());
        }
        let version = query(&config, &["--version"])?;
        if version.split('.').next() != Some("23") {
            return Err("the MC bridge supports LLVM 23; select a compatible LLVM_CONFIG".into());
        }
        let llvm_lib = PathBuf::from(query(&config, &["--libdir"])?);
        let bin = PathBuf::from(query(&config, &["--bindir"])?);
        let config_binary = bin.join(format!("llvm-config{}", env::consts::EXE_SUFFIX));
        println!("cargo::rerun-if-changed={}", config_binary.display());
        let lld_prefix = configured("LLD_PREFIX")?.map_or_else(
            || query(&config, &["--prefix"]).map(PathBuf::from),
            |value| Ok(value.into()),
        )?;
        let includes = [
            PathBuf::from(query(&config, &["--includedir"])?),
            lld_prefix.join("include"),
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
        let llvm_kind = match configured("LLVM_LINK_KIND")? {
            Some(kind) => link_kind(&kind.to_string_lossy())?,
            None => match query(&config, &["--shared-mode"])?.as_str() {
                "shared" => "dylib",
                "static" => "static",
                _ => return Err("llvm-config reported an unknown library mode".into()),
            },
        };
        let lld_kind = if let Some(kind) = configured("LLD_LINK_KIND")? {
            link_kind(&kind.to_string_lossy())?
        } else if llvm_kind == "dylib" && has_shared_lld(&lld_prefix)? {
            "dylib"
        } else {
            "static"
        };
        if llvm_kind == "static" && lld_kind == "dylib" {
            return Err(
                "shared LLD requires shared LLVM to avoid duplicate LLVM runtime state".into(),
            );
        }
        Ok(Self {
            includes,
            bin,
            config,
            version,
            llvm_lib,
            lld_prefix,
            llvm_kind,
            lld_kind,
        })
    }

    pub(super) fn emit_link_metadata(&self) -> Result<()> {
        println!(
            "cargo::rustc-link-search=native={}",
            self.lld_prefix.join("lib").display()
        );
        println!(
            "cargo::rustc-link-search=native={}",
            self.llvm_lib.display()
        );
        for name in ["lldELF", "lldCommon"] {
            println!("cargo::rustc-link-lib={}={name}", self.lld_kind);
        }
        let selection = if self.llvm_kind == "dylib" {
            "--link-shared"
        } else {
            "--link-static"
        };
        for file in query(&self.config, &[selection, "--libnames"])?.split_whitespace() {
            let name = library_name(file).ok_or("unrecognized LLVM library filename")?;
            println!("cargo::rustc-link-lib={}={name}", self.llvm_kind);
        }
        emit_system_libraries(&query(&self.config, &[selection, "--system-libs"])?)?;
        println!("cargo::rustc-env=OPLAB_LLVM_VERSION={}", self.version);
        Ok(())
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

fn query(program: &std::ffi::OsStr, arguments: &[&str]) -> Result<String> {
    let result = Command::new(program).args(arguments).output()?;
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

fn link_kind(value: &str) -> Result<&'static str> {
    match value {
        "static" => Ok("static"),
        "dylib" => Ok("dylib"),
        _ => Err("native link kinds must be 'static' or 'dylib'".into()),
    }
}

fn has_shared_lld(prefix: &Path) -> Result<bool> {
    let os = env::var("CARGO_CFG_TARGET_OS")?;
    Ok(["lldELF", "lldCommon"]
        .iter()
        .all(|name| match os.as_str() {
            "windows" => prefix.join("bin").join(format!("{name}.dll")).is_file(),
            "macos" => prefix
                .join("lib")
                .join(format!("lib{name}.dylib"))
                .is_file(),
            _ => prefix.join("lib").join(format!("lib{name}.so")).is_file(),
        }))
}

fn library_name(file: &str) -> Option<&str> {
    file.strip_suffix(".lib").or_else(|| {
        file.strip_prefix("lib").and_then(|name| {
            name.strip_suffix(".a")
                .or_else(|| name.strip_suffix(".dylib"))
                .or_else(|| name.strip_suffix(".tbd"))
                .or_else(|| name.split_once(".so").map(|(name, _)| name))
        })
    })
}

fn emit_system_libraries(flags: &str) -> Result<()> {
    let arguments = shlex::split(flags).ok_or("malformed llvm-config system libraries")?;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument == "-framework" {
            let name = arguments.next().ok_or("missing framework name")?;
            println!("cargo::rustc-link-lib=framework={name}");
        } else if let Some(directory) = argument.strip_prefix("-L") {
            println!("cargo::rustc-link-search=native={directory}");
        } else if let Some(name) = argument.strip_prefix("-l") {
            println!("cargo::rustc-link-lib={name}");
        } else if Path::new(argument).is_file() {
            let file = Path::new(argument);
            let directory = file.parent().ok_or("missing system library directory")?;
            let name = file
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("invalid library name")?;
            println!("cargo::rustc-link-search=native={}", directory.display());
            if name.contains(".so") {
                // Preserve a versioned ELF soname even without an unversioned symlink.
                println!("cargo::rustc-link-lib=dylib:+verbatim={name}");
            } else {
                let kind = if file.extension().is_some_and(|extension| extension == "a") {
                    "static"
                } else {
                    "dylib"
                };
                let name = library_name(name).ok_or("unsupported system library filename")?;
                println!("cargo::rustc-link-lib={kind}={name}");
            }
        } else if let Some(name) = argument.strip_suffix(".lib") {
            println!("cargo::rustc-link-lib={name}");
        } else {
            return Err(
                format!("unsupported llvm-config system-library argument: {argument}").into(),
            );
        }
    }
    Ok(())
}
