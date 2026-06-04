import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChatPanel, formatChatStartupError } from "./ChatPanel";
import { DEFAULT_CCCHAN_SETTINGS, DEFAULT_CCCHAN_ROLE_PROMPT } from "@/stores/useCCChanStore";
import type { CCChanRolePreset, CCChanSettings } from "./types";

const defaultRole: CCChanRolePreset = {
  id: "claude-local",
  name: "Claude Local",
  aiEngine: "claude",
  petId: "doro.codex-pet",
  systemPrompt: DEFAULT_CCCHAN_ROLE_PROMPT,
  runtimeKind: "local",
  wslRemotePath: null,
  wslDistro: null,
};

const wslRole: CCChanRolePreset = {
  id: "codex-wsl",
  name: "Codex WSL",
  aiEngine: "codex",
  petId: "doro.codex-pet",
  systemPrompt: "Use Codex inside WSL.",
  runtimeKind: "wsl",
  wslRemotePath: "/mnt/d/my-project/cc-pane",
  wslDistro: "Ubuntu-24.04",
};

function makeSettings(activeRoleId = defaultRole.id): CCChanSettings {
  return {
    ...DEFAULT_CCCHAN_SETTINGS,
    activeRoleId,
    roles: [defaultRole, wslRole],
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
}

describe("ChatPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listen).mockResolvedValue(() => {});
  });

  it("starts a local chat session with the active role settings", async () => {
    const onSessionIdChange = vi.fn();
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "start_ccchan_chat") return Promise.resolve("local-session");
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });

    render(
      <ChatPanel
        settings={makeSettings()}
        sessionId={null}
        onSessionIdChange={onSessionIdChange}
        onClose={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(onSessionIdChange).toHaveBeenCalledWith("local-session");
    });
    expect(invoke).toHaveBeenCalledWith("start_ccchan_chat", {
      aiEngine: "claude",
      systemPrompt: DEFAULT_CCCHAN_ROLE_PROMPT,
      runtimeKind: "local",
      wslRemotePath: null,
      wslDistro: null,
    });
  });

  it("starts a WSL chat session with explicit distro and remote path", async () => {
    const onSessionIdChange = vi.fn();
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "start_ccchan_chat") return Promise.resolve("wsl-session");
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });

    render(
      <ChatPanel
        settings={makeSettings(wslRole.id)}
        sessionId={null}
        onSessionIdChange={onSessionIdChange}
        onClose={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(onSessionIdChange).toHaveBeenCalledWith("wsl-session");
    });
    expect(invoke).toHaveBeenCalledWith("start_ccchan_chat", {
      aiEngine: "codex",
      systemPrompt: "Use Codex inside WSL.",
      runtimeKind: "wsl",
      wslRemotePath: "/mnt/d/my-project/cc-pane",
      wslDistro: "Ubuntu-24.04",
    });
  });

  it("does not start a WSL chat session with an invalid remote path", async () => {
    const invalidWslRole: CCChanRolePreset = {
      ...wslRole,
      wslRemotePath: "D:\\my-project\\cc-pane",
    };
    const settings: CCChanSettings = {
      ...makeSettings(invalidWslRole.id),
      roles: [defaultRole, invalidWslRole],
    };
    const onSessionIdChange = vi.fn();
    vi.mocked(invoke).mockResolvedValue("should-not-start");

    render(
      <ChatPanel
        settings={settings}
        sessionId={null}
        onSessionIdChange={onSessionIdChange}
        onClose={vi.fn()}
      />,
    );

    expect(await screen.findByText(/WSL 远端路径必须以 \/ 开头/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("start_ccchan_chat", expect.anything());
    expect(onSessionIdChange).not.toHaveBeenCalled();
  });

  it("formats CLI startup errors with actionable context", () => {
    expect(formatChatStartupError(new Error("program not found: codex"), "codex", "wsl")).toContain(
      "WSL Codex 启动失败：没有找到 CLI",
    );
    expect(formatChatStartupError(new Error("provider auth failed"), "claude", "local")).toContain(
      "本机 Claude Code 启动失败：provider、认证或登录状态异常",
    );
  });

  it("stops stale startup sessions and retries the latest role", async () => {
    const firstStart = deferred<string>();
    const onSessionIdChange = vi.fn();
    vi.mocked(invoke).mockImplementation((cmd, args) => {
      if (cmd === "start_ccchan_chat") {
        const startCalls = vi.mocked(invoke).mock.calls.filter(([name]) => name === "start_ccchan_chat");
        return startCalls.length === 1 ? firstStart.promise : Promise.resolve("wsl-session");
      }
      if (cmd === "stop_ccchan_chat") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd} ${JSON.stringify(args)}`));
    });

    const { rerender } = render(
      <ChatPanel
        settings={makeSettings()}
        sessionId={null}
        onSessionIdChange={onSessionIdChange}
        onClose={vi.fn()}
      />,
    );
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_ccchan_chat", expect.objectContaining({ aiEngine: "claude" }));
    });

    rerender(
      <ChatPanel
        settings={makeSettings(wslRole.id)}
        sessionId={null}
        onSessionIdChange={onSessionIdChange}
        onClose={vi.fn()}
      />,
    );
    firstStart.resolve("stale-local-session");

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("stop_ccchan_chat", { sessionId: "stale-local-session" });
    });
    await waitFor(() => {
      expect(onSessionIdChange).toHaveBeenCalledWith("wsl-session");
    });
    expect(onSessionIdChange).not.toHaveBeenCalledWith("stale-local-session");
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "start_ccchan_chat")).toHaveLength(2);
  });

  it("stops the active session when the user clicks stop", async () => {
    const onSessionIdChange = vi.fn();
    vi.mocked(invoke).mockResolvedValue(undefined);

    render(
      <ChatPanel
        settings={makeSettings()}
        sessionId="active-session"
        onSessionIdChange={onSessionIdChange}
        onClose={vi.fn()}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "停止当前 chat" }));

    expect(invoke).toHaveBeenCalledWith("stop_ccchan_chat", { sessionId: "active-session" });
    expect(onSessionIdChange).toHaveBeenCalledWith(null);
  });
});
