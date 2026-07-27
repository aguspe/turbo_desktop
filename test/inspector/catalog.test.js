import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { CATALOG, listComponents, getComponent } from "../../src/inspector/catalog.js";

describe("catalog", () => {
  it("lists the documented built-in components", () => {
    const names = listComponents();
    for (const expected of [
      "notification", "menu-item", "file-picker", "badge", "shortcut",
      "shell", "filesystem", "sudo", "tray", "deep-link", "updater",
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

  it("has exactly 11 components", () => {
    assert.equal(Object.keys(CATALOG).length, 11);
  });
});
