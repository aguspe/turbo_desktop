# Contributing to Turbo Desktop

Thanks for your interest in Turbo Desktop — the [Hotwire Native](https://native.hotwired.dev/)
pattern brought to the desktop. Contributions of all kinds are welcome: bug reports, docs,
tests, and features.

## Ways to help

- **Report a bug** — open an issue using the *Bug report* template.
- **Request a feature** — open an issue using the *Feature request* template.
- **Pick up a "good first issue"** — see the [issues](https://github.com/aguspe/turbo_desktop/issues)
  labelled `good first issue`.
- **Improve docs** — the README and the docs site (`docs/`) are always improvable.

## Adding a command

A `#[tauri::command]` needs three things, not one. Register it in
`generate_handler!`, add it to `APP_COMMANDS` in `src-tauri/build.rs`, and grant
the generated `allow-<command>` permission in
`src-tauri/capabilities/main.json`.

Miss either of the last two and the command works from bundled pages but is
refused for anything loaded from your server — with `not allowed. Plugin not
found`, visible only in the webview console. `test/acl.test.js` checks all three
stay in step.

New commands should also call `security::ensure_trusted_caller` before doing
anything, so only your app's own origin can reach them.

## Project layout

Turbo Desktop is three pieces in one repo:

| Path | What it is |
|------|-----------|
| `src-tauri/` | The Rust/Tauri desktop shell (window mgmt, path-config routing, OS APIs). |
| `src/`, `packages/bridge/` | The JS layer (`turbo-desktop.js`) that intercepts Turbo visits and bridges to native. |
| `turbo_desktop-rails/` | The Rails gem — desktop-shell awareness, view helpers, path-configuration endpoint. |
| `cli/` | The `turbo-desktop` CLI (`npx turbo-desktop new myapp`). |
| `docs/`, `site/` | Documentation site. |

## Development setup

```bash
git clone https://github.com/aguspe/turbo_desktop.git
cd turbo_desktop
cargo install tauri-cli   # if you don't have it
npm install
```

Configure the shell via `turbo-desktop.config.json` (JSON) — see the README quick-start.
Turbo Desktop points a WebView at a **running Rails app** (`server_url`), so start your Rails
server, then run the shell:

```bash
bin/rails server   # your Rails app (terminal 1)
cargo tauri dev    # the desktop shell (terminal 2)
```

## Running the tests

Please run the suite for whichever piece you touched (CI runs all three):

```bash
# Rails gem
cd turbo_desktop-rails && bundle exec rake test

# JavaScript
npm test

# Rust shell
cd src-tauri && cargo check      # cargo test once Rust tests exist
```

## Pull requests

- **One concern per PR.** Small, focused PRs are reviewed and merged faster.
- Add or update tests for behaviour changes.
- Make sure the relevant test suite passes locally before opening the PR.
- **Commit messages:** please use [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat:`, `fix:`, `docs:`, `chore:`, `test:`, optionally scoped like `fix(inspector):`).
  It keeps history readable and helps changelog generation.
- Reference the issue your PR addresses (e.g. `Closes #12`).

## Reporting security issues

Please **do not** open a public issue for security vulnerabilities. Instead, contact the
maintainer privately (see the repo profile) so it can be addressed before disclosure.

## License

By contributing, you agree that your contributions are licensed under the project's
[MIT License](LICENSE).
