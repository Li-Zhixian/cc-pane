import "@/i18n";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import CCChanSettings from "./CCChanSettings";
import { DEFAULT_CCCHAN_SETTINGS, useCCChanStore } from "@/stores/useCCChanStore";
import type { AwesomeCodexPetEntry, CCChanSettings as CCChanSettingsValue, PetMeta } from "@/ccchan/types";

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
  },
}));

const doroPet: PetMeta = {
  id: "doro.codex-pet",
  displayName: "Doro",
  description: "Doro pet",
  spritesheetUrl: "asset://doro",
  source: "builtin",
  atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
  animations: { idle: { row: 0, frames: 1, fps: 1 } },
};

const homiePet: PetMeta = {
  id: "homie",
  displayName: "Homie",
  description: "Default pet",
  spritesheetUrl: "asset://homie",
  source: "builtin",
  atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
  animations: { idle: { row: 0, frames: 1, fps: 1 } },
};

const userPet: PetMeta = {
  id: "custom-pet",
  displayName: "Custom Pet",
  description: "User installed pet",
  spritesheetUrl: "asset://custom-pet",
  source: "user",
  atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
  animations: { idle: { row: 0, frames: 1, fps: 1 } },
};

const awesomePet: AwesomeCodexPetEntry = {
  slug: "firefly--lingxiaotian",
  name: "Firefly",
  author: "Lingxiaotian",
  authorHandle: "legeling",
  authorUrl: "https://github.com/legeling",
  primaryCategory: "Anime Characters",
  license: "CC BY-NC 4.0",
  description: "A soft white, mint, and gold palette pet.",
};

function renderSettings(
  value: CCChanSettingsValue = DEFAULT_CCCHAN_SETTINGS,
  onChange = vi.fn(),
) {
  render(<CCChanSettings value={value} onChange={onChange} />);
  return { onChange };
}

function installPreview(pet: PetMeta = doroPet) {
  return {
    stagingId: "stage-1",
    sourcePath: "https://example.invalid/pet.zip",
    pet: { ...pet, source: "user" as const },
  };
}

async function waitForInitialLoad() {
  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("get_ccchan_settings");
    expect(invoke).toHaveBeenCalledWith("get_ccchan_pets");
  });
}

