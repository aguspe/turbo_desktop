# Scaffolded Release Workflow with Automatic macOS Signing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `turbo-desktop init` writes a GitHub Actions release workflow into the Rails app, and that workflow signs and notarizes macOS builds by itself whenever the six `APPLE_*` repository secrets exist.

**Architecture:** A gated shell step writes secrets into `$GITHUB_ENV` only when they are set, so the `tauri-action` step never carries them directly and nothing has to be uncommented. The workflow lives once in `templates/release.yml`, which the CLI copies to `<rails-root>/.github/workflows/release.yml`; the repo's own `.github/workflows/release.yml` adopts the same steps and a drift-guard test keeps the two files in step.

**Tech Stack:** GitHub Actions (`tauri-apps/tauri-action@v0`), bash, Node 18+ CLI (ESM, `node:test`), Markdown docs.

**Spec:** `docs/superpowers/specs/2026-10-03-scaffold-signed-release-design.md`

## Global Constraints

- Node `>=18` (package.json `engines`); CLI stays ESM, no new runtime dependencies.
- No YAML parser dependency: tests inspect the workflow files with string and regex operations.
- Workflow matrix unchanged: `macos-latest` (`--target universal-apple-darwin`), `ubuntu-latest`, `windows-latest`.
- `tauri-action` step `env` contains exactly one key: `GITHUB_TOKEN`.
- Secret names, verbatim: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- Template vs repo workflow may differ only on: `projectPath`, `working-directory`, `npm ci`/`npm install`, `cache: npm`, `releaseName`, and comment lines.
- Scaffold never overwrites an existing `.github/workflows/release.yml`.
- Any version quoted in `docs/DISTRIBUTION.md` or `README.md` must equal `package.json` `version` (existing test `the READMEs and guides quote the current version`); avoid adding new quoted versions.
- Test style: `node --test`, `assert/strict`, one behaviour per test, message explains the consequence.

## Review Focus

1. **Multi-line secret values.** Linux `base64` wraps at 76 columns; a user pasting that into `APPLE_CERTIFICATE` gives a value with newlines. `echo "NAME=$VALUE" >> $GITHUB_ENV` would corrupt the file. Expected: the step writes heredoc form (`NAME<<DELIM … DELIM`) so multi-line values survive. Test: Task 2, "the signing step survives a multi-line secret".
2. **Windows default shell.** `run:` on `windows-latest` uses PowerShell; bash syntax (`${!name}`, `>>`) fails there. The updater step runs on all three runners. Expected: both gated steps declare `shell: bash`. Test: Task 1, "the gated steps run under bash on every runner".
3. **Partial secrets.** `APPLE_CERTIFICATE` set, `APPLE_TEAM_ID` forgotten. Expected: fail in seconds naming the missing secret, not after a ten-minute Rust build with a cryptic notarytool error. Test: Task 2, "the signing step names a missing secret and stops".
4. **`.github/` exists, `workflows/` does not.** Rails 7.2+ scaffolds `.github/workflows/ci.yml`, but an older app may have `.github/` with only `dependabot.yml`. Expected: the directory is created and the file written. Test: Task 3, "the scaffold creates .github/workflows when only .github exists".
5. **Existing workflow kept, user told.** A user who hand-wrote a `release.yml` reruns `init` after deleting `desktop/`. Expected: their file is untouched and the output says so, with the template path. Test: Task 3, "the scaffold never overwrites a release workflow".

---

### Task 1: Repo workflow adopts gated signing steps

**Files:**
- Modify: `.github/workflows/release.yml`
- Test: `test/cli.test.js`

**Interfaces:**
- Consumes: nothing.
- Produces: the canonical workflow shape Task 2's template mirrors; a helper `tauriActionEnvKeys(yaml)` in the test file used again by Task 2.

- [ ] **Step 1: Write the failing tests**

Append to `test/cli.test.js`, before the final `// ─── What the documentation and the types promise` section:

