# Changelog

## 0.2.4 (2026-09-28)

Version aligned with the desktop shell's 0.2.4 release. In the gem, the Dev
Inspector's scripts are served so that an upgrade takes effect at once. What
changed in the shell:

### Added

- `turbo_desktop_modal?` and `turbo_desktop_presentation`: how the shell will
  present the page being rendered, from the path configuration, so that a
  layout can leave the navigation out of a modal.

### Fixed

- Opening the app starts the Rails server. `tauri dev` used to wait for the
  server before starting the shell that starts it, and gave up after three
  minutes unless a server was already running.
- The server starts on the project's Ruby. Commands ran through a login shell,
  which does not read `~/.zshrc` or `~/.bashrc`, where rbenv, asdf, nvm and
  mise install themselves, so they found the system's Ruby.
- Path configuration rules are fetched again when the server answers. An app
  that starts its own server finds it down at launch, so the rules never
  arrived and modals did not open.
- A rule that opens a modal, a new window or a native screen no longer also
  navigates the main window to the same URL.
- Quitting the app stops the server it started. The shell asked it to stop and
  left before it had.
- Ctrl+C on `turbo-desktop dev`, or a `kill`, quits the app properly rather
  than ending it where it stands and leaving the server behind.
- The `notification`, `badge`, `menu-item` and `shortcut` components do what
  they say. They answered "ok" and did nothing: no notification appeared, no
  badge was set, no menu item or shortcut was registered.
- `TurboDesktop` is there before the page's own scripts run. It used to arrive
  after the page had loaded, so a Stimulus controller that used it in
  `connect()` found nothing on the first page of every window.
  `turbo-desktop:ready` is dispatched once the document has loaded.
- An element that declares a component with `turbo_desktop_bridge` gets it.
  The helper wrote the attributes and nothing read them.
- `TurboDesktop.toggleDevTools()` and View → Developer Tools open the
  developer tools. They logged a line and did nothing.
- A modal that moves on to an ordinary page closes, and the window underneath
  goes there. A form saved in a modal left the modal open on the list, and
  the window underneath never showed what was saved.
- The Dev Inspector stays through Turbo visits. It was in the body, which
  Turbo replaces.
- The Dev Inspector's scripts are asked about every time rather than cached
  for an hour, which kept the old inspector running after an upgrade, and
  the inspector is asked for by the gem's version.
- Notifications appear under `tauri dev` on macOS.
- A file dialog can be given a default name and the kinds of file to offer.
- A deep link that starts the app is followed. It was handed to a window
  with no page in it yet, and lost. A file that starts the app while the
  server is still starting is kept for the app in the same way.
- `data-turbo-confirm` asks. The webview does not show the browser's
  `confirm()`, so every button that asked first did nothing. Turbo is given a
  dialog of the system's own, and `TurboDesktop.confirm()` and `.alert()` ask
  and tell from JavaScript.
- A file opened with the app on macOS is opened, once. It arrived as a
  `file:` URL, was taken for a link, and the app asked the server for the
  file's path as a page.
- The Actions menu can be chosen from with the mouse, and goes when its last
  item does.
- A modal on its way to another page shows nothing while the shell decides.
- Two bridge controllers on one page keep their own component names.
- In development, the error page opens in the app rather than in the browser.
- The offline banner goes away when the next request succeeds.
- `TurboDesktop.platform` reports the platform the app is running on. It said
  `"macos"` everywhere.
- `turbo-desktop-bridge` works from TypeScript, under `bundler` and `nodenext`
  resolution, and can be imported before the shell has injected.
- A link followed or a file opened while the app is running reaches it, on
  Windows and Linux. Each started a second copy of the app, with a window of
  its own, and the one already open heard nothing.
- Quitting the app stops the server on Windows. The shell stopped `cmd`, and
  the server `cmd` had started kept running, and kept the port.
- An app reports the platform it is running on to Rails. A new app's
  configuration carried the user agent of the machine it was scaffolded on,
  so one made on a Mac said macOS when it ran on Windows.
- A configuration saved with a byte order mark is read. Notepad and Windows
  PowerShell save one, and the app refused to start.
- `TurboDesktop.fs.read(path, "base64")` reads a file that is not text. The
  encoding was ignored, and such a file could not be read at all.

### Changed

- The TypeScript definitions declare the whole runtime API.
- The README's quick start mounts the engine, and lists every bridge component.
- The README says where a component differs between platforms, and that a
  dropped or opened path is granted for writing as well as reading.

## 0.2.3 (2026-09-27)

### Fixed

- The Dev Inspector now loads in apps with forgery protection on, which is
  every app that uses `load_defaults`. Its modules are fetched with `import()`,
  a plain GET for JavaScript, which Rails refused with a 422 as a cross-origin
  script embed. The inspector's controller serves a fixed list of public,
  static files and no longer takes part in that check.

### Changed

- The README shows the Gemfile line pinned (`"~> 0.2"`), states Ruby >= 3.2 to
  match the gemspec, and quotes a current User-Agent.

## 0.2.2 (2026-09-27)

Version aligned with the desktop shell's 0.2.2 release. The CLI now scaffolds
new apps with `gem "turbo_desktop-rails", "~> 0.2"`; it previously wrote
`"~> 0.1"`, which resolved to 0.1.1. First 0.2.x version published to RubyGems.
No gem-side API changes.

## 0.2.1 (2026-07-29)

Version aligned with the desktop shell's 0.2.1 release, which fixes bridge
events (shell/sudo output streaming, drag & drop, component onReceive) never
reaching pages loaded from the app server. No gem-side API changes.

## 0.2.0 (2026-07-29)

Version aligned with the desktop shell's 0.2.0 release (server auto-start,
Windows support, cross-platform sudo, drag & drop, file associations,
clipboard, launch-at-login). No gem-side API changes.

## 0.1.1 (2026-07-27)

### Added

- Install generator: `rails generate turbo_desktop:install` scaffolds the initializer.
- Dev Inspector support:
  - `config.inspector_enabled` and the `turbo_desktop_inspector_meta_tag` view helper to enable
    the in-app inspector overlay (dev only).
  - The gem now serves the inspector's JavaScript **same-origin** at `/turbo-desktop/inspector.js`
    (and its sub-modules), so the desktop shell can `import()` it without extra setup.
  - `config.inspector_mount_path` to match a custom engine mount point.

### Changed

- Minimum Ruby version is now 3.3.

## 0.0.1 (2026-03-22)

- Initial release
- User-Agent detection for Turbo Desktop apps (`turbo_desktop_app?`)
- Platform and architecture detection (`turbo_desktop_platform`, `turbo_desktop_arch`)
- View helpers for conditional rendering (`turbo_desktop_only`, `turbo_web_only`)
- Bridge component data attribute helper (`turbo_desktop_bridge`)
- Path configuration endpoint (`/turbo-desktop/path-configuration.json`)
- Configurable path configuration rules and User-Agent pattern
