//! Opening the app at a particular page from outside it.
//!
//! A link like `your-app://orders/123` — from an email, a calendar entry, or
//! another application — lands here and becomes a visit to the matching page on
//! your server. This is the desktop counterpart of Hotwire Native's universal
//! links, minus the domain verification those get from the OS.
//!
//! The scheme is registered per app rather than shared across everything built
//! on this shell, because no desktop OS arbitrates duplicate registrations in a
//! way you control: two apps claiming one scheme means links silently open the
//! wrong one.

use tauri::Manager;

/// Turn a deep link into the page on the app server it refers to.
///
/// Everything after the scheme is treated as a path, so `your-app://orders/123`
/// and `your-app:/orders/123` both mean `/orders/123`. The result is resolved
/// against the configured server and rejected if it lands anywhere else — a
/// link arrives from outside the app, so it is not trusted to say where to go.
pub fn resolve(server_url: &str, link: &url::Url) -> Result<url::Url, String> {
    let server: url::Url = server_url
        .parse()
        .map_err(|e| format!("Invalid server URL: {}", e))?;

    // A custom scheme puts the first segment in the host, so `app://orders/123`
    // parses as host "orders" with path "/123". Stitch them back together.
    let mut path = String::new();
    if let Some(host) = link.host_str() {
        path.push('/');
        path.push_str(host);
    }
    path.push_str(link.path());

    if path.is_empty() || path == "/" {
        return Err("Deep link has no path".to_string());
    }

    let mut target = server
        .join(&path)
        .map_err(|e| format!("Could not resolve '{}': {}", link, e))?;
    target.set_query(link.query());
    target.set_fragment(link.fragment());

    if crate::security::is_trusted_origin(server_url, &target) {
        Ok(target)
    } else {
        Err(format!("Refused: '{}' resolves outside the app", link))
    }
}

/// Handle deep links as they arrive.
pub fn handle(app: &tauri::AppHandle, urls: Vec<url::Url>) {
    let config = app.state::<crate::window::TurboDesktopConfig>();

    for link in urls {
        log::info!("Deep link: {}", link);

        let target = match resolve(&config.server_url, &link) {
            Ok(target) => target,
            Err(e) => {
                log::warn!("{}", e);
                continue;
            }
        };

        let Some(window) = app.get_webview_window("main") else {
            continue;
        };

        // Ask the page to visit, so Turbo handles it as a normal navigation and
        // the path configuration still decides how it is presented. Falling back
        // to navigating the window covers a page that has not loaded yet.
        crate::window::deliver_to_page(
            &window,
            "visit",
            &serde_json::json!({ "url": target.as_str() }),
        );

        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Files the OS asked the app to open, waiting for the page to collect them.
///
/// A file association launch usually happens before the page has loaded, so a
/// pushed event would land in an empty webview. The paths queue here instead:
/// the shell pings the page, and the page drains the queue through the bridge
/// — on its own startup if the ping arrived too early.
#[derive(Default)]
pub struct PendingOpenedFiles(std::sync::Mutex<Vec<String>>);

/// Handle files the OS handed to the app — a double-click on an associated
/// type, "Open With…", or a file dropped on the app's icon.
pub fn handle_files(app: &tauri::AppHandle, paths: Vec<std::path::PathBuf>) {
    if paths.is_empty() {
        return;
    }

    // Being asked to open a file is the same consent as picking it in a
    // dialog, so the page can read what it was handed.
    let grants = app.state::<crate::security::UserGrants>();
    let mut opened: Vec<String> = Vec::new();
    for path in &paths {
        let raw = path.to_string_lossy().into_owned();
        if path.is_dir() {
            grants.grant_folder(&raw);
        } else {
            grants.grant_file(&raw);
        }
        log::info!("Opening from the OS: {}", raw);
        opened.push(raw);
    }

    app.state::<PendingOpenedFiles>()
        .0
        .lock()
        .unwrap()
        .extend(opened);

    // Ping a loaded page so it drains the queue now; a page that is not there
    // yet misses the ping and drains on its own startup instead.
    if let Some(window) = app.get_webview_window("main") {
        crate::window::deliver_to_page(&window, "file-open-pending", &serde_json::json!({}));
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Bridge handler: the page collects (and thereby clears) the queued files.
pub fn drain_pending(app: &tauri::AppHandle) -> Vec<String> {
    std::mem::take(&mut *app.state::<PendingOpenedFiles>().0.lock().unwrap())
}

/// The paths the OS launched the app with, on platforms where an associated
/// file arrives as a plain argument (Windows and Linux; macOS uses an event).
pub fn paths_from_args<I: Iterator<Item = String>>(args: I) -> Vec<std::path::PathBuf> {
    args.skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .map(std::path::PathBuf::from)
        .filter(|path| path.exists())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(s: &str) -> url::Url {
        url::Url::parse(s).expect("test link should parse")
    }

    #[test]
    fn only_existing_non_flag_arguments_are_opened_files() {
        let file = std::env::temp_dir().join("turbo-desktop-assoc.txt");
        std::fs::write(&file, "x").unwrap();

        let args = vec![
            "/usr/bin/app".to_string(),
            "--flag".to_string(),
            file.to_string_lossy().into_owned(),
            "/nonexistent/other.txt".to_string(),
        ];

        assert_eq!(paths_from_args(args.into_iter()), vec![file.clone()]);
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn the_first_segment_is_part_of_the_path() {
        // A custom scheme parses that segment as the host, which it is not.
        let target = resolve("https://app.example.com", &link("myapp://orders/123")).unwrap();

        assert_eq!(target.as_str(), "https://app.example.com/orders/123");
    }

    #[test]
    fn a_single_segment_link_still_resolves() {
        let target = resolve("https://app.example.com", &link("myapp://settings")).unwrap();

        assert_eq!(target.as_str(), "https://app.example.com/settings");
    }

    #[test]
    fn queries_and_fragments_survive() {
        let target = resolve(
            "https://app.example.com",
            &link("myapp://search?q=hello#results"),
        )
        .unwrap();

        assert_eq!(target.query(), Some("q=hello"));
        assert_eq!(target.fragment(), Some("results"));
    }

    #[test]
    fn a_link_with_no_path_is_refused() {
        assert!(resolve("https://app.example.com", &link("myapp://")).is_err());
    }

    #[test]
    fn a_link_cannot_send_the_app_somewhere_else() {
        // Deep links arrive from outside, so a link naming another host must not
        // be able to point the app at it.
        let err = resolve(
            "https://app.example.com",
            &link("myapp:https://evil.example.com/steal"),
        )
        .expect_err("an absolute URL to another origin must be refused");

        assert!(err.contains("outside the app"), "unexpected error: {err}");
    }

    #[test]
    fn traversal_cannot_climb_out_of_the_server() {
        let target = resolve("https://app.example.com/", &link("myapp://../../etc/passwd"));

        // Either refused, or normalised back onto the app's own origin.
        if let Ok(url) = target {
            assert_eq!(url.host_str(), Some("app.example.com"));
        }
    }
}
