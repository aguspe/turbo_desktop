import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, mkdtempSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  bundleIdentifier,
  desktopPackage,
  urlScheme,
  defaultBuildTarget,
  appConfig,
  extractIconFlag,
  gemConstraint,
  guessAppName,
  packageVersion,
  run,
  writeReleaseWorkflow,
} from "../cli/turbo-desktop.js";

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (...parts) => readFileSync(join(PACKAGE_ROOT, ...parts), "utf-8");

test("extractIconFlag returns the args untouched when --icon is absent", () => {
  const { iconPath, rest } = extractIconFlag(["myapp"]);

  assert.equal(iconPath, null);
  assert.deepEqual(rest, ["myapp"]);
});

test("extractIconFlag pulls the flag out and resolves the path", () => {
  const { iconPath, rest } = extractIconFlag(["myapp", "--icon", "./logo.png"]);

  assert.equal(iconPath, resolve("./logo.png"));
  assert.deepEqual(rest, ["myapp"]);
});

test("extractIconFlag keeps positional args either side of the flag", () => {
  const { iconPath, rest } = extractIconFlag(["--icon", "logo.png", "myapp"]);

  assert.equal(iconPath, resolve("logo.png"));
  assert.deepEqual(rest, ["myapp"]);
});

test("guessAppName turns a directory name into a title", () => {
  assert.equal(guessAppName("/tmp/my_cool-app"), "My Cool App");
});

test("defaultBuildTarget matches the host platform", () => {
  const target = defaultBuildTarget();

  assert.match(target, /^(aarch64|x86_64)-/);
  if (process.platform === "darwin") assert.ok(target.endsWith("-apple-darwin"));
  if (process.platform === "linux") assert.ok(target.endsWith("-unknown-linux-gnu"));
  if (process.platform === "win32") assert.ok(target.endsWith("-pc-windows-msvc"));
});

test("a scaffolded app leaves the user agent to the shell", () => {
  // The shell names the machine it is running on. A user agent written at
  // scaffold time named the machine the scaffold ran on, in every build: an
  // app made on a Mac told Rails it was on macOS when it ran on Windows.
  const config = appConfig("Task Manager");

  assert.equal(config.app_name, "Task Manager");
  assert.ok(!("user_agent" in config), "the scaffold should not fix the user agent");
  assert.equal(config.server.command, "bin/rails server");
  assert.equal(config.sudo.enabled, false);
});

test("run passes arguments through without a shell", () => {
  // A semicolon inside an argument has to stay part of that argument. If the
  // command went through a shell this would run `echo hi` and then `whoami`.
  const result = run("node", ["-e", "process.stdout.write(process.argv[1])", "hi; whoami"], {
    stdio: "pipe",
  });

  assert.equal(result.stdout.toString(), "hi; whoami");
});

test("run surfaces a non-zero exit as an error", () => {
  assert.throws(
    () => run("node", ["-e", "process.exit(3)"], { stdio: "pipe" }),
    /exited with status 3/
  );
});

test("the injected bridge reports the same version as package.json", () => {
  const source = readFileSync(join(PACKAGE_ROOT, "src", "turbo-desktop.js"), "utf-8");
  const match = source.match(/version:\s*"([^"]+)"/);

  assert.ok(match, "src/turbo-desktop.js should declare a version");
  assert.equal(
    match[1],
    packageVersion(),
    "src/turbo-desktop.js version drifted from package.json"
  );
});

test("the Rust crate reports the same version as package.json", () => {
  const cargo = readFileSync(join(PACKAGE_ROOT, "src-tauri", "Cargo.toml"), "utf-8");
  const match = cargo.match(/^version\s*=\s*"([^"]+)"/m);

  assert.ok(match, "Cargo.toml should declare a version");
  assert.equal(match[1], packageVersion(), "Cargo.toml version drifted from package.json");
});

test("the Ruby gem reports the same version as package.json", () => {
  const version = readFileSync(
    join(PACKAGE_ROOT, "turbo_desktop-rails", "lib", "turbo_desktop", "version.rb"),
    "utf-8"
  );
  const match = version.match(/VERSION\s*=\s*"([^"]+)"/);

  assert.ok(match, "version.rb should declare a VERSION");
  assert.equal(match[1], packageVersion(), "the gem version drifted from package.json");
});

