// End-to-end tests: the real shell binary showing a real Turbo app.
//
// Where bridge.e2e.mjs exercises the bridge against a static page, this one
// exercises what the documentation promises about an app: that opening it
// starts the server, that path configuration decides how a link is presented,
// that a modal can dismiss itself, that the app survives its server going
// away. Each of these worked in the unit tests while it was broken in a window.
//
// Runs where tauri-driver does: Linux and Windows, not macOS. See
// bridge.e2e.mjs for why.
import { test, before, after } from "node:test";
import assert from "node:assert";
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { remote } from "webdriverio";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const binary = join(
  repoRoot,
  "src-tauri",
  "target",
  "debug",
  process.platform === "win32" ? "turbo-desktop.exe" : "turbo-desktop"
);
const { version } = JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf-8"));

const PORT = 3211;
const ORIGIN = `http://localhost:${PORT}`;
const BANNER = "turbo-desktop-offline-overlay";

// The shell reads its config from the directory it is started in.
const scratch = mkdtempSync(join(tmpdir(), "turbo-desktop-e2e-app-"));
writeFileSync(
  join(scratch, "turbo-desktop.config.json"),
  JSON.stringify(
    {
      server_url: ORIGIN,
      app_name: "E2E App",
      window: { width: 900, height: 700, min_width: 400, min_height: 300, resizable: true },
      filesystem: { allowed_roots: [] },
      sudo: { enabled: false, allowed_commands: [], confirm: true },
      navigation: { internal_hosts: [] },
      // Nothing else starts this server. If the page loads, the shell did.
      server: {
        command: `"${process.execPath}" "${join(here, "app", "serve.mjs")}"`,
        directory: ".",
      },
    },
    null,
    2
  )
);

let driver;
let browser;
let main;

async function waitFor(check, { tries = 60, delayMs = 500, label = "condition" } = {}) {
  let last;
  for (let i = 0; i < tries; i++) {
    try {
      const value = await check();
      if (value) return value;
    } catch (error) {
      last = error;
    }
    await new Promise((r) => setTimeout(r, delayMs));
  }
  throw new Error(`Timed out waiting for ${label}${last ? `: ${last.message}` : ""}`);
}

const pause = (ms) => new Promise((r) => setTimeout(r, ms));

async function requestsSeen() {
  const response = await fetch(`${ORIGIN}/__requests`);
  return response.json();
}

async function serverAnswers() {
  try {
    return (await fetch(`${ORIGIN}/up`)).ok;
  } catch {
    return false;
  }
}

async function bridgeReady() {
  return browser.execute(() => Boolean(window.__TURBO_DESKTOP__ && window.Turbo));
}

async function path() {
  return new URL(await browser.getUrl()).pathname;
}

async function click(id) {
  const link = await browser.$(`#${id}`);
  await link.click();
}

async function windows() {
  return browser.getWindowHandles();
}

/** The one window that is not the main one. */
async function otherWindow() {
  const handles = await waitFor(async () => {
    const all = await windows();
    return all.length === 2 ? all : null;
  }, { label: "a second window" });

  return handles.find((handle) => handle !== main);
}

async function onlyMainIsOpen() {
  await waitFor(async () => (await windows()).length === 1, { label: "the second window to close" });
  await browser.switchToWindow(main);
}

/** Back to the home page in the main window, with nothing else open. */
async function startOver() {
  for (const handle of await windows()) {
    if (handle === main) continue;
    await browser.switchToWindow(handle);
    await browser.closeWindow();
  }
  await browser.switchToWindow(main);
  await browser.url(`${ORIGIN}/`);
  await waitFor(bridgeReady, { label: "the bridge on the home page" });
}

