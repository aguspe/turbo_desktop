use serde::{Deserialize, Serialize};
use tauri::Emitter;

/// Bridge message sent from JavaScript to the native shell.
///
/// This is the desktop equivalent of Strada (Hotwire Native's bridge).
/// Web components can send structured messages to trigger native features
/// like notifications, file dialogs, menu items, and keyboard shortcuts.
///
/// The flow:
/// 1. A Stimulus controller on the web page extends BridgeComponent
/// 2. It calls `this.send("connect", { title: "Export" })`
/// 3. turbo-desktop.js forwards this to Rust via Tauri's invoke
/// 4. Rust handles it (e.g., adds a native menu item)
/// 5. When the native side triggers (e.g., menu clicked), it sends a message back
/// 6. The web component receives it via `onReceive(message)`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeMessage {
    /// Component identifier (e.g., "menu-item", "notification", "file-picker")
    pub component: String,
    /// Event name (e.g., "connect", "disconnect", "submit")
    pub event: String,
    /// Arbitrary JSON data payload
    pub data: serde_json::Value,
    /// Optional: which window sent this message
    #[serde(default)]
    pub window_label: Option<String>,
}

/// Response sent back to the web component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub component: String,
    pub event: String,
    pub data: serde_json::Value,
}

use crate::security::ensure_trusted_caller;

/// Handle an incoming bridge message from a web component.
#[tauri::command]
pub async fn handle_bridge_message(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    message: BridgeMessage,
) -> Result<serde_json::Value, String> {
    ensure_trusted_caller(&app, &webview)?;

    log::info!(
        "Bridge message: component={}, event={}",
        message.component,
        message.event
    );

    match message.component.as_str() {
        "notification" => handle_notification(&app, &message).await,
        "menu-item" => handle_menu_item(&app, &message).await,
        "file-picker" => handle_file_picker(&app, &message).await,
        "badge" => handle_badge(&app, &message).await,
        "shortcut" => handle_shortcut(&app, &message).await,
        "shell" => crate::shell_bridge::handle_shell(&app, &message).await,
        "filesystem" => crate::fs_bridge::handle_filesystem(&app, &message).await,
        "sudo" => crate::sudo_bridge::handle_sudo(&app, &message).await,
        "updater" => crate::updater_bridge::handle_updater(&app, &message).await,
        _ => {
            // Forward unknown components as events — allows user-defined bridge components
            app.emit("bridge-message", &message)
                .map_err(|e| format!("{}", e))?;
            Ok(serde_json::json!({ "status": "forwarded" }))
        }
    }
}

/// Send a bridge message from native back to the web component.
#[tauri::command]
pub async fn send_bridge_response(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    response: BridgeResponse,
) -> Result<(), String> {
    ensure_trusted_caller(&app, &webview)?;

    // Emit to all windows — the JS side filters by component
    app.emit("bridge-response", &response)
        .map_err(|e| format!("{}", e))?;
    Ok(())
}

// ─── Built-in Bridge Component Handlers ─────────────────────────────────────

async fn handle_notification(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    let title = message.data["title"].as_str().unwrap_or("Notification");
    let body = message.data["body"].as_str().unwrap_or("");

    // Use tauri-plugin-notification
    app.emit(
        "show-notification",
        serde_json::json!({ "title": title, "body": body }),
    )
    .map_err(|e| format!("{}", e))?;

    Ok(serde_json::json!({ "status": "shown" }))
}

