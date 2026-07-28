// Prevents a console window from appearing on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod config;
mod connection;
mod deep_link;
mod fs_bridge;
mod menu;
mod navigation;
mod process_manager;
mod security;
mod shell_bridge;
mod sudo_bridge;
mod tray;
mod updater_bridge;
mod window;

use config::PathConfigurationStore;
use connection::{ConnectionMonitor, Transition, VisitError};
use std::sync::Arc;
use std::time::Duration;
use tauri::webview::PageLoadEvent;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_deep_link::DeepLinkExt;

/// How often to check that the app server is still answering.
const PROBE_INTERVAL: Duration = Duration::from_secs(5);

fn main() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(process_manager::ProcessManager::new())
        .manage(window::LastWindowSize::default())
        .manage(window::FocusTracker::default())
        // Inject turbo-desktop.js into every page load across all webviews.
        .on_page_load(|webview, payload| {
            if let PageLoadEvent::Finished = payload.event() {
                let js = include_str!("../../src/turbo-desktop.js");
                let _ = webview.eval(js);

                log::info!("Injected turbo-desktop.js into {}", payload.url());
            }
        })
        .setup(move |app| {
            // Loaded here rather than in main() because a packaged app reads it
            // from the bundle's resource directory, which needs the app handle.
            let loaded = window::ConfigLookup::for_app(app)
                .load()
                .map_err(Box::<dyn std::error::Error>::from)?;

            match &loaded.source {
                Some(path) => log::info!("Configuration loaded from {}", path.display()),
                None => log::warn!(
                    "No {} found — starting with development defaults",
                    window::CONFIG_FILENAME
                ),
            }

            let app_config = loaded.config;
            let server_url = app_config.server_url.clone();
            let app_name = app_config.app_name.clone();
            let user_agent = app_config.user_agent.clone();
            let window_config = app_config.window.clone();
            let path_config_url = window::path_config_url(&app_config);
            let shell_defaults = app_config.clone();

            // Remembered window size, if the user has one. This file is theirs to
            // edit, so it can only carry geometry — never anything from the
            // security policy, which comes from the config above.
            let preferences = window::load_preferences(app.path().app_config_dir().ok().as_deref());
            let (window_width, window_height) = preferences.window_size(&window_config);
            log::debug!("Opening window at {}x{}", window_width, window_height);

            // Shared as an Arc so the background fetch task can hold on to it.
            let config_store = Arc::new(PathConfigurationStore::new());
            let config_store_for_fetch = config_store.clone();

            // Start with rules rather than none: the copy the server gave us last
            // time, or the one shipped in the bundle. The fetch below replaces
            // them when it succeeds.
            let cache_dir = app.path().app_config_dir().ok();
            let resource_dir = app.path().resource_dir().ok();
            match config::startup_configuration(cache_dir.as_deref(), resource_dir.as_deref()) {
                Some((path_config, source)) => {
                    log::info!(
                        "Path configuration: {} rules from the {:?}",
                        path_config.rules.len(),
                        source
                    );
                    config_store.set(path_config);
                }
                None => log::info!("No path configuration yet; asking the server"),
            }
            app.manage(config_store);
            app.manage(app_config);

            let app_handle = app.handle().clone();

            // Build and set the native menu bar
            let menu = menu::build_menu(&app_handle)?;
            app.set_menu(menu)?;

            // Handle menu events
            let menu_handle = app_handle.clone();
            app.on_menu_event(move |_app, event| {
                menu::handle_menu_event(&menu_handle, event.id().as_ref());
            });

            // Build the main window here rather than in tauri.conf.json, so it can
            // carry the runtime configuration: the user agent the Rails gem detects
            // on, and the window geometry from turbo-desktop.config.json. Opening
            // straight at the app URL also avoids loading a local page and then
            // scripting a redirect away from it.
            let url: url::Url = server_url.parse().expect("Invalid server URL");

            let reachable_at_startup = connection::server_is_reachable(&url);
            let target = if reachable_at_startup {
                WebviewUrl::External(url.clone())
            } else {
                log::warn!("Could not reach {} — opening the error page instead", url);
                WebviewUrl::App(
                    format!("error.html?error={}", VisitError::NetworkFailure.slug()).into(),
                )
            };


            let main_window = window::apply_shell_defaults(
                WebviewWindowBuilder::new(app, "main", target),
                &app.handle().clone(),
                &shell_defaults,
                "main",
            )
            .title(&app_name)
            .inner_size(window_width, window_height)
            .min_inner_size(window_config.min_width, window_config.min_height)
            .resizable(window_config.resizable)
            .build()?;

            // Track the size as it changes. A handler registered on the builder
            // does not apply to windows created here, so it is attached directly.
            // This is a fallback for exits that reach us after the window is gone;
            // normally the size is read from the window itself on the way out.
            let size_handle = app.handle().clone();
            let focus_config = shell_defaults.navigation.clone();
            main_window.on_window_event(move |event| match event {
                tauri::WindowEvent::Resized(size) => {
                    if let Some(window) = size_handle.get_webview_window("main") {
                        if let Ok(scale) = window.scale_factor() {
                            size_handle.state::<window::LastWindowSize>().set(
                                f64::from(size.width) / scale,
                                f64::from(size.height) / scale,
                            );
                        }
                    }
                }
                tauri::WindowEvent::Focused(focused) => {
                    on_focus_changed(&size_handle, &focus_config, *focused);
                }
                _ => {}
            });

            // Fetch path configuration from the server in the background
            let pc_url = path_config_url.clone();
            let pc_user_agent = user_agent.clone();
            let store = config_store_for_fetch.clone();
            let pc_cache_dir = cache_dir.clone();
            tauri::async_runtime::spawn(async move {
                match config::fetch_path_configuration(&pc_url, &pc_user_agent).await {
                    Ok(pc) => {
                        log::info!("Path configuration: {} rules from the server", pc.rules.len());

                        // Keep it for the next launch before handing it over.
                        if let Some(dir) = &pc_cache_dir {
                            if let Err(e) = config::save_cache(dir, &pc) {
                                log::warn!("{}", e);
                            }
                        }

                        store.set(pc);
                    }
                    Err(e) => {
                        log::warn!("Could not fetch path configuration: {}", e);
                        log::info!("Keeping the rules already loaded");
                    }
                }
            });

            // Watch the server so a drop is noticed while the app sits idle.
            // The web layer cannot see this on its own: the browser's `offline`
            // event reports the machine losing its network, not the app server
            // going away, which is the case that actually happens.
            watch_connection(app_handle.clone(), url.clone(), reachable_at_startup);

            // Links from outside the app: your-app://orders/123
            let deep_link_app = app_handle.clone();
            app.deep_link().on_open_url(move |event| {
                deep_link::handle(&deep_link_app, event.urls());
            });

            // Set up the system tray icon
            if let Err(e) = tray::setup_tray(&app_handle) {
                log::warn!("Could not set up system tray: {}", e);
            }

            log::info!("Turbo Desktop started — server: {}", server_url);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            navigation::handle_visit_proposal,
            navigation::update_window_title,
            navigation::page_loaded,
            navigation::page_loading,
            navigation::close_modal,
            navigation::dismiss_modal,
            bridge::handle_bridge_message,
            bridge::send_bridge_response,
            connection::retry_connection,
            window::get_window_info,
        ])
        .build(tauri::generate_context!())
        .expect("Error building Turbo Desktop")
        .run(|app_handle, event| {
            // Both, because which one arrives depends on how the app was closed:
            // the last window closing raises ExitRequested, while the Quit menu
            // item terminates through Cocoa and only reaches Exit.
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                log::debug!("Shutting down; saving window size");
                remember_window_size(app_handle);

                let pm = app_handle.state::<process_manager::ProcessManager>();
                tauri::async_runtime::block_on(pm.kill_all());
            }
        });
}

