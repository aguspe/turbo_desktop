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

        // Get the server URL from the app config
        let config = app.state::<crate::window::TurboDesktopConfig>();
        let target_url = format!("{}{}", config.server_url, path);

        // Navigate the main window to the target URL
        if let Some(window) = app.get_webview_window("main") {
            let js = format!("window.location.href = '{}'", target_url);
            let _ = window.eval(&js);

            // Ensure the window is visible and focused
            let _ = window.show();
            let _ = window.set_focus();

            log::info!("Deep link navigated to: {}", target_url);
        }
    }
}
