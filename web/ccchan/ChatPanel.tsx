import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Maximize2, Send, Square, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { CCChanSettings, TerminalOutputPayload } from "./types";

interface ChatPanelProps {
  settings: CCChanSettings;
  sessionId: string | null;
  onSessionIdChange: (sessionId: string | null) => void;
  onClose: () => void;
}

export function ChatPanel({ settings, sessionId, onSessionIdChange, onClose }: ChatPanelProps) {
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
    latestRoleSessionKeyRef.current = roleSessionKey;
  }, [roleSessionKey]);

  useEffect(() => {
    if (previousRoleSessionKeyRef.current === roleSessionKey) return;
    previousRoleSessionKeyRef.current = roleSessionKey;
    if (!sessionId) return;
    invoke("stop_ccchan_chat", { sessionId }).catch(() => {});
    onSessionIdChange(null);
    setLines([]);
  }, [onSessionIdChange, roleSessionKey, sessionId]);

  useEffect(() => {
    async function ensureSession() {
      if (sessionId || startingRef.current) return;
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
          setError(err instanceof Error ? err.message : String(err));
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
  }, [aiEngine, onSessionIdChange, roleSessionKey, runtimeKind, sessionId, startupRetryToken, systemPrompt, wslDistro, wslRemotePath]);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    async function attachOutput() {
      unlisten = await listen<TerminalOutputPayload>("terminal-output", (event) => {
        if (event.payload.sessionId !== sessionId) return;
        setLines((current) => [...current, event.payload.data].slice(-400));
      });
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
  }, [lines]);

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
    try {
      await invoke("stop_ccchan_chat", { sessionId });
    } finally {
      onSessionIdChange(null);
      setLines([]);
    }
  }

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
