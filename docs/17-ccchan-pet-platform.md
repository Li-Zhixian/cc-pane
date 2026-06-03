# ccchan Pet Platform

ccchan is the CC-Panes desktop mascot window. It follows terminal status events and can start a focused Claude Code or Codex chat using the active role preset.

## Settings Model

The `ccchan` settings block keeps legacy fields for compatibility and adds role presets:

- `activeRoleId`: selected role id.
- `roles`: role presets with `name`, `aiEngine`, `petId`, and `systemPrompt`.
- `scopeMode`: `global` or `focusedWindow`. `global` follows all live terminal sessions; `focusedWindow` follows the active terminal session bridged from the main window into the ccchan WebView.
- `petSources`: enables bundled pets, user-installed pets, and Codex Home pets.
- `aiEngine` and `defaultPetId`: legacy mirrors of the active role, kept for older callers.

On load, missing role fields are migrated into a `default` role. Selecting a role synchronizes `aiEngine` and `defaultPetId` so existing code paths remain compatible.

When ccchan chat is already open, changing the active role, AI engine, or role system prompt stops the old chat PTY and starts a new session with the updated role prompt. Renaming a role does not restart the session.

## Pet Package Format

CC-Panes accepts the Codex pet package shape:

```text
my-pet/
├── pet.json
└── spritesheet.webp
```

`pet.json`:

```json
{
  "id": "my-pet",
  "displayName": "My Pet",
  "description": "Short description",
  "spritesheetPath": "spritesheet.webp",
  "atlas": { "cellW": 192, "cellH": 208, "cols": 8, "rows": 9 },
  "animations": {
    "idle": { "row": 0, "frames": 6, "fps": 6 },
    "working": { "row": 7, "frames": 6, "fps": 12 },
    "waiting": { "row": 6, "frames": 6, "fps": 6 },
    "happy": { "row": 3, "frames": 4, "fps": 10 },
    "sad": { "row": 5, "frames": 8, "fps": 6 }
  }
}
```

If `atlas` or `animations` are omitted, CC-Panes applies Codex-style defaults for a `192x208`, `8x9` spritesheet.

## Pet Sources

Pets are discovered from:

- Bundled resources: `src-tauri/resources/ccchan`.
- User installs: `<data-dir>/ccchan/pets`.
- Codex Home: `$CODEX_HOME/pets` or `~/.codex/pets`.

Duplicate ids are resolved in this order: user installs, bundled pets, Codex Home.

## Installing Pets

The ccchan settings panel supports:

- Folder install: selects a folder containing `pet.json`, or a parent folder with one direct child containing `pet.json`.
- Zip install: extracts a zip with safe paths only, then installs the detected pet folder.
- HTTPS URL install: downloads a zip into `<data-dir>/ccchan/pet-staging`, previews metadata, then installs after confirmation.
- User pet management: lists and deletes pets installed under `<data-dir>/ccchan/pets`; bundled and Codex Home pets are read-only from this UI.
- Resource links: opens the Codex Pets community catalog, `awesome-codex-pet`, and the official Codex pets settings guide.

URL installs require `https://`, cap package size at 30 MB, cap file count at 128, and reject zip entries that escape the staging directory.

Official Codex app pets also support `codex://pets/install?name=&imageUrl=` deep links when that Codex app feature is enabled, and Codex can refresh custom pets from the user's local Codex home. CC-Panes does not depend on the Codex app deep-link flow; it reads Codex Home pets and supports package import directly.

## Runtime Status

ccchan combines two status paths:

- Hook/session notifier events from backend terminal lifecycle emit `task-complete`, `task-failed`, and `task-waiting`.
- The ccchan window also subscribes to terminal status snapshots and live updates, giving a PTY fallback for Claude Code and Codex sessions.
- `focusedWindow` mode uses a main-window bridge: ccchan emits `ccchan:ready`, the main window replies with `ccchan:active-session`, and subsequent pane/tab changes republish the active terminal session id.

Codex hooks are an enhancement path, not the only state source. Codex supports lifecycle hooks and `commandWindows`; CC-Panes enables the canonical `[features].hooks = true` flag while retaining the deprecated `codex_hooks` alias for compatibility. Project-local hook behavior still depends on Codex trust/config and host/runtime details, so CC-Panes treats terminal status snapshots and backend session notifications as the cross-platform baseline for Claude Code, Codex, Windows host launches, and WSL launches.

Windows-host-required validation still applies for desktop behavior: transparent WebView window, always-on-top behavior, tray interaction, WebView2, and Win32/WSL PTY details cannot be fully verified from WSL alone.