```js
// ─── Release workflow ────────────────────────────────────────────────────────

// Keys under `env:` of the tauri-action step. No YAML parser: the step is the
// only `uses: tauri-apps/tauri-action` in the file and `with:` always follows.
function tauriActionEnvKeys(yaml) {
  const step = yaml.match(/uses: tauri-apps\/tauri-action[\s\S]*?\n\s+with:/);
  assert.ok(step, "the workflow should run tauri-action with a `with:` block");
  const env = step[0].match(/\n\s+env:\n([\s\S]*?)\n\s+with:/);
  if (!env) return [];
  return env[1]
    .split("\n")
    .filter((line) => /^\s+[A-Z_]+:/.test(line))
    .map((line) => line.trim().split(":")[0]);
}

const WORKFLOWS = [[".github", "workflows", "release.yml"]];

test("signing is never forced on", () => {
  for (const file of WORKFLOWS) {
    const yaml = read(...file);
    assert.deepEqual(
      tauriActionEnvKeys(yaml),
      ["GITHUB_TOKEN"],
      `${file.join("/")}: an APPLE_* secret passed while unset makes Tauri import an empty certificate`
    );
    assert.doesNotMatch(yaml, /^\s*#\s*APPLE_/m, `${file.join("/")}: no commented-out signing block to uncomment`);
  }
});

test("signing turns on when the secrets are set", () => {
  for (const file of WORKFLOWS) {
    const yaml = read(...file);
    assert.match(yaml, /if: .*secrets\.APPLE_CERTIFICATE != ''/, `${file.join("/")}: macOS signing gated on the certificate`);
    assert.match(yaml, /if: .*secrets\.TAURI_SIGNING_PRIVATE_KEY != ''/, `${file.join("/")}: updater signing gated on the key`);
    for (const name of [
      "APPLE_CERTIFICATE",
      "APPLE_CERTIFICATE_PASSWORD",
      "APPLE_SIGNING_IDENTITY",
      "APPLE_ID",
      "APPLE_PASSWORD",
      "APPLE_TEAM_ID",
      "TAURI_SIGNING_PRIVATE_KEY",
      "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
    ]) {
      assert.match(yaml, new RegExp(`${name}: \\$\\{\\{ secrets\\.${name} \\}\\}`), `${file.join("/")}: passes ${name}`);
    }
  }
});

test("the gated steps run under bash on every runner", () => {
  for (const file of WORKFLOWS) {
    const yaml = read(...file);
    const gated = yaml.split(/\n(?=\s+- name:)/).filter((step) => /secrets\.\w+ != ''/.test(step));
    assert.equal(gated.length, 2, `${file.join("/")}: one step for Apple, one for the updater`);
    for (const step of gated) {
      assert.match(step, /shell: bash/, `${file.join("/")}: windows-latest defaults to PowerShell, where \${!name} is a syntax error`);
    }
  }
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test test/cli.test.js 2>&1 | grep -E "^(not ok|ok) .*(signing|gated)"`
Expected: three `not ok` lines (the current workflow has the commented block and no gated steps).

- [ ] **Step 3: Rewrite the workflow**

Replace `.github/workflows/release.yml` with:

