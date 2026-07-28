use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    /// Where links are allowed to open
    #[serde(default)]
    pub navigation: NavigationConfig,
}

/// Which links stay in the app and which are handed to the browser.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NavigationConfig {
    /// Hosts other than the app's own that may load in the app window.
    ///
    /// Anything else goes to the system browser, matching how Hotwire Native
    /// treats off-origin links. The usual reason to add one is an identity
    /// provider: an OAuth round trip has to happen in this webview for the
    /// session cookie to land in the right place.
    ///
    /// Loading in the app window is not the same as being trusted. The bridge
    /// still answers only the app's own origin, so a host listed here can render
    /// but cannot reach the shell.
    #[serde(default)]
    pub internal_hosts: Vec<String>,
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
        navigation: NavigationConfig::default(),
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

/// Name of the user-writable preferences file.
pub const PREFERENCES_FILENAME: &str = "preferences.json";

/// Settings the person using the app may change, stored in their own config
/// directory.
///
/// Deliberately separate from [`TurboDesktopConfig`] rather than a partial copy
/// of it. This file sits in a directory any process running as the user can
/// write, so the type is kept unable to express anything but window geometry —
/// there is no field here that could widen the sudo allowlist, add a filesystem
/// root, or move `server_url` and with it the origin the bridge trusts. Unknown
/// keys are ignored, so adding a `"sudo"` block to this file does nothing.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Preferences {
    #[serde(default)]
    pub window: Option<WindowPreferences>,
}

/// Remembered window geometry.
///
/// Size only, not position: a remembered position becomes an off-screen window
/// as soon as the display arrangement changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowPreferences {
    pub width: f64,
    pub height: f64,
}

/// Fall back to `fallback` for anything that is not a usable dimension.
///
/// The file is hand-editable, so it can hold a negative number, a NaN, or a
/// value below the app's minimum — none of which should produce a window the
/// user cannot recover.
fn usable_dimension(value: f64, min: f64, fallback: f64) -> f64 {
    if value.is_finite() && value >= min {
        value
    } else {
        fallback
    }
}

impl Preferences {
    /// Window size to open with, given the app's configured defaults.
    pub fn window_size(&self, config: &WindowConfig) -> (f64, f64) {
        match &self.window {
            Some(window) => (
                usable_dimension(window.width, config.min_width, config.width),
                usable_dimension(window.height, config.min_height, config.height),
            ),
            None => (config.width, config.height),
        }
    }
}

/// The main window's most recent size, in logical units.
///
/// Kept in memory and updated as the window resizes, because the size has to be
/// written at a point where the window itself may already be gone: the macOS
/// Quit item goes straight to Cocoa's `terminate:`, so the app can be on its way
/// out before anything gets a chance to measure the window.
#[derive(Default)]
pub struct LastWindowSize(std::sync::Mutex<Option<(f64, f64)>>);

impl LastWindowSize {
    pub fn set(&self, width: f64, height: f64) {
        if let Ok(mut size) = self.0.lock() {
            *size = Some((width, height));
        }
    }

    pub fn get(&self) -> Option<(f64, f64)> {
        self.0.lock().ok().and_then(|size| *size)
    }
}

/// Read preferences, treating any problem as "no preferences yet".
///
/// Unlike the app config, a broken file here is not fatal. It holds recoverable
/// user state, and refusing to start because someone corrupted their remembered
/// window size would be a worse failure than forgetting it.
pub fn load_preferences(dir: Option<&Path>) -> Preferences {
    let Some(path) = dir.map(|d| d.join(PREFERENCES_FILENAME)) else {
        return Preferences::default();
    };

    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Preferences::default();
    };

    match serde_json::from_str(&contents) {
        Ok(preferences) => preferences,
        Err(e) => {
            log::warn!("Ignoring unreadable {}: {}", path.display(), e);
            Preferences::default()
        }
    }
}

