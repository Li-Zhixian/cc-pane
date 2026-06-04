import { beforeEach, describe, expect, it, vi } from "vitest";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openCCChanSettingsInMainWindow } from "./openSettings";
import { useDialogStore } from "@/stores/useDialogStore";

const windowMock = {
  setFocus: vi.fn(() => Promise.resolve()),
  show: vi.fn(() => Promise.resolve()),
  unminimize: vi.fn(() => Promise.resolve()),
};

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => windowMock),
}));

describe("openCCChanSettingsInMainWindow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useDialogStore.setState({
      settingsOpen: false,
      settingsSection: null,
    });
  });

  it("restores and focuses the main window before opening ccchan settings", async () => {
    await openCCChanSettingsInMainWindow();

    expect(getCurrentWindow).toHaveBeenCalled();
    expect(windowMock.show).toHaveBeenCalled();
    expect(windowMock.unminimize).toHaveBeenCalled();
    expect(windowMock.setFocus).toHaveBeenCalled();
    expect(useDialogStore.getState().settingsOpen).toBe(true);
    expect(useDialogStore.getState().settingsSection).toBe("ccchan");
  });

  it("still opens settings when a window focus operation fails", async () => {
    windowMock.show.mockRejectedValueOnce(new Error("hidden"));

    await openCCChanSettingsInMainWindow();

    expect(windowMock.unminimize).toHaveBeenCalled();
    expect(windowMock.setFocus).toHaveBeenCalled();
    expect(useDialogStore.getState().settingsOpen).toBe(true);
    expect(useDialogStore.getState().settingsSection).toBe("ccchan");
  });
});
