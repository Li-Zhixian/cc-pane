# ccchan Pet Platform

ccchan is the CC-Panes desktop mascot window. It follows terminal status events and can start a focused Claude Code or Codex chat using the active role preset.

## Settings Model

The `ccchan` settings block keeps legacy fields for compatibility and adds role presets:

- `activeRoleId`: selected role id.
- `roles`: role presets with `name`, `aiEngine`, `petId`, `systemPrompt`, and chat runtime fields.
- `scopeMode`: `global` or `focusedWindow`. `global` follows all live terminal sessions; `focusedWindow` follows the active terminal session bridged from the main window into the ccchan WebView.
- `petSources`: enables bundled pets, user-installed pets, and Codex Home pets.
- `aiEngine` and `defaultPetId`: legacy mirrors of the active role, kept for older callers.

On load, missing role fields are migrated into a `default` role. Selecting a role synchronizes `aiEngine` and `defaultPetId` so existing code paths remain compatible.

When ccchan chat is already open, changing the active role, AI engine, or role system prompt stops the old chat PTY and starts a new session with the updated role prompt. If a previous startup finishes after the user already switched roles, ccchan stops that stale PTY and retries the newest role instead of leaving chat stuck without a frontend session. Renaming a role does not restart the session.

The mascot context menu lists role presets when more than one role exists, so users can switch ccchan role, pet, and engine directly from the desktop pet without opening settings.

Saving ccchan settings emits `ccchan:settings-updated`, so both the main window status bar and the separate ccchan WebView reload the latest role and pet list after settings changes, pet installs, and user pet deletion. Global settings saves use the same ccchan synchronization path, and the status bar mirrors fresh ccchan settings back into the global settings store so later saves do not overwrite mascot-only updates. Changing `windowVisible` through settings also calls the native show/hide command before persisting the new visible state. The settings save path rejects WSL roles without an absolute Linux remote path, so invalid WSL roles fail before the chat launcher is reached.

Settings include quick role templates for Claude local, Codex local, Claude WSL, Codex WSL, reviewer, and executor roles. WSL role templates preserve the current role's WSL path/distro when available, and the settings UI warns when a WSL role is missing the required remote path.

The WSL role editor can fill the active role from the currently selected workspace project. It prefers the project's explicit `wslRemotePath`, accepts already-Linux paths such as `/mnt/d/...`, converts Windows drive paths or WSL UNC paths with the same `toWslPath` helper used by workspace launches, and copies the workspace WSL distro when one is configured.

Role chat runtime supports `local` and explicit `wsl`. WSL chat requires a role-level `wslRemotePath` that starts with `/`; `~` paths are rejected because the Windows host must map the path to either a drive path like `D:\...` or a WSL UNC path like `\\wsl.localhost\<distro>\...` before writing project hooks. `wslDistro` is optional and falls back to the default distro. This is intentionally explicit so ccchan chat can run Claude Code or Codex from the same WSL project path the user expects, instead of silently guessing from the host data directory.

The ccchan window uses consistent sizes across frontend and backend resize commands: collapsed pet `120x120`, chat `460x640`, and context menu `460x260`.

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
- Custom read-only directories configured in settings, useful for Windows paths, WSL UNC paths, or manually mirrored Codex Home pet folders.

Duplicate ids are resolved in this order: user installs, bundled pets, custom directories, Codex Home.

## Installing Pets

The ccchan settings panel supports:

- Folder install: selects a folder containing `pet.json`, or a parent folder with exactly one direct child containing `pet.json`; multiple direct pet children are rejected to avoid installing the wrong pet.
- Zip install: extracts a zip with safe paths only, then installs the detected pet folder.
- URL install: downloads an HTTPS zip package, or imports a pasted `codex://pets/install?name=&imageUrl=` / `ccpanes://pets/install?name=&imageUrl=` link by downloading the HTTPS `imageUrl` into a single-frame ccchan pet package. Both paths stage into `<data-dir>/ccchan/pet-staging`, preview metadata, then install after confirmation.
- Awesome Codex Pet catalog: loads `awesome-codex-pet`'s public `pets.json`, supports search by name, author, category, license, or slug, stages the selected pet from GitHub raw assets, previews metadata, then installs after confirmation.
- User pet management: lists and deletes pets installed under `<data-dir>/ccchan/pets`; bundled and Codex Home pets are read-only from this UI.
- Custom directory source management: users can either type one read-only directory per line or use a directory picker to append another pet source, so local Codex Home mirrors, WSL UNC folders, and manually curated community pet folders can be added without hand-copying paths.
- Resource links: opens the Codex Pets community catalog, `awesome-codex-pet`, and the official Codex pets settings guide.

URL installs require `https://`, stream remote downloads with a 30 MB cap, cap zip file count at 128, and reject zip entries that escape the staging directory. Folder installs use the same file-count and total-size limits and reject symlinks.

Awesome Codex Pet catalog installs are pinned to `https://raw.githubusercontent.com/legeling/awesome-codex-pet/main`, validate catalog slugs and relative spritesheet paths, and use the same staging/install flow as zip and URL installs.

URL, zip, and Awesome catalog previews create a staging directory under `<data-dir>/ccchan/pet-staging`; successful installs remove that staging directory, and cancelled confirmation dialogs call `cancel_ccchan_pet_preview` to remove the staged preview instead of leaving abandoned packages behind.

Official Codex app pets support `codex://pets/install?name=&imageUrl=` deep links when that Codex app feature is enabled, and Codex can refresh custom pets from the user's local Codex home. CC-Panes does not depend on the Codex app flow: it can import pasted Codex links from the ccchan settings URL installer, reads Codex Home pets, and supports package import directly. CC-Panes intentionally does not register the global `codex://` OS scheme because that belongs to the Codex app. CC-Panes registers its own `ccpanes://` desktop scheme through Tauri's deep-link and single-instance plugins; `ccpanes://pets/install?name=&imageUrl=` opens/focuses the main window, switches to ccchan settings, previews the pet, asks for confirmation, then installs through the same staging flow as pasted URL installs.

Install confirmations use `@tauri-apps/plugin-dialog` instead of `window.confirm`. The default dialog capability allows message, save, and open commands, but not the deprecated confirm alias used by WebView confirm compatibility paths, so `src-tauri/capabilities/default.json` also grants `dialog:allow-confirm`. This prevents `dialog.confirm not allowed. Command not found` during Windows deep-link installs.

## Runtime Status

ccchan combines two status paths:

- Hook/session notifier events from backend terminal lifecycle emit `task-complete`, `task-failed`, and `task-waiting`.
- Event bubbles use a short session label such as `Session 12345678` when no richer title is available, avoiding full UUIDs in the desktop pet UI.
- The ccchan window also subscribes to terminal status snapshots and live updates, giving a PTY fallback for Claude Code and Codex sessions.
- Hook-driven `waitingInput` and `error` transitions are bridged into ccchan bubbles and sound cues. `turn-end` remains a status/notification event instead of a mascot "task complete" bubble because it means one assistant turn finished, not necessarily that the user's task is complete.
- `focusedWindow` mode uses a main-window bridge: ccchan emits `ccchan:ready`, the main window replies with `ccchan:active-session`, and subsequent pane/tab changes republish the active terminal session id.
- `focusedWindow` mode also filters the visible session dots, so the desktop pet's status badge and aggregate animation follow the same focused-session scope.
- `soundEnabled` controls the ccchan window's Web Audio cue for these lifecycle events; toast bubbles still appear when sound is disabled.

