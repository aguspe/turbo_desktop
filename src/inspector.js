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

  if (typeof host.proposeVisit === "function") {
    const originalProposeVisit = host.proposeVisit.bind(host);
    host.proposeVisit = async function (url, action) {
      const result = await originalProposeVisit(url, action);
      try {
        state.setNav({ url, presentation: result && result.presentation ? result.presentation : "default" });
      } catch (_e) { /* nav recording must never break navigation */ }
      return result;
    };
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
