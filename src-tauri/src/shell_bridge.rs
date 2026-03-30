use crate::bridge::{BridgeMessage, BridgeResponse};
use crate::process_manager::ProcessManager;
use std::collections::HashMap;
use std::process::Stdio;
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};

/// Handle bridge messages for the "shell" component.
///
/// Supports spawning child processes with streaming stdout/stderr,
/// killing running processes, and querying process status.
pub async fn handle_shell(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    match message.event.as_str() {
        "spawn" => handle_spawn(app, message).await,
        "kill" => handle_kill(app, message).await,
        "status" => handle_status(app, message).await,
        "list" => handle_list(app).await,
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

async fn handle_spawn(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    let id = message.data["id"]
        .as_str()
        .ok_or("Missing 'id' in shell spawn")?
        .to_string();
    let command = message.data["command"]
        .as_str()
        .ok_or("Missing 'command' in shell spawn")?
        .to_string();
    let args: Vec<String> = message.data["args"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let env: HashMap<String, String> = message.data["env"]
        .as_object()
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let cwd = message.data["cwd"].as_str().map(String::from);

    let pm = app.state::<ProcessManager>();

    // Check if ID is already in use
    if pm.status(&id).await.is_some() {
        return Err(format!("Process ID '{}' is already in use", id));
    }

    // Build the command — run through a login shell so the user's
    // environment (PATH, rbenv, nvm, etc.) is available.
    let shell_command = if args.is_empty() {
        command.clone()
    } else {
        format!("{} {}", command, args.iter().map(|a| shell_escape(a)).collect::<Vec<_>>().join(" "))
    };

    let user_shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = tokio::process::Command::new(&user_shell);
    cmd.arg("-l")
        .arg("-c")
        .arg(&shell_command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(&env);

    if let Some(ref dir) = cwd {
        cmd.current_dir(dir);
    }

    // Spawn the child process
    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn '{}': {}", command, e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Create kill channel
    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();

    // Register in process manager
    pm.register(id.clone(), command.clone(), args.clone(), kill_tx)
        .await;

    // Spawn the background streaming task
    let app_handle = app.clone();
    let task_id = id.clone();
    tauri::async_runtime::spawn(async move {
        stream_process(app_handle, task_id, child, stdout, stderr, kill_rx).await;
    });

    log::info!("Shell: spawned '{}' with id '{}'", command, id);
    Ok(serde_json::json!({ "status": "spawned", "id": id }))
}

async fn stream_process(
    app: tauri::AppHandle,
    id: String,
    mut child: tokio::process::Child,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
    kill_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let mut stdout_lines = stdout.map(|s| BufReader::new(s).lines());
    let mut stderr_lines = stderr.map(|s| BufReader::new(s).lines());
    let mut kill_rx = kill_rx;
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
                        emit_shell_event(&app, &id, "stdout", serde_json::json!({ "id": id, "line": l }));
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
                        emit_shell_event(&app, &id, "stderr", serde_json::json!({ "id": id, "line": l }));
                    }
                    Ok(None) => { stderr_done = true; }
                    Err(_) => { stderr_done = true; }
                }
            }
            _ = &mut kill_rx => {
                let _ = child.kill().await;
                emit_shell_event(&app, &id, "exit", serde_json::json!({ "id": id, "code": null }));
                let pm = app.state::<ProcessManager>();
                pm.mark_exited(&id, None).await;
                return;
            }
        }

        if stdout_done && stderr_done {
            break;
        }
    }

    // Wait for the process to finish and get exit code
    let pm = app.state::<ProcessManager>();
    match child.wait().await {
        Ok(status) => {
            let code = status.code();
            emit_shell_event(&app, &id, "exit", serde_json::json!({ "id": id, "code": code }));
            pm.mark_exited(&id, code).await;
        }
        Err(e) => {
            emit_shell_event(&app, &id, "exit", serde_json::json!({ "id": id, "code": null, "error": e.to_string() }));
            pm.mark_failed(&id, e.to_string()).await;
        }
    }
}

fn emit_shell_event(app: &tauri::AppHandle, id: &str, event: &str, data: serde_json::Value) {
    let response = BridgeResponse {
        component: "shell".to_string(),
        event: event.to_string(),
        data,
    };
    if let Err(e) = app.emit("bridge-response", &response) {
        log::warn!("Shell: failed to emit event for '{}': {}", id, e);
    }
}

async fn handle_kill(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    let id = message.data["id"]
        .as_str()
        .ok_or("Missing 'id' in shell kill")?;

    let pm = app.state::<ProcessManager>();
    pm.kill(id).await?;

    log::info!("Shell: killed process '{}'", id);
    Ok(serde_json::json!({ "status": "killed", "id": id }))
}

async fn handle_status(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    let id = message.data["id"]
        .as_str()
        .ok_or("Missing 'id' in shell status")?;

    let pm = app.state::<ProcessManager>();
    match pm.status(id).await {
        Some(info) => serde_json::to_value(&info).map_err(|e| format!("{}", e)),
        None => Ok(serde_json::json!({ "status": "not_found", "id": id })),
    }
}

async fn handle_list(app: &tauri::AppHandle) -> Result<serde_json::Value, String> {
    let pm = app.state::<ProcessManager>();
    let processes = pm.list().await;
    serde_json::to_value(&processes).map_err(|e| format!("{}", e))
}

/// Escape a string for safe inclusion in a shell command.
fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '@')) {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}
