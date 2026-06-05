import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type {
  CCChanAiEngine,
  CCChanPetSources,
  CCChanRolePreset,
  CCChanRoleRuntimeKind,
  CCChanScopeMode,
  CCChanSettings,
  PetMeta,
} from "@/ccchan/types";

const fallbackSprite = `data:image/svg+xml;utf8,${encodeURIComponent(`
<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">
  <rect width="64" height="64" fill="none"/>
  <circle cx="32" cy="33" r="23" fill="#3b82f6"/>
  <circle cx="24" cy="29" r="4" fill="#ffffff"/>
  <circle cx="40" cy="29" r="4" fill="#ffffff"/>
  <path d="M24 43 Q32 49 40 43" stroke="#ffffff" stroke-width="4" fill="none" stroke-linecap="round"/>
</svg>
`)}`;

export const DEFAULT_CCCHAN_ROLE_PROMPT = [
  "You are ccchan, the desktop mascot assistant inside CC-Panes.",
  "Help the user operate CC-Panes, inspect sessions, explain stuck panes, and coordinate Claude Code or Codex work.",
  "Keep replies concise, practical, and in the user's language.",
].join("\n");

const DEFAULT_ROLE_ID = "default";
const DEFAULT_PET_SOURCES: CCChanPetSources = { builtin: true, user: true, codexHome: true };

function defaultRole(aiEngine: CCChanAiEngine, petId: string): CCChanRolePreset {
  return {
    id: DEFAULT_ROLE_ID,
    name: "默认助手",
    aiEngine,
    petId,
    systemPrompt: DEFAULT_CCCHAN_ROLE_PROMPT,
    runtimeKind: "local",
    wslRemotePath: null,
    wslDistro: null,
  };
}

export const DEFAULT_CCCHAN_SETTINGS: CCChanSettings = {
  aiEngine: "claude",
  defaultPetId: "doro.codex-pet",
  activeRoleId: DEFAULT_ROLE_ID,
  roles: [defaultRole("claude", "doro.codex-pet")],
  scopeMode: "global",
  petSources: DEFAULT_PET_SOURCES,
  customPetDirs: [],
  autoStart: true,
  soundEnabled: true,
  windowVisible: true,
  windowX: null,
  windowY: null,
};

export const FALLBACK_PET: PetMeta = {
  id: "homie",
  displayName: "cc酱",
  description: "Default CC-Panes mascot",
  spritesheetUrl: fallbackSprite,
  source: "builtin",
  atlas: { cellW: 64, cellH: 64, cols: 1, rows: 1 },
  animations: {
    idle: { row: 0, frames: 1, fps: 1 },
    working: { row: 0, frames: 1, fps: 1 },
    waiting: { row: 0, frames: 1, fps: 1 },
    happy: { row: 0, frames: 1, fps: 1 },
    sad: { row: 0, frames: 1, fps: 1 },
  },
};

export function getCCChanRuntimeValidationError(settings: CCChanSettings): string | null {
  const activeRole = settings.roles.find((role) => role.id === settings.activeRoleId);
  if (!activeRole || activeRole.runtimeKind !== "wsl") {
    return null;
  }
  const label = activeRole.name.trim() || activeRole.id;
  const remotePath = activeRole.wslRemotePath?.trim() ?? "";
  if (!remotePath) {
    return `cc酱 WSL 角色“${label}”需要填写 WSL 远端路径。`;
  }
  if (!remotePath.startsWith("/")) {
    return `cc酱 WSL 角色“${label}”的远端路径必须以 / 开头。`;
  }
  return null;
}

interface CCChanStoreState {
  settings: CCChanSettings;
  pets: PetMeta[];
  expanded: boolean;
  chatSessionId: string | null;
  loading: boolean;
  loaded: boolean;
  load: () => Promise<void>;
  saveSettings: (settings: CCChanSettings) => Promise<void>;
  setExpanded: (expanded: boolean) => void;
  setChatSessionId: (sessionId: string | null) => void;
  setWindowVisible: (visible: boolean) => void;
  setPosition: (x: number, y: number) => void;
  setDefaultPetId: (petId: string) => void;
  setActiveRoleId: (roleId: string) => void;
  switchPet: () => void;
}

