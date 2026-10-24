//! Starting the app server, so the desktop app is something you can just open.
//!
//! Without this, every launch depends on someone having run `rails server` in
//! another terminal first, which is a strange thing to ask of a desktop app.
//!
//! Everything needed was already here: the reachability probe decides whether a
//! server is wanted, ProcessManager owns the child and reaps it on quit, and the
//! connection monitor moves the window off the waiting page once the server
//! answers. This only decides whether to start one and does so.

use crate::connection::Liveness;
use crate::window::ServerConfig;
use std::path::{Path, PathBuf};


/// Where the app server is listening, once it has said so.
///
/// A bundled app does not know its own port ahead of time: the server binds
/// 127.0.0.1:0 and the OS picks, which removes the race you get from probing
/// for a free port and then binding it. So the shell learns the address from
/// the server rather than deciding it.
#[derive(Default)]
pub struct ServerAddress(std::sync::Mutex<Option<String>>);

impl ServerAddress {
    pub fn set(&self, url: String) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = Some(url);
        }
    }

    pub fn get(&self) -> Option<String> {
        self.0.lock().ok().and_then(|g| g.clone())
    }

    /// What the app's own server announced, if it has said anything yet.
    ///
    /// Read wherever the configured `server_url` is used as the app's origin: a
    /// bundled config can only carry a placeholder port, so the real origin is
    /// whatever turned up at runtime.
    pub fn announced<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<String> {
        use tauri::Manager;
        app.try_state::<ServerAddress>().and_then(|a| a.get())
    }
}

/// The one line of handshake a bundled server writes before anything else.
///
/// Parsed rather than matched loosely: a line that is not a handshake is
/// ordinary output, which a developer running `bin/rails server` by hand
/// produces plenty of.
pub fn handshake_url(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let url = value.get("url")?.as_str()?;
    url.starts_with("http").then(|| url.to_string())
}

/// Whether to start a server, and why not when we won't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Start,
    /// This app's server is already answering — very likely one the developer is
    /// running themselves, which we must not duplicate or later kill.
    AlreadyRunning,
    /// Something holds the port but does not answer as this app. Starting ours
    /// could only fail to bind, so this is reported rather than retried.
    PortHeldByAnotherApp,
    /// No command configured, so the app expects a server it does not own.
    NotConfigured,
}

pub fn decide(config: &ServerConfig, liveness: Liveness) -> Decision {
    if config.command.as_deref().unwrap_or("").trim().is_empty() {
        return Decision::NotConfigured;
    }
    match liveness {
        Liveness::Absent => Decision::Start,
        Liveness::Ours => Decision::AlreadyRunning,
        Liveness::Foreign => Decision::PortHeldByAnotherApp,
    }
}

