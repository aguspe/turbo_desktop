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

  // Native → page events. Tauri's own event API is not exposed to pages
  // loaded from a remote URL — which an app page always is — so the shell
  // delivers responses through __receive and this fans them out. Handlers
  // get the same `{ payload }` shape Tauri's `listen` would have given them.
  const bridgeResponseHandlers = new Set();
  function onBridgeResponse(handler) {
    bridgeResponseHandlers.add(handler);
    return () => bridgeResponseHandlers.delete(handler);
  }
  function dispatchBridgeResponse(payload) {
    bridgeResponseHandlers.forEach((handler) => {
      try {
        handler({ payload });
      } catch (e) {
        console.error("[turbo-desktop] A bridge-response handler failed:", e);
      }
    });
  }

  // ─── Core API ──────────────────────────────────────────────────────────────

  // The shell's user agent names the platform it is running on, e.g.
  // "Turbo Desktop/0.2.4 (Windows; x86_64)". This file is the same on every
  // platform, so the answer has to be read rather than written in.
  function detectPlatform() {
    const agent = navigator.userAgent || "";
    const named = agent.match(/Turbo Desktop\/[\w.]+ \((\w+);/);
    if (named) return named[1].toLowerCase();

    if (/Windows/i.test(agent)) return "windows";
    if (/Linux/i.test(agent)) return "linux";
    return "macos";
  }

  const TurboDesktop = {
    version: "0.2.4",
    platform: detectPlatform(),
    isNative: true,

    /** True once the document has loaded. `turbo-desktop:ready` says when. */
    ready: false,

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

    async dismiss(then = "resume", label = undefined, url = undefined) {
      if (!INVOKE) return;

      try {
        const request = { label: label || null, then };
        // Where the window underneath should go, for `then: "visit"`.
        if (url) request.url = url;
        await INVOKE("dismiss_modal", request);
      } catch (e) {
        console.error("[turbo-desktop] Dismiss failed:", e);
      }
    },

    /**
     * Toggle developer tools (dispatches to Rust which can open the inspector).
     */
    /**
     * Ask before going ahead, with a dialog of the system's own. Resolves
     * with true or false. The browser's confirm() is not shown by a webview
     * in the shell, and answers no.
     *
     *   if (await TurboDesktop.confirm("Delete this task?", { confirm: "Delete" })) …
     */
    async confirm(message, options = {}) {
      const answer = await TurboDesktop.sendBridgeMessage("dialog", "confirm", {
        message: String(message),
        ...options,
      });
      // No answer is a no: what is being asked about is usually not undone.
      return Boolean(answer && answer.confirmed);
    },

    /** Say something, with a dialog of the system's own. */
    async alert(message, options = {}) {
      await TurboDesktop.sendBridgeMessage("dialog", "alert", {
        message: String(message),
        ...options,
      });
    },

    /**
     * Open the webview's developer tools, or close them if they are open.
     * Development builds only: resolves with `{ status: "unavailable" }` in
     * an app built for release.
     */
    async toggleDevTools() {
      return TurboDesktop.sendBridgeMessage("devtools", "toggle", {});
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

        // One listener per process. A controller that connects again, as it
        // does when Turbo brings its page back, replaces the one it had.
        this.offOutput(id);
        this._listeners.set(id, onBridgeResponse(handler));
      },

      offOutput(id) {
        const unlisten = this._listeners.get(id);
        if (unlisten) {
          unlisten();
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

        // One listener per process. A controller that connects again, as it
        // does when Turbo brings its page back, replaces the one it had.
        this.offOutput(id);
        this._listeners.set(id, onBridgeResponse(handler));
      },

      offOutput(id) {
        const unlisten = this._listeners.get(id);
        if (unlisten) {
          unlisten();
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
        return onBridgeResponse((event) => {
          const payload = event.payload;
          if (
            payload &&
            payload.component === "drag-drop" &&
            payload.event === name
          ) {
            callback(payload.data);
          }
        });
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
  {
    const domEventNames = {
      enter: "turbo-desktop:drag-enter",
      drop: "turbo-desktop:drop",
      leave: "turbo-desktop:drag-leave",
    };
    onBridgeResponse((event) => {
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
   * Every visit is proposed to the shell, which consults the path
   * configuration and decides how the URL is presented.
   *
   * Turbo reads `defaultPrevented` as soon as the event has been dispatched,
   * and the shell's answer arrives later, over IPC. So the visit is held
   * first and carried on with once the shell has agreed to it. Deciding after
   * the answer, as this used to, was deciding too late: a rule that opened a
   * modal also navigated the main window to the same URL.
   */
  const approvedVisits = new Set();
  let clicked = null;

  // The action a link asked for. `before-visit` only says where, not how.
  document.addEventListener("turbo:click", (event) => {
    const link = event.target.closest ? event.target.closest("[data-turbo-action]") : null;
    clicked = {
      url: event.detail.url,
      action: link ? link.dataset.turboAction : "advance",
    };
  });

  function carryOn(url, action) {
    if (!window.Turbo) {
      window.location.assign(url);
      return;
    }

    approvedVisits.add(url);
    window.Turbo.visit(url, { action });
  }

  document.addEventListener("turbo:before-visit", (event) => {
    const url = event.detail.url;

    // A visit the shell has already agreed to, coming back round.
    if (approvedVisits.delete(url)) {
      if (INVOKE) {
        INVOKE("page_loading", { url }).catch(() => {});
      }
      return;
    }

    const action = clicked && clicked.url === url ? clicked.action : "advance";
    clicked = null;

    event.preventDefault();

    // A modal on its way somewhere has nothing more to show: what it was for
    // is done. Blank until the shell has said whether it is staying.
    const leaving = TurboDesktop.isModal;
    if (leaving) document.documentElement.style.visibility = "hidden";

    TurboDesktop.proposeVisit(url, action).then((response) => {
      // A modal, a new window, a native screen: the shell opens the URL
      // itself, and this window stays where it is.
      const decided = response ? response.action : action;
      if (decided === "none") {
        // The shell opened the page in a window of its own. This one stays.
        if (leaving) document.documentElement.style.visibility = "";
        return;
      }

      // A modal heading for a page that is not a modal's: a saved form being
      // sent back to the list. The modal has done its work. It closes, and
      // the window underneath goes there, so it shows what was saved.
      if (TurboDesktop.isModal) {
        TurboDesktop.dismiss("visit", undefined, new URL(url, window.location.href).href);
        return;
      }

      carryOn(url, decided === "replace" ? "replace" : action);
    });
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
      this._unlisten = onBridgeResponse(this._boundReceive);
    }

    disconnect() {
      if (this._unlisten) {
        this._unlisten();
        this._unlisten = null;
      }
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
    // A class of its own for each component. Naming the component on
    // BridgeComponent itself renamed it for every controller on the page, so
    // two of them both spoke as whichever connected last.
    class Component extends BridgeComponent {
      static component = componentName;
    }

    return class extends BaseController {
      connect() {
        super.connect();
        this._bridge = new Component(this.element);
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

  // ─── Components declared in the markup ─────────────────────────────────────
  //
  // What the Rails helper writes:
  //
  //   <%= tag.button "Export PDF",
  //         **turbo_desktop_bridge("menu-item", title: "Export PDF", shortcut: "CmdOrCtrl+E") %>
  //
  // An element that declares a component gets it without a controller of its
  // own. Choosing the menu item, or pressing the shortcut, presses the element.

  const BRIDGE_ATTRIBUTE = "data-turbo-desktop-bridge";
  const BRIDGE_OPTION = "turboDesktopBridge";
  const boundElements = new WeakSet();
  // What is registered with the shell, and the element that asked for it.
  const declared = { "menu-item": new Map(), shortcut: new Map() };

  function optionsOf(element) {
    const options = {};
    for (const [key, value] of Object.entries(element.dataset)) {
      if (key.startsWith(BRIDGE_OPTION) && key.length > BRIDGE_OPTION.length) {
        const name = key.slice(BRIDGE_OPTION.length);
        options[name.charAt(0).toLowerCase() + name.slice(1)] = value;
      }
    }
    return options;
  }

  const bindings = {
    "menu-item"(element, options) {
      const id = options.id || options.title;
      if (!id) return;

      declared["menu-item"].set(id, element);
      const data = { id, title: options.title || id };
      if (options.shortcut) data.shortcut = options.shortcut;
      TurboDesktop.sendBridgeMessage("menu-item", "connect", data);
    },

    shortcut(element, options) {
      if (!options.accelerator) return;

      const id = options.id || options.accelerator;
      declared.shortcut.set(id, element);
      element.dataset.turboDesktopShortcut = options.accelerator;
      TurboDesktop.sendBridgeMessage("shortcut", "register", {
        id,
        accelerator: options.accelerator,
      });
    },

    notification(element) {
      // Read when it is pressed, not when it is bound: the title and body
      // may have been changed since.
      element.addEventListener("click", () => {
        const { title, body } = optionsOf(element);
        TurboDesktop.sendBridgeMessage("notification", "show", {
          title: title || "",
          body: body || "",
        });
      });
    },

    badge(_element, options) {
      TurboDesktop.sendBridgeMessage("badge", "set", { count: Number(options.count) || 0 });
    },
  };

  function bindDeclaredComponents() {
    // Whatever was declared by a page that has since gone.
    for (const [id, element] of declared["menu-item"]) {
      if (element.isConnected) continue;
      declared["menu-item"].delete(id);
      TurboDesktop.sendBridgeMessage("menu-item", "unregister", { id });
    }
    for (const [id, element] of declared.shortcut) {
      if (element.isConnected) continue;
      declared.shortcut.delete(id);
      TurboDesktop.sendBridgeMessage("shortcut", "unregister", {
        accelerator: element.dataset.turboDesktopShortcut,
      });
    }

    document.querySelectorAll(`[${BRIDGE_ATTRIBUTE}]`).forEach((element) => {
      if (boundElements.has(element)) return;
      boundElements.add(element);

      const bind = bindings[element.getAttribute(BRIDGE_ATTRIBUTE)];
      if (bind) bind(element, optionsOf(element));
    });
  }

  onBridgeResponse((event) => {
    const { component, event: name, data } = event.payload || {};
    const chosen =
      (component === "menu-item" && name === "clicked") ||
      (component === "shortcut" && name === "triggered");
    if (!chosen || !data) return;

    const element = declared[component].get(data.id);
    if (element && element.isConnected) element.click();
  });

  // ─── What Turbo asks with ──────────────────────────────────────────────────
  //
  // data-turbo-confirm asks with the browser's confirm(), which a webview in
  // the shell does not show. Turbo is given the system's dialog instead,
  // unless the app has chosen a way of asking for itself.

  function giveTurboAWayToAsk() {
    const turbo = window.Turbo;
    if (!turbo || turbo.__turboDesktopAsks) return;

    const ask = (message) => TurboDesktop.confirm(message);

    if (turbo.config && turbo.config.forms) {
      if (turbo.config.forms.confirm) return;
      turbo.config.forms.confirm = ask;
    } else if (typeof turbo.setConfirmMethod === "function") {
      turbo.setConfirmMethod(ask);
    } else {
      return;
    }
    turbo.__turboDesktopAsks = true;
  }

  document.addEventListener("turbo:load", giveTurboAWayToAsk);
  document.addEventListener("turbo:before-fetch-request", giveTurboAWayToAsk);
  document.addEventListener("turbo:click", giveTurboAWayToAsk);

  document.addEventListener("turbo:load", bindDeclaredComponents);
  document.addEventListener("turbo:render", bindDeclaredComponents);
  document.addEventListener("turbo:frame-load", bindDeclaredComponents);

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
        performNavigation(detail.action, detail.url);
        break;
      case "focus":
        handleFocusReturn(detail);
        break;
      case "visit":
        performVisit(detail.url);
        break;
      case "deep-link-pending":
        followTheLink();
        break;
      case "file-open-pending":
        drainOpenedFiles();
        break;
      case "bridge-response":
        dispatchBridgeResponse(detail);
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
  // Whether this is a page of the app, rather than the one the shell shows
  // while the server starts. What is collected from the shell is handed over
  // once, so it is for the app's page to collect.
  function onTheAppsOwnPage() {
    const server = window.__TURBO_DESKTOP_SERVER_URL__;
    if (!server) return true;

    try {
      return new URL(server).origin === window.location.origin;
    } catch (_e) {
      return true;
    }
  }

  async function drainOpenedFiles() {
    if (!onTheAppsOwnPage()) return;

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

  // Once the page has loaded, and not before: the file is handed over once,
  // and the page's own scripts have to be listening when it is. They connect
  // when the document is ready, after this script, which was there first.
  function whenThePageHasLoaded(callback) {
    if (document.readyState === "complete") {
      callback();
    } else {
      window.addEventListener("load", callback, { once: true });
    }
  }

  whenThePageHasLoaded(drainOpenedFiles);

  // ─── A link from outside ───────────────────────────────────────────────────
  //
  // The shell keeps the link the app was asked to open, and this collects it.
  // Asked for on startup too, because a link that started the app arrived
  // before there was a page to tell.

  async function followTheLink() {
    if (!onTheAppsOwnPage()) return;

    const pending = await TurboDesktop.sendBridgeMessage("deep-link", "pending", {});
    if (pending && pending.url) performVisit(pending.url);
  }

  whenTheDocumentIsReady(followTheLink);

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
  function performNavigation(action, url) {
    switch (action) {
      case "visit":
        // Replacing, not advancing: the window is being brought up to date,
        // not taken somewhere the back button should undo.
        if (!url) return performNavigation("refresh");
        if (window.Turbo && window.Turbo.visit) {
          window.Turbo.visit(url, { action: "replace" });
        } else {
          window.location.replace(url);
        }
        break;
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
    if (!response) return;

    // The server answered, so it can be reached. Nothing else takes the
    // banner down after a single failed request: the shell reports the
    // connection changing, and here it never changed.
    if (response.succeeded || response.statusCode < 500) {
      hideOfflineBanner();
      return;
    }

    reportVisitError(TurboDesktop.errors.HTTP_FAILURE, { status: response.statusCode });
  });

  // This machine losing its network is a different thing, but it looks the same
  // to the person using the app.
  window.addEventListener("offline", () =>
    reportConnection(false, TurboDesktop.errors.NETWORK_FAILURE)
  );
  window.addEventListener("online", () => reportConnection(true, null));

  // ─── Initial Setup ─────────────────────────────────────────────────────────

  // The shell runs this script before anything the page loads, so that a
  // page's own scripts find TurboDesktop already there. The document has no
  // head, body or title at that point, so what needs them waits for them.
  function whenTheDocumentIsReady(callback) {
    if (document.readyState === "complete" || document.readyState === "interactive") {
      callback();
    } else {
      document.addEventListener("DOMContentLoaded", callback, { once: true });
    }
  }

  // Sync title on initial load (before Turbo is initialized)
  whenTheDocumentIsReady(() => TurboDesktop.setTitle(document.title));

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

  // Decided once the page has a head: the tag that turns the inspector on is
  // in it.
  // The inspector is a module. A script the shell runs before the page
  // cannot import one, in some webviews, wherever the call comes from; the
  // shell does the importing itself once the page has loaded, and hands the
  // module over. Importing from here still serves a shell that injects this
  // script the older way, after the page. Whichever gets there first starts
  // the inspector, once.
  let inspectorStarted = false;

  function inspectorUrl() {
    TurboDesktop._inspectorWanted = inspectorEnabled();
    if (!INVOKE || !TurboDesktop._inspectorWanted || inspectorStarted) return null;

    // In priority order:
    //   1. an explicit override global,
    //   2. the same-origin URL the Rails gem advertises on the meta tag
    //      (turbo_desktop_inspector_meta_tag → data-inspector-url), served by
    //      the gem's engine so the import is same-origin,
    //   3. a relative fallback for setups that serve ./inspector.js themselves.
    var inspectorMeta = document.querySelector('meta[name="turbo-desktop-inspector"]');
    return (
      window.__TURBO_DESKTOP_INSPECTOR_URL__ ||
      (inspectorMeta && inspectorMeta.dataset && inspectorMeta.dataset.inspectorUrl) ||
      "./inspector.js"
    );
  }

  function startTheInspector(module) {
    if (inspectorStarted) return;

    module.startInspector(TurboDesktop, { doc: document, win: window });
    // Only once it has: a start that failed is worth another try.
    inspectorStarted = true;
    TurboDesktop._inspectorError = null;
  }

  function inspectorFailed(error) {
    TurboDesktop._inspectorError = String((error && error.stack) || error);
  }

  function loadTheInspector() {
    const url = inspectorUrl();
    if (!url) return;

    import(url).then(startTheInspector).catch(inspectorFailed);
  }

  TurboDesktop._inspectorUrl = inspectorUrl;
  TurboDesktop._startInspector = startTheInspector;
  TurboDesktop._inspectorFailed = inspectorFailed;
  TurboDesktop._loadInspector = loadTheInspector;

  whenTheDocumentIsReady(() => {
    loadTheInspector();
    giveTurboAWayToAsk();
    bindDeclaredComponents();

    // For a script that would rather be told than check: TurboDesktop is
    // there before it, but the document was not.
    TurboDesktop.ready = true;
    document.dispatchEvent(
      new CustomEvent("turbo-desktop:ready", {
        detail: { version: TurboDesktop.version, platform: TurboDesktop.platform },
      })
    );
  });
})();
