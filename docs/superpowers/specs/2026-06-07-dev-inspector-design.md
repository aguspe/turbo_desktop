# Dev Inspector — Design

**Date:** 2026-06-07
**Status:** Approved (design), pending implementation plan
**Goal:** Adoption / DX — solve **discoverability** as the primary first-run friction.

## Problem

Turbo Desktop already ships substantial native capability: path-config presentations
(`modal`, `new_window`, `native`, `replace`, `none`), five documented bridge components
(notification, menu-item, file-picker, badge, shortcut), and additional Rust modules that
are present but under-documented (`tray`, `fs_bridge`, `sudo_bridge`, `shell_bridge`,
`updater_bridge`, `deep_link`, `process_manager`, `menu`).

The capability exists; the **visibility** does not. A Rails developer trying the project
cannot see which bridge components exist, which are active on the current page, what
messages are flowing between web and native, or which path-configuration rule applied to
the current URL. There is an empty `src/inspector.js` placeholder signalling prior intent
but no implementation. This gap is the chosen adoption friction: discoverability (with a
secondary payoff for debugging confidence).

## Solution Overview

A dev-only, in-app **Dev Inspector** overlay injected by `turbo-desktop.js` and driven by a
self-contained `inspector.js` module. It taps the single existing chokepoints for bridge
traffic, requires no behavioral change to the host Rails app, and ships zero code to
production end users.

Toggle hotkey: `Cmd/Ctrl+Shift+D`. Off by default; only loads when explicitly enabled.

### Key architectural fact

All bridge traffic already funnels through two existing chokepoints, so the inspector can
observe everything from JavaScript alone with minimal/no Rust changes for v1:

- **Outbound:** `TurboDesktop.sendBridgeMessage(component, event, data)`
- **Inbound:** the Tauri `"bridge-response"` event (payload carries `.component`)
- **Navigation:** `proposeVisit` → `handle_visit_proposal`, whose response carries the
  resolved `presentation`.
- **Desktop detection:** `window.__TAURI_INTERNALS__`.

## Architecture & Boundaries

Three internal units, each independently testable:

1. **`BridgeTap`** — wraps `sendBridgeMessage` and the `bridge-response` listener to emit a
   stream of `{direction, component, event, data, ts}` records. Pure observer: strict
   pass-through, never alters return values, never swallows host errors. Knows nothing
   about the DOM.
2. **`InspectorState`** — in-memory ring buffer of recent messages plus derived `components`
   set and the current page's resolved path rule / presentation / platform / arch. No DOM.
   Plain data, unit-testable. Emits a `"change"` event.
3. **`InspectorPanel`** — the UI. A Shadow-DOM overlay (style-isolated from the Rails app)
   that renders `InspectorState`. Subscribes; never mutates state. Knows nothing about
   Tauri.

**Boundary rule:** `BridgeTap` and `InspectorPanel` meet only through `InspectorState`'s
event emitter. The tap is testable headless; the UI is swappable.

**Dependency direction:** `turbo-desktop.js` lazily loads `inspector.js` only when enabled,
then wires `BridgeTap` into the existing chokepoints. The inspector is a leaf module; core
never imports it.

## Panels & Data Flow

Dockable overlay (bottom or right edge, draggable) with four tabs.

### 1. Components (the discoverability win)

Two lists:

- **Active on this page** — components observed connecting/sending, derived from `BridgeTap`
  traffic plus Stimulus connects. Each row: name, message count, last event.
- **Available** — the full built-in catalog (notification, menu-item, file-picker, badge,
  shortcut, shell, fs, sudo, tray, deep-link, updater). Greyed when unused on the current
  page. Clicking a row reveals a copy-pasteable ERB + Stimulus snippet for that component.

### 2. Messages (debugging / confidence)

Live web↔Rust log from `BridgeTap`. Each entry: direction arrow (↑ web→native /
↓ native→web), component, event, expandable JSON `data`, timestamp. Filter by component.
Clear button.