```yaml
name: Release

# Cut a release by pushing a tag:  git tag v0.1.0 && git push origin v0.1.0
# This builds installers for macOS (universal), Windows, and Linux, and attaches
# them to a draft GitHub Release. Tauri can't cross-compile, so each OS builds on
# its own runner.
#
# Builds are unsigned until you add repository secrets; see docs/DISTRIBUTION.md.
# With the six APPLE_* secrets set, macOS builds are signed and notarized. With
# TAURI_SIGNING_PRIVATE_KEY set, updater artifacts are signed. Nothing to edit.
on:
  push:
    tags: ["v*"]
  workflow_dispatch: # lets you run it manually from the Actions tab

jobs:
  release:
    permissions:
      contents: write # needed to create the release + upload assets
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: macos-latest # universal .dmg (Intel + Apple Silicon)
            args: "--target universal-apple-darwin"
          - platform: ubuntu-latest # .deb + .AppImage
            args: ""
          - platform: windows-latest # .msi + NSIS .exe
            args: ""
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Linux build dependencies
        if: matrix.platform == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libgtk-3-dev \
            libwebkit2gtk-4.1-dev \
            libappindicator3-dev \
            librsvg2-dev \
            patchelf

      - uses: dtolnay/rust-toolchain@stable
        with:
          # macOS universal binary needs both arches
          targets: ${{ matrix.platform == 'macos-latest' && 'aarch64-apple-darwin,x86_64-apple-darwin' || '' }}

      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
      - run: npm ci

      # The `secrets` context can be tested in a step-level `if`, not in a step's
      # `env`, so a gated step hands the values to later steps through GITHUB_ENV.
      # Heredoc form keeps multi-line values (a wrapped base64 certificate) intact.
      - name: Enable signing and notarization when the secrets are set
        if: matrix.platform == 'macos-latest' && secrets.APPLE_CERTIFICATE != ''
        shell: bash
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
            { echo "$name<<__TURBO_DESKTOP__"; echo "${!name}"; echo "__TURBO_DESKTOP__"; } >> "$GITHUB_ENV"
          done

      - name: Enable updater signing when the key is set
        if: secrets.TAURI_SIGNING_PRIVATE_KEY != ''
        shell: bash
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        run: |
          for name in TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD; do
            { echo "$name<<__TURBO_DESKTOP__"; echo "${!name}"; echo "__TURBO_DESKTOP__"; } >> "$GITHUB_ENV"
          done

      - name: Build and release
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "Turbo Desktop ${{ github.ref_name }}"
          releaseBody: "Download the installer for your platform below."
          releaseDraft: true # review before publishing
          prerelease: false
          args: ${{ matrix.args }}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm test 2>&1 | tail -8`
Expected: `fail 0`.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml test/cli.test.js
git commit -m "ci: sign and notarize when the secrets exist instead of by uncommenting

A gated step writes the APPLE_* and updater secrets into GITHUB_ENV only
when they are set, so an unset secret means an unsigned build rather than
an empty certificate import. Partial secrets fail before the Rust build."
```

---

### Task 2: Workflow template for scaffolded apps

**Files:**
- Create: `templates/release.yml`
- Modify: `package.json` (`files` array)
- Test: `test/cli.test.js`

**Interfaces:**
- Consumes: `tauriActionEnvKeys`, `WORKFLOWS` from Task 1 (extend `WORKFLOWS` so Task 1's three tests also cover the template).
- Produces: `templates/release.yml`, the file Task 3 copies; packaged via `files`.

- [ ] **Step 1: Write the failing tests**

In `test/cli.test.js`, change the `WORKFLOWS` constant from Task 1 to:

```js
const WORKFLOWS = [
  [".github", "workflows", "release.yml"],
  ["templates", "release.yml"],
];
```

Append after the Task 1 tests:

```js
// The scaffold copies the template; the repo releases itself with the real
// workflow. They must be the same workflow apart from where the project lives.
const TEMPLATE_ONLY_DIFFERENCES = [
  /^\s*#/, // comments explain each file's own audience
  /^\s*$/,
  /projectPath: desktop/,
  /working-directory: desktop/,
  /run: npm (ci|install)/,
  /cache: npm/, // the scaffold commits no lockfile for setup-node to key on
  /releaseName:/,
];

test("the template tracks the repo's release workflow", () => {
  const repo = read(".github", "workflows", "release.yml").split("\n");
  const template = read("templates", "release.yml").split("\n");
  const differing = [
    ...repo.filter((line) => !template.includes(line)),
    ...template.filter((line) => !repo.includes(line)),
  ];

  assert.ok(differing.length > 0, "the template should at least point at desktop/");
  for (const line of differing) {
    assert.ok(
      TEMPLATE_ONLY_DIFFERENCES.some((allowed) => allowed.test(line)),
      `templates/release.yml and .github/workflows/release.yml disagree on \`${line.trim()}\`; fix both or allow the difference`
    );
  }
});

