import { describe, it, beforeEach } from "node:test";
import assert from "node:assert/strict";

// Imported once, here, with no shell in sight — which is how it goes in an
// app: module scripts run before the shell injects turbo-desktop.js, and in a
// plain browser the shell never arrives at all.
delete globalThis.TurboDesktop;
delete globalThis.__TURBO_DESKTOP__;
const bridge = await import("../packages/bridge/index.js");

class Controller {
  constructor(element) {
    this.element = element;
    this.connected = false;
  }
  connect() {
    this.connected = true;
  }
  disconnect() {
    this.connected = false;
  }
}

function injectShell() {
  const sent = [];

  class NativeComponent {
    static component = "unknown";
    constructor(element) {
      this.element = element;
    }
    connect() {
      sent.push({ component: this.constructor.component, event: "connect" });
    }
    disconnect() {
      sent.push({ component: this.constructor.component, event: "disconnect" });
    }
    async send(event, data = {}) {
      sent.push({ component: this.constructor.component, event, data });
      return { ok: true };
    }
    onReceive() {}
  }

  globalThis.TurboDesktop = {
    version: "0.0.0-test",
    isNative: true,
    BridgeComponent: NativeComponent,
    getWindowInfo: async function () {
      return { platform: "macos", version: this.version };
    },
  };
  globalThis.__TURBO_DESKTOP__ = globalThis.TurboDesktop;

  return sent;
}

describe("turbo-desktop-bridge, imported before the shell has injected", () => {
  beforeEach(() => {
    delete globalThis.TurboDesktop;
    delete globalThis.__TURBO_DESKTOP__;
  });

  it("can define a controller without the shell", () => {
    const Notify = bridge.stimulusBridge(Controller, "notification");

    assert.equal(typeof Notify, "function", "the README's example threw at module load");
  });

  it("runs the controller in a plain browser, and sends nothing", async () => {
    const Notify = bridge.stimulusBridge(Controller, "notification");
    const controller = new Notify({});

    controller.connect();

    assert.equal(controller.connected, true, "the controller's own connect() did not run");
    assert.equal(await controller.sendBridge("show", {}), null);
  });

  it("reaches the shell once it is there", async () => {
    const Notify = bridge.stimulusBridge(Controller, "notification");
    const sent = injectShell();
    const controller = new Notify({});

    controller.connect();
    await controller.sendBridge("show", { title: "Hello" });

    assert.deepEqual(sent, [
      { component: "notification", event: "connect" },
      { component: "notification", event: "show", data: { title: "Hello" } },
    ]);
  });

  it("keeps two components apart", async () => {
    const Notify = bridge.stimulusBridge(Controller, "notification");
    const Menu = bridge.stimulusBridge(Controller, "menu-item");
    const sent = injectShell();
    const notify = new Notify({});
    const menu = new Menu({});
    notify.connect();
    menu.connect();
    sent.length = 0;

    await notify.sendBridge("show");
    await menu.sendBridge("register");

    assert.deepEqual(
      sent.map((message) => message.component),
      ["notification", "menu-item"]
    );
  });

  it("hands messages from the shell to receiveBridge", () => {
    const received = [];
    class Notify extends bridge.stimulusBridge(Controller, "notification") {
      receiveBridge(message) {
        received.push(message);
      }
    }
    injectShell();
    const controller = new Notify({});
    controller.connect();

    controller._bridge._native.onReceive({ event: "clicked" });

    assert.deepEqual(received, [{ event: "clicked" }]);
  });

  it("sees the shell's API through TurboDesktop", async () => {
    assert.equal(bridge.TurboDesktop.version, undefined);

    injectShell();

    assert.equal(bridge.TurboDesktop.version, "0.0.0-test");
    assert.deepEqual(await bridge.TurboDesktop.getWindowInfo(), {
      platform: "macos",
      version: "0.0.0-test",
    });
  });

  it("can extend BridgeComponent without the shell", async () => {
    class Badge extends bridge.BridgeComponent {
      static component = "badge";
    }
    const badge = new Badge({});
    badge.connect();
    assert.equal(await badge.send("set", { count: 3 }), null);

    const sent = injectShell();
    const live = new Badge({});
    live.connect();
    await live.send("set", { count: 3 });

    assert.deepEqual(sent.at(-1), { component: "badge", event: "set", data: { count: 3 } });
  });

  it("knows whether the shell is there", () => {
    assert.equal(bridge.isTurboDesktop(), false);
    injectShell();
    assert.equal(bridge.isTurboDesktop(), true);
  });
});
