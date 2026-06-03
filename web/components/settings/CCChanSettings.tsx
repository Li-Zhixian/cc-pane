import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import { Bot, Download, ExternalLink, FolderPlus, MapPin, Music, Power, Sparkles, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { DEFAULT_CCCHAN_ROLE_PROMPT, FALLBACK_PET, normalizeCCChanSettings, useCCChanStore } from "@/stores/useCCChanStore";
import type { CCChanPetInstallPreview, CCChanRolePreset, CCChanSettings as CCChanSettingsValue } from "@/ccchan/types";

interface CCChanSettingsProps {
  value: CCChanSettingsValue;
  onChange: (value: CCChanSettingsValue) => void;
}

const ENGINE_OPTIONS = [
  { value: "claude", label: "Claude" },
  { value: "codex", label: "Codex" },
] as const;

const PET_RESOURCE_LINKS = [
  { label: "Codex Pets", url: "https://codex-pets.net/#/?sort=popular" },
  { label: "awesome-codex-pet", url: "https://github.com/legeling/awesome-codex-pet" },
  { label: "Codex 官方说明", url: "https://developers.openai.com/codex/app/settings#codex-pets" },
] as const;

export default function CCChanSettings({ value, onChange }: CCChanSettingsProps) {
  const pets = useCCChanStore((state) => state.pets);
  const load = useCCChanStore((state) => state.load);
  const petOptions = pets.length > 0 ? pets : [FALLBACK_PET];
  const activeRole = value.roles.find((role) => role.id === value.activeRoleId) ?? value.roles[0];
  const userPets = petOptions.filter((pet) => pet.source === "user");

  useEffect(() => {
    void load();
  }, [load]);

  function update<K extends keyof CCChanSettingsValue>(key: K, next: CCChanSettingsValue[K]) {
    onChange(normalizeCCChanSettings({ ...value, [key]: next }));
  }

  function updateRole(roleId: string, patch: Partial<CCChanRolePreset>) {
    const roles = value.roles.map((role) => role.id === roleId ? { ...role, ...patch } : role);
    onChange(normalizeCCChanSettings({ ...value, roles }));
  }

  function setActiveRole(roleId: string) {
    const role = value.roles.find((item) => item.id === roleId);
    if (!role) return;
    onChange(normalizeCCChanSettings({
      ...value,
      activeRoleId: role.id,
      aiEngine: role.aiEngine,
      defaultPetId: role.petId,
    }));
  }

  function addRole() {
    const id = `role-${Date.now().toString(36)}`;
    const role: CCChanRolePreset = {
      id,
      name: "新角色",
      aiEngine: value.aiEngine,
      petId: value.defaultPetId,
      systemPrompt: DEFAULT_CCCHAN_ROLE_PROMPT,
      runtimeKind: "local",
      wslRemotePath: null,
      wslDistro: null,
    };
    onChange(normalizeCCChanSettings({
      ...value,
      activeRoleId: id,
      roles: [...value.roles, role],
    }));
  }

  function removeActiveRole() {
    if (!activeRole || activeRole.id === "default") return;
    onChange(normalizeCCChanSettings({
      ...value,
      activeRoleId: "default",
      roles: value.roles.filter((role) => role.id !== activeRole.id),
    }));
  }

  async function installFromPath(directory: boolean) {
    const selected = await open({
      directory,
      multiple: false,
      title: directory ? "选择 cc酱宠物文件夹" : "选择 cc酱宠物 zip",
      filters: directory ? undefined : [{ name: "Pet package", extensions: ["zip"] }],
    });
    if (typeof selected !== "string") return;
    try {
      if (directory) {
        await invoke("install_ccchan_pet_from_path", { path: selected });
      } else {
        const preview = await invoke<CCChanPetInstallPreview>("preview_ccchan_pet_from_path", { path: selected });
        const confirmed = window.confirm(`安装桌宠 "${preview.pet.displayName}" (${preview.pet.id})？`);
        if (!confirmed) return;
        await invoke("install_ccchan_pet_from_preview", { stagingId: preview.stagingId });
      }
      await load();
      toast.success("桌宠已安装");
    } catch (error) {
      toast.error(`安装失败: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  async function installFromUrl() {
    const url = window.prompt("输入 HTTPS 桌宠 zip URL");
    if (!url) return;
    try {
      const preview = await invoke<CCChanPetInstallPreview>("preview_ccchan_pet_from_url", { url });
      const confirmed = window.confirm(`安装桌宠 "${preview.pet.displayName}" (${preview.pet.id})？`);
      if (!confirmed) return;
      await invoke("install_ccchan_pet_from_preview", { stagingId: preview.stagingId });
      await load();
      toast.success("桌宠已安装");
    } catch (error) {
      toast.error(`安装失败: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  function replaceDeletedPetReferences(petId: string): CCChanSettingsValue {
    const fallbackPetId = petOptions.find((pet) => pet.id !== petId)?.id ?? FALLBACK_PET.id;
    const roles = value.roles.map((role) =>
      role.petId === petId ? { ...role, petId: fallbackPetId } : role,
    );
    return normalizeCCChanSettings({
      ...value,
      defaultPetId: value.defaultPetId === petId ? fallbackPetId : value.defaultPetId,
      roles,
    });
  }

  async function deleteUserPet(petId: string) {
    const pet = petOptions.find((item) => item.id === petId);
    if (!pet || pet.source !== "user") return;
    const confirmed = window.confirm(`删除用户安装的桌宠 "${pet.displayName}" (${pet.id})？`);
    if (!confirmed) return;
    try {
      await invoke("delete_ccchan_user_pet", { petId: pet.id });
      const nextSettings = replaceDeletedPetReferences(pet.id);
      await useCCChanStore.getState().saveSettings(nextSettings);
      onChange(nextSettings);
      await load();
      toast.success("桌宠已删除");
    } catch (error) {
      toast.error(`删除失败: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  async function openPetResource(url: string) {
    try {
      await openUrl(url);
    } catch (error) {
      toast.error(`打开链接失败: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  const selectStyle = {
    border: "1px solid var(--app-border)",
    background: "var(--app-content)",
    color: "var(--app-text-primary)",
  };

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h3 className="mb-1 flex items-center gap-2 text-[15px] font-semibold" style={{ color: "var(--app-text-primary)" }}>
          <Bot size={16} />
          <span>cc酱</span>
        </h3>
        <p className="m-0 text-[12px]" style={{ color: "var(--app-text-tertiary)" }}>
          桌面浮窗、chat 引擎和角色设置。
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <Label className="flex items-center gap-2">
          <Sparkles size={14} />
          <span>当前角色</span>
        </Label>
        <div className="flex flex-wrap items-center gap-2">
          <select
            value={value.activeRoleId}
            className="h-9 w-52 rounded-md px-2 text-[13px] outline-none"
            style={selectStyle}
            onChange={(event) => setActiveRole(event.target.value)}
          >
            {value.roles.map((role) => (
              <option key={role.id} value={role.id}>
                {role.name} · {role.aiEngine}
              </option>
            ))}
          </select>
          <Button type="button" size="sm" variant="secondary" onClick={addRole}>新增角色</Button>
          <Button type="button" size="sm" variant="ghost" disabled={!activeRole || activeRole.id === "default"} onClick={removeActiveRole}>
            删除
          </Button>
        </div>
      </div>

      {activeRole && (
        <div className="flex flex-col gap-3 rounded-md border p-3" style={{ borderColor: "var(--app-border)", background: "var(--app-bg)" }}>
          <div className="grid gap-3 md:grid-cols-2">
            <div className="flex flex-col gap-1">
              <Label>角色名称</Label>
              <input
                value={activeRole.name}
                className="h-9 rounded-md px-2 text-[13px] outline-none"
                style={selectStyle}
                onChange={(event) => updateRole(activeRole.id, { name: event.target.value })}
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label>角色宠物</Label>
              <select
                value={activeRole.petId}
                className="h-9 rounded-md px-2 text-[13px] outline-none"
                style={selectStyle}
                onChange={(event) => updateRole(activeRole.id, { petId: event.target.value })}
              >
                {petOptions.map((pet) => (
                  <option key={pet.id} value={pet.id}>
                    {pet.displayName} · {pet.source}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <Label>AI 引擎</Label>
            <div className="flex flex-wrap gap-2">
              {ENGINE_OPTIONS.map((option) => {
                const active = activeRole.aiEngine === option.value;
                return (
                  <button
                    key={option.value}
                    type="button"
                    className={cn(
                      "h-9 rounded-md border px-3 text-[13px] font-medium transition-colors",
                      active
                        ? "border-blue-500 bg-blue-600 text-white"
                        : "border-[var(--app-border)] bg-[var(--app-content)] text-[var(--app-text-secondary)] hover:text-[var(--app-text-primary)]",
                    )}
                    onClick={() => updateRole(activeRole.id, { aiEngine: option.value })}
                  >
                    {option.label}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <Label>chat 运行环境</Label>
            <div className="flex flex-wrap gap-2">
              {[
                { value: "local", label: "本机" },
                { value: "wsl", label: "WSL" },
              ].map((option) => {
                const active = activeRole.runtimeKind === option.value;
                return (
                  <button
                    key={option.value}
                    type="button"
                    className={cn(
                      "h-9 rounded-md border px-3 text-[13px] font-medium transition-colors",
                      active
                        ? "border-blue-500 bg-blue-600 text-white"
                        : "border-[var(--app-border)] bg-[var(--app-content)] text-[var(--app-text-secondary)] hover:text-[var(--app-text-primary)]",
                    )}
                    onClick={() => updateRole(activeRole.id, { runtimeKind: option.value as CCChanRolePreset["runtimeKind"] })}
                  >
                    {option.label}
                  </button>
                );
              })}
            </div>
          </div>

          {activeRole.runtimeKind === "wsl" && (
            <div className="grid gap-3 md:grid-cols-2">
              <div className="flex flex-col gap-1">
                <Label>WSL 远端路径</Label>
                <input
                  value={activeRole.wslRemotePath ?? ""}
                  placeholder="/mnt/d/my-project/cc-pane"
                  className="h-9 rounded-md px-2 text-[13px] outline-none"
                  style={selectStyle}
                  onChange={(event) => updateRole(activeRole.id, { wslRemotePath: event.target.value || null })}
                />
              </div>
              <div className="flex flex-col gap-1">
                <Label>WSL 发行版</Label>
                <input
                  value={activeRole.wslDistro ?? ""}
                  placeholder="Ubuntu-24.04，可留空使用默认"
                  className="h-9 rounded-md px-2 text-[13px] outline-none"
                  style={selectStyle}
                  onChange={(event) => updateRole(activeRole.id, { wslDistro: event.target.value || null })}
                />
              </div>
            </div>
          )}

          <div className="flex flex-col gap-1">
            <Label>系统提示词</Label>
            <textarea
              value={activeRole.systemPrompt}
              className="min-h-[96px] resize-y rounded-md px-2 py-2 text-[12px] outline-none"
              style={selectStyle}
              onChange={(event) => updateRole(activeRole.id, { systemPrompt: event.target.value })}
            />
          </div>
        </div>
      )}

      <div className="flex flex-col gap-2">
        <Label>桌宠状态范围</Label>
        <div className="flex flex-wrap gap-2">
          {[
            { value: "global", label: "全局会话" },
            { value: "focusedWindow", label: "当前窗口" },
          ].map((option) => {
            const active = value.scopeMode === option.value;
            return (
              <button
                key={option.value}
                type="button"
                className={cn(
                  "h-9 rounded-md border px-3 text-[13px] font-medium transition-colors",
                  active
                    ? "border-blue-500 bg-blue-600 text-white"
                    : "border-[var(--app-border)] bg-[var(--app-content)] text-[var(--app-text-secondary)] hover:text-[var(--app-text-primary)]",
                )}
                onClick={() => update("scopeMode", option.value as CCChanSettingsValue["scopeMode"])}
              >
                {option.label}
              </button>
            );
          })}
        </div>
      </div>

      <div className="flex flex-col gap-1">
        <Label>默认宠物</Label>
        <select
          value={value.defaultPetId}
          className="h-9 w-52 rounded-md px-2 text-[13px] outline-none"
          style={selectStyle}
          onChange={(event) => activeRole ? updateRole(activeRole.id, { petId: event.target.value }) : update("defaultPetId", event.target.value)}
        >
          {petOptions.map((pet) => (
            <option key={pet.id} value={pet.id}>
              {pet.displayName}
            </option>
          ))}
        </select>
        <p className="m-0 text-[11px]" style={{ color: "var(--app-text-tertiary)" }}>
          {petOptions.find((pet) => pet.id === value.defaultPetId)?.description ?? "当前角色"}
        </p>
      </div>

      <div className="flex flex-col gap-3 border-t pt-3" style={{ borderColor: "var(--app-border)" }}>
        <Label>宠物扩展来源</Label>
        {[
          { key: "builtin", label: "内置宠物" },
          { key: "user", label: "用户安装目录" },
          { key: "codexHome", label: "Codex Home pets" },
        ].map((item) => (
          <label key={item.key} className="flex items-center justify-between gap-3 text-[13px]" style={{ color: "var(--app-text-primary)" }}>
            <span>{item.label}</span>
            <input
              type="checkbox"
              checked={value.petSources[item.key as keyof typeof value.petSources]}
              className="h-4 w-4 cursor-pointer"
              style={{ accentColor: "var(--app-accent)" }}
              onChange={(event) => update("petSources", { ...value.petSources, [item.key]: event.target.checked })}
            />
          </label>
        ))}
        <div className="flex flex-wrap gap-2">
          <Button type="button" size="sm" variant="secondary" onClick={() => void installFromPath(true)}>
            <FolderPlus size={14} />
            文件夹安装
          </Button>
          <Button type="button" size="sm" variant="secondary" onClick={() => void installFromPath(false)}>
            <FolderPlus size={14} />
            zip 安装
          </Button>
          <Button type="button" size="sm" variant="secondary" onClick={() => void installFromUrl()}>
            <Download size={14} />
            URL 安装
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={petOptions.find((item) => item.id === value.defaultPetId)?.source !== "user"}
            onClick={() => void deleteUserPet(value.defaultPetId)}
          >
            删除当前用户宠物
          </Button>
        </div>
        <div className="flex flex-col gap-1">
          <Label>额外宠物目录</Label>
          <textarea
            value={value.customPetDirs.join("\n")}
            placeholder={"每行一个目录，例如：\n/mnt/d/my-pets\n\\\\wsl.localhost\\Ubuntu-24.04\\home\\me\\.codex\\pets"}
            className="min-h-[78px] resize-y rounded-md px-2 py-2 font-mono text-[12px] outline-none"
            style={selectStyle}
            onChange={(event) => update("customPetDirs", event.target.value.split(/\r?\n/))}
          />
          <p className="m-0 text-[11px]" style={{ color: "var(--app-text-tertiary)" }}>
            用于手动接入 Codex Home、WSL UNC 或其他本地宠物目录；这些目录只读，不会被“删除用户宠物”影响。
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          {PET_RESOURCE_LINKS.map((link) => (
            <Button key={link.url} type="button" size="sm" variant="ghost" onClick={() => void openPetResource(link.url)}>
              <ExternalLink size={13} />
              {link.label}
            </Button>
          ))}
        </div>
        {userPets.length > 0 && (
          <div className="flex max-h-40 flex-col gap-2 overflow-y-auto rounded-md border p-2" style={{ borderColor: "var(--app-border)", background: "var(--app-bg)" }}>
            {userPets.map((pet) => (
              <div key={pet.id} className="flex items-center justify-between gap-3 rounded px-2 py-1 text-[12px]" style={{ color: "var(--app-text-primary)" }}>
                <span className="min-w-0">
                  <span className="block truncate font-medium">{pet.displayName}</span>
                  <span className="block truncate" style={{ color: "var(--app-text-tertiary)" }}>{pet.id}</span>
                </span>
                <Button type="button" size="sm" variant="ghost" onClick={() => void deleteUserPet(pet.id)}>
                  <Trash2 size={13} />
                  删除
                </Button>
              </div>
            ))}
          </div>
        )}
        <p className="m-0 text-[11px]" style={{ color: "var(--app-text-tertiary)" }}>
          支持 Codex pet 标准结构：pet.json + spritesheet.webp/png/gif；URL 安装仅允许 HTTPS zip。
        </p>
      </div>

      <div className="flex flex-col gap-3 border-t pt-3" style={{ borderColor: "var(--app-border)" }}>
        <label className="flex items-center justify-between gap-3 text-[13px]" style={{ color: "var(--app-text-primary)" }}>
          <span className="flex items-center gap-2">
            <Power size={14} />
            开机自动显示
          </span>
          <input
            type="checkbox"
            checked={value.autoStart}
            className="h-4 w-4 cursor-pointer"
            style={{ accentColor: "var(--app-accent)" }}
            onChange={(event) => update("autoStart", event.target.checked)}
          />
        </label>

        <label className="flex items-center justify-between gap-3 text-[13px]" style={{ color: "var(--app-text-primary)" }}>
          <span className="flex items-center gap-2">
            <Music size={14} />
            启用通知音
          </span>
          <input
            type="checkbox"
            checked={value.soundEnabled}
            className="h-4 w-4 cursor-pointer"
            style={{ accentColor: "var(--app-accent)" }}
            onChange={(event) => update("soundEnabled", event.target.checked)}
          />
        </label>

        <label className="flex items-center justify-between gap-3 text-[13px]" style={{ color: "var(--app-text-primary)" }}>
          <span>浮窗可见</span>
          <input
            type="checkbox"
            checked={value.windowVisible}
            className="h-4 w-4 cursor-pointer"
            style={{ accentColor: "var(--app-accent)" }}
            onChange={(event) => update("windowVisible", event.target.checked)}
          />
        </label>
      </div>

      <div className="flex flex-col gap-2 border-t pt-3" style={{ borderColor: "var(--app-border)" }}>
        <Label className="flex items-center gap-2">
          <MapPin size={14} />
          <span>当前位置</span>
        </Label>
        <div className="flex items-center gap-2">
          <span
            className="rounded-md px-2.5 py-1.5 font-mono text-[12px]"
            style={{
              background: "var(--app-hover)",
              color: "var(--app-text-secondary)",
              border: "1px solid var(--app-border)",
            }}
          >
            x: {value.windowX ?? "-"} · y: {value.windowY ?? "-"} · 60x60 / 380x520
          </span>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            onClick={() => onChange({ ...value, windowX: null, windowY: null })}
          >
            重置位置
          </Button>
        </div>
      </div>
    </div>
  );
}
