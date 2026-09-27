// Compiled, never run: test/cli.test.js checks that the published types accept
// the API the way the documentation shows it being used.
import { TurboDesktop, isTurboDesktop, DismissAction } from "../../packages/bridge/index";
async function demo() {
  if (!isTurboDesktop()) return;
  const label: string | null = TurboDesktop.windowLabel;
  const modal: boolean = TurboDesktop.isModal;
  await TurboDesktop.closeModal();
  await TurboDesktop.recede(); await TurboDesktop.refresh(); await TurboDesktop.resume();
  const how: DismissAction = "refresh"; await TurboDesktop.dismiss(how);
  const text: string | null = await TurboDesktop.clipboard.readText();
  await TurboDesktop.clipboard.writeText("x");
  const on: boolean = await TurboDesktop.autostart.isEnabled();
  TurboDesktop.dragDrop.onDrop((d) => d.paths.map((p) => p.toUpperCase()));
  const e: "network_failure" = TurboDesktop.errors.NETWORK_FAILURE;
  return [label, modal, text, on, e];
}
demo();