function isAiEngine(value: unknown): value is CCChanAiEngine {
  return value === "claude" || value === "codex";
}

function isScopeMode(value: unknown): value is CCChanScopeMode {
  return value === "global" || value === "focusedWindow";
}

function isRoleRuntimeKind(value: unknown): value is CCChanRoleRuntimeKind {
  return value === "local" || value === "wsl";
}

function normalizeOptionalString(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : null;
}

function normalizeRole(role: Partial<CCChanRolePreset>, fallback: CCChanRolePreset): CCChanRolePreset {
  const id = typeof role.id === "string" && role.id.trim().length > 0 ? role.id.trim() : fallback.id;
  const name = typeof role.name === "string" && role.name.trim().length > 0 ? role.name.trim() : fallback.name;
  const petId = typeof role.petId === "string" && role.petId.trim().length > 0 ? role.petId.trim() : fallback.petId;
  const systemPrompt =
    typeof role.systemPrompt === "string" && role.systemPrompt.trim().length > 0
      ? role.systemPrompt
      : fallback.systemPrompt;

  return {
    id,
    name,
    aiEngine: isAiEngine(role.aiEngine) ? role.aiEngine : fallback.aiEngine,
    petId,
    systemPrompt,
    runtimeKind: isRoleRuntimeKind(role.runtimeKind) ? role.runtimeKind : fallback.runtimeKind,
    wslRemotePath: normalizeOptionalString(role.wslRemotePath),
    wslDistro: normalizeOptionalString(role.wslDistro),
  };
}

function normalizePetSources(sources: Partial<CCChanPetSources> | null | undefined): CCChanPetSources {
  return {
    builtin: sources?.builtin ?? DEFAULT_PET_SOURCES.builtin,
    user: sources?.user ?? DEFAULT_PET_SOURCES.user,
    codexHome: sources?.codexHome ?? DEFAULT_PET_SOURCES.codexHome,
  };
}

function normalizeCustomPetDirs(dirs: unknown): string[] {
  if (!Array.isArray(dirs)) return [];
  const normalized: string[] = [];
  for (const dir of dirs) {
    if (typeof dir !== "string") continue;
    const trimmed = dir.trim();
    if (!trimmed || normalized.includes(trimmed)) continue;
    normalized.push(trimmed);
  }
  return normalized;
}

export function normalizeCCChanSettings(settings: Partial<CCChanSettings> | null | undefined): CCChanSettings {
  const aiEngine = isAiEngine(settings?.aiEngine) ? settings.aiEngine : DEFAULT_CCCHAN_SETTINGS.aiEngine;
  const defaultPetId =
    typeof settings?.defaultPetId === "string" && settings.defaultPetId.trim().length > 0
      ? settings.defaultPetId.trim()
      : DEFAULT_CCCHAN_SETTINGS.defaultPetId;
  const legacyDefault = defaultRole(aiEngine, defaultPetId);
  const incomingRoles = Array.isArray(settings?.roles) ? settings.roles : [];
  const roles = incomingRoles.length > 0
    ? incomingRoles.map((role) => normalizeRole(role, legacyDefault))
    : [legacyDefault];
  if (!roles.some((role) => role.id === DEFAULT_ROLE_ID)) {
    roles.unshift(legacyDefault);
  }

  const requestedRoleId =
    typeof settings?.activeRoleId === "string" && settings.activeRoleId.trim().length > 0
      ? settings.activeRoleId.trim()
      : DEFAULT_ROLE_ID;
  const activeRole = roles.find((role) => role.id === requestedRoleId) ?? roles[0] ?? legacyDefault;

  return {
    ...DEFAULT_CCCHAN_SETTINGS,
    ...settings,
    aiEngine: activeRole.aiEngine,
    defaultPetId: activeRole.petId,
    activeRoleId: activeRole.id,
    roles,
    scopeMode: isScopeMode(settings?.scopeMode) ? settings.scopeMode : DEFAULT_CCCHAN_SETTINGS.scopeMode,
    petSources: normalizePetSources(settings?.petSources),
    customPetDirs: normalizeCustomPetDirs(settings?.customPetDirs),
    windowX: settings?.windowX ?? null,
    windowY: settings?.windowY ?? null,
  };
}

