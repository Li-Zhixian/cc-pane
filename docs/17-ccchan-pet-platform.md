# ccchan Pet Platform

ccchan is the CC-Panes desktop mascot window. It follows terminal status events and can start a focused Claude Code or Codex chat using the active role preset.

## Settings Model

The `ccchan` settings block keeps legacy fields for compatibility and adds role presets:

- `activeRoleId`: selected role id.
- `roles`: role presets with `name`, `aiEngine`, `petId`, `systemPrompt`, and chat runtime fields.
- `scopeMode`: `global` or `focusedWindow`. `global` follows all live terminal sessions; `focusedWindow` follows the active terminal session bridged from the main window into the ccchan WebView.
- `petSources`: enables bundled pets, user-installed pets, and a default local pet directory. The settings UI intentionally uses neutral local-source labels and does not expose internal source names.
- `aiEngine` and `defaultPetId`: legacy mirrors of the active role, kept for older callers.

On load, missing role fields are migrated into a `default` role. Selecting a role synchronizes `aiEngine` and `defaultPetId` so existing code paths remain compatible.

When ccchan chat is already open, changing the active role, AI engine, or role system prompt stops the old chat PTY and starts a new session with the updated role prompt. If a previous startup finishes after the user already switched roles, ccchan stops that stale PTY and retries the newest role instead of leaving chat stuck without a frontend session. Renaming a role does not restart the session.

The mascot context menu lists role presets when more than one role exists, so users can switch ccchan role, pet, and engine directly from the desktop pet without opening settings.

Opening ccchan settings from the mascot menu restores, unminimizes, and focuses the main window before selecting the ccchan settings section, matching deep-link install behavior.

Saving ccchan settings emits `ccchan:settings-updated`, so both the main window status bar and the separate ccchan WebView reload the latest role and pet list after settings changes, pet installs, and user pet deletion. Global settings saves use the same ccchan synchronization path, and the status bar mirrors fresh ccchan settings back into the global settings store so later saves do not overwrite mascot-only updates. Changing `windowVisible` through settings also calls the native show/hide command before persisting the new visible state. The settings save path rejects WSL roles without an absolute Linux remote path, so invalid WSL roles fail before the chat launcher is reached.

Settings include quick role templates for Claude local, Codex local, Claude WSL, Codex WSL, reviewer, and executor roles. WSL role templates preserve the current role's WSL path/distro when available, and the settings UI warns when a WSL role is missing the required remote path.

The WSL role editor can fill the active role from the currently selected workspace project. It prefers the project's explicit `wslRemotePath`, accepts already-Linux paths such as `/mnt/d/...`, converts Windows drive paths or WSL UNC paths with the same `toWslPath` helper used by workspace launches, and copies the workspace WSL distro when one is configured.

Role chat runtime supports `local` and explicit `wsl`. WSL chat requires a role-level `wslRemotePath` that starts with `/`; `~` paths are rejected because the Windows host must map the path to either a drive path like `D:\...` or a WSL UNC path like `\\wsl.localhost\<distro>\...` before writing project hooks. `wslDistro` is optional and falls back to the default distro. This is intentionally explicit so ccchan chat can run Claude Code or Codex from the same WSL project path the user expects, instead of silently guessing from the host data directory.

The chat panel blocks obviously invalid WSL role paths before starting a PTY and formats startup failures into actionable CLI, WSL, MCP, or provider/auth hints. The backend serializes ccchan chat start/stop lifecycle operations so rapid role switches or a stop request during startup cannot interleave session id mutation with PTY kill/create.

Closing the chat panel only collapses the ccchan window back to the pet size. The active chat PTY stays mounted in the hidden panel, terminal output continues to be buffered, and reopening the panel replays output captured while hidden. Closing during startup also keeps the pending startup mounted so a late session id is retained instead of being killed as stale. Role changes made while the panel is hidden do not immediately stop the hidden session; reopening the panel applies the normal visible role-switch behavior. The explicit stop button still terminates the active chat session, clears the visible transcript, and suppresses automatic restart until the panel is reopened.

The mascot context menu's exit action stops the active ccchan chat and uses the same hide path as the status bar and tray toggle, so `windowVisible` stays synchronized instead of closing the WebView without persisting visibility.

Changing a visible role's WSL remote path or distro is treated as a role-session change: ccchan stops the previous PTY, clears the active session id, and starts a new chat session with the updated WSL path and distro after the parent applies the cleared session id.

The ccchan window uses consistent sizes across frontend and backend resize commands: collapsed pet `120x120`, chat `460x640`, and context menu `460x260`.

## Pet Package Format

CC-Panes accepts the pet package shape:

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

If `atlas` or `animations` are omitted, CC-Panes applies default animation metadata for a `192x208`, `8x9` spritesheet.