before(async () => {
  assert.equal(await serverAnswers(), false, `something is already listening on ${ORIGIN}`);

  driver = spawn("tauri-driver", [], {
    cwd: scratch,
    // The shell's own account of what it did, for when a test fails.
    env: { ...process.env, RUST_LOG: process.env.RUST_LOG || "turbo_desktop=info" },
    stdio: "inherit",
  });

  browser = await waitFor(
    () =>
      remote({
        hostname: "127.0.0.1",
        port: 4444,
        logLevel: "warn",
        capabilities: { "tauri:options": { application: binary } },
      }),
    { label: "a WebDriver session", tries: 30, delayMs: 1000 }
  );

  // The shell opens on its waiting page, starts the server, and moves to the
  // app when the server answers.
  await waitFor(async () => (await browser.getUrl()).startsWith(ORIGIN) && (await bridgeReady()), {
    label: "the app, served by a server the shell started",
    tries: 90,
  });

  main = await browser.getWindowHandle();
});

after(async () => {
  try {
    if (browser) await browser.deleteSession().catch(() => {});
  } finally {
    if (driver) driver.kill();
    rmSync(scratch, { recursive: true, force: true });
  }
});

// ─── Opening the app ─────────────────────────────────────────────────────────

test("opening the app starts its server", async () => {
  assert.equal(await serverAnswers(), true);
  assert.equal(await browser.getTitle(), "Home");
});

test("the server sees requests as coming from the desktop app", async () => {
  const fromShell = (await requestsSeen()).filter((request) => request.path === "/");

  assert.ok(fromShell.length > 0, "the shell never asked for the home page");
  assert.match(fromShell[0].userAgent, new RegExp(`Turbo Desktop/${version.replaceAll(".", "\\.")} \\(`));
});

test("the rules are fetched once the server the app started is up", async () => {
  const asked = (await requestsSeen()).filter(
    (request) => request.path === "/turbo-desktop/path-configuration.json"
  );

  assert.ok(asked.length > 0, "the shell never fetched the path configuration");
  assert.match(asked[0].userAgent, /Turbo Desktop/);
});

test("the bridge is there before the page's own scripts run", async () => {
  assert.equal(
    await browser.execute(() => window.__bridgeWhenThePageRan),
    true,
    "a controller's connect() would find no TurboDesktop on the first page"
  );
  assert.equal(await browser.execute(() => window.__heardReady), true);
  assert.equal(await browser.execute(() => window.TurboDesktop.ready), true);
});

test("the bridge is there from the start in a modal too", async () => {
  await startOver();
  await click("to-new-task");
  await browser.switchToWindow(await otherWindow());
  await waitFor(bridgeReady, { label: "the bridge in the modal" });

  assert.equal(await browser.execute(() => window.__bridgeWhenThePageRan), true);
  await startOver();
});

test("the page knows which platform it is on", async () => {
  const platform = await browser.execute(() => window.TurboDesktop.platform);
  const expected = { linux: "linux", win32: "windows", darwin: "macos" }[process.platform];

  assert.equal(platform, expected);
});

test("the main window is not a modal", async () => {
  const state = await browser.execute(() => ({
    isModal: window.TurboDesktop.isModal,
    label: window.TurboDesktop.windowLabel,
  }));

  assert.deepEqual(state, { isModal: false, label: "main" });
});

// ─── Path configuration ──────────────────────────────────────────────────────

test("an ordinary link navigates the window it is in", async () => {
  await startOver();

  await click("to-tasks");

  await waitFor(async () => (await path()) === "/tasks", { label: "the tasks page" });
  assert.equal(await browser.getTitle(), "Tasks");
  assert.equal((await windows()).length, 1);
});

test("navigating with Turbo does not reload the page", async () => {
  await startOver();
  await browser.execute(() => {
    window.__sameDocument = true;
  });

  await click("to-tasks");
  await waitFor(async () => (await path()) === "/tasks", { label: "the tasks page" });

  assert.equal(
    await browser.execute(() => window.__sameDocument === true),
    true,
    "the visit was a full page load, not a Turbo visit"
  );
});

test("a link can ask to replace the history entry", async () => {
  await startOver();
  await click("to-tasks");
  await waitFor(async () => (await path()) === "/tasks", { label: "the tasks page" });
  const before = await browser.execute(() => window.history.length);

  await click("to-tasks-replacing");
  await waitFor(async () => (await browser.getUrl()).endsWith("/tasks?page=2"), { label: "page 2" });

  assert.equal(await browser.execute(() => window.history.length), before);
});

