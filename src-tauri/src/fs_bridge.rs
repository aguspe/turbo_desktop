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
    let scope = FsScope {
        roots: configured_roots(app),
        grants: app.state::<security::UserGrants>(),
    };

    match message.event.as_str() {
        "read" => handle_read(message, &scope).await,
        "write" => handle_write(message, &scope).await,
        "exists" => handle_exists(message, &scope).await,
        "list" => handle_list(message, &scope).await,
        "mkdir" => handle_mkdir(message, &scope).await,
        "remove" => handle_remove(message, &scope).await,
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

/// Where this app may reach: the configured roots, plus whatever the user has
/// granted through a native file dialog this session.
struct FsScope<'a> {
    roots: Vec<PathBuf>,
    grants: tauri::State<'a, security::UserGrants>,
}

/// Roots this app may touch, defaulting to its own data directory.
fn configured_roots(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let config = app.state::<TurboDesktopConfig>();
    let app_data_dir = app.path().app_data_dir().ok();
    security::allowed_roots(app_data_dir, &config.filesystem)
}

/// Pull the `path` field out of a message and resolve it inside the scope.
fn scoped_path(
    message: &BridgeMessage,
    scope: &FsScope<'_>,
    event: &str,
) -> Result<PathBuf, String> {
    let raw = message.data["path"]
        .as_str()
        .ok_or_else(|| format!("Missing 'path' in filesystem {}", event))?;

    security::resolve_with_grants(raw, &scope.roots, &scope.grants).inspect_err(|e| {
        log::warn!("Filesystem: {}", e);
    })
}

async fn handle_read(
    message: &BridgeMessage,
    scope: &FsScope<'_>,
) -> Result<serde_json::Value, String> {
    let path = scoped_path(message, scope, "read")?;
    let encoding = message.data["encoding"].as_str().unwrap_or("utf8");

    match fs::read(&path).await {
        Ok(bytes) => Ok(read_response(bytes, encoding)),
        Err(e) => Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}

/// What a read answers with: the text of the file, or for `base64` its bytes,
/// which is how a page reads a file that is not text.
fn read_response(bytes: Vec<u8>, encoding: &str) -> serde_json::Value {
    use base64::Engine;

    match encoding {
        "base64" => serde_json::json!({
            "status": "ok",
            "encoding": "base64",
            "content": base64::engine::general_purpose::STANDARD.encode(bytes),
        }),
        "utf8" | "utf-8" => match String::from_utf8(bytes) {
            Ok(content) => serde_json::json!({ "status": "ok", "encoding": "utf8", "content": content }),
            Err(_) => serde_json::json!({
                "status": "error",
                "error": "The file is not text. Read it with the encoding \"base64\".",
            }),
        },
        other => serde_json::json!({
            "status": "error",
            "error": format!("Unknown encoding '{}': use \"utf8\" or \"base64\"", other),
        }),
    }
}

async fn handle_write(
    message: &BridgeMessage,
    scope: &FsScope<'_>,
) -> Result<serde_json::Value, String> {
    let content = message.data["content"]
        .as_str()
        .ok_or("Missing 'content' in filesystem write")?;
    let append = message.data["append"].as_bool().unwrap_or(false);
    let path = scoped_path(message, scope, "write")?;

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
    scope: &FsScope<'_>,
) -> Result<serde_json::Value, String> {
    let path = scoped_path(message, scope, "exists")?;

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
    scope: &FsScope<'_>,
) -> Result<serde_json::Value, String> {
    let path = scoped_path(message, scope, "list")?;

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
    scope: &FsScope<'_>,
) -> Result<serde_json::Value, String> {
    let path = scoped_path(message, scope, "mkdir")?;

    match fs::create_dir_all(&path).await {
        Ok(()) => Ok(serde_json::json!({ "status": "ok" })),
        Err(e) => Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    }
}

async fn handle_remove(
    message: &BridgeMessage,
    scope: &FsScope<'_>,
) -> Result<serde_json::Value, String> {
    let recursive = message.data["recursive"].as_bool().unwrap_or(false);
    let path = scoped_path(message, scope, "remove")?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_read_as_text() {
        let response = read_response("título\n".as_bytes().to_vec(), "utf8");
        assert_eq!(response["status"], "ok");
        assert_eq!(response["content"], "título\n");
    }

    // A PNG, a PDF, a spreadsheet: asked for as base64, the bytes arrive whole.
    #[test]
    fn a_file_that_is_not_text_is_read_as_base64() {
        let png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0xff];

        let response = read_response(png.clone(), "base64");
        assert_eq!(response["status"], "ok");
        assert_eq!(response["encoding"], "base64");
        assert_eq!(response["content"], "iVBORw0KGgoA/w==");

        let as_text = read_response(png, "utf8");
        assert_eq!(as_text["status"], "error");
        assert!(as_text["error"].as_str().unwrap().contains("base64"));
    }

    #[test]
    fn an_encoding_nobody_knows_is_refused() {
        let response = read_response(b"x".to_vec(), "latin1");
        assert_eq!(response["status"], "error");
    }
}