/// Tell the page the window came back, and whether it is due a refresh.
///
/// The decision is made here because the threshold lives in the config, but it
/// is only a proposal: the page can refuse it, and does by default when someone
/// is typing.
fn on_focus_changed(
    app: &tauri::AppHandle,
    config: &window::NavigationConfig,
    focused: bool,
) {
    let tracker = app.state::<window::FocusTracker>();

    if !focused {
        tracker.left();
        return;
    }

    // No recorded absence means this is the window opening, not returning.
    let Some(away_seconds) = tracker.returned() else {
        return;
    };

    let refreshing = window::should_refresh_after(config, away_seconds);
    if refreshing {
        log::info!("Back after {}s — proposing a refresh", away_seconds);
    }

    if let Some(main) = app.get_webview_window("main") {
        window::deliver_to_page(
            &main,
            "focus",
            &serde_json::json!({ "awaySeconds": away_seconds, "refreshing": refreshing }),
        );
    }
}

/// Hand a URL to whatever the operating system uses for it.
pub fn open_externally(app: &tauri::AppHandle, url: &url::Url) {
    use tauri_plugin_opener::OpenerExt;

    log::info!("Opening {} outside the app", url);
    if let Err(e) = app.opener().open_url(url.as_str(), None::<&str>) {
        log::warn!("Could not open {}: {}", url, e);
    }
}

