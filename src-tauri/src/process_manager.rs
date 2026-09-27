use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// Status of a tracked process.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ProcessStatus {
    Running,
    Exited { code: Option<i32> },
    Killed,
    Failed { error: String },
}

/// Public metadata about a tracked process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub status: ProcessStatus,
    pub started_at: u64,
}

/// What a process's task is sent to make it stop.
///
/// When the app is quitting it carries a way to say the process has stopped,
/// because the app must not leave before it has.
pub struct StopRequest {
    done: Option<tokio::sync::oneshot::Sender<()>>,
}

impl StopRequest {
    /// Say the process has been stopped. Call it after killing the child.
    pub fn stopped(self) {
        if let Some(done) = self.done {
            let _ = done.send(());
        }
    }
}

/// How long quitting waits for the processes to stop before leaving anyway.
pub const STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Internal entry holding metadata and the kill channel sender.
struct ProcessEntry {
    info: ProcessInfo,
    kill_sender: Option<tokio::sync::oneshot::Sender<StopRequest>>,
}

/// Ceiling on processes running at once, so a runaway caller cannot spawn
/// without bound. Exited entries stay in the map for status queries and do not
/// count against it.
const MAX_RUNNING_PROCESSES: usize = 64;

/// Shared state for tracking active child processes.
/// Registered via `app.manage()` in main.rs.
pub struct ProcessManager {
    processes: Arc<Mutex<HashMap<String, ProcessEntry>>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Number of processes currently in the Running state.
    #[cfg(test)]
    pub async fn running_count(&self) -> usize {
        self.processes
            .lock()
            .await
            .values()
            .filter(|e| matches!(e.info.status, ProcessStatus::Running))
            .count()
    }

    /// Register a new process with status Running.
    ///
    /// Fails once `MAX_RUNNING_PROCESSES` are already running.
    pub async fn register(
        &self,
        id: String,
        command: String,
        args: Vec<String>,
        kill_sender: tokio::sync::oneshot::Sender<StopRequest>,
    ) -> Result<(), String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut procs = self.processes.lock().await;

        let running = procs
            .values()
            .filter(|e| matches!(e.info.status, ProcessStatus::Running))
            .count();
        if running >= MAX_RUNNING_PROCESSES {
            return Err(format!(
                "Refused: {} processes are already running (limit {})",
                running, MAX_RUNNING_PROCESSES
            ));
        }

        let entry = ProcessEntry {
            info: ProcessInfo {
                id: id.clone(),
                command,
                args,
                status: ProcessStatus::Running,
                started_at: now,
            },
            kill_sender: Some(kill_sender),
        };

