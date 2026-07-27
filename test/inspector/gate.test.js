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
