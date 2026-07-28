use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    /// User-Agent the webview sends. This replaces the browser's own string
    /// rather than extending it, so keep the "Turbo Desktop" token — the Rails
    /// gem's detection matches on it.
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    /// Window configuration
    #[serde(default)]
    pub window: WindowConfig,
    /// What the filesystem bridge component is allowed to touch
    #[serde(default)]
    pub filesystem: FilesystemConfig,
    /// Whether — and which — commands may run with administrator privileges
    #[serde(default)]
    pub sudo: SudoConfig,
}

/// Filesystem bridge policy.
///
/// Defaults to the app data directory only; an app that needs wider access
/// names the roots it needs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilesystemConfig {
    /// Absolute paths (or `~/...`) the bridge may read and write under.
    #[serde(default)]
    pub allowed_roots: Vec<String>,
}

/// Sudo bridge policy. Disabled unless the app opts in and lists commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SudoConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Commands that may be run as administrator. Matched whole, or as a
    /// prefix up to a word boundary (`"brew install"` allows `brew install ruby`).
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    /// Show a dialog naming the command before the system password prompt.
    #[serde(default = "default_true")]
    pub confirm: bool,
}

impl Default for SudoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_commands: Vec::new(),
            confirm: true,
        }
    }
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
    let os = match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    format!(
        "Turbo Desktop/{} ({}; {})",
        env!("CARGO_PKG_VERSION"),
        os,
        std::env::consts::ARCH
    )
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

/// Name of the configuration file, in the project during development and in the
/// bundle's resource directory once the app ships.
pub const CONFIG_FILENAME: &str = "turbo-desktop.config.json";

/// Development defaults, used only when a debug build finds no config at all.
fn default_config() -> TurboDesktopConfig {
    TurboDesktopConfig {
        server_url: "http://localhost:3000".into(),
        path_configuration_url: None,
        app_name: "Turbo Desktop".into(),
        user_agent: default_user_agent(),
        window: WindowConfig::default(),
        filesystem: FilesystemConfig::default(),
        sudo: SudoConfig::default(),
    }
}

pub fn parse_config(contents: &str) -> Result<TurboDesktopConfig, String> {
    serde_json::from_str(contents).map_err(|e| e.to_string())
}

/// A loaded configuration and where it came from.
#[derive(Debug)]
pub struct LoadedConfig {
    pub config: TurboDesktopConfig,
    /// `None` when a development build fell back to defaults.
    pub source: Option<PathBuf>,
}

/// Where to look for the configuration, and how strict to be about finding it.
///
/// The file carries the app's trust boundary — `server_url` decides which origin
/// may call the bridge, and the filesystem and sudo policies live beside it — so a
/// shipped app reads it from inside its own bundle and refuses to start without
/// it. Falling back to defaults there would silently widen or move that boundary.
/// Development builds are looser: they read the project you are running from, and
/// tolerate its absence so a fresh clone still starts.
pub struct ConfigLookup {
    pub working_dir: Option<PathBuf>,
    pub resource_dir: Option<PathBuf>,
    pub development: bool,
}

impl ConfigLookup {
    pub fn for_app<R: tauri::Runtime>(app: &tauri::App<R>) -> Self {
        use tauri::Manager;

        Self {
            working_dir: std::env::current_dir().ok(),
            resource_dir: app.path().resource_dir().ok(),
            development: cfg!(debug_assertions),
        }
    }

    /// Candidate paths, most specific first.
    pub fn search_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if self.development {
            if let Some(dir) = &self.working_dir {
                // The project root, and one level up for `cargo tauri dev`, which
                // runs with src-tauri as the working directory.
                paths.push(dir.join(CONFIG_FILENAME));
                if let Some(parent) = dir.parent() {
                    paths.push(parent.join(CONFIG_FILENAME));
                }
            }
        }

        if let Some(dir) = &self.resource_dir {
            paths.push(dir.join(CONFIG_FILENAME));
        }

