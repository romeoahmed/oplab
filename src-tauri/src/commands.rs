//! Authorized binary IPC commands. Process waits run outside the `WebView` thread.

use crate::supervisor::Service;
use oplab_core::protocol::{
    MAX_OBJECT_BYTES,
    desktop::{ConnectionInfo, DesktopCall, DesktopFailure, FailureCode},
    frame::Kind,
    scalar::Counter,
    transport,
};
use tauri::{
    Manager, WebviewWindow,
    ipc::{Channel, InvokeBody, Request, Response},
};

type Result<T> = std::result::Result<T, DesktopFailure>;

fn authorize(window: &WebviewWindow) -> Result<()> {
    if window.label() != "main" {
        return Err(DesktopFailure::new(FailureCode::Unauthorized));
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn worker_connect(
    window: WebviewWindow,
    channel: Channel,
    restart: bool,
) -> Result<ConnectionInfo> {
    authorize(&window)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let executable =
            std::env::current_exe().map_err(|_| DesktopFailure::new(FailureCode::Unavailable))?;
        let parent = executable
            .parent()
            .ok_or_else(|| DesktopFailure::new(FailureCode::Unavailable))?;
        app.state::<Service>().connect(
            &parent.join(format!("oplab-worker{}", std::env::consts::EXE_SUFFIX)),
            &app.path()
                .resource_dir()
                .map_err(|_| DesktopFailure::new(FailureCode::Unavailable))?
                .join("runtime"),
            channel,
            restart,
        )
    })
    .await
    .map_err(|_| DesktopFailure::new(FailureCode::Unavailable))?
}

#[tauri::command]
pub(crate) async fn worker_request(
    window: WebviewWindow,
    request: Request<'_>,
    service: tauri::State<'_, Service>,
) -> Result<Response> {
    authorize(&window)?;
    let (call, payload) = decode(request.body())?;
    let ticket = service.request(call.connection, call.view, call.command, payload)?;
    tauri::async_runtime::spawn_blocking(move || {
        let message = ticket.wait()?;
        let mut bytes = Vec::new();
        transport::write_message(&mut bytes, &message)
            .map_err(|_| DesktopFailure::new(FailureCode::Protocol))?;
        Ok(Response::new(bytes))
    })
    .await
    .map_err(|_| DesktopFailure::new(FailureCode::Disconnected))?
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command arguments by value."
)]
#[tauri::command]
pub(crate) fn worker_ack(
    window: WebviewWindow,
    service: tauri::State<'_, Service>,
    connection: Counter,
    view: Counter,
    subscription: Counter,
    sequence: Counter,
) -> Result<()> {
    authorize(&window)?;
    service.acknowledge(connection, view, subscription, sequence)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command arguments by value."
)]
#[tauri::command]
pub(crate) fn worker_detach(
    window: WebviewWindow,
    service: tauri::State<'_, Service>,
    connection: Counter,
    view: Counter,
) -> Result<()> {
    authorize(&window)?;
    service.detach(connection, view);
    Ok(())
}

fn decode(body: &InvokeBody) -> Result<(DesktopCall, Option<Vec<u8>>)> {
    let InvokeBody::Raw(bytes) = body else {
        return Err(DesktopFailure::new(FailureCode::Protocol));
    };
    // The extra metadata allowance is independent of the optional one-MiB payload.
    if bytes.len() > 2 * MAX_OBJECT_BYTES + 1024 {
        return Err(DesktopFailure::new(FailureCode::Protocol));
    }
    let mut input = bytes.as_slice();
    let frame = transport::read_frame(&mut input)
        .map_err(|_| DesktopFailure::new(FailureCode::Protocol))?
        .filter(|frame| frame.kind == Kind::Control)
        .ok_or_else(|| DesktopFailure::new(FailureCode::Protocol))?;
    let call: DesktopCall = serde_json::from_slice(&frame.body)
        .map_err(|_| DesktopFailure::new(FailureCode::Protocol))?;
    let payload = transport::request_payload_length(&call.command)
        .and_then(|length| {
            length
                .map(|length| transport::read_payload(&mut input, length))
                .transpose()
        })
        .map_err(|_| DesktopFailure::new(FailureCode::Protocol))?;
    if !input.is_empty() {
        return Err(DesktopFailure::new(FailureCode::Protocol));
    }
    Ok((call, payload))
}
