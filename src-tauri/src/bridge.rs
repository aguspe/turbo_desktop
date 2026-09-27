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
        "clipboard" => handle_clipboard(&app, &message).await,
        "devtools" => handle_devtools(&app, &message),
        "autostart" => handle_autostart(&app, &message).await,
        // The page drains files the OS asked the app to open. Pull rather than
        // push: a launch-by-double-click happens before any page exists.
        // The page collecting the link the app was asked to open.
        "deep-link" => match message.event.as_str() {
            "pending" => {
                use tauri::Manager;
                Ok(serde_json::json!({
                    "status": "ok",
                    "url": app.state::<crate::deep_link::PendingLink>().take(),
                }))
            }
            _ => Ok(serde_json::json!({ "status": "unknown_event" })),
        },
        "file-open" => match message.event.as_str() {
            "pending" => Ok(serde_json::json!({
                "status": "ok",
                "paths": crate::deep_link::drain_pending(&app),
            })),
            _ => Ok(serde_json::json!({ "status": "unknown_event" })),
        },
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

    broadcast_response(&app, &response);
    Ok(())
}

/// Hand a response to every open page — the JS side filters by component.
///
/// Not Tauri's event API: that is not exposed to pages loaded from a remote
/// URL, which is what an app page always is, so `listen` never fires there.
/// Responses travel over the same eval channel as everything else the shell
/// tells the page.
pub fn broadcast_response(app: &tauri::AppHandle, response: &BridgeResponse) {
    match serde_json::to_value(response) {
        Ok(payload) => crate::window::deliver_to_all(app, "bridge-response", &payload),
        Err(e) => log::warn!("Bridge: could not serialize a response: {}", e),
    }
}

// ─── Built-in Bridge Component Handlers ─────────────────────────────────────

/// Open the developer tools in the main window, or close them if open.
///
/// A development build has them. An app built for release does not, and says
/// so rather than pretending.
fn handle_devtools(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    if message.event != "toggle" {
        return Ok(serde_json::json!({ "status": "unknown_event" }));
    }

    #[cfg(debug_assertions)]
    {
        use tauri::Manager;

        let window = app
            .get_webview_window("main")
            .ok_or_else(|| "There is no main window".to_string())?;

        if window.is_devtools_open() {
            window.close_devtools();
            Ok(serde_json::json!({ "status": "closed" }))
        } else {
            window.open_devtools();
            Ok(serde_json::json!({ "status": "opened" }))
        }
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = app;
        Ok(serde_json::json!({ "status": "unavailable", "error": "Developer tools are in development builds only" }))
    }
}

/// Whether a message to the notification component asks for one to be shown.
///
/// A component says goodbye when its element leaves the page, and may say
/// hello with nothing in its hands. Neither is news.
pub fn asks_for_a_notification(event: &str, data: &serde_json::Value) -> bool {
    if event == "disconnect" {
        return false;
    }

    ["title", "body"]
        .iter()
        .any(|key| data[*key].as_str().is_some_and(|text| !text.trim().is_empty()))
}

/// Show a notification through AppleScript. The title and body are handed
/// over as arguments, never written into the script, so neither can be read
/// as part of it.
#[cfg(all(target_os = "macos", debug_assertions))]
fn show_without_a_bundle(title: &str, body: &str) -> bool {
    std::process::Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv)",
            "-e",
            "end run",
            title,
            body,
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn handle_notification(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    use tauri_plugin_notification::NotificationExt;

    if !asks_for_a_notification(&message.event, &message.data) {
        return Ok(serde_json::json!({ "status": "ignored" }));
    }

    let title = message.data["title"].as_str().unwrap_or("").trim();
    let body = message.data["body"].as_str().unwrap_or("").trim();

    let mut notification = app.notification().builder();
    if !title.is_empty() {
        notification = notification.title(title);
    }
    if !body.is_empty() {
        notification = notification.body(body);
    }

    // An app run with `tauri dev` on macOS is a bare binary, with no bundle
    // for the system to file its notifications under. The plugin files them
    // under Terminal, where they are shown only if Terminal is allowed to
    // notify, and not at all from another terminal. The system's own
    // scripting shows them whoever asks.
    #[cfg(all(target_os = "macos", debug_assertions))]
    if show_without_a_bundle(title, body) {
        return Ok(serde_json::json!({ "status": "shown", "through": "osascript" }));
    }

    // A machine with nothing to show notifications with is not an error in the
    // app. The page is told, so it can say the thing some other way.
    match notification.show() {
        Ok(()) => Ok(serde_json::json!({ "status": "shown" })),
        Err(e) => {
            log::warn!("Could not show a notification: {}", e);
            Ok(serde_json::json!({ "status": "unavailable", "error": e.to_string() }))
        }
    }
}

