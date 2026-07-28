use crate::bridge::BridgeMessage;
use crate::security;
use crate::window::TurboDesktopConfig;
use std::path::PathBuf;
use tauri::Manager;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Handle bridge messages for the "filesystem" component.
///
/// Provides read, write, exists, list, mkdir, and remove operations.
/// Every path is resolved against the roots declared in
/// `turbo-desktop.config.json`; anything outside them is refused.
pub async fn handle_filesystem(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    let roots = configured_roots(app);

    match message.event.as_str() {
        "read" => handle_read(message, &roots).await,
        "write" => handle_write(message, &roots).await,
        "exists" => handle_exists(message, &roots).await,
        "list" => handle_list(message, &roots).await,
        "mkdir" => handle_mkdir(message, &roots).await,
        "remove" => handle_remove(message, &roots).await,
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

/// Roots this app may touch, defaulting to its own data directory.
fn configured_roots(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let config = app.state::<TurboDesktopConfig>();
    let app_data_dir = app.path().app_data_dir().ok();
    security::allowed_roots(app_data_dir, &config.filesystem)
}

/// Pull the `path` field out of a message and resolve it inside the allowed roots.
fn scoped_path(
    message: &BridgeMessage,
    roots: &[PathBuf],
    event: &str,
) -> Result<PathBuf, String> {
    let raw = message.data["path"]
        .as_str()
        .ok_or_else(|| format!("Missing 'path' in filesystem {}", event))?;

    security::resolve_in_scope(raw, roots).inspect_err(|e| {
        log::warn!("Filesystem: {}", e);
    })
}

async fn handle_read(
    message: &BridgeMessage,
    roots: &[PathBuf],
) -> Result<serde_json::Value, String> {
    let path = scoped_path(message, roots, "read")?;

    match fs::read_to_string(&path).await {
        Ok(content) => Ok(serde_json::json!({ "status": "ok", "content": content })),
        Err(e) => Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}

async fn handle_write(
    message: &BridgeMessage,
    roots: &[PathBuf],
) -> Result<serde_json::Value, String> {
    let content = message.data["content"]
        .as_str()
        .ok_or("Missing 'content' in filesystem write")?;
    let append = message.data["append"].as_bool().unwrap_or(false);
    let path = scoped_path(message, roots, "write")?;

    let result = if append {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .await
            .map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes()).await.map_err(|e| e.to_string())
    } else {
        fs::write(&path, content).await.map_err(|e| e.to_string())
    };

    match result {
        Ok(()) => Ok(serde_json::json!({ "status": "ok" })),
        Err(e) => Ok(serde_json::json!({ "status": "error", "error": e })),
    }
}

async fn handle_exists(
    message: &BridgeMessage,
    roots: &[PathBuf],
) -> Result<serde_json::Value, String> {
    let path = scoped_path(message, roots, "exists")?;

    match fs::metadata(&path).await {
        Ok(meta) => Ok(serde_json::json!({
            "status": "ok",
            "exists": true,
            "is_dir": meta.is_dir(),
            "is_file": meta.is_file(),
        })),
        Err(_) => Ok(serde_json::json!({
            "status": "ok",
            "exists": false,
            "is_dir": false,
            "is_file": false,
        })),
    }
}

async fn handle_list(
    message: &BridgeMessage,
    roots: &[PathBuf],
) -> Result<serde_json::Value, String> {
    let path = scoped_path(message, roots, "list")?;

    let mut entries = Vec::new();
    let mut dir = match fs::read_dir(&path).await {
        Ok(dir) => dir,
        Err(e) => return Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        let meta = entry.metadata().await;
        let (is_dir, is_file) = match meta {
            Ok(m) => (m.is_dir(), m.is_file()),
            Err(_) => (false, false),
        };
        entries.push(serde_json::json!({
            "name": name,
            "is_dir": is_dir,
            "is_file": is_file,
        }));
    }

    Ok(serde_json::json!({ "status": "ok", "entries": entries }))
}

async fn handle_mkdir(
    message: &BridgeMessage,
    roots: &[PathBuf],
) -> Result<serde_json::Value, String> {
    let path = scoped_path(message, roots, "mkdir")?;

    match fs::create_dir_all(&path).await {
        Ok(()) => Ok(serde_json::json!({ "status": "ok" })),
        Err(e) => Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}

async fn handle_remove(
    message: &BridgeMessage,
    roots: &[PathBuf],
) -> Result<serde_json::Value, String> {
    let recursive = message.data["recursive"].as_bool().unwrap_or(false);
    let path = scoped_path(message, roots, "remove")?;

    let result = match fs::metadata(&path).await {
        Ok(meta) if meta.is_dir() && recursive => fs::remove_dir_all(&path).await,
        Ok(meta) if meta.is_dir() => fs::remove_dir(&path).await,
        Ok(_) => fs::remove_file(&path).await,
        Err(e) => return Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    };

    match result {
        Ok(()) => Ok(serde_json::json!({ "status": "ok" })),
        Err(e) => Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}