async fn handle_menu_item(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    match message.event.as_str() {
        "connect" => {
            let title = message.data["title"].as_str().unwrap_or("Menu Item");

            log::info!("Bridge: registering menu item '{}'", title);

            // Emit event so the menu system can pick it up
            app.emit("bridge-menu-item-register", &message.data)
                .map_err(|e| format!("{}", e))?;

            Ok(serde_json::json!({ "status": "registered" }))
        }
        "disconnect" => {
            app.emit("bridge-menu-item-unregister", &message.data)
                .map_err(|e| format!("{}", e))?;
            Ok(serde_json::json!({ "status": "unregistered" }))
        }
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

async fn handle_file_picker(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;

    let title = message.data["title"].as_str().unwrap_or("Select");

    match message.event.as_str() {
        "open-folder" | "open_folder" => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            app.dialog()
                .file()
                .set_title(title)
                .pick_folder(move |folder| {
                    let path = folder.map(|f| f.to_string());
                    let _ = tx.send(path);
                });

            match rx.await {
                Ok(Some(path)) => {
                    // Picking a folder is consent for everything in it.
                    app.state::<crate::security::UserGrants>()
                        .grant_folder(&path);
                    Ok(serde_json::json!({ "status": "selected", "path": path }))
                }
                Ok(None) => Ok(serde_json::json!({ "status": "cancelled", "path": null })),
                Err(e) => Err(format!("Dialog error: {}", e)),
            }
        }
        "open" | "open-file" | "open_file" => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            app.dialog()
                .file()
                .set_title(title)
                .pick_file(move |file| {
                    let path = file.map(|f| f.to_string());
                    let _ = tx.send(path);
                });

            match rx.await {
                Ok(Some(path)) => {
                    // Picking a file is consent for that file.
                    app.state::<crate::security::UserGrants>().grant_file(&path);
                    Ok(serde_json::json!({ "status": "selected", "path": path }))
                }
                Ok(None) => Ok(serde_json::json!({ "status": "cancelled", "path": null })),
                Err(e) => Err(format!("Dialog error: {}", e)),
            }
        }
        "save" => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            app.dialog()
                .file()
                .set_title(title)
                .save_file(move |file| {
                    let path = file.map(|f| f.to_string());
                    let _ = tx.send(path);
                });

            match rx.await {
                Ok(Some(path)) => {
                    // Choosing where to save is consent to write there.
                    app.state::<crate::security::UserGrants>().grant_file(&path);
                    Ok(serde_json::json!({ "status": "selected", "path": path }))
                }
                Ok(None) => Ok(serde_json::json!({ "status": "cancelled", "path": null })),
                Err(e) => Err(format!("Dialog error: {}", e)),
            }
        }
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

async fn handle_badge(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    let count = message.data["count"].as_u64().unwrap_or(0);
    log::info!("Bridge: setting dock badge to {}", count);

    #[cfg(target_os = "macos")]
    {
        // macOS dock badge via Cocoa API
        app.emit("bridge-badge-update", count)
            .map_err(|e| format!("{}", e))?;
    }

    Ok(serde_json::json!({ "status": "updated", "count": count }))
}

async fn handle_shortcut(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    match message.event.as_str() {
        "register" => {
            let accelerator = message.data["accelerator"]
                .as_str()
                .unwrap_or("");
            let id = message.data["id"].as_str().unwrap_or("");

            log::info!("Bridge: registering shortcut '{}' -> '{}'", accelerator, id);

            app.emit("bridge-shortcut-register", &message.data)
                .map_err(|e| format!("{}", e))?;

            Ok(serde_json::json!({ "status": "registered" }))
        }
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

/// Forward a native drag-drop interaction to the web layer.
///
/// The webview's native handler owns file drags, so the page never sees the
/// dropped files through HTML5 events — this hands it the paths instead.
/// Dropping a file onto the app is the same consent as picking it in a
/// dialog, so the paths are granted for the session before the event goes
/// out. `Over` is not forwarded: it fires for every mouse move.
pub fn handle_drag_drop(app: &tauri::AppHandle, event: &tauri::DragDropEvent) {
    use tauri::DragDropEvent;
    use tauri::Manager;

    match event {
        DragDropEvent::Enter { paths, position } => {
            emit_drag_drop(app, "enter", drag_drop_payload(paths, Some(position)));
        }
        DragDropEvent::Drop { paths, position } => {
            let grants = app.state::<crate::security::UserGrants>();
            for path in paths {
                let raw = path.to_string_lossy();
                if path.is_dir() {
                    grants.grant_folder(&raw);
                } else {
                    grants.grant_file(&raw);
                }
            }
            emit_drag_drop(app, "drop", drag_drop_payload(paths, Some(position)));
        }
        DragDropEvent::Leave => {
            emit_drag_drop(app, "leave", serde_json::json!({ "paths": [] }));
        }
        _ => {}
    }
}

fn emit_drag_drop(app: &tauri::AppHandle, event: &str, data: serde_json::Value) {
    let response = BridgeResponse {
        component: "drag-drop".to_string(),
        event: event.to_string(),
        data,
    };
    if let Err(e) = app.emit("bridge-response", &response) {
        log::warn!("Drag-drop: failed to emit '{}': {}", event, e);
    }
}

fn drag_drop_payload(
    paths: &[std::path::PathBuf],
    position: Option<&tauri::PhysicalPosition<f64>>,
) -> serde_json::Value {
    serde_json::json!({
        "paths": paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "position": position.map(|p| serde_json::json!({ "x": p.x, "y": p.y })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drop_payload_carries_paths_and_position() {
        let paths = vec![std::path::PathBuf::from("/tmp/report.csv")];
        let position = tauri::PhysicalPosition { x: 10.0, y: 20.0 };

        let payload = drag_drop_payload(&paths, Some(&position));

        assert_eq!(payload["paths"][0], "/tmp/report.csv");
        assert_eq!(payload["position"]["x"], 10.0);
        assert_eq!(payload["position"]["y"], 20.0);
    }
}