/// The submenu items registered by pages are kept in.
pub const BRIDGE_MENU_ID: &str = "bridge-actions";
const BRIDGE_ITEM_PREFIX: &str = "bridge-item:";

/// The menu id for an item a page registers: its `id`, or failing that its
/// title. Registering the same one again replaces it, so a controller that
/// reconnects after a Turbo visit does not leave a second copy behind.
pub fn menu_item_id(data: &serde_json::Value) -> Option<String> {
    ["id", "title"]
        .iter()
        .filter_map(|key| data[*key].as_str())
        .map(str::trim)
        .find(|name| !name.is_empty())
        .map(|name| format!("{BRIDGE_ITEM_PREFIX}{name}"))
}

/// The page's name for the item a menu event came from, if a page registered it.
pub fn bridge_item_for_menu_event(event_id: &str) -> Option<&str> {
    event_id
        .strip_prefix(BRIDGE_ITEM_PREFIX)
        .filter(|name| !name.is_empty())
}

fn bridge_submenu(app: &tauri::AppHandle) -> Result<tauri::menu::Submenu<tauri::Wry>, String> {
    let menu = app
        .menu()
        .ok_or_else(|| "The app has no menu bar".to_string())?;

    if let Some(existing) = menu.get(BRIDGE_MENU_ID).and_then(|kind| kind.as_submenu().cloned()) {
        return Ok(existing);
    }

    let submenu = tauri::menu::SubmenuBuilder::with_id(app, BRIDGE_MENU_ID, "Actions")
        .build()
        .map_err(|e| format!("Could not create the Actions menu: {}", e))?;
    menu.append(&submenu)
        .map_err(|e| format!("Could not add the Actions menu: {}", e))?;

    Ok(submenu)
}

