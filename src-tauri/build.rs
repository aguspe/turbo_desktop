use std::path::Path;

/// Default configuration written when a build finds none.
///
/// The bundle lists turbo-desktop.config.json as a resource, and the packaged app
/// refuses to start without it. The file is per-project and git-ignored, so a
/// fresh clone or a CI checkout would otherwise fail to build. Writing a default
/// keeps those builds working and gives the resulting app sensible settings:
/// localhost, and both the filesystem and sudo bridges closed.
const DEFAULT_CONFIG: &str = r#"{
  "server_url": "http://localhost:3000",
  "app_name": "Turbo Desktop",
  "window": {
    "width": 1200,
    "height": 800,
    "min_width": 800,
    "min_height": 600,
    "resizable": true
  },
  "filesystem": {
    "allowed_roots": []
  },
  "sudo": {
    "enabled": false,
    "allowed_commands": [],
    "confirm": true
  },
  "navigation": {
    "internal_hosts": []
  }
}
"#;

/// Commands the web layer is allowed to call.
///
/// Tauri's ACL covers app-defined commands as well as plugin ones. Local pages
/// get them for free, but content loaded over the network — which is the entire
/// point of this shell — is refused with "not allowed. Plugin not found" unless
/// each command has a permission and the capability grants it. Declaring them
/// here generates `allow-<command>`; capabilities/main.json chooses which to
/// hand out, and each command still checks the calling origin itself.
const APP_COMMANDS: &[&str] = &[
    "handle_visit_proposal",
    "update_window_title",
    "page_loaded",
    "page_loading",
    "close_modal",
    "dismiss_modal",
    "handle_bridge_message",
    "send_bridge_response",
    "retry_connection",
    "get_window_info",
];

/// Path configuration bundled with the app.
///
/// The server's copy replaces this once it answers, and the last one it gave is
/// cached for the launch after that. This is what the app routes by before
/// either exists — on first run, or on a cold start with the server down.
const DEFAULT_PATH_CONFIG: &str = r#"{
  "rules": [
    { "patterns": ["/"], "properties": { "presentation": "default" } }
  ]
}
"#;

fn write_if_absent(path: &Path, contents: &str, what: &str) {
    if path.exists() {
        return;
    }

    std::fs::write(path, contents).unwrap_or_else(|e| panic!("could not write {what}: {e}"));
    println!("cargo:warning=No {what} found; wrote a default one.");
}

fn main() {
    // Relative to src-tauri/, matching the resource paths in tauri.conf.json.
    let config = Path::new("../turbo-desktop.config.json");
    let path_config = Path::new("../path-configuration.json");

    write_if_absent(config, DEFAULT_CONFIG, "turbo-desktop.config.json");
    write_if_absent(path_config, DEFAULT_PATH_CONFIG, "path-configuration.json");

    println!("cargo:rerun-if-changed=../turbo-desktop.config.json");
    println!("cargo:rerun-if-changed=../path-configuration.json");

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