test("a modal rule opens a modal and leaves the main window where it was", async () => {
  await startOver();

  await click("to-new-task");
  const modal = await otherWindow();

  // The main window first: it must not have followed the link as well.
  await pause(1500);
  assert.equal(await path(), "/", "the main window navigated to the page the modal shows");

  await browser.switchToWindow(modal);
  await waitFor(bridgeReady, { label: "the bridge in the modal" });
  assert.equal(await path(), "/tasks/new");
  assert.equal(await browser.getTitle(), "New task");

  const state = await browser.execute(() => ({
    isModal: window.TurboDesktop.isModal,
    label: window.TurboDesktop.windowLabel,
  }));
  assert.equal(state.isModal, true);
  assert.match(state.label, /^modal-/);
});

test("a modal is sized by its rule", async () => {
  await startOver();
  await click("to-edit-task");
  await browser.switchToWindow(await otherWindow());
  await waitFor(bridgeReady, { label: "the bridge in the modal" });

  const info = await browser.execute(() => window.TurboDesktop.getWindowInfo());

  assert.equal(Math.round(info.width / info.scaleFactor), 500);
  assert.equal(Math.round(info.height / info.scaleFactor), 400);
});

test("a modal talks to the server as the desktop app", async () => {
  const forModal = (await requestsSeen()).filter((request) => request.path === "/tasks/new");

  assert.ok(forModal.length > 0, "the modal's page was never requested");
  for (const request of forModal) assert.match(request.userAgent, /Turbo Desktop/);
});

for (const [dismissal, reloads] of [
  ["refresh", true],
  ["resume", false],
]) {
  test(`${dismissal}() closes the modal and ${reloads ? "reloads" : "leaves"} the page underneath`, async () => {
    await startOver();
    await click("to-tasks");
    await waitFor(async () => (await path()) === "/tasks", { label: "the tasks page" });
    const loadsBefore = (await requestsSeen()).filter((request) => request.path === "/tasks").length;

    await click("to-new-task");
    await browser.switchToWindow(await otherWindow());
    await waitFor(bridgeReady, { label: "the bridge in the modal" });
    // The window closes under the call, so the call itself may not return.
    await browser
      .execute((how) => {
        window.TurboDesktop[how]();
      }, dismissal)
      .catch(() => {});

    await onlyMainIsOpen();
    await pause(1500);
    assert.equal(await path(), "/tasks");

    const loadsAfter = (await requestsSeen()).filter((request) => request.path === "/tasks").length;
    assert.equal(loadsAfter > loadsBefore, reloads);
  });
}

test("recede() closes the modal and goes back underneath", async () => {
  await startOver();
  await click("to-tasks");
  await waitFor(async () => (await path()) === "/tasks", { label: "the tasks page" });

  await click("to-new-task");
  await browser.switchToWindow(await otherWindow());
  await waitFor(bridgeReady, { label: "the bridge in the modal" });
  await browser
    .execute(() => {
      window.TurboDesktop.recede();
    })
    .catch(() => {});

  await onlyMainIsOpen();
  await waitFor(async () => (await path()) === "/", { label: "the page before" });
});

test("closeModal() closes the window the page is in", async () => {
  await startOver();
  await click("to-new-task");
  await browser.switchToWindow(await otherWindow());
  await waitFor(bridgeReady, { label: "the bridge in the modal" });

  await browser
    .execute(() => {
      window.TurboDesktop.closeModal();
    })
    .catch(() => {});

  await onlyMainIsOpen();
  assert.equal(await path(), "/");
});

test("a new_window rule opens a window of its own", async () => {
  await startOver();

  await click("to-report");
  const report = await otherWindow();

  await pause(1500);
  assert.equal(await path(), "/", "the main window navigated as well");

  await browser.switchToWindow(report);
  await waitFor(bridgeReady, { label: "the bridge in the new window" });
  assert.equal(await path(), "/reports/1");

  const state = await browser.execute(() => ({
    isModal: window.TurboDesktop.isModal,
    label: window.TurboDesktop.windowLabel,
  }));
  assert.equal(state.isModal, false);
  assert.match(state.label, /^window-/);
});

