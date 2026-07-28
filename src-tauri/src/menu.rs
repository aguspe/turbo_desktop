use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    Manager, Runtime,
};

/// Build the native menu bar.
///
/// On macOS this provides the full standard menu structure (About, Services, Hide, Quit).
/// On Windows/Linux it provides a simpler menu (File > Quit).
///
/// Bridge components can dynamically add items via the "menu-item" bridge component.
pub fn build_menu<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Menu<R>, tauri::Error> {
    let app_menu = {
        let mut builder = SubmenuBuilder::new(app, "Turbo Desktop");
        #[cfg(target_os = "macos")]
        {
            builder = builder
                .about(None)
                .separator()
                .services()
                .separator()
                .hide()
                .hide_others()
                .show_all()
                .separator();
        }
        // A custom Quit rather than the predefined one: that maps to Cocoa's
        // `terminate:`, which ends the process without Tauri ever raising an exit
        // event, so child processes are never reaped and window preferences are
        // never written. This routes through AppHandle::exit instead.
        let quit = MenuItemBuilder::with_id("quit", "Quit")
            .accelerator("CmdOrCtrl+Q")
            .build(app)?;

        builder.item(&quit).build()?
    };

    let file_menu = SubmenuBuilder::new(app, "File")
        .close_window()
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let view_menu = {
        let reload = MenuItemBuilder::with_id("reload", "Reload")
            .accelerator("CmdOrCtrl+R")
            .build(app)?;
        let devtools = MenuItemBuilder::with_id("devtools", "Developer Tools")
            .accelerator("CmdOrCtrl+Alt+I")
            .build(app)?;
        let actual_size = MenuItemBuilder::with_id("actual-size", "Actual Size")
            .accelerator("CmdOrCtrl+0")
            .build(app)?;
        let zoom_in = MenuItemBuilder::with_id("zoom-in", "Zoom In")
            .accelerator("CmdOrCtrl+=")
            .build(app)?;
        let zoom_out = MenuItemBuilder::with_id("zoom-out", "Zoom Out")
            .accelerator("CmdOrCtrl+-")
            .build(app)?;

        SubmenuBuilder::new(app, "View")
            .item(&reload)
            .item(&devtools)
            .separator()
            .item(&actual_size)
            .item(&zoom_in)
            .item(&zoom_out)
            .separator()
            .fullscreen()
            .build()?
    };

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;

    let navigate_menu = {
        let back = MenuItemBuilder::with_id("nav-back", "Back")
            .accelerator("CmdOrCtrl+[")
            .build(app)?;
        let forward = MenuItemBuilder::with_id("nav-forward", "Forward")
            .accelerator("CmdOrCtrl+]")
            .build(app)?;

        SubmenuBuilder::new(app, "Navigate")
            .item(&back)
            .item(&forward)
            .build()?
    };

    let menu = MenuBuilder::new(app)
        .item(&app_menu)
        .item(&file_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&navigate_menu)
        .item(&window_menu)
        .build()?;

    Ok(menu)
}

/// Ask the main window's page to move.
fn navigate_main<R: Runtime>(app: &tauri::AppHandle<R>, action: &str) {
    if let Some(window) = app.get_webview_window("main") {
        crate::window::deliver_to_page(
            &window,
            "navigate",
            &serde_json::json!({ "action": action }),
        );
    }
}

/// Handle menu item clicks.
/// Called from the main event loop when a menu event fires.
pub fn handle_menu_event<R: Runtime>(app: &tauri::AppHandle<R>, event_id: &str) {
    log::debug!("Menu event: {}", event_id);

    match event_id {
        "quit" => {
            // Goes through Tauri's shutdown so the exit handler runs.
            app.exit(0);
        }
        // Navigation goes through the injected script rather than evaluating
        // statements at it, so the menu, modal dismissal and anything else that
        // moves the page share one path.
        "reload" => navigate_main(app, "reload"),
        "devtools" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("window.__TURBO_DESKTOP__.toggleDevTools()");
            }
        }
        "nav-back" => navigate_main(app, "back"),
        "nav-forward" => navigate_main(app, "forward"),
        "actual-size" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("document.body.style.zoom = '100%'");
            }
        }
        "zoom-in" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval(
                    "document.body.style.zoom = (parseFloat(document.body.style.zoom || 1) + 0.1) * 100 + '%'",
                );
            }
        }
        "zoom-out" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval(
                    "document.body.style.zoom = (parseFloat(document.body.style.zoom || 1) - 0.1) * 100 + '%'",
                );
            }
        }
        _ => {
            log::debug!("Unhandled menu event: {}", event_id);
        }
    }
}
