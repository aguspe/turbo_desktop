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
