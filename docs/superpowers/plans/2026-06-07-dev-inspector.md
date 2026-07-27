# Dev Inspector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a dev-only in-app overlay that surfaces available bridge components, a live web↔native message log, the current path-config presentation, and shell info — solving discoverability with zero production cost.

**Architecture:** A self-contained ESM `inspector.js` and `src/inspector/*` units (`BridgeTap`, `InspectorState`, `InspectorPanel`, `catalog`). The core `turbo-desktop.js` lazily `import()`s the inspector only when a gate passes, then `BridgeTap` taps the two existing bridge chokepoints (`sendBridgeMessage` outbound, the `bridge-response` Tauri event inbound). Traffic flows one-way: tap → state → Shadow-DOM panel. The Rails gem exposes a dev-only enable flag.

**Tech Stack:** Vanilla ESM JavaScript, Node's built-in test runner (`node --test`), jsdom for DOM tests, Shadow DOM for style isolation; Ruby/Minitest for the gem.

**Spec:** `docs/superpowers/specs/2026-06-07-dev-inspector-design.md`

---

## File Structure

**Create (JS):**
- `src/inspector/catalog.js` — static `component → {description, erb, stimulus}` map + accessors. Single source of truth for snippets.
- `src/inspector/state.js` — `InspectorState`: ring buffer + derived component set + nav/shell facts + change emitter. No DOM.
- `src/inspector/bridge-tap.js` — `BridgeTap`: wraps `sendBridgeMessage`, observes `bridge-response`. Strict pass-through. No DOM.
- `src/inspector/panel.js` — `InspectorPanel`: Shadow-DOM overlay rendering state. No Tauri.
- `src/inspector.js` — entry `startInspector(host, env)`: wires gate→tap→state→panel + hotkey. (Replaces the current empty placeholder file.)

**Create (tests):**
- `test/inspector/catalog.test.js`
- `test/inspector/state.test.js`
- `test/inspector/bridge-tap.test.js`
- `test/inspector/panel.test.js`
- `test/inspector/inspector.test.js`

**Modify:**
- `package.json:13` — `test` script so all `test/**/*.test.js` run (currently hard-codes one file).
- `src/turbo-desktop.js` — add the enablement gate (`_inspectorEnabled`) and the lazy `import()` at the end of the IIFE.
- `turbo_desktop-rails/lib/turbo_desktop/configuration.rb` — add `inspector_enabled` accessor (default `false`).
- `turbo_desktop-rails/lib/turbo_desktop/view_helpers.rb` — add `turbo_desktop_inspector?` + `turbo_desktop_inspector_meta_tag`.
- `turbo_desktop-rails/lib/generators/turbo_desktop/install/templates/initializer.rb.tt` — document enabling in development.
- `test/turbo_desktop-rails` — extend `configuration_test.rb` and `view_helpers_test.rb`.

**Deferred (NOT in this plan — flagged in spec):** exact matched-rule index in Navigation (needs Rust), `.toml`/config-file gate via Rust, catalog→README generation.

---

## Task 1: Test discovery plumbing

Make the test runner pick up new files under `test/inspector/` before adding any.

**Files:**
- Modify: `package.json:13`

- [ ] **Step 1: Update the test script**

In `package.json`, change the `scripts.test` line from:

```json
    "test": "node --test test/turbo-desktop.test.js"
```

to:

```json
    "test": "node --test"
```

`node --test` with no path auto-discovers every file named `*.test.js` (and `test/**`) — both the existing `test/turbo-desktop.test.js` and the new `test/inspector/*.test.js`.

- [ ] **Step 2: Verify existing tests still run**

Run: `npm test`
Expected: PASS — the existing `turbo-desktop.test.js` suite runs and passes (same count as before).

- [ ] **Step 3: Commit**

```bash
git add package.json
git commit -m "test: auto-discover all test files via node --test"
```

---

## Task 2: Component catalog

A pure data module: the list of built-in components and a copy-pasteable snippet for each. This is the discoverability payload.

**Files:**
- Create: `src/inspector/catalog.js`
- Test: `test/inspector/catalog.test.js`

- [ ] **Step 1: Write the failing test**

Create `test/inspector/catalog.test.js`:

```js
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { CATALOG, listComponents, getComponent } from "../../src/inspector/catalog.js";

describe("catalog", () => {
  it("lists the documented built-in components", () => {
    const names = listComponents();
    for (const expected of [
      "notification", "menu-item", "file-picker", "badge", "shortcut",
      "shell", "fs", "sudo", "tray", "deep-link", "updater",
    ]) {
      assert.ok(names.includes(expected), `missing ${expected}`);
    }
  });

  it("every component has non-empty description, erb, and stimulus", () => {
    for (const name of listComponents()) {
      const c = getComponent(name);
      assert.ok(c.description && c.description.length > 0, `${name} description`);
      assert.ok(c.erb && c.erb.length > 0, `${name} erb`);
      assert.ok(c.stimulus && c.stimulus.length > 0, `${name} stimulus`);
    }
  });

  it("returns null for an unknown component", () => {
    assert.equal(getComponent("nope"), null);
  });

  it("exposes the raw map", () => {
    assert.equal(typeof CATALOG, "object");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test test/inspector/catalog.test.js`
Expected: FAIL — `Cannot find module '.../src/inspector/catalog.js'`.

- [ ] **Step 3: Write the implementation**

Create `src/inspector/catalog.js`:

```js
/**
 * Static catalog of built-in bridge components.
 * Single source of truth for the Inspector's "Available" list and snippets.
 * Each entry: { description, erb, stimulus }.
 */
export const CATALOG = {
  "notification": {
    description: "Show native OS notifications.",
    erb: `<button data-controller="notification"\n        data-action="click->notification#notify"\n        data-body="Saved!">Notify</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "notification") {\n  notify(e) { this.sendBridge("connect", { title: "My App", body: e.target.dataset.body }) }\n}`,
  },
  "menu-item": {
    description: "Register an item in the native menu bar.",
    erb: `<%= tag.button "Export PDF",\n      **turbo_desktop_bridge("menu-item", title: "Export PDF", shortcut: "Cmd+E") %>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "menu-item") {\n  connect() { super.connect(); this.sendBridge("register", { title: "Export PDF", shortcut: "Cmd+E" }) }\n}`,
  },
  "file-picker": {
    description: "Open a native file open/save dialog.",
    erb: `<button data-controller="file-picker"\n        data-action="click->file-picker#open">Choose file…</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "file-picker") {\n  open() { this.sendBridge("open", { multiple: false }) }\n  receiveBridge(msg) { console.log("picked", msg.data) }\n}`,
  },
  "badge": {
    description: "Set the dock / taskbar badge count.",
    erb: `<span data-controller="badge" data-badge-count-value="3"></span>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "badge") {\n  static values = { count: Number }\n  connect() { super.connect(); this.sendBridge("set", { count: this.countValue }) }\n}`,
  },
  "shortcut": {
    description: "Register a global keyboard shortcut.",
    erb: `<div data-controller="shortcut" data-shortcut-keys-value="CmdOrCtrl+K"></div>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "shortcut") {\n  static values = { keys: String }\n  connect() { super.connect(); this.sendBridge("register", { keys: this.keysValue }) }\n  receiveBridge() { /* fired when the shortcut is pressed */ }\n}`,
  },
  "shell": {
    description: "Spawn and manage native shell processes.",
    erb: `<button data-controller="shell" data-action="click->shell#run">Run</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "shell") {\n  run() { TurboDesktop.shell.spawn("job-1", "echo", ["hello"]) }\n}`,
  },
  "fs": {
    description: "Read and write files through the native filesystem bridge.",
    erb: `<button data-controller="fs" data-action="click->fs#read">Read file</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "fs") {\n  read() { this.sendBridge("read", { path: "~/notes.txt" }) }\n  receiveBridge(msg) { console.log(msg.data) }\n}`,
  },
  "sudo": {
    description: "Run a privileged command via the native elevation prompt.",
    erb: `<button data-controller="sudo" data-action="click->sudo#elevate">Install</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "sudo") {\n  elevate() { this.sendBridge("run", { command: "brew", args: ["install", "foo"] }) }\n}`,
  },
  "tray": {
    description: "Add items to the system tray / menu-bar icon.",
    erb: `<div data-controller="tray" data-tray-title-value="My App"></div>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "tray") {\n  static values = { title: String }\n  connect() { super.connect(); this.sendBridge("set", { tooltip: this.titleValue }) }\n}`,
  },
  "deep-link": {
    description: "Handle custom-scheme deep links opened from outside the app.",
    erb: `<div data-controller="deep-link"></div>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "deep-link") {\n  receiveBridge(msg) { Turbo.visit(msg.data.path) }\n}`,
  },
  "updater": {
    description: "Check for and apply native app updates.",
    erb: `<button data-controller="updater" data-action="click->updater#check">Check for updates</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "updater") {\n  check() { this.sendBridge("check", {}) }\n  receiveBridge(msg) { console.log("update status", msg.data) }\n}`,
  },
};

/** Names of every catalogued component. */
export function listComponents() {
  return Object.keys(CATALOG);
}

/** Look up one component's metadata, or null if unknown. */
export function getComponent(name) {
  return Object.prototype.hasOwnProperty.call(CATALOG, name) ? CATALOG[name] : null;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test test/inspector/catalog.test.js`
Expected: PASS — all four tests green.

- [ ] **Step 5: Commit**

```bash
git add src/inspector/catalog.js test/inspector/catalog.test.js
git commit -m "feat(inspector): add component catalog with snippets"
```

---

## Task 3: InspectorState

In-memory model: a bounded ring buffer of message records, a derived component summary, nav/shell facts, and a change emitter. Pure data, no DOM.

**Files:**
- Create: `src/inspector/state.js`
- Test: `test/inspector/state.test.js`

- [ ] **Step 1: Write the failing test**

Create `test/inspector/state.test.js`:

```js
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { InspectorState } from "../../src/inspector/state.js";

const rec = (over = {}) => ({ direction: "out", component: "notification", event: "connect", data: {}, ts: 1, ...over });

describe("InspectorState", () => {
  it("records messages and derives a component summary", () => {
    const s = new InspectorState();
    s.record(rec());
    s.record(rec({ event: "notify" }));
    s.record(rec({ component: "badge", event: "set" }));

    assert.equal(s.messages.length, 3);
    const comps = s.components();
    const notif = comps.find((c) => c.name === "notification");
    assert.equal(notif.count, 2);
    assert.equal(notif.lastEvent, "notify");
    assert.ok(comps.find((c) => c.name === "badge"));
  });

  it("evicts oldest messages past capacity", () => {
    const s = new InspectorState({ capacity: 2 });
    s.record(rec({ event: "a" }));
    s.record(rec({ event: "b" }));
    s.record(rec({ event: "c" }));
    assert.equal(s.messages.length, 2);
    assert.deepEqual(s.messages.map((m) => m.event), ["b", "c"]);
  });

  it("notifies subscribers on change and supports unsubscribe", () => {
    const s = new InspectorState();
    let calls = 0;
    const off = s.subscribe(() => { calls += 1; });
    s.record(rec());
    s.setNav({ presentation: "modal" });
    assert.equal(calls, 2);
    off();
    s.record(rec());
    assert.equal(calls, 2);
  });

  it("a throwing subscriber does not break emit", () => {
    const s = new InspectorState();
    s.subscribe(() => { throw new Error("boom"); });
    let ok = false;
    s.subscribe(() => { ok = true; });
    s.record(rec());
    assert.ok(ok);
  });

  it("stores nav and shell facts", () => {
    const s = new InspectorState();
    s.setNav({ url: "/x", presentation: "modal" });
    s.setShell({ platform: "macos", arch: "aarch64" });
    assert.equal(s.nav.presentation, "modal");
    assert.equal(s.shell.platform, "macos");
  });

  it("clear empties messages and component summary", () => {
    const s = new InspectorState();
    s.record(rec());
    s.clear();
    assert.equal(s.messages.length, 0);
    assert.equal(s.components().length, 0);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test test/inspector/state.test.js`
