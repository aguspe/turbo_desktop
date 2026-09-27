import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const scriptSource = readFileSync(
  resolve(__dirname, "../src/turbo-desktop.js"),
  "utf-8"
);
const { version: packageVersion } = JSON.parse(
  readFileSync(resolve(__dirname, "../package.json"), "utf-8")
);

/**
 * Create a fresh JSDOM window and execute the turbo-desktop script in it.
 * When invoke is provided, it records all calls in an array for inspection.
 * Returns { window, calls } where calls is the array of { cmd, args } objects.
 */
function createEnvironment({ invoke = undefined, readyState = "complete" } = {}) {
  const dom = new JSDOM(
    `<!DOCTYPE html><html><head><title>Test Page</title></head><body></body></html>`,
    {
      url: "https://myapp.test/",
      runScripts: "dangerously",
      pretendToBeVisual: true,
    }
  );

  const { window } = dom;

  if (readyState !== "complete") {
    Object.defineProperty(window.document, "readyState", {
      get: () => readyState,
      configurable: true,
    });
  }

  const calls = [];

  if (invoke) {
    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd, args) => {
        calls.push({ cmd, args });
        return invoke(cmd, args);
      },
    };
  }

  window.eval(scriptSource);

  return { dom, window, calls };
}

/** Wait a microtask tick so the async initial setTitle settles. */
const tick = () => new Promise((r) => setTimeout(r, 5));

/**
 * deepEqual that works across JSDOM/Node realms.
 * Objects from window.eval have a different Object prototype, so
 * assert.deepStrictEqual fails even with identical structures.
 * Round-trip through JSON to normalize.
 */
function assertDeepEqual(actual, expected, message) {
  assert.deepStrictEqual(
    JSON.parse(JSON.stringify(actual)),
    JSON.parse(JSON.stringify(expected)),
    message
  );
}

// ─── Initialization ───────────────────────────────────────────────────────

describe("TurboDesktop initialization", () => {
  it("exposes TurboDesktop on window", () => {
    const { window } = createEnvironment();
    assert.ok(window.TurboDesktop);
    assert.ok(window.__TURBO_DESKTOP__);
    assert.strictEqual(window.TurboDesktop, window.__TURBO_DESKTOP__);
  });

  it("sets version, platform, and isNative", () => {
    const { window } = createEnvironment();
    assert.strictEqual(window.TurboDesktop.version, packageVersion);
    // Whichever machine runs this: the platform is read, not written in.
    assert.ok(
      ["macos", "windows", "linux"].includes(window.TurboDesktop.platform),
      `unexpected platform ${window.TurboDesktop.platform}`
    );
    assert.strictEqual(window.TurboDesktop.isNative, true);
  });

  it("guards against double injection", () => {
    const dom = new JSDOM(
      `<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>`,
      { url: "https://myapp.test/", runScripts: "dangerously", pretendToBeVisual: true }
    );
    const { window } = dom;

    window.eval(scriptSource);
    window.TurboDesktop._marker = "first";

    // Second injection should be a no-op
    window.eval(scriptSource);
    assert.strictEqual(window.TurboDesktop._marker, "first");
  });

  it("exposes BridgeComponent class", () => {
    const { window } = createEnvironment();
    assert.ok(window.TurboDesktop.BridgeComponent);
    assert.strictEqual(typeof window.TurboDesktop.BridgeComponent, "function");
  });

  it("exposes stimulusBridge function", () => {
    const { window } = createEnvironment();
    assert.strictEqual(typeof window.TurboDesktop.stimulusBridge, "function");
  });
});

// ─── proposeVisit ─────────────────────────────────────────────────────────

describe("TurboDesktop.proposeVisit", () => {
  it("returns default fallback when no INVOKE available", async () => {
    const { window } = createEnvironment();
    const result = await window.TurboDesktop.proposeVisit("https://myapp.test/page");
    assertDeepEqual(result, { action: "advance", presentation: "default" });
  });

  it("returns default fallback with custom action when no INVOKE", async () => {
    const { window } = createEnvironment();
    const result = await window.TurboDesktop.proposeVisit("https://myapp.test/page", "replace");
    assertDeepEqual(result, { action: "replace", presentation: "default" });
  });

  it("calls invoke with correct arguments", async () => {
    const mockInvoke = async () => ({ action: "advance", presentation: "modal" });
    const { window, calls } = createEnvironment({ invoke: mockInvoke });

    // Wait for the initial setTitle call from script init
    await tick();

    await window.TurboDesktop.proposeVisit("https://myapp.test/new", "advance");

    const visitCall = calls.find((c) => c.cmd === "handle_visit_proposal");
    assert.ok(visitCall, "Expected a handle_visit_proposal call");
    assert.strictEqual(visitCall.args.proposal.url, "https://myapp.test/new");
    assert.strictEqual(visitCall.args.proposal.path, "/new");
    assert.strictEqual(visitCall.args.proposal.action, "advance");
  });

  it("resolves relative URLs against window.location.origin", async () => {
    const mockInvoke = async () => ({ action: "advance", presentation: "default" });
    const { window, calls } = createEnvironment({ invoke: mockInvoke });
    await tick();

    await window.TurboDesktop.proposeVisit("/relative/path");

    const visitCall = calls.find((c) => c.cmd === "handle_visit_proposal");
    assert.ok(visitCall);
    assert.strictEqual(visitCall.args.proposal.url, "https://myapp.test/relative/path");
    assert.strictEqual(visitCall.args.proposal.path, "/relative/path");
  });

  it("returns fallback on invoke error", async () => {
    const mockInvoke = async (cmd) => {
      if (cmd === "handle_visit_proposal") throw new Error("Rust panicked");
      // Let other calls succeed silently
    };
    const { window } = createEnvironment({ invoke: mockInvoke });
    await tick();

    const result = await window.TurboDesktop.proposeVisit("https://myapp.test/page");
    assertDeepEqual(result, { action: "advance", presentation: "default" });
  });
});

