import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "sonner";
import { useDialogStore } from "@/stores";
import { useCCChanStore } from "@/stores/useCCChanStore";
import { isCCPanesPetInstallLink, previewAndInstallCCChanPetUrl } from "./installPet";

export async function handleCCPanesDeepLinks(urls: string[]) {
  const petInstallUrl = urls.find(isCCPanesPetInstallLink);
  if (!petInstallUrl) return;
  const window = getCurrentWindow();
  await window.show().catch(() => {});
  await window.setFocus().catch(() => {});
  useDialogStore.getState().openSettings("ccchan");
  try {
    await previewAndInstallCCChanPetUrl(petInstallUrl, useCCChanStore.getState().load);
  } catch (error) {
    toast.error(`安装失败: ${error instanceof Error ? error.message : String(error)}`);
  }
}
