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
    version: "0.1.1",
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
     * The label of the window this page is in, or null outside the shell.
     */
    get windowLabel() {
      return window.__TURBO_DESKTOP_WINDOW_LABEL__ || null;
    },

    /**
     * True when this page is in a modal window rather than the main one.
     */
    get isModal() {
      return String(this.windowLabel || "").startsWith("modal-");
    },

    /**
     * Close a modal window. Defaults to the window this page is in, so a page
     * can dismiss itself without being told which window it was opened in.
     */
    async closeModal(label = undefined) {
      if (!INVOKE) return;

      const target = label || TurboDesktop.windowLabel;
      if (!target) {
        console.warn("[turbo-desktop] No window label to close");
        return;
      }

      try {
        await INVOKE("close_modal", { label: target });
      } catch (e) {
        console.error("[turbo-desktop] Close modal failed:", e);
      }
    },

    /**
     * Close this modal and go back on the screen underneath, as if it had
     * never been opened. Named after Hotwire Native's dismissal semantics.
     */
    async recede() {
      return TurboDesktop.dismiss("recede");
    },

    /**
     * Close this modal and reload the screen underneath — what you usually
     * want after a form submits.
     */
    async refresh() {
      return TurboDesktop.dismiss("refresh");
    },

    /** Close this modal and leave the screen underneath as it was. */
    async resume() {
      return TurboDesktop.dismiss("resume");
    },

    async dismiss(then = "resume", label = undefined) {
      if (!INVOKE) return;

      try {
        await INVOKE("dismiss_modal", { label: label || null, then });
      } catch (e) {
        console.error("[turbo-desktop] Dismiss failed:", e);
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

    // ─── Drag & Drop API ─────────────────────────────────────────────────────
    //
    // Files dragged from the Finder/Explorer arrive here with their real
    // paths (the shell also grants them for reading, like a dialog pick).
    // Also dispatched as DOM events for Stimulus actions:
    //   turbo-desktop:drag-enter, turbo-desktop:drop, turbo-desktop:drag-leave
    // with { paths, position } in event.detail.

    dragDrop: {
      onDrop(callback) {
        return this._listen("drop", callback);
      },

      onEnter(callback) {
        return this._listen("enter", callback);
      },

      onLeave(callback) {
        return this._listen("leave", callback);
      },

      _listen(name, callback) {
        if (!window.__TAURI_INTERNALS__?.event?.listen) return null;
        return window.__TAURI_INTERNALS__.event.listen(
          "bridge-response",
          (event) => {
            const payload = event.payload;
            if (
              payload &&
              payload.component === "drag-drop" &&
              payload.event === name
            ) {
              callback(payload.data);
            }
          }
        );
      },
    },

    // ─── Clipboard API ───────────────────────────────────────────────────────
    //
    // The system clipboard, beyond what the webview can do itself: read what
    // another application put there, write without a user gesture.

    clipboard: {
      async readText() {
        const result = await TurboDesktop.sendBridgeMessage(
          "clipboard",
          "read-text",
          {}
        );
        return result ? result.text : null;
      },

      async writeText(text) {
        return TurboDesktop.sendBridgeMessage("clipboard", "write-text", {
          text,
        });
      },
    },

    // ─── Autostart API ───────────────────────────────────────────────────────
    //
    // Launch-at-login, meant to be driven by a toggle in the app's own
    // settings page rather than turned on silently.

    autostart: {
      async enable() {
        return TurboDesktop.sendBridgeMessage("autostart", "enable", {});
      },

      async disable() {
        return TurboDesktop.sendBridgeMessage("autostart", "disable", {});
      },

      async isEnabled() {
        const result = await TurboDesktop.sendBridgeMessage(
          "autostart",
          "status",
          {}
        );
        return Boolean(result && result.enabled);
      },
    },
  };

  // Surface drag-drop as DOM events so a Stimulus controller can subscribe
  // with a plain action instead of the TurboDesktop API.
  if (window.__TAURI_INTERNALS__?.event?.listen) {
    const domEventNames = {
      enter: "turbo-desktop:drag-enter",
      drop: "turbo-desktop:drop",
      leave: "turbo-desktop:drag-leave",
    };
    window.__TAURI_INTERNALS__.event.listen("bridge-response", (event) => {
      const payload = event.payload;
      const name = payload && payload.component === "drag-drop"
        ? domEventNames[payload.event]
        : null;
      if (name) {
        document.dispatchEvent(new CustomEvent(name, { detail: payload.data }));
      }
    });
  }

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

  // ─── Connection & Visit Errors ─────────────────────────────────────────────

  /**
   * Error names, matching Hotwire Native's TurboError / VisitError so the same
   * words mean the same thing on mobile and desktop.
   */
  TurboDesktop.errors = {
    NETWORK_FAILURE: "network_failure",
    TIMEOUT_FAILURE: "timeout_failure",
    HTTP_FAILURE: "http_failure",
    PAGE_LOAD_FAILURE: "page_load_failure",
  };

  const OVERLAY_ID = "turbo-desktop-offline-overlay";

  /**
   * Whether the shell presents failures itself.
   *
   * Opt out to present your own, the same way Hotwire Native lets you override
   * visitableDidFailRequest:
   *
   *   <meta name="turbo-desktop-error-handling" content="manual">
   *
   * Then listen for the events below and render whatever you like.
   */
  function shellPresentsErrors() {
    const meta = document.querySelector('meta[name="turbo-desktop-error-handling"]');
    return !meta || meta.content !== "manual";
  }

  /**
   * Announce a failed visit. Cancelable: preventDefault() suppresses the shell's
   * own banner for this one event, whatever the meta tag says.
   *
   * Listeners receive { error, status, retry }, where retry() attempts the visit
   * again — the desktop equivalent of Hotwire Native's retryHandler.
   */
  function reportVisitError(error, { status = null, retry = null } = {}) {
    const event = new CustomEvent("turbo-desktop:visit-error", {
      detail: { error, status, retry: retry || (() => window.location.reload()) },
      cancelable: true,
    });

    const notPrevented = document.dispatchEvent(event);
    console.warn("[turbo-desktop] Visit error:", error, status ?? "");

    if (notPrevented && shellPresentsErrors()) showOfflineBanner();
  }

  function reportConnection(online, error) {
    document.dispatchEvent(
      new CustomEvent("turbo-desktop:connection", { detail: { online, error } })
    );

    if (online) {
      hideOfflineBanner();
    } else if (shellPresentsErrors()) {
      showOfflineBanner();
    }
  }

  function showOfflineBanner() {
    if (!document.body || document.getElementById(OVERLAY_ID)) return;

    const overlay = document.createElement("div");
    overlay.id = OVERLAY_ID;
    overlay.setAttribute("role", "status");
    overlay.style.cssText =
      "position:fixed;bottom:0;left:0;right:0;padding:12px 20px;background:#1a1a2e;" +
      "color:#e0e0e0;font-family:system-ui,sans-serif;font-size:14px;text-align:center;" +
      "z-index:99999;border-top:2px solid #e73c7e;";
    overlay.textContent = "Can't reach the server — retrying…";
    document.body.appendChild(overlay);
  }

  function hideOfflineBanner() {
    const overlay = document.getElementById(OVERLAY_ID);
    if (overlay) overlay.remove();
  }

  TurboDesktop.reportVisitError = reportVisitError;

  /**
   * The shell watches the server and tells us when it goes away or comes back.
   *
   * The browser's own offline event only fires when this machine loses its
   * network, which is not the case that usually happens — the server going down
   * while the network is fine looks entirely healthy from in here.
   */
  /**
   * Entry point the shell calls into. Not part of the public API.
   *
   * The shell reaches the page this way rather than through Tauri's event API,
   * which would need the whole JS API exposed on window for any loaded page to
   * reach.
   */
  TurboDesktop.__receive = function (kind, payload) {
    const detail = payload || {};

    switch (kind) {
      case "connection":
        reportConnection(Boolean(detail.online), detail.error || null);
        break;
      case "navigate":
        performNavigation(detail.action);
        break;
      case "focus":
        handleFocusReturn(detail);
        break;
      case "visit":
        performVisit(detail.url);
        break;
      case "file-open-pending":
        drainOpenedFiles();
        break;
      default:
        console.debug("[turbo-desktop] Ignoring unknown message:", kind);
    }
  };

  /**
   * Collect files the OS asked the app to open (double-click on an associated
   * type, "Open With…"), announced as a turbo-desktop:file-open DOM event.
   *
   * Pull rather than push: launching by double-click queues the file in the
   * shell before any page exists, so the page asks — on its own startup, and
   * again whenever the shell pings a running page.
   */
  async function drainOpenedFiles() {
    try {
      const result = await TurboDesktop.sendBridgeMessage(
        "file-open",
        "pending",
        {}
      );
      const paths = result && result.paths;
      if (Array.isArray(paths) && paths.length > 0) {
        document.dispatchEvent(
          new CustomEvent("turbo-desktop:file-open", { detail: { paths } })
        );
      }
    } catch (_e) {
      // Not running inside the shell, or the bridge is not ready.
    }
  }

  drainOpenedFiles();

  /**
   * True when someone is part-way through entering something.
   *
   * A refresh would throw it away, which is a far worse outcome than showing
   * data a few seconds stale, so it is the one case the shell's proposal is
   * declined without being asked.
   */
  function isEditing() {
    const active = document.activeElement;
    if (!active) return false;

    const tag = active.tagName;
    return (
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      active.isContentEditable === true
    );
  }

  /**
   * The window came back after being away.
   *
   * Announced as a cancelable event whether or not a refresh is proposed, so an
   * app can revalidate its own way — or veto the refresh, which is worth doing
   * if it knows about unsaved state the focus check cannot see.
   */
  function handleFocusReturn(detail) {
    const event = new CustomEvent("turbo-desktop:focus", {
      detail: {
        awaySeconds: detail.awaySeconds || 0,
        refreshing: Boolean(detail.refreshing),
      },
      cancelable: true,
    });

    const notPrevented = document.dispatchEvent(event);
    if (!detail.refreshing || !notPrevented) return;

    if (isEditing()) {
      console.debug("[turbo-desktop] Not refreshing on focus while editing");
      return;
    }

    performNavigation("refresh");
  }

  /**
   * Go to a URL the shell asked for — a deep link, usually.
   *
   * Through Turbo where it exists, so the visit behaves like any other and the
   * path configuration still decides how the page is presented.
   */
  function performVisit(url) {
    if (!url) return;

    if (window.Turbo && window.Turbo.visit) {
      window.Turbo.visit(url);
    } else {
      window.location.assign(url);
    }
  }

  /**
   * Act on what the shell asked the page underneath to do after a modal closed.
   */
  function performNavigation(action) {
    switch (action) {
      case "back":
        window.history.back();
        break;
      case "forward":
        window.history.forward();
        break;
      case "reload":
        window.location.reload();
        break;
      case "refresh":
        // Turbo's own refresh keeps scroll position and morphs where it can.
        if (window.Turbo && window.Turbo.visit) {
          window.Turbo.visit(window.location.href, { action: "replace" });
        } else {
          window.location.reload();
        }
        break;
      case "none":
        break;
      default:
        console.debug("[turbo-desktop] Ignoring unknown navigation:", action);
    }
  }

  /**
   * Turbo reports its own failures. In a Turbo app most navigation is a fetch
   * rather than a document load, so this fires long before anything reaches the
   * webview's own error page.
   */
  document.addEventListener("turbo:fetch-request-error", (event) => {
    const url = event.detail && event.detail.url;
    reportVisitError(TurboDesktop.errors.NETWORK_FAILURE, {
      retry: () => (url ? window.location.replace(url) : window.location.reload()),
    });
  });

  /** A visit that completed with an error status. */
  document.addEventListener("turbo:before-fetch-response", (event) => {
    const response = event.detail && event.detail.fetchResponse;
    if (!response || response.succeeded || response.statusCode < 500) return;

    reportVisitError(TurboDesktop.errors.HTTP_FAILURE, { status: response.statusCode });
  });

  // This machine losing its network is a different thing, but it looks the same
  // to the person using the app.
  window.addEventListener("offline", () =>
    reportConnection(false, TurboDesktop.errors.NETWORK_FAILURE)
  );
  window.addEventListener("online", () => reportConnection(true, null));

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
    // Resolve the inspector entry URL, in priority order:
    //   1. an explicit override global,
    //   2. the same-origin URL the Rails gem advertises on the meta tag
    //      (turbo_desktop_inspector_meta_tag → data-inspector-url), served by
    //      the gem's engine so this import() is same-origin,
    //   3. a relative fallback for setups that serve ./inspector.js themselves.
    var inspectorMeta = document.querySelector('meta[name="turbo-desktop-inspector"]');
    var inspectorUrl =
      window.__TURBO_DESKTOP_INSPECTOR_URL__ ||
      (inspectorMeta && inspectorMeta.dataset && inspectorMeta.dataset.inspectorUrl) ||
      "./inspector.js";
    import(inspectorUrl)
      .then(function (m) { m.startInspector(TurboDesktop, { doc: document, win: window }); })
      .catch(function (e) { console.error("[turbo-desktop] inspector failed to load", e); });
  }
})();
