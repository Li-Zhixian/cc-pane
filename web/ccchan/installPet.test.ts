import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { confirm } from "@tauri-apps/plugin-dialog";
import { isCCPanesPetInstallLink, previewAndInstallCCChanPetUrl } from "./installPet";

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
  },
}));

const TEST_IMAGE_URL_ENCODED = "https%3A%2F%2Fpet.test.invalid%2Fdoro.webp";
const TEST_CAT_URL_ENCODED = "https%3A%2F%2Fpet.test.invalid%2Fcat.webp";
const TEST_ZIP_URL = ["https:", "//", "pet.test.invalid", "/pet.zip"].join("");

function installLink(protocol: "ccpanes" | "codex", suffix = "?name=Doro") {
  return [protocol, ":", "//", "pets", "/install", suffix].join("");
}

const petPreview = {
  stagingId: "stage-1",
  sourcePath: installLink("ccpanes"),
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
      installLink("ccpanes", `?name=Doro&imageUrl=${TEST_IMAGE_URL_ENCODED}`),
    )).toBe(true);
    expect(isCCPanesPetInstallLink(
      installLink("ccpanes", `/?name=猫&image_url=${TEST_CAT_URL_ENCODED}`),
    )).toBe(true);
    expect(isCCPanesPetInstallLink(["ccpanes", ":", "//", "pets", "/other?name=Doro"].join(""))).toBe(false);
    expect(isCCPanesPetInstallLink(installLink("codex"))).toBe(false);
    expect(isCCPanesPetInstallLink(TEST_ZIP_URL)).toBe(false);
    expect(isCCPanesPetInstallLink("not a url")).toBe(false);
  });
});

describe("previewAndInstallCCChanPetUrl", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "preview_ccchan_pet_from_url") return Promise.resolve(petPreview);
      if (cmd === "install_ccchan_pet_from_preview") return Promise.resolve(undefined);
      if (cmd === "cancel_ccchan_pet_preview") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    vi.mocked(confirm).mockResolvedValue(true);
  });

  it("installs the staged pet after plugin dialog confirmation", async () => {
    const load = vi.fn(() => Promise.resolve());
    const link = installLink("ccpanes");

    await expect(previewAndInstallCCChanPetUrl(link, load)).resolves.toBe(true);

    expect(invoke).toHaveBeenCalledWith("preview_ccchan_pet_from_url", {
      url: link,
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

  it("does not expose the preview source path in confirmation copy", async () => {
    const load = vi.fn(() => Promise.resolve());
    const sourcePath = "redacted-source-path";
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "preview_ccchan_pet_from_url") {
        return Promise.resolve({
          ...petPreview,
          sourcePath,
        });
      }
      if (cmd === "install_ccchan_pet_from_preview") return Promise.resolve(undefined);
      if (cmd === "cancel_ccchan_pet_preview") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });

    await expect(previewAndInstallCCChanPetUrl(installLink("ccpanes"), load)).resolves.toBe(true);

    const [message] = vi.mocked(confirm).mock.calls[0] ?? [];
    expect(message).toBe("安装桌宠 \"Doro\" (doro)？");
    expect(message).not.toContain(sourcePath);
  });

  it("cancels the staged preview when the user cancels", async () => {
    vi.mocked(confirm).mockResolvedValue(false);
    const load = vi.fn(() => Promise.resolve());
    const link = installLink("ccpanes");

    await expect(previewAndInstallCCChanPetUrl(link, load)).resolves.toBe(false);

    expect(invoke).toHaveBeenCalledWith("preview_ccchan_pet_from_url", {
      url: link,
    });
    expect(invoke).not.toHaveBeenCalledWith("install_ccchan_pet_from_preview", expect.anything());
    expect(invoke).toHaveBeenCalledWith("cancel_ccchan_pet_preview", { stagingId: "stage-1" });
    expect(load).not.toHaveBeenCalled();
  });
});
