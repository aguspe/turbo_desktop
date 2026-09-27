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
  /** True in a development build; false in an app built for release. */
  development: boolean;
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
  element: Element;

  constructor(element: Element);

  /** Called when the component connects. Sets up native event listeners. */
  connect(): void;

  /** Called when the component disconnects. Notifies the native shell. */
  disconnect(): void;

  /** Send a message to the native shell. */
  send(event: string, data?: Record<string, unknown>): Promise<unknown | null>;

  /** Override to handle messages from the native shell. */
  onReceive(message: BridgeResponse): void;
}

/** What a dialog says besides its message. */
export interface DialogOptions {
  title?: string;
  /** The button that goes ahead. "OK" unless said. */
  confirm?: string;
  /** The button that does not. "Cancel" unless said. */
  cancel?: string;
}

/** Why a visit failed, in Hotwire Native's vocabulary. */
export type VisitFailure =
  | "network_failure"
  | "timeout_failure"
  | "http_failure"
  | "page_load_failure";

/** What the screen underneath a modal does when the modal is dismissed. */
export type DismissAction = "recede" | "refresh" | "resume" | "visit";

/** What a drag or a drop carries. */
export interface DragDropPayload {
  /** Absolute paths of the files and folders being dragged. */
  paths: string[];

  /** Where in the window, in physical pixels. */
  position?: { x: number; y: number };
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
   * True once the document has loaded. The API itself is there before the
   * page's own scripts run; `turbo-desktop:ready` is dispatched on `document`
   * when this becomes true.
   */
  readonly ready: boolean;

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

  /** The label of the window this page is in, or null outside the shell. */
  readonly windowLabel: string | null;

  /** True when this page is in a modal window rather than the main one. */
  readonly isModal: boolean;

  /**
   * Close a modal window. Defaults to the window this page is in, so a page
   * can dismiss itself without being told which window it was opened in.
   */
  closeModal(label?: string): Promise<void>;

  /**
   * Close this modal and go back on the screen underneath, as if it had never
   * been opened. Named after Hotwire Native's dismissal semantics.
   */
  recede(): Promise<void>;

  /**
   * Close this modal and reload the screen underneath — what you usually want
   * after a form submits. Goes through Turbo when it is present.
   */
  refresh(): Promise<void>;

  /** Close this modal and leave the screen underneath as it was. */
  resume(): Promise<void>;

  /**
   * Close a modal and say what the screen underneath should do. `recede()`,
   * `refresh()` and `resume()` call this for the window the page is in.
   */
  dismiss(then?: DismissAction, label?: string, url?: string): Promise<void>;

  /**
   * Ask before going ahead, with a dialog of the system's own. The browser's
   * `confirm()` is not shown by a webview in the shell, and answers no.
   * Turbo's `data-turbo-confirm` uses this without being told to.
   */
  confirm(message: string, options?: DialogOptions): Promise<boolean>;

  /** Say something, with a dialog of the system's own. */
  alert(message: string, options?: Omit<DialogOptions, "cancel">): Promise<void>;

  /**
   * Open the webview's developer tools, or close them if they are open.
   * Development builds only: resolves with `{ status: "unavailable" }` in an
   * app built for release.
   */
  toggleDevTools(): Promise<unknown | null>;

  /**
   * Files dragged onto a window from the Finder or Explorer, with their real
   * paths. A drop grants the dropped paths for reading, like a dialog pick.
   * Also dispatched as DOM events: `turbo-desktop:drop`,
   * `turbo-desktop:drag-enter` and `turbo-desktop:drag-leave`.
   */
  dragDrop: {
    /** Files were dropped. Returns a function that stops listening. */
    onDrop(callback: (drop: DragDropPayload) => void): () => boolean;

    /** A drag entered the window. Returns a function that stops listening. */
    onEnter(callback: (drop: DragDropPayload) => void): () => boolean;

    /** A drag left the window without dropping. Returns a function that stops listening. */
    onLeave(callback: (drop: DragDropPayload) => void): () => boolean;
  };

  /**
   * The system clipboard, beyond what the webview can do itself: read what
   * another application put there, write without a user gesture.
   */
  clipboard: {
    /** The clipboard's text, or null when it holds none. */
    readText(): Promise<string | null>;

    /** Set the clipboard's text. */
    writeText(text: string): Promise<unknown | null>;
  };

  /**
   * Launch at login. Meant to be driven by a toggle in the app's own settings
   * page rather than turned on silently.
   */
  autostart: {
    enable(): Promise<unknown | null>;
    disable(): Promise<unknown | null>;

    /** The current state as the operating system has it. */
    isEnabled(): Promise<boolean>;
  };

  /**
   * Report a failed visit the way the shell does: dispatches
   * `turbo-desktop:visit-error`, and shows the shell's banner unless a
   * listener cancels it. For an app that detects a failure of its own.
   */
  reportVisitError(
    error: VisitFailure,
    options?: { status?: number | null; retry?: (() => void) | null }
  ): void;

  /**
   * Why a visit failed, in Hotwire Native's vocabulary. Compare against
   * `event.detail.error` from `turbo-desktop:visit-error`.
   */
  errors: {
    readonly NETWORK_FAILURE: "network_failure";
    readonly TIMEOUT_FAILURE: "timeout_failure";
    readonly HTTP_FAILURE: "http_failure";
    readonly PAGE_LOAD_FAILURE: "page_load_failure";
  };

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