// ─── setTitle ─────────────────────────────────────────────────────────────

describe("TurboDesktop.setTitle", () => {
  it("does nothing when no INVOKE available", async () => {
    const { window } = createEnvironment();
    await window.TurboDesktop.setTitle("New Title");
    // No error thrown means pass
  });

  it("calls invoke with title", async () => {
    const mockInvoke = async () => {};
    const { window, calls } = createEnvironment({ invoke: mockInvoke });

    // Wait for the init setTitle("Test Page") to settle
    await tick();

    await window.TurboDesktop.setTitle("My App - Dashboard");

    const titleCalls = calls.filter((c) => c.cmd === "update_window_title");
    // First call is from init ("Test Page"), second is our explicit call
    const lastCall = titleCalls[titleCalls.length - 1];
    assert.strictEqual(lastCall.args.title, "My App - Dashboard");
  });

  it("handles invoke error gracefully", async () => {
    const mockInvoke = async () => {
      throw new Error("fail");
    };
    const { window } = createEnvironment({ invoke: mockInvoke });
    await tick();

    // Should not throw
    await window.TurboDesktop.setTitle("Title");
  });
});

// ─── sendBridgeMessage ────────────────────────────────────────────────────

describe("TurboDesktop.sendBridgeMessage", () => {
  it("returns null when no INVOKE available", async () => {
    const { window } = createEnvironment();
    const result = await window.TurboDesktop.sendBridgeMessage("menu", "click", { id: 1 });
    assert.strictEqual(result, null);
  });

  it("calls invoke with correct message structure", async () => {
    const mockInvoke = async (cmd) => {
      if (cmd === "handle_bridge_message") return { ok: true };
    };
    const { window, calls } = createEnvironment({ invoke: mockInvoke });
    await tick();

    const result = await window.TurboDesktop.sendBridgeMessage("notification", "show", { title: "Hello" });

    // The script also sends its own startup messages (e.g. draining files the
    // OS asked the app to open), so look for this call rather than the first.
    const bridgeCall = calls.find(
      (c) => c.cmd === "handle_bridge_message" && c.args.message.component === "notification"
    );
    assert.ok(bridgeCall);
    assertDeepEqual(bridgeCall.args.message, {
      component: "notification",
      event: "show",
      data: { title: "Hello" },
    });
    assertDeepEqual(result, { ok: true });
  });

  it("returns null on error", async () => {
    const mockInvoke = async (cmd) => {
      if (cmd === "handle_bridge_message") throw new Error("fail");
    };
    const { window } = createEnvironment({ invoke: mockInvoke });
    await tick();

    const result = await window.TurboDesktop.sendBridgeMessage("menu", "click");
    assert.strictEqual(result, null);
  });
});

// ─── getWindowInfo ────────────────────────────────────────────────────────

describe("TurboDesktop.getWindowInfo", () => {
  it("returns null when no INVOKE", async () => {
    const { window } = createEnvironment();
    const result = await window.TurboDesktop.getWindowInfo();
    assert.strictEqual(result, null);
  });

  it("calls invoke and returns result", async () => {
    const mockInvoke = async (cmd) => {
      if (cmd === "get_window_info") return { label: "main", title: "App" };
    };
    const { window } = createEnvironment({ invoke: mockInvoke });
    await tick();

    const info = await window.TurboDesktop.getWindowInfo();
    assertDeepEqual(info, { label: "main", title: "App" });
  });
});

// ─── closeModal ───────────────────────────────────────────────────────────

describe("TurboDesktop.closeModal", () => {
  it("does nothing when no INVOKE", async () => {
    const { window } = createEnvironment();
    await window.TurboDesktop.closeModal("modal-1");
  });

  it("calls invoke with label", async () => {
    const mockInvoke = async () => {};
    const { window, calls } = createEnvironment({ invoke: mockInvoke });
    await tick();

    await window.TurboDesktop.closeModal("modal-1");

    const modalCall = calls.find((c) => c.cmd === "close_modal");
    assert.ok(modalCall);
    assert.strictEqual(modalCall.args.label, "modal-1");
  });
});

// ─── BridgeComponent ─────────────────────────────────────────────────────

