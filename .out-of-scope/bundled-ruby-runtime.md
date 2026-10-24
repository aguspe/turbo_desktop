# Bundled Ruby runtime (local-first, serverless apps)

Turbo Desktop does not ship a Ruby interpreter inside the app bundle, and does
not aim to run a Rails app with no server the developer operates.

## Why this is out of scope

Turbo Desktop is a thin native shell around a running Rails app, the same shape
as Hotwire Native on iOS and Android. The Rails server is the product; the shell
adds window management, menus, tray, notifications and OS bridges. That is what
keeps the project small and the binary around 5–10 MB, and it is the reason a
team with an existing Rails app can adopt it without writing a second app.

Shipping the runtime changes what the project is. It is well understood what
it would take, and most of it is packaging rather than Ruby:

- A package-manager Ruby does not relocate. On macOS, `psych.bundle` links
  `/opt/homebrew/opt/libyaml`, `openssl.bundle` and `puma_http11.bundle` link
  `/opt/homebrew/opt/openssl@3`, and both link `libruby` by absolute Cellar
  path. Rails will not boot without psych, so copying a Ruby in does not work.
  The interpreter has to be built for it (`--enable-load-relative`,
  `--with-static-linked-ext`), per target triple, in CI.
- `rails server` creates `tmp/cache`, `tmp/pids` and `tmp/sockets` under
  `Rails.root` and ignores `config.paths` (railties
  `commands/server/server_command.rb`, `create_tmp_directories`). Inside a
  read-only `.app` that is `Errno::EACCES`. rails/rails#39583 was closed stale.
  Working around it means booting Puma from `config.ru` and relocating `tmp`,
  `log` and `storage` to a per-OS writable directory.
- Every bundled interpreter and native extension has to be signed and
  notarized alongside the shell, on three platforms.

That is a build-and-signing pipeline for Ruby itself, maintained forever, for a
use case the project was not designed around. Someone who wants an offline
desktop app with no server is better served by Tauri on its own, or by a
project dedicated to pressing Ruby apps into bundles (Tebako).

## What is still possible

Nothing in the shell prevents it from the outside. `server.command` is an
arbitrary command run from the config directory, and `server.rs` only starts it
when nothing is listening on the port. A separate project that produces a
relocatable Ruby can point `server.command` at it and the shell will work
unchanged. That is the right place for the work to live.

## Prior requests

- #2 — question about whether a server is required (answered, not a request)
- #36 — "[Feature]: ship the Ruby runtime, so an app can run without a hosted server"
