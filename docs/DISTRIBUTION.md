# Distributing a Turbo Desktop app

Turbo Desktop apps are [Tauri](https://tauri.app) apps, so distribution means producing native
installers per OS. This guide covers the easy path (a release workflow), local builds, and the
optional-but-recommended signing/update setup.

## TL;DR — cut a release by pushing a tag

This repo ships [`.github/workflows/release.yml`](../.github/workflows/release.yml). To release:

```bash
git tag v0.2.4
git push origin v0.2.4
```

CI then builds on three runners (Tauri can't cross-compile) and attaches installers to a **draft
GitHub Release** for you to review and publish:

| Platform | You get |
|----------|---------|
| macOS (universal) | `.dmg` + `.app` (runs on Intel **and** Apple Silicon) |
| Windows | `.msi` + NSIS `.exe` |
| Linux | `.deb` + `.AppImage` |

You can also run it manually from the **Actions → Release → Run workflow** button.

## What ships inside the app

Turbo Desktop follows the Hotwire Native model: the shell loads `server_url` from
`turbo-desktop.config.json`, baked in at build time. So a distributed app is a **thin native shell
pointing at your hosted Rails app** — you ship the binary, your Rails app is the product. Set
`server_url` to your production URL before building for release.

## Building locally (to test a bundle)

```bash
npm run build                    # cargo tauri build — bundles for the current OS
npm run build:apple-silicon      # arm64 macOS only
```
Output: `src-tauri/target/release/bundle/`.

## Using this in your own app

`npx turbo-desktop new myapp` (and `init`) writes the same workflow into your Rails app at
`.github/workflows/release.yml`, pointed at `desktop/`, where the scaffold puts the Tauri project.
Push a tag and you get the draft release above, named after your repository.

Scaffolded before this existed? Copy
[`templates/release.yml`](../templates/release.yml) into your app's `.github/workflows/`.

## Signing & notarization (recommended before shipping to real users)

Unsigned builds trigger Gatekeeper (macOS) and SmartScreen (Windows) warnings; on macOS the user
has to right-click → Open the first time. Builds are **unsigned by default** so a first release
just works.

**macOS:** add six repository secrets and the next tag ships signed and notarized. There is
nothing to edit in the workflow: a step before the build checks for `APPLE_CERTIFICATE` and
passes the secrets through only when it is set. If some are set and some are missing, the
workflow stops in seconds and names the missing one.

You need an [Apple Developer Program](https://developer.apple.com/programs/) membership (paid,
yearly) and Xcode installed once to create the certificate.

| Secret | Value | How to get it |
|---|---|---|
| `APPLE_CERTIFICATE` | the `.p12` export, base64-encoded | Xcode → Settings → Accounts → Manage Certificates → **+** → *Developer ID Application*. Then in Keychain Access, right-click that certificate → Export → `.p12`, choose a password. `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | the password you chose at export | |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` | `security find-identity -v -p codesigning` prints it in quotes |
| `APPLE_ID` | the email of your Apple account | |
| `APPLE_PASSWORD` | an app-specific password, not your account password | [appleid.apple.com](https://appleid.apple.com) → Sign-In and Security → App-Specific Passwords |
| `APPLE_TEAM_ID` | ten characters, e.g. `A1B2C3D4E5` | [developer.apple.com/account](https://developer.apple.com/account) → Membership details |

From the terminal, with the [GitHub CLI](https://cli.github.com):

```bash
gh secret set APPLE_CERTIFICATE < <(base64 -i cert.p12)
gh secret set APPLE_CERTIFICATE_PASSWORD
gh secret set APPLE_SIGNING_IDENTITY
gh secret set APPLE_ID
gh secret set APPLE_PASSWORD
gh secret set APPLE_TEAM_ID
```

Each command without input prompts for the value. Verify a released build with
`spctl -a -vv "YourApp.app"`; a notarized app prints `source=Notarized Developer ID`.

**Windows** (Authenticode): configure `bundle.windows.certificateThumbprint` (or a signing
command) in `tauri.conf.json`. See the Tauri guide: [Windows](https://tauri.app/distribute/sign/windows/).

## Auto-update (optional)

`tauri.conf.json` includes the `updater` plugin, but `endpoints` and `pubkey` are empty — updates
are **off** until you configure them:

1. Generate a keypair: `npx tauri signer generate`.
2. Put the public key in `tauri.conf.json` → `plugins.updater.pubkey` and add your update-server
   `endpoints`.
3. Add the private key + password as the `TAURI_SIGNING_PRIVATE_KEY` /
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets. The workflow passes them through
   automatically when the key is set.

Details: [Tauri updater](https://tauri.app/plugin/updater/).

## Status

- ✅ Cross-OS installers via one tag (this workflow).
- ✅ macOS signing / notarization — automatic once the six `APPLE_*` secrets are set.
- ⚙️ Windows signing — configure in `tauri.conf.json`.
- ⚙️ Auto-update — plugin present, endpoints/keys not yet configured.

---

More at the official site: **[turbo-desktop.dev](https://turbo-desktop.dev/)**.
