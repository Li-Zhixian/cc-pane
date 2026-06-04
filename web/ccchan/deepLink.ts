import { toast } from "sonner";
import { useCCChanStore } from "@/stores/useCCChanStore";
import { isCCPanesPetInstallLink, previewAndInstallCCChanPetUrl } from "./installPet";
import { openCCChanSettingsInMainWindow } from "./openSettings";

export async function handleCCPanesDeepLinks(urls: string[]) {
  const petInstallUrl = urls.find(isCCPanesPetInstallLink);
  if (!petInstallUrl) return;
  await openCCChanSettingsInMainWindow();
  try {
    await previewAndInstallCCChanPetUrl(petInstallUrl, useCCChanStore.getState().load);
  } catch (error) {
    toast.error(`安装失败: ${error instanceof Error ? error.message : String(error)}`);
  }
}