test("the template builds the desktop/ project the scaffold creates", () => {
  const template = read("templates", "release.yml");
  assert.match(template, /projectPath: desktop/, "tauri-action must look in desktop/");
  assert.match(template, /working-directory: desktop/, "npm install must run in desktop/");
  assert.doesNotMatch(template, /npm ci/, "the scaffold writes no lockfile, so npm ci would fail");
  assert.doesNotMatch(template, /cache: npm/, "setup-node's cache needs a lockfile to key on");
  assert.doesNotMatch(template, /Turbo Desktop \$\{\{/, "the release should carry the app's name, not the shell's");
});

test("the published package carries the release template", () => {
  const { files } = JSON.parse(read("package.json"));
  assert.ok(files.includes("templates"), "turbo-desktop init copies templates/release.yml out of the installed package");
});

// Extract the `run:` script of the first step whose `if:` mentions the given
// secret, and run it the way Actions does (bash --noprofile --norc -eo pipefail)
// with the given environment and a scratch GITHUB_ENV.
function runGatedStep(secret, env) {
  const yaml = read("templates", "release.yml");
  const step = yaml
    .split(/\n(?=\s+- name:)/)
    .find((candidate) => candidate.includes(`secrets.${secret} != ''`));
  assert.ok(step, `a step gated on ${secret}`);
  const script = step
    .match(/run: \|\n([\s\S]*?)(?=\n\s+- name:|\n*$)/)[1]
    .split("\n")
    .map((line) => line.replace(/^ {10}/, ""))
    .join("\n");

  const githubEnvPath = join(mkdtempSync(join(tmpdir(), "turbo-desktop-gh-env-")), "env");
  writeFileSync(githubEnvPath, "");
  const result = spawnSync("bash", ["--noprofile", "--norc", "-eo", "pipefail", "-c", script], {
    env: { PATH: process.env.PATH, GITHUB_ENV: githubEnvPath, ...env },
    encoding: "utf-8",
  });
  return { status: result.status, stderr: result.stderr, stdout: result.stdout, githubEnv: readFileSync(githubEnvPath, "utf-8") };
}

const ALL_APPLE_SECRETS = {
  APPLE_CERTIFICATE: "MIIK",
  APPLE_CERTIFICATE_PASSWORD: "pw",
  APPLE_SIGNING_IDENTITY: "Developer ID Application: Example (TEAMID1234)",
  APPLE_ID: "dev@example.com",
  APPLE_PASSWORD: "abcd-efgh-ijkl-mnop",
  APPLE_TEAM_ID: "TEAMID1234",
};

test("the signing step survives a multi-line secret", () => {
  // Linux `base64` wraps at 76 columns, so a pasted certificate has newlines.
  const wrapped = "MIIK\nAAAA\nBBBB";
  const { status, githubEnv } = runGatedStep("APPLE_CERTIFICATE", { ...ALL_APPLE_SECRETS, APPLE_CERTIFICATE: wrapped });

  assert.equal(status, 0);
  assert.ok(
    githubEnv.includes(`APPLE_CERTIFICATE<<__TURBO_DESKTOP__\n${wrapped}\n__TURBO_DESKTOP__\n`),
    "a KEY=value line would truncate the certificate at the first newline"
  );
  assert.ok(githubEnv.includes("APPLE_TEAM_ID<<__TURBO_DESKTOP__\nTEAMID1234\n__TURBO_DESKTOP__"));
});

test("the signing step names a missing secret and stops", () => {
  // GitHub hands an unset secret to the step as an empty string, not an unset variable.
  const { status, stdout, githubEnv } = runGatedStep("APPLE_CERTIFICATE", { ...ALL_APPLE_SECRETS, APPLE_TEAM_ID: "" });

  assert.equal(status, 1, "a half-configured signing setup should fail before the Rust build");
  assert.match(stdout, /::error::APPLE_TEAM_ID is not set/);
  assert.doesNotMatch(githubEnv, /APPLE_TEAM_ID/, "nothing partial should reach later steps");
});

test("the updater step passes both key secrets through", () => {
  const { status, githubEnv } = runGatedStep("TAURI_SIGNING_PRIVATE_KEY", {
    TAURI_SIGNING_PRIVATE_KEY: "dW50cnVzdGVkIGNvbW1lbnQ6\nline2",
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "",
  });

  assert.equal(status, 0, "an empty password is valid for an unencrypted key");
  assert.ok(githubEnv.includes("TAURI_SIGNING_PRIVATE_KEY<<__TURBO_DESKTOP__\ndW50cnVzdGVkIGNvbW1lbnQ6\nline2\n__TURBO_DESKTOP__"));
  assert.ok(githubEnv.includes("TAURI_SIGNING_PRIVATE_KEY_PASSWORD<<__TURBO_DESKTOP__\n\n__TURBO_DESKTOP__"));
});
```

Add `writeFileSync` to the `node:fs` import at the top of the test file:

```js
import { readFileSync, mkdtempSync, writeFileSync } from "node:fs";
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test test/cli.test.js 2>&1 | grep -E "^not ok"`
Expected: `not ok` for the template tests (`ENOENT` on `templates/release.yml`) and the package test.

- [ ] **Step 3: Create the template**

Write `templates/release.yml`:

```yaml
name: Release

# Cut a release by pushing a tag:  git tag v0.1.0 && git push origin v0.1.0
# This builds installers for macOS (universal), Windows, and Linux, and attaches
# them to a draft GitHub Release. Tauri can't cross-compile, so each OS builds on
# its own runner.
#
# Written by `turbo-desktop init`. Builds are unsigned until you add repository
# secrets: https://github.com/aguspe/turbo_desktop/blob/main/docs/DISTRIBUTION.md
# With the six APPLE_* secrets set, macOS builds are signed and notarized. With
# TAURI_SIGNING_PRIVATE_KEY set, updater artifacts are signed. Nothing to edit.
on:
  push:
    tags: ["v*"]
  workflow_dispatch: # lets you run it manually from the Actions tab

jobs:
  release:
    permissions:
      contents: write # needed to create the release + upload assets
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: macos-latest # universal .dmg (Intel + Apple Silicon)
            args: "--target universal-apple-darwin"
          - platform: ubuntu-latest # .deb + .AppImage
            args: ""
          - platform: windows-latest # .msi + NSIS .exe
            args: ""
    runs-on: ${{ matrix.platform }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Linux build dependencies
        if: matrix.platform == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libgtk-3-dev \
            libwebkit2gtk-4.1-dev \
            libappindicator3-dev \
            librsvg2-dev \
            patchelf

      - uses: dtolnay/rust-toolchain@stable
        with:
          # macOS universal binary needs both arches
          targets: ${{ matrix.platform == 'macos-latest' && 'aarch64-apple-darwin,x86_64-apple-darwin' || '' }}

      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - run: npm install
        working-directory: desktop

      # The `secrets` context can be tested in a step-level `if`, not in a step's
      # `env`, so a gated step hands the values to later steps through GITHUB_ENV.
      # Heredoc form keeps multi-line values (a wrapped base64 certificate) intact.
      - name: Enable signing and notarization when the secrets are set
        if: matrix.platform == 'macos-latest' && secrets.APPLE_CERTIFICATE != ''
        shell: bash
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
            { echo "$name<<__TURBO_DESKTOP__"; echo "${!name}"; echo "__TURBO_DESKTOP__"; } >> "$GITHUB_ENV"
          done

      - name: Enable updater signing when the key is set
        if: secrets.TAURI_SIGNING_PRIVATE_KEY != ''
        shell: bash
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        run: |
          for name in TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD; do
            { echo "$name<<__TURBO_DESKTOP__"; echo "${!name}"; echo "__TURBO_DESKTOP__"; } >> "$GITHUB_ENV"
          done

      - name: Build and release
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          projectPath: desktop
          tagName: ${{ github.ref_name }}
          releaseName: "${{ github.event.repository.name }} ${{ github.ref_name }}"
          releaseBody: "Download the installer for your platform below."
          releaseDraft: true # review before publishing
          prerelease: false
          args: ${{ matrix.args }}
```

Note: the `run: |` script bodies are byte-identical to the repo workflow; `runGatedStep` dedents ten spaces, which is the indentation of the script lines in both files.

- [ ] **Step 4: Ship the template in the npm package**

In `package.json`, add `"templates"` to `files`, after `"src-tauri/tauri.conf.json"`:

```json
    "src-tauri/tauri.conf.json",
    "templates",
    "README.md",
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npm test 2>&1 | tail -8`
Expected: `fail 0`. If `the template tracks the repo's release workflow` fails on a line you did not expect, the two files have drifted: fix the file, not the allowlist.

- [ ] **Step 6: Commit**

```bash
git add templates/release.yml package.json test/cli.test.js
git commit -m "feat(cli): ship a release workflow template for scaffolded apps

Same workflow as the repo's own, pointed at desktop/ and named after the
user's repository. A drift test keeps the two files in step and bash tests
pin the gated step's behaviour on wrapped and missing secrets."
```

---

### Task 3: Scaffold writes the workflow

**Files:**
- Modify: `cli/turbo-desktop.js` (new export `writeReleaseWorkflow`; call it in `cmdInit` after `turbo-desktop.config.json` is written; extend the "Next steps" message)
- Test: `test/cli.test.js`

**Interfaces:**
- Consumes: `templates/release.yml` (Task 2), `PACKAGE_ROOT`.
- Produces: `export function writeReleaseWorkflow(projectDir, log = console.log)` returning `{ path: string, written: boolean }`.

- [ ] **Step 1: Write the failing tests**

Add `writeReleaseWorkflow` to the import list from `../cli/turbo-desktop.js` in `test/cli.test.js`, and `existsSync`, `mkdirSync` to the `node:fs` import:

```js
import { readFileSync, mkdtempSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
```

Append after the Task 2 tests:

```js
// ─── Scaffolding the release workflow ────────────────────────────────────────

const scratchProject = () => mkdtempSync(join(tmpdir(), "turbo-desktop-scaffold-"));

test("a scaffolded project gets a release workflow", () => {
  const project = scratchProject();
  const quiet = [];
  const { path, written } = writeReleaseWorkflow(project, (line) => quiet.push(line));

  assert.equal(written, true);
  assert.equal(path, join(project, ".github", "workflows", "release.yml"));
  assert.equal(readFileSync(path, "utf-8"), read("templates", "release.yml"), "the scaffold copies the template verbatim");
});

test("the scaffold creates .github/workflows when only .github exists", () => {
  // An app older than Rails 7.2 may have .github/dependabot.yml and no workflows/.
  const project = scratchProject();
  mkdirSync(join(project, ".github"));
  writeFileSync(join(project, ".github", "dependabot.yml"), "version: 2\n");

  const { written } = writeReleaseWorkflow(project, () => {});

  assert.equal(written, true);
  assert.ok(existsSync(join(project, ".github", "workflows", "release.yml")));
  assert.equal(readFileSync(join(project, ".github", "dependabot.yml"), "utf-8"), "version: 2\n", "neighbours untouched");
});

test("the scaffold never overwrites a release workflow", () => {
  const project = scratchProject();
  const target = join(project, ".github", "workflows", "release.yml");
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, "# mine\n");
  const said = [];

  const { written } = writeReleaseWorkflow(project, (line) => said.push(line));

  assert.equal(written, false);
  assert.equal(readFileSync(target, "utf-8"), "# mine\n", "a hand-written workflow must survive a re-run of init");
  assert.match(said.join("\n"), /release\.yml already exists/, "the user should hear that their file was kept");
  assert.match(said.join("\n"), /templates\/release\.yml/, "and where to find the template if they want it");
});

test("init runs the workflow step and tells the user about tags and secrets", () => {
  const cli = read("cli", "turbo-desktop.js");
  const init = cli.slice(cli.indexOf("function cmdInit("), cli.indexOf("function cmdDev("));

  assert.match(init, /writeReleaseWorkflow\(projectDir\)/, "cmdInit should write the workflow");
  assert.match(init, /git tag v/, "next steps should show how a release starts");
  assert.match(init, /APPLE_\*/, "next steps should mention the signing secrets");
  assert.match(init, /DISTRIBUTION\.md/, "and point at the guide");
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test test/cli.test.js 2>&1 | grep -E "^not ok"`
Expected: `not ok` for the four new tests; the first three with `writeReleaseWorkflow is not a function` or an import error.

- [ ] **Step 3: Implement `writeReleaseWorkflow`**

In `cli/turbo-desktop.js`, in the `// ─── Helpers` section after `guessAppName`, add:

```js
// Copy the release workflow into the Rails app. It goes in the app root, not in
// desktop/, because GitHub only reads workflows from the repository root. A
// workflow that is already there is the user's; leave it alone and say so.
export function writeReleaseWorkflow(projectDir, log = console.log) {
  const template = join(PACKAGE_ROOT, "templates", "release.yml");
  const path = join(resolve(projectDir), ".github", "workflows", "release.yml");

  if (existsSync(path)) {
    log(
      `\n  .github/workflows/release.yml already exists, left alone.\n` +
        `  The scaffold's version is at ${template}`
    );
    return { path, written: false };
  }

  mkdirSync(dirname(path), { recursive: true });
  copyFileSync(template, path);
  return { path, written: true };
}
```

- [ ] **Step 4: Call it from `cmdInit` and extend the next steps**

In `cmdInit`, directly after the `writeFileSync(join(desktopDir, "turbo-desktop.config.json"), ...)` call and before `console.log(\`\nTurbo Desktop initialized successfully!`, add:

```js
  writeReleaseWorkflow(projectDir);
```

Then change the end of the "Next steps" template literal from:

```
  4. Start the desktop app (it starts the Rails server too):
     cd desktop && turbo-desktop dev
`);
```

to:

```
  4. Start the desktop app (it starts the Rails server too):
     cd desktop && turbo-desktop dev

  5. Ship it. .github/workflows/release.yml builds installers for
     macOS, Windows and Linux when you push a tag:
       git tag v0.1.0 && git push origin v0.1.0
     Add the six APPLE_* repository secrets and macOS builds are signed
     and notarized too. See docs/DISTRIBUTION.md in the turbo-desktop repo.
