import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  defaultBuildTarget,
  defaultUserAgent,
  extractIconFlag,
  guessAppName,
  packageVersion,
  run,
} from "../cli/turbo-desktop.js";

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

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

test("defaultUserAgent reports the running version and platform", () => {
  const ua = defaultUserAgent();

  assert.ok(
    ua.startsWith(`Turbo Desktop/${packageVersion()}`),
    `user agent should carry the package version, got: ${ua}`
  );
  assert.doesNotMatch(ua, /undefined/);

  if (process.platform === "darwin") assert.match(ua, /\(macOS; /);
  if (process.platform === "linux") assert.match(ua, /\(Linux; /);
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