        paths
    }

    pub fn load(&self) -> Result<LoadedConfig, String> {
        let paths = self.search_paths();

        for path in &paths {
            let Ok(contents) = std::fs::read_to_string(path) else {
                continue;
            };

            // A config that exists but does not parse is always fatal — silently
            // falling back would hide a typo in the security policy.
            let config = parse_config(&contents)
                .map_err(|e| format!("{} is not valid JSON for this app: {}", path.display(), e))?;

            return Ok(LoadedConfig {
                config,
                source: Some(path.clone()),
            });
        }

        if self.development {
            return Ok(LoadedConfig {
                config: default_config(),
                source: None,
            });
        }

        Err(format!(
            "No {} found. Looked in: {}. A packaged app must ship this file in its \
             resource directory — add it to bundle.resources in tauri.conf.json.",
            CONFIG_FILENAME,
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("turbo-desktop-config-{name}"));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_config(dir: &Path, server_url: &str) {
        std::fs::write(
            dir.join(CONFIG_FILENAME),
            format!(r#"{{"server_url":"{server_url}"}}"#),
        )
        .unwrap();
    }

    #[test]
    fn a_release_build_only_looks_in_the_bundle() {
        let lookup = ConfigLookup {
            working_dir: Some(PathBuf::from("/some/cwd")),
            resource_dir: Some(PathBuf::from("/app/Resources")),
            development: false,
        };

        assert_eq!(
            lookup.search_paths(),
            vec![PathBuf::from("/app/Resources").join(CONFIG_FILENAME)],
            "a packaged app must not read a config from its working directory"
        );
    }

    #[test]
    fn a_development_build_also_looks_beside_and_above_the_working_directory() {
        let lookup = ConfigLookup {
            working_dir: Some(PathBuf::from("/project/src-tauri")),
            resource_dir: Some(PathBuf::from("/app/Resources")),
            development: true,
        };

        assert_eq!(
            lookup.search_paths(),
            vec![
                PathBuf::from("/project/src-tauri").join(CONFIG_FILENAME),
                PathBuf::from("/project").join(CONFIG_FILENAME),
                PathBuf::from("/app/Resources").join(CONFIG_FILENAME),
            ]
        );
    }

    #[test]
    fn a_release_build_refuses_to_start_without_a_config() {
        let dir = scratch("release-missing");
        let lookup = ConfigLookup {
            working_dir: Some(dir.clone()),
            resource_dir: Some(dir.join("empty")),
            development: false,
        };

        let err = lookup
            .load()
            .expect_err("a packaged app should refuse to start with no config");
        assert!(err.contains("No turbo-desktop.config.json"), "got: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_development_build_falls_back_to_defaults() {
        let dir = scratch("dev-missing");
        let lookup = ConfigLookup {
            working_dir: Some(dir.join("empty")),
            resource_dir: None,
            development: true,
        };

        let loaded = lookup.load().expect("development should tolerate no config");
        assert_eq!(loaded.config.server_url, "http://localhost:3000");
        assert!(loaded.source.is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_bundled_config_is_used_when_present() {
        let dir = scratch("release-present");
        write_config(&dir, "https://app.example.com");

        let lookup = ConfigLookup {
            working_dir: None,
            resource_dir: Some(dir.clone()),
            development: false,
        };

        let loaded = lookup.load().expect("the bundled config should load");
        assert_eq!(loaded.config.server_url, "https://app.example.com");
        assert_eq!(loaded.source, Some(dir.join(CONFIG_FILENAME)));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_working_directory_wins_during_development() {
        let dir = scratch("dev-precedence");
        let project = dir.join("project");
        let resources = dir.join("resources");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&resources).unwrap();
        write_config(&project, "http://localhost:4000");
        write_config(&resources, "https://bundled.example.com");

        let lookup = ConfigLookup {
            working_dir: Some(project),
            resource_dir: Some(resources),
            development: true,
        };

        assert_eq!(
            lookup.load().unwrap().config.server_url,
            "http://localhost:4000",
            "the project you are running from should win in development"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_malformed_config_is_fatal_even_in_development() {
        let dir = scratch("malformed");
        std::fs::write(dir.join(CONFIG_FILENAME), "{ not json").unwrap();

        let lookup = ConfigLookup {
            working_dir: Some(dir.clone()),
            resource_dir: None,
            development: true,
        };

        let err = lookup
            .load()
            .expect_err("a config that does not parse must not fall back to defaults");
        assert!(err.contains("not valid JSON"), "got: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_security_policy_defaults_closed_when_a_config_omits_it() {
        let config = parse_config(r#"{"server_url":"https://app.example.com"}"#).unwrap();

        assert!(!config.sudo.enabled);
        assert!(config.sudo.allowed_commands.is_empty());
        assert!(config.filesystem.allowed_roots.is_empty());
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
pub async fn get_window_info(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    window: tauri::Window,
) -> Result<serde_json::Value, String> {
    crate::security::ensure_trusted_caller(&app, &webview)?;

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
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    }))
}
