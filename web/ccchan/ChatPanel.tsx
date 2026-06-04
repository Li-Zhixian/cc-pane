import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Maximize2, Send, Square, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { CCChanSettings, TerminalOutputPayload } from "./types";

interface ChatPanelProps {
  settings: CCChanSettings;
  sessionId: string | null;
  visible?: boolean;
  autoStart?: boolean;
  onSessionIdChange: (sessionId: string | null) => void;
  onClose: () => void;
}

function validateChatStartup(runtimeKind: string, wslRemotePath: string | null): string | null {
  if (runtimeKind !== "wsl") return null;
  const remotePath = wslRemotePath?.trim() ?? "";
  if (!remotePath) return "WSL chat 需要先在 cc酱角色设置里填写 WSL 远端路径。";
  if (!remotePath.startsWith("/")) return `WSL 远端路径必须以 / 开头：${remotePath}`;
  return null;
}

export function formatChatStartupError(error: unknown, aiEngine: string, runtimeKind: string): string {
  const raw = error instanceof Error ? error.message : String(error);
  const text = raw.trim() || "未知错误";
  const lower = text.toLowerCase();
  const engineLabel = aiEngine === "codex" ? "Codex" : aiEngine === "claude" ? "Claude Code" : aiEngine;
  const runtimeLabel = runtimeKind === "wsl" ? "WSL" : "本机";

  if (lower.includes("not found") || lower.includes("no such file") || lower.includes("command not found")) {
    return `${runtimeLabel} ${engineLabel} 启动失败：没有找到 CLI。请确认 ${aiEngine} 已安装，并且在该运行环境的 PATH 中可执行。原始错误：${text}`;
  }
  if (lower.includes("wsl") || lower.includes("remote path") || lower.includes("must start with /")) {
    return `WSL chat 启动失败：请检查发行版、远端路径和项目 hooks 同步。原始错误：${text}`;
  }
  if (lower.includes("mcp")) {
    return `${runtimeLabel} ${engineLabel} 启动失败：MCP 配置或注入异常。原始错误：${text}`;
  }
  if (lower.includes("provider") || lower.includes("auth") || lower.includes("login")) {
    return `${runtimeLabel} ${engineLabel} 启动失败：provider、认证或登录状态异常。原始错误：${text}`;
  }
  return `${runtimeLabel} ${engineLabel} 启动失败：${text}`;
}