test("the scaffold copies every Rust module main.rs declares", () => {
  const cli = readFileSync(join(PACKAGE_ROOT, "cli", "turbo-desktop.js"), "utf-8");
  const main = readFileSync(join(PACKAGE_ROOT, "src-tauri", "src", "main.rs"), "utf-8");

  const declared = [...main.matchAll(/^mod\s+(\w+);/gm)].map((m) => `${m[1]}.rs`);
  const copied = cli.match(/const rustFiles = \[([\s\S]*?)\]/)[1];

  assert.ok(declared.length > 0, "main.rs should declare modules");
  for (const file of declared) {
    assert.ok(
      copied.includes(`"${file}"`),
      `cli scaffold is missing ${file}; the generated project would not compile`
    );
  }
});

test("each app gets its own URL scheme", () => {
  assert.equal(urlScheme("Task Manager"), "task-manager");
  assert.equal(urlScheme("rbenv Manager"), "rbenv-manager");
  assert.equal(urlScheme("My  App!!"), "my-app");
});

test("a scheme always starts with a letter", () => {
  // Schemes may not begin with a digit, and a name can.
  assert.match(urlScheme("1Password Clone"), /^[a-z]/);
  assert.equal(urlScheme("1Password Clone"), "app-1password-clone");
});

test("each app gets its own bundle identifier", () => {
  assert.equal(bundleIdentifier("Task Manager"), "com.task-manager.app");
  assert.notEqual(
    bundleIdentifier("Task Manager"),
    bundleIdentifier("Invoice Tracker"),
    "two apps sharing an identifier would share their stored preferences"
  );
});

test("the shell's own config does not leak its identity into scaffolds", () => {
  const conf = JSON.parse(read("src-tauri", "tauri.conf.json"));
  const cli = read("cli", "turbo-desktop.js");

  // The scaffold must rewrite these rather than copying them.
  for (const key of ["productName", "identifier"]) {
    assert.ok(
      cli.includes(`tauriConf.${key} =`),
      `the scaffold should set ${key} rather than inherit "${conf[key]}"`
    );
  }
  assert.ok(cli.includes('tauriConf.plugins["deep-link"]'));
});

test("a scaffolded project gets its own package identity", () => {
  const shell = JSON.parse(read("package.json"));
  const scaffold = desktopPackage("Task Manager");

  assert.equal(scaffold.name, "task-manager-desktop");
  assert.notEqual(scaffold.name, shell.name, "every app would be called turbo-desktop");
  assert.ok(!scaffold.bin, "a scaffold has no cli/ directory for a bin to point at");
  assert.equal(scaffold.private, true);
});

test("a scaffolded project depends on the published shell", () => {
  const scaffold = desktopPackage("Task Manager");

  assert.equal(scaffold.dependencies["turbo-desktop"], `^${packageVersion()}`);
  assert.ok(scaffold.devDependencies["@tauri-apps/cli"], "turbo-desktop dev shells out to tauri");
});

test("the published package carries everything the scaffold copies", () => {
  const { files } = JSON.parse(read("package.json"));

  // The CLI copies these out of the installed package at scaffold time.
  for (const needed of ["cli", "src", "src-tauri/src", "src-tauri/capabilities"]) {
    assert.ok(files.includes(needed), `files must include ${needed}`);
  }
  assert.ok(
    files.some((f) => f.startsWith("src-tauri/Cargo.toml")),
    "the scaffolded project needs a Cargo.toml"
  );
});

