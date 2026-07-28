/**
 * turbo-desktop-bridge — TypeScript definitions
 *
 * Re-exports all types from the turbo-desktop.js bridge.
 */

export {
  TurboDesktopAPI,
  BridgeComponent,
  BridgeMessage,
  BridgeResponse,
  VisitResponse,
  WindowInfo,
} from "../../src/turbo-desktop";

import type { TurboDesktopAPI, BridgeComponent as BridgeComponentClass, BridgeResponse } from "../../src/turbo-desktop";

/** The main Turbo Desktop API (from `window.TurboDesktop`). */
export declare const TurboDesktop: TurboDesktopAPI;

/** The BridgeComponent base class for native communication. */
export declare const BridgeComponent: typeof BridgeComponentClass;

/** Factory to create Stimulus-compatible bridge controller mixins. */
export declare const stimulusBridge: TurboDesktopAPI["stimulusBridge"];

/** Check if the current environment is a Turbo Desktop shell. */
export declare function isTurboDesktop(): boolean;
