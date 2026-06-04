import { beforeEach, describe, expect, it, vi } from "vitest";
import { handleCCPanesDeepLinks } from "./deepLink";
import { previewAndInstallCCChanPetUrl } from "./installPet";
import { openCCChanSettingsInMainWindow } from "./openSettings";
import { useCCChanStore } from "@/stores/useCCChanStore";
import { useDialogStore } from "@/stores/useDialogStore";

vi.mock("./installPet", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./installPet")>();
  return {
    ...actual,
    previewAndInstallCCChanPetUrl: vi.fn(() => Promise.resolve(true)),
  };
});

vi.mock("./openSettings", () => ({
  openCCChanSettingsInMainWindow: vi.fn(() => Promise.resolve()),
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
    useDialogStore.setState({
      settingsOpen: false,
      settingsSection: null,
    });
  });

  it("opens ccchan settings and installs a CC-Panes pet deep link", async () => {
    const load = vi.spyOn(useCCChanStore.getState(), "load").mockResolvedValue(undefined);
    const link = installLink("ccpanes");

    await handleCCPanesDeepLinks([TEST_ZIP_URL, link]);

    expect(openCCChanSettingsInMainWindow).toHaveBeenCalled();
    expect(previewAndInstallCCChanPetUrl).toHaveBeenCalledWith(link, load);
  });

  it("ignores non-CC-Panes pet deep links", async () => {
    await handleCCPanesDeepLinks([
      installLink("codex"),
      TEST_ZIP_URL,
    ]);

    expect(openCCChanSettingsInMainWindow).not.toHaveBeenCalled();
    expect(useDialogStore.getState().settingsOpen).toBe(false);
    expect(previewAndInstallCCChanPetUrl).not.toHaveBeenCalled();
  });
});
