import type { TerminalOutput } from "@/types";

export type CCChanAiEngine = "claude" | "codex";
export type CCChanScopeMode = "global" | "focusedWindow";
export type CCChanRoleRuntimeKind = "local" | "wsl";

export interface CCChanPetSources {
  builtin: boolean;
  user: boolean;
  codexHome: boolean;
}

export interface CCChanRolePreset {
  id: string;
  name: string;
  aiEngine: CCChanAiEngine;
  petId: string;
  systemPrompt: string;
  runtimeKind: CCChanRoleRuntimeKind;
  wslRemotePath: string | null;
  wslDistro: string | null;
}

export interface CCChanSettings {
  aiEngine: CCChanAiEngine;
  defaultPetId: string;
  activeRoleId: string;
  roles: CCChanRolePreset[];
  scopeMode: CCChanScopeMode;
  petSources: CCChanPetSources;
  customPetDirs: string[];
  autoStart: boolean;
  soundEnabled: boolean;
  windowVisible: boolean;
  windowX: number | null;
  windowY: number | null;
}

export interface PetMeta {
  id: string;
  displayName: string;
  description: string;
  spritesheetUrl: string;
  source: "builtin" | "user" | "custom" | "codexHome";
  atlas: { cellW: number; cellH: number; cols: number; rows: number };
  animations: Record<string, { row: number; frames: number; fps: number; colOffset?: number }>;
}

export interface CCChanPetInstallPreview {
  stagingId: string;
  pet: PetMeta;
  sourcePath: string;
}

export interface CustomPetDirStatus {
  path: string;
  status: "ready" | "warning" | "missing" | "invalid" | "empty" | string;
  petCount: number;
  message: string;
}

export interface AwesomeCodexPetEntry {
  slug: string;
  name: string;
  author: string;
  authorHandle: string;
  authorUrl: string;
  primaryCategory: string;
  license: string;
  description: string;
}

export interface CCChanEvent {
  kind: "task-complete" | "task-failed" | "task-waiting";
  sessionId: string;
  title: string | null;
  ok: boolean;
  ts: number;
}

export type CCChanPetState =
  | "idle"
  | "working"
  | "waiting"
  | "happy"
  | "sad"
  | "thinking"
  | "walking"
  | "jumping";

export type TerminalOutputPayload = TerminalOutput;
