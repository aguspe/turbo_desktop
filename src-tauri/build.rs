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

fn main() {
    // Relative to src-tauri/, matching the resource path in tauri.conf.json.
    let config = Path::new("../turbo-desktop.config.json");

    if !config.exists() {
        std::fs::write(config, DEFAULT_CONFIG).expect("could not write a default app config");
        println!("cargo:warning=No turbo-desktop.config.json found; wrote a default one.");
    }

    println!("cargo:rerun-if-changed=../turbo-desktop.config.json");

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
