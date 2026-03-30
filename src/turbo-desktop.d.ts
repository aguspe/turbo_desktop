/**
 * Turbo Desktop — TypeScript Definitions
 *
 * Type definitions for the turbo-desktop.js bridge API.
 * The bridge is injected into the WebView by the Tauri shell and
 * exposes the TurboDesktop object on the global window.
 */

/** Response from a visit proposal to the native shell. */
export interface VisitResponse {
  action: string;
  presentation: "default" | "modal" | "new_window" | "replace" | "native" | "none";
}

/** Information about the current native window. */
export interface WindowInfo {
  label: string;
  width: number;
  height: number;
  x: number;
  y: number;
  scaleFactor: number;
  isFullscreen: boolean;
  isMaximized: boolean;
  platform: string;
  arch: string;
}

/** A bridge message passed between web and native. */
export interface BridgeMessage {
  component: string;
  event: string;
  data: Record<string, unknown>;
}

/** A bridge response received from the native shell. */
export interface BridgeResponse {
  component: string;
  event: string;
  data: Record<string, unknown>;
}

/** Information about a tracked child process. */
export interface ProcessInfo {
  id: string;
  command: string;
  args: string[];
  status:
    | { type: "running" }
    | { type: "exited"; code: number | null }
    | { type: "killed" }
    | { type: "failed"; error: string };
  started_at: number;
}

/** Event emitted during shell process streaming. */
export interface ShellOutputEvent {
  event: "stdout" | "stderr" | "exit";
  line?: string;
  code?: number | null;
}

/** Result of a sudo execute call. */
export interface SudoResult {
  status: "ok" | "error" | "cancelled";
  stdout: string;
  stderr: string;
  code: number | null;
}

/** Information about an available update. */
export interface UpdateInfo {
  status: "available" | "up_to_date" | "error";
  version?: string;
  date?: string;
  body?: string;
  current_version?: string;
  error?: string;
}

/** An entry in a directory listing. */
export interface DirectoryEntry {
  name: string;
  is_dir: boolean;
  is_file: boolean;
}

/**
 * BridgeComponent — the desktop equivalent of Strada's BridgeComponent.
 *
 * Extend this class to communicate with native desktop features.
 */
export declare class BridgeComponent {
  /** The native component name (e.g., "notification", "menu-item"). */
  static component: string;

  /** The DOM element this component is attached to. */
  element: HTMLElement;

  constructor(element: HTMLElement);

  /** Called when the component connects. Sets up native event listeners. */
  connect(): void;

  /** Called when the component disconnects. Notifies the native shell. */
  disconnect(): void;

  /** Send a message to the native shell. */
  send(event: string, data?: Record<string, unknown>): Promise<unknown | null>;

  /** Override to handle messages from the native shell. */
  onReceive(message: BridgeResponse): void;
}

/** The main Turbo Desktop API exposed on `window.TurboDesktop`. */
export interface TurboDesktopAPI {
  /** The turbo-desktop.js version. */
  readonly version: string;

  /** The current platform (e.g., "macos"). */
  readonly platform: string;

  /** Always `true` inside a Turbo Desktop shell. */
  readonly isNative: true;

  /**
   * Send a visit proposal to the native shell.
   * The shell consults the path configuration and decides how to present the URL.
   *
   * @param url - The URL to visit.
   * @param action - The Turbo visit action ("advance" or "replace").
   * @returns The shell's presentation decision.
   */
  proposeVisit(url: string, action?: string): Promise<VisitResponse>;

  /**
   * Update the native window title bar.
   */
  setTitle(title: string): Promise<void>;

  /**
   * Send a bridge message to the native shell.
   *
   * @param component - The bridge component name (e.g., "notification").
   * @param event - The event name (e.g., "connect", "show").
   * @param data - Arbitrary data payload.
   * @returns The native response, or null on error.
   */
  sendBridgeMessage(
    component: string,
    event: string,
    data?: Record<string, unknown>
  ): Promise<unknown | null>;

  /**
   * Get information about the current native window.
   */
  getWindowInfo(): Promise<WindowInfo | null>;

  /**
   * Close a modal window by its label.
   */
  closeModal(label: string): Promise<void>;

  /**
   * Toggle the developer tools / bridge inspector.
   */
  toggleDevTools(): void;