Expected: FAIL — cannot find `src/inspector/state.js`.

- [ ] **Step 3: Write the implementation**

Create `src/inspector/state.js`:

```js
/**
 * In-memory model for the Dev Inspector.
 * Holds a bounded ring buffer of message records plus a derived component
 * summary and nav/shell facts. Emits "change" to subscribers. No DOM.
 */
export class InspectorState {
  constructor({ capacity = 200 } = {}) {
    this.capacity = capacity;
    this.messages = [];
    this._components = new Map(); // name -> { count, lastEvent }
    this.nav = { url: null, presentation: null };
    this.shell = { platform: null, arch: null, version: null, serverUrl: null, updater: null };
    this._listeners = new Set();
  }

  /** Append a record { direction, component, event, data, ts }. */
  record(record) {
    this.messages.push(record);
    if (this.messages.length > this.capacity) this.messages.shift();

    const prev = this._components.get(record.component) || { count: 0, lastEvent: null };
    prev.count += 1;
    prev.lastEvent = record.event;
    this._components.set(record.component, prev);

    this._emit();
  }

  /** Derived component summary as an array. */
  components() {
    return [...this._components.entries()].map(([name, v]) => ({ name, count: v.count, lastEvent: v.lastEvent }));
  }

  setNav(nav) {
    this.nav = { ...this.nav, ...nav };
    this._emit();
  }

  setShell(shell) {
    this.shell = { ...this.shell, ...shell };
    this._emit();
  }

  clear() {
    this.messages = [];
    this._components.clear();
    this._emit();
  }

  /** Subscribe to changes; returns an unsubscribe function. */
  subscribe(fn) {
    this._listeners.add(fn);
    return () => this._listeners.delete(fn);
  }

  _emit() {
    for (const fn of this._listeners) {
      try { fn(this); } catch (_e) { /* a bad subscriber must not break others */ }
    }
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test test/inspector/state.test.js`
Expected: PASS — all six tests green.

- [ ] **Step 5: Commit**

```bash
git add src/inspector/state.js test/inspector/state.test.js
git commit -m "feat(inspector): add InspectorState ring buffer and emitter"
```

---

## Task 4: BridgeTap

Wrap the host's `sendBridgeMessage` and observe inbound responses, emitting records to a callback. The critical property: strict pass-through — the original is called exactly once and its return/throw is preserved unchanged.

**Files:**
- Create: `src/inspector/bridge-tap.js`
- Test: `test/inspector/bridge-tap.test.js`

- [ ] **Step 1: Write the failing test**

Create `test/inspector/bridge-tap.test.js`:

```js
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { BridgeTap } from "../../src/inspector/bridge-tap.js";

function makeHost(impl) {
  return { sendBridgeMessage: impl };
}

describe("BridgeTap", () => {
  it("records outbound calls and preserves the return value", async () => {
    let calls = 0;
    const host = makeHost(async (c, e, d) => { calls += 1; return { ok: true, c, e, d }; });
    const records = [];
    const tap = new BridgeTap(host, { onRecord: (r) => records.push(r), now: () => 7 });
    tap.install();

    const result = await host.sendBridgeMessage("notification", "connect", { title: "Hi" });

    assert.equal(calls, 1, "original called exactly once");
    assert.deepEqual(result, { ok: true, c: "notification", e: "connect", d: { title: "Hi" } });
    assert.equal(records.length, 1);
    assert.deepEqual(records[0], { direction: "out", component: "notification", event: "connect", data: { title: "Hi" }, ts: 7 });
  });

  it("preserves a thrown/rejected error from the original", async () => {
    const host = makeHost(async () => { throw new Error("native fail"); });
    const tap = new BridgeTap(host, { onRecord: () => {} });
    tap.install();
    await assert.rejects(() => host.sendBridgeMessage("x", "y"), /native fail/);
  });

  it("a throwing onRecord never affects the real call", async () => {
    let calls = 0;
    const host = makeHost(async () => { calls += 1; return "value"; });
    const tap = new BridgeTap(host, { onRecord: () => { throw new Error("record boom"); } });
    tap.install();
    const result = await host.sendBridgeMessage("a", "b");
    assert.equal(result, "value");
    assert.equal(calls, 1);
  });

  it("observeResponse records inbound payloads", () => {
    const records = [];
    const tap = new BridgeTap(makeHost(async () => {}), { onRecord: (r) => records.push(r), now: () => 3 });
    tap.observeResponse({ component: "file-picker", event: "selected", data: { path: "/a" } });
    assert.deepEqual(records[0], { direction: "in", component: "file-picker", event: "selected", data: { path: "/a" }, ts: 3 });
  });

  it("observeResponse ignores null payloads", () => {
    const records = [];
    const tap = new BridgeTap(makeHost(async () => {}), { onRecord: (r) => records.push(r) });
    tap.observeResponse(null);
    assert.equal(records.length, 0);
  });

  it("uninstall restores the original function", async () => {
    const original = async () => "orig";
    const host = makeHost(original);
    const tap = new BridgeTap(host, { onRecord: () => {} });
    tap.install();
    tap.uninstall();
    assert.equal(host.sendBridgeMessage, original);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test test/inspector/bridge-tap.test.js`
