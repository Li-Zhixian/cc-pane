import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { CCChanApp } from "./CCChanApp";
import { DEFAULT_CCCHAN_SETTINGS, FALLBACK_PET, useCCChanStore } from "@/stores/useCCChanStore";
import { useTerminalStatusStore } from "@/stores/useTerminalStatusStore";
import type { TerminalOutputPayload } from "./types";
import type { TerminalStatusInfo } from "@/types";

vi.mock("@tauri-apps/api/window", () => ({
  currentMonitor: vi.fn(() => Promise.resolve(null)),
  getCurrentWindow: vi.fn(() => ({
    close: vi.fn(() => Promise.resolve()),
    outerPosition: vi.fn(() => Promise.resolve({ x: 0, y: 0 })),
    scaleFactor: vi.fn(() => Promise.resolve(1)),
    startDragging: vi.fn(() => Promise.resolve()),
  })),
}));

vi.mock("@/utils/notificationSound", () => ({
  playNotificationSound: vi.fn(() => Promise.resolve()),
}));

describe("CCChanApp", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listen).mockResolvedValue(() => {});
    useCCChanStore.setState({
      settings: DEFAULT_CCCHAN_SETTINGS,
      pets: [FALLBACK_PET],
      expanded: true,
      chatSessionId: "active-session",
      loading: false,
      loaded: true,
    });
    useTerminalStatusStore.setState({
      statusMap: new Map(),
      _unlisten: null,
      _idleCheckInterval: null,
      _initialized: false,
    });
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(DEFAULT_CCCHAN_SETTINGS);
      if (cmd === "get_ccchan_pets") return Promise.resolve([FALLBACK_PET]);
      if (cmd === "get_all_terminal_status") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
  });

  it("announces readiness after subscribing to active main session updates", async () => {
    vi.mocked(listen).mockImplementation((eventName) => {
      if (eventName === "ccchan:active-session") return Promise.resolve(() => {});
      return Promise.resolve(() => {});
    });

    render(<CCChanApp />);

    await waitFor(() => {
      expect(emitTo).toHaveBeenCalledWith("main", "ccchan:ready");
    });
  });

  it("filters focused-window status dots to the active main session and focuses it from the pet window", async () => {
    const settings = { ...DEFAULT_CCCHAN_SETTINGS, scopeMode: "focusedWindow" as const };
    const statuses: TerminalStatusInfo[] = [
      { sessionId: "inactive-session", status: "toolRunning", lastOutputAt: 1000, updatedAt: 1000 },
      { sessionId: "focused-session", status: "waitingInput", lastOutputAt: 1000, updatedAt: 1000 },
    ];
    const handlers: {
      activeSession?: (event: { payload: { sessionId?: string | null } }) => void;
    } = {};

    useCCChanStore.setState({
      settings,
      expanded: false,
      chatSessionId: null,
    });
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(settings);
      if (cmd === "get_ccchan_pets") return Promise.resolve([FALLBACK_PET]);
      if (cmd === "get_all_terminal_status") return Promise.resolve(statuses);
      return Promise.resolve(undefined);
    });
    vi.mocked(listen).mockImplementation((eventName, handler) => {
      if (eventName === "ccchan:active-session") {
        handlers.activeSession = handler as (event: { payload: { sessionId?: string | null } }) => void;
      }
      return Promise.resolve(() => {});
    });

    render(<CCChanApp />);

    await waitFor(() => {
      expect(handlers.activeSession).toBeTruthy();
    });
    act(() => {
      handlers.activeSession?.({ payload: { sessionId: "focused-session" } });
    });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "打开 cc酱 chat" })).toHaveAttribute("data-pet-state", "waiting");
    });
    expect(await screen.findByRole("button", { name: "Focus session focused-session" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Focus session inactive-session" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Focus session focused-session" }));

    expect(emitTo).toHaveBeenCalledWith("main", "ccchan:focus-session", { sessionId: "focused-session" });
  });

  it("keeps the chat panel mounted after closing and replays hidden output on reopen", async () => {
    const handlers: {
      output?: (event: { payload: TerminalOutputPayload }) => void;
    } = {};
    vi.mocked(listen).mockImplementation((eventName, handler) => {
      if (eventName === "terminal-output") {
        handlers.output = handler as (event: { payload: TerminalOutputPayload }) => void;
      }
      return Promise.resolve(() => {});
    });

    render(<CCChanApp />);

    await waitFor(() => {
      expect(handlers.output).toBeTruthy();
    });

    await userEvent.click(screen.getByRole("button", { name: "关闭 chat" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("resize_ccchan_for_chat", { expanded: false });
    });
    expect(invoke).not.toHaveBeenCalledWith("stop_ccchan_chat", { sessionId: "active-session" });

    act(() => {
      handlers.output?.({
        payload: { sessionId: "active-session", data: "hidden parent output\n" },
      });
    });

    await userEvent.click(screen.getByRole("button", { name: "打开 cc酱 chat" }));

    expect(await screen.findByText(/hidden parent output/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("stop_ccchan_chat", { sessionId: "active-session" });
  });
});