  /** Shell execution API for spawning and managing child processes. */
  shell: {
    /**
     * Spawn a new child process with streaming output.
     * Use `onOutput()` to listen for stdout/stderr/exit events.
     */
    spawn(
      id: string,
      command: string,
      args?: string[],
      options?: { env?: Record<string, string>; cwd?: string }
    ): Promise<{ status: string; id: string } | null>;

    /** Kill a running process by ID. */
    kill(id: string): Promise<{ status: string; id: string } | null>;

    /** Get the status of a tracked process. */
    status(id: string): Promise<ProcessInfo | null>;

    /** List all tracked processes. */
    list(): Promise<ProcessInfo[] | null>;

    /** Subscribe to streaming output events for a process. */
    onOutput(id: string, callback: (event: ShellOutputEvent) => void): void;

    /** Unsubscribe from streaming output events for a process. */
    offOutput(id: string): void;
  };

  /** Sudo API for running commands with administrator privileges (macOS). */
  sudo: {
    /**
     * Execute a command with admin privileges and return the full output.
     * Triggers the macOS password dialog.
     */
    execute(command: string): Promise<SudoResult | null>;

    /**
     * Spawn a command with admin privileges and stream output.
     * Use `onOutput()` to listen for stdout/stderr/exit events.
     */
    spawn(id: string, command: string): Promise<{ status: string; id: string } | null>;

    /** Subscribe to streaming output events for a privileged process. */
    onOutput(id: string, callback: (event: ShellOutputEvent) => void): void;

    /** Unsubscribe from streaming output events for a privileged process. */
    offOutput(id: string): void;
  };

  /** Updater API for checking and installing app updates. */
  updater: {
    /** Check if an update is available. */
    check(): Promise<UpdateInfo | null>;

    /** Download and install the available update. May restart the app. */
    downloadAndInstall(): Promise<{ status: string; version?: string; error?: string } | null>;
  };

  /** File system API for reading and writing files. Supports ~/ expansion. */
  fs: {
    /** Read a file's contents as a string. */
    read(
      path: string,
      encoding?: "utf8" | "base64"
    ): Promise<{ status: string; content?: string; error?: string } | null>;

    /** Write content to a file. Use `append: true` to append instead of overwrite. */
    write(
      path: string,
      content: string,
      options?: { append?: boolean }
    ): Promise<{ status: string; error?: string } | null>;

    /** Check if a path exists and whether it's a file or directory. */
    exists(
      path: string
    ): Promise<{
      status: string;
      exists: boolean;
      is_dir: boolean;
      is_file: boolean;
    } | null>;

    /** List the contents of a directory. */
    list(
      path: string
    ): Promise<{
      status: string;
      entries?: DirectoryEntry[];
      error?: string;
    } | null>;

    /** Create a directory and any missing parent directories. */
    mkdir(path: string): Promise<{ status: string; error?: string } | null>;

    /** Remove a file or directory. Use `recursive: true` for non-empty directories. */
    remove(
      path: string,
      options?: { recursive?: boolean }
    ): Promise<{ status: string; error?: string } | null>;
  };

  /** The BridgeComponent base class. */
  BridgeComponent: typeof BridgeComponent;

  /**
   * Create a Stimulus-compatible bridge controller mixin.
   *
   * @example
   * ```js
   * import { Controller } from "@hotwired/stimulus"
   *
   * export default class extends TurboDesktop.stimulusBridge(Controller, "notification") {
   *   connect() {
   *     super.connect()
   *     this.sendBridge("connect", { title: "Hello" })
   *   }
   *   receiveBridge(message) {
   *     console.log("Native says:", message)
   *   }
   * }
   * ```
   */
  stimulusBridge<T extends abstract new (...args: any[]) => any>(
    BaseController: T,
    componentName: string
  ): T & (new (...args: any[]) => {
    /** Send a bridge message to the native shell. */
    sendBridge(event: string, data?: Record<string, unknown>): Promise<unknown | null>;
    /** Override to handle messages from the native shell. */
    receiveBridge(message: BridgeResponse): void;
  });
}

declare global {
  interface Window {
    /** The Turbo Desktop bridge API. Available inside a Turbo Desktop shell. */
    TurboDesktop: TurboDesktopAPI;
    /** Internal reference (same as TurboDesktop). */
    __TURBO_DESKTOP__: TurboDesktopAPI;
  }
}
