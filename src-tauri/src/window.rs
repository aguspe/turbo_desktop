use serde::{Deserialize, Serialize};

/// Desktop app configuration loaded from turbo-desktop.config.json.
/// This file lives in the Rails project root (or wherever the user runs `turbo-desktop init`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurboDesktopConfig {
    /// The URL of the Rails server to load
    pub server_url: String,
    /// Optional: URL to fetch path configuration from (defaults to {server_url}/turbo-desktop/path-configuration.json)
    #[serde(default)]
    pub path_configuration_url: Option<String>,
    /// Application name shown in the title bar and menu
    #[serde(default = "default_app_name")]
    pub app_name: String,
    /// User-Agent string appended to the WebView's default UA
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    /// Window configuration
    #[serde(default)]
    pub window: WindowConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    #[serde(default = "default_width")]
    pub width: f64,
    #[serde(default = "default_height")]
    pub height: f64,
    #[serde(default = "default_min_width")]
    pub min_width: f64,
    #[serde(default = "default_min_height")]
    pub min_height: f64,
    #[serde(default = "default_true")]
    pub resizable: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            min_width: default_min_width(),
            min_height: default_min_height(),
            resizable: true,
        }
    }
}

fn default_app_name() -> String {
    "Turbo Desktop".into()
}

fn default_user_agent() -> String {
    format!("Turbo Desktop/{} (macOS)", env!("CARGO_PKG_VERSION"))
}

fn default_width() -> f64 {
    1200.0
}
fn default_height() -> f64 {
    800.0
}
fn default_min_width() -> f64 {
    800.0
}
fn default_min_height() -> f64 {
    600.0
}
fn default_true() -> bool {
    true
}

/// Load the turbo-desktop.config.json from the current directory or a specified path.
pub fn load_config(path: Option<&str>) -> Result<TurboDesktopConfig, String> {
    let config_path = path.unwrap_or("turbo-desktop.config.json");

    // Try the given path, then fall back to a bundled default
    if let Ok(contents) = std::fs::read_to_string(config_path) {
        serde_json::from_str(&contents).map_err(|e| format!("Invalid config: {}", e))
    } else {
        // Default config pointing to localhost for development
        Ok(TurboDesktopConfig {
            server_url: "http://localhost:3000".into(),
            path_configuration_url: None,
            app_name: "Turbo Desktop".into(),
            user_agent: default_user_agent(),
            window: WindowConfig::default(),
        })
    }
}

/// Get the effective path configuration URL.
pub fn path_config_url(config: &TurboDesktopConfig) -> String {
    config
        .path_configuration_url
        .clone()
        .unwrap_or_else(|| format!("{}/turbo-desktop/path-configuration.json", config.server_url))
}

/// Get information about the current window state.
#[tauri::command]
pub async fn get_window_info(window: tauri::Window) -> Result<serde_json::Value, String> {
    let size = window.inner_size().map_err(|e| format!("{}", e))?;
    let position = window.outer_position().map_err(|e| format!("{}", e))?;
    let scale = window.scale_factor().map_err(|e| format!("{}", e))?;
    let is_fullscreen = window.is_fullscreen().map_err(|e| format!("{}", e))?;
    let is_maximized = window.is_maximized().map_err(|e| format!("{}", e))?;
    let label = window.label().to_string();

    Ok(serde_json::json!({
        "label": label,
        "width": size.width,
        "height": size.height,
        "x": position.x,
        "y": position.y,
        "scaleFactor": scale,
        "isFullscreen": is_fullscreen,
        "isMaximized": is_maximized,
        "platform": "macos",
        "arch": std::env::consts::ARCH,
    }))
}