test("missing prerequisites stop before anything is created", () => {
  // An empty PATH makes every prerequisite unavailable.
  const result = spawnSync(
    process.execPath,
    [join(PACKAGE_ROOT, "cli", "turbo-desktop.js"), "new", "scratch-app"],
    { cwd: mkdtempSync(join(tmpdir(), "turbo-desktop-preflight-")), env: { PATH: "" }, encoding: "utf-8" }
  );

  assert.equal(result.status, 1, "a missing prerequisite should be a clean exit, not a crash");
  assert.match(result.stderr, /Rails is not installed/);
  assert.doesNotMatch(
    result.stderr,
    /at \w+ \(node:/,
    "a stack trace buries the thing the reader needs to see"
  );
});

test("a scaffolded app pins the gem to the shell's minor version", () => {
  assert.equal(gemConstraint("0.2.2"), "~> 0.2");
  assert.equal(gemConstraint("1.4.0"), "~> 1.4");
});

test("the gem constraint follows package.json by default", () => {
  const [major, minor] = packageVersion().split(".");

  assert.equal(gemConstraint(), `~> ${major}.${minor}`);
});

test("the CLI carries no hard-coded gem constraint", () => {
  const source = read("cli", "turbo-desktop.js");

  assert.doesNotMatch(
    source,
    /turbo_desktop-rails["'],\s*["']~>\s*\d/,
    "a literal constraint goes stale at the next minor release; use gemConstraint()"
  );
  assert.doesNotMatch(
    source,
    /turbo_desktop-rails['"],\s*path:/,
    "the next-steps text should point at the published gem, not a local path"
  );
});

test("tauri.conf.json reports the same version as package.json", () => {
  const conf = JSON.parse(read("src-tauri", "tauri.conf.json"));

  assert.equal(conf.version, packageVersion(), "tauri.conf.json version drifted from package.json");
});

test("the documentation pages quote the current version", () => {
  for (const page of [["docs", "index.html"], ["site", "index.html"]]) {
    const quoted = [...read(...page).matchAll(/(?:Turbo Desktop\/|\bv)(\d+\.\d+\.\d+)/g)].map(
      (match) => match[1]
    );

    assert.ok(quoted.length > 0, `${page.join("/")} should quote a version`);
    for (const version of quoted) {
      assert.equal(version, packageVersion(), `${page.join("/")} quotes ${version}`);
    }
  }
});

const READMES = [["README.md"], ["turbo_desktop-rails", "README.md"]];
const GUIDES = [...READMES, ["docs", "DISTRIBUTION.md"], ["docs", "RELEASING.md"]];

test("the READMEs tell readers to pin the gem the way the CLI does", () => {
  for (const readme of READMES) {
    const lines = read(...readme)
      .split("\n")
      .filter((line) => /^\s*gem ["']turbo_desktop-rails["']/.test(line));

    assert.ok(lines.length > 0, `${readme.join("/")} should show the Gemfile line`);
    for (const line of lines) {
      assert.ok(
        line.includes(`"${gemConstraint()}"`),
        `${readme.join("/")} shows \`${line.trim()}\`; unpinned, Bundler can resolve to an ancient version`
      );
    }
  }
});

test("the READMEs and guides quote the current version", () => {
  for (const readme of GUIDES) {
    const quoted = [...read(...readme).matchAll(/(?:Turbo Desktop\/|\bv)(\d+\.\d+\.\d+)/g)].map(
      (match) => match[1]
    );

    for (const version of quoted) {
      assert.equal(version, packageVersion(), `${readme.join("/")} quotes ${version}`);
    }
  }
});

test("the gem's README states the Ruby version the gemspec requires", () => {
  const gemspec = read("turbo_desktop-rails", "turbo_desktop-rails.gemspec");
  const required = gemspec.match(/required_ruby_version\s*=\s*">=\s*(\d+\.\d+)/);
  const stated = read("turbo_desktop-rails", "README.md").match(/Ruby >= (\d+\.\d+)/);

  assert.ok(required, "the gemspec should declare required_ruby_version");
  assert.ok(stated, "the gem's README should state a Ruby version");
  assert.equal(stated[1], required[1], "the README and the gemspec disagree about the Ruby floor");
});

test("the README's Docs link opens the documentation, not the README again", () => {
  const link = read("README.md").match(/<a href="([^"]+)">Docs<\/a>/);

  assert.ok(link, "README.md should link to the docs");
  assert.equal(link[1], "https://aguspe.github.io/turbo_desktop/docs/");
});

test("the release guide covers every place a release has to reach", () => {
  const guide = read("docs", "RELEASING.md");

  // 0.2.1 was tagged and announced but never reached either registry, because
  // nothing written down said that a tag alone publishes neither.
  for (const step of ["gem push", "npm publish", "git tag", "turbo_desktop_site"]) {
    assert.ok(guide.includes(step), `docs/RELEASING.md should cover \`${step}\``);
  }
});

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

const WORKFLOWS = [
  [".github", "workflows", "release.yml"],
  ["templates", "release.yml"],
];

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

// ─── What the documentation and the types promise ────────────────────────────

test("the README mounts the engine rather than routing to a controller that does not exist", () => {
  const readme = read("README.md");

  assert.doesNotMatch(readme, /to:\s*"turbo_desktop#path_configuration"/);
  assert.match(readme, /mount TurboDesktop::Engine => "\/turbo-desktop"/);
});

test("the README lists every bridge component the shell dispatches", () => {
  const dispatch = read("src-tauri", "src", "bridge.rs").match(
    /match message\.component\.as_str\(\) \{([\s\S]*?)\n {8}_ =>/
  );
  assert.ok(dispatch, "bridge.rs should dispatch on the component name");

  const components = [...dispatch[1].matchAll(/^ {8}"([a-z-]+)" =>/gm)].map((match) => match[1]);
  assert.ok(components.length >= 10, `expected the full dispatch table, found ${components}`);

  const table = read("README.md").match(/### Built-in Components([\s\S]*?)\n### /)[1];
  for (const component of components) {
    assert.ok(table.includes(`\`${component}\``), `README.md does not list \`${component}\``);
  }
});

// The public members of the object the shell injects, and of each namespace on
// it. Members starting with an underscore are internal.
function runtimeApi() {
  const source = read("src", "turbo-desktop.js");
  const body = source.slice(
    source.indexOf("const TurboDesktop = {"),
    source.indexOf("\n  };", source.indexOf("const TurboDesktop = {"))
  );
  const keywords = new Set(["if", "for", "while", "switch", "return", "catch"]);
  const members = (indent) =>
    [...body.matchAll(new RegExp(`^ {${indent}}(?:async |get )?([a-zA-Z]\\w*)\\s*[(:]`, "gm"))]
      .map((match) => match[1])
      .filter((name) => !keywords.has(name));

  return { body, topLevel: members(4), nested: members(6) };
}

test("the type definitions declare every member of the runtime API", () => {
  const types = read("src", "turbo-desktop.d.ts");
  const api = types.slice(types.indexOf("export interface TurboDesktopAPI"));
  const { topLevel, nested } = runtimeApi();

  assert.ok(topLevel.includes("proposeVisit"), "the runtime API was not found in turbo-desktop.js");

  const missing = [...new Set([...topLevel, ...nested])].filter(
    (name) => !new RegExp(`\\b${name}\\??\\s*[(:<]`).test(api)
  );
  assert.deepEqual(missing, [], "turbo-desktop.d.ts is missing members the runtime has");
});

test("the bridge package carries the same version as the shell", () => {
  const bridge = JSON.parse(read("packages", "bridge", "package.json"));

  assert.equal(bridge.version, packageVersion(), "packages/bridge drifted from package.json");
});

test("the bridge package ships every file its types import", () => {
  const bridge = JSON.parse(read("packages", "bridge", "package.json"));
  const imports = [...read("packages", "bridge", "index.d.ts").matchAll(/from "(\.[^"]+)"/g)].map(
    (match) => match[1]
  );

  assert.ok(imports.length > 0, "index.d.ts should import the shared types");
  for (const path of imports) {
    assert.ok(!path.startsWith(".."), `index.d.ts imports ${path}, which is outside the package`);
    assert.match(path, /\.js$/, `index.d.ts imports ${path} without an extension, which node16 refuses`);

    const declarations = path.replace(/^\.\//, "").replace(/\.js$/, ".d.ts");
    assert.ok(
      bridge.files.includes(declarations),
      `${declarations} is imported but not listed in "files", so it is not published`
    );
  }
});

test("the bridge package's copy of the types matches the shell's", () => {
  assert.equal(
    read("packages", "bridge", "turbo-desktop.d.ts"),
    read("src", "turbo-desktop.d.ts"),
    "packages/bridge/turbo-desktop.d.ts drifted; copy src/turbo-desktop.d.ts over it"
  );
});

// Most apps resolve modules one of these two ways. Under node16 and nodenext a
// relative import has to name its extension, which the types once did not.
for (const [module, resolution] of [
  ["esnext", "bundler"],
  ["nodenext", "nodenext"],
]) {
  test(`the published types compile against the documented usage (${resolution})`, () => {
    const result = spawnSync(
      process.execPath,
      [
        join(PACKAGE_ROOT, "node_modules", "typescript", "bin", "tsc"),
        "--noEmit",
        "--strict",
        "--target", "es2022",
        "--lib", "es2022,dom",
        "--module", module,
        "--moduleResolution", resolution,
        join(PACKAGE_ROOT, "fixtures", "types", "usage.mts"),
      ],
      { encoding: "utf-8" }
    );

    assert.equal(result.status, 0, `the types do not compile:\n${result.stdout}${result.stderr}`);
  });
}
