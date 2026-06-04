import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CCChanApp } from "./CCChanApp";
import { DEFAULT_CCCHAN_SETTINGS, FALLBACK_PET, useCCChanStore } from "@/stores/useCCChanStore";
import { useTerminalStatusStore } from "@/stores/useTerminalStatusStore";
import type { TerminalOutputPayload } from "./types";
import type { TerminalStatusInfo } from "@/types";

const windowMock = {
  close: vi.fn(() => Promise.resolve()),
  outerPosition: vi.fn(() => Promise.resolve({ x: 0, y: 0 })),
  scaleFactor: vi.fn(() => Promise.resolve(1)),
  startDragging: vi.fn(() => Promise.resolve()),
};

vi.mock("@tauri-apps/api/window", () => ({
  currentMonitor: vi.fn(() => Promise.resolve(null)),
  getCurrentWindow: vi.fn(() => windowMock),
}));

vi.mock("@/utils/notificationSound", () => ({
  playNotificationSound: vi.fn(() => Promise.resolve()),
}));

describe("CCChanApp", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useRealTimers();
    windowMock.close.mockClear();
    windowMock.outerPosition.mockResolvedValue({ x: 0, y: 0 });
    windowMock.scaleFactor.mockResolvedValue(1);
    windowMock.startDragging.mockClear();
    vi.mocked(listen).mockResolvedValue(() => {});
    vi.mocked(getCurrentWebview().listen).mockResolvedValue(() => {});
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

  it("updates the pet state from terminal status events", async () => {
    let terminalStatusHandler: ((event: { payload: TerminalStatusInfo }) => void) | null = null;
    vi.mocked(getCurrentWebview().listen).mockImplementation(async (eventName, handler) => {
      if (eventName === "terminal-status") {
        terminalStatusHandler = handler as (event: { payload: TerminalStatusInfo }) => void;
      }
      return () => {};
    });
    useCCChanStore.setState({
      expanded: false,
      chatSessionId: null,
    });

    render(<CCChanApp />);

    await waitFor(() => {
      expect(terminalStatusHandler).toBeTruthy();
      expect(screen.getByRole("button", { name: "打开 cc酱 chat" })).toHaveAttribute("data-pet-state", "idle");
    });

    act(() => {
      terminalStatusHandler?.({
        payload: {
          sessionId: "event-session",
          status: "waitingInput",
          lastOutputAt: 1000,
          updatedAt: 1000,
        },
      });
    });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "打开 cc酱 chat" })).toHaveAttribute("data-pet-state", "waiting");
    });

    act(() => {
      terminalStatusHandler?.({
        payload: {
          sessionId: "event-session",
          status: "toolRunning",
          lastOutputAt: 2000,
          updatedAt: 2000,
        },
      });
    });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "打开 cc酱 chat" })).toHaveAttribute("data-pet-state", "working");
    });
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

  it("persists the logical window position after a drag release", async () => {
    const nowSpy = vi.spyOn(Date, "now");
    nowSpy.mockReturnValueOnce(1_000).mockReturnValueOnce(1_250);
    windowMock.outerPosition.mockResolvedValue({ x: 300, y: 180 });
    windowMock.scaleFactor.mockResolvedValue(1.5);
    useCCChanStore.setState({
      expanded: false,
      chatSessionId: null,
    });

    render(<CCChanApp />);
    fireEvent.mouseDown(screen.getByRole("button", { name: "打开 cc酱 chat" }), { button: 0 });
    window.dispatchEvent(new MouseEvent("mouseup"));

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(getCurrentWindow).toHaveBeenCalled();
    expect(windowMock.startDragging).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("move_ccchan_window", { x: 200, y: 120 });
    const settings = useCCChanStore.getState().settings;
    expect(settings.windowX).toBe(200);
    expect(settings.windowY).toBe(120);
    nowSpy.mockRestore();
  });
});