pub fn save_preferences(dir: &Path, preferences: &Preferences) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Could not create {}: {}", dir.display(), e))?;

    let contents = serde_json::to_string_pretty(preferences)
        .map_err(|e| format!("Could not serialize preferences: {}", e))?;

    std::fs::write(dir.join(PREFERENCES_FILENAME), contents)
        .map_err(|e| format!("Could not write preferences: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn no_preferences_means_the_configured_size() {
        let config = WindowConfig::default();
        let (width, height) = Preferences::default().window_size(&config);

        assert_eq!((width, height), (config.width, config.height));
    }

    #[test]
    fn a_remembered_size_is_used() {
        let preferences = Preferences {
            window: Some(WindowPreferences {
                width: 1440.0,
                height: 900.0,
            }),
        };

        assert_eq!(
            preferences.window_size(&WindowConfig::default()),
            (1440.0, 900.0)
        );
    }

    #[test]
    fn an_unusable_remembered_size_falls_back() {
        let config = WindowConfig::default();

        for (width, height) in [
            (0.0, 0.0),
            (-1200.0, -800.0),
            (f64::NAN, f64::NAN),
            (f64::INFINITY, f64::INFINITY),
            // Below the app's own minimum.
            (config.min_width - 1.0, config.min_height - 1.0),
        ] {
            let preferences = Preferences {
                window: Some(WindowPreferences { width, height }),
            };

            assert_eq!(
                preferences.window_size(&config),
                (config.width, config.height),
                "({width}, {height}) should not produce a window the user cannot use"
            );
        }
    }

    #[test]
    fn preferences_cannot_carry_policy() {
        // Someone editing their own preferences file cannot grant the app
        // anything: the type has nowhere to put these keys, so they are dropped.
        let preferences: Preferences = serde_json::from_str(
            r#"{
                "window": { "width": 1000, "height": 700 },
                "sudo": { "enabled": true, "allowed_commands": ["rm -rf /"] },
                "filesystem": { "allowed_roots": ["/"] },
                "server_url": "https://evil.example.com"
            }"#,
        )
        .expect("unknown keys should be ignored, not rejected");

        assert_eq!(
            preferences,
            Preferences {
                window: Some(WindowPreferences {
                    width: 1000.0,
                    height: 700.0
                })
            }
        );
    }

    #[test]
    fn unreadable_preferences_are_ignored_rather_than_fatal() {
        let dir = scratch("prefs-malformed");
        std::fs::write(dir.join(PREFERENCES_FILENAME), "{ not json").unwrap();

        assert_eq!(load_preferences(Some(&dir)), Preferences::default());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn preferences_round_trip_through_the_config_directory() {
        let dir = scratch("prefs-roundtrip").join("nested");
        let preferences = Preferences {
            window: Some(WindowPreferences {
                width: 1024.0,
                height: 768.0,
            }),
        };

        // The directory does not exist yet — saving should create it.
        save_preferences(&dir, &preferences).expect("preferences should save");
        assert_eq!(load_preferences(Some(&dir)), preferences);

        std::fs::remove_dir_all(dir.parent().unwrap()).ok();
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

/// Apply the settings every webview in the app should carry.
///
/// Modal and secondary windows used to be built bare, so they were missing the
/// user agent the Rails gem detects on, the external-link handling, and the
/// globals the injected script reads. Anything opening a webview goes through
/// here so they cannot drift apart again.
pub fn apply_shell_defaults<'a, M: tauri::Manager<tauri::Wry>>(
    builder: tauri::webview::WebviewWindowBuilder<'a, tauri::Wry, M>,
    app: &tauri::AppHandle,
    config: &TurboDesktopConfig,
    label: &str,
) -> tauri::webview::WebviewWindowBuilder<'a, tauri::Wry, M> {
    let server_url = config.server_url.clone();
    let internal_hosts = config.navigation.internal_hosts.clone();

    let navigation_app = app.clone();
    let navigation_server = server_url.clone();
    let navigation_hosts = internal_hosts.clone();

    let new_window_app = app.clone();

    // Globals the injected script and the error page read. The label lets a
    // window ask to be closed without the page having to be told which it is.
    let globals = format!(
        "window.__TURBO_DESKTOP_SERVER_URL__ = {};\nwindow.__TURBO_DESKTOP_WINDOW_LABEL__ = {};",
        serde_json::to_string(&server_url).unwrap_or_else(|_| "null".into()),
        serde_json::to_string(label).unwrap_or_else(|_| "null".into()),
    );

    builder
        .user_agent(&config.user_agent)
        .initialization_script(&globals)
        .on_navigation(move |url| {
            match crate::security::destination_for(&navigation_server, &navigation_hosts, url) {
                crate::security::LinkDestination::App => true,
                crate::security::LinkDestination::SystemBrowser => {
                    crate::open_externally(&navigation_app, url);
                    false
                }
            }
        })
        .on_new_window(move |url, _features| {
            match crate::security::destination_for(&server_url, &internal_hosts, &url) {
                crate::security::LinkDestination::App => tauri::webview::NewWindowResponse::Allow,
                crate::security::LinkDestination::SystemBrowser => {
                    crate::open_externally(&new_window_app, &url);
                    tauri::webview::NewWindowResponse::Deny
                }
            }
        })
}

/// Hand a message to the injected script in a webview.
///
/// Tauri's event API would be the obvious route, but reaching it from the page
/// needs `withGlobalTauri`, which exposes the whole JS API to whatever the
/// webview has loaded. Calling into our own injected object instead keeps the
/// surface to one function, and works without the frontend bundling anything.
///
/// Both arguments are JSON-encoded, so neither can break out of the call.
pub fn deliver_to_page<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
    kind: &str,
    payload: &serde_json::Value,
) {
    let js = format!(
        "window.__TURBO_DESKTOP__ && window.__TURBO_DESKTOP__.__receive({}, {})",
        serde_json::to_string(kind).unwrap_or_else(|_| "null".into()),
        serde_json::to_string(payload).unwrap_or_else(|_| "null".into()),
    );

    if let Err(e) = window.eval(&js) {
        log::debug!("Could not deliver '{}' to {}: {}", kind, window.label(), e);
    }
}

/// Deliver to every open webview.
pub fn deliver_to_all<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    kind: &str,
    payload: &serde_json::Value,
) {
    use tauri::Manager;

    for window in app.webview_windows().values() {
        deliver_to_page(window, kind, payload);
    }
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
