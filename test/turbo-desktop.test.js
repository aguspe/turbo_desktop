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
    assert.strictEqual(window.TurboDesktop.platform, "macos");
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

    const bridgeCall = calls.find((c) => c.cmd === "handle_bridge_message");
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

    const bridgeCall = calls.find((c) => c.cmd === "handle_bridge_message");
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
