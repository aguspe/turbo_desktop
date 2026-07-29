# Changelog

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
