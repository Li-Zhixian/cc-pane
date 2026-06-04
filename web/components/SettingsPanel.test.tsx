import "@/i18n";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import SettingsPanel from "./SettingsPanel";
import { settingsService } from "@/services";
import { DEFAULT_CCCHAN_SETTINGS, useCCChanStore } from "@/stores/useCCChanStore";
import { useDialogStore, useSettingsStore } from "@/stores";
import { createTestSettings } from "@/test/utils/testData";
import type { PetMeta } from "@/ccchan/types";

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    info: vi.fn(),
    success: vi.fn(),
  },
}));

vi.mock("@/services", () => ({
  settingsService: {
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
  },
}));

vi.mock("./settings/GeneralSection", () => ({
  default: () => <div>General Section</div>,
}));

vi.mock("./settings/NotificationSection", () => ({
  default: () => <div>Notification Section</div>,
}));

vi.mock("./settings/ProviderSection", () => ({
  default: () => <div>Provider Section</div>,
}));

vi.mock("./settings/ProxySection", () => ({
  default: () => <div>Proxy Section</div>,
}));

vi.mock("./settings/TerminalSection", () => ({
  default: () => <div>Terminal Section</div>,
}));

vi.mock("./settings/ShortcutsSection", () => ({
  default: () => <div>Shortcuts Section</div>,
}));

vi.mock("./settings/AboutSection", () => ({
  default: () => <div>About Section</div>,
}));

vi.mock("./settings/ScreenshotSection", () => ({
  default: () => <div>Screenshot Section</div>,
}));

vi.mock("./settings/SharedMcpSection", () => ({
  default: () => <div>Shared MCP Section</div>,
}));

vi.mock("./settings/VoiceSection", () => ({
  default: () => <div>Voice Section</div>,
}));

const doroPet: PetMeta = {
  id: "doro.codex-pet",
  displayName: "Doro",
  description: "Doro pet",
  spritesheetUrl: "asset://doro",
  source: "builtin",
  atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
  animations: { idle: { row: 0, frames: 1, fps: 1 } },
};

function renderPanel() {
  render(<SettingsPanel open onOpenChange={vi.fn()} />);
}

describe("SettingsPanel ccchan validation", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(settingsService.updateSettings).mockResolvedValue();
    vi.mocked(settingsService.getSettings).mockResolvedValue(createTestSettings());
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(DEFAULT_CCCHAN_SETTINGS);
      if (cmd === "get_ccchan_pets") return Promise.resolve([doroPet]);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    useSettingsStore.setState({
      settings: { ...createTestSettings(), ccchan: DEFAULT_CCCHAN_SETTINGS },
      loading: false,
    });
    useDialogStore.setState({
      settingsOpen: true,
      settingsSection: "ccchan",
    });
    useCCChanStore.setState({
      settings: DEFAULT_CCCHAN_SETTINGS,
      pets: [doroPet],
      expanded: false,
      chatSessionId: null,
      loading: false,
      loaded: true,
    });
  });

  it("blocks saving a WSL ccchan role without a remote path", async () => {
    renderPanel();

    await userEvent.click(screen.getByRole("button", { name: "Codex WSL" }));
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(settingsService.updateSettings).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining("需要填写 WSL 远端路径"));
    expect(screen.getByText(/WSL 角色需要填写以 \/ 开头的绝对远端路径/)).toBeInTheDocument();
  });

  it("saves a WSL ccchan role after a Linux remote path is configured", async () => {
    renderPanel();

    await userEvent.click(screen.getByRole("button", { name: "Codex WSL" }));
    fireEvent.change(screen.getByPlaceholderText("/mnt/d/my-project/cc-pane"), {
      target: { value: "/mnt/d/my-project/cc-pane" },
    });
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => {
      expect(settingsService.updateSettings).toHaveBeenCalledTimes(1);
    });
    const saved = vi.mocked(settingsService.updateSettings).mock.calls[0][0];
    const role = saved.ccchan?.roles.find((item) => item.id === saved.ccchan?.activeRoleId);
    expect(role).toEqual(expect.objectContaining({
      aiEngine: "codex",
      runtimeKind: "wsl",
      wslRemotePath: "/mnt/d/my-project/cc-pane",
    }));
  });
});
