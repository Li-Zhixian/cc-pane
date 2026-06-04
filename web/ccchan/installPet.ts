import { invoke } from "@tauri-apps/api/core";
import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import type { CCChanPetInstallPreview } from "./types";

interface CCChanConfirmOptions {
  title?: string;
  kind?: "info" | "warning" | "error";
  okLabel?: string;
  cancelLabel?: string;
}

export async function confirmCCChanAction(
  message: string,
  options: CCChanConfirmOptions = {},
): Promise<boolean> {
  return confirmDialog(message, {
    title: options.title ?? "cc酱",
    kind: options.kind ?? "info",
    okLabel: options.okLabel ?? "确认",
    cancelLabel: options.cancelLabel ?? "取消",
  });
}

export async function previewAndInstallCCChanPetUrl(url: string, load: () => Promise<void>) {
  const preview = await invoke<CCChanPetInstallPreview>("preview_ccchan_pet_from_url", { url });
  const confirmed = await confirmCCChanAction(
    `安装桌宠 "${preview.pet.displayName}" (${preview.pet.id})？`,
    { okLabel: "安装" },
  );
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