describe("BridgeComponent", () => {
  it("has default component name 'unknown'", () => {
    const { window } = createEnvironment();
    const BC = window.TurboDesktop.BridgeComponent;
    assert.strictEqual(BC.component, "unknown");
  });

  it("stores element reference", () => {
    const { window } = createEnvironment();
    const BC = window.TurboDesktop.BridgeComponent;
    const el = window.document.createElement("div");
    const instance = new BC(el);
    assert.strictEqual(instance.element, el);
  });

  it("send() delegates to TurboDesktop.sendBridgeMessage", async () => {
    const mockInvoke = async (cmd) => {
      if (cmd === "handle_bridge_message") return { handled: true };
    };
    const { window, calls } = createEnvironment({ invoke: mockInvoke });
    await tick();

    const BC = window.TurboDesktop.BridgeComponent;

    // Create a subclass inside the JSDOM context so it shares the same class identity
    const TestComponent = window.eval(`
      (function(BC) {
        class TestComponent extends BC {
          static component = "test-widget";
        }
        return TestComponent;
      })
    `)(BC);

    const el = window.document.createElement("div");
    const instance = new TestComponent(el);
    const result = await instance.send("activate", { color: "red" });

    const bridgeCall = calls.find(
      (c) => c.cmd === "handle_bridge_message" && c.args.message.component === "test-widget"
    );
    assert.ok(bridgeCall);
    assert.strictEqual(bridgeCall.args.message.component, "test-widget");
    assert.strictEqual(bridgeCall.args.message.event, "activate");
    assertDeepEqual(bridgeCall.args.message.data, { color: "red" });
    assertDeepEqual(result, { handled: true });
  });

  it("disconnect() sends a disconnect message", async () => {
    const mockInvoke = async () => {};
    const { window, calls } = createEnvironment({ invoke: mockInvoke });
    await tick();

    const BC = window.TurboDesktop.BridgeComponent;

    const MyComponent = window.eval(`
      (function(BC) {
        class MyComponent extends BC {
          static component = "my-comp";
        }
        return MyComponent;
      })
    `)(BC);

    const el = window.document.createElement("div");
    const instance = new MyComponent(el);
    await instance.disconnect();

    const disconnectCall = calls.find(
      (c) => c.cmd === "handle_bridge_message" && c.args.message.event === "disconnect"
    );
    assert.ok(disconnectCall);
    assert.strictEqual(disconnectCall.args.message.component, "my-comp");
  });

  it("_handleReceive filters by component name", () => {
    const { window } = createEnvironment();
    const BC = window.TurboDesktop.BridgeComponent;

    const WidgetA = window.eval(`
      (function(BC) {
        class WidgetA extends BC {
          static component = "widget-a";
        }
        return WidgetA;
      })
    `)(BC);

    const el = window.document.createElement("div");
    const instance = new WidgetA(el);

    let received = null;
    instance.onReceive = (msg) => {
      received = msg;
    };

    // Matching component
    instance._handleReceive({ payload: { component: "widget-a", event: "update", data: {} } });
    assert.ok(received);
    assert.strictEqual(received.component, "widget-a");

    // Non-matching component — should not call onReceive
    received = null;
    instance._handleReceive({ payload: { component: "widget-b", event: "update", data: {} } });
    assert.strictEqual(received, null);
  });
});

// ─── Bridge response delivery ────────────────────────────────────────────

describe("bridge-response delivery", () => {
  // Tauri's event API is not exposed to remote pages, so responses arrive
  // through __receive and fan out to the registered listeners.
  it("reaches shell.onOutput through __receive", () => {
    const { window } = createEnvironment();
    const td = window.TurboDesktop;
    const seen = [];

    td.shell.onOutput("job-1", (message) => seen.push(message));
    td.__receive("bridge-response", {
      component: "shell",
      event: "stdout",
      data: { id: "job-1", line: "hi" },
    });

    assert.strictEqual(seen.length, 1);
    assert.strictEqual(seen[0].event, "stdout");
    assert.strictEqual(seen[0].line, "hi");

    // Another job's output does not leak in.
    td.__receive("bridge-response", {
      component: "shell",
      event: "stdout",
      data: { id: "job-2", line: "not mine" },
    });
    assert.strictEqual(seen.length, 1);

    // And unsubscribing stops delivery.
    td.shell.offOutput("job-1");
    td.__receive("bridge-response", {
      component: "shell",
      event: "stdout",
      data: { id: "job-1", line: "after off" },
    });
    assert.strictEqual(seen.length, 1);
  });

  it("mirrors drag-drop responses as DOM events", () => {
    const { window } = createEnvironment();
    let detail = null;
    window.document.addEventListener("turbo-desktop:drop", (e) => {
      detail = e.detail;
    });

    window.TurboDesktop.__receive("bridge-response", {
      component: "drag-drop",
      event: "drop",
      data: { paths: ["/tmp/a.csv"], position: { x: 1, y: 2 } },
    });

    assert.ok(detail, "the DOM event should have fired");
    assert.deepStrictEqual(detail.paths, ["/tmp/a.csv"]);
  });
});

// ─── stimulusBridge ──────────────────────────────────────────────────────

describe("stimulusBridge", () => {
  it("creates a subclass with bridge methods", () => {
    const { window } = createEnvironment();

    class FakeController {
      constructor() {
        this.element = window.document.createElement("div");
      }
      connect() {}
      disconnect() {}
    }

    const BridgedController = window.TurboDesktop.stimulusBridge(FakeController, "toolbar");
    const instance = new BridgedController();

    assert.strictEqual(typeof instance.sendBridge, "function");
    assert.strictEqual(typeof instance.receiveBridge, "function");
    assert.ok(instance instanceof FakeController);
  });

  it("connect creates internal bridge component", () => {
    const { window } = createEnvironment();

    class FakeController {
      constructor() {
        this.element = window.document.createElement("div");
      }
      connect() {}
      disconnect() {}
    }

    const BridgedController = window.TurboDesktop.stimulusBridge(FakeController, "toolbar");
    const instance = new BridgedController();
    instance.connect();

    assert.ok(instance._bridge);
    assert.strictEqual(instance._bridge.constructor.component, "toolbar");
    assert.strictEqual(instance._bridge.element, instance.element);
  });
});

// ─── Title sync on initial load ──────────────────────────────────────────

