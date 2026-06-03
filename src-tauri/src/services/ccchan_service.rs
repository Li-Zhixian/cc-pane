//! ccchan mascot backend service.
//!
//! Sprite attribution: Homie spritesheet from oc-claw (MIT), Copyright (c) rainnoon.

use crate::models::settings::CCChanSettings;
use crate::models::{CliTool, LaunchProviderSelection};
use crate::services::{SettingsService, TerminalService};
use crate::utils::{AppError, AppPaths, AppResult};
use cc_panes_core::events::SessionNotifier;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::{debug, warn};

const CCCHAN_WINDOW_LABEL: &str = "ccchan";
const CCCHAN_EVENT: &str = "ccchan-event";
const CCCHAN_HELPER_PROMPT: &str =
    include_str!("../../resources/claude-bundle/default-skills/ccchan-helper.md");
const MAX_PET_PACKAGE_BYTES: usize = 30 * 1024 * 1024;
const MAX_PET_FILES: usize = 128;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PetMeta {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub spritesheet_url: String,
    pub source: PetSource,
    pub atlas: PetAtlas,
    pub animations: HashMap<String, PetAnimation>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PetSource {
    Builtin,
    User,
    CodexHome,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PetAtlas {
    pub cell_w: u32,
    pub cell_h: u32,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PetAnimation {
    pub row: u32,
    pub frames: u32,
    pub fps: u32,
    #[serde(default)]
    pub col_offset: u32,
}

#[derive(Deserialize)]
struct PetsManifest {
    pets: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PetDefinition {
    #[serde(default)]
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    spritesheet_path: String,
    #[serde(default)]
    atlas: PetAtlas,
    #[serde(default = "default_pet_animations")]
    animations: HashMap<String, PetAnimation>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PetInstallPreview {
    pub staging_id: String,
    pub pet: PetMeta,
    pub source_path: String,
}

pub struct CCChanService {
    settings_service: Arc<SettingsService>,
    app_paths: Arc<AppPaths>,
    app_handle: Mutex<Option<AppHandle>>,
    chat_session_id: Mutex<Option<String>>,
}

impl CCChanService {
    pub fn new(settings_service: Arc<SettingsService>, app_paths: Arc<AppPaths>) -> Self {
        Self {
            settings_service,
            app_paths,
            app_handle: Mutex::new(None),
            chat_session_id: Mutex::new(None),
        }
    }

    pub fn set_app_handle(&self, app_handle: AppHandle) {
        if let Ok(mut handle) = self.app_handle.lock() {
            *handle = Some(app_handle);
        }
    }

    pub fn settings(&self) -> CCChanSettings {
        self.settings_service.get_settings().ccchan
    }

    pub fn save_settings(&self, settings: CCChanSettings) -> AppResult<()> {
        let mut app_settings = self.settings_service.get_settings();
        app_settings.ccchan = settings;
        self.settings_service.update_settings(app_settings)?;
        Ok(())
    }

    pub fn show_window(&self, app: &AppHandle) -> AppResult<()> {
        let window = ccchan_window(app)?;
        window
            .set_size(LogicalSize::new(120.0, 120.0))
            .map_err(|error| AppError::from(error.to_string()))?;
        window
            .set_decorations(false)
            .map_err(|error| AppError::from(error.to_string()))?;
        window
            .set_always_on_top(true)
            .map_err(|error| AppError::from(error.to_string()))?;
        position_window(&window, &self.settings())?;
        window
            .show()
            .map_err(|error| AppError::from(error.to_string()))?;
        self.set_window_visible(true)?;
        Ok(())
    }

    pub fn hide_window(&self, app: &AppHandle) -> AppResult<()> {
        let window = ccchan_window(app)?;
        window
            .hide()
            .map_err(|error| AppError::from(error.to_string()))?;
        self.set_window_visible(false)?;
        Ok(())
    }

    pub fn save_window_position(&self, x: f64, y: f64) -> AppResult<()> {
        let mut settings = self.settings();
        settings.window_x = Some(x);
        settings.window_y = Some(y);
        self.save_settings(settings)
    }

    pub fn get_pets(&self, app: &AppHandle) -> AppResult<Vec<PetMeta>> {
        self.discover_pets(app)
    }

    pub async fn preview_pet_from_url(&self, url: String) -> AppResult<PetInstallPreview> {
        let parsed = reqwest::Url::parse(url.trim()).map_err(|error| {
            AppError::from(format!(
                "Invalid ccchan pet URL '{}': {}",
                url.trim(),
                error
            ))
        })?;
        if parsed.scheme() != "https" {
            return Err(AppError::from(
                "ccchan pet URL installs require an https:// URL",
            ));
        }

        let response = reqwest::get(parsed.clone())
            .await
            .map_err(|error| AppError::from(format!("Failed to download pet package: {error}")))?;
        if !response.status().is_success() {
            return Err(AppError::from(format!(
                "Failed to download pet package: HTTP {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| AppError::from(format!("Failed to read pet package: {error}")))?;
        if bytes.len() > MAX_PET_PACKAGE_BYTES {
            return Err(AppError::from(format!(
                "Pet package is too large: {} bytes",
                bytes.len()
            )));
        }

        let staging_id = uuid::Uuid::new_v4().to_string();
        let staging_dir = self.pet_staging_dir().join(&staging_id);
        std::fs::create_dir_all(&staging_dir)?;
        let path_ext = Path::new(parsed.path())
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if path_ext != "zip" {
            return Err(AppError::from(
                "ccchan URL installs currently require a .zip pet package",
            ));
        }
        extract_pet_zip(&bytes, &staging_dir)?;

        let pet_root = find_pet_root(&staging_dir)?;
        let pet = load_pet_from_dir(&pet_root, PetSource::User)?;
        Ok(PetInstallPreview {
            staging_id,
            pet,
            source_path: parsed.to_string(),
        })
    }

    pub fn preview_pet_from_path(&self, path: String) -> AppResult<PetInstallPreview> {
        let source = PathBuf::from(path.trim());
        let mut staging_id = String::new();
        let pet_root = if source.is_file()
            && source
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        {
            let bytes = std::fs::read(&source).map_err(|error| {
                AppError::from(format!(
                    "Failed to read pet zip {}: {}",
                    source.display(),
                    error
                ))
            })?;
            if bytes.len() > MAX_PET_PACKAGE_BYTES {
                return Err(AppError::from(format!(
                    "Pet package is too large: {} bytes",
                    bytes.len()
                )));
            }
            staging_id = uuid::Uuid::new_v4().to_string();
            let staging_dir = self.pet_staging_dir().join(&staging_id);
            std::fs::create_dir_all(&staging_dir)?;
            extract_pet_zip(&bytes, &staging_dir)?;
            find_pet_root(&staging_dir)?
        } else {
            find_pet_root(&source)?
        };
        let pet = load_pet_from_dir(&pet_root, PetSource::User)?;
        Ok(PetInstallPreview {
            staging_id,
            pet,
            source_path: source.to_string_lossy().to_string(),
        })
    }

    pub fn install_pet_from_preview(&self, staging_id: String) -> AppResult<PetMeta> {
        if staging_id.trim().is_empty() {
            return Err(AppError::from("ccchan pet preview staging id is required"));
        }
        let staging_dir = self
            .pet_staging_dir()
            .join(sanitize_path_segment(&staging_id));
        let pet_root = find_pet_root(&staging_dir)?;
        self.install_pet_dir(&pet_root)
    }

    pub fn install_pet_from_path(&self, path: String) -> AppResult<PetMeta> {
        let source = PathBuf::from(path.trim());
        let pet_root = if source.is_file()
            && source
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        {
            let bytes = std::fs::read(&source).map_err(|error| {
                AppError::from(format!(
                    "Failed to read pet zip {}: {}",
                    source.display(),
                    error
                ))
            })?;
            if bytes.len() > MAX_PET_PACKAGE_BYTES {
                return Err(AppError::from(format!(
                    "Pet package is too large: {} bytes",
                    bytes.len()
                )));
            }
            let staging_id = uuid::Uuid::new_v4().to_string();
            let staging_dir = self.pet_staging_dir().join(&staging_id);
            std::fs::create_dir_all(&staging_dir)?;
            extract_pet_zip(&bytes, &staging_dir)?;
            find_pet_root(&staging_dir)?
        } else {
            find_pet_root(&source)?
        };
        self.install_pet_dir(&pet_root)
    }

    pub fn start_chat(
        &self,
        terminal_service: Arc<TerminalService>,
        ai_engine: String,
        system_prompt: Option<String>,
    ) -> AppResult<String> {
        let cli_tool = parse_ai_engine(&ai_engine)?;
        let chat_dir = self.app_paths.data_dir().join("ccchan");
        std::fs::create_dir_all(&chat_dir).map_err(|error| {
            AppError::from(format!(
                "Failed to create ccchan chat directory {}: {}",
                chat_dir.display(),
                error
            ))
        })?;

        if let Some(existing) = self.take_chat_session_id()? {
            let _ = terminal_service.kill(&existing);
        }

        let chat_dir_str = chat_dir.to_string_lossy().to_string();
        let prompt = build_ccchan_prompt(system_prompt.as_deref());
        let session_id = terminal_service.create_session(
            None,
            &chat_dir_str,
            80,
            24,
            None,
            None,
            LaunchProviderSelection::Inherit,
            None,
            None,
            None,
            cli_tool,
            None,
            false,
            Some(&prompt),
            None,
            None,
            None,
            None,
        )?;

        let mut stored = self
            .chat_session_id
            .lock()
            .map_err(|_| AppError::from("ccchan chat session lock poisoned"))?;
        *stored = Some(session_id.clone());
        Ok(session_id)
    }

    pub fn send_to_chat(
        &self,
        terminal_service: Arc<TerminalService>,
        session_id: &str,
        text: &str,
    ) -> AppResult<()> {
        terminal_service.write(session_id, text)?;
        terminal_service.write(session_id, "\r")?;
        Ok(())
    }

    pub fn stop_chat(
        &self,
        terminal_service: Arc<TerminalService>,
        session_id: &str,
    ) -> AppResult<()> {
        self.clear_chat_session_id(session_id)?;
        match terminal_service.kill(session_id) {
            Ok(()) => Ok(()),
            Err(error) if error.to_string().to_ascii_lowercase().contains("not found") => Ok(()),
            Err(error) => Err(AppError::from(error.to_string())),
        }
    }

    pub fn notify_task_done(&self, session_id: &str, ok: bool) {
        let kind = if ok { "task-complete" } else { "task-failed" };
        self.emit_ccchan_event(kind, session_id, ok);
    }

    pub fn notify_task_waiting(&self, session_id: &str) {
        self.emit_ccchan_event("task-waiting", session_id, true);
    }

    fn set_window_visible(&self, visible: bool) -> AppResult<()> {
        let mut settings = self.settings();
        settings.window_visible = visible;
        self.save_settings(settings)
    }

    fn discover_pets(&self, app: &AppHandle) -> AppResult<Vec<PetMeta>> {
        let mut pets: HashMap<String, PetMeta> = HashMap::new();
        let settings = self.settings();

        if settings.pet_sources.builtin {
            let root = resolve_ccchan_root(app)?;
            for pet in load_manifest_pets(&root, PetSource::Builtin)? {
                pets.insert(pet.id.clone(), pet);
            }
        }

        if settings.pet_sources.codex_home {
            if let Some(codex_pets_dir) = codex_home_pets_dir() {
                for pet in self.load_pet_dir_children(&codex_pets_dir, PetSource::CodexHome)? {
                    pets.entry(pet.id.clone()).or_insert(pet);
                }
            }
        }

        if settings.pet_sources.user {
            for pet in self.load_pet_dir_children(&self.user_pets_dir(), PetSource::User)? {
                pets.insert(pet.id.clone(), pet);
            }
        }

        let mut result: Vec<PetMeta> = pets.into_values().collect();
        result.sort_by(|a, b| {
            source_rank(a.source)
                .cmp(&source_rank(b.source))
                .then_with(|| a.display_name.cmp(&b.display_name))
        });
        Ok(result)
    }

    fn load_pet_dir_children(&self, root: &Path, source: PetSource) -> AppResult<Vec<PetMeta>> {
        if !root.exists() {
            return Ok(Vec::new());
        }
        let mut pets = Vec::new();
        for entry in std::fs::read_dir(root).map_err(|error| {
            AppError::from(format!(
                "Failed to read pet directory {}: {}",
                root.display(),
                error
            ))
        })? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            match load_pet_from_dir(&path, source) {
                Ok(pet) => pets.push(pet),
                Err(error) => {
                    warn!(path = %path.display(), error = %error, "skipping invalid ccchan pet")
                }
            }
        }
        Ok(pets)
    }

    fn install_pet_dir(&self, pet_root: &Path) -> AppResult<PetMeta> {
        let pet = load_pet_from_dir(pet_root, PetSource::User)?;
        let target = self.user_pets_dir().join(sanitize_path_segment(&pet.id));
        if target.exists() {
            std::fs::remove_dir_all(&target).map_err(|error| {
                AppError::from(format!(
                    "Failed to replace existing pet {}: {}",
                    target.display(),
                    error
                ))
            })?;
        }
        copy_pet_dir(pet_root, &target)?;
        load_pet_from_dir(&target, PetSource::User)
    }

    fn user_pets_dir(&self) -> PathBuf {
        self.app_paths.data_dir().join("ccchan").join("pets")
    }

    fn pet_staging_dir(&self) -> PathBuf {
        self.app_paths.data_dir().join("ccchan").join("pet-staging")
    }

    fn take_chat_session_id(&self) -> AppResult<Option<String>> {
        let mut stored = self
            .chat_session_id
            .lock()
            .map_err(|_| AppError::from("ccchan chat session lock poisoned"))?;
        Ok(stored.take())
    }

    fn clear_chat_session_id(&self, session_id: &str) -> AppResult<()> {
        let mut stored = self
            .chat_session_id
            .lock()
            .map_err(|_| AppError::from("ccchan chat session lock poisoned"))?;
        if stored.as_deref() == Some(session_id) {
            *stored = None;
        }
        Ok(())
    }

    fn emit_ccchan_event(&self, kind: &str, session_id: &str, ok: bool) {
        let app_handle = self
            .app_handle
            .lock()
            .ok()
            .and_then(|handle| handle.clone());
        let Some(app) = app_handle else {
            debug!(
                session_id,
                kind, "ccchan event skipped before app handle is set"
            );
            return;
        };

        let payload = serde_json::json!({
            "kind": kind,
            "sessionId": session_id,
            "title": serde_json::Value::Null,
            "ok": ok,
            "ts": current_epoch_seconds(),
        });
        if let Err(error) = app.emit(CCCHAN_EVENT, payload) {
            warn!(session_id, kind, error = %error, "failed to emit ccchan event");
        }
    }
}

pub struct CcChanSessionNotifier {
    inner: Arc<dyn SessionNotifier>,
    ccchan_service: Arc<CCChanService>,
}

impl CcChanSessionNotifier {
    pub fn new(inner: Arc<dyn SessionNotifier>, ccchan_service: Arc<CCChanService>) -> Self {
        Self {
            inner,
            ccchan_service,
        }
    }
}

impl SessionNotifier for CcChanSessionNotifier {
    fn notify_waiting_input(&self, session_id: &str) {
        self.inner.notify_waiting_input(session_id);
        self.ccchan_service.notify_task_waiting(session_id);
    }

    fn notify_session_exited(&self, session_id: &str, exit_code: i32) {
        self.inner.notify_session_exited(session_id, exit_code);
        self.ccchan_service
            .notify_task_done(session_id, exit_code == 0);
    }

    fn cleanup_session(&self, session_id: &str) {
        self.inner.cleanup_session(session_id);
    }
}

fn ccchan_window(app: &AppHandle) -> AppResult<WebviewWindow> {
    if let Some(window) = app.get_webview_window(CCCHAN_WINDOW_LABEL) {
        return Ok(window);
    }

    WebviewWindowBuilder::new(
        app,
        CCCHAN_WINDOW_LABEL,
        WebviewUrl::App("index.html?mode=ccchan".into()),
    )
    .title("cc酱")
    .visible(false)
    .inner_size(120.0, 120.0)
    .position(-9999.0, -9999.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .build()
    .map_err(|error| AppError::from(format!("Failed to create ccchan window: {error}")))
}

fn position_window(window: &WebviewWindow, settings: &CCChanSettings) -> AppResult<()> {
    let (x, y) = match (settings.window_x, settings.window_y) {
        (Some(x), Some(y)) => clamp_position_to_visible(window, x, y),
        _ => (80.0, 80.0),
    };
    window
        .set_position(LogicalPosition::new(x, y))
        .map_err(|error| AppError::from(error.to_string()))
}

/// Snap a window position into a currently-attached monitor.
///
/// - If `(x, y)` is already inside any monitor (with a 40px sliver for half-off
///   tolerance), return as-is.
/// - Otherwise pick the monitor closest to `(x, y)` and clamp the position to
///   that monitor's interior (leaving the mascot's full body visible).
/// - If no monitors are attached at all, fall back to (80, 80).
///
/// Used both on startup (resolve stale persisted positions after monitor
/// hot-unplug / DPI change) AND on every drag-release (so a user who drags
/// the mascot off-screen sees it snap back instead of vanishing).
pub fn clamp_position_to_visible(window: &WebviewWindow, x: f64, y: f64) -> (f64, f64) {
    const PET_SIZE: f64 = 120.0;
    const SAFE_MARGIN: f64 = 8.0;
    const HALF_OFF_TOLERANCE: f64 = 40.0;

    let Ok(monitors) = window.available_monitors() else {
        return (80.0, 80.0);
    };
    if monitors.is_empty() {
        return (80.0, 80.0);
    }

    let already_visible = monitors.iter().any(|m| {
        let (lx, ly, lw, lh) = monitor_logical_rect(m);
        x + HALF_OFF_TOLERANCE > lx
            && x < lx + lw - HALF_OFF_TOLERANCE
            && y + HALF_OFF_TOLERANCE > ly
            && y < ly + lh - HALF_OFF_TOLERANCE
    });
    if already_visible {
        return (x, y);
    }

    let mut best: Option<(f64, f64, f64)> = None;
    for m in &monitors {
        let (lx, ly, lw, lh) = monitor_logical_rect(m);
        let cx = x.clamp(
            lx + SAFE_MARGIN,
            (lx + lw - PET_SIZE - SAFE_MARGIN).max(lx + SAFE_MARGIN),
        );
        let cy = y.clamp(
            ly + SAFE_MARGIN,
            (ly + lh - PET_SIZE - SAFE_MARGIN).max(ly + SAFE_MARGIN),
        );
        let dist = (cx - x).powi(2) + (cy - y).powi(2);
        if best.is_none_or(|b| dist < b.0) {
            best = Some((dist, cx, cy));
        }
    }
    best.map(|(_, cx, cy)| (cx, cy)).unwrap_or((80.0, 80.0))
}

fn monitor_logical_rect(monitor: &tauri::Monitor) -> (f64, f64, f64, f64) {
    let scale = monitor.scale_factor();
    let pos = monitor.position();
    let size = monitor.size();
    (
        pos.x as f64 / scale,
        pos.y as f64 / scale,
        size.width as f64 / scale,
        size.height as f64 / scale,
    )
}

fn parse_ai_engine(ai_engine: &str) -> AppResult<CliTool> {
    match ai_engine.trim().to_ascii_lowercase().as_str() {
        "claude" => Ok(CliTool::Claude),
        "codex" => Ok(CliTool::Codex),
        other => Err(AppError::from(format!(
            "Unsupported ccchan aiEngine '{}'; expected 'claude' or 'codex'",
            other
        ))),
    }
}

fn build_ccchan_prompt(system_prompt: Option<&str>) -> String {
    let Some(system_prompt) = system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return CCCHAN_HELPER_PROMPT.to_string();
    };
    format!("{CCCHAN_HELPER_PROMPT}\n\n# Active ccchan Role\n\n{system_prompt}\n")
}

fn load_manifest_pets(root: &Path, source: PetSource) -> AppResult<Vec<PetMeta>> {
    let manifest_path = root.join("pets-manifest.json");
    let manifest_content = std::fs::read_to_string(&manifest_path).map_err(|error| {
        AppError::from(format!(
            "Failed to read {}: {}",
            manifest_path.display(),
            error
        ))
    })?;
    let manifest: PetsManifest = serde_json::from_str(&manifest_content)
        .map_err(|error| AppError::from(format!("Invalid pets manifest: {error}")))?;
    manifest
        .pets
        .iter()
        .map(|pet_id| load_pet_from_dir(&root.join(pet_id), source))
        .collect()
}

fn load_pet_from_dir(pet_dir: &Path, source: PetSource) -> AppResult<PetMeta> {
    let pet_json_path = pet_dir.join("pet.json");
    let pet_content = std::fs::read_to_string(&pet_json_path).map_err(|error| {
        AppError::from(format!(
            "Failed to read {}: {}",
            pet_json_path.display(),
            error
        ))
    })?;
    let definition: PetDefinition = serde_json::from_str(&pet_content).map_err(|error| {
        AppError::from(format!(
            "Invalid pet.json for {}: {error}",
            pet_dir.display()
        ))
    })?;
    let pet_id = if definition.id.trim().is_empty() {
        pet_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("ccchan-pet")
            .to_string()
    } else {
        definition.id.trim().to_string()
    };
    let spritesheet_path = resolve_spritesheet_path(pet_dir, &definition.spritesheet_path)?;

    Ok(PetMeta {
        id: pet_id.clone(),
        display_name: non_empty_or(definition.display_name, &pet_id),
        description: non_empty_or(definition.description, "Custom ccchan pet"),
        spritesheet_url: file_asset_url(&spritesheet_path),
        source,
        atlas: definition.atlas,
        animations: definition.animations,
    })
}

fn default_pet_atlas() -> PetAtlas {
    PetAtlas {
        cell_w: 192,
        cell_h: 208,
        cols: 8,
        rows: 9,
    }
}

impl Default for PetAtlas {
    fn default() -> Self {
        default_pet_atlas()
    }
}

fn default_pet_animations() -> HashMap<String, PetAnimation> {
    HashMap::from([
        (
            "idle".to_string(),
            PetAnimation {
                row: 0,
                frames: 6,
                fps: 6,
                col_offset: 0,
            },
        ),
        (
            "walking".to_string(),
            PetAnimation {
                row: 1,
                frames: 8,
                fps: 8,
                col_offset: 0,
            },
        ),
        (
            "thinking".to_string(),
            PetAnimation {
                row: 8,
                frames: 6,
                fps: 8,
                col_offset: 0,
            },
        ),
        (
            "waiting".to_string(),
            PetAnimation {
                row: 6,
                frames: 6,
                fps: 6,
                col_offset: 0,
            },
        ),
        (
            "working".to_string(),
            PetAnimation {
                row: 7,
                frames: 6,
                fps: 12,
                col_offset: 0,
            },
        ),
        (
            "happy".to_string(),
            PetAnimation {
                row: 3,
                frames: 4,
                fps: 10,
                col_offset: 0,
            },
        ),
        (
            "sad".to_string(),
            PetAnimation {
                row: 5,
                frames: 8,
                fps: 6,
                col_offset: 0,
            },
        ),
        (
            "jumping".to_string(),
            PetAnimation {
                row: 4,
                frames: 5,
                fps: 6,
                col_offset: 0,
            },
        ),
    ])
}

fn codex_home_pets_dir() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
        .map(|home| home.join("pets"))
}

fn source_rank(source: PetSource) -> u8 {
    match source {
        PetSource::User => 0,
        PetSource::Builtin => 1,
        PetSource::CodexHome => 2,
    }
}

fn find_pet_root(path: &Path) -> AppResult<PathBuf> {
    if path.join("pet.json").exists() {
        return Ok(path.to_path_buf());
    }
    if path.is_file() {
        return Err(AppError::from(format!(
            "ccchan pet path is not a directory or zip: {}",
            path.display()
        )));
    }
    for entry in std::fs::read_dir(path).map_err(|error| {
        AppError::from(format!(
            "Failed to read pet path {}: {}",
            path.display(),
            error
        ))
    })? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() && child.join("pet.json").exists() {
            return Ok(child);
        }
    }
    Err(AppError::from(format!(
        "No pet.json found in {}",
        path.display()
    )))
}

fn resolve_spritesheet_path(pet_dir: &Path, configured_path: &str) -> AppResult<PathBuf> {
    let configured = configured_path.trim();
    let candidates: Vec<PathBuf> = if configured.is_empty() {
        vec![
            pet_dir.join("spritesheet.webp"),
            pet_dir.join("spritesheet.png"),
            pet_dir.join("spritesheet.gif"),
        ]
    } else {
        vec![pet_dir.join(configured)]
    };
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| AppError::from(format!("No spritesheet found in {}", pet_dir.display())))
}

fn non_empty_or(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn sanitize_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .trim_matches('-')
        .to_string()
}

fn copy_pet_dir(source: &Path, target: &Path) -> AppResult<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source).map_err(|error| {
        AppError::from(format!(
            "Failed to read pet directory {}: {}",
            source.display(),
            error
        ))
    })? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_pet_dir(&source_path, &target_path)?;
        } else {
            std::fs::copy(&source_path, &target_path).map_err(|error| {
                AppError::from(format!(
                    "Failed to copy pet file {}: {}",
                    source_path.display(),
                    error
                ))
            })?;
        }
    }
    Ok(())
}

fn extract_pet_zip(bytes: &[u8], target: &Path) -> AppResult<()> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|error| AppError::from(format!("Invalid pet zip archive: {error}")))?;
    if archive.len() > MAX_PET_FILES {
        return Err(AppError::from(format!(
            "Pet zip contains too many files: {}",
            archive.len()
        )));
    }
    let mut total_size = 0usize;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| AppError::from(format!("Invalid pet zip entry: {error}")))?;
        let Some(enclosed_name) = file.enclosed_name().map(|path| path.to_path_buf()) else {
            return Err(AppError::from("Pet zip contains an unsafe path"));
        };
        let out_path = target.join(enclosed_name);
        if file.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        total_size = total_size.saturating_add(file.size() as usize);
        if total_size > MAX_PET_PACKAGE_BYTES {
            return Err(AppError::from("Pet zip extracted size is too large"));
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .map_err(|error| AppError::from(format!("Failed to read pet zip entry: {error}")))?;
        std::fs::write(out_path, contents)?;
    }
    Ok(())
}

fn resolve_ccchan_root(app: &AppHandle) -> AppResult<PathBuf> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let root = resource_dir.join("resources").join("ccchan");
        if root.exists() {
            return Ok(root);
        }
    }

    let cwd = std::env::current_dir()?;
    for candidate in [
        cwd.join("src-tauri").join("resources").join("ccchan"),
        cwd.join("resources").join("ccchan"),
    ] {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(AppError::from("ccchan resources not found"))
}

fn file_asset_url(path: &Path) -> String {
    let path_text = path.to_string_lossy();
    let encoded = urlencoding::encode(&path_text);
    if cfg!(windows) {
        format!("http://asset.localhost/{encoded}")
    } else {
        format!("asset://localhost/{encoded}")
    }
}

fn current_epoch_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
