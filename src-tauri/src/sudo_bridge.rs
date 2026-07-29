use crate::bridge::{BridgeMessage, BridgeResponse};
use crate::security;
use crate::window::TurboDesktopConfig;
use std::path::PathBuf;
use std::process::Stdio;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tokio::io::{AsyncBufReadExt, BufReader};

/// Handle bridge messages for the "sudo" component.
///
/// Executes commands with administrator privileges, prompting through the
/// platform's own elevation UI: `osascript` on macOS, `pkexec` (polkit) on
/// Linux, UAC via PowerShell on Windows.
///
/// The bridge is disabled unless `turbo-desktop.config.json` enables it and
/// lists the permitted commands. The system's elevation prompt never shows
/// what is about to run — and may cache the credential afterwards — so an
/// app-level confirmation naming the command runs first unless it is turned
/// off.
///
/// Events:
///   - "execute": Run a command with admin privileges (blocking, returns output)
///   - "spawn": Run a command with admin privileges (streaming stdout/stderr)
pub async fn handle_sudo(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    match message.event.as_str() {
        "execute" => handle_execute(app, message).await,
        "spawn" => handle_spawn(app, message).await,
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

/// Check the command against the configured policy, then ask the user to confirm it.
///
/// `Ok(false)` means the user declined; the caller reports that as a cancellation.
async fn authorize(app: &tauri::AppHandle, command: &str) -> Result<bool, String> {
    let config = app.state::<TurboDesktopConfig>();
    security::authorize_sudo_command(&config.sudo, command).inspect_err(|e| {
        log::warn!("Sudo: {}", e);
    })?;

    if !config.sudo.confirm {
        return Ok(true);
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .message(format!(
            "{} wants to run this command as administrator:\n\n{}",
            config.app_name, command
        ))
        .kind(MessageDialogKind::Warning)
        .title("Administrator privileges requested")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Run".to_string(),
            "Cancel".to_string(),
        ))
        .show(move |confirmed| {
            let _ = tx.send(confirmed);
        });

    rx.await
        .map_err(|e| format!("Sudo confirmation dialog failed: {}", e))
}

/// Response returned when the user declines the confirmation dialog.
fn cancelled() -> serde_json::Value {
    serde_json::json!({
        "status": "cancelled",
        "stdout": "",
        "stderr": "",
        "code": -128,
    })
}

/// Execute a command with admin privileges and return the full output.
async fn handle_execute(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    let command = message.data["command"]
        .as_str()
        .ok_or("Missing 'command' in sudo execute")?;

    if !authorize(app, command).await? {
        return Ok(cancelled());
    }

    let mut elevated = elevated_command(command)?;

    let output = elevated
        .command
        .output()
        .await
        .map_err(|e| format!("Failed to run the elevation helper: {}", e));
    remove_temp_files(&elevated.temp_files);
    let output = output?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code();

    if user_cancelled(code, &stderr) {
        return Ok(cancelled());
    }

    Ok(serde_json::json!({
        "status": if output.status.success() { "ok" } else { "error" },
        "stdout": stdout.trim_end(),
        "stderr": stderr.trim_end(),
        "code": code,
    }))
}

/// Spawn a command with admin privileges and stream output back.
///
/// On macOS and Linux the lines arrive as the command produces them. On
/// Windows an elevated child cannot write to a non-elevated parent's pipes,
/// so the output is captured to files and streamed once the command finishes.
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

    if !authorize(app, &command).await? {
        return Ok(serde_json::json!({ "status": "cancelled", "id": id }));
    }

    let mut elevated = elevated_command(&command)?;

    let spawned = elevated
        .command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            remove_temp_files(&elevated.temp_files);
            return Err(format!("Failed to spawn the elevation helper: {}", e));
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let app_handle = app.clone();
    let task_id = id.clone();
    let temp_files = elevated.temp_files;
    tauri::async_runtime::spawn(async move {
        stream_sudo_output(app_handle, task_id, child, stdout, stderr).await;
        remove_temp_files(&temp_files);
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

fn emit_sudo_event(app: &tauri::AppHandle, _id: &str, event: &str, data: serde_json::Value) {
    crate::bridge::broadcast_response(
        app,
        &BridgeResponse {
            component: "sudo".to_string(),
            event: event.to_string(),
            data,
        },
    );
}

/// A process that runs the command elevated, plus any temp files that must be
/// removed once it has finished.
struct ElevatedCommand {
    command: tokio::process::Command,
    /// Only Windows uses these — see the Windows `elevated_command`.
    temp_files: Vec<PathBuf>,
}

fn remove_temp_files(files: &[PathBuf]) {
    for file in files {
        let _ = std::fs::remove_file(file);
    }
}

/// macOS: AppleScript's `do shell script … with administrator privileges`,
/// which triggers the system password dialog. Output flows through
/// osascript's own stdout/stderr.
#[cfg(target_os = "macos")]
fn elevated_command(command: &str) -> Result<ElevatedCommand, String> {
    let mut cmd = tokio::process::Command::new("osascript");
    cmd.arg("-e").arg(build_osascript_command(command));
    Ok(ElevatedCommand {
        command: cmd,
        temp_files: Vec::new(),
    })
}

/// Build the osascript command string for privileged execution.
#[cfg(target_os = "macos")]
fn build_osascript_command(command: &str) -> String {
    // Escape backslashes and double quotes for AppleScript string
    let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "do shell script \"{}\" with administrator privileges",
        escaped
    )
}

/// Linux: `pkexec`, whose polkit agent shows the authentication dialog on
/// every desktop distribution. The command goes through `sh -c` so a command
/// line behaves the same as on the other platforms, and output flows through
/// pkexec's own stdout/stderr. Exit 126 means the user dismissed the dialog.
#[cfg(target_os = "linux")]
fn elevated_command(command: &str) -> Result<ElevatedCommand, String> {
    let mut cmd = tokio::process::Command::new("pkexec");
    cmd.arg("sh").arg("-c").arg(command);
    Ok(ElevatedCommand {
        command: cmd,
        temp_files: Vec::new(),
    })
}

/// Windows: UAC via PowerShell's `Start-Process -Verb RunAs`.
///
/// An elevated child cannot inherit a non-elevated parent's pipes or
/// environment, so the command is written verbatim into a batch file — no
/// escaping to get wrong — which redirects its output to temp files. The
/// PowerShell wrapper waits, prints those files to its own stdout/stderr, and
/// forwards the exit code. A declined UAC prompt raises ERROR_CANCELLED,
/// which the wrapper turns into exit 1223, the Windows code for it.
#[cfg(windows)]
fn elevated_command(command: &str) -> Result<ElevatedCommand, String> {
    let (batch, out, err) = windows_temp_paths();

    std::fs::write(&batch, windows_batch_script(command, &out, &err))
        .map_err(|e| format!("Could not write the elevation batch file: {}", e))?;

    let mut cmd = tokio::process::Command::new("powershell");
    cmd.arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(windows_elevation_script(&batch, &out, &err));
    Ok(ElevatedCommand {
        command: cmd,
        temp_files: vec![batch, out, err],
    })
}

/// Unique sibling paths in the temp directory for one elevation run.
#[cfg(windows)]
fn windows_temp_paths() -> (PathBuf, PathBuf, PathBuf) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let base = std::env::temp_dir().join(format!(
        "turbo-desktop-sudo-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    (
        base.with_extension("cmd"),
        base.with_extension("out"),
        base.with_extension("err"),
    )
}

#[cfg(windows)]
fn windows_batch_script(command: &str, out: &std::path::Path, err: &std::path::Path) -> String {
    format!(
        "@echo off\r\n{} 1>\"{}\" 2>\"{}\"\r\n",
        command,
        out.display(),
        err.display()
    )
}

#[cfg(windows)]
fn windows_elevation_script(
    batch: &std::path::Path,
    out: &std::path::Path,
    err: &std::path::Path,
) -> String {
    format!(
        "try {{ \
           $p = Start-Process -FilePath '{}' -Verb RunAs -Wait -PassThru -WindowStyle Hidden; \
           Get-Content -ErrorAction SilentlyContinue '{}' | Write-Output; \
           Get-Content -ErrorAction SilentlyContinue '{}' | ForEach-Object {{ [Console]::Error.WriteLine($_) }}; \
           exit $p.ExitCode \
         }} catch {{ exit 1223 }}",
        batch.display(),
        out.display(),
        err.display()
    )
}

/// Whether the exit means the user declined the system's elevation prompt,
/// which the bridge reports as a cancellation rather than a failure.
fn user_cancelled(code: Option<i32>, stderr: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        // osascript reports the dialog's cancel as error -128.
        let _ = code;
        stderr.contains("-128") || stderr.contains("User canceled")
    }
    #[cfg(target_os = "linux")]
    {
        // pkexec: 126 is "dialog dismissed"; the agent also says so on stderr.
        code == Some(126) || stderr.contains("Request dismissed")
    }
    #[cfg(windows)]
    {
        // ERROR_CANCELLED, forwarded by the PowerShell wrapper.
        let _ = stderr;
        code == Some(1223)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn the_applescript_quotes_the_command() {
        let script = build_osascript_command("echo \"hi\" C:\\path");
        assert_eq!(
            script,
            "do shell script \"echo \\\"hi\\\" C:\\\\path\" with administrator privileges"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn elevation_runs_through_osascript() {
        let elevated = elevated_command("softwareupdate -l").unwrap();
        assert_eq!(elevated.command.as_std().get_program(), "osascript");
        assert!(elevated.temp_files.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn elevation_runs_through_pkexec() {
        let elevated = elevated_command("apt-get update").unwrap();
        let std = elevated.command.as_std();
        assert_eq!(std.get_program(), "pkexec");
        let args: Vec<_> = std.get_args().collect();
        assert_eq!(args, ["sh", "-c", "apt-get update"]);
        assert!(elevated.temp_files.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn elevation_runs_through_a_uac_prompt() {
        let elevated = elevated_command("ipconfig /flushdns").unwrap();
        let std = elevated.command.as_std();
        assert_eq!(std.get_program(), "powershell");
        let script = std.get_args().last().unwrap().to_string_lossy().to_string();
        assert!(script.contains("-Verb RunAs"));
        assert!(script.contains("1223"));

        // The command itself lives in the batch file, verbatim.
        let [batch, _, _] = &elevated.temp_files[..] else {
            panic!("expected batch, out and err temp files");
        };
        let contents = std::fs::read_to_string(batch).unwrap();
        assert!(contents.contains("ipconfig /flushdns 1>"));
        remove_temp_files(&elevated.temp_files);
    }

    #[test]
    fn a_declined_prompt_reads_as_cancelled() {
        #[cfg(target_os = "macos")]
        assert!(user_cancelled(
            Some(1),
            "execution error: User canceled. (-128)"
        ));
        #[cfg(target_os = "linux")]
        assert!(user_cancelled(Some(126), ""));
        #[cfg(windows)]
        assert!(user_cancelled(Some(1223), ""));
    }

    #[test]
    fn an_ordinary_failure_is_not_a_cancellation() {
        assert!(!user_cancelled(Some(1), "No such file or directory"));
        assert!(!user_cancelled(Some(0), ""));
    }
}
