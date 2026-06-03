import { describe, expect, it, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  DEFAULT_CCCHAN_SETTINGS,
  normalizeCCChanSettings,
  useCCChanStore,
} from "./useCCChanStore";
import type { CCChanSettings, CCChanRolePreset, PetMeta } from "@/ccchan/types";

const doroPet: PetMeta = {
  id: "doro.codex-pet",
  displayName: "Doro",
  description: "Doro pet",
  spritesheetUrl: "asset://doro",
  source: "user",
  atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
  animations: { idle: { row: 0, frames: 1, fps: 1 } },
};

const homiePet: PetMeta = {
  id: "homie",
  displayName: "Homie",
  description: "Homie pet",
  spritesheetUrl: "asset://homie",
  source: "builtin",
  atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
  animations: { idle: { row: 0, frames: 1, fps: 1 } },
};

describe("useCCChanStore", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useCCChanStore.setState({
      settings: DEFAULT_CCCHAN_SETTINGS,
      pets: [],
      expanded: false,
      chatSessionId: null,
      loading: false,
      loaded: false,
    });
  });

  it("normalizes legacy settings into a default role preset", () => {
    const settings = normalizeCCChanSettings({
      aiEngine: "codex",
      defaultPetId: "doro.codex-pet",
      autoStart: true,
      soundEnabled: false,
      windowVisible: true,
      windowX: 12,
      windowY: 34,
    });

    expect(settings.activeRoleId).toBe("default");
    expect(settings.scopeMode).toBe("global");
    expect(settings.petSources).toEqual({ builtin: true, user: true, codexHome: true });
    expect(settings.roles).toEqual([
      expect.objectContaining({
        id: "default",
        name: "默认助手",
        aiEngine: "codex",
        petId: "doro.codex-pet",
      }),
    ]);
    expect(settings.roles[0].systemPrompt).toContain("CC-Panes");
  });

  it("switches active role and synchronizes legacy pet and engine fields", () => {
    const reviewer: CCChanRolePreset = {
      id: "reviewer",
      name: "审查员",
      aiEngine: "codex",
      petId: "doro.codex-pet",
      systemPrompt: "Review the current work.",
    };
    useCCChanStore.setState({
      settings: normalizeCCChanSettings({
        ...DEFAULT_CCCHAN_SETTINGS,
        roles: [...DEFAULT_CCCHAN_SETTINGS.roles, reviewer],
      } as Partial<CCChanSettings>),
      pets: [homiePet, doroPet],
    });

    useCCChanStore.getState().setActiveRoleId("reviewer");

    const settings = useCCChanStore.getState().settings;
    expect(settings.activeRoleId).toBe("reviewer");
    expect(settings.defaultPetId).toBe("doro.codex-pet");
    expect(settings.aiEngine).toBe("codex");
  });

  it("loads settings and pets through Tauri commands", async () => {
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") {
        return Promise.resolve({
          aiEngine: "codex",
          defaultPetId: "doro.codex-pet",
          autoStart: true,
          soundEnabled: true,
          windowVisible: true,
          windowX: null,
          windowY: null,
        });
      }
      if (cmd === "get_ccchan_pets") {
        return Promise.resolve([doroPet]);
      }
      return Promise.reject(new Error(`Unhandled command: ${cmd}`));
    });

    await useCCChanStore.getState().load();

    expect(useCCChanStore.getState().pets).toEqual([doroPet]);
    expect(useCCChanStore.getState().settings.activeRoleId).toBe("default");
  });
});