test("a replace rule shows the page without adding to the history", async () => {
  await startOver();
  await click("to-tasks");
  await waitFor(async () => (await path()) === "/tasks", { label: "the tasks page" });
  const before = await browser.execute(() => window.history.length);

  await click("to-dashboard");
  await waitFor(async () => (await path()) === "/dashboard", { label: "the dashboard" });

  assert.equal(await browser.execute(() => window.history.length), before);
  assert.equal((await windows()).length, 1);
});

test("a rule of none leaves the link to a bridge component", async () => {
  await startOver();

  await click("to-component");
  await pause(2000);

  assert.equal(await path(), "/");
  assert.equal((await windows()).length, 1);
  assert.equal(
    (await requestsSeen()).some((request) => request.path === "/handled-by-a-component"),
    false,
    "the page was requested although the rule said to do nothing"
  );
});

test("the window title follows the page", async () => {
  await startOver();
  await click("to-tasks");
  await waitFor(async () => (await browser.getTitle()) === "Tasks", { label: "the page title" });

  // Set over IPC after the page loads.
  const info = await waitFor(
    async () => {
      const window_ = await browser.execute(() => window.TurboDesktop.getWindowInfo());
      return window_ && window_.label === "main" ? window_ : null;
    },
    { label: "the main window's details" }
  );
  assert.equal(info.label, "main");
  assert.equal(info.development, true, "the tests run a debug build");
});

// ─── Links ───────────────────────────────────────────────────────────────────

test("a link to another site leaves the app where it is", async () => {
  await startOver();

  await click("to-elsewhere");
  await pause(2500);

  assert.ok((await browser.getUrl()).startsWith(ORIGIN), "the app was replaced by the other site");
  assert.equal((await windows()).length, 1);
});

// ─── The bridge ──────────────────────────────────────────────────────────────

test("two components on a page each speak for themselves", async () => {
  await startOver();

  const names = await browser.executeAsync((done) => {
    class Base {
      constructor(element) {
        this.element = element;
      }
      connect() {}
      disconnect() {}
    }
    const Notify = window.TurboDesktop.stimulusBridge(Base, "notification");
    const Clip = window.TurboDesktop.stimulusBridge(Base, "clipboard");
    const notify = new Notify(document.body);
    const clip = new Clip(document.body);
    notify.connect();
    clip.connect();

    done([notify._bridge.constructor.component, clip._bridge.constructor.component]);
  });

  assert.deepEqual(names, ["notification", "clipboard"]);
});

async function bridge(component, event, data = {}) {
  // As a list: WebDriver takes a returned object with an `error` key for a
  // failure of its own, and a component that cannot do its job says so there.
  const [status, rest] = await browser.executeAsync(
    (component, event, data, done) => {
      window.TurboDesktop.sendBridgeMessage(component, event, data).then((response) => {
        if (!response) return done([null, {}]);
        const { status, error: reason, ...rest } = response;
        done([status, { ...rest, reason }]);
      });
    },
    component,
    event,
    data
  );

  return { status, ...rest };
}

// The machine running this may have nothing to show a notification with, or
// no dock to badge. What is checked is that the shell tried, rather than
// answering "ok" and doing nothing, which is what these four used to do.
test("a notification is shown, or the page is told it cannot be", async () => {
  await startOver();

  const response = await bridge("notification", "show", { title: "E2E", body: "Hello" });

  assert.ok(["shown", "unavailable"].includes(response.status), JSON.stringify(response));
});

test("a component saying goodbye is not shown as a notification", async () => {
  assert.equal((await bridge("notification", "disconnect", {})).status, "ignored");
  assert.equal((await bridge("notification", "connect", {})).status, "ignored");
});

test("the badge is set and cleared", async () => {
  const set = await bridge("badge", "set", { count: 3 });
  assert.ok(["updated", "unavailable"].includes(set.status), JSON.stringify(set));
  if (set.status === "updated") assert.equal(set.count, 3);

  const cleared = await bridge("badge", "set", { count: 0 });
  if (cleared.status === "updated") assert.equal(cleared.count, 0);
});

