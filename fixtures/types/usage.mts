// Compiled, never run: test/cli.test.js checks that the published types accept
// the API the way the documentation shows it being used, under the module
// resolutions an app is likely to have.
import {
  TurboDesktop,
  BridgeComponent,
  stimulusBridge,
  isTurboDesktop,
} from "../../packages/bridge/index.js";
import type { DismissAction, BridgeResponse } from "../../packages/bridge/index.js";

class Controller {
  element: Element;
  constructor(element: Element) {
    this.element = element;
  }
  connect(): void {}
  disconnect(): void {}
}

class Notify extends stimulusBridge(Controller, "notification") {
  notify(): void {
    this.sendBridge("show", { title: "Hello" });
  }

  receiveBridge(message: BridgeResponse): void {
    console.log(message.event);
  }
}

class Badge extends BridgeComponent {
  static component = "badge";

  onReceive(message: BridgeResponse): void {
    console.log(message.component);
  }
}

async function demo(element: Element): Promise<unknown[]> {
  if (!isTurboDesktop()) return [];

  new Notify(element).connect();
  await new Badge(element).send("set", { count: 3 });

  const label: string | null = TurboDesktop.windowLabel;
  const modal: boolean = TurboDesktop.isModal;
  const platform: string = TurboDesktop.platform;

  await TurboDesktop.closeModal();
  await TurboDesktop.recede();
  await TurboDesktop.refresh();
  await TurboDesktop.resume();
  const how: DismissAction = "refresh";
  await TurboDesktop.dismiss(how);

  const text: string | null = await TurboDesktop.clipboard.readText();
  await TurboDesktop.clipboard.writeText("INV-2024-001");
  const launches: boolean = await TurboDesktop.autostart.isEnabled();

  const stop = TurboDesktop.dragDrop.onDrop((drop) => drop.paths.map((path) => path.toUpperCase()));
  stop();

  const failure: "network_failure" = TurboDesktop.errors.NETWORK_FAILURE;
  TurboDesktop.reportVisitError(failure, { status: 503, retry: () => {} });

  return [label, modal, platform, text, launches];
}

export { demo };
