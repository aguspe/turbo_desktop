use crate::config::{PathConfigurationStore, Presentation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// Represents a navigation proposal from the JavaScript side.
/// When Turbo initiates a visit, our injected JS sends this to Rust
/// so the native shell can decide how to present the destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitProposal {
    /// The full URL being navigated to
    pub url: String,
    /// The URL path component (e.g., "/posts/1/edit")
    pub path: String,
    /// Turbo visit action: "advance", "replace", or "restore"
    pub action: String,
}

/// Response sent back to JS telling it how to proceed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitResponse {
    pub action: String,
    pub presentation: String,
}

/// Handle a visit proposal from the WebView.
/// This is the core routing logic — it looks up the path configuration
/// and decides whether to navigate in-place, open a modal, or open a new window.
#[tauri::command]
pub async fn handle_visit_proposal(
    app: tauri::AppHandle,
    proposal: VisitProposal,
) -> Result<VisitResponse, String> {
    let config_store = app.state::<Arc<PathConfigurationStore>>();
    let properties = config_store.properties_for_path(&proposal.path);

    log::info!(
        "Visit proposal: {} -> {:?}",
        proposal.path,
        properties.presentation
    );

    match properties.presentation {
        Presentation::Default => {
            // Navigate in the current window — let Turbo Drive handle it
            Ok(VisitResponse {
                action: "advance".into(),
                presentation: "default".into(),
            })
        }
        Presentation::Replace => {
            // Replace current page — no back navigation
            Ok(VisitResponse {
                action: "replace".into(),
                presentation: "replace".into(),
            })
        }
        Presentation::Modal => {
            // Open in a modal window
            let label = format!("modal-{}", uuid_simple());
            let window = WebviewWindowBuilder::new(
                &app,
                &label,
                WebviewUrl::External(proposal.url.parse().map_err(|e| format!("{}", e))?),
            )
            .title(properties.title.unwrap_or_else(|| "".into()))
            .inner_size(800.0, 600.0)
            .resizable(true)
            .build()
            .map_err(|e| format!("Failed to create modal window: {}", e))?;

            // Inject our bridge JS into the new window
            inject_turbo_desktop_js(&window);

            Ok(VisitResponse {
                action: "none".into(),
                presentation: "modal".into(),
            })
        }
        Presentation::NewWindow => {
            // Open in a completely new window
            let label = format!("window-{}", uuid_simple());
            let window = WebviewWindowBuilder::new(
                &app,
                &label,
                WebviewUrl::External(proposal.url.parse().map_err(|e| format!("{}", e))?),
            )
            .title(properties.title.unwrap_or_else(|| "Turbo Desktop".into()))
            .inner_size(1200.0, 800.0)
            .resizable(true)
            .build()
            .map_err(|e| format!("Failed to create new window: {}", e))?;

            inject_turbo_desktop_js(&window);

            Ok(VisitResponse {
                action: "none".into(),
                presentation: "new_window".into(),
            })
        }
        Presentation::Native => {
            // Emit event for native screen handling
            app.emit("native-screen-requested", &proposal)
                .map_err(|e| format!("{}", e))?;

            Ok(VisitResponse {
                action: "none".into(),
                presentation: "native".into(),
            })
        }
        Presentation::None => Ok(VisitResponse {
            action: "none".into(),
            presentation: "none".into(),
        }),
    }
}

/// Update the window title from the web page's <title> tag.
#[tauri::command]
pub async fn update_window_title(window: tauri::Window, title: String) -> Result<(), String> {
    window
        .set_title(&title)
        .map_err(|e| format!("Failed to set title: {}", e))?;
    Ok(())
}

/// Signal that a page has finished loading (Turbo Drive "load" event).
#[tauri::command]
pub async fn page_loaded(app: tauri::AppHandle, url: String) -> Result<(), String> {
    log::info!("Page loaded: {}", url);
    app.emit("turbo:load", &url)
        .map_err(|e| format!("{}", e))?;
    Ok(())
}

/// Signal that a page started loading (Turbo Drive "before-visit" event).
#[tauri::command]
pub async fn page_loading(app: tauri::AppHandle, url: String) -> Result<(), String> {
    log::info!("Page loading: {}", url);
    app.emit("turbo:before-visit", &url)
        .map_err(|e| format!("{}", e))?;
    Ok(())
}

/// Close a modal window by label.
#[tauri::command]
pub async fn close_modal(app: tauri::AppHandle, label: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        window
            .close()
            .map_err(|e| format!("Failed to close window: {}", e))?;
    }
    Ok(())
}

/// Inject the turbo-desktop.js bridge script into a webview window.
fn inject_turbo_desktop_js(window: &tauri::WebviewWindow) {
    let js = include_str!("../../src/turbo-desktop.js");
    let _ = window.eval(js);
}

/// Simple unique ID generator (no external crate needed).
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{:x}", nanos)
}
