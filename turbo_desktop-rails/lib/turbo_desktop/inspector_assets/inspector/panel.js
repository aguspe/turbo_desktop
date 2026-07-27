import { listComponents, getComponent } from "./catalog.js";

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/"/g, "&quot;");
}

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
      const tag = seen ? `<span>×${escapeHtml(seen.count)} (${escapeHtml(seen.lastEvent)})</span>` : `<span class="muted">available</span>`;
      return `<div class="row"><strong>${escapeHtml(name)}</strong> ${tag}<br><span class="muted">${escapeHtml(c.description)}</span></div>`;
    }).join("");
    return `<div data-panel="components">${rows}</div>`;
  }

  _messagesHtml() {
    const f = this.filter.toLowerCase();
    const rows = this.state.messages
      .filter((m) => !f || (m.component || "").toLowerCase().includes(f))
      .map((m) => {
        const arrow = m.direction === "out" ? "↑" : "↓";
        return `<div class="row" data-message-row>${arrow} <strong>${escapeHtml(m.component)}</strong> ${escapeHtml(m.event)} <span class="muted">${escapeHtml(JSON.stringify(m.data))}</span></div>`;
      }).join("");
    return `<div data-panel="messages"><input data-filter placeholder="filter by component" value="${escapeAttr(this.filter)}">${rows}</div>`;
  }

  _navHtml() {
    const n = this.state.nav;
    return `<div data-panel="navigation"><div class="row">URL: ${escapeHtml(n.url || "—")}</div><div class="row">Presentation: <strong>${escapeHtml(n.presentation || "default")}</strong></div></div>`;
  }

  _shellHtml() {
    const s = this.state.shell;
    return `<div data-panel="shell">
      <div class="row">Platform: ${escapeHtml(s.platform || "—")}</div>
      <div class="row">Arch: ${escapeHtml(s.arch || "—")}</div>
      <div class="row">Version: ${escapeHtml(s.version || "—")}</div>
      <div class="row">Server: ${escapeHtml(s.serverUrl || "—")}</div>
      <div class="row">Updater: ${escapeHtml(s.updater || "—")}</div>
    </div>`;
  }
}
