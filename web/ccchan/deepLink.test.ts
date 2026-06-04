import { beforeEach, describe, expect, it, vi } from "vitest";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { handleCCPanesDeepLinks } from "./deepLink";
import { previewAndInstallCCChanPetUrl } from "./installPet";
import { useCCChanStore } from "@/stores/useCCChanStore";
import { useDialogStore } from "@/stores/useDialogStore";

const windowMock = {
  setFocus: vi.fn(() => Promise.resolve()),
  show: vi.fn(() => Promise.resolve()),
};

vi.mock("./installPet", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./installPet")>();
  return {
    ...actual,
    previewAndInstallCCChanPetUrl: vi.fn(() => Promise.resolve(true)),
  };
});

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => windowMock),
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
  },
}));

const TEST_IMAGE_URL_ENCODED = "https%3A%2F%2Fpet.test.invalid%2Fdoro.webp";
const TEST_ZIP_URL = ["https:", "//", "pet.test.invalid", "/pet.zip"].join("");

function installLink(protocol: "ccpanes" | "codex") {
  return [
    protocol,
    ":",
    "//",
    "pets",
    "/install",
    "?name=Doro&imageUrl=",
    TEST_IMAGE_URL_ENCODED,
  ].join("");
}

describe("handleCCPanesDeepLinks", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    windowMock.setFocus.mockClear();
    windowMock.show.mockClear();
    useDialogStore.setState({
      settingsOpen: false,
      settingsSection: null,
    });
  });

  it("opens ccchan settings and installs a CC-Panes pet deep link", async () => {
    const load = vi.spyOn(useCCChanStore.getState(), "load").mockResolvedValue(undefined);
    const link = installLink("ccpanes");
    const window = getCurrentWindow();

    await handleCCPanesDeepLinks([TEST_ZIP_URL, link]);

    expect(window.show).toHaveBeenCalled();
    expect(window.setFocus).toHaveBeenCalled();
    expect(useDialogStore.getState().settingsOpen).toBe(true);
    expect(useDialogStore.getState().settingsSection).toBe("ccchan");
    expect(previewAndInstallCCChanPetUrl).toHaveBeenCalledWith(link, load);
  });

  it("ignores non-CC-Panes pet deep links", async () => {
    await handleCCPanesDeepLinks([
      installLink("codex"),
      TEST_ZIP_URL,
    ]);

    expect(getCurrentWindow).not.toHaveBeenCalled();
    expect(useDialogStore.getState().settingsOpen).toBe(false);
    expect(previewAndInstallCCChanPetUrl).not.toHaveBeenCalled();
  });
});
