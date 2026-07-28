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
    /// Size of the window this rule opens, when it opens one.
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
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

/// A rule with its patterns already compiled.
struct CompiledRule {
    patterns: Vec<regex_lite::Regex>,
    properties: PathProperties,
}

/// Compile every pattern once, reporting the ones that do not parse.
///
/// An unparseable pattern used to be skipped silently on every navigation,
/// which made a typo in the path configuration look like a routing bug.
fn compile(config: &PathConfiguration) -> Vec<CompiledRule> {
    config
        .rules
        .iter()
        .map(|rule| {
            let patterns = rule
                .patterns
                .iter()
                .filter_map(|pattern| match regex_lite::Regex::new(pattern) {
                    Ok(re) => Some(re),
                    Err(e) => {
                        log::warn!(
                            "Path configuration: ignoring invalid pattern '{}': {}",
                            pattern,
                            e
                        );
                        None
                    }
                })
                .collect();

            CompiledRule {
                patterns,
                properties: rule.properties.clone(),
            }
        })
        .collect()
}

/// Thread-safe container for the active path configuration.
///
/// Rules are stored with their patterns already compiled, so matching a path
/// does not recompile every regex.
pub struct PathConfigurationStore {
    compiled: RwLock<Vec<CompiledRule>>,
}

impl PathConfigurationStore {
    pub fn new() -> Self {
        Self {
            compiled: RwLock::new(Vec::new()),
        }
    }

    pub fn set(&self, config: PathConfiguration) {
        *self.compiled.write().unwrap() = compile(&config);
    }

    /// Find the matching rule for a given URL path.
    /// Rules are evaluated in order; the LAST match wins (same as Hotwire Native).
    pub fn properties_for_path(&self, path: &str) -> PathProperties {
        let mut result = PathProperties {
            presentation: Presentation::Default,
            title: None,
            pull_to_refresh_enabled: None,
            width: None,
            height: None,
            context: None,
        };

        for rule in self.compiled.read().unwrap().iter() {
            if rule.patterns.iter().any(|re| re.is_match(path)) {
                result = rule.properties.clone();
            }
        }

        result
    }
}

/// Load path configuration from a remote URL (your Rails server).
///
/// Sends the app's user agent so this request is recognisable to the server as
/// coming from the desktop shell, the same as the requests the webview makes.
pub async fn fetch_path_configuration(
    url: &str,
    user_agent: &str,
) -> Result<PathConfiguration, String> {
    let client = reqwest::Client::builder()
        .user_agent(user_agent)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch path configuration: {}", e))?;

    let config: PathConfiguration = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse path configuration: {}", e))?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(json: &str) -> PathConfigurationStore {
        let config: PathConfiguration =
            serde_json::from_str(json).expect("test configuration should parse");
        let store = PathConfigurationStore::new();
        store.set(config);
        store
    }

    #[test]
    fn defaults_when_nothing_is_configured() {
        let store = PathConfigurationStore::new();
        assert_eq!(
            store.properties_for_path("/anything").presentation,
            Presentation::Default
        );
    }

    #[test]
    fn matches_a_rule_by_pattern() {
        let store = store_with(
            r#"{"rules":[{"patterns":["/new$"],"properties":{"presentation":"modal"}}]}"#,
        );

        assert_eq!(
            store.properties_for_path("/posts/new").presentation,
            Presentation::Modal
        );
        assert_eq!(
            store.properties_for_path("/posts").presentation,
            Presentation::Default
        );
    }

    #[test]
    fn the_last_matching_rule_wins() {
        let store = store_with(
            r#"{"rules":[
                {"patterns":["/settings"],"properties":{"presentation":"modal"}},
                {"patterns":["/settings"],"properties":{"presentation":"native"}}
            ]}"#,
        );

        assert_eq!(
            store.properties_for_path("/settings").presentation,
            Presentation::Native
        );
    }

    #[test]
    fn an_invalid_pattern_does_not_disable_the_rest_of_the_rule() {
        let store = store_with(
            r#"{"rules":[{"patterns":["[unclosed","/new$"],"properties":{"presentation":"modal"}}]}"#,
        );

        assert_eq!(
            store.properties_for_path("/posts/new").presentation,
            Presentation::Modal
        );
    }

    #[test]
    fn replacing_the_configuration_replaces_the_compiled_patterns() {
        let store = store_with(
            r#"{"rules":[{"patterns":["/new$"],"properties":{"presentation":"modal"}}]}"#,
        );
        assert_eq!(
            store.properties_for_path("/posts/new").presentation,
            Presentation::Modal
        );

        store.set(
            serde_json::from_str(
                r#"{"rules":[{"patterns":["/edit$"],"properties":{"presentation":"native"}}]}"#,
            )
            .unwrap(),
        );

        assert_eq!(
            store.properties_for_path("/posts/new").presentation,
            Presentation::Default
        );
        assert_eq!(
            store.properties_for_path("/posts/edit").presentation,
            Presentation::Native
        );
    }
}
