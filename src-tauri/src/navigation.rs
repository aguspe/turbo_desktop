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
    webview: tauri::Webview,
    proposal: VisitProposal,
) -> Result<VisitResponse, String> {
    crate::security::ensure_trusted_caller(&app, &webview)?;

    let config_store = app.state::<Arc<PathConfigurationStore>>();
    let properties = config_store.properties_for_path(&proposal.path);

    log::info!(
        "Visit proposal: {} -> {:?}",
        proposal.path,
        properties.presentation
    );

    // Somewhere else entirely — hand it over and tell the web layer to drop it.
    if handed_to_the_system(&app, &proposal.url) {
        return Ok(VisitResponse {
            action: "none".into(),
            presentation: "external".into(),
        });
    }

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
                WebviewUrl::External(same_origin_url(&app, &proposal.url)?),
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
                WebviewUrl::External(same_origin_url(&app, &proposal.url)?),
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

/// Parse a proposed URL and confirm it stays on the app origin.
///
/// A proposal names the URL a new window will open, and we inject the bridge
/// into that window — so a proposal pointing at someone else's site would hand
/// them a window carrying our API.
fn same_origin_url(app: &tauri::AppHandle, raw: &str) -> Result<url::Url, String> {
    let config = app.state::<crate::window::TurboDesktopConfig>();
    let url: url::Url = raw
        .parse()
        .map_err(|e| format!("Invalid visit proposal URL '{}': {}", raw, e))?;

    if crate::security::is_trusted_origin(&config.server_url, &url) {
        Ok(url)
    } else {
        log::warn!("Navigation: refused a visit proposal to '{}'", url);
        Err(format!(
            "Refused: '{}' is not on the configured app origin",
            raw
        ))
    }
}

/// Whether this proposal is for somewhere else, and if so, send it there.
///
/// A rule can name any path, including one that points off-origin. Opening a
/// window on someone else's site and injecting our bridge into it would be
/// wrong, so those go to the browser like any other external link.
fn handed_to_the_system(app: &tauri::AppHandle, raw: &str) -> bool {
    let config = app.state::<crate::window::TurboDesktopConfig>();
    let Ok(url) = raw.parse::<url::Url>() else {
        return false;
    };

    match crate::security::destination_for(
        &config.server_url,
        &config.navigation.internal_hosts,
        &url,
    ) {
        crate::security::LinkDestination::App => false,
        crate::security::LinkDestination::SystemBrowser => {
            crate::open_externally(app, &url);
            true
        }
    }
}

/// Update the window title from the web page's <title> tag.
#[tauri::command]
pub async fn update_window_title(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    window: tauri::Window,
    title: String,
) -> Result<(), String> {
    crate::security::ensure_trusted_caller(&app, &webview)?;

    window
        .set_title(&title)
        .map_err(|e| format!("Failed to set title: {}", e))?;
    Ok(())
}

/// Signal that a page has finished loading (Turbo Drive "load" event).
#[tauri::command]
pub async fn page_loaded(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    url: String,
) -> Result<(), String> {
    crate::security::ensure_trusted_caller(&app, &webview)?;

    log::info!("Page loaded: {}", url);
    app.emit("turbo:load", &url)
        .map_err(|e| format!("{}", e))?;
    Ok(())
}

/// Signal that a page started loading (Turbo Drive "before-visit" event).
#[tauri::command]
pub async fn page_loading(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    url: String,
) -> Result<(), String> {
    crate::security::ensure_trusted_caller(&app, &webview)?;

    log::info!("Page loading: {}", url);
    app.emit("turbo:before-visit", &url)
        .map_err(|e| format!("{}", e))?;
    Ok(())
}

/// Close a modal window by label.
#[tauri::command]
pub async fn close_modal(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    label: String,
) -> Result<(), String> {
    crate::security::ensure_trusted_caller(&app, &webview)?;

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
