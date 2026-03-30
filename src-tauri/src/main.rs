// Prevents a console window from appearing on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod config;
mod deep_link;
mod fs_bridge;
mod menu;
mod navigation;
mod process_manager;
mod shell_bridge;
mod sudo_bridge;
mod tray;
mod updater_bridge;
mod window;

use config::PathConfigurationStore;
use std::sync::Arc;
use tauri::webview::PageLoadEvent;
use tauri::Manager;

fn main() {
    env_logger::init();

    // Load the app configuration (server URL, window size, etc.)
    let app_config = window::load_config(None).expect("Failed to load turbo-desktop config");
    let server_url = app_config.server_url.clone();
    let _user_agent = app_config.user_agent.clone();
    let app_name = app_config.app_name.clone();
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
        // In debug builds, also inject the bridge inspector overlay.
        .on_page_load(|webview, payload| {
            if let PageLoadEvent::Finished = payload.event() {
                let js = include_str!("../../src/turbo-desktop.js");
                let _ = webview.eval(js);

                #[cfg(debug_assertions)]
                {
                    let inspector_js = include_str!("../../src/inspector.js");
                    let _ = webview.eval(inspector_js);
                }

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

            // Get the main window created by tauri.conf.json
            let main_window = app
                .get_webview_window("main")
                .expect("main window not found");

            // Set the window title to the app name
            main_window.set_title(&app_name).ok();

            // Navigate to the Rails server URL
            let url: url::Url = server_url.parse().expect("Invalid server URL");
            main_window
                .eval(&format!("window.location.replace('{}')", url))
                .ok();

            // Fetch path configuration from the server in the background
            let pc_url = path_config_url.clone();
            let store = config_store_for_fetch.clone();
            tauri::async_runtime::spawn(async move {
                match config::fetch_path_configuration(&pc_url).await {
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