test("a menu item a page registers is in the menu bar", async () => {
  const registered = await bridge("menu-item", "connect", {
    id: "export",
    title: "Export PDF",
    shortcut: "CmdOrCtrl+E",
  });
  assert.equal(registered.status, "registered", JSON.stringify(registered));

  const { items } = await bridge("menu-item", "list");
  assert.deepEqual(items, [{ id: "export", title: "Export PDF" }]);
});

test("registering a menu item again replaces it", async () => {
  await bridge("menu-item", "connect", { id: "export", title: "Export as PDF" });

  const { items } = await bridge("menu-item", "list");
  assert.deepEqual(items, [{ id: "export", title: "Export as PDF" }]);
});

test("a menu item can be taken away", async () => {
  await bridge("menu-item", "unregister", { id: "export" });

  const { items } = await bridge("menu-item", "list");
  assert.deepEqual(items, []);
});

test("an element that declares a menu item gets one, for as long as its page is shown", async () => {
  await startOver();
  await click("to-declared");
  await waitFor(async () => (await path()) === "/declared", { label: "the page that declares it" });

  await waitFor(
    async () => (await bridge("menu-item", "list")).items.some((item) => item.id === "Export PDF"),
    { label: "the declared item to reach the menu bar" }
  );

  await click("to-tasks");
  await waitFor(async () => (await path()) === "/tasks", { label: "the tasks page" });

  await waitFor(async () => (await bridge("menu-item", "list")).items.length === 0, {
    label: "the item to leave with its page",
  });
});

test("a global shortcut is registered, or the page is told it cannot be", async () => {
  const response = await bridge("shortcut", "register", {
    id: "quick-add",
    accelerator: "CmdOrCtrl+Shift+K",
  });

  assert.ok(["registered", "unavailable"].includes(response.status), JSON.stringify(response));
  await bridge("shortcut", "unregister", { accelerator: "CmdOrCtrl+Shift+K" });
});

test("something that is not a shortcut is refused", async () => {
  const response = await bridge("shortcut", "register", { id: "bad", accelerator: "Banana+Q" });

  assert.equal(response.status, null, "the shell accepted a shortcut that does not parse");
});

test("launch at login can be turned on and off", async () => {
  const states = await browser.executeAsync((done) => {
    const td = window.TurboDesktop;
    const seen = [];

    td.autostart
      .enable()
      .then(() => td.autostart.isEnabled())
      .then((enabled) => seen.push(enabled))
      .then(() => td.autostart.disable())
      .then(() => td.autostart.isEnabled())
      .then((enabled) => seen.push(enabled))
      .then(() => done(seen))
      .catch((error) => done([String(error)]));
  });

  assert.deepEqual(states, [true, false]);
});

test("the shell has nothing queued when no file opened the app", async () => {
  const pending = await browser.executeAsync((done) => {
    window.TurboDesktop.sendBridgeMessage("file-open", "pending", {}).then(done);
  });

  assert.deepEqual(pending.paths, []);
});

test("the filesystem is closed outside the app's own directory", async () => {
  const result = await browser.executeAsync((done) => {
    window.TurboDesktop.fs.read("/etc/hostname").then(done);
  });

  assert.notEqual(result && result.status, "ok", "a path nobody granted was read");
});

test("sudo is off unless the app turns it on", async () => {
  const result = await browser.executeAsync((done) => {
    window.TurboDesktop.sudo.execute("whoami").then(done, (error) => done({ error: String(error) }));
  });

  assert.notEqual(result && result.status, "ok", "a privileged command ran with sudo disabled");
});

// ─── Dev Inspector ───────────────────────────────────────────────────────────

