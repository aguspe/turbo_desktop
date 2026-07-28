use tauri::Manager;

/// Handle an incoming deep link URL.
///
/// Deep links follow the pattern: `turbo-desktop://path/to/page`
/// The path portion is matched against the path configuration rules
/// just like a regular Turbo visit, and the main window navigates
/// to the corresponding Rails route.
pub fn handle_deep_link(app: &tauri::AppHandle, urls: Vec<url::Url>) {
    for url in urls {
        log::info!("Deep link received: {}", url);

        // Extract the path from the deep link URL
        let path = url.path();
        if path.is_empty() || path == "/" {
            continue;
        }

        // Resolve the path against the configured server. Joining through the URL
        // parser keeps a hostile deep link from redirecting to another origin, and
        // JSON-encoding keeps it from breaking out of the string into script.
        let config = app.state::<crate::window::TurboDesktopConfig>();
        let Ok(server) = url::Url::parse(&config.server_url) else {
            log::warn!("Deep link ignored: server_url is not a valid URL");
            continue;
        };
        let Ok(target) = server.join(path) else {
            log::warn!("Deep link ignored: could not resolve path '{}'", path);
            continue;
        };
        if !crate::security::is_trusted_origin(&config.server_url, &target) {
            log::warn!("Deep link ignored: '{}' leaves the app origin", target);
            continue;
        }
        let target_url = target.to_string();

        // Navigate the main window to the target URL
        if let Some(window) = app.get_webview_window("main") {
            let encoded = match serde_json::to_string(&target_url) {
                Ok(encoded) => encoded,
                Err(e) => {
                    log::warn!("Deep link ignored: could not encode target: {}", e);
                    continue;
                }
            };
            let js = format!("window.location.href = {}", encoded);
            let _ = window.eval(&js);

            // Ensure the window is visible and focused
            let _ = window.show();
            let _ = window.set_focus();

            log::info!("Deep link navigated to: {}", target_url);
        }
    }
}
