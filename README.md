<p align="center">
  <img src="turbo-desktop-icon.png" alt="Turbo Desktop" width="180" />
</p>

<h1 align="center">Turbo Desktop</h1>

<p align="center">
  <strong>Turbo Native for Desktop</strong> — wrap your Rails app in a native macOS / Windows / Linux shell
</p>

<p align="center">
  <strong>🌐 Official site: <a href="https://turbo-desktop.dev/">turbo-desktop.dev</a></strong>
</p>

<p align="center">
  <a href="https://turbo-desktop.dev/">Website</a> •
  <a href="#features">Features</a> •
  <a href="#architecture">Architecture</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#path-configuration">Path Config</a> •
  <a href="#bridge-components">Bridge</a> •
  <a href="#rails-gem">Rails Gem</a> •
  <a href="#comparison">Comparison</a> •
  <a href="https://aguspe.github.io/turbo_desktop/">Docs</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Tauri-2.0-blue?logo=tauri" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/Rust-stable-orange?logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Rails-7+-red?logo=rubyonrails" alt="Rails" />
  <img src="https://img.shields.io/badge/Hotwire-Turbo_Drive-yellow" alt="Hotwire" />
  <img src="https://img.shields.io/badge/License-MIT-green" alt="MIT License" />
</p>

---

## The Problem

Rails developers already have **Hotwire Native** (`turbo-ios` and `turbo-android`) to wrap their web apps in native mobile shells. But there has been *nothing* for desktop.