function normalizePets(pets: PetMeta[] | null | undefined): PetMeta[] {
  return pets && pets.length > 0 ? pets : [FALLBACK_PET];
}

function normalizeSettingsForPets(settings: CCChanSettings, pets: PetMeta[]): CCChanSettings {
  const fallbackPetId = pets[0]?.id ?? FALLBACK_PET.id;
  const availablePetIds = new Set(pets.map((pet) => pet.id));
  const roles = settings.roles.map((role) =>
    availablePetIds.has(role.petId) ? role : { ...role, petId: fallbackPetId },
  );
  return normalizeCCChanSettings({
    ...settings,
    defaultPetId: availablePetIds.has(settings.defaultPetId) ? settings.defaultPetId : fallbackPetId,
    roles,
  });
}

let loadPromise: Promise<void> | null = null;
let reloadRequested = false;

export const useCCChanStore = create<CCChanStoreState>((set, get) => ({
  settings: DEFAULT_CCCHAN_SETTINGS,
  pets: [FALLBACK_PET],
  expanded: false,
  chatSessionId: null,
  loading: false,
  loaded: false,

  load: async () => {
    if (loadPromise) {
      reloadRequested = true;
      await loadPromise;
      return;
    }
    loadPromise = (async () => {
      set({ loading: true });
      try {
        do {
          reloadRequested = false;
          const [settings, pets] = await Promise.all([
            invoke<CCChanSettings>("get_ccchan_settings").catch(() => DEFAULT_CCCHAN_SETTINGS),
            invoke<PetMeta[]>("get_ccchan_pets").catch(() => [FALLBACK_PET]),
          ]);
          const normalizedPets = normalizePets(pets);
          set({
            settings: normalizeSettingsForPets(normalizeCCChanSettings(settings), normalizedPets),
            pets: normalizedPets,
            loaded: true,
          });
        } while (reloadRequested);
      } finally {
        set({ loading: false });
        loadPromise = null;
      }
    })();
    await loadPromise;
  },

  saveSettings: async (settings) => {
    const normalized = normalizeCCChanSettings(settings);
    await invoke("save_ccchan_settings", { settings: normalized });
    set({ settings: normalized });
  },

  setExpanded: (expanded) => set({ expanded }),
  setChatSessionId: (sessionId) => set({ chatSessionId: sessionId }),
  setWindowVisible: (visible) => {
    set((state) => ({
      settings: { ...state.settings, windowVisible: visible },
    }));
  },
  setPosition: (x, y) => {
    set((state) => ({
      settings: { ...state.settings, windowX: x, windowY: y },
    }));
  },
  setDefaultPetId: (petId) => {
    set((state) => ({
      settings: normalizeCCChanSettings({
        ...state.settings,
        defaultPetId: petId,
        roles: state.settings.roles.map((role) =>
          role.id === state.settings.activeRoleId ? { ...role, petId } : role,
        ),
      }),
    }));
  },
  setActiveRoleId: (roleId) => {
    set((state) => {
      const role = state.settings.roles.find((item) => item.id === roleId);
      if (!role) return state;
      return {
        settings: normalizeCCChanSettings({
          ...state.settings,
          activeRoleId: role.id,
          aiEngine: role.aiEngine,
          defaultPetId: role.petId,
        }),
      };
    });
  },
  switchPet: () => {
    const { pets, settings } = get();
    if (pets.length === 0) return;
    const currentIndex = pets.findIndex((pet) => pet.id === settings.defaultPetId);
    const safeCurrentIndex = currentIndex >= 0 ? currentIndex : pets.length - 1;
    const nextPet = pets[(safeCurrentIndex + 1) % pets.length];
    set((state) => ({
      settings: normalizeCCChanSettings({
        ...state.settings,
        defaultPetId: nextPet.id,
        roles: state.settings.roles.map((role) =>
          role.id === state.settings.activeRoleId ? { ...role, petId: nextPet.id } : role,
        ),
      }),
    }));
  },
}));