describe("CCChanSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useCCChanStore.setState({
      settings: DEFAULT_CCCHAN_SETTINGS,
      pets: [doroPet],
      expanded: false,
      chatSessionId: null,
      loading: false,
      loaded: true,
    });
    vi.mocked(invoke).mockImplementation((cmd, args) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(DEFAULT_CCCHAN_SETTINGS);
      if (cmd === "get_ccchan_pets") return Promise.resolve([doroPet]);
      if (cmd === "preview_ccchan_pet_from_url") {
        return Promise.resolve(installPreview());
      }
      if (cmd === "preview_ccchan_pet_from_path") {
        return Promise.resolve(installPreview());
      }
      if (cmd === "list_ccchan_awesome_codex_pets") return Promise.resolve([awesomePet]);
      if (cmd === "preview_ccchan_awesome_codex_pet") {
        expect(args).toEqual({ slug: "firefly--lingxiaotian" });
        return Promise.resolve(installPreview({
          ...doroPet,
          id: "firefly--lingxiaotian",
          displayName: "Firefly",
        }));
      }
      if (cmd === "install_ccchan_pet_from_preview") {
        expect(args).toEqual({ stagingId: "stage-1" });
        return Promise.resolve(undefined);
      }
      if (cmd === "install_ccchan_pet_from_path") return Promise.resolve(undefined);
      if (cmd === "delete_ccchan_user_pet") return Promise.resolve(undefined);
      if (cmd === "save_ccchan_settings") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    vi.mocked(open).mockResolvedValue(null);
    vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.spyOn(window, "prompt").mockReturnValue(null);
  });

  it("adds a Codex WSL role from the quick templates and warns when remote path is missing", async () => {
    const { onChange } = renderSettings();
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "Codex WSL" }));

    const next = onChange.mock.calls[onChange.mock.calls.length - 1]?.[0] as CCChanSettingsValue;
    expect(next.activeRoleId).toMatch(/^codex-wsl-/);
    expect(next.roles.find((role) => role.id === next.activeRoleId)).toEqual(expect.objectContaining({
      aiEngine: "codex",
      runtimeKind: "wsl",
      wslRemotePath: null,
    }));

    renderSettings(next);
    await waitForInitialLoad();
    expect(screen.getByText(/WSL 角色需要填写.*远端路径/)).toBeInTheDocument();
  });

  it("updates WSL remote path for the active role", async () => {
    const wslRole = {
      ...DEFAULT_CCCHAN_SETTINGS.roles[0],
      id: "codex-wsl",
      name: "Codex WSL 助手",
      aiEngine: "codex" as const,
      runtimeKind: "wsl" as const,
      wslRemotePath: null,
    };
    const settings = {
      ...DEFAULT_CCCHAN_SETTINGS,
      activeRoleId: wslRole.id,
      roles: [...DEFAULT_CCCHAN_SETTINGS.roles, wslRole],
    };
    const { onChange } = renderSettings(settings);
    await waitForInitialLoad();

    fireEvent.change(screen.getByPlaceholderText("/mnt/d/my-project/cc-pane"), {
      target: { value: "/mnt/d/my-project/cc-pane" },
    });

    const next = onChange.mock.calls[onChange.mock.calls.length - 1]?.[0] as CCChanSettingsValue;
    expect(next.roles.find((role) => role.id === wslRole.id)?.wslRemotePath).toBe("/mnt/d/my-project/cc-pane");
  });

  it("loads, searches, and installs an Awesome Codex Pet catalog entry", async () => {
    renderSettings();
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "载入目录" }));
    expect(await screen.findByText("Firefly")).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("搜索名称、作者、分类或 license"), {
      target: { value: "ling" },
    });
    await userEvent.click(screen.getByRole("button", { name: "安装" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("preview_ccchan_awesome_codex_pet", { slug: "firefly--lingxiaotian" });
      expect(invoke).toHaveBeenCalledWith("install_ccchan_pet_from_preview", { stagingId: "stage-1" });
    });
  });

  it("previews and installs a pet folder directly from the selected path", async () => {
    vi.mocked(open).mockResolvedValue("/home/dev/pets/doro");
    renderSettings();
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "文件夹安装" }));

    await waitFor(() => {
      expect(open).toHaveBeenCalledWith(expect.objectContaining({
        directory: true,
        multiple: false,
        title: "选择 cc酱宠物文件夹",
      }));
      expect(invoke).toHaveBeenCalledWith("preview_ccchan_pet_from_path", { path: "/home/dev/pets/doro" });
      expect(invoke).toHaveBeenCalledWith("install_ccchan_pet_from_path", { path: "/home/dev/pets/doro" });
    });
  });

  it("previews a selected pet zip and installs it from staging", async () => {
    vi.mocked(open).mockResolvedValue("/home/dev/pets/doro.zip");
    renderSettings();
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "zip 安装" }));

    await waitFor(() => {
      expect(open).toHaveBeenCalledWith(expect.objectContaining({
        directory: false,
        multiple: false,
        title: "选择 cc酱宠物 zip",
        filters: [{ name: "Pet package", extensions: ["zip"] }],
      }));
      expect(invoke).toHaveBeenCalledWith("preview_ccchan_pet_from_path", { path: "/home/dev/pets/doro.zip" });
      expect(invoke).toHaveBeenCalledWith("install_ccchan_pet_from_preview", { stagingId: "stage-1" });
    });
  });

  it("previews and installs an HTTPS zip URL", async () => {
    vi.mocked(window.prompt).mockReturnValue("https://example.invalid/pet.zip");
    renderSettings();
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "URL 安装" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("preview_ccchan_pet_from_url", { url: "https://example.invalid/pet.zip" });
      expect(invoke).toHaveBeenCalledWith("install_ccchan_pet_from_preview", { stagingId: "stage-1" });
    });
  });

  it("previews and installs a codex pet deep link", async () => {
    const deepLink = "codex://pets/install?name=Doro&imageUrl=https%3A%2F%2Fexample.invalid%2Fdoro.webp";
    vi.mocked(window.prompt).mockReturnValue(deepLink);
    renderSettings();
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "URL 安装" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("preview_ccchan_pet_from_url", { url: deepLink });
      expect(invoke).toHaveBeenCalledWith("install_ccchan_pet_from_preview", { stagingId: "stage-1" });
    });
  });

  it("deletes the active user pet and saves fallback pet references", async () => {
    const settings = {
      ...DEFAULT_CCCHAN_SETTINGS,
      defaultPetId: userPet.id,
      roles: [{ ...DEFAULT_CCCHAN_SETTINGS.roles[0], petId: userPet.id }],
    };
    useCCChanStore.setState({
      settings,
      pets: [userPet, homiePet],
      loaded: true,
    });
    vi.mocked(invoke).mockImplementation((cmd, args) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(settings);
      if (cmd === "get_ccchan_pets") return Promise.resolve([userPet, homiePet]);
      if (cmd === "delete_ccchan_user_pet") {
        expect(args).toEqual({ petId: userPet.id });
        return Promise.resolve(undefined);
      }
      if (cmd === "save_ccchan_settings") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    const { onChange } = renderSettings(settings);
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "删除当前用户宠物" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_ccchan_user_pet", { petId: userPet.id });
      expect(invoke).toHaveBeenCalledWith(
        "save_ccchan_settings",
        expect.objectContaining({
          settings: expect.objectContaining({
            defaultPetId: homiePet.id,
            roles: expect.arrayContaining([expect.objectContaining({ petId: homiePet.id })]),
          }),
        }),
      );
    });
    const next = onChange.mock.calls[onChange.mock.calls.length - 1]?.[0] as CCChanSettingsValue;
    expect(next.defaultPetId).toBe(homiePet.id);
  });
});