`);
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npm test 2>&1 | tail -8`
Expected: `fail 0`.

Also run `npm run lint` — expected: no errors.

- [ ] **Step 6: Smoke-test the scaffold on disk**

```bash
cd "$(mktemp -d)" && mkdir sample && cd sample && node ~/turbo_desktop/cli/turbo-desktop.js init . 2>&1 | tail -12 && ls .github/workflows && diff -q .github/workflows/release.yml ~/turbo_desktop/templates/release.yml && echo IDENTICAL
```

Expected: next steps print step 5; `release.yml` listed; `IDENTICAL`. (Rust/Rails tools are not needed by `init` without `--icon`.)

- [ ] **Step 7: Commit**

```bash
git add cli/turbo-desktop.js test/cli.test.js
git commit -m "feat(cli): init writes .github/workflows/release.yml into the Rails app

One tag now builds installers for a scaffolded app, and six secrets sign
them, with no workflow to copy and edit by hand. An existing release.yml
is kept and the user told where the template lives."
```

---

### Task 4: Documentation

**Files:**
- Modify: `docs/DISTRIBUTION.md`
- Modify: `README.md:886-897` (Distribution section)
- Test: `test/cli.test.js`

**Interfaces:**
- Consumes: the behaviour from Tasks 1–3.
- Produces: nothing code depends on.

