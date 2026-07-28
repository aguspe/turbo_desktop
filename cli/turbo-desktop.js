#!/usr/bin/env node

/**
 * turbo-desktop CLI
 *
 * Commands:
 *   new    — Create a new Rails app with Turbo Desktop pre-configured
 *   init   — Add desktop support to an existing Rails app
 *   dev    — Start the desktop app in development mode
 *   build  — Build the desktop app for distribution
 */

import { execSync, spawn, spawnSync } from "child_process";
import { existsSync, mkdirSync, writeFileSync, copyFileSync, readFileSync, appendFileSync } from "fs";
import { resolve, dirname, join, basename } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(__dirname, "..");

const COMMANDS = {
  new: cmdNew,
  init: cmdInit,
  dev: cmdDev,
  build: cmdBuild,
  help: cmdHelp,
};

// Only dispatch when run as a program. Importing this file (from the tests, say)
// should expose the helpers without scaffolding anything.
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [, , command, ...args] = process.argv;
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
}

// ─── Commands ────────────────────────────────────────────────────────────────

function cmdNew(args) {
  const { iconPath, rest } = extractIconFlag(args);
  const appName = rest[0];

  if (!appName) {
    console.error("Usage: turbo-desktop new <appname> [--icon <file.png>]");
    console.error("Example: turbo-desktop new myapp --icon ./logo.png");
    process.exit(1);
  }

  if (iconPath && !existsSync(iconPath)) {
    console.error(`Error: icon file not found: ${iconPath}`);
    process.exit(1);
  }

  // Validate app name (no spaces, basic chars only)
  if (!/^[a-zA-Z0-9_-]+$/.test(appName)) {
    console.error("Error: App name must contain only letters, numbers, hyphens, and underscores.");
    process.exit(1);
  }

  const appDir = resolve(appName);

  if (existsSync(appDir)) {
    console.error(`Error: Directory "${appName}" already exists.`);
    process.exit(1);
  }

  // Check prerequisites
  try {
    execSync("rails --version", { stdio: "pipe" });
  } catch {
    console.error("Error: Rails is not installed. Install it with: gem install rails");
    process.exit(1);
  }

  try {
    execSync("rustc --version", { stdio: "pipe" });
  } catch {
    console.error("Error: Rust is not installed. Install it from https://rustup.rs");
    process.exit(1);
  }

  // Step 1: Create the Rails app
  console.log(`\nCreating Rails app: ${appName}...\n`);
  run("rails", ["new", appName, "--skip-jbuilder"], { stdio: "inherit" });

  // Step 2: Add the turbo_desktop-rails gem
  console.log("\nAdding turbo_desktop-rails gem...");
  const gemfilePath = join(appDir, "Gemfile");
  const gemfileContent = readFileSync(gemfilePath, "utf-8");
  if (!gemfileContent.includes("turbo_desktop-rails")) {
    appendFileSync(gemfilePath, '\ngem "turbo_desktop-rails"\n');
  }

  // Step 3: Bundle install
  console.log("\nInstalling gems...\n");
  execSync("bundle install", { cwd: appDir, stdio: "inherit" });

  // Step 4: Run the install generator
  console.log("\nRunning turbo_desktop:install generator...\n");
  execSync("bin/rails generate turbo_desktop:install", { cwd: appDir, stdio: "inherit" });

  // Step 5: Scaffold the desktop shell (forwarding a custom icon if given)
  console.log("\nScaffolding desktop shell...\n");
  cmdInit([appName, ...(iconPath ? ["--icon", iconPath] : [])]);

  console.log(`
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Turbo Desktop app "${appName}" is ready!
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  cd ${appName}

  # Terminal 1: Start Rails
  bin/rails server

  # Terminal 2: Start the desktop app
  cd desktop && npx turbo-desktop dev
`);
}

