/**
 * Static catalog of built-in bridge components.
 * Single source of truth for the Inspector's "Available" list and snippets.
 * Each entry: { description, erb, stimulus }.
 */
export const CATALOG = {
  "notification": {
    description: "Show native OS notifications.",
    erb: `<button data-controller="notification"\n        data-action="click->notification#notify"\n        data-body="Saved!">Notify</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "notification") {\n  notify(e) { this.sendBridge("connect", { title: "My App", body: e.target.dataset.body }) }\n}`,
  },
  "menu-item": {
    description: "Register an item in the native menu bar.",
    erb: `<%= tag.button "Export PDF",\n      **turbo_desktop_bridge("menu-item", title: "Export PDF", shortcut: "Cmd+E") %>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "menu-item") {\n  connect() { super.connect(); this.sendBridge("register", { title: "Export PDF", shortcut: "Cmd+E" }) }\n}`,
  },
  "file-picker": {
    description: "Open a native file open/save dialog.",
    erb: `<button data-controller="file-picker"\n        data-action="click->file-picker#open">Choose file…</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "file-picker") {\n  open() { this.sendBridge("open", { multiple: false }) }\n  receiveBridge(msg) { console.log("picked", msg.data) }\n}`,
  },
  "badge": {
    description: "Set the dock / taskbar badge count.",
    erb: `<span data-controller="badge" data-badge-count-value="3"></span>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "badge") {\n  static values = { count: Number }\n  connect() { super.connect(); this.sendBridge("set", { count: this.countValue }) }\n}`,
  },
  "shortcut": {
    description: "Register a global keyboard shortcut.",
    erb: `<div data-controller="shortcut" data-shortcut-keys-value="CmdOrCtrl+K"></div>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "shortcut") {\n  static values = { keys: String }\n  connect() { super.connect(); this.sendBridge("register", { keys: this.keysValue }) }\n  receiveBridge() { /* fired when the shortcut is pressed */ }\n}`,
  },
  "shell": {
    description: "Spawn and manage native shell processes.",
    erb: `<button data-controller="shell" data-action="click->shell#run">Run</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "shell") {\n  run() { TurboDesktop.shell.spawn("job-1", "echo", ["hello"]) }\n}`,
  },
  "filesystem": {
    description: "Read and write files through the native filesystem bridge.",
    erb: `<button data-controller="fs" data-action="click->fs#read">Read file</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "filesystem") {\n  read() { this.sendBridge("read", { path: "~/notes.txt" }) }\n  receiveBridge(msg) { console.log(msg.data) }\n}`,
  },
  "sudo": {
    description: "Run a privileged command via the native elevation prompt.",
    erb: `<button data-controller="sudo" data-action="click->sudo#elevate">Install</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "sudo") {\n  elevate() { this.sendBridge("execute", { command: "brew install foo" }) }\n}`,
  },
  "tray": {
    description: "Add items to the system tray / menu-bar icon.",
    erb: `<div data-controller="tray" data-tray-title-value="My App"></div>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "tray") {\n  static values = { title: String }\n  connect() { super.connect(); this.sendBridge("set", { tooltip: this.titleValue }) }\n}`,
  },
  "deep-link": {
    description: "Handle custom-scheme deep links opened from outside the app.",
    erb: `<div data-controller="deep-link"></div>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "deep-link") {\n  receiveBridge(msg) { Turbo.visit(msg.data.path) }\n}`,
  },
  "updater": {
    description: "Check for and apply native app updates.",
    erb: `<button data-controller="updater" data-action="click->updater#check">Check for updates</button>`,
    stimulus: `import { Controller } from "@hotwired/stimulus"\nexport default class extends TurboDesktop.stimulusBridge(Controller, "updater") {\n  check() { this.sendBridge("check", {}) }\n  receiveBridge(msg) { console.log("update status", msg.data) }\n}`,
  },
};

for (const entry of Object.values(CATALOG)) Object.freeze(entry);
Object.freeze(CATALOG);

/** Names of every catalogued component. */
export function listComponents() {
  return Object.keys(CATALOG);
}

/** Look up one component's metadata, or null if unknown. */
export function getComponent(name) {
  return Object.prototype.hasOwnProperty.call(CATALOG, name) ? CATALOG[name] : null;
}