test("the Dev Inspector loads and opens with its shortcut", async () => {
  await startOver();

  try {
    await waitFor(
      () => browser.execute(() => Boolean(document.querySelector("[data-turbo-desktop-inspector]"))),
      { label: "the inspector to mount", tries: 20 }
    );
  } catch (error) {
    // Why not, in the page's own words.
    const why = await browser.executeAsync((done) => {
      const td = window.TurboDesktop;
      const report = [
        `wanted=${td._inspectorWanted}`,
        `ready=${td.ready}`,
        `error=${td._inspectorError}`,
        `body=${Boolean(document.body)}`,
      ];
      const timer = setTimeout(() => done([...report, "import: still pending after 5s"]), 5000);
      import("/turbo-desktop/inspector.js").then(
        (m) => {
          clearTimeout(timer);
          done([...report, `import: ok, exports ${Object.keys(m)}`]);
        },
        (e) => {
          clearTimeout(timer);
          done([...report, `import: failed, ${e && e.message}`]);
        }
      );
    });
    throw new Error(`${error.message} — ${why.join("; ")}`, { cause: error });
  }
  const hidden = await browser.execute(
    () => document.querySelector("[data-turbo-desktop-inspector]").style.display
  );
  assert.equal(hidden, "none");

  await browser.execute(() => {
    window.dispatchEvent(
      new window.KeyboardEvent("keydown", { key: "D", ctrlKey: true, shiftKey: true, bubbles: true })
    );
  });

  const shown = await browser.execute(
    () => document.querySelector("[data-turbo-desktop-inspector]").style.display
  );
  assert.equal(shown, "block");
});

// ─── When things go wrong ────────────────────────────────────────────────────

test("a page the server fails on is reported, and the banner clears on the next good one", async () => {
  await startOver();
  // Kept as a list, not an object: WebDriver takes any returned object with
  // an `error` key for a failure of its own.
  await browser.execute(() => {
    window.__errors = [];
    document.addEventListener("turbo-desktop:visit-error", (event) => {
      window.__errors.push([event.detail.error, event.detail.status, typeof event.detail.retry]);
    });
  });

  await click("to-broken");
  await waitFor(() => browser.execute(() => window.__errors && window.__errors.length > 0), {
    label: "the failure to be reported",
  });

  assert.deepEqual(await browser.execute(() => window.__errors[0]), [
    "http_failure",
    500,
    "function",
  ]);
  // What the person sees is the server's own error page, which Turbo renders.
  await waitFor(async () => (await browser.getTitle()) === "Broken", {
    label: "the server's error page",
  });

  await startOver();
  await click("to-tasks");
  await waitFor(async () => (await path()) === "/tasks", { label: "the tasks page" });
  assert.equal(
    await browser.execute((id) => Boolean(document.getElementById(id)), BANNER),
    false,
    "the banner outlived the failure"
  );
});

test("the app notices its server going away, and coming back", async () => {
  await startOver();
  await browser.execute(() => {
    window.__connection = [];
    document.addEventListener("turbo-desktop:connection", (event) => {
      window.__connection.push(event.detail.online);
    });
  });

  await fetch(`${ORIGIN}/__outage?seconds=12`);

  await waitFor(() => browser.execute(() => window.__connection.includes(false)), {
    label: "the app to notice the server is gone",
    tries: 80,
  });
  assert.equal(
    await browser.execute((id) => Boolean(document.getElementById(id)), BANNER),
    true,
    "nothing told the person the server was gone"
  );

  await waitFor(() => browser.execute(() => window.__connection.includes(true)), {
    label: "the app to notice the server is back",
    tries: 80,
  });
  assert.equal(await browser.execute((id) => Boolean(document.getElementById(id)), BANNER), false);

  // And it is usable again, without a restart.
  await click("to-tasks");
  await waitFor(async () => (await path()) === "/tasks", { label: "the tasks page" });
});

// ─── Closing the app ─────────────────────────────────────────────────────────
//
// Last, because it ends the app.

test("quitting the app stops the server it started", { skip: process.platform === "win32" }, async () => {
  await startOver();

  // Closing the last window over WebDriver kills the process outright, which
  // is a crash, not a quit. Asking it to stop is what Ctrl+C on
  // `turbo-desktop dev` does, and goes through the same exit as the Quit menu.
  const pids = execFileSync("pgrep", ["-f", binary], { encoding: "utf-8" }).trim().split("\n");
  assert.ok(pids.length > 0 && pids[0], "could not find the app's process");
  for (const pid of pids) process.kill(Number(pid), "SIGTERM");

  await waitFor(async () => !(await serverAnswers()), {
    label: "the server to stop",
    tries: 40,
  });
  browser = null;
});
