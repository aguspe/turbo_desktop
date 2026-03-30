# @turbo-desktop/bridge

Typed ESM imports for the [Turbo Desktop](https://github.com/aguspe/turbo_desktop) JavaScript bridge API.

## Installation

```bash
npm install @turbo-desktop/bridge
```

## Usage

```javascript
import { TurboDesktop, BridgeComponent, stimulusBridge, isTurboDesktop } from "@turbo-desktop/bridge"

// Check if running inside a Turbo Desktop shell
if (isTurboDesktop()) {
  const info = await TurboDesktop.getWindowInfo()
  console.log(`Running on ${info.platform}`)
}
```

### With Stimulus

```javascript
import { Controller } from "@hotwired/stimulus"
import { stimulusBridge } from "@turbo-desktop/bridge"

export default class extends stimulusBridge(Controller, "notification") {
  connect() {
    super.connect()
    this.sendBridge("connect", { title: "My App" })
  }

  receiveBridge(message) {
    console.log("Native says:", message)
  }
}
```

## How it works

The `turbo-desktop.js` IIFE is automatically injected by the Tauri shell into every page. This package provides typed ESM exports that reference the same `window.TurboDesktop` globals — no bundling or duplication required.

## License

MIT
