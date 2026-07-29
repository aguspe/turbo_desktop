//! Starting the app server, so the desktop app is something you can just open.
//!
//! Without this, every launch depends on someone having run `rails server` in
//! another terminal first, which is a strange thing to ask of a desktop app.
//!
//! Everything needed was already here: the reachability probe decides whether a
//! server is wanted, ProcessManager owns the child and reaps it on quit, and the
//! connection monitor moves the window off the waiting page once the server
//! answers. This only decides whether to start one and does so.

use crate::window::ServerConfig;
use std::path::{Path, PathBuf};

/// Whether to start a server, and why not when we won't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Start,
    /// Something is already listening — very likely a server the developer is
    /// running themselves, which we must not duplicate or later kill.
    AlreadyRunning,
    /// No command configured, so the app expects a server it does not own.
    NotConfigured,
}

pub fn decide(config: &ServerConfig, reachable: bool) -> Decision {
    if config.command.as_deref().unwrap_or("").trim().is_empty() {
        return Decision::NotConfigured;
    }
    if reachable {
        return Decision::AlreadyRunning;
    }
    Decision::Start
}

/// Where the command runs, resolved against the directory the config was read from.
///
/// The scaffold puts the config in `desktop/` and the Rails app one level up, so
/// `..` is the usual answer and the default.
pub fn working_directory(config: &ServerConfig, config_dir: Option<&Path>) -> Option<PathBuf> {
    let base = config_dir?;
    let relative = config.directory.as_deref().unwrap_or("..");
    let joined = base.join(relative);

    // Fall back to the lexical path when the directory does not exist yet, so the
    // failure is reported by the spawn rather than swallowed here.
    Some(joined.canonicalize().unwrap_or(joined))
}

/// Start the configured server and hand it to ProcessManager.
///
/// The command runs the way the platform runs commands — through a login shell
/// on Unix (a version manager only sets itself up in a configured shell),
/// through `cmd` on Windows. See [`crate::shell_bridge::shell_invocation`].
pub async fn start(
    app: &tauri::AppHandle,
    config: &ServerConfig,
    config_dir: Option<&Path>,
) -> Result<(), String> {
    use std::process::Stdio;
    use tauri::Manager;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let Some(command) = config.command.as_deref() else {
        return Ok(());
    };
    let directory = working_directory(config, config_dir);

    let (program, args) = crate::shell_bridge::shell_invocation(command);
    let mut spawner = tokio::process::Command::new(&program);
    spawner
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = &directory {
        spawner.current_dir(dir);
    }

    log::info!(
        "Starting the app server: {} (in {})",
        command,
        directory
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| "the working directory".into())
    );

    let mut child = spawner
        .spawn()
        .map_err(|e| format!("Could not start the app server: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Registered so quitting the app stops the server it started.
    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();
    app.state::<crate::process_manager::ProcessManager>()
        .register(
            SERVER_PROCESS_ID.to_string(),
            command.to_string(),
            Vec::new(),
            kill_tx,
        )
        .await?;

    tauri::async_runtime::spawn(async move {
        let mut kill_rx = kill_rx;

        // The server's own output is the only clue when it fails to boot, so it
        // goes to the log rather than into a pipe nobody reads.
        let mut out = stdout.map(|s| BufReader::new(s).lines());
        let mut err = stderr.map(|s| BufReader::new(s).lines());

        loop {
            tokio::select! {
                line = async { match out.as_mut() { Some(l) => l.next_line().await, None => std::future::pending().await } } => {
                    match line { Ok(Some(l)) => log::info!("[server] {}", l), _ => out = None }
                }
                line = async { match err.as_mut() { Some(l) => l.next_line().await, None => std::future::pending().await } } => {
                    match line { Ok(Some(l)) => log::warn!("[server] {}", l), _ => err = None }
                }
                _ = &mut kill_rx => {
                    log::info!("Stopping the app server");
                    let _ = child.kill().await;
                    return;
                }
                status = child.wait() => {
                    log::warn!("The app server exited: {:?}", status.ok().and_then(|s| s.code()));
                    return;
                }
            }
        }
    });

    Ok(())
}

/// ProcessManager id for the server, so it is distinguishable from anything the
/// web layer spawns through the shell bridge.
pub const SERVER_PROCESS_ID: &str = "turbo-desktop:app-server";

#[cfg(test)]
mod tests {
    use super::*;

    fn configured() -> ServerConfig {
        ServerConfig {
            command: Some("bin/rails server".into()),
            directory: None,
        }
    }

    #[test]
    fn nothing_to_start_without_a_command() {
        assert_eq!(
            decide(&ServerConfig::default(), false),
            Decision::NotConfigured
        );
        assert_eq!(
            decide(
                &ServerConfig {
                    command: Some("   ".into()),
                    directory: None
                },
                false
            ),
            Decision::NotConfigured
        );
    }

    #[test]
    fn starts_when_nothing_is_listening() {
        assert_eq!(decide(&configured(), false), Decision::Start);
    }

    #[test]
    fn leaves_a_server_someone_else_is_running_alone() {
        // Starting a second one would fail on the port, and quitting the app
        // would kill a server the developer started by hand.
        assert_eq!(decide(&configured(), true), Decision::AlreadyRunning);
    }

    #[test]
    fn the_rails_app_is_a_level_above_the_config_by_default() {
        let dir = std::env::temp_dir().join("turbo-desktop-server-default");
        let desktop = dir.join("desktop");
        std::fs::create_dir_all(&desktop).unwrap();

        let resolved = working_directory(&configured(), Some(&desktop)).unwrap();

        assert_eq!(resolved, dir.canonicalize().unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_explicit_directory_wins() {
        let dir = std::env::temp_dir().join("turbo-desktop-server-explicit");
        let api = dir.join("api");
        std::fs::create_dir_all(&api).unwrap();

        let config = ServerConfig {
            command: Some("bin/rails server".into()),
            directory: Some("api".into()),
        };

        assert_eq!(
            working_directory(&config, Some(&dir)).unwrap(),
            api.canonicalize().unwrap()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_directory_is_left_for_the_spawn_to_report() {
        let dir = std::env::temp_dir().join("turbo-desktop-server-missing");
        let config = ServerConfig {
            command: Some("bin/rails server".into()),
            directory: Some("nope".into()),
        };

        assert_eq!(
            working_directory(&config, Some(&dir)).unwrap(),
            dir.join("nope")
        );
    }
}
