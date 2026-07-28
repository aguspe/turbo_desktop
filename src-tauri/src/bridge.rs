use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

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

/// Reject calls from any page that is not the configured app origin.
///
/// Tauri's ACL only covers plugin commands, so app-defined commands like this one
/// are reachable from whatever the webview happens to have loaded. Since the
/// bridge fans out to shell, filesystem and sudo, the origin check is the gate.
fn ensure_trusted_caller(
    app: &tauri::AppHandle,
    webview: &tauri::Webview,
) -> Result<(), String> {
    let config = app.state::<crate::window::TurboDesktopConfig>();
    let url = webview
        .url()
        .map_err(|e| format!("Could not determine the calling page: {}", e))?;

    if crate::security::is_trusted_origin(&config.server_url, &url) {
        return Ok(());
    }

    log::warn!(
        "Bridge: refused a message from untrusted origin '{}' (expected '{}')",
        url.origin().ascii_serialization(),
        config.server_url
    );
    Err("Refused: the bridge is only available to the configured app origin".to_string())
}

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
            let _shortcut = message.data["shortcut"].as_str();

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
                Ok(Some(path)) => Ok(serde_json::json!({ "status": "selected", "path": path })),
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
                Ok(Some(path)) => Ok(serde_json::json!({ "status": "selected", "path": path })),
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
                Ok(Some(path)) => Ok(serde_json::json!({ "status": "selected", "path": path })),
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