## Pet Sources

Pets are discovered from:

- Bundled resources: `src-tauri/resources/ccchan`.
- User installs: `<data-dir>/ccchan/pets`.
- Default local pet directory resolved from the user's local pet home.
- Custom read-only directories configured in settings, useful for Windows paths, WSL UNC paths, or manually mirrored local pet folders.

Duplicate ids are resolved in this order: user installs, bundled pets, custom directories, default local pet directory.

Custom directories may point either at a parent folder containing multiple pet folders or at one concrete pet folder containing `pet.json`. Pets discovered from custom directories and the default local pet directory remain read-only in place, but the settings UI can copy one into the user install directory so it becomes a normal managed CC-Panes pet.

## Installing Pets

The ccchan settings panel supports:

- Folder install: selects a folder containing `pet.json`, or a parent folder with exactly one direct child containing `pet.json`; multiple direct pet children are rejected to avoid installing the wrong pet.
- Zip install: extracts a zip with safe paths only, then installs the detected pet folder.
- Remote URL or resource-link install is not exposed in ccchan settings. Users install pets through explicit folder, zip, the default local pet directory, or custom local directories.
- User pet management: lists and deletes pets installed under `<data-dir>/ccchan/pets`; bundled and read-only local-source pets cannot be deleted from this UI.
- Custom directory source management: users can either type one read-only directory per line or use a directory picker to append another pet source, so local mirrors, WSL UNC folders, and manually curated trusted pet folders can be added without hand-copying paths. The settings panel can check these directories and report missing, empty, invalid, warning, or ready states with pet counts. Read-only local-source pets can also be copied into the user install directory from settings.
- Internal source identifiers are not shown in ccchan settings; user-facing labels stay neutral and local-source oriented.

Folder and zip installs validate file-count, total-size, safe paths, and symlink limits before showing the confirmation dialog and again during the final copy.

Zip previews create a staging directory under `<data-dir>/ccchan/pet-staging`; successful installs, failed install attempts, and cancelled confirmation dialogs remove that staging directory instead of leaving abandoned packages behind.

CC-Panes keeps compatibility with local pet-home folders and discovers them through the default local directory source, but the settings UI does not show third-party resource catalogs, remote install links, or supported-link syntax.

Install confirmations use `@tauri-apps/plugin-dialog` instead of `window.confirm`. The default dialog capability allows message, save, and open commands, but not the deprecated confirm alias used by WebView confirm compatibility paths, so `src-tauri/capabilities/default.json` also grants `dialog:allow-confirm`. This prevents `dialog.confirm not allowed. Command not found` during Windows deep-link installs.

## Runtime Status

ccchan combines two status paths:

- Hook/session notifier events from backend terminal lifecycle emit `task-complete`, `task-failed`, and `task-waiting`.
- Event bubbles use a short session label such as `Session 12345678` when no richer title is available, avoiding full UUIDs in the desktop pet UI.
- The ccchan window also subscribes to terminal status snapshots and live updates, giving a PTY fallback for Claude Code and Codex sessions.
- Hook-driven `waitingInput` and `error` transitions are bridged into ccchan bubbles and sound cues. `turn-end` remains a status/notification event instead of a mascot "task complete" bubble because it means one assistant turn finished, not necessarily that the user's task is complete.
- Session status snapshots only expose tool metadata while the session is actually `toolRunning`; prompt, waiting-input, turn-end, compacting, error, and session-end transitions clear stale tool name, tool id, and summary fields before the main window or ccchan WebView receives the next status payload.
- `focusedWindow` mode uses a main-window bridge: ccchan emits `ccchan:ready`, the main window replies with `ccchan:active-session`, and subsequent pane/tab changes republish the active terminal session id.
- `focusedWindow` mode also filters the visible session dots, so the desktop pet's status badge and aggregate animation follow the same focused-session scope.
- `soundEnabled` controls the ccchan window's Web Audio cue for these lifecycle events; toast bubbles still appear when sound is disabled.

Codex hooks are an enhancement path, not the only state source. Codex supports lifecycle hooks and `commandWindows`; CC-Panes enables the canonical `[features].hooks = true` flag while retaining the deprecated `codex_hooks` alias for compatibility. Project-local hook behavior still depends on Codex trust/config and host/runtime details, so CC-Panes treats terminal status snapshots and backend session notifications as the cross-platform baseline for Claude Code, Codex, Windows host launches, and WSL launches.

Windows-host-required validation still applies for desktop behavior: transparent WebView window, always-on-top behavior, tray interaction, WebView2, and Win32/WSL PTY details cannot be fully verified from WSL alone.

## Validation Matrix

Current-environment-verifiable:

- TypeScript: `npx tsc --noEmit --pretty false`.
- Frontend focused checks: `npx vitest run web/stores/useCCChanStore.test.ts web/components/settings/CCChanSettings.test.tsx web/components/SettingsPanel.test.tsx web/stores/useSettingsStore.test.ts web/utils/notificationSound.test.ts web/ccchan/statusAggregator.test.ts web/ccchan/SessionDots.test.tsx web/ccchan/installPet.test.ts web/ccchan/deepLink.test.ts web/ccchan/ChatPanel.test.tsx web/ccchan/CCChanApp.test.tsx`.
- Focused Windows frontend reruns: `powershell.exe -NoProfile -Command "Set-Location 'D:\my-project\cc-pane'; npx vitest run web/ccchan/ChatPanel.test.tsx web/ccchan/installPet.test.ts web/ccchan/deepLink.test.ts --reporter=dot"` verifies WSL role restart behavior, hidden chat lifecycle handling, and protocol compatibility without exposing remote install affordances in settings.
- Rust model/adapter checks: `cargo test -p cc-panes-core ccchan_ -- --nocapture`, `cargo test -p cc-panes-core wsl_hook_sync -- --nocapture`, `cargo test -p cc-panes-core wsl_remote_project_path_to_host_path -- --nocapture`, `cargo test -p cc-cli-adapters codex -- --nocapture`, `cargo check -p cc-panes-core && cargo check -p cc-cli-adapters`.
- Runtime status regression checks: `cargo test -p cc-panes-core session_state_machine -- --nocapture` verifies hook transitions, stale tool metadata cleanup, and listener behavior; `cargo test -p cc-panes-core status_info_merges_state_machine_tool_snapshot -- --nocapture` verifies terminal status payloads merge current state-machine tool metadata for ccchan/frontend consumption.
- Formatting and patch hygiene: `cargo fmt --all -- --check`, `git diff --check`.

Windows-host compile/build checks run from WSL through PowerShell:

- `powershell.exe -NoProfile -Command "Set-Location 'D:\my-project\cc-pane'; cargo check -p cc-panes"`.
- `powershell.exe -NoProfile -Command "Set-Location 'D:\my-project\cc-pane'; npm run build"`.
- `powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Set-Location 'D:\my-project\cc-pane'; npm run probe:ccchan:windows"` verifies the running dev process, a visible `120x120` topmost mascot window, a visible main window, protocol registration, and persisted dev ccchan config fields.
- `powershell.exe -NoProfile -Command "Set-Location 'D:\my-project\cc-pane'; npx vitest run web/ccchan/installPet.test.ts web/components/settings/CCChanSettings.test.tsx --reporter=dot"`.
- `powershell.exe -NoProfile -Command "Set-Location 'D:\my-project\cc-pane'; cargo test -p cc-panes-core wsl_hook_sync -- --nocapture; cargo test -p cc-panes-core wsl_remote_project_path_to_host_path -- --nocapture; cargo test -p cc-cli-adapters codex -- --nocapture"` verifies WSL path mapping plus Codex Windows unsupported/WSL sync adapter behavior.

Windows-host runtime checks performed against `npm run tauri:dev` on this branch:

- The dev build starts successfully from `D:\my-project\cc-pane\target\debug\cc-panes.exe`; boot logs reach `=== setup complete ===`.
- The main window is visible as `CC-Panes [DEV]`, and the ccchan WebView2 mascot window is visible as `cc酱` at `120x120`.
- The ccchan window has topmost extended style bits (`WS_EX_TOPMOST`) and persisted `windowVisible`, `windowX`, and `windowY` updates in `C:\Users\ROG\.cc-panes-dev\config.toml`.
- A later scripted Win32 window probe on the same branch found `CC-Panes [DEV]` and `cc酱` under the dev process `77300`; the mascot window was `120x120` and `TopMost=true`.
- After the hidden-chat lifecycle fixes, a fresh scripted Win32 probe still found the dev process `77300` with `CC-Panes [DEV]` visible and a separate `cc酱` window visible at `120x120`, positioned at `(765, 489)`, with `TopMost=true`.
- A later ASCII-only Win32 probe on 2026-06-04 avoided Unicode title matching and enumerated windows by `cc-panes.exe` PID. It found the dev process at `D:\my-project\cc-pane\target\debug\cc-panes.exe`, a visible `120x120` topmost mascot window at `(563, 485)`, and a visible full-size main window under the same PID. A separate release process was also running, but the protocol registration still pointed to the dev executable.
- A subsequent Win32 probe on 2026-06-04 found two `cc-panes.exe` processes: the installed release at `C:\Users\ROG\AppData\Local\cc-panes\cc-panes.exe` and the dev process at `D:\my-project\cc-pane\target\debug\cc-panes.exe`. Under the dev PID `59620`, it found `CC-Panes [DEV]` at `1724x1084` and a visible `120x120` ccchan window at `(666, 595)` with `TopMost=true`.
- `npm run probe:ccchan:windows` now codifies that Win32 probe so future Windows-host validation does not depend on ad hoc scripts or Unicode window-title matching.
- `HKCU\Software\Classes\ccpanes\shell\open\command` points to `"D:\my-project\cc-pane\target\debug\cc-panes.exe" "%1"` while the dev app is running.
- `C:\Users\ROG\.cc-panes-dev\config.toml` has `windowVisible = true`, `windowX = 192.12036453656117`, and `windowY = 711.6158735115789`, confirming ccchan window visibility and position persistence in dev config.
- A Windows screen probe on 2026-06-04 found one primary monitor with bounds `0,0 1707x1067`; multi-monitor behavior remains unverified in this environment because no secondary display is currently attached.
- Triggering redacted protocol links did not leave a second `cc-panes.exe` process running, confirming single-instance forwarding at the process level.
- With the dev app stopped and Vite still serving `localhost:14200`, triggering a redacted pet install link cold-started `D:\my-project\cc-pane\target\debug\cc-panes.exe`; the main window and `120x120` ccchan window were created and boot logs reached `=== setup complete ===`.
- Triggering a redacted protocol link on Windows dev created a native ccchan confirmation dialog and did not show the previous `dialog.confirm not allowed` rejection.
- Runtime probes through the same `TerminalService.create_session` path used by ccchan chat verified Windows local Codex, Windows local Claude, WSL Codex, and WSL Claude launch plumbing. Windows local Codex spawned `C:\Users\ROG\AppData\Roaming\npm\codex.cmd`; Windows local Claude spawned `C:\Users\ROG\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe`; WSL Codex and WSL Claude entered `create_session: WSL mode` with `remote_path=/mnt/d/my-project/cc-pane` and spawned `C:\WINDOWS\system32\wsl.exe`. WSL Codex stayed live until killed by the probe. WSL Claude created and attached the PTY, then exited quickly with exit code 1 in this probe run; `wsl.exe -d Ubuntu-24.04 -- bash -lc "command -v claude; claude --version"` still reports Claude Code `2.1.159`, so the remaining gap is an interactive Claude WSL chat transcript, not the CC-Panes WSL launch path.
- A desktop screenshot captured during the run shows the ccchan settings panel open in the dev app and the mascot visible on the desktop; the screenshot is stored outside git under `_mod_memory/ccpanes-dev-screenshot.png`.

Current WSL limitation:

- `cargo check -p cc-panes` and `cargo test -p cc-panes ccchan_ -- --nocapture` require Linux WebKit/GTK pkg-config dependencies (`glib-2.0`, `gobject-2.0`, `gio-2.0`) in this WSL environment.
- Windows `cargo test -p cc-panes ccchan_ -- --nocapture` currently compiles the test binary but the binary exits before running tests with `STATUS_ENTRYPOINT_NOT_FOUND`; this still needs a Windows host runtime environment check separate from compile validation. A rerun with `CARGO_TARGET_DIR=D:\my-project\cc-pane-target-ccchan-test` avoided the running dev exe lock and still reproduced `STATUS_ENTRYPOINT_NOT_FOUND` after compiling `cc_panes_lib-2777f230912660e4.exe`.
- The external-resource catalog path was removed because it exposed nonessential remote references from the app surface.

Windows-host-required:

- Launch the dev or built Tauri app on Windows and verify the transparent ccchan WebView2 window, always-on-top, drag movement, tray/status-bar show-hide, settings `windowVisible`, and multi-monitor positioning. Current automated evidence verifies a visible `120x120` topmost dev ccchan window and persisted visibility/position on a single-monitor Windows host; physical multi-monitor positioning still requires a host with a second display.
- Verify desktop protocol registration on Windows, including cold-start URL handling and second-instance URL forwarding into the already-running main window.
- Verify local folder, zip, and trusted local-source install confirmations on Windows.
- Verify an interactive Claude WSL chat transcript from the ccchan UI. Launch plumbing for Windows local Claude/Codex and WSL Claude/Codex has Windows dev-run evidence above; WSL Claude still needs a manual interactive chat check because the automated probe exited quickly after PTY attach.
- Verify status updates for Claude and Codex through project hooks when supported, and through terminal/session fallback when hooks are degraded or unsupported.
- Install pets from folder, zip, and trusted local sources. Protocol compatibility remains a separate Windows-host check and must not reintroduce remote install affordances in settings.

Commit convention:

- Feature and follow-up commits should use Conventional Commits, for example `feat(ccchan): add pet import flow` or `fix(ccchan): sync settings across windows`.