Expected: FAIL — cannot find `src/inspector/bridge-tap.js`.

- [ ] **Step 3: Write the implementation**

Create `src/inspector/bridge-tap.js`:

```js
/**
 * Observes bridge traffic without altering it.
 *
 * install() wraps host.sendBridgeMessage so every outbound call is recorded
 * then forwarded to the original exactly once, with its return/throw preserved.
 * observeResponse() records inbound bridge-response payloads.
 *
 * Recording runs inside its own try/catch so a logging bug can never affect a
 * real bridge call. No DOM access.
 */
export class BridgeTap {
  constructor(host, { onRecord, now = () => Date.now() } = {}) {
    this.host = host;
    this.onRecord = onRecord;
    this.now = now;
    this._installed = false;
    this._original = null;
  }

  install() {
    if (this._installed) return;
    const original = this.host.sendBridgeMessage.bind(this.host);
    this._original = original;
    const self = this;
    this.host.sendBridgeMessage = function (component, event, data = {}) {
      self._safeRecord({ direction: "out", component, event, data, ts: self.now() });
      return original(component, event, data); // pass-through, exactly once
    };
    this._installed = true;
  }

  /** Record an inbound bridge-response payload. */
  observeResponse(payload) {
    if (!payload) return;
    this._safeRecord({
      direction: "in",
      component: payload.component,
      event: payload.event || "response",
      data: payload.data !== undefined ? payload.data : payload,
      ts: this.now(),
    });
  }

  uninstall() {
    if (this._installed && this._original) {
      this.host.sendBridgeMessage = this._original;
      this._installed = false;
    }
  }

  _safeRecord(record) {
    try {
      if (this.onRecord) this.onRecord(record);
    } catch (_e) {
      /* recording must never break the host */
    }
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test test/inspector/bridge-tap.test.js`
Expected: PASS — all six tests green.

- [ ] **Step 5: Commit**

```bash
git add src/inspector/bridge-tap.js test/inspector/bridge-tap.test.js
git commit -m "feat(inspector): add BridgeTap pass-through observer"
```

---

## Task 5: InspectorPanel

The Shadow-DOM overlay. Renders state into an isolated root, supports tab switching, a message filter, and show/hide toggle. Tested under jsdom.

**Files:**
- Create: `src/inspector/panel.js`
- Test: `test/inspector/panel.test.js`

- [ ] **Step 1: Write the failing test**

Create `test/inspector/panel.test.js`:

```js
import { describe, it, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import { InspectorState } from "../../src/inspector/state.js";
import { InspectorPanel } from "../../src/inspector/panel.js";

let dom, document, state, panel;

beforeEach(() => {
  dom = new JSDOM("<!DOCTYPE html><html><body></body></html>", { pretendToBeVisual: true });
  document = dom.window.document;
  state = new InspectorState();
  panel = new InspectorPanel(state, { document });
  panel.mount(document.body);
});

describe("InspectorPanel", () => {
  it("mounts a shadow root host and starts hidden", () => {
    const host = document.querySelector("[data-turbo-desktop-inspector]");
    assert.ok(host, "host element exists");
    assert.ok(host.shadowRoot, "uses shadow DOM");
    assert.equal(panel.visible, false);
  });

  it("renders the four tabs", () => {
    const labels = [...panel.root.querySelectorAll("[data-tab]")].map((b) => b.dataset.tab);
    assert.deepEqual(labels.sort(), ["components", "messages", "navigation", "shell"]);
  });

  it("toggle shows and hides the panel", () => {
    panel.toggle();
    assert.equal(panel.visible, true);
    panel.toggle();
    assert.equal(panel.visible, false);
  });

  it("lists every catalogued component in the Components tab", () => {
    panel.show();
    panel.selectTab("components");
    const text = panel.root.querySelector('[data-panel="components"]').textContent;
    assert.match(text, /notification/);
    assert.match(text, /updater/);
  });

  it("renders recorded messages and filters them", () => {
    panel.show();
    panel.selectTab("messages");
    state.record({ direction: "out", component: "notification", event: "connect", data: {}, ts: 1 });
    state.record({ direction: "out", component: "badge", event: "set", data: {}, ts: 2 });

    let rows = panel.root.querySelectorAll('[data-panel="messages"] [data-message-row]');
    assert.equal(rows.length, 2);

    panel.setFilter("badge");
    rows = panel.root.querySelectorAll('[data-panel="messages"] [data-message-row]');
    assert.equal(rows.length, 1);
    assert.match(rows[0].textContent, /badge/);
  });

  it("shows the current presentation in the Navigation tab", () => {
    panel.show();
    panel.selectTab("navigation");
    state.setNav({ url: "/reports/1", presentation: "new_window" });
    const text = panel.root.querySelector('[data-panel="navigation"]').textContent;
    assert.match(text, /new_window/);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test test/inspector/panel.test.js`
Expected: FAIL — cannot find `src/inspector/panel.js`.

- [ ] **Step 3: Write the implementation**

Create `src/inspector/panel.js`:

