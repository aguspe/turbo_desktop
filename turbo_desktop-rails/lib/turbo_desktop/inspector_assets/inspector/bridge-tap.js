/**
 * Observes bridge traffic without altering it.
 *
 * install() wraps host.sendBridgeMessage so every outbound call is recorded
 * then forwarded to the original exactly once, with its return/throw preserved.
 * observeResponse() records inbound bridge-response payloads.
 *
 * Recording runs inside its own try/catch so a logging bug can never affect a
 * real bridge call. No DOM access.
 */
export class BridgeTap {
  constructor(host, { onRecord, now = () => Date.now() } = {}) {
    this.host = host;
    this.onRecord = onRecord;
    this.now = now;
    this._installed = false;
    this._original = null;
  }

  install() {
    if (this._installed) return;
    // Store the raw property so uninstall can restore the exact same reference.
    this._original = this.host.sendBridgeMessage;
    const original = this._original.bind(this.host);
    const self = this;
    this.host.sendBridgeMessage = function (component, event, data = {}) {
      self._safeRecord({ direction: "out", component, event, data, ts: self.now() });
      return original(component, event, data); // pass-through, exactly once
    };
    this._installed = true;
  }

  /** Record an inbound bridge-response payload. */
  observeResponse(payload) {
    if (!payload) return;
    this._safeRecord({
      direction: "in",
      component: payload.component,
      event: payload.event || "response",
      data: payload.data !== undefined ? payload.data : payload,
      ts: this.now(),
    });
  }

  uninstall() {
    if (this._installed && this._original) {
      this.host.sendBridgeMessage = this._original;
      this._installed = false;
    }
  }

  _safeRecord(record) {
    try {
      if (this.onRecord) this.onRecord(record);
    } catch (_e) {
      /* recording must never break the host */
    }
  }
}