### 3. Navigation

For the current URL: the matched path-config rule (pattern + resolved `presentation`) and a
list of all rules with the active one highlighted. **v1** shows the resolved presentation
from the proposal response. Showing the exact matched rule index requires a small Rust
return-value addition — flagged as an enhancement, not blocking for v1.

### 4. Shell info

Static facts: `turbo_desktop_platform`, `arch`, app version, updater status, server URL.
Sourced from `getWindowInfo` + config.

### Data flow

```
sendBridgeMessage ──┐
                    ├─▶ BridgeTap ──▶ InspectorState (ring buffer + derived sets)
bridge-response  ───┘                      │  emits "change"
                                           ▼
                                   InspectorPanel re-renders (Shadow DOM)
```

One-way: traffic → state → UI. The hotkey toggles panel **visibility only**; state keeps
collecting while hidden, so the log is already populated when the panel opens.

### Snippet source

A static `catalog.js` map: `component → { description, erb, stimulus }`. Single source of
truth. The README component table can later be generated from it to eliminate doc drift
(fast-follow, not v1).

## Enablement

Off by default, zero production cost.

- **Gate:** `inspector.js` loads only when enabled. Enabled if **any** of: `turbo-desktop.toml`
  `[inspector] enabled = true`; a dev build (`tauri dev`); or
  `localStorage["td:inspector"] = "1"` (flip on against any build without a rebuild).
- **Lazy load:** core performs a dynamic `import("./inspector.js")` only when the gate
  passes, so the production bundle is unaffected and no overlay code ships to end users.
- **Rails side:** the gem exposes `turbo_desktop_inspector?` and auto-injects the enable flag
  in the `development` environment only — a Rails developer gets it locally for free and
  never in production.

## Error Handling

The inspector must never break the host app.

- `BridgeTap` is strict pass-through: it wraps the original `sendBridgeMessage`, calls it
  inside `try`, and **always** returns/throws exactly what the original did. Its own
  recording runs in a separate `try/catch` that swallows only the tap's own errors, so a
  logging bug cannot affect a real bridge call. The original is called exactly once.
- Panel render failures are caught per-frame; a bad record renders as `<unrenderable>`
  rather than crashing the overlay.
- If `__TAURI_INTERNALS__` is absent (plain web browser), the inspector no-ops entirely —
  safe to leave enabled.
- Shadow DOM plus a namespaced high `z-index` prevents the app's CSS/JS from colliding with
  the overlay and vice versa.

## Testing

- **`BridgeTap`** — headless unit tests: feed a fake `sendBridgeMessage` and fake events,
  assert emitted records and pass-through fidelity (return value preserved, throw preserved,
  original called exactly once).
- **`InspectorState`** — pure data tests: ring-buffer eviction, component-set derivation,
  dedup.
- **`catalog.js`** — test that every built-in component has a non-empty
  `{ description, erb, stimulus }` (guards doc drift).
- **`InspectorPanel`** — light DOM test (jsdom): renders tabs, filter narrows the list,
  toggle hides/shows.
- **Manual smoke** in the example app: fire a notification and confirm it appears in
  Messages; open a modal route and confirm Navigation shows the rule.

## Out of Scope (YAGNI for v1)

- Editing or replaying messages
- Persisting logs to disk
- Remote inspection
- Time-travel / history scrubbing
- Exact matched-rule index in Navigation (needs Rust change — enhancement)
- Catalog auto-generating the README (fast-follow)

## Success Criteria

- A Rails developer running the example app under `tauri dev` can open the overlay, see the
  full list of available bridge components, and copy a working snippet for one they were not
  previously aware of.
- Firing any bridge action shows a corresponding entry in the Messages log.
- The inspector ships no code to a production build and no-ops in a plain browser.
- Host app behavior is byte-for-byte unchanged whether the inspector is enabled or not.
