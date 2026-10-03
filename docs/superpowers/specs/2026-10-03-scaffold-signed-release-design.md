# Scaffolded release workflow with automatic macOS signing

**Date:** 2026-10-03
**Status:** approved design, awaiting implementation plan

## Problem

An app author who scaffolds a desktop shell with `turbo-desktop new` or `init`
gets no release pipeline. To ship installers they must find
`.github/workflows/release.yml` in this repo, copy it, edit `projectPath`,
`working-directory` and `npm ci`, and then, to sign for macOS, uncomment a
block of six environment variables and set six repository secrets. Leaving the
block uncommented with secrets unset breaks the build, because an empty
`APPLE_CERTIFICATE` makes Tauri import an empty certificate. The documentation
describes the secrets by name only and defers to Tauri's guides for how to
obtain each value.

Result: notarization is the step most likely to stall a first release.

## Goal

An author who has never notarized an app ships a signed, notarized macOS
installer on the first tag, with no YAML editing. An author without an Apple
Developer account ships an unsigned installer with the same workflow and no
configuration.

## Scope

In: macOS signing and notarization in CI, the auto-updater signing keys
(same mechanism, two lines), the scaffold, and the documentation that goes
with them.

Out: Windows Authenticode, Linux signing, updater endpoints and public key,
a local (non-CI) notarization command, an interactive secret-setup command.
These stay as documented opt-ins.

## Design

### 1. Workflow template: `templates/release.yml`

Same shape as the repo's own release workflow: three runners (macOS universal,
Ubuntu, Windows), `tauri-action` builds and attaches installers to a draft
GitHub Release on a `v*` tag or manual dispatch.

Two new steps run before `tauri-action` and replace the commented-out signing
block:

```yaml
- name: Enable signing and notarization when the secrets are set
  if: matrix.platform == 'macos-latest' && secrets.APPLE_CERTIFICATE != ''
  env:
    APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
    APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
    APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
    APPLE_ID: ${{ secrets.APPLE_ID }}
    APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
    APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
  run: |
    for name in APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_SIGNING_IDENTITY APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
      if [ -z "${!name}" ]; then
        echo "::error::$name is not set. Signing needs all six APPLE_* secrets; see docs/DISTRIBUTION.md"
        exit 1
      fi
      echo "$name=${!name}" >> "$GITHUB_ENV"
    done

- name: Enable updater signing when the key is set
  if: secrets.TAURI_SIGNING_PRIVATE_KEY != ''
  env:
    TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
  run: |
    echo "TAURI_SIGNING_PRIVATE_KEY=$TAURI_SIGNING_PRIVATE_KEY" >> "$GITHUB_ENV"
    echo "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" >> "$GITHUB_ENV"
```

Behaviour:

- No secrets: both steps skip, build is unsigned, same as today.
- All six Apple secrets: `tauri-action` sees them in the environment and
  signs and notarizes.
- Some but not all Apple secrets: the step fails in seconds naming the
  missing one, before the Rust build.

The `tauri-action` step's `env` carries only `GITHUB_TOKEN`. There is nothing
to uncomment.

Why a step and not `env` on the `tauri-action` step: the `secrets` context can
be tested in a step-level `if`, but not at job level, and a step's `env` cannot
include a key conditionally. Writing to `$GITHUB_ENV` from a gated step is the
one place both constraints are satisfied. Values written there are masked in
logs like any secret.

Template versus the repo's own `.github/workflows/release.yml`, the only
permitted differences:

| Line | Repo | Template |
|---|---|---|
| `tauri-action` `with.projectPath` | absent (`.`) | `desktop` |
| Node steps `working-directory` | absent | `desktop` |
| dependency install | `npm ci` | `npm install` (scaffold commits no lockfile) |
| `releaseName` | `Turbo Desktop ${{ github.ref_name }}` | `${{ github.event.repository.name }} ${{ github.ref_name }}` |
| header comment | release this repo | release your app |

The repo workflow adopts the two gated steps as well, so the shell's own
releases exercise the same path.

### 2. Scaffold: `cmdInit` in `cli/turbo-desktop.js`

After writing `turbo-desktop.config.json`:

1. Target path is `<projectDir>/.github/workflows/release.yml`, the Rails
   application root, because GitHub reads workflows only from the repository
   root. Rails 7.2+ already puts `ci.yml` there.
2. Create the directory recursively.
3. If the target exists, do not touch it. Print one line saying so and where
   the template lives. Otherwise copy `templates/release.yml` verbatim.
4. The "Next steps" message gains a step: tag and push to build installers;
   set the six `APPLE_*` secrets to sign them; link to `docs/DISTRIBUTION.md`.

`cmdNew` calls `cmdInit`, so it inherits the behaviour.

`package.json` `files` gains `"templates"` so the published npm package
carries the template. The existing `templates/turbo_desktop.rb` is not used by
the CLI (the gem generator has its own copy); it ships along and is otherwise
out of scope.

No new CLI command. Authors with an existing scaffold copy the template by
hand; the docs give the raw URL.

### 3. Documentation

`docs/DISTRIBUTION.md`:

- "Using this in your own app": shrink to "the scaffold writes
  `.github/workflows/release.yml` for you". Keep one sentence for existing
  projects with the raw template URL.
- "Signing & notarization": replace the uncomment instructions with: set six
  repository secrets, the next tag ships signed and notarized. Then a table,
  one row per secret, with the value and where to get it:

  | Secret | Value | How to get it |
  |---|---|---|
  | `APPLE_CERTIFICATE` | base64 of the `.p12` export | Xcode > Settings > Accounts > Manage Certificates > + > Developer ID Application; Keychain Access > export as `.p12`; `base64 -i cert.p12 \| pbcopy` |
  | `APPLE_CERTIFICATE_PASSWORD` | password chosen at export | |
  | `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Name (TEAMID)` | `security find-identity -v -p codesigning` |
  | `APPLE_ID` | Apple account email | |
  | `APPLE_PASSWORD` | app-specific password | appleid.apple.com > Sign-In and Security > App-Specific Passwords |
  | `APPLE_TEAM_ID` | ten-character team ID | developer.apple.com/account > Membership details |

- Prerequisite sentence: Apple Developer Program membership (paid, yearly).
  Without it the build is unsigned and users open the app via right-click >
  Open the first time.
- A `gh secret set` snippet, six lines, for terminal users.
- Verification: `spctl -a -vv YourApp.app` reports `source=Notarized Developer ID`.
- Partial-secrets behaviour: the workflow fails early and names the missing
  secret.
- Auto-update: key passthrough is now automatic once the two
  `TAURI_SIGNING_PRIVATE_KEY*` secrets exist; endpoints and public key remain
  manual, text unchanged.
- Status block: signing goes from "opt-in (uncomment)" to "automatic when the
  secrets are set".

`README.md` Distribution section: one sentence that the scaffold includes the
workflow and that six secrets turn on signing.

Windows signing text unchanged.

### 4. Tests (`test/cli.test.js`, Node test runner, existing style)

1. **Template tracks the repo workflow.** Line-diff `templates/release.yml`
   against `.github/workflows/release.yml`; every differing line must match
   one of the allowed patterns from the table in section 1. Fixing one file
   without the other fails the suite.
2. **A scaffolded project gets a release workflow.** In the existing tmpdir
   scaffold test, assert `.github/workflows/release.yml` exists and equals
   the template byte for byte.
3. **The scaffold never overwrites a release workflow.** Pre-create the file
   with sentinel content, scaffold, assert the sentinel survives.
4. **The published package carries the template.** Extend the existing
   "carries everything the scaffold copies" test with `templates/release.yml`.
5. **Signing is never forced on.** In both workflow files, the `tauri-action`
   step's `env` contains only `GITHUB_TOKEN`. Guards against the empty-secret
   footgun returning.

Manual, after merge: push a tag on this repo with no Apple secrets and confirm
the unsigned build is green. Signing with real secrets requires the
maintainer's Apple account and happens at their discretion.

## Non-goals and follow-ups

- Windows Authenticode, including free options for open source (SignPath,
  Azure Trusted Signing). Candidate for a later spec.
- `turbo-desktop sign setup`: interactive export of the certificate and
  `gh secret set`. Revisit if the table in the docs proves insufficient.
- Local notarization without CI (`xcrun notarytool submit --wait`, staple).
- Updater endpoints and public key configuration.
