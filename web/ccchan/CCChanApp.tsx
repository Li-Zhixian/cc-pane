import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { Toaster, toast } from "sonner";
import { useCCChanStore } from "@/stores/useCCChanStore";
import { useTerminalStatusStore } from "@/stores";
import type { TerminalStatusInfo, TerminalStatusType } from "@/types";
import { playNotificationSound } from "@/utils/notificationSound";
import { aggregateStatus } from "./statusAggregator";
import { ChatPanel } from "./ChatPanel";
import { ContextMenu, type CCChanContextMenuPosition } from "./ContextMenu";
import { SessionDots } from "./SessionDots";
import { SpritePet } from "./SpritePet";
import type { CCChanEvent, CCChanPetState } from "./types";

const PET_SIZE = 120;
const CHAT_EXPANDED_W = 460;
const CHAT_EXPANDED_H = 640;
const MENU_W = 460;
const MENU_H = 260;
const INITIAL_WANDER_AFTER_MS = 4_000;
const WANDER_REPEAT_MIN_MS = 8_000;
const WANDER_REPEAT_MAX_MS = 18_000;
const WANDER_SPEED_PX_PER_SEC = 110;
const WANDER_STEP_MS = 120;
const WANDER_EDGE_PAD = 40;
const WANDER_MIN_DISTANCE = 160;
const DRAG_PERSIST_DEBOUNCE_MS = 320;
const DRAG_IDLE_FINISH_MS = 1500;
const POSITION_SYNC_INTERVAL_MS = 1200;
const POSITION_SYNC_EPSILON = 1;

function formatSessionTitle(sessionId: string) {
  const trimmed = sessionId.trim();
  return trimmed ? `Session ${trimmed.slice(0, 8)}` : "Session";
}

function getEventPetState(event: CCChanEvent): CCChanPetState {
  if (event.kind === "task-complete") return "happy";
  if (event.kind === "task-failed") return "sad";
  return "waiting";
}

