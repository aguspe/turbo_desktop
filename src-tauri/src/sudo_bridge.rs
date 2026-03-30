use crate::bridge::{BridgeMessage, BridgeResponse};
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use std::process::Stdio;

/// Handle bridge messages for the "sudo" component.
///
/// Executes commands with administrator privileges on macOS using
/// `osascript` to prompt for the user's password via the system dialog.
///
/// Events:
///   - "execute": Run a command with admin privileges (blocking, returns output)
///   - "spawn": Run a command with admin privileges (streaming stdout/stderr)
pub async fn handle_sudo(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    match message.event.as_str() {
        "execute" => handle_execute(message).await,
        "spawn" => handle_spawn(app, message).await,
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

/// Execute a command with admin privileges and return the full output.
/// Uses macOS `osascript` to prompt for password.
async fn handle_execute(message: &BridgeMessage) -> Result<serde_json::Value, String> {
    let command = message.data["command"]
        .as_str()
        .ok_or("Missing 'command' in sudo execute")?;

    let script = build_osascript_command(command);

    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .await
        .map_err(|e| format!("Failed to run osascript: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code();

    // osascript returns error code -128 when the user cancels the dialog
    if stderr.contains("-128") || stderr.contains("User canceled") {
        return Ok(serde_json::json!({
            "status": "cancelled",
            "stdout": "",
            "stderr": "",
            "code": -128,
        }));
    }

    Ok(serde_json::json!({
        "status": if output.status.success() { "ok" } else { "error" },
        "stdout": stdout.trim_end(),
        "stderr": stderr.trim_end(),
        "code": code,
    }))
}

/// Spawn a command with admin privileges and stream output back.
/// Unlike the shell bridge, this wraps the command in osascript for privilege escalation.
async fn handle_spawn(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    let id = message.data["id"]
        .as_str()
        .ok_or("Missing 'id' in sudo spawn")?
        .to_string();
    let command = message.data["command"]
        .as_str()
        .ok_or("Missing 'command' in sudo spawn")?
        .to_string();

    let script = build_osascript_command(&command);

    let mut child = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn osascript: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let app_handle = app.clone();
    let task_id = id.clone();
    tauri::async_runtime::spawn(async move {
        stream_sudo_output(app_handle, task_id, child, stdout, stderr).await;
    });

    log::info!("Sudo: spawned privileged command with id '{}'", id);
    Ok(serde_json::json!({ "status": "spawned", "id": id }))
}

async fn stream_sudo_output(
    app: tauri::AppHandle,
    id: String,
    mut child: tokio::process::Child,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
) {
    let mut stdout_lines = stdout.map(|s| BufReader::new(s).lines());
    let mut stderr_lines = stderr.map(|s| BufReader::new(s).lines());
    let mut stdout_done = stdout_lines.is_none();
    let mut stderr_done = stderr_lines.is_none();

    loop {
        tokio::select! {
            line = async {
                match stdout_lines.as_mut() {
                    Some(lines) => lines.next_line().await,
                    None => std::future::pending().await,
                }
            }, if !stdout_done => {
                match line {
                    Ok(Some(l)) => {
                        emit_sudo_event(&app, &id, "stdout", serde_json::json!({ "id": id, "line": l }));
                    }
                    Ok(None) => { stdout_done = true; }
                    Err(_) => { stdout_done = true; }
                }
            }
            line = async {
                match stderr_lines.as_mut() {
                    Some(lines) => lines.next_line().await,
                    None => std::future::pending().await,
                }
            }, if !stderr_done => {
                match line {
                    Ok(Some(l)) => {
                        emit_sudo_event(&app, &id, "stderr", serde_json::json!({ "id": id, "line": l }));
                    }
                    Ok(None) => { stderr_done = true; }
                    Err(_) => { stderr_done = true; }
                }
            }
        }

        if stdout_done && stderr_done {
            break;
        }
    }

    match child.wait().await {
        Ok(status) => {
            let code = status.code();
            emit_sudo_event(&app, &id, "exit", serde_json::json!({ "id": id, "code": code }));
        }
        Err(e) => {
            emit_sudo_event(&app, &id, "exit", serde_json::json!({ "id": id, "code": null, "error": e.to_string() }));
        }
    }
}

fn emit_sudo_event(app: &tauri::AppHandle, id: &str, event: &str, data: serde_json::Value) {
    let response = BridgeResponse {
        component: "sudo".to_string(),
        event: event.to_string(),
        data,
    };
    if let Err(e) = app.emit("bridge-response", &response) {
        log::warn!("Sudo: failed to emit event for '{}': {}", id, e);
    }
}

/// Build the osascript command string for privileged execution.
///
/// Uses AppleScript's `do shell script` with `administrator privileges`
/// which triggers the macOS password dialog.
fn build_osascript_command(command: &str) -> String {
    // Escape backslashes and double quotes for AppleScript string
    let escaped = command
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "do shell script \"{}\" with administrator privileges",
        escaped
    )
}
