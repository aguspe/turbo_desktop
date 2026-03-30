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

/// Internal entry holding metadata and the kill channel sender.
struct ProcessEntry {
    info: ProcessInfo,
    kill_sender: Option<tokio::sync::oneshot::Sender<()>>,
}

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

    /// Register a new process with status Running.
    pub async fn register(
        &self,
        id: String,
        command: String,
        args: Vec<String>,
        kill_sender: tokio::sync::oneshot::Sender<()>,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

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

        self.processes.lock().await.insert(id, entry);
    }

    /// Send a kill signal to a running process.
    pub async fn kill(&self, id: &str) -> Result<(), String> {
        let mut procs = self.processes.lock().await;
        let entry = procs.get_mut(id).ok_or_else(|| format!("Process '{}' not found", id))?;

        if let Some(sender) = entry.kill_sender.take() {
            sender.send(()).map_err(|_| format!("Process '{}' already exited", id))?;
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

    /// Remove a process entry (cleanup after exit).
    pub async fn remove(&self, id: &str) {
        self.processes.lock().await.remove(id);
    }

    /// Kill all running processes (called on app exit).
    pub async fn kill_all(&self) {
        let mut procs = self.processes.lock().await;
        for entry in procs.values_mut() {
            if let Some(sender) = entry.kill_sender.take() {
                let _ = sender.send(());
                entry.info.status = ProcessStatus::Killed;
            }
        }
    }
}