export function CCChanApp() {
  const settings = useCCChanStore((state) => state.settings);
  const pets = useCCChanStore((state) => state.pets);
  const expanded = useCCChanStore((state) => state.expanded);
  const chatSessionId = useCCChanStore((state) => state.chatSessionId);
  const loadCCChan = useCCChanStore((state) => state.load);
  const setExpanded = useCCChanStore((state) => state.setExpanded);
  const setChatSessionId = useCCChanStore((state) => state.setChatSessionId);
  const setWindowVisible = useCCChanStore((state) => state.setWindowVisible);
  const setPosition = useCCChanStore((state) => state.setPosition);
  const setActiveRoleId = useCCChanStore((state) => state.setActiveRoleId);
  const switchPet = useCCChanStore((state) => state.switchPet);
  const initTerminalStatus = useTerminalStatusStore((state) => state.init);
  const cleanupTerminalStatus = useTerminalStatusStore((state) => state.cleanup);
  const statusMap = useTerminalStatusStore((state) => state.statusMap);

  const [eventState, setEventState] = useState<CCChanPetState | null>(null);
  const [eggState, setEggState] = useState<CCChanPetState | null>(null);
  const [bubbleText, setBubbleText] = useState<string | null>(null);
  const [activeMainSessionId, setActiveMainSessionId] = useState<string | null>(null);
  const [menuPosition, setMenuPosition] = useState<CCChanContextMenuPosition | null>(null);
  const [menuOwnsResize, setMenuOwnsResize] = useState(false);
  const [chatMounted, setChatMounted] = useState(() => expanded || Boolean(chatSessionId));
  const [dragging, setDragging] = useState(false);
  const dragStartedAtRef = useRef<number | null>(null);
  const suppressNextClickRef = useRef(false);
  const dragPersistTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const dragFinishTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastPersistedDragPhysicalRef = useRef<{ x: number; y: number } | null>(null);

  const selectedPet = useMemo(
    () => pets.find((pet) => pet.id === settings.defaultPetId) ?? pets[0],
    [pets, settings.defaultPetId],
  );

  const aggregateState = useMemo(() => {
    const statuses = Array.from(statusMap.values())
      .filter((info) => settings.scopeMode === "global" || info.sessionId === activeMainSessionId)
      .map((info) => info.status as TerminalStatusType);
    return aggregateStatus(statuses);
  }, [activeMainSessionId, settings.scopeMode, statusMap]);

  const petState = eventState ?? eggState ?? aggregateState;

  useEffect(() => {
    if (eventState || expanded || menuPosition || dragging) {
      setEggState(null);
      return;
    }

    let cancelled = false;
    let nextTimer: ReturnType<typeof setTimeout> | null = null;
    let stepTimer: ReturnType<typeof setTimeout> | null = null;

    function clearTimers() {
      if (nextTimer) clearTimeout(nextTimer);
      if (stepTimer) clearTimeout(stepTimer);
      nextTimer = null;
      stepTimer = null;
    }

    function scheduleNext() {
      if (cancelled) return;
      const next = WANDER_REPEAT_MIN_MS + Math.random() * (WANDER_REPEAT_MAX_MS - WANDER_REPEAT_MIN_MS);
      nextTimer = setTimeout(wander, next);
    }

    async function wander() {
      if (cancelled) {
        scheduleNext();
        return;
      }
      try {
        const win = getCurrentWindow();
        const monitor = await currentMonitor();
        const physicalPos = await win.outerPosition();
        const scale = await win.scaleFactor();
        const startX = physicalPos.x / scale;
        const startY = physicalPos.y / scale;
        if (!monitor) {
          scheduleNext();
          return;
        }
        const mScale = monitor.scaleFactor;
        const mx = monitor.position.x / mScale;
        const my = monitor.position.y / mScale;
        const mw = monitor.size.width / mScale;
        const mh = monitor.size.height / mScale;
        let targetX = startX;
        let targetY = startY;
        let dist = 0;
        for (let attempt = 0; attempt < 8; attempt += 1) {
          targetX = mx + WANDER_EDGE_PAD + Math.random() * Math.max(1, mw - WANDER_EDGE_PAD * 2 - PET_SIZE);
          targetY = my + WANDER_EDGE_PAD + Math.random() * Math.max(1, mh - WANDER_EDGE_PAD * 2 - PET_SIZE);
          dist = Math.hypot(targetX - startX, targetY - startY);
          if (dist >= WANDER_MIN_DISTANCE) break;
        }
        const dx = targetX - startX;
        const dy = targetY - startY;
        if (dist < 30) {
          scheduleNext();
          return;
        }
        const duration = Math.min(12_000, Math.max(1_600, (dist / WANDER_SPEED_PX_PER_SEC) * 1000));
        const startTime = performance.now();
        setEggState("walking");

        const stepOnce = async () => {
          if (cancelled) return;
          const elapsed = performance.now() - startTime;
          const t = Math.min(1, elapsed / duration);
          const nx = startX + dx * t;
          const ny = startY + dy * t;
          const done = t >= 1;
          await invoke("move_ccchan_window", { x: nx, y: ny, persist: done }).catch(() => {});
          if (done || cancelled) {
            setEggState(null);
            scheduleNext();
            return;
          }
          stepTimer = setTimeout(stepOnce, WANDER_STEP_MS);
        };
        stepOnce();
      } catch {
        setEggState(null);
        scheduleNext();
      }
    }

    nextTimer = setTimeout(wander, INITIAL_WANDER_AFTER_MS);
    return () => {
      cancelled = true;
      clearTimers();
      setEggState(null);
    };
  }, [dragging, eventState, expanded, menuPosition]);

  useEffect(() => {
    const style = document.createElement("style");
    style.textContent = "html, body, #root { background: transparent !important; margin: 0; padding: 0; overflow: hidden; }";
    document.head.appendChild(style);
    return () => style.remove();
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    listen<{ sessionId?: string | null }>("ccchan:active-session", (event) => {
      if (!cancelled) setActiveMainSessionId(event.payload?.sessionId ?? null);
    }).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
      void emitTo("main", "ccchan:ready").catch(() => {});
    }).catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    void loadCCChan();
    void initTerminalStatus();
    invoke<TerminalStatusInfo[]>("get_all_terminal_status")
      .then((statuses) => {
        if (!Array.isArray(statuses)) return;
        useTerminalStatusStore.setState(() => {
          const next = new Map(statuses.map((status) => [status.sessionId, status]));
          return { statusMap: next };
        });
      })
      .catch(() => {});
    return cleanupTerminalStatus;
  }, [cleanupTerminalStatus, initTerminalStatus, loadCCChan]);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    listen("ccchan:settings-updated", () => {
      if (!cancelled) void loadCCChan();
    }).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
    }).catch(() => {});

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadCCChan]);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;

    listen<CCChanEvent>("ccchan-event", (event) => {
      const payload = event.payload;
      const nextState = getEventPetState(payload);
      const title = payload.title ?? formatSessionTitle(payload.sessionId);
      setEventState(nextState);
      setBubbleText(payload.kind === "task-complete" ? `${title} 完成` : payload.kind === "task-failed" ? `${title} 失败` : `${title} 等待输入`);
      if (payload.kind === "task-complete") toast.success(title);
      if (payload.kind === "task-failed") toast.error(title);
      if (payload.kind === "task-waiting") toast.info(title);
      if (settings.soundEnabled) {
        playNotificationSound().catch((error) => {
          console.warn("ccchan notification sound failed:", error);
        });
      }
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        setEventState(null);
        setBubbleText(null);
      }, 3600);
    }).then((fn) => {
      unlisten = fn;
    }).catch(() => {});

    return () => {
      if (timer) clearTimeout(timer);
      unlisten?.();
    };
  }, [settings.soundEnabled]);

  useEffect(() => {
    let cancelled = false;
    let unlistenMoved: UnlistenFn | null = null;
    let positionSyncTimer: ReturnType<typeof setInterval> | null = null;

    function clearDragTimers() {
      if (dragPersistTimerRef.current) clearTimeout(dragPersistTimerRef.current);
      if (dragFinishTimerRef.current) clearTimeout(dragFinishTimerRef.current);
      dragPersistTimerRef.current = null;
      dragFinishTimerRef.current = null;
    }

    async function persistDragPosition(physicalPos?: { x: number; y: number }) {
      try {
        const win = getCurrentWindow();
        const pos = physicalPos ?? await win.outerPosition();
        const scale = await win.scaleFactor();
        const logicalX = pos.x / scale;
        const logicalY = pos.y / scale;
        lastPersistedDragPhysicalRef.current = { x: pos.x, y: pos.y };
        setPosition(logicalX, logicalY);
        await invoke("move_ccchan_window", { x: logicalX, y: logicalY });
      } catch {
        /* drag end save best-effort */
      }
    }

    function finishDrag() {
      const startedAt = dragStartedAtRef.current;
      if (!startedAt) return;
      dragStartedAtRef.current = null;
      setDragging(false);
      lastPersistedDragPhysicalRef.current = null;
      clearDragTimers();
      if (Date.now() - startedAt > 180) suppressNextClickRef.current = true;
    }

    function scheduleMovedPersist(physicalPos: { x: number; y: number }) {
      const lastPersisted = lastPersistedDragPhysicalRef.current;
      if (
        lastPersisted &&
        Math.abs(lastPersisted.x - physicalPos.x) < 1 &&
        Math.abs(lastPersisted.y - physicalPos.y) < 1
      ) {
        return;
      }
      clearDragTimers();
      dragPersistTimerRef.current = setTimeout(() => {
        dragPersistTimerRef.current = null;
        void persistDragPosition(physicalPos);
      }, DRAG_PERSIST_DEBOUNCE_MS);
      if (dragStartedAtRef.current) {
        dragFinishTimerRef.current = setTimeout(() => {
          void persistDragPosition(physicalPos);
          finishDrag();
        }, DRAG_IDLE_FINISH_MS);
      }
    }

    function handleMouseUp() {
      if (!dragStartedAtRef.current) return;
      clearDragTimers();
      setTimeout(() => {
        void persistDragPosition();
        finishDrag();
      }, 0);
    }

    const win = getCurrentWindow();
    win.onMoved((event) => {
      if (!cancelled) scheduleMovedPersist(event.payload);
    }).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlistenMoved = fn;
    }).catch(() => {});

    positionSyncTimer = setInterval(() => {
      if (cancelled) return;
      void (async () => {
        try {
          const pos = await win.outerPosition();
          const scale = await win.scaleFactor();
          const logicalX = pos.x / scale;
          const logicalY = pos.y / scale;
          const current = useCCChanStore.getState().settings;
          if (
            current.windowX !== null &&
            current.windowY !== null &&
            Math.abs(current.windowX - logicalX) < POSITION_SYNC_EPSILON &&
            Math.abs(current.windowY - logicalY) < POSITION_SYNC_EPSILON
          ) {
            return;
          }
          await persistDragPosition(pos);
        } catch {
          /* best-effort native position sync */
        }
      })();
    }, POSITION_SYNC_INTERVAL_MS);

    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      cancelled = true;
      clearDragTimers();
      if (positionSyncTimer) clearInterval(positionSyncTimer);
      unlistenMoved?.();
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [setPosition]);

  async function openChat() {
    setChatMounted(true);
    if (expanded) return;
    await invoke("resize_ccchan_for_chat", { expanded: true });
    setExpanded(true);
  }

  async function closeChat() {
    await invoke("resize_ccchan_for_chat", { expanded: false });
    setExpanded(false);
  }

  function handleMouseDown(event: MouseEvent<HTMLButtonElement>) {
    if (event.button !== 0) return;
    if (dragPersistTimerRef.current) clearTimeout(dragPersistTimerRef.current);
    if (dragFinishTimerRef.current) clearTimeout(dragFinishTimerRef.current);
    dragPersistTimerRef.current = null;
    dragFinishTimerRef.current = null;
    lastPersistedDragPhysicalRef.current = null;
    dragStartedAtRef.current = Date.now();
    setDragging(true);
    getCurrentWindow().startDragging().catch(() => {});
  }

  async function handleContextMenu(event: MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    const openedForMenu = !expanded;
    if (openedForMenu) {
      await invoke("resize_ccchan_for_menu", { expanded: true }).catch(() => {});
    }
    setMenuOwnsResize(openedForMenu);
    // Place menu just below mascot (mascot occupies top-left 120x120 of the
    // expanded menu window). Keep within the expanded window bounds.
    setMenuPosition({ x: Math.min(event.clientX, 140), y: Math.min(event.clientY + 4, 130) });
  }

  function closeMenu() {
    setMenuPosition(null);
    if (menuOwnsResize) {
      setMenuOwnsResize(false);
      void invoke("resize_ccchan_for_menu", { expanded: false }).catch(() => {});
    }
  }

  async function hideWindow() {
    await invoke("hide_ccchan");
    setWindowVisible(false);
  }

  async function exitWindow() {
    if (chatSessionId) {
      await invoke("stop_ccchan_chat", { sessionId: chatSessionId }).catch(() => {});
      setChatSessionId(null);
    }
    await hideWindow();
  }

  function switchRole(roleId: string) {
    setActiveRoleId(roleId);
    const nextSettings = useCCChanStore.getState().settings;
    useCCChanStore.getState().saveSettings(nextSettings).catch((error) => {
      console.warn("ccchan role switch save failed:", error);
    });
  }

  if (!selectedPet) return null;

  return (
    <div
      className="relative select-none"
      style={{
        width: expanded ? CHAT_EXPANDED_W : menuPosition ? MENU_W : PET_SIZE,
        height: expanded ? CHAT_EXPANDED_H : menuPosition ? MENU_H : PET_SIZE,
        background: "transparent",
      }}
      onClick={() => {
        if (suppressNextClickRef.current) {
          suppressNextClickRef.current = false;
        }
      }}
    >
      <div className="absolute left-0 top-0" style={{ width: PET_SIZE, height: PET_SIZE }}>
        <div className="pointer-events-auto absolute left-1/2 top-1 z-10 -translate-x-1/2">
          <SessionDots scopeMode={settings.scopeMode} activeSessionId={activeMainSessionId} />
        </div>
        {bubbleText && (
          <div
            className="absolute left-[112px] top-1 max-w-[280px] rounded-md border px-2 py-1 text-[12px] shadow-lg"
            style={{
              background: "var(--app-content)",
              borderColor: "var(--app-border)",
              color: "var(--app-text-primary)",
            }}
          >
            {bubbleText}
          </div>
        )}
        <SpritePet
          pet={selectedPet}
          state={petState}
          size={PET_SIZE}
          title="打开 cc酱 chat"
          onMouseDown={handleMouseDown}
          onContextMenu={(event) => void handleContextMenu(event)}
          onClick={(event) => {
            event.stopPropagation();
            if (suppressNextClickRef.current) {
              suppressNextClickRef.current = false;
              return;
            }
            void openChat().catch(() => {});
          }}
        />
      </div>

      <div
        className="absolute left-3 transition-all duration-200"
        style={{
          top: PET_SIZE + 12,
          opacity: expanded ? 1 : 0,
          transform: expanded ? "translateY(0)" : "translateY(-6px)",
          pointerEvents: expanded ? "auto" : "none",
        }}
      >
        {chatMounted && (
          <ChatPanel
            settings={settings}
            sessionId={chatSessionId}
            visible={expanded}
            autoStart={chatMounted}
            onSessionIdChange={setChatSessionId}
            onClose={() => void closeChat().catch(() => {})}
          />
        )}
      </div>

      {menuPosition && (
        <ContextMenu
          position={menuPosition}
          onHide={() => void hideWindow().catch(() => {})}
          onSwitchPet={switchPet}
          roles={settings.roles}
          activeRoleId={settings.activeRoleId}
          onSwitchRole={switchRole}
          onOpenSettings={() => void emitTo("main", "ccchan:open-settings")}
          onExit={() => void exitWindow().catch(() => {})}
          onClose={closeMenu}
        />
      )}

      <Toaster position="top-center" richColors />
    </div>
  );
}
