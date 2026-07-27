/**
 * Turbo Desktop — JavaScript Bridge
 *
 * This script is injected into the WebView by the Tauri shell.
 * It hooks into Turbo Drive events and provides the bridge between
 * web components and native desktop features.
 *
 * Architecture (mirrors Hotwire Native mobile):
 * 1. Intercept Turbo navigation → send visit proposals to Rust
 * 2. Sync page title → native window title bar
 * 3. Bridge components → JS ↔ Rust message passing (Strada equivalent)
 */
(function () {
  "use strict";

  // Guard against double-injection
  if (window.__TURBO_DESKTOP__) return;

  const INVOKE = window.__TAURI_INTERNALS__?.invoke;

  // ─── Core API ──────────────────────────────────────────────────────────────

  const TurboDesktop = {
    version: "0.1.0",
    platform: "macos",
    isNative: true,

    /**
     * Send a visit proposal to the native shell.
     * The shell consults the path configuration and decides how to present the URL.
     */
    async proposeVisit(url, action = "advance") {
      if (!INVOKE) return { action, presentation: "default" };

      try {
        const urlObj = new URL(url, window.location.origin);
        return await INVOKE("handle_visit_proposal", {
          proposal: {
            url: urlObj.href,
            path: urlObj.pathname,
            action: action,
          },
        });
      } catch (e) {
        console.error("[turbo-desktop] Visit proposal failed:", e);
        return { action, presentation: "default" };
      }
    },

    /**
     * Update the native window title.
     */
    async setTitle(title) {
      if (!INVOKE) return;
      try {
        await INVOKE("update_window_title", { title });
      } catch (e) {
        console.error("[turbo-desktop] Set title failed:", e);
      }
    },

    /**
     * Send a bridge message to the native shell.
     */
    async sendBridgeMessage(component, event, data = {}) {
      if (!INVOKE) return null;
      try {
        return await INVOKE("handle_bridge_message", {
          message: { component, event, data },
        });
      } catch (e) {
        console.error("[turbo-desktop] Bridge message failed:", e);
        return null;
      }
    },

    /**
     * Get information about the current window.
     */
    async getWindowInfo() {
      if (!INVOKE) return null;
      try {
        return await INVOKE("get_window_info");
      } catch (e) {
        console.error("[turbo-desktop] Window info failed:", e);
        return null;
      }
    },

    /**
     * Close a modal window.
     */
    async closeModal(label) {
      if (!INVOKE) return;
      try {
        await INVOKE("close_modal", { label });
      } catch (e) {
        console.error("[turbo-desktop] Close modal failed:", e);
      }
    },

    /**
     * Toggle developer tools (dispatches to Rust which can open the inspector).
     */
    toggleDevTools() {
      // Tauri 2 doesn't expose devtools toggle from JS directly,
      // but we can emit a custom event for the Rust side to handle
      console.log("[turbo-desktop] DevTools toggle requested");
    },

    // ─── Shell Execution API ─────────────────────────────────────────────────

    shell: {
      _listeners: new Map(),

      async spawn(id, command, args = [], options = {}) {
        return TurboDesktop.sendBridgeMessage("shell", "spawn", {
          id,
          command,
          args,
          env: options.env || {},
          cwd: options.cwd || null,
        });
      },

      async kill(id) {
        return TurboDesktop.sendBridgeMessage("shell", "kill", { id });
      },

      async status(id) {
        return TurboDesktop.sendBridgeMessage("shell", "status", { id });
      },

      async list() {
        return TurboDesktop.sendBridgeMessage("shell", "list", {});
      },

      onOutput(id, callback) {
        const handler = (event) => {
          const payload = event.payload;
          if (
            payload &&
            payload.component === "shell" &&
            payload.data &&
            payload.data.id === id
          ) {
            callback({
              event: payload.event,
              line: payload.data.line,
              code: payload.data.code,
            });
          }
        };

        if (window.__TAURI_INTERNALS__?.event?.listen) {
          const unlisten = window.__TAURI_INTERNALS__.event.listen(
            "bridge-response",
            handler
          );
          this._listeners.set(id, unlisten);
        }
      },

      offOutput(id) {
        const unlisten = this._listeners.get(id);
        if (unlisten) {
          unlisten.then((fn) => fn());
          this._listeners.delete(id);
        }
      },
    },

    // ─── Sudo API ─────────────────────────────────────────────────────────────

    sudo: {
      async execute(command) {
        return TurboDesktop.sendBridgeMessage("sudo", "execute", { command });
      },

      _listeners: new Map(),

      async spawn(id, command) {
        return TurboDesktop.sendBridgeMessage("sudo", "spawn", { id, command });
      },

      onOutput(id, callback) {
        const handler = (event) => {
          const payload = event.payload;
          if (
            payload &&
            payload.component === "sudo" &&
            payload.data &&
            payload.data.id === id
          ) {
            callback({
              event: payload.event,
              line: payload.data.line,
              code: payload.data.code,
            });
          }
        };

        if (window.__TAURI_INTERNALS__?.event?.listen) {
          const unlisten = window.__TAURI_INTERNALS__.event.listen(
            "bridge-response",
            handler
          );
          this._listeners.set(id, unlisten);
        }
      },

      offOutput(id) {
        const unlisten = this._listeners.get(id);
        if (unlisten) {
          unlisten.then((fn) => fn());
          this._listeners.delete(id);
        }
      },
    },

    // ─── Updater API ─────────────────────────────────────────────────────────

    updater: {
      async check() {
        return TurboDesktop.sendBridgeMessage("updater", "check", {});
      },

      async downloadAndInstall() {
        return TurboDesktop.sendBridgeMessage("updater", "download-and-install", {});
      },
    },

    // ─── File System API ─────────────────────────────────────────────────────

    fs: {
      async read(path, encoding = "utf8") {
        return TurboDesktop.sendBridgeMessage("filesystem", "read", {
          path,
          encoding,
        });
      },

      async write(path, content, options = {}) {
        return TurboDesktop.sendBridgeMessage("filesystem", "write", {
          path,
          content,
          append: options.append || false,
        });
      },

      async exists(path) {
        return TurboDesktop.sendBridgeMessage("filesystem", "exists", { path });
      },

      async list(path) {
        return TurboDesktop.sendBridgeMessage("filesystem", "list", { path });
      },

      async mkdir(path) {
        return TurboDesktop.sendBridgeMessage("filesystem", "mkdir", { path });
      },

      async remove(path, options = {}) {
        return TurboDesktop.sendBridgeMessage("filesystem", "remove", {
          path,
          recursive: options.recursive || false,
        });
      },
    },
  };

  // ─── Turbo Drive Integration ───────────────────────────────────────────────

  /**
   * Intercept Turbo Drive's "before-visit" to propose the visit to the native shell.
   * If the shell decides to open a modal or new window, we cancel the Turbo visit.
   */
  document.addEventListener("turbo:before-visit", async (event) => {
    const url = event.detail.url;

    // Notify Rust that a page is loading
    if (INVOKE) {
      INVOKE("page_loading", { url }).catch(() => {});
    }

    // Propose the visit to the native shell
    const response = await TurboDesktop.proposeVisit(url, "advance");

    // If the native shell handled it (modal, new window, native screen),
    // cancel the Turbo visit — the native side opens the URL itself.
    if (response.action === "none") {
      event.preventDefault();
    }
    // If "replace", tell Turbo to replace instead of advance
    else if (response.action === "replace") {
      event.preventDefault();
      window.Turbo?.visit(url, { action: "replace" });
    }
  });

  /**
   * After Turbo loads a page, sync the title and notify Rust.
   */
  document.addEventListener("turbo:load", () => {
    const title = document.title;
    TurboDesktop.setTitle(title);

    if (INVOKE) {
      INVOKE("page_loaded", { url: window.location.href }).catch(() => {});
    }
  });

  /**
   * Handle Turbo frame navigation — these don't trigger turbo:before-visit
   * but we still want to track them.
   */
  document.addEventListener("turbo:frame-load", (event) => {
    // Frame loads don't change the main URL, but we log them
    console.debug("[turbo-desktop] Frame loaded:", event.target.id);
  });

  /**
   * Handle form submissions that Turbo intercepts.
   */
  document.addEventListener("turbo:submit-start", () => {
    // Could show a native loading indicator here
    console.debug("[turbo-desktop] Form submit started");
  });

  // ─── Bridge Component Base Class ───────────────────────────────────────────

  /**
   * BridgeComponent — the desktop equivalent of Strada's BridgeComponent.
   *
   * Extend this class in your Stimulus controllers to communicate with native features.
   *
   * Example:
   *   class NotificationBridge extends TurboDesktop.BridgeComponent {
   *     static component = "notification"
   *     connect() {
   *       super.connect()
   *       this.send("connect", { title: "My App" })
   *     }
   *     onReceive(message) {
   *       if (message.event === "clicked") { ... }
   *     }
   *   }
   */
  class BridgeComponent {
    static component = "unknown";

    constructor(element) {
      this.element = element;
      this._boundReceive = this._handleReceive.bind(this);
    }

    connect() {
      // Listen for responses from the native shell
      if (window.__TAURI_INTERNALS__?.event?.listen) {
        window.__TAURI_INTERNALS__.event.listen(
          "bridge-response",
          this._boundReceive
        );
      }
    }

    disconnect() {
      // Notify native side that this component is going away
      this.send("disconnect", {});
    }

    /**
     * Send a message to the native shell.
     */
    async send(event, data = {}) {
      return TurboDesktop.sendBridgeMessage(
        this.constructor.component,
        event,
        data
      );
    }

    /**
     * Override this to handle messages from the native shell.
     */
    onReceive(_message) {
      // Override in subclass
    }

    _handleReceive(event) {
      const response = event.payload;
      if (response && response.component === this.constructor.component) {
        this.onReceive(response);
      }
    }
  }

  TurboDesktop.BridgeComponent = BridgeComponent;

  // ─── Stimulus Integration Helper ───────────────────────────────────────────

  /**
   * Helper to create a Stimulus-compatible bridge controller.
   * This creates a mixin that can be used with Stimulus controllers.
   *
   * Usage in a Stimulus controller:
   *   import { Controller } from "@hotwired/stimulus"
   *
   *   export default class extends TurboDesktop.stimulusBridge(Controller, "notification") {
   *     connect() {
   *       super.connect()
   *       this.sendBridge("connect", { title: "Hello" })
   *     }
   *     receiveBridge(message) {
   *       console.log("Native says:", message)
   *     }
   *   }
   */
  TurboDesktop.stimulusBridge = function (BaseController, componentName) {
    return class extends BaseController {
      connect() {
        super.connect();
        this._bridge = new BridgeComponent(this.element);
        this._bridge.constructor.component = componentName;
        this._bridge.onReceive = (msg) => this.receiveBridge(msg);
        this._bridge.connect();
      }

      disconnect() {
        super.disconnect();
        if (this._bridge) {
          this._bridge.disconnect();
        }
      }

      sendBridge(event, data = {}) {
        return this._bridge.send(event, data);
      }

      receiveBridge(_message) {
        // Override in subclass
      }
    };
  };

  // ─── Offline Detection ─────────────────────────────────────────────────────

  /**
   * Show a non-intrusive overlay when the connection to the server drops.
   * Dismiss automatically when connectivity is restored.
   */
  window.addEventListener("offline", () => {
    if (document.getElementById("turbo-desktop-offline-overlay")) return;
    const overlay = document.createElement("div");
    overlay.id = "turbo-desktop-offline-overlay";
    overlay.style.cssText =
      "position:fixed;bottom:0;left:0;right:0;padding:12px 20px;background:#1a1a2e;" +
      "color:#e0e0e0;font-family:system-ui,sans-serif;font-size:14px;text-align:center;" +
      "z-index:99999;border-top:2px solid #e73c7e;";
    overlay.textContent = "Connection lost — waiting for server...";
    document.body.appendChild(overlay);
    console.warn("[turbo-desktop] Network offline detected");
  });

  window.addEventListener("online", () => {
    const overlay = document.getElementById("turbo-desktop-offline-overlay");
    if (overlay) overlay.remove();
    console.log("[turbo-desktop] Network back online");
  });

  // ─── Initial Setup ─────────────────────────────────────────────────────────

  // Sync title on initial load (before Turbo is initialized)
  if (document.readyState === "complete" || document.readyState === "interactive") {
    TurboDesktop.setTitle(document.title);
  } else {
    document.addEventListener("DOMContentLoaded", () => {
      TurboDesktop.setTitle(document.title);
    });
  }

  // Expose the API globally
  window.__TURBO_DESKTOP__ = TurboDesktop;
  window.TurboDesktop = TurboDesktop;

  console.log(`[turbo-desktop] v${TurboDesktop.version} initialized`);

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
})();