function cmdInit(args) {
  const { iconPath, rest } = extractIconFlag(args);
  const projectDir = rest[0] || ".";
  const desktopDir = resolve(projectDir, "desktop");

  if (iconPath && !existsSync(iconPath)) {
    console.error(`Error: icon file not found: ${iconPath}`);
    process.exit(1);
  }

  console.log("Initializing Turbo Desktop in", desktopDir);

  if (existsSync(desktopDir)) {
    console.error("Error: desktop/ directory already exists.");
    process.exit(1);
  }

  // Create directory structure
  mkdirSync(desktopDir, { recursive: true });
  mkdirSync(join(desktopDir, "src-tauri", "src"), { recursive: true });
  mkdirSync(join(desktopDir, "src-tauri", "icons"), { recursive: true });
  mkdirSync(join(desktopDir, "src-tauri", "capabilities"), { recursive: true });
  mkdirSync(join(desktopDir, "src"), { recursive: true });

  // Copy Rust source files. This list must cover every `mod` declared in main.rs,
  // otherwise the scaffolded project will not compile.
  const rustFiles = [
    "main.rs",
    "bridge.rs",
    "config.rs",
    "fs_bridge.rs",
    "menu.rs",
    "navigation.rs",
    "process_manager.rs",
    "security.rs",
    "shell_bridge.rs",
    "sudo_bridge.rs",
    "tray.rs",
    "updater_bridge.rs",
    "window.rs",
  ];
  for (const file of rustFiles) {
    copyFileSync(
      join(PACKAGE_ROOT, "src-tauri", "src", file),
      join(desktopDir, "src-tauri", "src", file)
    );
  }

  // Copy icons (the default Turbo Desktop icon set)
  const iconsDir = join(PACKAGE_ROOT, "src-tauri", "icons");
  if (existsSync(iconsDir)) {
    const iconFiles = ["32x32.png", "64x64.png", "128x128.png", "128x128@2x.png", "icon.icns", "icon.ico", "icon.png"];
    for (const file of iconFiles) {
      const src = join(iconsDir, file);
      if (existsSync(src)) {
        copyFileSync(src, join(desktopDir, "src-tauri", "icons", file));
      }
    }
  }

  // Generate a custom icon set from --icon, overwriting the defaults. Best-effort:
  // if tauri-cli isn't installed, keep the default icon and tell the user how to apply theirs.
  if (iconPath) {
    console.log(`\nGenerating app icons from ${iconPath}...`);
    try {
      run("cargo", ["tauri", "icon", iconPath], { cwd: desktopDir, stdio: "inherit" });
    } catch {
      console.warn(
        "\n  Could not generate icons automatically (is tauri-cli installed?).\n" +
        "  Your app keeps the default icon for now. To apply yours later, run:\n" +
        `    cd ${join(projectDir, "desktop")} && cargo tauri icon "${iconPath}"\n`
      );
    }
  }

  // Copy capabilities
  const capFile = join(PACKAGE_ROOT, "src-tauri", "capabilities", "main.json");
  if (existsSync(capFile)) {
    copyFileSync(capFile, join(desktopDir, "src-tauri", "capabilities", "main.json"));
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

  // Copy JS bridge and supporting files
  copyFileSync(
    join(PACKAGE_ROOT, "src", "turbo-desktop.js"),
    join(desktopDir, "src", "turbo-desktop.js")
  );
  copyFileSync(
    join(PACKAGE_ROOT, "src", "index.html"),
    join(desktopDir, "src", "index.html")
  );
  if (existsSync(join(PACKAGE_ROOT, "src", "inspector.js"))) {
    copyFileSync(
      join(PACKAGE_ROOT, "src", "inspector.js"),
      join(desktopDir, "src", "inspector.js")
    );
  }

  // Copy package.json
  copyFileSync(
    join(PACKAGE_ROOT, "package.json"),
    join(desktopDir, "package.json")
  );

  // Create the app config file. The filesystem and sudo bridges start closed —
  // an app widens them by naming the roots and commands it actually needs.
  const config = {
    server_url: "http://localhost:3000",
    app_name: guessAppName(projectDir),
    user_agent: defaultUserAgent(),
    window: {
      width: 1200,
      height: 800,
      min_width: 800,
      min_height: 600,
      resizable: true,
    },
    filesystem: {
      allowed_roots: [],
    },
    sudo: {
      enabled: false,
      allowed_commands: [],
      confirm: true,
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

function cmdDev() {
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
    : defaultBuildTarget();

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
  new <appname> [--icon <file>]   Create a new Rails app with Turbo Desktop
  init [path] [--icon <file>]     Add desktop support to an existing Rails app
  dev                             Start the desktop app in development mode
  build [--target <arch>]         Build for distribution (default: aarch64-apple-darwin)
  help                            Show this help message

Options:
  --icon <file>   Use a custom app icon (square PNG, ideally 1024x1024). Generates
                  all platform icon formats via tauri-cli.

Examples:
  turbo-desktop new myapp                      # Create a new app from scratch
  turbo-desktop new myapp --icon ./logo.png    # ...with a custom icon
  turbo-desktop init .                         # Add desktop to existing Rails app
  turbo-desktop dev                            # Start dev mode
  turbo-desktop build                          # Build for Apple Silicon
  turbo-desktop build --target universal-apple-darwin  # Universal binary
`);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

// Run a command without a shell, so arguments carrying spaces, quotes or
// semicolons stay arguments instead of turning into extra commands.
export function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", ...options });

  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} exited with status ${result.status}`);
  }
  return result;
}

export function packageVersion() {
  const pkg = JSON.parse(readFileSync(join(PACKAGE_ROOT, "package.json"), "utf-8"));
  return pkg.version;
}

export function defaultUserAgent() {
  const os =
    { darwin: "macOS", win32: "Windows", linux: "Linux" }[process.platform] ||
    process.platform;
  const arch =
    { arm64: "aarch64", x64: "x86_64" }[process.arch] || process.arch;

  return `Turbo Desktop/${packageVersion()} (${os}; ${arch})`;
}

// Pull an optional `--icon <file>` flag out of args. Returns the resolved
// absolute icon path (or null) and the remaining positional args.
export function extractIconFlag(args) {
  const i = args.indexOf("--icon");
  if (i === -1) return { iconPath: null, rest: args };

  const value = args[i + 1];
  if (!value || value.startsWith("--")) {
    console.error("Error: --icon requires a path to an image (a square PNG, ideally 1024x1024).");
    process.exit(1);
  }

  const rest = args.filter((_, idx) => idx !== i && idx !== i + 1);
  return { iconPath: resolve(value), rest };
}

export function defaultBuildTarget() {
  const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
  const platform = process.platform;
  if (platform === "darwin") return `${arch}-apple-darwin`;
  if (platform === "win32") return `${arch}-pc-windows-msvc`;
  if (platform === "linux") return `${arch}-unknown-linux-gnu`;
  return `${arch}-apple-darwin`; // fallback
}

export function guessAppName(projectDir) {
  const name = basename(resolve(projectDir)) || "My App";
  return name
    .replace(/[_-]/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}
