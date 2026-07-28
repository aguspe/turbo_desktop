// Prevents a console window from appearing on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod config;
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
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;
use tauri::webview::PageLoadEvent;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// How long to wait when checking whether the app server is up.
const REACHABILITY_TIMEOUT: Duration = Duration::from_millis(500);

/// Can we open a TCP connection to the app server?
///
/// Decides whether the window opens on the app or on the bundled "waiting for
/// your server" page. A plain connect keeps startup predictable — no HTTP
/// client, no async runtime, and a bounded wait.
fn server_is_reachable(url: &url::Url) -> bool {
    let (Some(host), Some(port)) = (url.host_str(), url.port_or_known_default()) else {
        return false;
    };

    match (host, port).to_socket_addrs() {
        Ok(addrs) => addrs
            .into_iter()
            .any(|addr| TcpStream::connect_timeout(&addr, REACHABILITY_TIMEOUT).is_ok()),
        Err(_) => false,
    }
}

fn main() {
    env_logger::init();

    // Load the app configuration (server URL, window size, etc.)
    let app_config = window::load_config(None).expect("Failed to load turbo-desktop config");
    let server_url = app_config.server_url.clone();
    let app_name = app_config.app_name.clone();
    let user_agent = app_config.user_agent.clone();
    let window_config = app_config.window.clone();
    let path_config_url = window::path_config_url(&app_config);

    // Create the path config store as Arc so we can share it with the async fetch task.
    let config_store = Arc::new(PathConfigurationStore::new());
    let config_store_for_fetch = config_store.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(config_store)
        .manage(app_config)
        .manage(process_manager::ProcessManager::new())
        // Inject turbo-desktop.js into every page load across all webviews.
        .on_page_load(|webview, payload| {
            if let PageLoadEvent::Finished = payload.event() {
                let js = include_str!("../../src/turbo-desktop.js");
                let _ = webview.eval(js);

                log::info!("Injected turbo-desktop.js into {}", payload.url());
            }
        })
        .setup(move |app| {
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

            let target = if server_is_reachable(&url) {
                WebviewUrl::External(url.clone())
            } else {
                log::warn!(
                    "Could not reach {} — opening the bundled waiting page instead",
                    url
                );
                WebviewUrl::App("index.html".into())
            };

            // The waiting page polls this to know where to redirect once the
            // server answers; without it that page falls back to a guess.
            let server_url_script = format!(
                "window.__TURBO_DESKTOP_SERVER_URL__ = {};",
                serde_json::to_string(url.as_str()).expect("URL should serialize")
            );

            WebviewWindowBuilder::new(app, "main", target)
                .title(&app_name)
                .user_agent(&user_agent)
                .inner_size(window_config.width, window_config.height)
                .min_inner_size(window_config.min_width, window_config.min_height)
                .resizable(window_config.resizable)
                .initialization_script(&server_url_script)
                .build()?;

            // Fetch path configuration from the server in the background
            let pc_url = path_config_url.clone();
            let pc_user_agent = user_agent.clone();
            let store = config_store_for_fetch.clone();
            tauri::async_runtime::spawn(async move {
                match config::fetch_path_configuration(&pc_url, &pc_user_agent).await {
                    Ok(pc) => {
                        log::info!("Path configuration loaded: {} rules", pc.rules.len());
                        store.set(pc);
                    }
                    Err(e) => {
                        log::warn!("Could not fetch path configuration: {}", e);
                        log::info!("Using default path configuration (all routes -> default)");
                    }
                }
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
            bridge::handle_bridge_message,
            bridge::send_bridge_response,
            window::get_window_info,
        ])
        .build(tauri::generate_context!())
        .expect("Error building Turbo Desktop")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let pm = app_handle.state::<process_manager::ProcessManager>();
                tauri::async_runtime::block_on(pm.kill_all());
            }
        });
}
