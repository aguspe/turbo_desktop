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
  }
}
"#;

fn main() {
    // Relative to src-tauri/, matching the resource path in tauri.conf.json.
    let config = Path::new("../turbo-desktop.config.json");

    if !config.exists() {
        std::fs::write(config, DEFAULT_CONFIG).expect("could not write a default app config");
        println!("cargo:warning=No turbo-desktop.config.json found; wrote a default one.");
    }

    println!("cargo:rerun-if-changed=../turbo-desktop.config.json");

    tauri_build::build()
}
