use crate::bridge::BridgeMessage;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Handle bridge messages for the "filesystem" component.
///
/// Provides read, write, exists, list, mkdir, and remove operations
/// with support for tilde (~/) expansion to the user's home directory.
pub async fn handle_filesystem(
    _app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    match message.event.as_str() {
        "read" => handle_read(message).await,
        "write" => handle_write(message).await,
        "exists" => handle_exists(message).await,
        "list" => handle_list(message).await,
        "mkdir" => handle_mkdir(message).await,
        "remove" => handle_remove(message).await,
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

/// Expand ~/... to $HOME/...
fn expand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}{}", home, &path[1..]);
        }
    } else if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

async fn handle_read(message: &BridgeMessage) -> Result<serde_json::Value, String> {
    let path = message.data["path"]
        .as_str()
        .ok_or("Missing 'path' in filesystem read")?;
    let path = expand_path(path);

    match fs::read_to_string(&path).await {
        Ok(content) => Ok(serde_json::json!({ "status": "ok", "content": content })),
        Err(e) => Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}

async fn handle_write(message: &BridgeMessage) -> Result<serde_json::Value, String> {
    let path = message.data["path"]
        .as_str()
        .ok_or("Missing 'path' in filesystem write")?;
    let content = message.data["content"]
        .as_str()
        .ok_or("Missing 'content' in filesystem write")?;
    let append = message.data["append"].as_bool().unwrap_or(false);
    let path = expand_path(path);

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

async fn handle_exists(message: &BridgeMessage) -> Result<serde_json::Value, String> {
    let path = message.data["path"]
        .as_str()
        .ok_or("Missing 'path' in filesystem exists")?;
    let path = expand_path(path);

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

async fn handle_list(message: &BridgeMessage) -> Result<serde_json::Value, String> {
    let path = message.data["path"]
        .as_str()
        .ok_or("Missing 'path' in filesystem list")?;
    let path = expand_path(path);

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

async fn handle_mkdir(message: &BridgeMessage) -> Result<serde_json::Value, String> {
    let path = message.data["path"]
        .as_str()
        .ok_or("Missing 'path' in filesystem mkdir")?;
    let path = expand_path(path);

    match fs::create_dir_all(&path).await {
        Ok(()) => Ok(serde_json::json!({ "status": "ok" })),
        Err(e) => Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}

async fn handle_remove(message: &BridgeMessage) -> Result<serde_json::Value, String> {
    let path = message.data["path"]
        .as_str()
        .ok_or("Missing 'path' in filesystem remove")?;
    let recursive = message.data["recursive"].as_bool().unwrap_or(false);
    let path = expand_path(path);

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
