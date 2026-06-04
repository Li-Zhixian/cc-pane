import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import type { CCChanPetInstallPreview } from "./types";

export async function previewAndInstallCCChanPetUrl(url: string, load: () => Promise<void>) {
  const preview = await invoke<CCChanPetInstallPreview>("preview_ccchan_pet_from_url", { url });
  const confirmed = window.confirm(`安装桌宠 "${preview.pet.displayName}" (${preview.pet.id})？`);
  if (!confirmed) return false;
  await invoke("install_ccchan_pet_from_preview", { stagingId: preview.stagingId });
  await load();
  toast.success("桌宠已安装");
  return true;
}

export function isCCPanesPetInstallLink(url: string): boolean {
  try {
    const parsed = new URL(url);
    return (
      parsed.protocol === "ccpanes:" &&
      parsed.hostname === "pets" &&
      parsed.pathname.replace(/\/+$/, "") === "/install"
    );
  } catch {
    return false;
  }
}
