use serde::{Deserialize, Serialize};
use std::sync::RwLock;

/// Path Configuration — the core routing mechanism.
///
/// This mirrors the path configuration pattern from Hotwire Native (turbo-ios/turbo-android).
/// A JSON file (served by the Rails app or bundled locally) maps URL path patterns to
/// presentation rules that the native shell uses to decide HOW to display each page.
///
/// Example path configuration JSON:
/// ```json
/// {
///   "settings": {
///     "screenshots_enabled": true
///   },
///   "rules": [
///     { "patterns": ["/"], "properties": { "presentation": "default" } },
///     { "patterns": ["/new$", "/edit$"], "properties": { "presentation": "modal" } },
///     { "patterns": ["/settings"], "properties": { "presentation": "native" } }
///   ]
/// }
/// ```

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathConfiguration {
    #[serde(default)]
    pub settings: PathSettings,
    pub rules: Vec<PathRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathSettings {
    #[serde(default)]
    pub screenshots_enabled: bool,
    #[serde(default)]
    pub pull_to_refresh_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRule {
    pub patterns: Vec<String>,
    pub properties: PathProperties,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathProperties {
    #[serde(default = "default_presentation")]
    pub presentation: Presentation,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pull_to_refresh_enabled: Option<bool>,
    /// Arbitrary context passed to bridge components
    #[serde(default)]
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Presentation {
    /// Navigate in the current window (default)
    Default,
    /// Open in a modal/sheet window
    Modal,
    /// Open in a new separate window
    NewWindow,
    /// Replace the current page (no back navigation)
    Replace,
    /// Route to a fully native screen
    Native,
    /// Do nothing (handled by bridge component)
    None,
}

fn default_presentation() -> Presentation {
    Presentation::Default
}

/// Thread-safe container for the active path configuration.
pub struct PathConfigurationStore {
    config: RwLock<Option<PathConfiguration>>,
}

impl PathConfigurationStore {
    pub fn new() -> Self {
        Self {
            config: RwLock::new(None),
        }
    }

    pub fn set(&self, config: PathConfiguration) {
        let mut store = self.config.write().unwrap();
        *store = Some(config);
    }

    pub fn get(&self) -> Option<PathConfiguration> {
        let store = self.config.read().unwrap();
        store.clone()
    }

    /// Find the matching rule for a given URL path.
    /// Rules are evaluated in order; the LAST match wins (same as Hotwire Native).
    pub fn properties_for_path(&self, path: &str) -> PathProperties {
        let store = self.config.read().unwrap();
        let mut result = PathProperties {
            presentation: Presentation::Default,
            title: None,
            pull_to_refresh_enabled: None,
            context: None,
        };

        if let Some(config) = store.as_ref() {
            for rule in &config.rules {
                for pattern in &rule.patterns {
                    if let Ok(re) = regex_lite::Regex::new(pattern) {
                        if re.is_match(path) {
                            result = rule.properties.clone();
                        }
                    }
                }
            }
        }

        result
    }
}

/// Load path configuration from a remote URL (your Rails server).
pub async fn fetch_path_configuration(url: &str) -> Result<PathConfiguration, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to fetch path configuration: {}", e))?;

    let config: PathConfiguration = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse path configuration: {}", e))?;

    Ok(config)
}

/// Load path configuration from a local JSON file.
pub fn load_path_configuration_from_file(path: &str) -> Result<PathConfiguration, String> {
    let contents =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let config: PathConfiguration =
        serde_json::from_str(&contents).map_err(|e| format!("Failed to parse JSON: {}", e))?;

    Ok(config)
}
