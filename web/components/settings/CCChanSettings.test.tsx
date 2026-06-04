import "@/i18n";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import CCChanSettings from "./CCChanSettings";
import { DEFAULT_CCCHAN_SETTINGS, useCCChanStore } from "@/stores/useCCChanStore";
import { useWorkspacesStore } from "@/stores/useWorkspacesStore";
import type { CCChanSettings as CCChanSettingsValue, PetMeta } from "@/ccchan/types";

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

const customSourcePet: PetMeta = {
  id: "external-pet",
  displayName: "External Pet",
  description: "Custom source pet",
  spritesheetUrl: "asset://external-pet",
  source: "custom",
  atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
  animations: { idle: { row: 0, frames: 1, fps: 1 } },
};

const codexHomePet: PetMeta = {
  id: "local-home-pet",
  displayName: "Local Home Pet",
  description: "Local home pet",
  spritesheetUrl: "asset://local-home-pet",
  source: "codexHome",
  atlas: { cellW: 192, cellH: 208, cols: 8, rows: 9 },
  animations: { idle: { row: 0, frames: 1, fps: 1 } },
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
    sourcePath: "/home/dev/pets/doro.zip",
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
    useWorkspacesStore.setState({
      workspaces: [],
      expandedWorkspaceId: null,
      expandedProjectId: null,
      loading: false,
    });
    vi.mocked(invoke).mockImplementation((cmd, args) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(DEFAULT_CCCHAN_SETTINGS);
      if (cmd === "get_ccchan_pets") return Promise.resolve([doroPet]);
      if (cmd === "preview_ccchan_pet_from_path") {
        return Promise.resolve(installPreview());
      }
      if (cmd === "install_ccchan_pet_from_preview") {
        expect(args).toEqual({ stagingId: "stage-1" });
        return Promise.resolve(undefined);
      }
      if (cmd === "cancel_ccchan_pet_preview") {
        expect(args).toEqual({ stagingId: "stage-1" });
        return Promise.resolve(undefined);
      }
      if (cmd === "install_ccchan_pet_from_path") return Promise.resolve(undefined);
      if (cmd === "install_ccchan_pet_from_source") return Promise.resolve(undefined);
      if (cmd === "get_ccchan_custom_pet_dir_statuses") return Promise.resolve([]);
      if (cmd === "delete_ccchan_user_pet") return Promise.resolve(undefined);
      if (cmd === "save_ccchan_settings") return Promise.resolve(undefined);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    vi.mocked(open).mockResolvedValue(null);
    vi.mocked(confirm).mockResolvedValue(true);
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

  it("shows neutral pet source labels without resource-link affordances", async () => {
    useCCChanStore.setState({
      settings: DEFAULT_CCCHAN_SETTINGS,
      pets: [doroPet, userPet, customSourcePet, codexHomePet],
      loaded: true,
    });
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(DEFAULT_CCCHAN_SETTINGS);
      if (cmd === "get_ccchan_pets") return Promise.resolve([doroPet, userPet, customSourcePet, codexHomePet]);
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    renderSettings();
    await waitForInitialLoad();

    expect(screen.getByText("宠物来源")).toBeInTheDocument();
    expect(screen.getByText("默认本地目录")).toBeInTheDocument();
    const rolePetSelect = screen.getAllByRole("combobox").find((select) =>
      within(select).queryByRole("option", { name: "Local Home Pet · 本地目录" })
    );
    expect(rolePetSelect).toBeDefined();
    const rolePetOptions = within(rolePetSelect as HTMLElement)
      .getAllByRole("option")
      .map((option) => option.textContent ?? "");
    expect(rolePetOptions).toContain("Local Home Pet · 本地目录");
    expect(rolePetOptions).toContain("External Pet · 本地目录");
    expect(rolePetOptions).toContain("Custom Pet · 已安装");
    expect(rolePetOptions.join("\n")).not.toMatch(/codexHome|custom/);
    expect(screen.queryByRole("button", { name: "链接安装" })).not.toBeInTheDocument();
    expect(screen.getByText("额外本地来源")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "添加来源" })).toBeInTheDocument();
    expect(screen.getByText(/文件夹和 zip 安装前会先预览并确认/)).toBeInTheDocument();
    expect(screen.queryByText(/HTTPS|桌宠安装链接/)).not.toBeInTheDocument();
    expect(document.body.textContent ?? "").not.toMatch(/codex-pet|codex-pets|awesome-codex|链接安装|HTTPS/i);
    expect(invoke).not.toHaveBeenCalledWith("preview_ccchan_pet_from_url", expect.anything());
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

  it("warns when the active WSL role uses a Windows path", async () => {
    const wslRole = {
      ...DEFAULT_CCCHAN_SETTINGS.roles[0],
      id: "codex-wsl",
      name: "Codex WSL 助手",
      aiEngine: "codex" as const,
      runtimeKind: "wsl" as const,
      wslRemotePath: "D:\\my-project\\cc-pane",
    };
    const settings = {
      ...DEFAULT_CCCHAN_SETTINGS,
      activeRoleId: wslRole.id,
      roles: [...DEFAULT_CCCHAN_SETTINGS.roles, wslRole],
    };
    renderSettings(settings);
    await waitForInitialLoad();

    expect(screen.getByText(/WSL 远端路径必须以 \/ 开头/)).toBeInTheDocument();
    expect(screen.getByText(/D:\\my-project\\cc-pane/)).toBeInTheDocument();
  });

  it("fills the active WSL role path from the selected workspace project", async () => {
    const wslRole = {
      ...DEFAULT_CCCHAN_SETTINGS.roles[0],
      id: "claude-wsl",
      name: "Claude WSL 助手",
      aiEngine: "claude" as const,
      runtimeKind: "wsl" as const,
      wslRemotePath: null,
      wslDistro: null,
    };
    const settings = {
      ...DEFAULT_CCCHAN_SETTINGS,
      activeRoleId: wslRole.id,
      roles: [...DEFAULT_CCCHAN_SETTINGS.roles, wslRole],
    };
    useWorkspacesStore.setState({
      expandedWorkspaceId: "ws-1",
      expandedProjectId: "project-1",
      workspaces: [{
        id: "ws-1",
        name: "ccpanes",
        createdAt: "2026-06-04T00:00:00Z",
        path: "D:\\my-project",
        wsl: { distro: "Ubuntu-24.04", remotePath: "/mnt/d/my-project" },
        projects: [{
          id: "project-1",
          path: "D:\\my-project\\cc-pane",
        }],
      }],
    });
    const { onChange } = renderSettings(settings);
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "使用当前项目路径" }));

    const next = onChange.mock.calls[onChange.mock.calls.length - 1]?.[0] as CCChanSettingsValue;
    const nextRole = next.roles.find((role) => role.id === wslRole.id);
    expect(nextRole?.wslRemotePath).toBe("/mnt/d/my-project/cc-pane");
    expect(nextRole?.wslDistro).toBe("Ubuntu-24.04");
  });

  it("uses an already-Linux selected project path for the active WSL role", async () => {
    const wslRole = {
      ...DEFAULT_CCCHAN_SETTINGS.roles[0],
      id: "codex-wsl",
      name: "Codex WSL 助手",
      aiEngine: "codex" as const,
      runtimeKind: "wsl" as const,
      wslRemotePath: null,
      wslDistro: null,
    };
    const settings = {
      ...DEFAULT_CCCHAN_SETTINGS,
      activeRoleId: wslRole.id,
      roles: [...DEFAULT_CCCHAN_SETTINGS.roles, wslRole],
    };
    useWorkspacesStore.setState({
      expandedWorkspaceId: "ws-1",
      expandedProjectId: "project-1",
      workspaces: [{
        id: "ws-1",
        name: "ccpanes",
        createdAt: "2026-06-04T00:00:00Z",
        projects: [{
          id: "project-1",
          path: "/mnt/d/my-project/cc-pane",
        }],
      }],
    });
    const { onChange } = renderSettings(settings);
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "使用当前项目路径" }));

    const next = onChange.mock.calls[onChange.mock.calls.length - 1]?.[0] as CCChanSettingsValue;
    expect(next.roles.find((role) => role.id === wslRole.id)?.wslRemotePath).toBe("/mnt/d/my-project/cc-pane");
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
      expect(confirm).toHaveBeenCalledWith(
        "安装桌宠 \"Doro\" (doro.codex-pet)？",
        expect.objectContaining({ okLabel: "安装" }),
      );
      expect(invoke).toHaveBeenCalledWith("install_ccchan_pet_from_path", { path: "/home/dev/pets/doro" });
    });
  });

  it("adds a selected directory as a read-only custom pet source", async () => {
    vi.mocked(open).mockResolvedValue("/home/dev/pets");
    const settings = {
      ...DEFAULT_CCCHAN_SETTINGS,
      customPetDirs: ["/mnt/d/shared-pets"],
    };
    const { onChange } = renderSettings(settings);
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "添加来源" }));

    await waitFor(() => {
      expect(open).toHaveBeenCalledWith(expect.objectContaining({
        directory: true,
        multiple: false,
        title: "选择 cc酱本地来源",
      }));
    });
    const next = onChange.mock.calls[onChange.mock.calls.length - 1]?.[0] as CCChanSettingsValue;
    expect(next.customPetDirs).toEqual([
      "/mnt/d/shared-pets",
      "/home/dev/pets",
    ]);
  });

  it("does not add a duplicate custom pet source directory", async () => {
    vi.mocked(open).mockResolvedValue(" /mnt/d/shared-pets ");
    const settings = {
      ...DEFAULT_CCCHAN_SETTINGS,
      customPetDirs: ["/mnt/d/shared-pets"],
    };
    const { onChange } = renderSettings(settings);
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "添加来源" }));

    await waitFor(() => {
      expect(toast.success).toHaveBeenCalledWith("该本地来源已存在");
    });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("checks custom pet source directory statuses", async () => {
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(DEFAULT_CCCHAN_SETTINGS);
      if (cmd === "get_ccchan_pets") return Promise.resolve([doroPet]);
      if (cmd === "get_ccchan_custom_pet_dir_statuses") {
        return Promise.resolve([
          { path: "/mnt/d/shared-pets", status: "ready", petCount: 2, message: "发现 2 个可用宠物" },
          { path: "/mnt/d/missing", status: "missing", petCount: 0, message: "目录不存在" },
        ]);
      }
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    renderSettings({
      ...DEFAULT_CCCHAN_SETTINGS,
      customPetDirs: ["/mnt/d/shared-pets", "/mnt/d/missing"],
    });
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "检查来源" }));

    expect(await screen.findByText("/mnt/d/shared-pets")).toBeInTheDocument();
    expect(screen.getByText(/ready · 2 · 发现 2 个可用宠物/)).toBeInTheDocument();
    expect(screen.getByText(/missing · 0 · 目录不存在/)).toBeInTheDocument();
  });

  it("installs a custom source pet into the user pet directory", async () => {
    useCCChanStore.setState({
      settings: DEFAULT_CCCHAN_SETTINGS,
      pets: [doroPet, customSourcePet],
      loaded: true,
    });
    vi.mocked(invoke).mockImplementation((cmd, args) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(DEFAULT_CCCHAN_SETTINGS);
      if (cmd === "get_ccchan_pets") return Promise.resolve([doroPet, customSourcePet]);
      if (cmd === "install_ccchan_pet_from_source") {
        expect(args).toEqual({ petId: customSourcePet.id, source: "custom" });
        return Promise.resolve(undefined);
      }
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    renderSettings();
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "安装到用户目录" }));

    await waitFor(() => {
      expect(confirm).toHaveBeenCalledWith(
        "把 \"External Pet\" (external-pet) 安装到用户宠物目录？",
        expect.objectContaining({ okLabel: "安装" }),
      );
      expect(invoke).toHaveBeenCalledWith("install_ccchan_pet_from_source", { petId: "external-pet", source: "custom" });
    });
  });

  it("installs a default local directory pet into the user pet directory", async () => {
    useCCChanStore.setState({
      settings: DEFAULT_CCCHAN_SETTINGS,
      pets: [doroPet, codexHomePet],
      loaded: true,
    });
    vi.mocked(invoke).mockImplementation((cmd, args) => {
      if (cmd === "get_ccchan_settings") return Promise.resolve(DEFAULT_CCCHAN_SETTINGS);
      if (cmd === "get_ccchan_pets") return Promise.resolve([doroPet, codexHomePet]);
      if (cmd === "install_ccchan_pet_from_source") {
        expect(args).toEqual({ petId: codexHomePet.id, source: "codexHome" });
        return Promise.resolve(undefined);
      }
      return Promise.reject(new Error(`Unhandled invoke command: ${cmd}`));
    });
    renderSettings();
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "安装到用户目录" }));

    await waitFor(() => {
      expect(confirm).toHaveBeenCalledWith(
        "把 \"Local Home Pet\" (local-home-pet) 安装到用户宠物目录？",
        expect.objectContaining({ okLabel: "安装" }),
      );
      expect(invoke).toHaveBeenCalledWith("install_ccchan_pet_from_source", {
        petId: "local-home-pet",
        source: "codexHome",
      });
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
      expect(confirm).toHaveBeenCalledWith(
        "删除用户安装的桌宠 \"Custom Pet\" (custom-pet)？",
        expect.objectContaining({ kind: "warning", okLabel: "删除" }),
      );
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

  it("does not install the selected pet when the confirmation is cancelled", async () => {
    vi.mocked(confirm).mockResolvedValue(false);
    vi.mocked(open).mockResolvedValue("/home/dev/pets/doro.zip");
    renderSettings();
    await waitForInitialLoad();

    await userEvent.click(screen.getByRole("button", { name: "zip 安装" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("preview_ccchan_pet_from_path", { path: "/home/dev/pets/doro.zip" });
      expect(confirm).toHaveBeenCalled();
    });
    expect(invoke).not.toHaveBeenCalledWith("install_ccchan_pet_from_preview", { stagingId: "stage-1" });
    expect(invoke).toHaveBeenCalledWith("cancel_ccchan_pet_preview", { stagingId: "stage-1" });
  });
});
