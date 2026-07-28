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
            open_child_window(
                &app,
                &proposal.url,
                &format!("modal-{}", uuid_simple()),
                properties.title.clone().unwrap_or_default(),
                (properties.width.unwrap_or(800.0), properties.height.unwrap_or(600.0)),
            )?;

            Ok(VisitResponse {
                action: "none".into(),
                presentation: "modal".into(),
            })
        }
        Presentation::NewWindow => {
            open_child_window(
                &app,
                &proposal.url,
                &format!("window-{}", uuid_simple()),
                properties.title.clone().unwrap_or_else(|| "Turbo Desktop".into()),
                (properties.width.unwrap_or(1200.0), properties.height.unwrap_or(800.0)),
            )?;

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

/// What the screen underneath should do once a modal closes.
///
/// Named after Hotwire Native's dismissal semantics, so the same words describe
/// the same outcome on mobile and desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dismissal {
    /// Go back, as if the modal had never been opened.
    Recede,
    /// Stay where it is, but reload — the usual choice after a form submits.
    Refresh,
    /// Leave it exactly as it was.
    Resume,
}

impl Dismissal {
    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("resume") {
            "recede" => Self::Recede,
            "refresh" => Self::Refresh,
            _ => Self::Resume,
        }
    }

    /// The instruction handed to the page underneath.
    pub fn action(&self) -> &'static str {
        match self {
            Self::Recede => "back",
            Self::Refresh => "refresh",
            Self::Resume => "none",
        }
    }
}

/// Close a modal and tell the screen underneath what to do next.
///
/// `label` defaults to the calling window, so a page can dismiss itself without
/// knowing which window it was opened in.
#[tauri::command]
pub async fn dismiss_modal(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    label: Option<String>,
    then: Option<String>,
) -> Result<(), String> {
    crate::security::ensure_trusted_caller(&app, &webview)?;

    let label = label.unwrap_or_else(|| webview.label().to_string());
    let dismissal = Dismissal::parse(then.as_deref());

    if let Some(window) = app.get_webview_window(&label) {
        window
            .close()
            .map_err(|e| format!("Failed to close '{}': {}", label, e))?;
    }

    log::info!("Dismissed '{}' with {:?}", label, dismissal);

    if let Some(main) = app.get_webview_window("main") {
        crate::window::deliver_to_page(
            &main,
            "navigate",
            &serde_json::json!({ "action": dismissal.action() }),
        );
    }

    Ok(())
}

/// Open a modal or secondary window carrying the shell's configuration.
///
/// Both presentations differ only in their label, title and default size, and
/// both need everything the main window has — the user agent, external-link
/// handling and the globals the injected script reads.
fn open_child_window(
    app: &tauri::AppHandle,
    raw_url: &str,
    label: &str,
    title: String,
    (width, height): (f64, f64),
) -> Result<(), String> {
    let url = same_origin_url(app, raw_url)?;
    let config = app.state::<crate::window::TurboDesktopConfig>();

    let mut builder = crate::window::apply_shell_defaults(
        WebviewWindowBuilder::new(app, label, WebviewUrl::External(url)),
        app,
        &config,
        label,
    )
    .title(title)
    .inner_size(width, height)
    .resizable(true);

    // Tie a modal to the window it came from, so it travels with it and closes
    // with it rather than being left behind as an orphan. Secondary windows are
    // meant to stand alone, so they are not parented.
    //
    // This is ownership, not modality: the main window stays interactive. A true
    // blocking sheet needs AppKit APIs that Tauri does not expose.
    if label.starts_with("modal-") {
        if let Some(parent) = app.get_webview_window("main") {
            builder = builder
                .parent(&parent)
                .map_err(|e| format!("Could not attach '{}' to the main window: {}", label, e))?;
        }
    }

    let window = builder
        .build()
        .map_err(|e| format!("Failed to create window '{}': {}", label, e))?;

    inject_turbo_desktop_js(&window);
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dismissal_defaults_to_leaving_the_screen_alone() {
        assert_eq!(Dismissal::parse(None), Dismissal::Resume);
        assert_eq!(Dismissal::parse(Some("nonsense")), Dismissal::Resume);
    }

    #[test]
    fn dismissal_names_match_hotwire_native() {
        assert_eq!(Dismissal::parse(Some("recede")), Dismissal::Recede);
        assert_eq!(Dismissal::parse(Some("refresh")), Dismissal::Refresh);
        assert_eq!(Dismissal::parse(Some("resume")), Dismissal::Resume);
    }

    #[test]
    fn each_dismissal_instructs_the_page_underneath() {
        assert_eq!(Dismissal::Recede.action(), "back");
        assert_eq!(Dismissal::Refresh.action(), "refresh");
        assert_eq!(Dismissal::Resume.action(), "none");
    }
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