describe("Title sync on initial load", () => {
  it("syncs title when document is already complete", async () => {
    const mockInvoke = async () => {};
    const { calls } = createEnvironment({ invoke: mockInvoke, readyState: "complete" });

    await tick();

    const titleCall = calls.find((c) => c.cmd === "update_window_title");
    assert.ok(titleCall, "Expected an update_window_title call on init");
    assert.strictEqual(titleCall.args.title, "Test Page");
  });

  it("does not sync title synchronously when document is loading", async () => {
    const mockInvoke = async () => {};
    const { calls } = createEnvironment({ invoke: mockInvoke, readyState: "loading" });

    // No title call should have happened yet (DOMContentLoaded hasn't fired)
    const titleCall = calls.find((c) => c.cmd === "update_window_title");
    assert.strictEqual(titleCall, undefined);
  });
});

describe("Connection and visit errors", () => {
  const BANNER = "#turbo-desktop-offline-overlay";

  it("exposes the same error names as Hotwire Native", () => {
    const { window } = createEnvironment();

    assertDeepEqual(window.TurboDesktop.errors, {
      NETWORK_FAILURE: "network_failure",
      TIMEOUT_FAILURE: "timeout_failure",
      HTTP_FAILURE: "http_failure",
      PAGE_LOAD_FAILURE: "page_load_failure",
    });
  });

  it("shows a banner when a Turbo fetch fails", () => {
    const { window } = createEnvironment();

    window.document.dispatchEvent(
      new window.CustomEvent("turbo:fetch-request-error", { detail: {} })
    );

    assert.ok(window.document.querySelector(BANNER), "expected the shell's banner");
  });

  it("announces failures as a cancelable event carrying a retry handler", () => {
    const { window } = createEnvironment();
    const seen = [];

    window.document.addEventListener("turbo-desktop:visit-error", (event) => {
      seen.push(event.detail);
    });

    window.document.dispatchEvent(
      new window.CustomEvent("turbo:fetch-request-error", { detail: {} })
    );

    assert.strictEqual(seen.length, 1);
    assert.strictEqual(seen[0].error, "network_failure");
    assert.strictEqual(typeof seen[0].retry, "function");
  });

  it("lets a listener suppress the shell's banner with preventDefault", () => {
    const { window } = createEnvironment();

    window.document.addEventListener("turbo-desktop:visit-error", (event) =>
      event.preventDefault()
    );
    window.document.dispatchEvent(
      new window.CustomEvent("turbo:fetch-request-error", { detail: {} })
    );

    assert.strictEqual(
      window.document.querySelector(BANNER),
      null,
      "the app took over presentation, so the shell should stay out of the way"
    );
  });

  it("stays out of the way entirely when error handling is set to manual", () => {
    const { window } = createEnvironment();
    const meta = window.document.createElement("meta");
    meta.name = "turbo-desktop-error-handling";
    meta.content = "manual";
    window.document.head.appendChild(meta);

    window.document.dispatchEvent(
      new window.CustomEvent("turbo:fetch-request-error", { detail: {} })
    );

    assert.strictEqual(window.document.querySelector(BANNER), null);
  });

  it("reports server errors with their status code", () => {
    const { window } = createEnvironment();
    const seen = [];

    window.document.addEventListener("turbo-desktop:visit-error", (event) =>
      seen.push(event.detail)
    );

    window.document.dispatchEvent(
      new window.CustomEvent("turbo:before-fetch-response", {
        detail: { fetchResponse: { succeeded: false, statusCode: 503 } },
      })
    );

    assert.strictEqual(seen.length, 1);
    assert.strictEqual(seen[0].error, "http_failure");
    assert.strictEqual(seen[0].status, 503);
  });

  it("ignores responses the app is expected to handle itself", () => {
    const { window } = createEnvironment();
    const seen = [];

    window.document.addEventListener("turbo-desktop:visit-error", (event) =>
      seen.push(event.detail)
    );

    // A 404 or a failed form validation is the app's own page to render.
    for (const statusCode of [404, 422]) {
      window.document.dispatchEvent(
        new window.CustomEvent("turbo:before-fetch-response", {
          detail: { fetchResponse: { succeeded: false, statusCode } },
        })
      );
    }

    assert.deepStrictEqual(seen, []);
  });

  it("clears the banner when the machine comes back online", () => {
    const { window } = createEnvironment();

    window.dispatchEvent(new window.Event("offline"));
    assert.ok(window.document.querySelector(BANNER));

    window.dispatchEvent(new window.Event("online"));
    assert.strictEqual(window.document.querySelector(BANNER), null);
  });
});

describe("Modal dismissal", () => {
  it("knows the window it is in", () => {
    const { window } = createEnvironment();
    window.__TURBO_DESKTOP_WINDOW_LABEL__ = "modal-abc123";

    assert.strictEqual(window.TurboDesktop.windowLabel, "modal-abc123");
    assert.strictEqual(window.TurboDesktop.isModal, true);
  });

  it("does not think the main window is a modal", () => {
    const { window } = createEnvironment();
    window.__TURBO_DESKTOP_WINDOW_LABEL__ = "main";

    assert.strictEqual(window.TurboDesktop.isModal, false);
  });

  it("closes the window it is in when given no label", async () => {
    const { window, calls } = createEnvironment({ invoke: async () => {} });
    window.__TURBO_DESKTOP_WINDOW_LABEL__ = "modal-abc123";
    await tick();

    await window.TurboDesktop.closeModal();

    const call = calls.find((c) => c.cmd === "close_modal");
    assert.ok(call, "expected a close_modal call");
    assert.strictEqual(call.args.label, "modal-abc123");
  });

  it("uses Hotwire Native's dismissal names", async () => {
    for (const [method, then] of [
      ["recede", "recede"],
      ["refresh", "refresh"],
      ["resume", "resume"],
    ]) {
      const { window, calls } = createEnvironment({ invoke: async () => {} });
      await tick();

      await window.TurboDesktop[method]();

      const call = calls.find((c) => c.cmd === "dismiss_modal");
      assert.ok(call, `expected ${method}() to dismiss`);
      assert.strictEqual(call.args.then, then);
    }
  });
});

