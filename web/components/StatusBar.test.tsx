import "@/i18n";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import StatusBar from "./StatusBar";
import { DEFAULT_CCCHAN_SETTINGS, useCCChanStore } from "@/stores/useCCChanStore";
import { useSettingsStore, useTerminalStatusStore, useUpdateStore, useWorkspacesStore } from "@/stores";
import { createTestSettings } from "@/test/utils/testData";

vi.mock("@/components/ui/tooltip", () => ({
  Tooltip: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  TooltipContent: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  TooltipTrigger: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

vi.mock("@/hooks/useWindowControl", () => ({
  useWindowControl: () => ({
    isPinned: false,
    togglePin: vi.fn(),
  }),
}));

vi.mock("@/services", () => ({
  triggerUpdate: vi.fn(),
}));

const doroPet = {
  id: "doro.codex-pet",
  displayName: "Doro",
  description: "Doro pet",
  spritesheetUrl: "asset://doro",
  source: "builtin" as const,
  atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
  animations: { idle: { row: 0, frames: 1, fps: 1 } },
};

describe("StatusBar ccchan controls", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(DEFAULT_CCCHAN_SETTINGS);
      if (cmd === "get_ccchan_pets") return Promise.resolve([doroPet]);
      if (cmd === "show_ccchan") return Promise.resolve(undefined);
      if (cmd === "hide_ccchan") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    useCCChanStore.setState({
      settings: DEFAULT_CCCHAN_SETTINGS,
      pets: [doroPet],
      expanded: false,
      chatSessionId: null,
      loading: false,
      loaded: true,
    });
    useSettingsStore.setState({
      settings: { ...createTestSettings(), ccchan: DEFAULT_CCCHAN_SETTINGS },
      loading: false,
    });
    useWorkspacesStore.setState({
      workspaces: [],
      expandedWorkspaceId: null,
      expandedProjectId: null,
      loading: false,
    });
    useTerminalStatusStore.setState({
      statusMap: new Map(),
    });
    useUpdateStore.setState({
      available: false,
      version: null,
      body: null,
    });
  });

  it("hides ccchan from the status bar and updates local visibility state", async () => {
    render(<StatusBar />);

    await userEvent.click(screen.getByRole("button", { name: /隐藏 cc酱/ }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("hide_ccchan");
    });
    expect(useCCChanStore.getState().settings.windowVisible).toBe(false);
    expect(screen.getByRole("button", { name: /显示 cc酱/ })).toBeInTheDocument();
  });

  it("shows ccchan from the status bar and updates local visibility state", async () => {
    useCCChanStore.setState({
      settings: { ...DEFAULT_CCCHAN_SETTINGS, windowVisible: false },
    });
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") {
        return Promise.resolve({ ...DEFAULT_CCCHAN_SETTINGS, windowVisible: false });
      }
      if (cmd === "get_ccchan_pets") return Promise.resolve([doroPet]);
      if (cmd === "show_ccchan") return Promise.resolve(undefined);
      if (cmd === "hide_ccchan") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });

    render(<StatusBar />);

    await userEvent.click(screen.getByRole("button", { name: /显示 cc酱/ }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("show_ccchan");
    });
    expect(useCCChanStore.getState().settings.windowVisible).toBe(true);
    expect(screen.getByRole("button", { name: /隐藏 cc酱/ })).toBeInTheDocument();
  });
});
