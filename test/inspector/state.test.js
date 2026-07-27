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
