import { describe, expect, it, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  DEFAULT_CCCHAN_SETTINGS,
  getCCChanRuntimeValidationError,
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
      customPetDirs: [
        " /home/dev/.codex/pets ",
        "",
        "/home/dev/.codex/pets",
        "\\\\wsl.localhost\\Ubuntu-24.04\\home\\dev\\.codex\\pets",
      ],
    });

    expect(settings.activeRoleId).toBe("default");
    expect(settings.scopeMode).toBe("global");
    expect(settings.petSources).toEqual({ builtin: true, user: true, codexHome: true });
    expect(settings.customPetDirs).toEqual([
      "/home/dev/.codex/pets",
      "\\\\wsl.localhost\\Ubuntu-24.04\\home\\dev\\.codex\\pets",
    ]);
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
      runtimeKind: "wsl",
      wslRemotePath: "/home/dev/repo",
      wslDistro: "Ubuntu",
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
    expect(settings.roles.find((role) => role.id === "reviewer")?.runtimeKind).toBe("wsl");
  });

  it("validates WSL role runtime paths before settings are saved", () => {
    const wslRole: CCChanRolePreset = {
      id: "codex-wsl",
      name: "Codex WSL",
      aiEngine: "codex",
      petId: "doro.codex-pet",
      systemPrompt: "Use Codex in WSL.",
      runtimeKind: "wsl",
      wslRemotePath: null,
      wslDistro: null,
    };
    const settings = normalizeCCChanSettings({
      ...DEFAULT_CCCHAN_SETTINGS,
      activeRoleId: wslRole.id,
      roles: [...DEFAULT_CCCHAN_SETTINGS.roles, wslRole],
    });

    expect(getCCChanRuntimeValidationError(settings)).toContain("需要填写 WSL 远端路径");

    const windowsPath = normalizeCCChanSettings({
      ...settings,
      roles: settings.roles.map((role) =>
        role.id === wslRole.id ? { ...role, wslRemotePath: "D:\\my-project\\cc-pane" } : role,
      ),
    });
    expect(getCCChanRuntimeValidationError(windowsPath)).toContain("必须以 / 开头");

    const uncPath = normalizeCCChanSettings({
      ...settings,
      roles: settings.roles.map((role) =>
        role.id === wslRole.id
          ? { ...role, wslRemotePath: "\\\\wsl.localhost\\Ubuntu-24.04\\home\\me\\repo" }
          : role,
      ),
    });
    expect(getCCChanRuntimeValidationError(uncPath)).toContain("必须以 / 开头");

    const relativePath = normalizeCCChanSettings({
      ...settings,
      roles: settings.roles.map((role) =>
        role.id === wslRole.id ? { ...role, wslRemotePath: "workspace/repo" } : role,
      ),
    });
    expect(getCCChanRuntimeValidationError(relativePath)).toContain("必须以 / 开头");

    const homePath = normalizeCCChanSettings({
      ...settings,
      roles: settings.roles.map((role) =>
        role.id === wslRole.id ? { ...role, wslRemotePath: "~/repo" } : role,
      ),
    });
    expect(getCCChanRuntimeValidationError(homePath)).toContain("必须以 / 开头");

    const linuxPath = normalizeCCChanSettings({
      ...settings,
      roles: settings.roles.map((role) =>
        role.id === wslRole.id ? { ...role, wslRemotePath: "/mnt/d/my-project/cc-pane" } : role,
      ),
    });
    expect(getCCChanRuntimeValidationError(linuxPath)).toBeNull();
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

  it("falls back to the first available pet when the saved pet is missing", async () => {
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") {
        return Promise.resolve({
          ...DEFAULT_CCCHAN_SETTINGS,
          defaultPetId: "missing-pet",
          roles: [{ ...DEFAULT_CCCHAN_SETTINGS.roles[0], petId: "missing-pet" }],
        });
      }
      if (cmd === "get_ccchan_pets") {
        return Promise.resolve([homiePet, doroPet]);
      }
      return Promise.reject(new Error(`Unhandled command: ${cmd}`));
    });

    await useCCChanStore.getState().load();

    const settings = useCCChanStore.getState().settings;
    expect(settings.defaultPetId).toBe("homie");
    expect(settings.roles[0].petId).toBe("homie");
  });

  it("switches from the visible first pet when the saved pet is missing", () => {
    useCCChanStore.setState({
      settings: normalizeCCChanSettings({
        ...DEFAULT_CCCHAN_SETTINGS,
        defaultPetId: "missing-pet",
        roles: [{ ...DEFAULT_CCCHAN_SETTINGS.roles[0], petId: "missing-pet" }],
      }),
      pets: [homiePet, doroPet],
    });

    useCCChanStore.getState().switchPet();

    const settings = useCCChanStore.getState().settings;
    expect(settings.defaultPetId).toBe("homie");
    expect(settings.roles[0].petId).toBe("homie");
  });

  it("runs a follow-up load when an update arrives during loading", async () => {
    let resolveFirstSettings: (settings: CCChanSettings) => void = () => {};
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") {
        if (vi.mocked(invoke).mock.calls.filter(([name]) => name === "get_ccchan_settings").length === 1) {
          return new Promise((resolve) => {
            resolveFirstSettings = resolve as (settings: CCChanSettings) => void;
          });
        }
        return Promise.resolve({
          ...DEFAULT_CCCHAN_SETTINGS,
          roles: [
            {
              ...DEFAULT_CCCHAN_SETTINGS.roles[0],
              aiEngine: "codex",
            },
          ],
        });
      }
      if (cmd === "get_ccchan_pets") {
        return Promise.resolve([doroPet]);
      }
      return Promise.reject(new Error(`Unhandled command: ${cmd}`));
    });

    const firstLoad = useCCChanStore.getState().load();
    const secondLoad = useCCChanStore.getState().load();
    resolveFirstSettings(DEFAULT_CCCHAN_SETTINGS);
    await Promise.all([firstLoad, secondLoad]);

    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_ccchan_settings")).toHaveLength(2);
    expect(useCCChanStore.getState().settings.aiEngine).toBe("codex");
  });
});