describe("Messages from the shell", () => {
  it("refreshes the page when told to", () => {
    const { window } = createEnvironment();
    const visits = [];
    window.Turbo = { visit: (url, opts) => visits.push({ url, opts }) };

    window.TurboDesktop.__receive("navigate", { action: "refresh" });

    assert.strictEqual(visits.length, 1);
    assert.strictEqual(visits[0].opts.action, "replace");
  });

  it("leaves the page alone when told to resume", () => {
    const { window } = createEnvironment();
    const visits = [];
    window.Turbo = { visit: (url, opts) => visits.push({ url, opts }) };

    window.TurboDesktop.__receive("navigate", { action: "none" });

    assert.deepStrictEqual(visits, []);
  });

  it("shows the banner when the shell reports a lost connection", () => {
    const { window } = createEnvironment();

    window.TurboDesktop.__receive("connection", {
      online: false,
      error: "network_failure",
    });

    assert.ok(window.document.querySelector("#turbo-desktop-offline-overlay"));
  });

  it("clears the banner when the shell reports reconnection", () => {
    const { window } = createEnvironment();

    window.TurboDesktop.__receive("connection", { online: false });
    window.TurboDesktop.__receive("connection", { online: true });

    assert.strictEqual(
      window.document.querySelector("#turbo-desktop-offline-overlay"),
      null
    );
  });

  it("ignores messages it does not understand", () => {
    const { window } = createEnvironment();

    // Must not throw — the shell may be newer than the injected script.
    window.TurboDesktop.__receive("something-new", { a: 1 });
  });
});

describe("Returning to the window", () => {
  function focusReturn(window, detail) {
    window.TurboDesktop.__receive("focus", detail);
  }

  it("refreshes when the shell says the absence was long enough", () => {
    const { window } = createEnvironment();
    const visits = [];
    window.Turbo = { visit: (url, opts) => visits.push({ url, opts }) };

    focusReturn(window, { awaySeconds: 120, refreshing: true });

    assert.strictEqual(visits.length, 1);
    assert.strictEqual(visits[0].opts.action, "replace");
  });

  it("does nothing when the absence was short", () => {
    const { window } = createEnvironment();
    const visits = [];
    window.Turbo = { visit: (url, opts) => visits.push({ url, opts }) };

    focusReturn(window, { awaySeconds: 3, refreshing: false });

    assert.deepStrictEqual(visits, []);
  });

  it("announces the return either way", () => {
    const { window } = createEnvironment();
    const seen = [];
    window.document.addEventListener("turbo-desktop:focus", (e) => seen.push(e.detail));

    focusReturn(window, { awaySeconds: 3, refreshing: false });
    focusReturn(window, { awaySeconds: 300, refreshing: true });

    assert.strictEqual(seen.length, 2);
    assert.strictEqual(seen[0].awaySeconds, 3);
    assert.strictEqual(seen[1].refreshing, true);
  });

  it("lets the app veto the refresh", () => {
    const { window } = createEnvironment();
    const visits = [];
    window.Turbo = { visit: (url, opts) => visits.push({ url, opts }) };
    window.document.addEventListener("turbo-desktop:focus", (e) => e.preventDefault());

    focusReturn(window, { awaySeconds: 300, refreshing: true });

    assert.deepStrictEqual(visits, [], "the app knows about state we cannot see");
  });

  it("does not throw away what someone is typing", () => {
    const { window } = createEnvironment();
    const visits = [];
    window.Turbo = { visit: (url, opts) => visits.push({ url, opts }) };

    const input = window.document.createElement("input");
    window.document.body.appendChild(input);
    input.focus();

    focusReturn(window, { awaySeconds: 3600, refreshing: true });

    assert.deepStrictEqual(visits, [], "a half-filled form outranks stale data");
  });

  it("refreshes once the field is no longer focused", () => {
    const { window } = createEnvironment();
    const visits = [];
    window.Turbo = { visit: (url, opts) => visits.push({ url, opts }) };

    const input = window.document.createElement("input");
    window.document.body.appendChild(input);
    input.focus();
    input.blur();

    focusReturn(window, { awaySeconds: 3600, refreshing: true });

    assert.strictEqual(visits.length, 1);
  });
});

// ─── Turbo Drive integration ──────────────────────────────────────────────