- [ ] **Step 1: Write the failing test**

Append to `test/cli.test.js` after the Task 3 tests:

```js
test("the distribution guide explains every signing secret and no longer says to uncomment", () => {
  const guide = read("docs", "DISTRIBUTION.md");

  for (const secret of [
    "APPLE_CERTIFICATE",
    "APPLE_CERTIFICATE_PASSWORD",
    "APPLE_SIGNING_IDENTITY",
    "APPLE_ID",
    "APPLE_PASSWORD",
    "APPLE_TEAM_ID",
  ]) {
    assert.match(guide, new RegExp(`gh secret set ${secret}`), `the guide should show how to set ${secret}`);
    assert.match(guide, new RegExp(`\\| \`${secret}\` \\|`), `the guide should say where ${secret} comes from`);
  }
  assert.doesNotMatch(guide, /uncomment/i, "signing is automatic now; telling readers to uncomment sends them hunting");
  assert.match(guide, /spctl -a -vv/, "readers need a way to confirm the notarization took");
  assert.match(guide, /templates\/release\.yml/, "existing projects need the template's location");
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test test/cli.test.js 2>&1 | grep -E "^not ok"`
Expected: `not ok` for the new test (the guide currently says "uncomment").

- [ ] **Step 3: Rewrite the affected sections of `docs/DISTRIBUTION.md`**

Replace the section `## Using this in your own app` with:

```markdown
## Using this in your own app

`npx turbo-desktop new myapp` (and `init`) writes the same workflow into your Rails app at
`.github/workflows/release.yml`, pointed at `desktop/`, where the scaffold puts the Tauri project.
Push a tag and you get the draft release above, named after your repository.

Scaffolded before this existed? Copy
[`templates/release.yml`](../templates/release.yml) into your app's `.github/workflows/`.
```

Replace the section `## Signing & notarization (recommended before shipping to real users)` with:

```markdown
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
```

In `## Auto-update (optional)`, replace step 3:

```markdown
3. Add the private key + password as the `TAURI_SIGNING_PRIVATE_KEY` /
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets. The workflow passes them through
   automatically when the key is set.
```

In `## Status`, replace the signing line:

```markdown
- ✅ macOS signing / notarization — automatic once the six `APPLE_*` secrets are set.
- ⚙️ Windows signing — configure in `tauri.conf.json`.
```

Leave the TL;DR, "What ships inside the app", "Building locally" and the rest of Auto-update as they are.

- [ ] **Step 4: Update the README Distribution section**

In `README.md`, replace the paragraph after the `git tag` code block in `## Distribution`:

```markdown
`turbo-desktop new` writes the same workflow into your app. Add six `APPLE_*` repository secrets
and macOS builds come out signed and notarized; nothing in the workflow needs editing. See
**[docs/DISTRIBUTION.md](docs/DISTRIBUTION.md)** for the secrets, local builds and auto-update.
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `npm test 2>&1 | tail -8`
Expected: `fail 0`. The existing test `the READMEs and guides quote the current version` must still pass: the new text quotes no version numbers.

- [ ] **Step 6: Commit**

```bash
git add docs/DISTRIBUTION.md README.md test/cli.test.js
git commit -m "docs: signing is six secrets, with a table of where each comes from

