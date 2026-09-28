/**
 * turbo-desktop-bridge
 *
 * Typed ESM exports for the Turbo Desktop JavaScript bridge.
 *
 * The shell injects turbo-desktop.js into every page once the page has
 * loaded. A module that imports this package runs before that, and in a plain
 * browser the shell never arrives. So nothing here reads the shell when it is
 * imported: everything looks it up at the moment it is used, and does nothing
 * when it is not there.
 *
 * Usage:
 *   import { TurboDesktop, BridgeComponent, stimulusBridge } from "turbo-desktop-bridge"
 */

function shell() {
  return globalThis.TurboDesktop;
}

/**
 * The main Turbo Desktop API. Reads through to `window.TurboDesktop`, so a
 * property is `undefined` until the shell is there.
 */
export const TurboDesktop = new Proxy(
  {},
  {
    get(_target, property) {
      const api = shell();
      if (!api) return undefined;

      const value = api[property];
      return typeof value === "function" ? value.bind(api) : value;
    },
    has(_target, property) {
      const api = shell();
      return Boolean(api) && property in api;
    },
  }
);

/**
 * The base class for talking to a native component. Extend it and name the
 * component:
 *
 *   class Badge extends BridgeComponent {
 *     static component = "badge"
 *   }
 */
export class BridgeComponent {
  static component = "unknown";

  constructor(element) {
    this.element = element;
    this._native = null;
  }

  connect() {
    const Native = shell()?.BridgeComponent;
    if (!Native) return;

    const owner = this;
    const component = this.constructor.component;

    // A class of its own, so naming the component names it for this one only.
    class Component extends Native {
      static component = component;

      onReceive(message) {
        owner.onReceive(message);
      }
    }

    this._native = new Component(this.element);
    this._native.connect();
  }

  disconnect() {
    if (this._native) this._native.disconnect();
    this._native = null;
  }

  /** Resolves with the native response, or null outside the shell. */
  async send(event, data = {}) {
    return this._native ? this._native.send(event, data) : null;
  }

  /** Override to handle messages from the native shell. */
  onReceive(_message) {}
}

/**
 * A Stimulus controller that speaks to one native component:
 *
 *   export default class extends stimulusBridge(Controller, "notification") {
 *     notify() { this.sendBridge("show", { title: "Hello" }) }
 *     receiveBridge(message) { ... }
 *   }
 *
 * Outside the shell the controller still connects and `sendBridge` resolves
 * with null, so the same controller serves the browser too.
 */
export function stimulusBridge(BaseController, componentName) {
  class Component extends BridgeComponent {
    static component = componentName;
  }

  return class extends BaseController {
    connect() {
      super.connect();
      this._bridge = new Component(this.element);
      this._bridge.onReceive = (message) => this.receiveBridge(message);
      this._bridge.connect();
    }

    disconnect() {
      super.disconnect();
      if (this._bridge) this._bridge.disconnect();
    }

    async sendBridge(event, data = {}) {
      return this._bridge ? this._bridge.send(event, data) : null;
    }

    receiveBridge(_message) {
      // Override in a subclass.
    }
  };
}

/**
 * Check if the current environment is a Turbo Desktop shell.
 * Returns false when running in a regular browser.
 */
export function isTurboDesktop() {
  return globalThis.__TURBO_DESKTOP__?.isNative === true;
}
