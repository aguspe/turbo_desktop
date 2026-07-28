use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::RwLock;

/// Path Configuration — the core routing mechanism.
///
/// This mirrors the path configuration pattern from Hotwire Native (turbo-ios/turbo-android).
/// A JSON file (served by the Rails app or bundled locally) maps URL path patterns to
/// presentation rules that the native shell uses to decide HOW to display each page.
///
/// Rules the server does not know about are ignored, so a configuration shared
/// with the mobile shells can carry their settings without upsetting this one.
///
/// Example path configuration JSON:
/// ```json
/// {
///   "rules": [
///     { "patterns": ["/"], "properties": { "presentation": "default" } },
///     { "patterns": ["/new$", "/edit$"], "properties": { "presentation": "modal" } },
///     { "patterns": ["/settings"], "properties": { "presentation": "native" } }
///   ]
/// }
/// ```

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathConfiguration {
    pub rules: Vec<PathRule>,
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

/// Name of the bundled path configuration, and of the cached copy.
pub const PATH_CONFIG_FILENAME: &str = "path-configuration.json";

/// Where the rules in use came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The last copy successfully fetched from the server.
    Cache,
    /// The copy shipped inside the app.
    Bundle,
}

pub fn load_from_file(path: &Path) -> Result<PathConfiguration, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read {}: {}", path.display(), e))?;

    serde_json::from_str(&contents).map_err(|e| format!("{} is not valid: {}", path.display(), e))
}

/// Rules to start with, before the server has been asked.
///
/// Mirrors how Hotwire Native treats path configuration: ship a copy so the app
/// routes correctly on first run, and keep the last one the server gave you so a
/// launch without a server is not a launch without rules. Without this, starting
/// offline silently sent every route to the default presentation — modals and
/// secondary windows quietly stopped happening.
pub fn startup_configuration(
    cache_dir: Option<&Path>,
    resource_dir: Option<&Path>,
) -> Option<(PathConfiguration, Source)> {
    let candidates = [
        (cache_dir, Source::Cache),
        (resource_dir, Source::Bundle),
    ];

    for (dir, source) in candidates {
        let Some(path) = dir.map(|d| d.join(PATH_CONFIG_FILENAME)) else {
            continue;
        };
        if !path.exists() {
            continue;
        }

        match load_from_file(&path) {
            Ok(config) => return Some((config, source)),
            // A damaged cache should not stop the bundled copy being tried.
            Err(e) => log::warn!("Ignoring path configuration: {}", e),
        }
    }

    None
}

/// Keep the copy the server just gave us for the next launch.
pub fn save_cache(dir: &Path, config: &PathConfiguration) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Could not create {}: {}", dir.display(), e))?;

    let contents = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Could not serialize path configuration: {}", e))?;

    std::fs::write(dir.join(PATH_CONFIG_FILENAME), contents)
        .map_err(|e| format!("Could not cache path configuration: {}", e))
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

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("turbo-desktop-pathconfig-{name}"));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_rules(dir: &Path, pattern: &str) {
        std::fs::write(
            dir.join(PATH_CONFIG_FILENAME),
            format!(
                r#"{{"rules":[{{"patterns":["{pattern}"],"properties":{{"presentation":"modal"}}}}]}}"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn a_configuration_shared_with_the_mobile_shells_still_parses() {
        // The Rails gem emits Hotwire Native's settings block so one endpoint can
        // serve every shell. Nothing here uses it, and it must not be an error.
        let config: PathConfiguration = serde_json::from_str(
            r#"{
                "settings": { "screenshots_enabled": true, "pull_to_refresh_enabled": true },
                "rules": [{"patterns": ["/new$"], "properties": {"presentation": "modal"}}]
            }"#,
        )
        .expect("settings the desktop shell ignores must not break parsing");

        assert_eq!(config.rules.len(), 1);
    }

    #[test]
    fn nothing_to_start_from_is_not_an_error() {
        let dir = scratch("startup-empty");
        assert!(startup_configuration(Some(&dir), Some(&dir)).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_bundled_copy_is_used_on_first_run() {
        let dir = scratch("startup-bundle");
        let cache = dir.join("cache");
        let bundle = dir.join("bundle");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        write_rules(&bundle, "/bundled$");

        let (config, source) = startup_configuration(Some(&cache), Some(&bundle)).unwrap();

        assert_eq!(source, Source::Bundle);
        assert_eq!(config.rules[0].patterns, vec!["/bundled$"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn what_the_server_last_said_wins_over_the_bundled_copy() {
        let dir = scratch("startup-cache");
        let cache = dir.join("cache");
        let bundle = dir.join("bundle");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        write_rules(&cache, "/cached$");
        write_rules(&bundle, "/bundled$");

        let (config, source) = startup_configuration(Some(&cache), Some(&bundle)).unwrap();

        assert_eq!(source, Source::Cache);
        assert_eq!(config.rules[0].patterns, vec!["/cached$"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_damaged_cache_falls_back_to_the_bundled_copy() {
        let dir = scratch("startup-damaged");
        let cache = dir.join("cache");
        let bundle = dir.join("bundle");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(cache.join(PATH_CONFIG_FILENAME), "{ not json").unwrap();
        write_rules(&bundle, "/bundled$");

        let (config, source) = startup_configuration(Some(&cache), Some(&bundle)).unwrap();

        assert_eq!(source, Source::Bundle, "a broken cache must not strand the app");
        assert_eq!(config.rules[0].patterns, vec!["/bundled$"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_cached_configuration_survives_the_round_trip() {
        let dir = scratch("cache-roundtrip").join("nested");
        let config: PathConfiguration = serde_json::from_str(
            r#"{"rules":[{"patterns":["/new$"],"properties":{"presentation":"modal"}}]}"#,
        )
        .unwrap();

        save_cache(&dir, &config).expect("the cache directory should be created");

        let (loaded, source) = startup_configuration(Some(&dir), None).unwrap();
        assert_eq!(source, Source::Cache);
        assert_eq!(loaded.rules[0].properties.presentation, Presentation::Modal);

        std::fs::remove_dir_all(dir.parent().unwrap()).ok();
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
