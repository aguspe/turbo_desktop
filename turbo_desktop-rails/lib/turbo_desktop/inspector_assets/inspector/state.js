/**
 * In-memory model for the Dev Inspector.
 * Holds a bounded ring buffer of message records plus a derived component
 * summary and nav/shell facts. Emits "change" to subscribers. No DOM.
 */
export class InspectorState {
  constructor({ capacity = 200 } = {}) {
    this.capacity = capacity;
    this.messages = [];
    this._components = new Map(); // name -> { count, lastEvent }
    this.nav = { url: null, presentation: null };
    this.shell = { platform: null, arch: null, version: null, serverUrl: null, updater: null };
    this._listeners = new Set();
  }

  /** Append a record { direction, component, event, data, ts }. */
  record(record) {
    this.messages.push(record);
    if (this.messages.length > this.capacity) this.messages.shift();

    const prev = this._components.get(record.component) || { count: 0, lastEvent: null };
    prev.count += 1;
    prev.lastEvent = record.event;
    this._components.set(record.component, prev);

    this._emit();
  }

  /** Derived component summary as an array. */
  components() {
    return [...this._components.entries()].map(([name, v]) => ({ name, count: v.count, lastEvent: v.lastEvent }));
  }

  setNav(nav) {
    this.nav = { ...this.nav, ...nav };
    this._emit();
  }

  setShell(shell) {
    this.shell = { ...this.shell, ...shell };
    this._emit();
  }

  clear() {
    this.messages = [];
    this._components.clear();
    this._emit();
  }

  /** Subscribe to changes; returns an unsubscribe function. */
  subscribe(fn) {
    this._listeners.add(fn);
    return () => this._listeners.delete(fn);
  }

  _emit() {
    for (const fn of this._listeners) {
      try { fn(this); } catch (_e) { /* a bad subscriber must not break others */ }
    }
  }
}
