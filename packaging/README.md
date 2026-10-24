# Packaging a Rails app as a desktop application

Turn a Rails app plus a relocatable interpreter into something a person can
download and open, on macOS, Linux or Windows.

```bash
packaging/build-runtime.sh --out out/ruby          # or install turbo_desktop-runtime
packaging/verify-runtime.sh out/ruby

packaging/pack.sh        --app ../my_app --runtime out/ruby --gems out/gems --name "Ledger"
packaging/pack-linux.sh  --app ../my_app --runtime out/ruby --gems out/gems --name "Ledger"
packaging\pack-windows.ps1 -App ..\my_app -Runtime out\ruby -Gems out\gems -Name "Ledger"
```

| File | What it does |
|---|---|
| `build-runtime.sh` | Builds a relocatable Ruby. macOS and Linux; Windows is fetched. |
| `verify-runtime.sh` | Proves one relocates before you trust it. |
| `gem.sh` | Wraps a build in a platform gem. |
| `pack.sh` | macOS `.app`, pruned and signed. |
| `pack-linux.sh` | Linux directory, tarball and `.desktop` entry. |
| `pack-windows.ps1` | Windows directory and zip. |
| `prune.sh` / `prune.ps1` | Removes what a user's machine never reads. |
| `dmg.sh` | Disk image. No certificate needed. |
| `notarize.sh` | Signs and notarises. Needs a Developer ID. |
| `templates/boot.rb` | The boot sequence all three platforms share. |

Longer notes: [CONTROL_CHANNEL.md](CONTROL_CHANNEL.md) for calling native from
Ruby, [DISTRIBUTION.md](DISTRIBUTION.md) for what Gatekeeper actually does.

## The four things that decide whether this works

Each was measured, and each cost time to find.

### The interpreter must be built for the job

A package-manager Ruby will not relocate. On macOS `libruby` links gmp,
`openssl.bundle` links libssl, and stdlib `psych.bundle` links libyaml — all by
absolute path. **Rails cannot boot without psych**, so copying an existing
interpreter fails on any path. Build it with `--enable-load-relative` against
statically linked dependencies.

Linux has the same problem and one of its own: OpenSSL installs to `lib64` while
Ruby's configure looks in `lib`, so a build can silently link the *system*
OpenSSL and then die at runtime on a missing symbol. `--libdir=lib` removes it.

`verify-runtime.sh` catches that by doing real work with OpenSSL and comparing
`OPENSSL_VERSION` against `OPENSSL_LIBRARY_VERSION`. An earlier version printed
the constant and reported a broken build as healthy, which is worse than no
check at all.

### Never `rails server`

`railties .../server_command.rb:70` creates `tmp/cache`, `tmp/pids` and
`tmp/sockets` under `Rails.root` without consulting `config.paths`. In a
read-only bundle that is `Errno::EACCES`. `templates/boot.rb` starts Puma from
`config.ru` instead.

### Writable state lives outside the application

A signed `.app`, a `/opt` directory and `C:\Program Files` are all read-only,
while Rails expects `tmp`, `log` and `storage` to be writable. Each launcher
points `DESKTOP_DATA_DIR` at the right place — Application Support,
`XDG_DATA_HOME`, or `%LOCALAPPDATA%` — and creates them there. Verified on macOS
and Linux by setting the whole tree `a-w` and booting anyway.

### The server must exit when stdin closes

The only layer that survives the shell being force-quit, since no shell code
runs then. Tauri's process kill signals the direct child only: no process group,
no job object. `boot.rb` watches stdin and exits on EOF, measured at about 0.2s.

## The handshake

Puma binds `127.0.0.1:0`, the OS picks the port, and one line goes to stdout:

```json
{"protocol":"1.0","url":"http://127.0.0.1:58747","pid":95489}
```

Binding port zero and reporting back removes the race you get from probing for a
free port and then binding it.

Two details that cost time: Puma's banner would pollute the channel and `quiet`
does not stop it, so `boot.rb` keeps a private `dup` of the real stdout and
points `$stdout` at stderr. And `binder.full_urls` does not exist in Puma 8 —
the port comes from `binder.connected_ports`, and an exception in that hook is
swallowed by Puma's event loop, so the hook reports failure explicitly.

## Pruning

| Removed | macOS | Linux |
|---|---|---|
| Static archives, `.gem` cache, debug symbols, docs, headers, gem test suites, stripped binaries | 182 MB → 118 MB (35%) | 357 MB → 145 MB (59%) |

The app is booted after pruning, because smaller is worthless if it is also
broken. That caught a real one: deleting every directory named `test` also
removes `rack-test`'s `lib/rack/test/`, which is library code. Pruning only
touches each gem's root.

## Signing, and what it is worth

Inside-out, never `--deep`, which is deprecated and signs in the wrong order.
**The entitlements go on the interpreter, not the bundle** — the app's main
executable is a launcher script, and `ruby` is what `dlopen`s the extensions.

Two are needed under an ad-hoc signature, and they fail differently: without
`disable-library-validation` the `dlopen` is refused, and without
`allow-unsigned-executable-memory` the kernel kills the process.

⚠️ **Signing is not the same as being accepted.** Measured on macOS 26.5.1: both
an ad-hoc signature and a real Apple *Distribution* certificate are rejected by
Gatekeeper, quarantined or not, even though `codesign --verify --strict` passes.
Direct download is judged on a **Developer ID Application** certificate.
See [DISTRIBUTION.md](DISTRIBUTION.md).

## Verified end to end, in CI, on every push

`.github/workflows/package-smoke.yml` builds the interpreter, generates a real
Rails app with it, packs and prunes and signs, then launches the artifact and
holds it to its contract: it announces itself, answers `GET /up`, and exits when
stdin closes. Linux additionally unpacks the tarball elsewhere and runs it with
the tree read-only.

```
macOS   175M -> 108M, signature verifies, GET /up 200, exited 0.2s, 63M dmg
Linux   357M -> 145M, GET /up 200 twice (packed and read-only), 52M tarball
```

## Not yet done

Notarisation and a stapled first launch, both gated on a Developer ID.
Auto-update. A Windows installer — Tauri's bundler already produces the MSI and
NSIS packages, so this deliberately stops at a directory and a zip.