describe("visits the shell presents itself", () => {
  // Turbo reads defaultPrevented as soon as dispatchEvent returns. A decision
  // that arrives after an await is a decision Turbo never sees.
  function propose(window, url) {
    const event = new window.CustomEvent("turbo:before-visit", {
      detail: { url },
      cancelable: true,
      bubbles: true,
    });
    window.document.dispatchEvent(event);
    return event;
  }

  function environment(decision) {
    const visits = [];
    const env = createEnvironment({
      invoke: (cmd) => (cmd === "handle_visit_proposal" ? decision : undefined),
    });
    env.window.Turbo = { visit: (url, options) => visits.push({ url, ...options }) };
    return { ...env, visits };
  }

  it("holds the visit while the shell decides", () => {
    const { window } = environment({ action: "none", presentation: "modal" });

    const event = propose(window, "https://myapp.test/tasks/new");

    assert.equal(
      event.defaultPrevented,
      true,
      "the main window would navigate to the URL the modal is about to show"
    );
  });

  it("does not navigate the main window when the shell opens a modal", async () => {
    const { window, visits } = environment({ action: "none", presentation: "modal" });

    propose(window, "https://myapp.test/tasks/new");
    await tick();

    assertDeepEqual(visits, []);
  });

  it("carries on with an ordinary visit once the shell agrees", async () => {
    const { window, visits } = environment({ action: "advance", presentation: "default" });

    propose(window, "https://myapp.test/tasks");
    await tick();

    assertDeepEqual(visits, [{ url: "https://myapp.test/tasks", action: "advance" }]);
  });

  it("lets the visit it re-issued through, rather than proposing it again", async () => {
    const { window, visits, calls } = environment({ action: "advance", presentation: "default" });

    propose(window, "https://myapp.test/tasks");
    await tick();
    const second = propose(window, visits[0].url);
    await tick();

    assert.equal(second.defaultPrevented, false, "the approved visit was held a second time");
    assert.equal(calls.filter((call) => call.cmd === "handle_visit_proposal").length, 1);
    assert.equal(visits.length, 1, "the visit went round in a loop");
  });

  it("replaces instead of advancing when the rule says so, once", async () => {
    const { window, visits, calls } = environment({ action: "replace", presentation: "replace" });

    propose(window, "https://myapp.test/dashboard");
    await tick();
    propose(window, visits[0].url);
    await tick();

    assertDeepEqual(visits, [{ url: "https://myapp.test/dashboard", action: "replace" }]);
    assert.equal(calls.filter((call) => call.cmd === "handle_visit_proposal").length, 1);
  });

  it("keeps the action a link asked for", async () => {
    const { window, visits } = environment({ action: "advance", presentation: "default" });
    const link = window.document.createElement("a");
    link.href = "https://myapp.test/tasks?page=2";
    link.dataset.turboAction = "replace";
    window.document.body.appendChild(link);

    link.dispatchEvent(
      new window.CustomEvent("turbo:click", {
        detail: { url: link.href },
        bubbles: true,
        cancelable: true,
      })
    );
    propose(window, link.href);
    await tick();

    assertDeepEqual(visits, [{ url: "https://myapp.test/tasks?page=2", action: "replace" }]);
  });

  it("still navigates when the shell cannot be asked", async () => {
    const visits = [];
    const { window } = createEnvironment({
      invoke: (cmd) => {
        if (cmd === "handle_visit_proposal") throw new Error("the shell is gone");
      },
    });
    window.Turbo = { visit: (url, options) => visits.push({ url, ...options }) };

    propose(window, "https://myapp.test/tasks");
    await tick();

    assert.equal(visits.length, 1, "a failed proposal left the link doing nothing");
  });

  it("still navigates when the shell answers with nothing", async () => {
    const { window, visits } = environment(null);

    propose(window, "https://myapp.test/tasks");
    await tick();

    assertDeepEqual(visits, [{ url: "https://myapp.test/tasks", action: "advance" }]);
  });
});

describe("stimulusBridge with more than one component", () => {
  it("keeps each controller on its own component", async () => {
    const { window, calls } = createEnvironment({ invoke: () => ({ ok: true }) });
    class Base {
      constructor(element) {
        this.element = element;
      }
      connect() {}
      disconnect() {}
    }

    const Notify = window.TurboDesktop.stimulusBridge(Base, "notification");
    const Menu = window.TurboDesktop.stimulusBridge(Base, "menu-item");
    const notify = new Notify(window.document.createElement("div"));
    const menu = new Menu(window.document.createElement("div"));
    notify.connect();
    menu.connect();
    calls.length = 0;

    await notify.sendBridge("show", {});
    await menu.sendBridge("register", {});

    assertDeepEqual(
      calls
        .filter((call) => call.cmd === "handle_bridge_message")
        .map((call) => call.args.message.component),
      ["notification", "menu-item"]
    );
  });

  it("leaves the base class's own component name alone", () => {
    const { window } = createEnvironment({ invoke: () => ({ ok: true }) });
    class Base {
      constructor(element) {
        this.element = element;
      }
      connect() {}
    }

    const Notify = window.TurboDesktop.stimulusBridge(Base, "notification");
    new Notify(window.document.createElement("div")).connect();

    assert.equal(window.TurboDesktop.BridgeComponent.component, "unknown");
  });
});

describe("TurboDesktop.platform", () => {
  function platformFor(userAgent) {
    const dom = new JSDOM(`<!DOCTYPE html><html><head></head><body></body></html>`, {
      url: "https://myapp.test/",
      runScripts: "dangerously",
    });
    Object.defineProperty(dom.window.navigator, "userAgent", { value: userAgent });
    dom.window.eval(scriptSource);
    return dom.window.TurboDesktop.platform;
  }

  it("reports the platform the shell is running on", () => {
    assert.equal(platformFor("Turbo Desktop/0.2.4 (macOS; aarch64)"), "macos");
    assert.equal(platformFor("Turbo Desktop/0.2.4 (Windows; x86_64)"), "windows");
    assert.equal(platformFor("Turbo Desktop/0.2.4 (Linux; x86_64)"), "linux");
  });

  it("reads it past a custom user agent that keeps the token", () => {
    assert.equal(platformFor("MyApp/3.1 Turbo Desktop/0.2.4 (Linux; aarch64)"), "linux");
  });
});

