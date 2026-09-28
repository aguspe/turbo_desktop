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
    proposeVisit: async (url, action) => ({ action: action || "advance", presentation: "modal" }),
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

  it("taps proposeVisit and records nav presentation, preserving the return value", async () => {
    const { window, document, host } = setup();
    const ctx = startInspector(host, { doc: document, win: window });
    const result = await host.proposeVisit("/new", "advance");
    assert.deepEqual(result, { action: "advance", presentation: "modal" });
    assert.equal(ctx.state.nav.url, "/new");
    assert.equal(ctx.state.nav.presentation, "modal");
  });
});

describe("the inspector, across Turbo navigation", () => {
  const HOST = "[data-turbo-desktop-inspector]";

  it("is not inside the part of the page Turbo replaces", () => {
    const { window, document, host } = setup();
    startInspector(host, { doc: document, win: window });

    assert.equal(document.body.querySelector(HOST), null, "mounted in the body, which Turbo swaps");
    assert.ok(document.documentElement.querySelector(HOST));
  });

  it("is still there after Turbo has rendered another page", () => {
    const { window, document, host } = setup();
    startInspector(host, { doc: document, win: window });

    // What a Turbo visit does.
    const next = document.createElement("body");
    next.innerHTML = "<h1>Tasks</h1>";
    document.documentElement.replaceChild(next, document.body);
    document.dispatchEvent(new window.Event("turbo:render"));

    assert.equal(document.querySelectorAll(HOST).length, 1);
  });

  it("comes back if something takes it out of the page", () => {
    const { window, document, host } = setup();
    const { panel } = startInspector(host, { doc: document, win: window });

    panel.hostEl.remove();
    document.dispatchEvent(new window.Event("turbo:load"));

    assert.equal(document.querySelectorAll(HOST).length, 1);
  });

  it("stays open across a visit if it was open", () => {
    const { window, document, host } = setup();
    const { panel } = startInspector(host, { doc: document, win: window });
    panel.show();

    panel.hostEl.remove();
    document.dispatchEvent(new window.Event("turbo:load"));

    assert.equal(document.querySelector(HOST).style.display, "block");
  });
});