        procs.insert(id, entry);
        Ok(())
    }

    /// Send a kill signal to a running process.
    pub async fn kill(&self, id: &str) -> Result<(), String> {
        let mut procs = self.processes.lock().await;
        let entry = procs.get_mut(id).ok_or_else(|| format!("Process '{}' not found", id))?;

        if let Some(sender) = entry.kill_sender.take() {
            sender
                .send(StopRequest { done: None })
                .map_err(|_| format!("Process '{}' already exited", id))?;
            entry.info.status = ProcessStatus::Killed;
            Ok(())
        } else {
            Err(format!("Process '{}' is not running", id))
        }
    }

    /// Mark a process as exited with the given code.
    pub async fn mark_exited(&self, id: &str, code: Option<i32>) {
        if let Some(entry) = self.processes.lock().await.get_mut(id) {
            entry.info.status = ProcessStatus::Exited { code };
            entry.kill_sender = None;
        }
    }

    /// Mark a process as failed with an error message.
    pub async fn mark_failed(&self, id: &str, error: String) {
        if let Some(entry) = self.processes.lock().await.get_mut(id) {
            entry.info.status = ProcessStatus::Failed { error };
            entry.kill_sender = None;
        }
    }

    /// Get the status of a specific process.
    pub async fn status(&self, id: &str) -> Option<ProcessInfo> {
        self.processes.lock().await.get(id).map(|e| e.info.clone())
    }

    /// List all tracked processes.
    pub async fn list(&self) -> Vec<ProcessInfo> {
        self.processes
            .lock()
            .await
            .values()
            .map(|e| e.info.clone())
            .collect()
    }

    /// Stop every running process, and wait until they have stopped.
    ///
    /// Called as the app quits. Asking is not enough: the app exits as soon as
    /// this returns, and a process that has only been asked is still running.
    pub async fn kill_all(&self) {
        let mut waiting = Vec::new();

        {
            let mut procs = self.processes.lock().await;
            for entry in procs.values_mut() {
                if let Some(sender) = entry.kill_sender.take() {
                    let (done, stopped) = tokio::sync::oneshot::channel();
                    if sender.send(StopRequest { done: Some(done) }).is_ok() {
                        waiting.push(stopped);
                    }
                    entry.info.status = ProcessStatus::Killed;
                }
            }
            // Released here: a task marks its process exited as it stops.
        }

        let all_stopped = async {
            for stopped in waiting {
                let _ = stopped.await;
            }
        };
        if tokio::time::timeout(STOP_TIMEOUT, all_stopped).await.is_err() {
            log::warn!("Some processes had not stopped after {:?}", STOP_TIMEOUT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn register(pm: &ProcessManager, id: &str) -> Result<(), String> {
        let (tx, rx) = tokio::sync::oneshot::channel::<StopRequest>();
        // Keep the receiver alive so the entry looks like a live process.
        std::mem::forget(rx);
        pm.register(id.to_string(), "sleep".into(), vec!["1".into()], tx)
            .await
    }

    // Quitting used to ask each process to stop and leave at once, before any
    // of them had. The app's own server outlived the app.
    #[tokio::test]
    async fn quitting_waits_for_the_processes_to_stop() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let pm = ProcessManager::new();
        let stopped = Arc::new(AtomicBool::new(false));
        let (tx, rx) = tokio::sync::oneshot::channel::<StopRequest>();
        pm.register("server".into(), "bin/rails".into(), vec![], tx)
            .await
            .unwrap();

        let flag = stopped.clone();
        tokio::spawn(async move {
            let request = rx.await.expect("the process was never asked to stop");
            // Stopping takes a moment, as killing a real process does.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            flag.store(true, Ordering::SeqCst);
            request.stopped();
        });

        pm.kill_all().await;

        assert!(
            stopped.load(Ordering::SeqCst),
            "kill_all returned while the process was still running"
        );
    }

    #[tokio::test]
    async fn quitting_does_not_wait_forever_for_a_process_that_will_not_stop() {
        let pm = ProcessManager::new();
        let (tx, rx) = tokio::sync::oneshot::channel::<StopRequest>();
        pm.register("stuck".into(), "sleep".into(), vec![], tx)
            .await
            .unwrap();
        // Asked, and never answers.
        tokio::spawn(async move {
            let _request = rx.await;
            std::future::pending::<()>().await;
        });

        let started = std::time::Instant::now();
        pm.kill_all().await;

        assert!(started.elapsed() < STOP_TIMEOUT + std::time::Duration::from_secs(2));
    }

    #[tokio::test]
    async fn stopping_one_process_does_not_wait_for_it() {
        let pm = ProcessManager::new();
        let (tx, rx) = tokio::sync::oneshot::channel::<StopRequest>();
        pm.register("job".into(), "sleep".into(), vec![], tx)
            .await
            .unwrap();

        pm.kill("job").await.unwrap();

        let request = rx.await.expect("the process was never asked to stop");
        request.stopped(); // Nobody is waiting, and that is not an error.
    }

    #[tokio::test]
    async fn refuses_to_exceed_the_running_limit() {
        let pm = ProcessManager::new();

        for i in 0..MAX_RUNNING_PROCESSES {
            register(&pm, &format!("p{i}"))
                .await
                .expect("registration below the limit should succeed");
        }
        assert_eq!(pm.running_count().await, MAX_RUNNING_PROCESSES);

        let err = register(&pm, "one-too-many")
            .await
            .expect_err("registration past the limit should be refused");
        assert!(err.contains("already running"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn exited_processes_free_up_capacity() {
        let pm = ProcessManager::new();

        for i in 0..MAX_RUNNING_PROCESSES {
            register(&pm, &format!("p{i}")).await.unwrap();
        }
        pm.mark_exited("p0", Some(0)).await;

        assert_eq!(pm.running_count().await, MAX_RUNNING_PROCESSES - 1);
        register(&pm, "replacement")
            .await
            .expect("a finished process should free a slot");
    }
}