describe("the offline banner", () => {
  const BANNER = "turbo-desktop-offline-overlay";

  function fire(window, name, detail) {
    window.document.dispatchEvent(new window.CustomEvent(name, { detail, cancelable: true }));
  }

  it("goes away when the next request succeeds", () => {
    const { window } = createEnvironment({ invoke: () => undefined });

    fire(window, "turbo:fetch-request-error", { url: "https://myapp.test/tasks" });
    assert.ok(window.document.getElementById(BANNER), "test setup: the banner never appeared");

    // One failed request, and the server is fine. Nothing else would take the
    // banner down: the shell only reports the connection changing, and it
    // has not changed.
    fire(window, "turbo:before-fetch-response", {
      fetchResponse: { succeeded: true, statusCode: 200 },
    });

    assert.equal(window.document.getElementById(BANNER), null);
  });

  it("stays while requests keep failing", () => {
    const { window } = createEnvironment({ invoke: () => undefined });

    fire(window, "turbo:fetch-request-error", { url: "https://myapp.test/tasks" });
    fire(window, "turbo:before-fetch-response", {
      fetchResponse: { succeeded: false, statusCode: 503 },
    });

    assert.ok(window.document.getElementById(BANNER));
  });

  it("goes away when the server answers with an error page of its own", () => {
    const { window } = createEnvironment({ invoke: () => undefined });

    fire(window, "turbo:fetch-request-error", { url: "https://myapp.test/tasks" });
    fire(window, "turbo:before-fetch-response", {
      fetchResponse: { succeeded: false, statusCode: 422 },
    });

    assert.equal(
      window.document.getElementById(BANNER),
      null,
      "a 422 is the server answering; it is reachable"
    );
  });
});

describe("listening to a process's output twice", () => {
  for (const namespace of ["shell", "sudo"]) {
    it(`${namespace}.onOutput replaces the earlier listener for that process`, () => {
      const { window } = createEnvironment();
      const td = window.TurboDesktop;
      const seen = [];

      // What a Stimulus controller does when Turbo brings its page back.
      td[namespace].onOutput("job-1", (message) => seen.push(["first", message.line]));
      td[namespace].onOutput("job-1", (message) => seen.push(["second", message.line]));
      td.__receive("bridge-response", {
        component: namespace,
        event: "stdout",
        data: { id: "job-1", line: "hi" },
      });

      assertDeepEqual(seen, [["second", "hi"]]);

      td[namespace].offOutput("job-1");
      td.__receive("bridge-response", {
        component: namespace,
        event: "stdout",
        data: { id: "job-1", line: "after off" },
      });

      assert.equal(seen.length, 1, "a listener outlived offOutput");
    });
  }
});

describe("being there before the page's own scripts", () => {
  // The shell runs this script before anything the page loads, so that a
  // Stimulus controller can use TurboDesktop in connect(). The document has
  // no head or body yet at that point.
  async function beforeThePage() {
    const dom = new JSDOM(`<!DOCTYPE html>`, {
      url: "https://myapp.test/",
      runScripts: "dangerously",
    });
    const { window } = dom;
    // Let jsdom finish loading its own empty document first, so the only
    // DOMContentLoaded from here on is the one the test dispatches.
    await tick();
    const ready = [];
    const calls = [];

    Object.defineProperty(window.document, "readyState", {
      get: () => (ready.loaded ? "complete" : "loading"),
      configurable: true,
    });
    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd, args) => {
        calls.push({ cmd, args });
      },
    };
    window.document.addEventListener("turbo-desktop:ready", (event) => ready.push(event.detail));

    window.eval(scriptSource);

    return {
      window,
      ready,
      calls,
      finishLoading() {
        ready.loaded = true;
        window.document.dispatchEvent(new window.Event("DOMContentLoaded"));
      },
    };
  }

  it("is usable at once", async () => {
    const { window } = await beforeThePage();

    assert.equal(window.TurboDesktop.isNative, true);
    assert.equal(typeof window.TurboDesktop.sendBridgeMessage, "function");
  });

  it("says it is ready once the document is, and not before", async () => {
    const { ready, finishLoading } = await beforeThePage();
    await tick();
    assert.equal(ready.length, 0, "announced readiness to a document with nothing in it");

    finishLoading();
    await tick();

    assert.equal(ready.length, 1);
    assert.equal(ready[0].version, packageVersion);
  });

  it("says it is ready straight away when the document already is", async () => {
    const { window } = createEnvironment({ invoke: () => undefined });
    await tick();

    // Too late to hear the event: what a late listener can check instead.
    assert.equal(window.TurboDesktop.ready, true);
  });

  it("looks for the Dev Inspector's tag once there is a head to look in", async () => {
    const { window, finishLoading } = await beforeThePage();
    assert.equal(window.TurboDesktop._inspectorWanted, undefined, "decided before the page had a head");

    const meta = window.document.createElement("meta");
    meta.name = "turbo-desktop-inspector";
    meta.content = "enabled";
    window.document.head.appendChild(meta);
    finishLoading();
    await tick();

    assert.equal(window.TurboDesktop._inspectorWanted, true);
  });

  it("sets the title once there is one", async () => {
    const { window, calls, finishLoading } = await beforeThePage();
    window.document.title = "Tasks";

    finishLoading();
    await tick();

    const titles = calls.filter((call) => call.cmd === "update_window_title");
    assert.equal(titles.at(-1).args.title, "Tasks");
  });
});

