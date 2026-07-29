// End-to-end tests: the real shell binary, driven over WebDriver.
//
// tauri-driver bridges WebDriver to the platform webview driver, which exists
// on Linux (WebKitWebDriver) and Windows (msedgedriver) — not macOS. CI runs
// this on ubuntu under xvfb; locally it needs a Linux machine.
//
// Native dialogs cannot be driven by WebDriver, so the shell's debug build
// accepts TURBO_DESKTOP_E2E_PICKER as "what the user would have picked" —
// everything after the dialog (grants, filesystem, events) is the real path.
import { test, before, after } from "node:test";
import assert from "node:assert";
import { spawn } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  existsSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { remote } from "webdriverio";
import { startFixtureServer } from "./fixture/server.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const binary = join(
  repoRoot,
  "src-tauri",
  "target",
  "debug",
  process.platform === "win32" ? "turbo-desktop.exe" : "turbo-desktop"
);

const scratch = mkdtempSync(join(tmpdir(), "turbo-desktop-e2e-"));
const pickedFile = join(scratch, "picked", "report.txt");

let server;
let driver;
let browser;

async function waitFor(check, { tries = 60, delayMs = 500, label = "condition" } = {}) {
  for (let i = 0; i < tries; i++) {
    try {
      const value = await check();
      if (value) return value;
    } catch {
      // Not up yet.
    }
    await new Promise((r) => setTimeout(r, delayMs));
  }
  throw new Error(`Timed out waiting for ${label}`);
}

before(async () => {
  mkdirSync(dirname(pickedFile), { recursive: true });
  server = await startFixtureServer(3210);

  driver = spawn("tauri-driver", [], {
    cwd: join(here, "fixture"),
    env: {
      ...process.env,
      TURBO_DESKTOP_E2E_PICKER: pickedFile,
    },
    stdio: "inherit",
  });

  browser = await waitFor(
    () =>
      remote({
        hostname: "127.0.0.1",
        port: 4444,
        logLevel: "warn",
        capabilities: {
          "tauri:options": { application: binary },
        },
      }),
    { label: "a WebDriver session", tries: 30, delayMs: 1000 }
  );

  // The shell injects turbo-desktop.js once the fixture page finishes loading.
  await waitFor(
    () => browser.execute(() => Boolean(window.__TURBO_DESKTOP__)),
    { label: "the injected bridge" }
  );
});

after(async () => {
  try {
    if (browser) await browser.deleteSession();
  } finally {
    if (driver) driver.kill();
    if (server) server.close();
    rmSync(scratch, { recursive: true, force: true });
  }
});

test("the shell loads the app server's page", async () => {
  const heading = await browser.execute(
    () => document.getElementById("heading")?.textContent
  );
  assert.strictEqual(heading, "Turbo Desktop E2E fixture");
});

test("the bridge announces itself to the page", async () => {
  const bridge = await browser.execute(() => ({
    isNative: window.__TURBO_DESKTOP__.isNative,
    hasFs: typeof window.__TURBO_DESKTOP__.fs.write === "function",
    hasClipboard:
      typeof window.__TURBO_DESKTOP__.clipboard.writeText === "function",
  }));
  assert.deepStrictEqual(bridge, {
    isNative: true,
    hasFs: true,
    hasClipboard: true,
  });
});

test("clipboard text survives a write/read round trip", async () => {
  const text = await browser.executeAsync((done) => {
    const td = window.__TURBO_DESKTOP__;
    td.clipboard
      .writeText("e2e-clipboard-payload")
      .then(() => td.clipboard.readText())
      .then(done)
      .catch((e) => done(`error: ${e}`));
  });
  assert.strictEqual(text, "e2e-clipboard-payload");
});

test("a save-dialog pick makes the path writable and readable", async () => {
  // The config allows no filesystem roots at all, so this only works if the
  // picker consent grant is honoured end to end.
  const result = await browser.executeAsync((done) => {
    const td = window.__TURBO_DESKTOP__;
    td.sendBridgeMessage("file-picker", "save", { title: "Save report" })
      .then((picked) =>
        td.fs
          .write(picked.path, "written through the bridge")
          .then((write) => td.fs.read(picked.path).then((read) => done({
            path: picked.path,
            writeStatus: write.status,
            readBack: read.content,
          })))
      )
      .catch((e) => done(`error: ${e}`));
  });

  assert.strictEqual(result.path, pickedFile);
  assert.strictEqual(result.writeStatus, "ok");
  assert.strictEqual(result.readBack, "written through the bridge");

  // And it genuinely landed on disk, not in a mock.
  assert.ok(existsSync(pickedFile));
  assert.strictEqual(readFileSync(pickedFile, "utf8"), "written through the bridge");
});

test("a path nobody picked is still refused", async () => {
  const result = await browser.executeAsync((done) => {
    window.__TURBO_DESKTOP__.fs
      .write("/tmp/turbo-desktop-e2e-unpicked.txt", "nope")
      .then((r) => done(r))
      .catch((e) => done(`error: ${e}`));
  });
  // The bridge surfaces policy refusals as errors; the exact shape is a null
  // result from the JS wrapper (which logs the rejection).
  assert.ok(
    result === null || String(result).startsWith("error:"),
    `expected a refusal, got ${JSON.stringify(result)}`
  );
});

test("a shell command streams its output back", async () => {
  // Returns the whole observed state rather than just the lines, so a
  // failure says which link broke: the event system, the spawn, or delivery.
  const result = await browser.executeAsync((done) => {
    const td = window.__TURBO_DESKTOP__;
    const state = {
      canListen: Boolean(window.__TAURI_INTERNALS__?.event?.listen),
      spawn: null,
      lines: [],
      exit: null,
      finishedBy: null,
    };
    const finish = (how) => {
      state.finishedBy = how;
      td.shell.offOutput("e2e-echo");
      done(state);
    };
    const guard = setTimeout(() => finish("timeout"), 15000);

    td.shell.onOutput("e2e-echo", (message) => {
      if (message.event === "stdout") state.lines.push(message.line);
      if (message.event === "exit") {
        state.exit = message.code;
        clearTimeout(guard);
        finish("exit-event");
      }
    });
    td.shell.spawn("e2e-echo", "echo", ["hello-from-e2e"]).then((r) => {
      state.spawn = r;
    });
  });

  const detail = JSON.stringify(result);
  assert.strictEqual(result.canListen, true, detail);
  assert.strictEqual(result.spawn?.status, "spawned", detail);
  assert.deepStrictEqual(result.lines, ["hello-from-e2e"], detail);
  assert.strictEqual(result.finishedBy, "exit-event", detail);
});