/// Whether the window may open straight on the app, or must wait on the error
/// page for the server to come up.
///
/// A port held by another app is the case that was slipping through: it was
/// reported in the log and then opened in the window anyway, because anything
/// accepting a connection counted as reachable.
pub fn opens_app_at_startup(decision: &Decision, liveness: Liveness) -> bool {
    match decision {
        Decision::AlreadyRunning => true,
        // Not ours to judge: a hosted app may not mount the engine at all.
        Decision::NotConfigured => liveness != Liveness::Absent,
        // Just asked to start, or cannot start: not there yet either way.
        Decision::Start | Decision::PortHeldByAnotherApp => false,
    }
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
    control: Option<&crate::control::ControlChannel>,
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
        .stdin(Stdio::piped()) // the handshake goes in here, and EOF reaps the child
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A backstop for the orphan rule: if the task that owns the child ever
        // goes away without stopping it, kill it rather than leave a server
        // running with nothing attached to it.
        .kill_on_drop(true);

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

    // One line of handshake, then the pipe stays open. The token travels here
    // rather than in the environment or the command line so another process on
    // the machine cannot read it out of `ps`.
    //
    // Holding the pipe open is also what reaps the server: a backend that reads
    // stdin exits on EOF, which is the only layer that survives the shell being
    // force-quit, since no Rust code runs then.
    let mut stdin = child.stdin.take();
    if let (Some(pipe), Some(control)) = (stdin.as_mut(), control) {
        use tokio::io::AsyncWriteExt;
        let line = serde_json::json!({
            "protocol": "1.0",
            "control": control.url,
            "token": control.token,
            "header": crate::control::TOKEN_HEADER,
        });
        if let Err(e) = pipe.write_all(format!("{}\n", line).as_bytes()).await {
            log::warn!("Could not hand the app server its handshake: {}", e);
        } else {
            let _ = pipe.flush().await;
            log::info!(
                "Handed the app server the control channel at {}",
                control.url
            );
        }
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Registered so quitting the app stops the server it started.
    let (kill_tx, kill_rx) =
        tokio::sync::oneshot::channel::<crate::process_manager::StopRequest>();
    app.state::<crate::process_manager::ProcessManager>()
        .register(
            SERVER_PROCESS_ID.to_string(),
            command.to_string(),
            Vec::new(),
            kill_tx,
        )
        .await?;

    // Two tasks, not one. The child and its stdin are kept away from anything
    // that parses output: closing that pipe is how the server is told to exit,
    // so it must only ever happen deliberately. When both lived in the same
    // task, a panic while handling a line of output unwound the task, dropped
    // the pipe, and the server quit — moments after announcing its address.
    tauri::async_runtime::spawn(supervise(child, stdin, kill_rx));

    let app_for_output = app.clone();
    tauri::async_runtime::spawn(async move {
        use tauri::{Emitter, Manager};

        // The server's own output is the only clue when it fails to boot, so it
        // goes to the log rather than into a pipe nobody reads.
        let mut out = stdout.map(|s| BufReader::new(s).lines());
        let mut err = stderr.map(|s| BufReader::new(s).lines());

        while out.is_some() || err.is_some() {
            tokio::select! {
                line = async { match out.as_mut() { Some(l) => l.next_line().await, None => std::future::pending().await } } => {
                    match line {
                        Ok(Some(l)) => {
                            // The first line a bundled server writes says where
                            // it is listening. Anything else is ordinary output.
                            if let Some(url) = handshake_url(&l) {
                                log::info!("The app server is listening at {}", url);
                                // Announced first: moving the window to the app
                                // is the point, and it must not be lost to a
                                // failure in the bookkeeping below it.
                                let _ = app_for_output.emit("turbo-desktop://server-ready", url.clone());
                                match app_for_output.try_state::<ServerAddress>() {
                                    Some(address) => address.set(url),
                                    // `state()` would panic here, and a panic in
                                    // this task used to cost us the server.
                                    None => log::warn!(
                                        "Nowhere to record the app server's address: ServerAddress was never managed"
                                    ),
                                }
                            } else {
                                log::info!("[server] {}", l);
                            }
                        }
                        _ => out = None,
                    }
                }
                line = async { match err.as_mut() { Some(l) => l.next_line().await, None => std::future::pending().await } } => {
                    match line { Ok(Some(l)) => log::warn!("[server] {}", l), _ => err = None }
                }
            }
        }
    });

    Ok(())
}

/// Own the app server until it is asked to stop, holding its stdin open.
///
/// The open pipe is the orphan rule: a backend that reads stdin exits when it
/// reaches EOF, which is the only layer that survives the shell being
/// force-quit, since no Rust code runs then. So this task holds the write end
/// and does nothing else that could fail.
async fn supervise(
    mut child: tokio::process::Child,
    stdin: Option<tokio::process::ChildStdin>,
    kill_rx: tokio::sync::oneshot::Receiver<crate::process_manager::StopRequest>,
) {
    let _stdin = stdin;
    let mut kill_rx = kill_rx;

    tokio::select! {
        request = &mut kill_rx => {
            log::info!("Stopping the app server");
            crate::process_manager::stop_tree(&mut child).await;
            if let Ok(request) = request {
                request.stopped();
            }
        }
        status = child.wait() => {
            log::warn!("The app server exited: {:?}", status.ok().and_then(|s| s.code()));
        }
    }
}

/// ProcessManager id for the server, so it is distinguishable from anything the
/// web layer spawns through the shell bridge.
pub const SERVER_PROCESS_ID: &str = "turbo-desktop:app-server";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handshake_gives_up_the_address() {
        let line = r#"{"protocol":"1.0","url":"http://127.0.0.1:53411","pid":9}"#;
        assert_eq!(
            handshake_url(line),
            Some("http://127.0.0.1:53411".to_string())
        );
    }

    #[test]
    fn ordinary_server_output_is_not_a_handshake() {
        // A developer running `bin/rails server` by hand produces plenty of
        // this, and none of it should be mistaken for an address.
        for line in [
            "Puma starting in single mode...",
            "* Listening on http://127.0.0.1:3000",
            "",
            "{}",
            r#"{"url":"not-a-url"}"#,
            r#"{"url":42}"#,
            "{ broken json",
        ] {
            assert_eq!(handshake_url(line), None, "{line:?} should not parse as a handshake");
        }
    }

    fn configured() -> ServerConfig {
        ServerConfig {
            command: Some("bin/rails server".into()),
            directory: None,
        }
    }

    #[test]
    fn nothing_to_start_without_a_command() {
        assert_eq!(
            decide(&ServerConfig::default(), Liveness::Absent),
            Decision::NotConfigured
        );
        assert_eq!(
            decide(
                &ServerConfig {
                    command: Some("   ".into()),
                    directory: None
                },
                Liveness::Absent
            ),
            Decision::NotConfigured
        );
    }

    #[test]
    fn starts_when_nothing_is_listening() {
        assert_eq!(decide(&configured(), Liveness::Absent), Decision::Start);
    }

    #[test]
    fn leaves_our_own_server_alone() {
        // Starting a second one would fail on the port, and quitting the app
        // would kill a server the developer started by hand.
        assert_eq!(
            decide(&configured(), Liveness::Ours),
            Decision::AlreadyRunning
        );
    }

    #[test]
    fn reports_a_port_held_by_a_different_app() {
        // The failure this prevents: a second Rails app on 3000 was silently
        // adopted, so the desktop app opened someone else's project.
        assert_eq!(
            decide(&configured(), Liveness::Foreign),
            Decision::PortHeldByAnotherApp
        );
    }

    #[test]
    fn the_rails_app_is_a_level_above_the_config_by_default() {
        let dir = crate::test_temp_dir().join("turbo-desktop-server-default");
        let desktop = dir.join("desktop");
        std::fs::create_dir_all(&desktop).unwrap();

        let resolved = working_directory(&configured(), Some(&desktop)).unwrap();

        assert_eq!(resolved, dir.canonicalize().unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_explicit_directory_wins() {
        let dir = crate::test_temp_dir().join("turbo-desktop-server-explicit");
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

    /// A stand-in for the bundled server: it exits when stdin reaches EOF, the
    /// way `packaging/templates/boot.rb` does, and leaves a file behind when it
    /// goes so the test can tell without racing on a pid.
    #[cfg(unix)]
    fn a_server_that_exits_on_eof(
        marker: &Path,
    ) -> (tokio::process::Child, tokio::process::ChildStdin) {
        let mut child = tokio::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("cat > /dev/null; : > '{}'", marker.display()))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("could not start the stand-in server");
        let stdin = child.stdin.take().expect("no stdin on the stand-in server");
        (child, stdin)
    }

    /// The regression. The task that parsed the server's output used to hold
    /// the child's stdin as well, so a panic while handling a line closed the
    /// pipe, and the server exited on EOF seconds after announcing its address.
    /// Supervision owns the pipe now, and nothing that parses can reach it.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_server_outlives_a_panic_in_the_code_that_reads_its_output() {
        let marker = std::env::temp_dir().join("turbo-desktop-supervise-panic");
        std::fs::remove_file(&marker).ok();
        let (child, stdin) = a_server_that_exits_on_eof(&marker);

        let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<crate::process_manager::StopRequest>();
        let supervisor = tokio::spawn(supervise(child, Some(stdin), kill_rx));

        // What `state()` on unmanaged state did, in the task next door.
        let reader = tokio::spawn(async { panic!("something in the reader") });
        assert!(reader.await.is_err(), "the reader was supposed to panic");

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert!(
            !marker.exists(),
            "the app server saw EOF and exited when the output reader panicked"
        );

        // And it still stops when it is told to.
        kill_tx
            .send(crate::process_manager::StopRequest::detached())
            .ok();
        supervisor.await.ok();
        std::fs::remove_file(&marker).ok();
    }

    /// The other half of the contract: quitting the app stops the server it
    /// started, rather than leaving it behind on a port nobody remembers.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_kill_signal_stops_the_server() {
        let marker = std::env::temp_dir().join("turbo-desktop-supervise-kill");
        std::fs::remove_file(&marker).ok();
        let (child, stdin) = a_server_that_exits_on_eof(&marker);

        let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<crate::process_manager::StopRequest>();
        let supervisor = tokio::spawn(supervise(child, Some(stdin), kill_rx));

        assert!(
            kill_tx
                .send(crate::process_manager::StopRequest::detached())
                .is_ok(),
            "nothing was listening for the kill"
        );
        let stopped = tokio::time::timeout(std::time::Duration::from_secs(5), supervisor).await;
        assert!(stopped.is_ok(), "supervision did not return after the kill");
        std::fs::remove_file(&marker).ok();
    }

    #[test]
    fn a_missing_directory_is_left_for_the_spawn_to_report() {
        let dir = crate::test_temp_dir().join("turbo-desktop-server-missing");
        let config = ServerConfig {
            command: Some("bin/rails server".into()),
            directory: Some("nope".into()),
        };

        assert_eq!(
            working_directory(&config, Some(&dir)).unwrap(),
            dir.join("nope")
        );
    }

    #[test]
    fn the_window_does_not_open_on_a_port_held_by_another_app() {
        // The bug this prevents: the stranger was reported in the log and then
        // opened in the window anyway.
        assert!(!opens_app_at_startup(
            &Decision::PortHeldByAnotherApp,
            Liveness::Foreign
        ));
    }

    #[test]
    fn the_window_opens_on_our_own_running_server() {
        assert!(opens_app_at_startup(
            &Decision::AlreadyRunning,
            Liveness::Ours
        ));
    }

    #[test]
    fn a_hosted_app_opens_on_whatever_answers() {
        // Without a server command the shell cannot tell whose server it is,
        // and a hosted app need not mount the engine at all.
        assert!(opens_app_at_startup(
            &Decision::NotConfigured,
            Liveness::Foreign
        ));
        assert!(!opens_app_at_startup(
            &Decision::NotConfigured,
            Liveness::Absent
        ));
    }

    #[test]
    fn a_server_just_asked_to_start_is_not_there_yet() {
        assert!(!opens_app_at_startup(&Decision::Start, Liveness::Absent));
    }
}
