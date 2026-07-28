use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager, Runtime,
};

/// Build and register the system tray icon with a context menu.
///
/// The tray provides Show/Hide and Quit actions, and can be updated
/// dynamically via the "tray" bridge component.
pub fn setup_tray<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), tauri::Error> {
    let show_hide = MenuItemBuilder::with_id("tray-show-hide", "Show/Hide")
        .build(app)?;
    let quit = MenuItemBuilder::with_id("tray-quit", "Quit")
        .build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show_hide)
        .separator()
        .item(&quit)
        .build()?;

    let _tray = TrayIconBuilder::new()
        .tooltip("Turbo Desktop")
        .menu(&menu)
        .on_menu_event(move |app, event: tauri::menu::MenuEvent| {
            handle_tray_menu_event(app, event.id().as_ref());
        })
        .build(app)?;

    Ok(())
}

/// Handle tray menu item clicks.
fn handle_tray_menu_event<R: Runtime>(app: &tauri::AppHandle<R>, event_id: &str) {
    match event_id {
        "tray-show-hide" => {
            if let Some(window) = app.get_webview_window("main") {
                if window.is_visible().unwrap_or(false) {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        }
        "tray-quit" => {
            // Not std::process::exit: that skips Tauri's shutdown, leaving child
            // processes running and window preferences unwritten.
            app.exit(0);
        }
        // The menu bar has its own handler; tray events for anything else are not
        // ours to act on.
        other => log::debug!("Unhandled tray event: {}", other),
    }
}
