import { getCurrentWindow } from "@tauri-apps/api/window";
import { useDialogStore } from "@/stores/useDialogStore";

export async function openCCChanSettingsInMainWindow() {
  const window = getCurrentWindow();
  await window.show().catch(() => {});
  await window.unminimize().catch(() => {});
  await window.setFocus().catch(() => {});
  useDialogStore.getState().openSettings("ccchan");
}
