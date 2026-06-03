import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type { CCChanAiEngine, CCChanPetSources, CCChanRolePreset, CCChanScopeMode, CCChanSettings, PetMeta } from "@/ccchan/types";

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
  };
}

export const DEFAULT_CCCHAN_SETTINGS: CCChanSettings = {
  aiEngine: "claude",
  defaultPetId: "doro.codex-pet",
  activeRoleId: DEFAULT_ROLE_ID,
  roles: [defaultRole("claude", "doro.codex-pet")],
  scopeMode: "global",
  petSources: DEFAULT_PET_SOURCES,
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
  };
}

function normalizePetSources(sources: Partial<CCChanPetSources> | null | undefined): CCChanPetSources {
  return {
    builtin: sources?.builtin ?? DEFAULT_PET_SOURCES.builtin,
    user: sources?.user ?? DEFAULT_PET_SOURCES.user,
    codexHome: sources?.codexHome ?? DEFAULT_PET_SOURCES.codexHome,
  };
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
    windowX: settings?.windowX ?? null,
    windowY: settings?.windowY ?? null,
  };
}

function normalizePets(pets: PetMeta[] | null | undefined): PetMeta[] {
  return pets && pets.length > 0 ? pets : [FALLBACK_PET];
}

export const useCCChanStore = create<CCChanStoreState>((set, get) => ({
  settings: DEFAULT_CCCHAN_SETTINGS,
  pets: [FALLBACK_PET],
  expanded: false,
  chatSessionId: null,
  loading: false,
  loaded: false,

  load: async () => {
    if (get().loading) return;
    set({ loading: true });
    try {
      const [settings, pets] = await Promise.all([
        invoke<CCChanSettings>("get_ccchan_settings").catch(() => DEFAULT_CCCHAN_SETTINGS),
        invoke<PetMeta[]>("get_ccchan_pets").catch(() => [FALLBACK_PET]),
      ]);
      set({
        settings: normalizeCCChanSettings(settings),
        pets: normalizePets(pets),
        loaded: true,
      });
    } finally {
      set({ loading: false });
    }
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
    const currentIndex = Math.max(0, pets.findIndex((pet) => pet.id === settings.defaultPetId));
    const nextPet = pets[(currentIndex + 1) % pets.length];
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
