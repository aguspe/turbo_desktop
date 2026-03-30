/**
 * @turbo-desktop/bridge
 *
 * Typed ESM exports for the Turbo Desktop JavaScript bridge.
 *
 * The turbo-desktop.js IIFE is automatically injected by the Tauri shell
 * into every page via `on_page_load`. This package provides typed module
 * imports that reference the same globals — no bundling required.
 *
 * Usage:
 *   import { TurboDesktop, BridgeComponent, stimulusBridge } from "@turbo-desktop/bridge"
 */

/** The main Turbo Desktop API. */
export const TurboDesktop = globalThis.TurboDesktop;

/** The BridgeComponent base class for native communication. */
export const BridgeComponent = globalThis.TurboDesktop?.BridgeComponent;

/** Factory to create Stimulus-compatible bridge controller mixins. */
export const stimulusBridge = globalThis.TurboDesktop?.stimulusBridge;

/**
 * Check if the current environment is a Turbo Desktop shell.
 * Returns false when running in a regular browser.
 */
export function isTurboDesktop() {
  return globalThis.__TURBO_DESKTOP__?.isNative === true;
}