/// Poll the app server and tell the web layer when reachability changes.
///
/// Only transitions are emitted, so a server that stays down is reported once
/// rather than every few seconds.
fn watch_connection(app: tauri::AppHandle, url: url::Url, reachable_at_startup: bool) {
    tauri::async_runtime::spawn(async move {
        let mut monitor = ConnectionMonitor::new();

        // Seed the monitor so a start on the error page is already "offline" and
        // recovery gets announced rather than passing silently.
        if !reachable_at_startup {
            for _ in 0..2 {
                monitor.record(false);
            }
        }

        loop {
            tokio::time::sleep(PROBE_INTERVAL).await;

            let probe_url = url.clone();
            let reachable =
                tokio::task::spawn_blocking(move || connection::server_is_reachable(&probe_url))
                    .await
                    .unwrap_or(false);

            let payload = match monitor.record(reachable) {
                Transition::WentOffline(error) => {
                    log::warn!("Lost the connection to {}", url);
                    serde_json::json!({ "online": false, "error": error })
                }
                Transition::CameOnline => {
                    log::info!("Reconnected to {}", url);
                    return_to_app_if_on_error_page(&app, &url);
                    serde_json::json!({ "online": true, "error": null })
                }
                Transition::Unchanged => continue,
            };

            window::deliver_to_all(&app, "connection", &payload);
        }
    });
}

/// Send the window back to the app once the server answers again.
///
/// Only when it is sitting on the bundled error page — if the app is still
/// loaded, navigating would throw away whatever the person was doing, and the
/// web layer handles that case itself with the connection event.
///
/// Recovery is driven from here rather than by the error page polling, because
/// that page runs under the bundle's content security policy and cannot reach
/// the server to find out for itself.
fn return_to_app_if_on_error_page(app: &tauri::AppHandle, url: &url::Url) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(current) = window.url() else {
        return;
    };

    if !security::is_bundled_app_origin(&current) {
        return;
    }

    log::info!("Returning to {}", url);
    if let Err(e) = window.navigate(url.clone()) {
        log::warn!("Could not navigate back to the app: {}", e);
    }
}

/// Write the last known window size so the next launch reopens at it.
///
/// Best-effort: failing to write preferences is not worth interrupting a quit,
/// so problems are logged and dropped. Called from every plausible exit point,
/// and writing the same size twice is harmless.
fn remember_window_size(app: &tauri::AppHandle) {
    // Measure the window if it is still there, otherwise use the last size we saw.
    // outer_size, not inner_size: the whole window is what the builder reproduces
    // on the next launch, so it is what has to be measured. The window manager may
    // also have shrunk the window to fit the screen's usable area, and recording
    // the result of that settles on a stable size instead of losing the title bar
    // height again on every launch.
    let current = app
        .get_webview_window("main")
        .and_then(|window| Some((window.outer_size().ok()?, window.scale_factor().ok()?)))
        .map(|(size, scale)| (f64::from(size.width) / scale, f64::from(size.height) / scale));

    let Some((width, height)) = current.or_else(|| app.state::<window::LastWindowSize>().get())
    else {
        log::debug!("No window size available; nothing to save");
        return;
    };
    let Ok(dir) = app.path().app_config_dir() else {
        log::warn!("No config directory available; window size not saved");
        return;
    };

    let preferences = window::Preferences {
        window: Some(window::WindowPreferences { width, height }),
    };

    if let Err(e) = window::save_preferences(&dir, &preferences) {
        log::warn!("Could not save window preferences: {}", e);
    }
}
