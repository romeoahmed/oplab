//! Desktop entry point. Native engines remain isolated in the supervised worker.

mod commands;
mod supervisor;

use tauri::Manager;

/// Launch the desktop workbench and reap its worker on application exit.
///
/// # Errors
///
/// Returns an error if desktop application initialization fails.
pub fn run() -> tauri::Result<()> {
    let app = tauri::Builder::default()
        .manage(supervisor::Service::default())
        .invoke_handler(tauri::generate_handler![
            commands::worker_connect,
            commands::worker_request,
            commands::worker_ack,
            commands::worker_detach,
        ])
        .build(tauri::generate_context!())?;
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            app.state::<supervisor::Service>().shutdown();
        }
    });
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/supervisor.rs"]
mod tests;
