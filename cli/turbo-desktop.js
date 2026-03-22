#!/usr/bin/env node

/**
 * turbo-desktop CLI
 *
 * Commands:
 *   init   — Add desktop support to an existing Rails app
 *   dev    — Start the desktop app in development mode
 *   build  — Build the desktop app for distribution
 */

import { execSync, spawn } from "child_process";
import { existsSync, mkdirSync, writeFileSync, copyFileSync } from "fs";
import { resolve, dirname, join } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(__dirname, "..");

const [, , command, ...args] = process.argv;

const COMMANDS = {
  init: cmdInit,
  dev: cmdDev,
  build: cmdBuild,
  help: cmdHelp,
};

const handler = COMMANDS[command];
if (!handler) {
  console.error(
    command
      ? `Unknown command: ${command}`
      : "Usage: turbo-desktop <command> [options]"
  );
  cmdHelp();
  process.exit(1);
}

handler(args);

// ─── Commands ────────────────────────────────────────────────────────────────

function cmdInit(args) {
  const projectDir = args[0] || ".";
  const desktopDir = resolve(projectDir, "desktop");

  console.log("Initializing Turbo Desktop in", desktopDir);

  if (existsSync(desktopDir)) {
    console.error("Error: desktop/ directory already exists.");
    process.exit(1);
  }

  // Create directory structure
  mkdirSync(desktopDir, { recursive: true });
  mkdirSync(join(desktopDir, "src-tauri", "src"), { recursive: true });
  mkdirSync(join(desktopDir, "src-tauri", "icons"), { recursive: true });
  mkdirSync(join(desktopDir, "src"), { recursive: true });

  // Copy Rust source files
  const rustFiles = [
    "main.rs",
    "config.rs",
    "navigation.rs",
    "bridge.rs",
    "menu.rs",
    "window.rs",
  ];
  for (const file of rustFiles) {
    copyFileSync(
      join(PACKAGE_ROOT, "src-tauri", "src", file),
      join(desktopDir, "src-tauri", "src", file)
    );
  }

  // Copy Cargo.toml and build.rs
  copyFileSync(
    join(PACKAGE_ROOT, "src-tauri", "Cargo.toml"),
    join(desktopDir, "src-tauri", "Cargo.toml")
  );
  copyFileSync(
    join(PACKAGE_ROOT, "src-tauri", "build.rs"),
    join(desktopDir, "src-tauri", "build.rs")
  );

  // Copy tauri.conf.json
  copyFileSync(
    join(PACKAGE_ROOT, "src-tauri", "tauri.conf.json"),
    join(desktopDir, "src-tauri", "tauri.conf.json")
  );

  // Copy JS bridge
  copyFileSync(
    join(PACKAGE_ROOT, "src", "turbo-desktop.js"),
    join(desktopDir, "src", "turbo-desktop.js")
  );
  copyFileSync(
    join(PACKAGE_ROOT, "src", "index.html"),
    join(desktopDir, "src", "index.html")
  );

  // Copy package.json
  copyFileSync(
    join(PACKAGE_ROOT, "package.json"),
    join(desktopDir, "package.json")
  );

  // Create the app config file
  const config = {
    server_url: "http://localhost:3000",
    app_name: guessAppName(projectDir),
    user_agent: "Turbo Desktop/0.1.0 (macOS; aarch64)",
    window: {
      width: 1200,
      height: 800,
      min_width: 800,
      min_height: 600,
      resizable: true,
    },
  };

  writeFileSync(
    join(desktopDir, "turbo-desktop.config.json"),
    JSON.stringify(config, null, 2)
  );

  console.log(`
Turbo Desktop initialized successfully!

Next steps:
  1. Add the gem to your Gemfile:
     gem 'turbo_desktop-rails', path: '../turbo_desktop/turbo_desktop-rails'

  2. Mount the engine in config/routes.rb:
     mount TurboDesktop::Engine => "/turbo-desktop"

  3. Configure path rules in config/initializers/turbo_desktop.rb:
     TurboDesktop.configure do |config|
       config.path_configuration = {
         rules: [
           { patterns: ["/"], properties: { presentation: "default" } },
           { patterns: ["/new$", "/edit$"], properties: { presentation: "modal" } }
         ]
       }
     end

  4. Start your Rails server:
     rails server

  5. Start the desktop app:
     cd desktop && turbo-desktop dev
`);
}

function cmdDev(args) {
  console.log("Starting Turbo Desktop in development mode...");

  // Check if we're in a desktop/ directory or the project root
  const tauriConf = existsSync("src-tauri/tauri.conf.json")
    ? "."
    : existsSync("desktop/src-tauri/tauri.conf.json")
    ? "desktop"
    : null;

  if (!tauriConf) {
    console.error(
      'Error: Cannot find Tauri config. Run from the desktop/ directory or project root.\n' +
      'Hint: Run "turbo-desktop init" first.'
    );
    process.exit(1);
  }

  const cwd = resolve(tauriConf);

  // Check for Rust
  try {
    execSync("rustc --version", { stdio: "pipe" });
  } catch {
    console.error(
      "Error: Rust is not installed. Install it from https://rustup.rs"
    );
    process.exit(1);
  }

  // Install npm deps if needed
  if (!existsSync(join(cwd, "node_modules"))) {
    console.log("Installing dependencies...");
    execSync("npm install", { cwd, stdio: "inherit" });
  }

  // Run cargo tauri dev
  const child = spawn("npx", ["tauri", "dev"], {
    cwd,
    stdio: "inherit",
    env: {
      ...process.env,
      RUST_LOG: process.env.RUST_LOG || "turbo_desktop=info",
    },
  });

  child.on("exit", (code) => process.exit(code ?? 0));
}

function cmdBuild(args) {
  const target = args.includes("--target")
    ? args[args.indexOf("--target") + 1]
    : "aarch64-apple-darwin";

  console.log(`Building Turbo Desktop for ${target}...`);

  const cwd = existsSync("src-tauri") ? "." : existsSync("desktop/src-tauri") ? "desktop" : null;

  if (!cwd) {
    console.error('Error: Cannot find Tauri config. Run from the desktop/ directory or project root.');
    process.exit(1);
  }

  // Install deps if needed
  if (!existsSync(join(resolve(cwd), "node_modules"))) {
    execSync("npm install", { cwd: resolve(cwd), stdio: "inherit" });
  }

  const child = spawn("npx", ["tauri", "build", "--target", target], {
    cwd: resolve(cwd),
    stdio: "inherit",
    env: {
      ...process.env,
      RUST_LOG: process.env.RUST_LOG || "turbo_desktop=info",
    },
  });

  child.on("exit", (code) => {
    if (code === 0) {
      console.log("\nBuild complete! Check src-tauri/target/release/bundle/");
    }
    process.exit(code ?? 0);
  });
}

function cmdHelp() {
  console.log(`
turbo-desktop — Turbo Native for Desktop

Commands:
  init [path]              Add desktop support to a Rails app
  dev                      Start the desktop app in development mode
  build [--target <arch>]  Build for distribution (default: aarch64-apple-darwin)
  help                     Show this help message

Examples:
  turbo-desktop init .                     # Initialize in current directory
  turbo-desktop dev                        # Start dev mode
  turbo-desktop build                      # Build for Apple Silicon
  turbo-desktop build --target universal-apple-darwin  # Universal binary
`);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function guessAppName(projectDir) {
  const resolved = resolve(projectDir);
  const basename = resolved.split("/").pop() || "My App";
  return basename
    .replace(/[_-]/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}