```js
import { listComponents, getComponent } from "./catalog.js";

/**
 * Shadow-DOM overlay that renders an InspectorState.
 * Subscribes to state changes; never mutates state. No Tauri access.
 */
export class InspectorPanel {
  constructor(state, { document }) {
    this.state = state;
    this.document = document;
    this.visible = false;
    this.activeTab = "components";
    this.filter = "";
    this.hostEl = null;
    this.root = null; // shadow root
    this._unsub = null;
  }

  mount(parent) {
    const host = this.document.createElement("div");
    host.setAttribute("data-turbo-desktop-inspector", "");
    host.style.cssText = "position:fixed;right:0;bottom:0;z-index:2147483647;";
    this.root = host.attachShadow ? host.attachShadow({ mode: "open" }) : host;
    this.hostEl = host;
    parent.appendChild(host);
    this._unsub = this.state.subscribe(() => this.render());
    this.render();
    this._applyVisibility();
    return this;
  }

  unmount() {
    if (this._unsub) this._unsub();
    if (this.hostEl && this.hostEl.parentNode) this.hostEl.parentNode.removeChild(this.hostEl);
  }

  show() { this.visible = true; this._applyVisibility(); }
  hide() { this.visible = false; this._applyVisibility(); }
  toggle() { this.visible = !this.visible; this._applyVisibility(); }

  selectTab(tab) { this.activeTab = tab; this.render(); }
  setFilter(text) { this.filter = text || ""; this.render(); }

  _applyVisibility() {
    if (this.hostEl) this.hostEl.style.display = this.visible ? "block" : "none";
  }

  render() {
    if (!this.root) return;
    try {
      this.root.innerHTML = this._html();
      this._wire();
    } catch (_e) {
      this.root.innerHTML = "<div>&lt;unrenderable&gt;</div>";
    }
  }

  _wire() {
    const q = (sel) => this.root.querySelectorAll(sel);
    q("[data-tab]").forEach((btn) => {
      btn.addEventListener("click", () => this.selectTab(btn.dataset.tab));
    });
    const input = this.root.querySelector("[data-filter]");
    if (input) input.addEventListener("input", (e) => this.setFilter(e.target.value));
  }

  _html() {
    return `
      <style>
        :host { all: initial; }
        .wrap { font: 12px/1.4 monospace; width: 420px; height: 320px; background:#111; color:#eee; border:1px solid #333; display:flex; flex-direction:column; }
        .tabs { display:flex; }
        .tabs button { flex:1; background:#222; color:#ccc; border:0; padding:6px; cursor:pointer; }
        .tabs button[aria-selected="true"] { background:#0a84ff; color:#fff; }
        .body { flex:1; overflow:auto; padding:8px; }
        .row { padding:2px 0; border-bottom:1px solid #222; }
        .muted { color:#888; }
        input { width:100%; box-sizing:border-box; margin-bottom:6px; background:#000; color:#eee; border:1px solid #333; }
      </style>
      <div class="wrap">
        <div class="tabs">
          ${["components", "messages", "navigation", "shell"].map((t) =>
            `<button data-tab="${t}" aria-selected="${this.activeTab === t}">${t}</button>`).join("")}
        </div>
        <div class="body">
          ${this._panelHtml()}
        </div>
      </div>`;
  }

  _panelHtml() {
    switch (this.activeTab) {
      case "messages": return this._messagesHtml();
      case "navigation": return this._navHtml();
      case "shell": return this._shellHtml();
      default: return this._componentsHtml();
    }
  }

  _componentsHtml() {
    const active = new Map(this.state.components().map((c) => [c.name, c]));
    const rows = listComponents().map((name) => {
      const c = getComponent(name);
      const seen = active.get(name);
      const tag = seen ? `<span>×${seen.count} (${seen.lastEvent})</span>` : `<span class="muted">available</span>`;
      return `<div class="row"><strong>${name}</strong> ${tag}<br><span class="muted">${c.description}</span></div>`;
    }).join("");
    return `<div data-panel="components">${rows}</div>`;
  }

  _messagesHtml() {
    const f = this.filter.toLowerCase();
    const rows = this.state.messages
      .filter((m) => !f || (m.component || "").toLowerCase().includes(f))
      .map((m) => {
        const arrow = m.direction === "out" ? "↑" : "↓";
        return `<div class="row" data-message-row>${arrow} <strong>${m.component}</strong> ${m.event} <span class="muted">${JSON.stringify(m.data)}</span></div>`;
      }).join("");
    return `<div data-panel="messages"><input data-filter placeholder="filter by component" value="${this.filter}">${rows}</div>`;
  }

  _navHtml() {
    const n = this.state.nav;
    return `<div data-panel="navigation"><div class="row">URL: ${n.url || "—"}</div><div class="row">Presentation: <strong>${n.presentation || "default"}</strong></div></div>`;
  }

  _shellHtml() {
    const s = this.state.shell;
    return `<div data-panel="shell">
      <div class="row">Platform: ${s.platform || "—"}</div>
      <div class="row">Arch: ${s.arch || "—"}</div>
      <div class="row">Version: ${s.version || "—"}</div>
      <div class="row">Server: ${s.serverUrl || "—"}</div>
      <div class="row">Updater: ${s.updater || "—"}</div>
    </div>`;
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test test/inspector/panel.test.js`
Expected: PASS — all seven tests green.

- [ ] **Step 5: Commit**

```bash
git add src/inspector/panel.js test/inspector/panel.test.js
git commit -m "feat(inspector): add Shadow-DOM InspectorPanel"
```

---

## Task 6: Inspector entry point

`startInspector(host, env)` wires the units together: create state, install the tap, subscribe to inbound responses, seed shell info, mount the panel, bind the hotkey. Replaces the empty `src/inspector.js`.

**Files:**
- Modify: `src/inspector.js` (currently empty)
- Test: `test/inspector/inspector.test.js`

- [ ] **Step 1: Write the failing test**

Create `test/inspector/inspector.test.js`:

```js
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import { startInspector } from "../../src/inspector.js";

function setup() {
  const dom = new JSDOM("<!DOCTYPE html><html><body></body></html>", { pretendToBeVisual: true });
  const { window } = dom;
  const listeners = [];
  const host = {
    platform: "macos",
    version: "0.1.0",
    sendBridgeMessage: async () => "ok",
    getWindowInfo: async () => ({ arch: "aarch64", serverUrl: "http://localhost:3000" }),
  };
  window.__TAURI_INTERNALS__ = { event: { listen: (name, cb) => listeners.push({ name, cb }) } };
  return { window, document: window.document, host, listeners };
}

describe("startInspector", () => {
  it("taps outbound bridge calls into the panel state", async () => {
    const { window, document, host } = setup();
    const ctx = startInspector(host, { doc: document, win: window });
    await host.sendBridgeMessage("notification", "connect", { title: "Hi" });
    assert.equal(ctx.state.messages.length, 1);
    assert.equal(ctx.state.messages[0].component, "notification");
  });

  it("records inbound bridge-response events", () => {
    const { window, document, host, listeners } = setup();
    const ctx = startInspector(host, { doc: document, win: window });
    const sub = listeners.find((l) => l.name === "bridge-response");
    assert.ok(sub, "subscribed to bridge-response");
    sub.cb({ payload: { component: "file-picker", event: "selected", data: { path: "/a" } } });
    assert.equal(ctx.state.messages.at(-1).direction, "in");
    assert.equal(ctx.state.messages.at(-1).component, "file-picker");
  });

  it("mounts the panel hidden and the hotkey toggles it", () => {
    const { window, document, host } = setup();
    const ctx = startInspector(host, { doc: document, win: window });
    assert.equal(ctx.panel.visible, false);
    const ev = new window.KeyboardEvent("keydown", { key: "D", metaKey: true, shiftKey: true });
    window.dispatchEvent(ev);
    assert.equal(ctx.panel.visible, true);
  });

  it("seeds shell info from the host", async () => {
    const { window, document, host } = setup();
    const ctx = startInspector(host, { doc: document, win: window });
    assert.equal(ctx.state.shell.platform, "macos");
    await Promise.resolve();
    await Promise.resolve();
    assert.equal(ctx.state.shell.arch, "aarch64");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test test/inspector/inspector.test.js`
Expected: FAIL — `startInspector` is not exported (file is empty).

- [ ] **Step 3: Write the implementation**

Replace the contents of `src/inspector.js` with:

```js
/**
 * Dev Inspector entry point.
 *
 * startInspector(host, env) wires the inspector units onto a TurboDesktop-like
 * host: it installs a BridgeTap on the host's sendBridgeMessage, subscribes to
 * inbound bridge-response events, seeds shell facts, mounts the Shadow-DOM
 * panel, and binds the toggle hotkey (Cmd/Ctrl+Shift+D).
 *
 * This module is loaded lazily by turbo-desktop.js only when the inspector gate
 * passes, so it ships no code to production builds.
 */
import { BridgeTap } from "./inspector/bridge-tap.js";
import { InspectorState } from "./inspector/state.js";
import { InspectorPanel } from "./inspector/panel.js";

export function startInspector(host, { doc = document, win = window } = {}) {
  const state = new InspectorState();

  const now = (win.Date && typeof win.Date.now === "function") ? () => win.Date.now() : () => 0;
  const tap = new BridgeTap(host, { onRecord: (r) => state.record(r), now });
  tap.install();

  const internals = win.__TAURI_INTERNALS__;
  if (internals && internals.event && typeof internals.event.listen === "function") {
    internals.event.listen("bridge-response", (e) => tap.observeResponse(e && e.payload));
  }

  state.setShell({ platform: host.platform, version: host.version });
  if (typeof host.getWindowInfo === "function") {
    host.getWindowInfo().then((info) => { if (info) state.setShell(info); }).catch(() => {});
  }

  const panel = new InspectorPanel(state, { document: doc });
  panel.mount(doc.body);

  win.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key === "D" || e.key === "d")) {
      e.preventDefault();
      panel.toggle();
    }
  });

  return { state, tap, panel };
}

export default startInspector;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test test/inspector/inspector.test.js`
Expected: PASS — all four tests green.

- [ ] **Step 5: Run the full JS suite**

Run: `npm test`
Expected: PASS — existing `turbo-desktop` suite plus all five inspector suites green.

- [ ] **Step 6: Commit**

```bash
git add src/inspector.js test/inspector/inspector.test.js
git commit -m "feat(inspector): add startInspector entry point and hotkey"
```

---

## Task 7: Core integration (gate + lazy load)

Add the enablement gate and the lazy `import()` to the core script. The gate is exposed for testing; the actual `import()` is guarded behind it and the Tauri presence check, so it never runs in tests or plain browsers.

**Files:**
- Modify: `src/turbo-desktop.js` (end of the IIFE, before the closing `})();`)
- Test: `test/inspector/gate.test.js`

- [ ] **Step 1: Find the exposure point**

Run: `grep -n "window.__TURBO_DESKTOP__\|__TURBO_DESKTOP__ =\|})();" src/turbo-desktop.js | tail -5`
Expected: shows where `TurboDesktop` is assigned to `window.__TURBO_DESKTOP__` and the IIFE close `})();`. Insert the new block immediately before `})();`.

- [ ] **Step 2: Write the failing test**

Create `test/inspector/gate.test.js`:

```js
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const scriptSource = readFileSync(resolve(__dirname, "../../src/turbo-desktop.js"), "utf-8");

function load({ storage = {}, metaEnabled = false, globalEnabled = false } = {}) {
  const dom = new JSDOM("<!DOCTYPE html><html><head></head><body></body></html>", {
    url: "https://myapp.test/", runScripts: "dangerously", pretendToBeVisual: true,
  });
  const { window } = dom;
  // jsdom localStorage is read-only-ish; stub a simple one
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: { getItem: (k) => (k in storage ? storage[k] : null) },
  });
  if (metaEnabled) {
    const m = window.document.createElement("meta");
    m.setAttribute("name", "turbo-desktop-inspector");
    m.setAttribute("content", "enabled");
    window.document.head.appendChild(m);
  }
  if (globalEnabled) window.__TURBO_DESKTOP_INSPECTOR_ENABLED__ = true;
  window.eval(scriptSource);
  return window;
}

describe("inspector gate", () => {
  it("is disabled by default", () => {
    const w = load();
    assert.equal(w.__TURBO_DESKTOP__._inspectorEnabled(), false);
  });

  it("enables via localStorage td:inspector = 1", () => {
    const w = load({ storage: { "td:inspector": "1" } });
    assert.equal(w.__TURBO_DESKTOP__._inspectorEnabled(), true);
  });

  it("enables via meta tag", () => {
    const w = load({ metaEnabled: true });
    assert.equal(w.__TURBO_DESKTOP__._inspectorEnabled(), true);
  });

  it("enables via global flag", () => {
    const w = load({ globalEnabled: true });
    assert.equal(w.__TURBO_DESKTOP__._inspectorEnabled(), true);
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `node --test test/inspector/gate.test.js`
Expected: FAIL — `_inspectorEnabled` is not a function (undefined).

- [ ] **Step 4: Add the gate and lazy import**

In `src/turbo-desktop.js`, immediately before the IIFE's closing `})();`, insert:

```js
  // ─── Dev Inspector (lazy, dev-only) ──────────────────────────────────────
  function inspectorEnabled() {
    try {
      if (window.localStorage && window.localStorage.getItem("td:inspector") === "1") return true;
    } catch (_e) { /* storage may be blocked */ }
    if (document.querySelector('meta[name="turbo-desktop-inspector"][content="enabled"]')) return true;
    if (window.__TURBO_DESKTOP_INSPECTOR_ENABLED__ === true) return true;
    return false;
  }
  TurboDesktop._inspectorEnabled = inspectorEnabled;

  if (INVOKE && inspectorEnabled()) {
    // The Tauri shell sets __TURBO_DESKTOP_INSPECTOR_URL__ to the injected
    // asset URL; fall back to a relative path for bundled setups.
    var inspectorUrl = window.__TURBO_DESKTOP_INSPECTOR_URL__ || "./inspector.js";
    import(inspectorUrl)
      .then(function (m) { m.startInspector(TurboDesktop, { doc: document, win: window }); })
      .catch(function (e) { console.error("[turbo-desktop] inspector failed to load", e); });
  }
```

> Note: `_inspectorEnabled` is exposed for testing and runtime introspection. The `import()` only runs when both `INVOKE` (real Tauri) is present and the gate passes, so it never fires under jsdom or in a plain browser. Wiring `__TURBO_DESKTOP_INSPECTOR_URL__` to the shell's injected asset path is a small Tauri-side follow-up; the JS contract is complete here.

- [ ] **Step 5: Run test to verify it passes**

Run: `node --test test/inspector/gate.test.js`
Expected: PASS — all four gate tests green.

- [ ] **Step 6: Run the full JS suite (no regressions)**

Run: `npm test`
Expected: PASS — every suite green, including the original `turbo-desktop.test.js`.

- [ ] **Step 7: Commit**

```bash
git add src/turbo-desktop.js test/inspector/gate.test.js
git commit -m "feat(inspector): gate and lazily load the inspector from core"
```

---

## Task 8: Rails gem enablement

Give Rails developers a one-line, development-only way to turn the inspector on: a config flag plus a helper that emits the enabling `<meta>` tag.

**Files:**
- Modify: `turbo_desktop-rails/lib/turbo_desktop/configuration.rb`
- Modify: `turbo_desktop-rails/lib/turbo_desktop/view_helpers.rb`
- Modify: `turbo_desktop-rails/lib/generators/turbo_desktop/install/templates/initializer.rb.tt`
- Test: `turbo_desktop-rails/test/configuration_test.rb` (extend)
- Test: `turbo_desktop-rails/test/view_helpers_test.rb` (extend)

- [ ] **Step 1: Write the failing config test**

Append to `turbo_desktop-rails/test/configuration_test.rb` (inside the existing test class):

```ruby
  def test_inspector_disabled_by_default
    config = TurboDesktop::Configuration.new
    refute config.inspector_enabled
  end

  def test_inspector_can_be_enabled
    config = TurboDesktop::Configuration.new
    config.inspector_enabled = true
    assert config.inspector_enabled
  end
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd turbo_desktop-rails && ruby -Itest -Ilib test/configuration_test.rb`
Expected: FAIL — `NoMethodError: undefined method 'inspector_enabled'`.

- [ ] **Step 3: Add the config accessor**

In `turbo_desktop-rails/lib/turbo_desktop/configuration.rb`, change the `attr_accessor` line and `initialize`:

```ruby
    attr_accessor :path_configuration, :user_agent_pattern, :inspector_enabled

    def initialize
      @path_configuration = default_path_configuration
      @user_agent_pattern = /Turbo Desktop/
      @inspector_enabled = false
    end
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cd turbo_desktop-rails && ruby -Itest -Ilib test/configuration_test.rb`
Expected: PASS.

- [ ] **Step 5: Write the failing helper test**

Append to `turbo_desktop-rails/test/view_helpers_test.rb` (inside the test class). Note the host stubs `tag` to mirror Rails' tag helper minimally:

```ruby
  def test_inspector_predicate_reflects_config
    TurboDesktop.configuration.inspector_enabled = true
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    assert host.turbo_desktop_inspector?
  ensure
    TurboDesktop.configuration.inspector_enabled = false
  end

  def test_inspector_meta_tag_present_when_enabled
    TurboDesktop.configuration.inspector_enabled = true
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    assert_includes host.turbo_desktop_inspector_meta_tag.to_s, "turbo-desktop-inspector"
    assert_includes host.turbo_desktop_inspector_meta_tag.to_s, "enabled"
  ensure
    TurboDesktop.configuration.inspector_enabled = false
  end

  def test_inspector_meta_tag_absent_when_disabled
    host = ViewHelpersTestHost.new(DESKTOP_UA)
    assert_nil host.turbo_desktop_inspector_meta_tag
  end
```

Also add a minimal `tag` stub to `ViewHelpersTestHost` (near its `capture` stub):

```ruby
  # Minimal stub for Rails' tag.meta helper
  def tag
    @tag ||= Class.new do
      def meta(**attrs)
        pairs = attrs.map { |k, v| %(#{k.to_s.tr("_", "-")}="#{v}") }.join(" ")
        "<meta #{pairs}>"
      end
    end.new
  end
```

- [ ] **Step 6: Run it to verify it fails**

Run: `cd turbo_desktop-rails && ruby -Itest -Ilib test/view_helpers_test.rb`
Expected: FAIL — `undefined method 'turbo_desktop_inspector?'`.

- [ ] **Step 7: Add the helpers**

In `turbo_desktop-rails/lib/turbo_desktop/view_helpers.rb`, add inside the `ViewHelpers` module:

```ruby
    # Returns true when the Dev Inspector is enabled in configuration.
    def turbo_desktop_inspector?
      TurboDesktop.configuration.inspector_enabled
    end

    # Emits the <meta> tag that enables the Dev Inspector in the browser, or nil
    # when the inspector is disabled. Place in your layout <head>; it is a no-op
    # in production unless you explicitly enable the inspector there.
    #
    #   <%= turbo_desktop_inspector_meta_tag %>
    def turbo_desktop_inspector_meta_tag
      return nil unless turbo_desktop_inspector?

      tag.meta(name: "turbo-desktop-inspector", content: "enabled")
    end
```

- [ ] **Step 8: Run it to verify it passes**

Run: `cd turbo_desktop-rails && ruby -Itest -Ilib test/view_helpers_test.rb`
Expected: PASS.

- [ ] **Step 9: Document enabling in the initializer template**

In `turbo_desktop-rails/lib/generators/turbo_desktop/install/templates/initializer.rb.tt`, add inside the `TurboDesktop.configure do |config|` block, before the `config.path_configuration` assignment:

```ruby
  # Dev Inspector — an in-app overlay (Cmd/Ctrl+Shift+D) that surfaces available
  # bridge components, a live web↔native message log, and the current path-config
  # presentation. Enable it in development only:
  config.inspector_enabled = Rails.env.development?

```

- [ ] **Step 10: Run the gem's full suite**

Run: `cd turbo_desktop-rails && rake test`
Expected: PASS — configuration, view-helper, detection, controller, and generator suites all green.

- [ ] **Step 11: Commit**

```bash
git add turbo_desktop-rails/lib turbo_desktop-rails/test
git commit -m "feat(rails): add dev-only inspector enable flag and meta-tag helper"
```

---

## Task 9: Documentation + final verification

Document the inspector in the README and confirm the whole tree is green.

**Files:**
- Modify: `README.md` (add a "Dev Inspector" subsection under Bridge Components)

- [ ] **Step 1: Add a README section**

In `README.md`, after the "Built-in Components" table, add:

```markdown
### Dev Inspector

In development, press **Cmd/Ctrl+Shift+D** to open the Dev Inspector — an in-app
overlay that shows:

- **Components** — every available bridge component, with a copy-pasteable
  Rails + Stimulus snippet, and which are active on the current page
- **Messages** — a live log of web↔native bridge traffic
- **Navigation** — the path-configuration presentation applied to the current URL
- **Shell** — platform, arch, version, and server URL

Enable it from the Rails gem (added by the installer in development):

\`\`\`ruby
# config/initializers/turbo_desktop.rb
config.inspector_enabled = Rails.env.development?
\`\`\`

\`\`\`erb
<%# app/views/layouts/application.html.erb, in <head> %>
<%= turbo_desktop_inspector_meta_tag %>
\`\`\`

Or flip it on against any build without a rebuild:
\`localStorage.setItem("td:inspector", "1")\`.
```

- [ ] **Step 2: Run the complete JS suite**

Run: `npm test`
Expected: PASS — all JS suites green.

- [ ] **Step 3: Run the complete gem suite**

Run: `cd turbo_desktop-rails && rake test`
Expected: PASS — all Ruby suites green.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document the Dev Inspector"
```

---

## Verification Checklist (final)

- [ ] `npm test` is green (existing + 6 new inspector suites).
- [ ] `cd turbo_desktop-rails && rake test` is green.
- [ ] With the inspector disabled, `src/turbo-desktop.js` never imports `inspector.js` (verified by the gate test and the `INVOKE && enabled` guard).
- [ ] `BridgeTap` calls the original `sendBridgeMessage` exactly once and preserves its return/throw (verified in `bridge-tap.test.js`).
- [ ] The panel uses a Shadow root, so host-app CSS cannot leak into it (verified in `panel.test.js`).

## Manual Smoke (in the example app, after merge)

1. In `turbo_desktop_example_app`, set `config.inspector_enabled = true` and add `<%= turbo_desktop_inspector_meta_tag %>` to the layout head.
2. `cargo tauri dev`; press Cmd/Ctrl+Shift+D — the overlay appears.
3. Open the Components tab — confirm all 11 components listed; click one and copy its snippet.
4. Trigger a notification — confirm an `↑ notification` row appears in Messages.
5. Visit a `/new` or `/edit` route — confirm Navigation shows `modal`.
