use crate::bridge::BridgeMessage;
use tauri_plugin_updater::UpdaterExt;

/// Handle bridge messages for the "updater" component.
///
/// Provides update checking and installation from the web layer.
///
/// Events:
///   - "check": Check if an update is available
///   - "download-and-install": Download and install the available update
pub async fn handle_updater(
    app: &tauri::AppHandle,
    message: &BridgeMessage,
) -> Result<serde_json::Value, String> {
    match message.event.as_str() {
        "check" => handle_check(app).await,
        "download-and-install" => handle_download_and_install(app).await,
        _ => Ok(serde_json::json!({ "status": "unknown_event" })),
    }
}

async fn handle_check(app: &tauri::AppHandle) -> Result<serde_json::Value, String> {
    let updater = app
        .updater_builder()
        .build()
        .map_err(|e| format!("Updater not available: {}", e))?;

    match updater.check().await {
        Ok(Some(update)) => {
            log::info!("Updater: update available — v{}", update.version);
            Ok(serde_json::json!({
                "status": "available",
                "version": update.version,
                "date": update.date.map(|d| d.to_string()),
                "body": update.body,
                "current_version": update.current_version,
            }))
        }
        Ok(None) => {
            log::info!("Updater: no update available");
            Ok(serde_json::json!({ "status": "up_to_date" }))
        }
        Err(e) => {
            log::warn!("Updater: check failed — {}", e);
            Ok(serde_json::json!({ "status": "error", "error": e.to_string() }))
        }
    }
}

async fn handle_download_and_install(app: &tauri::AppHandle) -> Result<serde_json::Value, String> {
    let updater = app
        .updater_builder()
        .build()
        .map_err(|e| format!("Updater not available: {}", e))?;

    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return Ok(serde_json::json!({ "status": "up_to_date" })),
        Err(e) => return Ok(serde_json::json!({ "status": "error", "error": e.to_string() })),
    };

    let version = update.version.clone();

    // Download and install — this may restart the app
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => {
            log::info!("Updater: installed v{}", version);
            Ok(serde_json::json!({
                "status": "installed",
                "version": version,
            }))
        }
        Err(e) => {
            log::warn!("Updater: install failed — {}", e);
            Ok(serde_json::json!({ "status": "error", "error": e.to_string() }))
        }
    }
}
