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

  it("escapes a double-quote in the filter without breaking the input value", () => {
    panel.show();
    panel.selectTab("messages");
    panel.setFilter('a"b');
    const input = panel.root.querySelector("[data-filter]");
    assert.equal(input.value, 'a"b');
  });

  it("does not inject markup from a message component name", () => {
    panel.show();
    panel.selectTab("messages");
    state.record({ direction: "out", component: "<img src=x>", event: "e", data: {}, ts: 1 });
    assert.equal(panel.root.querySelectorAll("img").length, 0);
  });
});