describe("elements that declare a bridge component", () => {
  // What `turbo_desktop_bridge("menu-item", title: "Export PDF", shortcut: "Cmd+E")`
  // writes. The attributes were written and never read.
  function page(html) {
    const env = createEnvironment({ invoke: () => ({ status: "ok" }) });
    env.window.document.body.innerHTML = html;
    env.window.document.dispatchEvent(new env.window.Event("turbo:load"));
    return env;
  }

  function messages(calls, component) {
    return calls
      .filter((call) => call.cmd === "handle_bridge_message")
      .map((call) => call.args.message)
      .filter((message) => message.component === component);
  }

  function receive(window, component, event, data) {
    window.TurboDesktop.__receive("bridge-response", { component, event, data });
  }

  it("puts a menu item in the menu bar", async () => {
    const { calls } = page(`
      <button id="export"
              data-turbo-desktop-bridge="menu-item"
              data-turbo-desktop-bridge-title="Export PDF"
              data-turbo-desktop-bridge-shortcut="CmdOrCtrl+E">Export PDF</button>`);
    await tick();

    assertDeepEqual(messages(calls, "menu-item"), [
      {
        component: "menu-item",
        event: "connect",
        data: { id: "Export PDF", title: "Export PDF", shortcut: "CmdOrCtrl+E" },
      },
    ]);
  });

  it("presses the element when its menu item is chosen", async () => {
    const { window } = page(`
      <button id="export" data-turbo-desktop-bridge="menu-item"
              data-turbo-desktop-bridge-title="Export PDF">Export PDF</button>`);
    let clicks = 0;
    window.document.getElementById("export").addEventListener("click", () => clicks++);
    await tick();

    receive(window, "menu-item", "clicked", { id: "Export PDF" });

    assert.equal(clicks, 1);
  });

  it("presses the element when its shortcut is pressed", async () => {
    const { window, calls } = page(`
      <button id="add" data-turbo-desktop-bridge="shortcut"
              data-turbo-desktop-bridge-id="quick-add"
              data-turbo-desktop-bridge-accelerator="CmdOrCtrl+Shift+K">Add</button>`);
    let clicks = 0;
    window.document.getElementById("add").addEventListener("click", () => clicks++);
    await tick();

    assertDeepEqual(messages(calls, "shortcut")[0].data, {
      id: "quick-add",
      accelerator: "CmdOrCtrl+Shift+K",
    });
    receive(window, "shortcut", "triggered", { id: "quick-add", accelerator: "CmdOrCtrl+Shift+K" });

    assert.equal(clicks, 1);
  });

  it("shows a notification when the element is pressed", async () => {
    const { window, calls } = page(`
      <button id="done" data-turbo-desktop-bridge="notification"
              data-turbo-desktop-bridge-title="Task done"
              data-turbo-desktop-bridge-body="Well done">Complete</button>`);
    await tick();
    assert.equal(messages(calls, "notification").length, 0, "shown before anyone pressed anything");

    window.document.getElementById("done").click();
    await tick();

    assertDeepEqual(messages(calls, "notification"), [
      { component: "notification", event: "show", data: { title: "Task done", body: "Well done" } },
    ]);
  });

  it("sets the badge to the count on the page", async () => {
    const { calls } = page(`<span data-turbo-desktop-bridge="badge" data-turbo-desktop-bridge-count="4">4</span>`);
    await tick();

    assertDeepEqual(messages(calls, "badge")[0].data, { count: 4 });
  });

  it("binds an element once, however often the page is rendered", async () => {
    const { window, calls } = page(`
      <button data-turbo-desktop-bridge="menu-item" data-turbo-desktop-bridge-title="Export PDF">x</button>`);
    await tick();

    window.document.dispatchEvent(new window.Event("turbo:render"));
    window.document.dispatchEvent(new window.Event("turbo:load"));
    await tick();

    assert.equal(messages(calls, "menu-item").length, 1);
  });

  it("takes the menu item away when the page that declared it has gone", async () => {
    const { window, calls } = page(`
      <button data-turbo-desktop-bridge="menu-item" data-turbo-desktop-bridge-title="Export PDF">x</button>`);
    await tick();

    window.document.body.innerHTML = "<p>Another page</p>";
    window.document.dispatchEvent(new window.Event("turbo:load"));
    await tick();

    assertDeepEqual(messages(calls, "menu-item").at(-1), {
      component: "menu-item",
      event: "unregister",
      data: { id: "Export PDF" },
    });
  });

  it("does not press an element that is no longer on the page", async () => {
    const { window } = page(`
      <button id="export" data-turbo-desktop-bridge="menu-item"
              data-turbo-desktop-bridge-title="Export PDF">x</button>`);
    const button = window.document.getElementById("export");
    let clicks = 0;
    button.addEventListener("click", () => clicks++);
    await tick();

    button.remove();
    receive(window, "menu-item", "clicked", { id: "Export PDF" });

    assert.equal(clicks, 0);
  });
});

describe("TurboDesktop.toggleDevTools", () => {
  it("asks the shell to open the developer tools", async () => {
    const { window, calls } = createEnvironment({ invoke: () => ({ status: "opened" }) });

    await window.TurboDesktop.toggleDevTools();

    const asked = calls
      .filter((call) => call.cmd === "handle_bridge_message")
      .map((call) => call.args.message);
    assertDeepEqual(asked.at(-1), { component: "devtools", event: "toggle", data: {} });
  });
});

describe("loading the Dev Inspector", () => {
  it("can be asked for again by the shell, and starts once", async () => {
    const { window } = createEnvironment({ invoke: () => undefined });
    await tick();

    assert.equal(typeof window.TurboDesktop._loadInspector, "function");
    // Nothing on the page asks for the inspector, so asking changes nothing.
    window.TurboDesktop._loadInspector();
    window.TurboDesktop._loadInspector();
    assert.equal(window.TurboDesktop._inspectorWanted, false);
  });
});