async fn handle_menu_item(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    match message.event.as_str() {
        "connect" | "register" => {
            let Some(id) = menu_item_id(&message.data) else {
                // A component connecting with nothing to register yet.
                return Ok(serde_json::json!({ "status": "ignored" }));
            };
            let title = message.data["title"]
                .as_str()
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| bridge_item_for_menu_event(&id).unwrap_or("Menu Item"));

            let submenu = bridge_submenu(app)?;
            if let Some(existing) = submenu.get(&id) {
                let _ = submenu.remove(&existing);
            }

            let mut item = tauri::menu::MenuItemBuilder::with_id(id.clone(), title);
            if let Some(shortcut) = message.data["shortcut"].as_str().filter(|s| !s.is_empty()) {
                item = item.accelerator(shortcut);
            }
            let item = item
                .build(app)
                .map_err(|e| format!("Could not create the menu item: {}", e))?;
            submenu
                .append(&item)
                .map_err(|e| format!("Could not add the menu item: {}", e))?;

            log::info!("Bridge: menu item '{}' registered", title);
            Ok(serde_json::json!({ "status": "registered", "id": bridge_item_for_menu_event(&id) }))
        }
        "unregister" | "disconnect" => {
            let Some(id) = menu_item_id(&message.data) else {
                return Ok(serde_json::json!({ "status": "ignored" }));
            };
            let submenu = bridge_submenu(app)?;
            if let Some(existing) = submenu.get(&id) {
                submenu
                    .remove(&existing)
                    .map_err(|e| format!("Could not remove the menu item: {}", e))?;
            }
            Ok(serde_json::json!({ "status": "unregistered" }))
        }
        "list" => {
            let submenu = bridge_submenu(app)?;
            let items: Vec<serde_json::Value> = submenu
                .items()
                .map_err(|e| format!("Could not read the menu: {}", e))?
                .iter()
                .filter_map(|kind| kind.as_menuitem())
                .map(|item| {
                    serde_json::json!({
                        "id": bridge_item_for_menu_event(item.id().as_ref()),
                        "title": item.text().unwrap_or_default(),
                    })
                })
                .collect();
            Ok(serde_json::json!({ "status": "ok", "items": items }))
        }
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

/// Tell the pages that a menu item one of them registered was chosen.
pub fn menu_item_chosen<R: tauri::Runtime>(app: &tauri::AppHandle<R>, name: &str) {
    crate::window::deliver_to_all(
        app,
        "bridge-response",
        &serde_json::json!({
            "component": "menu-item",
            "event": "clicked",
            "data": { "id": name },
        }),
    );
}

/// What a page may say about the dialog it asks for.
#[derive(Debug, PartialEq)]
pub struct PickerOptions {
    pub title: String,
    /// The name a save dialog opens with.
    pub default_name: Option<String>,
    /// The kinds of file offered, as a name and its extensions.
    pub filters: Vec<(String, Vec<String>)>,
}

impl From<&serde_json::Value> for PickerOptions {
    fn from(data: &serde_json::Value) -> Self {
        let title = data["title"]
            .as_str()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .unwrap_or("Select")
            .to_string();

        // The last part only. Where the file goes is for the person to say,
        // in the dialog.
        let default_name = data["defaultName"]
            .as_str()
            .and_then(|name| std::path::Path::new(name.trim()).file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty());

        let filters = data["filters"]
            .as_array()
            .map(|filters| {
                filters
                    .iter()
                    .filter_map(|filter| {
                        let extensions: Vec<String> = filter["extensions"]
                            .as_array()?
                            .iter()
                            .filter_map(|extension| extension.as_str())
                            .map(|extension| extension.trim().trim_start_matches('.').to_string())
                            .filter(|extension| !extension.is_empty())
                            .collect();
                        if extensions.is_empty() {
                            return None;
                        }

                        let name = filter["name"]
                            .as_str()
                            .map(str::trim)
                            .filter(|name| !name.is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| extensions.join(", "));
                        Some((name, extensions))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self { title, default_name, filters }
    }
}

impl PickerOptions {
    fn dialog(&self, app: &tauri::AppHandle) -> tauri_plugin_dialog::FileDialogBuilder<tauri::Wry> {
        use tauri_plugin_dialog::DialogExt;

        let mut dialog = app.dialog().file().set_title(&self.title);
        if let Some(name) = &self.default_name {
            dialog = dialog.set_file_name(name);
        }
        for (name, extensions) in &self.filters {
            let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
            dialog = dialog.add_filter(name, &extensions);
        }
        dialog
    }
}

async fn handle_file_picker(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    use tauri::Manager;

    let options = PickerOptions::from(&message.data);

    // E2E seam: WebDriver cannot click a native dialog, so a debug build can
    // be told what the user would have picked. The grant matches what the
    // real dialog would have recorded. A packaged (release) app ignores the
    // variable entirely.
    #[cfg(debug_assertions)]
    if let Ok(path) = std::env::var("TURBO_DESKTOP_E2E_PICKER") {
        let grants = app.state::<crate::security::UserGrants>();
        match message.event.as_str() {
            "open-folder" | "open_folder" => grants.grant_folder(&path),
            _ => grants.grant_file(&path),
        }
        log::info!("File picker (e2e seam): {}", path);
        return Ok(serde_json::json!({ "status": "selected", "path": path }));
    }

    match message.event.as_str() {
        "open-folder" | "open_folder" => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            options
                .dialog(app)
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
            options
                .dialog(app)
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
            options
                .dialog(app)
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

/// The count the dock or taskbar badge should show. Nothing clears it.
pub fn badge_count(data: &serde_json::Value) -> Option<i64> {
    data["count"].as_i64().filter(|count| *count > 0)
}

async fn handle_badge(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    use tauri::Manager;

    let count = badge_count(&message.data);
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "There is no main window to badge".to_string())?;

    match window.set_badge_count(count) {
        Ok(()) => Ok(serde_json::json!({ "status": "updated", "count": count.unwrap_or(0) })),
        Err(e) => {
            log::warn!("Could not set the badge: {}", e);
            Ok(serde_json::json!({ "status": "unavailable", "error": e.to_string() }))
        }
    }
}

/// The shortcuts pages have registered, by the accelerator they fire on.
#[derive(Default)]
pub struct RegisteredShortcuts(std::sync::Mutex<std::collections::HashMap<u32, (String, String)>>);

impl RegisteredShortcuts {
    /// The page's id and accelerator for a shortcut that fired.
    pub fn named(&self, shortcut_id: u32) -> Option<(String, String)> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&shortcut_id)
            .cloned()
    }
}

/// Tell the pages that a shortcut one of them registered was pressed.
pub fn shortcut_pressed<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: &str, accelerator: &str) {
    crate::window::deliver_to_all(
        app,
        "bridge-response",
        &serde_json::json!({
            "component": "shortcut",
            "event": "triggered",
            "data": { "id": id, "accelerator": accelerator },
        }),
    );
}

async fn handle_shortcut(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    use tauri::Manager;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

    let accelerator = message.data["accelerator"].as_str().unwrap_or("").trim();

    match message.event.as_str() {
        "register" | "connect" => {
            if accelerator.is_empty() {
                return Ok(serde_json::json!({ "status": "ignored" }));
            }
            let shortcut: Shortcut = accelerator
                .parse()
                .map_err(|e| format!("'{}' is not a shortcut: {}", accelerator, e))?;
            let id = message.data["id"].as_str().unwrap_or(accelerator).to_string();

            let registered = app.state::<RegisteredShortcuts>();
            // Registering again, as a controller does when it reconnects.
            let _ = app.global_shortcut().unregister(shortcut);

            match app.global_shortcut().register(shortcut) {
                Ok(()) => {
                    registered
                        .0
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .insert(shortcut.id(), (id.clone(), accelerator.to_string()));
                    log::info!("Bridge: shortcut '{}' registered as '{}'", accelerator, id);
                    Ok(serde_json::json!({ "status": "registered", "id": id }))
                }
                // Usually another application holding it.
                Err(e) => {
                    log::warn!("Could not register '{}': {}", accelerator, e);
                    Ok(serde_json::json!({ "status": "unavailable", "error": e.to_string() }))
                }
            }
        }
        "unregister" | "disconnect" => {
            if accelerator.is_empty() {
                return Ok(serde_json::json!({ "status": "ignored" }));
            }
            let shortcut: Shortcut = accelerator
                .parse()
                .map_err(|e| format!("'{}' is not a shortcut: {}", accelerator, e))?;

            let _ = app.global_shortcut().unregister(shortcut);
            app.state::<RegisteredShortcuts>()
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&shortcut.id());
            Ok(serde_json::json!({ "status": "unregistered" }))
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
    broadcast_response(
        app,
        &BridgeResponse {
            component: "drag-drop".to_string(),
            event: event.to_string(),
            data,
        },
    );
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

/// The system clipboard, for the cases the webview cannot reach: reading what
/// another application put there, and writing without a user gesture. The
/// webview's own copy/paste keeps working for everything else.
async fn handle_clipboard(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    match message.event.as_str() {
        "read-text" | "read_text" => match app.clipboard().read_text() {
            Ok(text) => Ok(serde_json::json!({ "status": "ok", "text": text })),
            // An empty or non-text clipboard is a normal state, not a failure.
            Err(_) => Ok(serde_json::json!({ "status": "ok", "text": null })),
        },
        "write-text" | "write_text" => {
            let text = message.data["text"]
                .as_str()
                .ok_or("Missing 'text' in clipboard write")?;
            app.clipboard()
                .write_text(text.to_string())
                .map_err(|e| format!("Could not write to the clipboard: {}", e))?;
            Ok(serde_json::json!({ "status": "ok" }))
        }
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

/// Launch-at-login, driven by the app's own settings page.
///
/// Left to the user rather than the config: registering login items
/// silently is how apps end up in "why does this start with my computer"
/// lists. The app asks, the user decides, this records it with the OS
/// (Launch Agent on macOS, registry Run key on Windows, XDG autostart
/// entry on Linux).
async fn handle_autostart(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    use tauri_plugin_autostart::ManagerExt;

    let autolaunch = app.autolaunch();
    match message.event.as_str() {
        "enable" => {
            autolaunch
                .enable()
                .map_err(|e| format!("Could not enable launch at login: {}", e))?;
            Ok(serde_json::json!({ "status": "ok", "enabled": true }))
        }
        "disable" => {
            autolaunch
                .disable()
                .map_err(|e| format!("Could not disable launch at login: {}", e))?;
            Ok(serde_json::json!({ "status": "ok", "enabled": false }))
        }
        "status" => {
            let enabled = autolaunch
                .is_enabled()
                .map_err(|e| format!("Could not read the launch-at-login state: {}", e))?;
            Ok(serde_json::json!({ "status": "ok", "enabled": enabled }))
        }
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
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

#[cfg(test)]
mod native_component_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_message_with_something_to_say_asks_for_a_notification() {
        assert!(asks_for_a_notification("show", &json!({ "title": "Task done" })));
        assert!(asks_for_a_notification("connect", &json!({ "title": "Task done", "body": "Well done" })));
        assert!(asks_for_a_notification("show", &json!({ "body": "Only a body" })));
    }

    // A bridge component says goodbye when its element leaves the page. That
    // is not something to tell the person about.
    #[test]
    fn a_component_going_away_is_not_a_notification() {
        assert!(!asks_for_a_notification("disconnect", &json!({})));
        assert!(!asks_for_a_notification("disconnect", &json!({ "title": "Task done" })));
    }

    #[test]
    fn a_message_with_nothing_to_say_is_not_a_notification() {
        assert!(!asks_for_a_notification("connect", &json!({})));
        assert!(!asks_for_a_notification("show", &json!({ "title": "", "body": "  " })));
    }

    #[test]
    fn the_badge_shows_a_count_and_clears_at_zero() {
        assert_eq!(badge_count(&json!({ "count": 3 })), Some(3));
        assert_eq!(badge_count(&json!({ "count": 0 })), None);
        assert_eq!(badge_count(&json!({})), None);
        assert_eq!(badge_count(&json!({ "count": -2 })), None);
        assert_eq!(badge_count(&json!({ "count": "7" })), None);
    }

    #[test]
    fn a_menu_item_is_known_by_its_id_or_failing_that_its_title() {
        assert_eq!(
            menu_item_id(&json!({ "id": "export", "title": "Export PDF" })).as_deref(),
            Some("bridge-item:export")
        );
        assert_eq!(
            menu_item_id(&json!({ "title": "Export PDF" })).as_deref(),
            Some("bridge-item:Export PDF")
        );
        assert_eq!(menu_item_id(&json!({})), None);
        assert_eq!(menu_item_id(&json!({ "title": "" })), None);
    }

    // A save dialog that opens on an empty name, with no extension, asks the
    // person to know the file's type. The page knows it already.
    #[test]
    fn a_dialog_can_be_told_the_name_and_the_kinds_of_file() {
        let options = PickerOptions::from(&json!({
            "title": "Export tasks",
            "defaultName": "tasks.csv",
            "filters": [{ "name": "CSV", "extensions": ["csv", ".txt"] }]
        }));

        assert_eq!(options.title, "Export tasks");
        assert_eq!(options.default_name.as_deref(), Some("tasks.csv"));
        assert_eq!(
            options.filters,
            vec![("CSV".to_string(), vec!["csv".to_string(), "txt".to_string()])]
        );
    }

    #[test]
    fn a_dialog_told_nothing_is_an_ordinary_dialog() {
        let options = PickerOptions::from(&json!({}));

        assert_eq!(options.title, "Select");
        assert_eq!(options.default_name, None);
        assert!(options.filters.is_empty());
    }

    #[test]
    fn a_name_is_a_name_and_not_somewhere_to_put_it() {
        let options = PickerOptions::from(&json!({ "defaultName": "../../etc/passwd" }));

        assert_eq!(options.default_name.as_deref(), Some("passwd"));
    }

    #[test]
    fn filters_that_say_nothing_are_left_out() {
        let options = PickerOptions::from(&json!({
            "filters": [{ "name": "Empty", "extensions": [] }, { "extensions": ["csv"] }, "nonsense"]
        }));

        assert_eq!(options.filters, vec![("csv".to_string(), vec!["csv".to_string()])]);
    }

    #[test]
    fn a_click_is_traced_back_to_the_item_the_page_registered() {
        assert_eq!(bridge_item_for_menu_event("bridge-item:export"), Some("export"));
        assert_eq!(bridge_item_for_menu_event("quit"), None);
        assert_eq!(bridge_item_for_menu_event("bridge-item:"), None);
    }
}
