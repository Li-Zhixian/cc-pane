import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";
import { isCCPanesPetInstallLink, previewAndInstallCCChanPetUrl } from "./installPet";

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
  },
}));

const petPreview = {
  stagingId: "stage-1",
  sourcePath: "ccpanes://pets/install?name=Doro",
  pet: {
    id: "doro",
    displayName: "Doro",
    description: "Doro pet",
    spritesheetUrl: "asset://doro",
    source: "user" as const,
    atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
    animations: { idle: { row: 0, frames: 1, fps: 1 } },
  },
};

describe("isCCPanesPetInstallLink", () => {
  it("accepts CC-Panes pet install links only", () => {
    expect(isCCPanesPetInstallLink(
      "ccpanes://pets/install?name=Doro&imageUrl=https%3A%2F%2Fexample.invalid%2Fdoro.webp",
    )).toBe(true);
    expect(isCCPanesPetInstallLink(
      "ccpanes://pets/install/?name=猫&image_url=https%3A%2F%2Fexample.invalid%2Fcat.webp",
    )).toBe(true);
    expect(isCCPanesPetInstallLink("ccpanes://pets/other?name=Doro")).toBe(false);
    expect(isCCPanesPetInstallLink("codex://pets/install?name=Doro")).toBe(false);
    expect(isCCPanesPetInstallLink("https://example.invalid/pet.zip")).toBe(false);
    expect(isCCPanesPetInstallLink("not a url")).toBe(false);
  });
});

describe("previewAndInstallCCChanPetUrl", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "preview_ccchan_pet_from_url") return Promise.resolve(petPreview);
      if (cmd === "install_ccchan_pet_from_preview") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    vi.mocked(confirm).mockResolvedValue(true);
  });

  it("installs the staged pet after plugin dialog confirmation", async () => {
    const load = vi.fn(() => Promise.resolve());

    await expect(previewAndInstallCCChanPetUrl("ccpanes://pets/install?name=Doro", load)).resolves.toBe(true);

    expect(invoke).toHaveBeenCalledWith("preview_ccchan_pet_from_url", {
      url: "ccpanes://pets/install?name=Doro",
    });
    expect(confirm).toHaveBeenCalledWith(
      "安装桌宠 \"Doro\" (doro)？",
      expect.objectContaining({
        title: "cc酱",
        okLabel: "安装",
        cancelLabel: "取消",
      }),
    );
    expect(invoke).toHaveBeenCalledWith("install_ccchan_pet_from_preview", { stagingId: "stage-1" });
    expect(load).toHaveBeenCalledOnce();
  });

  it("keeps the preview staged when the user cancels", async () => {
    vi.mocked(confirm).mockResolvedValue(false);
    const load = vi.fn(() => Promise.resolve());

    await expect(previewAndInstallCCChanPetUrl("ccpanes://pets/install?name=Doro", load)).resolves.toBe(false);

    expect(invoke).toHaveBeenCalledWith("preview_ccchan_pet_from_url", {
      url: "ccpanes://pets/install?name=Doro",
    });
    expect(invoke).not.toHaveBeenCalledWith("install_ccchan_pet_from_preview", expect.anything());
    expect(load).not.toHaveBeenCalled();
  });
});
