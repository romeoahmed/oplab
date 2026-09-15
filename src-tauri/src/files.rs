//! User-selected files. Paths stay native; reads and replacements are bounded.

use oplab_core::protocol::{MAX_OBJECT_BYTES, MAX_SOURCE_BYTES, desktop::FileFormat};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
use tauri::WebviewWindow;
use tauri_plugin_dialog::DialogExt;

type Result<T> = std::result::Result<T, &'static str>;

const fn limit(format: FileFormat) -> usize {
    match format {
        FileFormat::Source => MAX_SOURCE_BYTES,
        FileFormat::Binary | FileFormat::Object | FileFormat::Image => MAX_OBJECT_BYTES,
    }
}

const fn extension(format: FileFormat) -> &'static str {
    match format {
        FileFormat::Source => "s",
        FileFormat::Binary => "bin",
        FileFormat::Object => "o",
        FileFormat::Image => "elf",
    }
}

fn validate(bytes: &[u8], format: FileFormat) -> Result<()> {
    if bytes.len() > limit(format) || (bytes.is_empty() && format != FileFormat::Source) {
        return Err("file_size");
    }
    if format == FileFormat::Source && std::str::from_utf8(bytes).is_err() {
        return Err("file_encoding");
    }
    Ok(())
}

fn read(path: &Path, format: FileFormat) -> Result<Vec<u8>> {
    // Reject special files before opening: a FIFO can block during open itself.
    if !std::fs::metadata(path).map_err(|_| "file_read")?.is_file() {
        return Err("file_read");
    }
    let file = File::open(path).map_err(|_| "file_read")?;
    if !file.metadata().map_err(|_| "file_read")?.is_file() {
        return Err("file_read");
    }
    let mut bytes = Vec::new();
    file.take(limit(format) as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "file_read")?;
    validate(&bytes, format)?;
    Ok(bytes)
}

fn write(path: &Path, bytes: &[u8], format: FileFormat) -> Result<()> {
    validate(bytes, format)?;
    let parent = path.parent().ok_or("file_write")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| "file_write")?;
    temporary.write_all(bytes).map_err(|_| "file_write")?;
    temporary.as_file().sync_all().map_err(|_| "file_write")?;
    temporary.persist(path).map_err(|_| "file_write")?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn import_file(
    window: WebviewWindow,
    format: FileFormat,
    title: String,
) -> Result<Option<Vec<u8>>> {
    if window.label() != "main"
        || !matches!(format, FileFormat::Source | FileFormat::Binary)
        || title.len() > 256
    {
        return Err("file_read");
    }
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = window
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(title)
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|_| "file_read")?;
        read(&path, format).map(Some)
    })
    .await
    .map_err(|_| "file_read")?
}

#[tauri::command]
pub(crate) async fn export_file(
    window: WebviewWindow,
    format: FileFormat,
    title: String,
    bytes: Vec<u8>,
) -> Result<bool> {
    if window.label() != "main" || title.len() > 256 {
        return Err("file_write");
    }
    validate(&bytes, format)?;
    tauri::async_runtime::spawn_blocking(move || {
        let suffix = extension(format);
        let Some(file) = window
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(title)
            .set_file_name(format!("oplab.{suffix}"))
            .add_filter(suffix, &[suffix])
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file.into_path().map_err(|_| "file_write")?;
        write(&path, &bytes, format)?;
        Ok(true)
    })
    .await
    .map_err(|_| "file_write")?
}

#[cfg(test)]
#[path = "../tests/unit/files.rs"]
mod tests;
