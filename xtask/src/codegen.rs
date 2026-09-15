//! Deterministic schema export belongs to repository tooling, not the domain library.

use super::{
    Result,
    process::{pnpm, run},
};
use oplab_core::protocol::{
    Response,
    desktop::{ConnectionInfo, DesktopCall, DesktopFailure, FileFormat},
    stream::StreamEvent,
};
use std::{collections::BTreeSet, fs, path::Path};
use ts_rs::{Config, TS};

pub(super) fn generate(check: bool) -> Result {
    let temporary = tempfile::tempdir()?;
    let config = Config::default()
        .with_out_dir(temporary.path())
        .with_import_extension(Some("js"));
    Response::export_all(&config)?;
    StreamEvent::export_all(&config)?;
    DesktopCall::export_all(&config)?;
    ConnectionInfo::export_all(&config)?;
    DesktopFailure::export_all(&config)?;
    FileFormat::export_all(&config)?;
    run(pnpm()
        .args(["exec", "oxfmt", "--config", ".oxfmtrc.json"])
        .arg(temporary.path()))?;
    let output = Path::new("src/lib/protocol/generated");
    let expected = files(temporary.path())?;
    let existing = files(output)?;
    let mut changed = expected != existing;
    if !check {
        fs::create_dir_all(output)?;
    }
    for name in &expected {
        let bytes = fs::read(temporary.path().join(name))?;
        let previous = if existing.contains(name) {
            fs::read(output.join(name))?
        } else {
            Vec::new()
        };
        if bytes != previous {
            changed = true;
            if !check {
                fs::write(output.join(name), bytes)?;
            }
        }
    }
    if !check {
        for name in existing.difference(&expected) {
            fs::remove_file(output.join(name))?;
        }
    }
    if check && changed {
        return Err("protocol drift; run cargo xtask codegen".into());
    }
    println!("Verified {} protocol declarations", expected.len());
    Ok(())
}

fn files(directory: &Path) -> Result<BTreeSet<std::ffi::OsString>> {
    match fs::read_dir(directory) {
        Ok(entries) => entries.map(|entry| Ok(entry?.file_name())).collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(error.into()),
    }
}