Replaces the uncomment-a-block instructions with the secret names, how to
obtain each value in Xcode, Keychain Access and the Apple developer site,
a gh secret set recipe, and how to verify the notarization."
```

---

### Task 5: Whole-branch check

**Files:** none new.

- [ ] **Step 1: Full suite + lint**

Run: `npm test 2>&1 | tail -8 && npm run lint`
Expected: `fail 0`, lint clean.

- [ ] **Step 2: Rust and gem untouched**

Run: `git diff --stat main -- src-tauri turbo_desktop-rails`
Expected: empty output. This change is CLI, workflow and docs only.

- [ ] **Step 3: Push and open a PR**

```bash
git push -u origin feat/scaffold-release-workflow
gh pr create --title "Scaffold a release workflow that signs macOS builds when secrets exist" --body-file - <<'EOF'
## Why

Notarization was the step most likely to stall a first release: copy the repo workflow, edit three paths, uncomment a block, set six secrets, and don't leave the block uncommented with secrets unset or Tauri imports an empty certificate.

## What

- `turbo-desktop init` / `new` write `.github/workflows/release.yml` into the Rails app (never overwrite).
- Signing turns on by itself when the six `APPLE_*` secrets exist; updater key likewise. `tauri-action` carries only `GITHUB_TOKEN`.
- Partial secrets fail in seconds naming the missing one. Wrapped base64 values survive (heredoc into `GITHUB_ENV`).
- `templates/release.yml` is the single source; a drift test keeps it in step with the repo's own workflow, which uses the same steps.
- `docs/DISTRIBUTION.md`: table of each secret and where it comes from, `gh secret set` recipe, `spctl` verification.

Spec: `docs/superpowers/specs/2026-10-03-scaffold-signed-release-design.md`
Plan: `docs/superpowers/plans/2026-10-03-scaffold-signed-release.md`

## Manual follow-up

Push a tag on this repo with no Apple secrets set and confirm the unsigned build is still green. Signing with real secrets needs the maintainer's Apple account.
EOF
```

Expected: PR URL printed; the five required CI checks start.