Codex hooks are an enhancement path, not the only state source. Codex supports lifecycle hooks and `commandWindows`; CC-Panes enables the canonical `[features].hooks = true` flag while retaining the deprecated `codex_hooks` alias for compatibility. Project-local hook behavior still depends on Codex trust/config and host/runtime details, so CC-Panes treats terminal status snapshots and backend session notifications as the cross-platform baseline for Claude Code, Codex, Windows host launches, and WSL launches.

Windows-host-required validation still applies for desktop behavior: transparent WebView window, always-on-top behavior, tray interaction, WebView2, and Win32/WSL PTY details cannot be fully verified from WSL alone.

## Validation Matrix

Current-environment-verifiable:

- TypeScript: `npx tsc --noEmit --pretty false`.
- Frontend focused checks: `npx vitest run web/stores/useCCChanStore.test.ts web/components/settings/CCChanSettings.test.tsx web/components/SettingsPanel.test.tsx web/stores/useSettingsStore.test.ts web/utils/notificationSound.test.ts web/ccchan/statusAggregator.test.ts web/ccchan/SessionDots.test.tsx web/ccchan/installPet.test.ts`.
- Rust model/adapter checks: `cargo test -p cc-panes-core ccchan_ -- --nocapture`, `cargo test -p cc-panes-core wsl_hook_sync -- --nocapture`, `cargo test -p cc-panes-core wsl_remote_project_path_to_host_path -- --nocapture`, `cargo test -p cc-cli-adapters codex -- --nocapture`, `cargo check -p cc-panes-core && cargo check -p cc-cli-adapters`.
- Formatting and patch hygiene: `cargo fmt --all -- --check`, `git diff --check`.

Windows-host compile/build checks run from WSL through PowerShell:

- `powershell.exe -NoProfile -Command "Set-Location 'D:\my-project\cc-pane'; cargo check -p cc-panes"`.
- `powershell.exe -NoProfile -Command "Set-Location 'D:\my-project\cc-pane'; npm run build"`.
- `powershell.exe -NoProfile -Command "Set-Location 'D:\my-project\cc-pane'; npx vitest run web/ccchan/installPet.test.ts web/components/settings/CCChanSettings.test.tsx --reporter=dot"`.
- `powershell.exe -NoProfile -Command "Set-Location 'D:\my-project\cc-pane'; cargo test -p cc-panes-core wsl_hook_sync -- --nocapture; cargo test -p cc-panes-core wsl_remote_project_path_to_host_path -- --nocapture; cargo test -p cc-cli-adapters codex -- --nocapture"` verifies WSL path mapping plus Codex Windows unsupported/WSL sync adapter behavior.

Windows-host runtime checks performed against `npm run tauri:dev` on this branch:

- The dev build starts successfully from `D:\my-project\cc-pane\target\debug\cc-panes.exe`; boot logs reach `=== setup complete ===`.
- The main window is visible as `CC-Panes [DEV]`, and the ccchan WebView2 mascot window is visible as `cc酱` at `120x120`.
- The ccchan window has topmost extended style bits (`WS_EX_TOPMOST`) and persisted `windowVisible`, `windowX`, and `windowY` updates in `C:\Users\ROG\.cc-panes-dev\config.toml`.
- `HKCU\Software\Classes\ccpanes\shell\open\command` points to `"D:\my-project\cc-pane\target\debug\cc-panes.exe" "%1"` while the dev app is running.
- Triggering `ccpanes://pets/install/?name=DeepLinkProbe&imageUrl=https%3A%2F%2Fexample.invalid%2Fprobe.webp` and a real `awesome-codex-pet` style link did not leave a second `cc-panes.exe` process running, confirming single-instance forwarding at the process level.
- With the dev app stopped and Vite still serving `localhost:14200`, triggering `ccpanes://pets/install/?name=ColdStartProbe&imageUrl=https%3A%2F%2Fexample.invalid%2Fprobe.webp` cold-started `D:\my-project\cc-pane\target\debug\cc-panes.exe`; the main window and `120x120` ccchan window were created and boot logs reached `=== setup complete ===`.
- Triggering `ccpanes://pets/install/?name=NetworkDialogFix&image_url=https%3A%2F%2Fplacehold.co%2F192x208.png&desc=Network%20install%20probe` on Windows dev created a native ccchan confirmation dialog, installed the remote image pet after pressing the install button, and wrote `C:\Users\ROG\.cc-panes-dev\ccchan\pets\NetworkDialogFix\pet.json` plus `spritesheet.png`. The dev log did not show the previous `dialog.confirm not allowed` rejection.
- Runtime probes through the same `TerminalService.create_session` path used by ccchan chat verified Windows local Codex, Windows local Claude, WSL Codex, and WSL Claude launch plumbing. Windows local Codex spawned `C:\Users\ROG\AppData\Roaming\npm\codex.cmd`; Windows local Claude spawned `C:\Users\ROG\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe`; WSL Codex and WSL Claude entered `create_session: WSL mode` with `remote_path=/mnt/d/my-project/cc-pane` and spawned `C:\WINDOWS\system32\wsl.exe`. WSL Codex stayed live until killed by the probe. WSL Claude created and attached the PTY, then exited quickly with exit code 1 in this probe run; `wsl.exe -d Ubuntu-24.04 -- bash -lc "command -v claude; claude --version"` still reports Claude Code `2.1.159`, so the remaining gap is an interactive Claude WSL chat transcript, not the CC-Panes WSL launch path.
- A desktop screenshot captured during the run shows the ccchan settings panel open in the dev app and the mascot visible on the desktop; the screenshot is stored outside git under `_mod_memory/ccpanes-dev-screenshot.png`.

Current WSL limitation:

- `cargo check -p cc-panes` and `cargo test -p cc-panes ccchan_ -- --nocapture` require Linux WebKit/GTK pkg-config dependencies (`glib-2.0`, `gobject-2.0`, `gio-2.0`) in this WSL environment.
- Windows `cargo test -p cc-panes ccchan_ -- --nocapture` currently compiles the test binary but the binary exits before running tests with `STATUS_ENTRYPOINT_NOT_FOUND`; this still needs a Windows host runtime environment check separate from compile validation.
- The Windows host could not resolve `raw.githubusercontent.com` during runtime validation, so a real Awesome Codex Pet image deep-link could not complete the remote HTTPS download path in that environment. Re-run pet install validation on a Windows network that can resolve and fetch GitHub raw assets.

Windows-host-required:

- Launch the dev or built Tauri app on Windows and verify the transparent ccchan WebView2 window, always-on-top, drag movement, tray/status-bar show-hide, settings `windowVisible`, and multi-monitor positioning.
- Verify `ccpanes://` protocol registration on Windows, including cold-start URL handling and second-instance URL forwarding into the already-running main window.
- Verify `ccpanes://` install confirmation and download with additional community assets. A real Windows dev-run install with `placehold.co` has passed; GitHub raw assets still require a Windows network that can resolve and fetch `raw.githubusercontent.com`.
- Verify an interactive Claude WSL chat transcript from the ccchan UI. Launch plumbing for Windows local Claude/Codex and WSL Claude/Codex has Windows dev-run evidence above; WSL Claude still needs a manual interactive chat check because the automated probe exited quickly after PTY attach.
- Verify status updates for Claude and Codex through project hooks when supported, and through terminal/session fallback when hooks are degraded or unsupported.
- Install pets from folder, zip, HTTPS URL, `codex://pets/install` paste, `ccpanes://pets/install` paste, Codex Home/custom directories, and the Awesome Codex Pet catalog. `ccpanes://pets/install` OS deep-link click has passed on Windows dev with a reachable HTTPS asset.

Commit convention:

- Feature and follow-up commits should use Conventional Commits, for example `feat(ccchan): add community pet catalog` or `fix(ccchan): sync settings across windows`.
