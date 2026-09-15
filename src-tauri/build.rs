//! Generate explicit application-command permissions and desktop assets.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "worker_connect",
            "worker_request",
            "worker_ack",
            "worker_detach",
            "import_file",
            "export_file",
        ]),
    ))?;
    Ok(())
}