export function ChatPanel({
  settings,
  sessionId,
  visible = true,
  autoStart = true,
  onSessionIdChange,
  onClose,
}: ChatPanelProps) {
  const [input, setInput] = useState("");
  const [lines, setLines] = useState<string[]>([]);
  const [starting, setStarting] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeRole = settings.roles.find((role) => role.id === settings.activeRoleId) ?? settings.roles[0];
  const roleSessionKey = activeRole
    ? `${activeRole.id}:${activeRole.aiEngine}:${activeRole.systemPrompt}:${activeRole.runtimeKind}:${activeRole.wslRemotePath ?? ""}:${activeRole.wslDistro ?? ""}`
    : "default";
  const outputRef = useRef<HTMLDivElement>(null);
  const startingRef = useRef(false);
  const latestRoleSessionKeyRef = useRef(roleSessionKey);
  const mountedRef = useRef(true);
  const [startupRetryToken, setStartupRetryToken] = useState(0);
  const previousRoleSessionKeyRef = useRef(roleSessionKey);
  const previousVisibleRef = useRef(visible);
  const stoppedByUserRef = useRef(false);
  const aiEngine = activeRole?.aiEngine ?? settings.aiEngine;
  const roleName = activeRole?.name ?? "默认助手";
  const systemPrompt = activeRole?.systemPrompt;
  const runtimeKind = activeRole?.runtimeKind ?? "local";
  const wslRemotePath = activeRole?.wslRemotePath ?? null;
  const wslDistro = activeRole?.wslDistro ?? null;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    if (visible || (!sessionId && !startingRef.current)) {
      latestRoleSessionKeyRef.current = roleSessionKey;
    }
  }, [roleSessionKey, sessionId, visible]);

  useEffect(() => {
    if (visible && !previousVisibleRef.current) {
      stoppedByUserRef.current = false;
    }
    previousVisibleRef.current = visible;
  }, [visible]);

  useEffect(() => {
    if (previousRoleSessionKeyRef.current === roleSessionKey) return;
    if (!sessionId) {
      if (visible || !startingRef.current) {
        previousRoleSessionKeyRef.current = roleSessionKey;
      }
      return;
    }
    if (!visible) return;
    previousRoleSessionKeyRef.current = roleSessionKey;
    invoke("stop_ccchan_chat", { sessionId }).catch(() => {});
    onSessionIdChange(null);
    setLines([]);
  }, [onSessionIdChange, roleSessionKey, sessionId, visible]);

  useEffect(() => {
    async function ensureSession() {
      if (sessionId || startingRef.current || !visible || !autoStart || stoppedByUserRef.current) return;
      const validationError = validateChatStartup(runtimeKind, wslRemotePath);
      if (validationError) {
        setError(validationError);
        return;
      }
      const requestedRoleSessionKey = roleSessionKey;
      startingRef.current = true;
      setStarting(true);
      setError(null);
      try {
        const nextSessionId = await invoke<string>("start_ccchan_chat", {
          aiEngine,
          systemPrompt,
          runtimeKind,
          wslRemotePath,
          wslDistro,
        });
        if (mountedRef.current && latestRoleSessionKeyRef.current === requestedRoleSessionKey) {
          onSessionIdChange(nextSessionId);
          return;
        }
        void invoke("stop_ccchan_chat", { sessionId: nextSessionId }).catch(() => {});
      } catch (err) {
        if (mountedRef.current && latestRoleSessionKeyRef.current === requestedRoleSessionKey) {
          setError(formatChatStartupError(err, aiEngine, runtimeKind));
        }
      } finally {
        startingRef.current = false;
        if (!mountedRef.current) return;
        setStarting(false);
        if (latestRoleSessionKeyRef.current !== requestedRoleSessionKey) {
          setStartupRetryToken((current) => current + 1);
        }
      }
    }

    void ensureSession();
  }, [
    aiEngine,
    autoStart,
    onSessionIdChange,
    roleSessionKey,
    runtimeKind,
    sessionId,
    startupRetryToken,
    systemPrompt,
    visible,
    wslDistro,
    wslRemotePath,
  ]);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    async function attachOutput() {
      const nextUnlisten = await listen<TerminalOutputPayload>("terminal-output", (event) => {
        if (event.payload.sessionId !== sessionId) return;
        setLines((current) => [...current, event.payload.data].slice(-400));
      });
      if (cancelled) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
    }

    if (sessionId) {
      attachOutput().catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      });
    }

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [sessionId]);

  useEffect(() => {
    outputRef.current?.scrollTo({ top: outputRef.current.scrollHeight });
  }, [lines, visible]);

  async function handleSubmit() {
    const text = input.trimEnd();
    if (!text || !sessionId) return;
    setInput("");
    setSending(true);
    setError(null);
    setLines((current) => [...current, `> ${text}\n`].slice(-400));
    try {
      await invoke("send_to_ccchan", { sessionId, text });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSending(false);
    }
  }

  async function handleStop() {
    if (!sessionId) return;
    stoppedByUserRef.current = true;
    try {
      await invoke("stop_ccchan_chat", { sessionId });
    } finally {
      onSessionIdChange(null);
      setLines([]);
    }
  }

  if (!visible) return null;

  return (
    <section
      className="flex h-[440px] w-[360px] flex-col overflow-hidden rounded-md border shadow-xl"
      style={{
        background: "var(--app-content)",
        borderColor: "var(--app-border)",
        color: "var(--app-text-primary)",
      }}
    >
      <header className="flex h-10 items-center justify-between px-3" style={{ borderBottom: "1px solid var(--app-border)" }}>
        <div className="flex min-w-0 items-center gap-2">
          <Maximize2 size={14} style={{ color: "var(--app-accent)" }} />
          <span className="truncate text-[13px] font-medium">cc酱 · {roleName} · {aiEngine} · {runtimeKind}</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            className="flex h-7 w-7 items-center justify-center rounded transition-colors hover:bg-[var(--app-hover)]"
            title="停止当前 chat"
            disabled={!sessionId}
            onClick={handleStop}
          >
            <Square size={13} />
          </button>
          <button
            type="button"
            className="flex h-7 w-7 items-center justify-center rounded transition-colors hover:bg-[var(--app-hover)]"
            title="关闭 chat"
            onClick={onClose}
          >
            <X size={14} />
          </button>
        </div>
      </header>

      <div
        ref={outputRef}
        className="min-h-0 flex-1 overflow-y-auto p-3 font-mono text-[12px] leading-5"
        style={{ background: "rgba(0,0,0,0.18)", color: "var(--app-text-primary)" }}
      >
        {starting && <p className="m-0" style={{ color: "var(--app-text-tertiary)" }}>启动 chat...</p>}
        {!starting && lines.length === 0 && (
          <p className="m-0" style={{ color: "var(--app-text-tertiary)" }}>输入消息开始和 cc酱对话。</p>
        )}
        {lines.length > 0 && <pre className="m-0 whitespace-pre-wrap break-words">{lines.join("")}</pre>}
        {error && <p className="mt-2 text-[12px] text-red-400">{error}</p>}
      </div>

      <div className="flex items-end gap-2 p-2" style={{ borderTop: "1px solid var(--app-border)" }}>
        <textarea
          value={input}
          className="min-h-[38px] flex-1 resize-none rounded-md px-2 py-2 text-[13px] outline-none"
          style={{
            border: "1px solid var(--app-border)",
            background: "var(--app-bg)",
            color: "var(--app-text-primary)",
          }}
          placeholder={sessionId ? "输入消息..." : "chat 启动中..."}
          disabled={!sessionId || sending}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void handleSubmit();
            }
          }}
        />
        <button
          type="button"
          className="flex h-[38px] w-[38px] shrink-0 items-center justify-center rounded-md transition-colors hover:bg-[var(--app-hover)] disabled:opacity-50"
          style={{ background: "var(--app-active-bg)", color: "var(--app-accent)" }}
          disabled={!sessionId || sending || input.trimEnd().length === 0}
          title="发送"
          onClick={() => void handleSubmit()}
        >
          <Send size={16} />
        </button>
      </div>
    </section>
  );
}