**Turbo Desktop** fills this gap. It gives you a thin, native desktop shell powered by [Tauri 2](https://tauri.app) that treats your Rails app as the single source of truth — the same pattern you already know from Hotwire Native, but for the desktop.

## Example App

Here's what a Rails app looks like running inside Turbo Desktop (from the [example Task Manager app](https://github.com/aguspe/turbo_desktop_example_app)):

<p align="center">
  <img src="docs/screenshots/dashboard.png" alt="Dashboard — desktop features banner, stats, recent tasks" width="700" />
</p>

## Features

- **No new UI framework** — your existing Rails views, Turbo Frames, and Stimulus controllers just work
- **Native when you need it** — notifications, file pickers, menus, and keyboard shortcuts via Bridge Components
- **Tiny binary** — Tauri uses the OS WebView, no bundled Chromium. Ship a ~5-10 MB app
- **Path configuration** — JSON-based routing rules (same concept as turbo-ios / turbo-android)
- **Bridge components** — web-to-native communication via Stimulus controllers
- **Rails gem** — `turbo_desktop-rails` gives your Rails app desktop shell awareness
- **CLI scaffolding** — `npx turbo-desktop new myapp` to get started fast

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────────┐
│ Rails Server │ ──▶ │   WebView    │ ──▶ │  Tauri / Rust    │
│ HTML + Turbo │     │ turbo-       │     │  Windows, menus, │
│   Drive      │     │ desktop.js   │     │  OS APIs         │
└──────────────┘     └──────────────┘     └──────────────────┘
```

Three layers that mirror the Hotwire Native pattern:

1. **Rails Server** — your existing app serves HTML with Turbo Drive
2. **WebView** — `turbo-desktop.js` intercepts Turbo visits and bridges to native
3. **Tauri Shell** — Rust handles window management, path config routing, and OS APIs

## Quick Start

### 1. Clone and install dependencies

```bash
git clone https://github.com/aguspe/turbo_desktop.git
cd turbo_desktop
cargo install tauri-cli
npm install
```

### 2. Configure your Rails server URL

Edit `turbo-desktop.config.json`:

```json
{
  "server_url": "http://localhost:3000",
  "app_name": "My App",
  "path_configuration_url": "http://localhost:3000/turbo-desktop/path-configuration.json"
}
```

> `path_configuration_url` is optional — it defaults to
> `{server_url}/turbo-desktop/path-configuration.json`.

`server_url` is also the app's trust boundary: the bridge only answers calls from
pages on that exact origin (scheme, host and port). A page from anywhere else —
an off-site link, a redirect, an embedded frame — gets a refusal instead of
native access. See [Bridge security](#bridge-security).

The window is created from this file at startup, so `app_name`, `user_agent` and
the `window` block all take effect. `user_agent` **replaces** the webview's own
string rather than extending it, so keep the `Turbo Desktop` token — the Rails
gem's `turbo_desktop_app?` and the `turbo_desktop_only` helper match on it.

#### Where the config is read from

This file carries the app's trust boundary, so where it is read from matters:

- **In development**, it is read from the project you run in — the working
  directory or one level up, so both `turbo-desktop dev` and `cargo tauri dev`
  find it. If there is none, the app starts on defaults.
- **In a packaged app**, it is read only from inside the bundle
  (`Contents/Resources` on macOS), never from the working directory, and the app
  **refuses to start** if it is missing. It ships there via `bundle.resources` in
  `tauri.conf.json`, and `turbo-desktop build` includes it automatically.

A config that exists but does not parse is always fatal, in both cases.

#### User preferences

The window size the user leaves the app at is remembered separately, in their own
config directory (`~/Library/Application Support/<bundle id>/preferences.json` on
macOS), and reapplied on the next launch:

```json
{ "window": { "width": 1440, "height": 900 } }
```

That file is the only user-writable input the app reads, and it can hold nothing
but geometry. Adding a `sudo` or `server_url` key to it has no effect — the type
it deserializes into has nowhere to put them. Sizes that would produce an
unusable window (below the configured minimum, negative, not a number) fall back
to the configured defaults, and a corrupt file is ignored rather than fatal,
since losing a remembered window size should not stop the app from starting.

Only size is remembered, not position: a remembered position becomes an
off-screen window as soon as the display arrangement changes.

The reason for the split is that a writable config is a way around every other
protection here: `server_url` decides which origin the bridge trusts, and the
filesystem roots and sudo allowlist sit in the same file. Reading it from the
working directory of a shipped app would let anyone who can write a file next to
it grant themselves shell and sudo access. Note that the bundle only becomes
tamper-*resistant* once you sign the app — see
[Signing & notarization](#distribution).

If the server is not reachable when the app launches, it opens a bundled page
that waits and redirects once your server answers.

### 3. Add the Rails gem

```ruby
# Gemfile
gem "turbo_desktop-rails"
```

```bash
bundle install
rails generate turbo_desktop:install
```

### 4. Serve path configuration from Rails

```ruby
# config/routes.rb
get "/turbo-desktop/path-configuration", to: "turbo_desktop#path_configuration"
```

### 5. Run the desktop app

```bash
# Start your Rails server
bin/rails server

# Start the Tauri desktop app
cargo tauri dev
```

## Path Configuration

The path configuration is a JSON file that maps URL patterns to presentation rules — the same concept from turbo-ios and turbo-android.

```json
{
  "settings": {
    "screenshots_enabled": true,
    "pull_to_refresh_enabled": false
  },
  "rules": [
    {
      "patterns": ["/"],
      "properties": { "presentation": "default" }
    },
    {
      "patterns": ["/new$", "/edit$"],
      "properties": { "presentation": "modal", "title": "Edit", "width": 640, "height": 480 }
    },
    {
      "patterns": ["/reports/"],
      "properties": { "presentation": "new_window" }
    },
    {
      "patterns": ["/settings"],
      "properties": { "presentation": "native" }
    }
  ]
}
```

| Presentation | Behavior |
|---|---|
| `default` | Navigate in the current window (Turbo Drive handles it) |
| `modal` | Open the URL in a modal-style window (800×600 unless the rule sets `width`/`height`) |
| `new_window` | Open the URL in a full separate window (1200×800) |
| `replace` | Replace the current page with no back-navigation |
| `native` | Emit a `native-screen-requested` event for Rust UI |
| `none` | Do nothing — handled entirely by a Bridge Component |

## Bridge Components

The Bridge is the desktop equivalent of **Strada**. It lets your web components talk to native OS features through structured message passing.

### Built-in Components

| Component | Description |
|---|---|
| `notification` | Show native OS notifications |
| `menu-item` | Register items in the native menu bar |
| `file-picker` | Open native file-open/save dialogs |
| `badge` | Set the dock/taskbar badge count |
| `shortcut` | Register global keyboard shortcuts |

### Modal and secondary windows

A rule with `presentation: "modal"` or `"new_window"` opens the URL in its own
window, sized by the rule's `width` and `height`. These carry everything the
main window does — the user agent your Rails app detects on, off-origin links
going to the browser, and a working bridge.

A page in one of these windows knows where it is and can dismiss itself:

```js
if (TurboDesktop.isModal) {
  TurboDesktop.closeModal()      // no argument: closes the window it is in
}
TurboDesktop.windowLabel         // e.g. "modal-9b8b948"
```

Note these are separate top-level windows rather than sheets attached to the
main one, so they do not block interaction with it.

### External links

Links to anywhere other than your app open in the system browser, the same way
Hotwire Native treats off-origin links. Without that, following a link to a
payment provider or a terms page replaces your app in its own window and leaves
the person with no way back. `mailto:`, `tel:` and other non-web schemes go to
whichever app owns them.

This is decided in the shell rather than in JavaScript, because Turbo only
intercepts same-origin links — an off-origin one never reaches the web layer at
all. Ordinary navigations, `target="_blank"`, `window.open` and path
configuration rules pointing off-origin all go the same way.

Sometimes you need a third-party page *inside* the app: an OAuth round trip has
to happen in this webview for the session cookie to land in the right place.
List those hosts:

```json
{
  "navigation": {
    "internal_hosts": ["accounts.google.com"]
  }
}
```

Matching is exact, so `example.com` does not admit `evil-example.com` or
`sub.example.com`. Being internal is not the same as being trusted: the bridge
still answers only your app's own origin, so a listed host can render but cannot
reach the shell.

### Connection loss and error pages

The shell watches your server and reports failures using the same vocabulary as
Hotwire Native, so `network_failure`, `timeout_failure`, `http_failure` and
`page_load_failure` mean here what they mean on turbo-ios and turbo-android.

**What happens by default.** If your server is unreachable at launch, the window
opens on a bundled error page. If it goes away while the app is running, a
banner appears. Either way the shell keeps probing, and puts the window back on
your app as soon as the server answers — you do not have to do anything.

The shell is what notices this, not the web layer, because the browser's
`offline` event fires when *this machine* loses its network, not when your
server goes down. The second is the case that actually happens.

**Customising the error page.** `desktop/src/error.html` is yours. It is
bundled with your app, so it must work with no network: inline everything, no
CDN fonts or remote stylesheets. It receives the server URL as
`window.__TURBO_DESKTOP_SERVER_URL__` and the reason as an `?error=` parameter.

**Handling failures in your app instead.** Listen for `turbo-desktop:visit-error`
and call `preventDefault()` to suppress the shell's banner for that failure:

```js
document.addEventListener("turbo-desktop:visit-error", (event) => {
  const { error, status, retry } = event.detail
  event.preventDefault()
  showMyOwnBanner(error, status, retry)   // retry() attempts the visit again
})
```

`retry` is the desktop counterpart of the retry handler Hotwire Native passes to
a failed visitable. To take over presentation entirely rather than case by case:

```html
<meta name="turbo-desktop-error-handling" content="manual">
```

There is also `turbo-desktop:connection` with `{ online, error }` for reacting to
the connection dropping and returning without tying it to a specific visit.

Server errors your app can render itself are left alone — a 404 or a 422 is your
page to serve. Only 5xx responses and failures to reach the server at all are
reported.

### Bridge security

The bridge reaches the shell, the filesystem and (on macOS) administrator
privileges, so it is closed by default and opened deliberately.

**Origin.** Every bridge message is checked against `server_url` before it is
dispatched. Only pages served from that origin can use the bridge.

**Filesystem.** The `filesystem` component can only read and write under the
roots you declare. With no configuration it is limited to the app's own data
directory. Paths are resolved before the check, so `..` and symlinks cannot walk
out of a root, and locations like `.ssh`, `.aws`, `.gnupg` and Rails
`master.key` / `credentials.yml.enc` are refused even inside one.

```json
{
  "filesystem": {
    "allowed_roots": ["~/Projects", "~/.rbenv"]
  }
}
```

**Sudo.** The `sudo` component is off unless you enable it and name the commands
it may run. A command is matched whole or as a prefix up to a word boundary, and
anything containing shell metacharacters (`;`, `&&`, `|`, backticks, `$(...)`)
is refused so an allowed prefix cannot be extended into a second command. Before
the system password prompt — which does not say what is about to run, and caches
your credential afterwards — the app shows the exact command and asks.

```json
{
  "sudo": {
    "enabled": true,
    "allowed_commands": ["softwareupdate", "brew install"],
    "confirm": true
  }
}
```

Set `confirm` to `false` only if your app already asks the user itself.

### Dev Inspector

In development, press **Cmd/Ctrl+Shift+D** to open the Dev Inspector — an in-app
overlay that shows:

- **Components** — every available bridge component, with a copy-pasteable
  Rails + Stimulus snippet, and which are active on the current page
- **Messages** — a live log of web↔native bridge traffic
- **Navigation** — the path-configuration presentation applied to the current URL
- **Shell** — platform, arch, version, and server URL

Enable it from the Rails gem (added by the installer in development):

```ruby
# config/initializers/turbo_desktop.rb
config.inspector_enabled = Rails.env.development?
```

```erb
<%# app/views/layouts/application.html.erb, in <head> %>
<%= turbo_desktop_inspector_meta_tag %>
```

Or flip it on against any build without a rebuild:
`localStorage.setItem("td:inspector", "1")`.

### JavaScript Example

```javascript
import { Controller } from "@hotwired/stimulus"

export default class extends TurboDesktop.stimulusBridge(Controller, "notification") {
  connect() {
    super.connect()
    this.sendBridge("connect", { title: "My App" })
  }

  notify(event) {
    this.sendBridge("connect", {
      title: "New Message",
      body: event.target.dataset.body
    })
  }

  receiveBridge(message) {
    console.log("Native says:", message)
  }
}
```

### Rails View Helpers

```erb
<%# Attach bridge data attributes to any element %>
<%= tag.button "Export PDF",
    **turbo_desktop_bridge("menu-item",
      title: "Export PDF",
      shortcut: "Cmd+E"
    ) %>
```

## Rails Gem

The `turbo_desktop-rails` gem gives your Rails app awareness of the desktop shell.

| Helper | Description |
|---|---|
| `turbo_desktop_app?` | Returns `true` if request comes from Turbo Desktop |
| `turbo_desktop_platform` | Returns `"macos"`, `"windows"`, `"linux"`, or `nil` |
| `turbo_desktop_arch` | Returns `"aarch64"`, `"x86_64"`, or `nil` |
| `turbo_desktop_only { }` | Renders block only inside the desktop app |
| `turbo_web_only { }` | Renders block only for web (non-desktop) users |
| `turbo_desktop_bridge(component, **opts)` | Outputs bridge data attributes |

## Comparison

| Concept | turbo-ios | turbo-android | Turbo Desktop |
|---|---|---|---|
| Shell runtime | WKWebView (Swift) | WebView (Kotlin) | Tauri WebView (Rust) |
| Path configuration | JSON, last-match-wins | JSON, last-match-wins | JSON, last-match-wins |
| Bridge / native comms | Strada | Strada | BridgeComponent |
| JS injection | WKUserScript | evaluateJavascript | on_page_load + eval |
| Rails gem | turbo-rails | turbo-rails | turbo_desktop-rails |
| Binary size | System WebKit | ~20 MB | ~5-10 MB |
| Platforms | iOS, iPadOS | Android | macOS, Windows, Linux |

## Custom App Icon

Your app ships with the default Turbo Desktop icon (in `src-tauri/icons/`). To use your own, run
Tauri's icon generator on a single source image — it produces every size and format
(`.png`, macOS `.icns`, Windows `.ico`, and mobile sets):

```bash
npm run tauri icon path/to/your-icon.png
# or:  cargo tauri icon path/to/your-icon.png
```

Use a **square PNG, 1024×1024, with a transparent background**. The generator overwrites
`src-tauri/icons/`, and `tauri.conf.json`'s `bundle.icon` already points at those files — so the next
`cargo tauri build` (or tagged release) uses your icon automatically. No config changes needed.

Prefer to do it by hand? Replace the files in `src-tauri/icons/` listed under `bundle.icon`.

**Starting a new app?** Brand it from the start — the CLI generates your icon during scaffolding:

```bash
npx turbo-desktop new myapp --icon ./logo.png
```

## Distribution

Ship native installers for macOS, Windows, and Linux by pushing a git tag — the
[release workflow](.github/workflows/release.yml) builds each OS and attaches the installers to a
draft GitHub Release:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

See **[docs/DISTRIBUTION.md](docs/DISTRIBUTION.md)** for local builds, using it in your own app,
and the optional signing / auto-update setup.

## Project Structure

```
turbo_desktop/
├── src/                    # JavaScript (turbo-desktop.js)
├── src-tauri/              # Rust / Tauri shell
│   └── src/
│       ├── main.rs         # App entry point
│       ├── security.rs     # Origin, filesystem and sudo policy
│       ├── navigation.rs   # Visit proposals & path config routing
│       ├── bridge.rs       # Bridge dispatch
│       ├── shell_bridge.rs # Process spawning
│       ├── fs_bridge.rs    # Scoped filesystem access
│       ├── sudo_bridge.rs  # Privileged commands
│       ├── config.rs       # Path configuration
│       └── window.rs       # Window management & app config
├── turbo_desktop-rails/    # Rails gem
├── cli/                    # CLI scaffolding tool
├── templates/              # Project templates
├── test/                   # Tests
└── docs/                   # Documentation
```

## License

MIT — see [LICENSE](LICENSE) for details.

---

<p align="center">
  Built with <a href="https://tauri.app">Tauri</a>, <a href="https://hotwired.dev">Hotwire</a>, and <a href="https://rubyonrails.org">Ruby on Rails</a>.
</p>
