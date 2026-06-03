//! ccchan mascot backend service.
//!
//! Sprite attribution: Homie spritesheet from oc-claw (MIT), Copyright (c) rainnoon.

use crate::models::settings::CCChanSettings;
use crate::models::{CliTool, LaunchProviderSelection, WslLaunchInfo};
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
    Custom,
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

struct CodexPetLink {
    name: String,
    image_url: reqwest::Url,
    description: String,
}

struct LimitedDownload {
    bytes: Vec<u8>,
    headers: reqwest::header::HeaderMap,
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
        if parsed.scheme() == "codex" {
            return self.preview_pet_from_codex_deeplink(parsed).await;
        }
        if parsed.scheme() != "https" {
            return Err(AppError::from(
                "ccchan pet URL installs require an https:// zip URL or codex://pets/install link",
            ));
        }
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

        let download = download_limited_pet_url(parsed.clone(), "Pet package").await?;

        let staging_id = uuid::Uuid::new_v4().to_string();
        let staging_dir = self.pet_staging_dir().join(&staging_id);
        std::fs::create_dir_all(&staging_dir)?;
        extract_pet_zip(&download.bytes, &staging_dir)?;

        let pet_root = find_pet_root(&staging_dir)?;
        let pet = load_pet_from_dir(&pet_root, PetSource::User)?;
        Ok(PetInstallPreview {
            staging_id,
            pet,
            source_path: parsed.to_string(),
        })
    }

    async fn preview_pet_from_codex_deeplink(
        &self,
        parsed: reqwest::Url,
    ) -> AppResult<PetInstallPreview> {
        let link = parse_codex_pet_link(&parsed)?;
        let download = download_limited_pet_url(link.image_url.clone(), "Pet image").await?;
        let image_ext =
            pet_image_extension_from_download(&link.image_url, &download.headers, &download.bytes)?;

        let pet_id = sanitized_pet_dir_name(&link.name)?;
        let staging_id = uuid::Uuid::new_v4().to_string();
        let staging_dir = self.pet_staging_dir().join(&staging_id);
        let pet_dir = staging_dir.join(&pet_id);
        std::fs::create_dir_all(&pet_dir)?;
        let sprite_name = format!("spritesheet.{image_ext}");
        std::fs::write(pet_dir.join(&sprite_name), download.bytes)?;
        write_single_frame_pet_json(
            &pet_dir,
            &pet_id,
            &link.name,
            &link.description,
            &sprite_name,
        )?;

        let pet = load_pet_from_dir(&pet_dir, PetSource::User)?;
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
        let pet = self.install_pet_dir(&pet_root)?;
        if let Err(error) = std::fs::remove_dir_all(&staging_dir) {
            warn!(
                path = %staging_dir.display(),
                error = %error,
                "failed to remove ccchan pet staging directory after install"
            );
        }
        Ok(pet)
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

    pub fn delete_user_pet(&self, pet_id: String) -> AppResult<()> {
        let requested_pet_id = pet_id.trim().to_string();
        let pet_dir_name = sanitized_pet_dir_name(&requested_pet_id)?;
        let pet_dir = self.user_pets_dir().join(&pet_dir_name);
        if !pet_dir.exists() {
            return Err(AppError::NotFound(format!(
                "ccchan user pet '{}' not found",
                requested_pet_id
            )));
        }
        let installed = load_pet_from_dir(&pet_dir, PetSource::User)?;
        if installed.id != requested_pet_id {
            return Err(AppError::from(format!(
                "ccchan user pet id mismatch: requested '{}', found '{}'",
                requested_pet_id, installed.id
            )));
        }
        std::fs::remove_dir_all(&pet_dir).map_err(|error| {
            AppError::from(format!(
                "Failed to delete ccchan user pet {}: {}",
                pet_dir.display(),
                error
            ))
        })
    }

    pub fn start_chat(
        &self,
        terminal_service: Arc<TerminalService>,
        ai_engine: String,
        system_prompt: Option<String>,
        runtime_kind: Option<String>,
        wsl_remote_path: Option<String>,
        wsl_distro: Option<String>,
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
        let runtime_kind = runtime_kind
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("local");
        let wsl_launch = build_ccchan_wsl_launch(runtime_kind, wsl_remote_path, wsl_distro)?;
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
            wsl_launch.as_ref(),
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
        terminal_service.submit_text_to_session(session_id, text)?;
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

        if settings.pet_sources.user {
            for pet in self.load_pet_dir_children(&self.user_pets_dir(), PetSource::User)? {
                pets.insert(pet.id.clone(), pet);
            }
        }

        for dir in &settings.custom_pet_dirs {
            match self.load_pet_dir_children(Path::new(dir), PetSource::Custom) {
                Ok(custom_pets) => {
                    for pet in custom_pets {
                        pets.entry(pet.id.clone()).or_insert(pet);
                    }
                }
                Err(error) => {
                    warn!(
                        path = %dir,
                        error = %error,
                        "skipping unreadable ccchan custom pet directory"
                    );
                }
            }
        }

        if settings.pet_sources.codex_home {
            if let Some(codex_pets_dir) = codex_home_pets_dir() {
                for pet in self.load_pet_dir_children(&codex_pets_dir, PetSource::CodexHome)? {
                    pets.entry(pet.id.clone()).or_insert(pet);
                }
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
        let pet_dir_name = sanitized_pet_dir_name(&pet.id)?;
        let pets_dir = self.user_pets_dir();
        std::fs::create_dir_all(&pets_dir)?;
        let target = pets_dir.join(&pet_dir_name);
        let temp_target = pets_dir.join(format!(".{}-{}", pet_dir_name, uuid::Uuid::new_v4()));
        if temp_target.exists() {
            std::fs::remove_dir_all(&temp_target).map_err(|error| {
                AppError::from(format!(
                    "Failed to clear pet install temp {}: {}",
                    temp_target.display(),
                    error
                ))
            })?;
        }
        if let Err(error) = copy_pet_dir(pet_root, &temp_target) {
            let _ = std::fs::remove_dir_all(&temp_target);
            return Err(error);
        }
        if target.exists() {
            std::fs::remove_dir_all(&target).map_err(|error| {
                AppError::from(format!(
                    "Failed to replace existing pet {}: {}",
                    target.display(),
                    error
                ))
            })?;
        }
        std::fs::rename(&temp_target, &target).map_err(|error| {
            AppError::from(format!(
                "Failed to move installed pet {} to {}: {}",
                temp_target.display(),
                target.display(),
                error
            ))
        })?;
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

fn build_ccchan_wsl_launch(
    runtime_kind: &str,
    wsl_remote_path: Option<String>,
    wsl_distro: Option<String>,
) -> AppResult<Option<WslLaunchInfo>> {
    match runtime_kind {
        "local" => Ok(None),
        "wsl" => {
            let remote_path = wsl_remote_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::from("ccchan WSL chat requires a WSL remote path"))?;
            Ok(Some(WslLaunchInfo {
                remote_path: remote_path.to_string(),
                workspace_remote_path: None,
                distro: wsl_distro
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
            }))
        }
        other => Err(AppError::from(format!(
            "Unsupported ccchan runtimeKind '{}'; expected 'local' or 'wsl'",
            other
        ))),
    }
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
        PetSource::Custom => 2,
        PetSource::CodexHome => 3,
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
        let relative_path = safe_relative_pet_path(configured)?;
        vec![pet_dir.join(relative_path)]
    };
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| AppError::from(format!("No spritesheet found in {}", pet_dir.display())))
}

fn parse_codex_pet_link(parsed: &reqwest::Url) -> AppResult<CodexPetLink> {
    if parsed.host_str() != Some("pets") || parsed.path() != "/install" {
        return Err(AppError::from(
            "ccchan only supports codex://pets/install pet links",
        ));
    }
    let name = codex_pet_query_value(parsed, "name")
        .ok_or_else(|| AppError::from("codex pet install link requires name="))?;
    let image_url = codex_pet_query_value(parsed, "imageUrl")
        .ok_or_else(|| AppError::from("codex pet install link requires imageUrl="))?;
    let description = codex_pet_query_value(parsed, "description")
        .unwrap_or_else(|| format!("{name} imported from a Codex pet link"));

    let image_url = reqwest::Url::parse(&image_url)
        .map_err(|error| AppError::from(format!("Invalid codex pet imageUrl: {error}")))?;
    if image_url.scheme() != "https" {
        return Err(AppError::from(
            "codex pet install imageUrl must be an https:// URL",
        ));
    }

    Ok(CodexPetLink {
        name,
        image_url,
        description,
    })
}

fn codex_pet_query_value(parsed: &reqwest::Url, key: &str) -> Option<String> {
    parsed
        .query_pairs()
        .find_map(|(item_key, value)| (item_key == key).then(|| value.into_owned()))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn download_limited_pet_url(
    url: reqwest::Url,
    label: &'static str,
) -> AppResult<LimitedDownload> {
    let mut response = reqwest::get(url)
        .await
        .map_err(|error| AppError::from(format!("Failed to download pet asset: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::from(format!(
            "Failed to download pet asset: HTTP {}",
            response.status()
        )));
    }
    if let Some(length) = response.content_length() {
        if length > MAX_PET_PACKAGE_BYTES as u64 {
            return Err(AppError::from(format!(
                "{label} is too large: {length} bytes"
            )));
        }
    }

    let headers = response.headers().clone();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| AppError::from(format!("Failed to read pet asset: {error}")))?
    {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_PET_PACKAGE_BYTES {
            return Err(AppError::from(format!(
                "{label} is too large: {} bytes",
                bytes.len()
            )));
        }
    }
    Ok(LimitedDownload { bytes, headers })
}

fn pet_image_extension_from_download(
    url: &reqwest::Url,
    headers: &reqwest::header::HeaderMap,
    bytes: &[u8],
) -> AppResult<&'static str> {
    if let Some(ext) = pet_image_extension_from_magic(bytes) {
        return Ok(ext);
    }
    if let Some(ext) = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(pet_image_extension_from_content_type)
    {
        return Ok(ext);
    }
    pet_image_extension_from_url(url)
}

fn pet_image_extension_from_content_type(content_type: &str) -> Option<&'static str> {
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match media_type.as_str() {
        "image/webp" => Some("webp"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        _ => None,
    }
}

fn pet_image_extension_from_magic(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some("webp");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("jpg");
    }
    None
}

fn pet_image_extension_from_url(url: &reqwest::Url) -> AppResult<&'static str> {
    let ext = Path::new(url.path())
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "webp" => Ok("webp"),
        "png" => Ok("png"),
        "gif" => Ok("gif"),
        "jpg" | "jpeg" => Ok("jpg"),
        _ => Err(AppError::from(
            "codex pet imageUrl must end with .webp, .png, .gif, .jpg, or .jpeg",
        )),
    }
}

fn write_single_frame_pet_json(
    pet_dir: &Path,
    pet_id: &str,
    display_name: &str,
    description: &str,
    sprite_name: &str,
) -> AppResult<()> {
    let pet_json = serde_json::json!({
        "id": pet_id,
        "displayName": display_name,
        "description": description,
        "spritesheetPath": sprite_name,
        "atlas": { "cellW": 192, "cellH": 208, "cols": 1, "rows": 1 },
        "animations": {
            "idle": { "row": 0, "frames": 1, "fps": 1 },
            "working": { "row": 0, "frames": 1, "fps": 1 },
            "waiting": { "row": 0, "frames": 1, "fps": 1 },
            "happy": { "row": 0, "frames": 1, "fps": 1 },
            "sad": { "row": 0, "frames": 1, "fps": 1 }
        }
    });
    std::fs::write(
        pet_dir.join("pet.json"),
        serde_json::to_string_pretty(&pet_json)?,
    )?;
    Ok(())
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

fn sanitized_pet_dir_name(pet_id: &str) -> AppResult<String> {
    let dir_name = sanitize_path_segment(pet_id);
    if dir_name.is_empty() {
        return Err(AppError::from(format!(
            "ccchan pet id '{}' cannot be used as an install directory",
            pet_id
        )));
    }
    Ok(dir_name)
}

fn safe_relative_pet_path(value: &str) -> AppResult<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::from(format!(
            "ccchan pet asset path must stay inside the pet folder: {}",
            value
        )));
    }
    Ok(path.to_path_buf())
}

fn copy_pet_dir(source: &Path, target: &Path) -> AppResult<()> {
    let mut limits = PetCopyLimits::default();
    copy_pet_dir_limited(source, target, &mut limits)
}

#[derive(Default)]
struct PetCopyLimits {
    files: usize,
    bytes: usize,
}

fn copy_pet_dir_limited(source: &Path, target: &Path, limits: &mut PetCopyLimits) -> AppResult<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source).map_err(|error| {
        AppError::from(format!(
            "Failed to read pet directory {}: {}",
            source.display(),
            error
        ))
    })? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if file_type.is_symlink() {
            return Err(AppError::from(format!(
                "ccchan pet folders cannot contain symlinks: {}",
                source_path.display()
            )));
        }
        if file_type.is_dir() {
            copy_pet_dir_limited(&source_path, &target_path, limits)?;
        } else if file_type.is_file() {
            limits.files = limits.files.saturating_add(1);
            if limits.files > MAX_PET_FILES {
                return Err(AppError::from(format!(
                    "Pet folder contains too many files: {}",
                    limits.files
                )));
            }
            let size = entry.metadata()?.len() as usize;
            limits.bytes = limits.bytes.saturating_add(size);
            if limits.bytes > MAX_PET_PACKAGE_BYTES {
                return Err(AppError::from("Pet folder total size is too large"));
            }
            std::fs::copy(&source_path, &target_path).map_err(|error| {
                AppError::from(format!(
                    "Failed to copy pet file {}: {}",
                    source_path.display(),
                    error
                ))
            })?;
        } else {
            return Err(AppError::from(format!(
                "ccchan pet folders can only contain files and directories: {}",
                source_path.display()
            )));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    fn write_minimal_pet(dir: &Path, id: &str) {
        std::fs::create_dir_all(dir).expect("create pet dir");
        std::fs::write(
            dir.join("pet.json"),
            format!(
                r#"{{
                  "id": "{id}",
                  "displayName": "",
                  "description": "",
                  "spritesheetPath": "spritesheet.webp"
                }}"#
            ),
        )
        .expect("write pet.json");
        std::fs::write(dir.join("spritesheet.webp"), [1_u8, 2, 3]).expect("write sprite");
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        for (path, bytes) in entries {
            zip.start_file(*path, SimpleFileOptions::default())
                .expect("start file");
            zip.write_all(bytes).expect("write file");
        }
        zip.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn load_pet_from_dir_applies_codex_defaults() {
        let temp = tempdir().expect("tempdir");
        let pet_dir = temp.path().join("sample");
        write_minimal_pet(&pet_dir, "sample");

        let pet = load_pet_from_dir(&pet_dir, PetSource::User).expect("load pet");

        assert_eq!(pet.id, "sample");
        assert_eq!(pet.display_name, "sample");
        assert_eq!(pet.description, "Custom ccchan pet");
        assert_eq!(pet.source, PetSource::User);
        assert_eq!(pet.atlas.cell_w, 192);
        assert!(pet.animations.contains_key("working"));
    }

    #[test]
    fn find_pet_root_accepts_parent_folder_with_single_pet_child() {
        let temp = tempdir().expect("tempdir");
        let pet_dir = temp.path().join("nested").join("sample");
        write_minimal_pet(&pet_dir, "sample");

        let root = find_pet_root(&temp.path().join("nested")).expect("find pet root");

        assert_eq!(root, pet_dir);
    }

    #[test]
    fn extract_pet_zip_rejects_unsafe_paths() {
        let temp = tempdir().expect("tempdir");
        let bytes = zip_bytes(&[("../pet.json", b"{}")]);

        let error = extract_pet_zip(&bytes, temp.path()).expect_err("zip slip rejected");

        assert!(error.to_string().contains("unsafe path"));
    }

    #[test]
    fn extract_pet_zip_extracts_valid_pet_package() {
        let temp = tempdir().expect("tempdir");
        let bytes = zip_bytes(&[
            (
                "sample/pet.json",
                br#"{"id":"sample","spritesheetPath":"spritesheet.webp"}"#,
            ),
            ("sample/spritesheet.webp", &[1_u8, 2, 3]),
        ]);

        extract_pet_zip(&bytes, temp.path()).expect("extract zip");

        let root = find_pet_root(temp.path()).expect("find pet root");
        let pet = load_pet_from_dir(&root, PetSource::User).expect("load pet");
        assert_eq!(pet.id, "sample");
    }

    #[test]
    fn copy_pet_dir_rejects_too_many_files() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        std::fs::create_dir_all(&source).expect("create source");
        for index in 0..=MAX_PET_FILES {
            std::fs::write(source.join(format!("{index}.txt")), b"x").expect("write file");
        }

        let error = copy_pet_dir(&source, &target).expect_err("too many files rejected");

        assert!(error.to_string().contains("too many files"));
    }

    #[cfg(unix)]
    #[test]
    fn copy_pet_dir_rejects_symlinks() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        std::fs::create_dir_all(&source).expect("create source");
        std::fs::write(source.join("sprite.webp"), b"x").expect("write file");
        std::os::unix::fs::symlink(source.join("sprite.webp"), source.join("linked.webp"))
            .expect("create symlink");

        let error = copy_pet_dir(&source, &target).expect_err("symlink rejected");

        assert!(error.to_string().contains("symlinks"));
    }

    #[test]
    fn sanitize_path_segment_blocks_path_separators() {
        assert_eq!(sanitize_path_segment("../sample"), "sample");
        assert_eq!(sanitize_path_segment("sample/pet"), "sample-pet");
        assert_eq!(sanitize_path_segment("sample\\pet"), "sample-pet");
    }

    #[test]
    fn sanitized_pet_dir_name_rejects_unusable_ids() {
        let error = sanitized_pet_dir_name("猫").expect_err("non ascii id should not install");

        assert!(error.to_string().contains("install directory"));
    }

    #[test]
    fn safe_relative_pet_path_rejects_paths_outside_pet_folder() {
        assert!(safe_relative_pet_path("spritesheet.webp").is_ok());
        assert!(safe_relative_pet_path("assets/spritesheet.webp").is_ok());
        assert!(safe_relative_pet_path("../spritesheet.webp").is_err());
        assert!(safe_relative_pet_path("/tmp/spritesheet.webp").is_err());
    }

    #[test]
    fn pet_image_extension_from_url_allows_codex_pet_images() {
        let webp = reqwest::Url::parse("https://example.invalid/pet.webp").expect("url");
        let jpeg = reqwest::Url::parse("https://example.invalid/pet.jpeg").expect("url");
        let svg = reqwest::Url::parse("https://example.invalid/pet.svg").expect("url");

        assert_eq!(pet_image_extension_from_url(&webp).expect("webp"), "webp");
        assert_eq!(pet_image_extension_from_url(&jpeg).expect("jpeg"), "jpg");
        assert!(pet_image_extension_from_url(&svg).is_err());
    }

    #[test]
    fn parse_codex_pet_link_requires_https_image_url() {
        let link = reqwest::Url::parse(
            "codex://pets/install?name=Doro&imageUrl=https%3A%2F%2Fexample.invalid%2Fdoro.webp&description=Hi",
        )
        .expect("url");
        let parsed = parse_codex_pet_link(&link).expect("parse link");

        assert_eq!(parsed.name, "Doro");
        assert_eq!(
            parsed.image_url.as_str(),
            "https://example.invalid/doro.webp"
        );
        assert_eq!(parsed.description, "Hi");

        let insecure = reqwest::Url::parse(
            "codex://pets/install?name=Doro&imageUrl=http://example.invalid/doro.webp",
        )
        .expect("url");
        assert!(parse_codex_pet_link(&insecure)
            .expect_err("http imageUrl rejected")
            .to_string()
            .contains("https"));
    }

    #[test]
    fn pet_image_extension_from_download_prefers_mime_and_magic_before_url() {
        let no_ext = reqwest::Url::parse("https://example.invalid/pet").expect("url");
        let svg = reqwest::Url::parse("https://example.invalid/pet.svg").expect("url");
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("image/png; charset=binary"),
        );

        assert_eq!(
            pet_image_extension_from_download(&no_ext, &headers, b"").expect("mime"),
            "png"
        );
        headers.clear();
        assert_eq!(
            pet_image_extension_from_download(&svg, &headers, b"RIFFxxxxWEBP").expect("magic"),
            "webp"
        );
    }

    #[test]
    fn write_single_frame_pet_json_creates_loadable_pet() {
        let temp = tempdir().expect("tempdir");
        let pet_dir = temp.path().join("pet");
        std::fs::create_dir_all(&pet_dir).expect("create pet dir");
        std::fs::write(pet_dir.join("spritesheet.png"), [1_u8, 2, 3]).expect("write sprite");

        write_single_frame_pet_json(
            &pet_dir,
            "pet-id",
            "Pet Name",
            "Pet description",
            "spritesheet.png",
        )
        .expect("write json");
        let pet = load_pet_from_dir(&pet_dir, PetSource::User).expect("load pet");

        assert_eq!(pet.id, "pet-id");
        assert_eq!(pet.display_name, "Pet Name");
        assert_eq!(pet.atlas.cols, 1);
        assert_eq!(pet.animations["idle"].frames, 1);
    }

    #[test]
    fn load_pet_from_dir_rejects_spritesheet_path_traversal() {
        let temp = tempdir().expect("tempdir");
        let pet_dir = temp.path().join("sample");
        std::fs::create_dir_all(&pet_dir).expect("create pet dir");
        std::fs::write(
            pet_dir.join("pet.json"),
            br#"{"id":"sample","spritesheetPath":"../spritesheet.webp"}"#,
        )
        .expect("write pet.json");
        std::fs::write(temp.path().join("spritesheet.webp"), [1_u8, 2, 3]).expect("write sprite");

        let error = load_pet_from_dir(&pet_dir, PetSource::User).expect_err("reject traversal");

        assert!(error.to_string().contains("inside the pet folder"));
    }

    #[test]
    fn load_pet_from_dir_uses_folder_name_when_id_missing() {
        let temp = tempdir().expect("tempdir");
        let pet_dir = temp.path().join("folder-id");
        std::fs::create_dir_all(&pet_dir).expect("create pet dir");
        std::fs::write(
            pet_dir.join("pet.json"),
            br#"{"spritesheetPath":"spritesheet.webp"}"#,
        )
        .expect("write pet.json");
        std::fs::write(pet_dir.join("spritesheet.webp"), [1_u8, 2, 3]).expect("write sprite");

        let pet = load_pet_from_dir(&pet_dir, PetSource::User).expect("load pet");

        assert_eq!(pet.id, "folder-id");
        assert_eq!(pet.display_name, "folder-id");
    }

    #[test]
    fn build_ccchan_wsl_launch_requires_remote_path() {
        let error = build_ccchan_wsl_launch("wsl", None, None).expect_err("remote path required");

        assert!(error.to_string().contains("WSL remote path"));
    }

    #[test]
    fn build_ccchan_wsl_launch_trims_values() {
        let launch = build_ccchan_wsl_launch(
            "wsl",
            Some(" /home/dev/repo ".to_string()),
            Some(" Ubuntu ".to_string()),
        )
        .expect("build wsl")
        .expect("wsl launch");

        assert_eq!(launch.remote_path, "/home/dev/repo");
        assert_eq!(launch.distro.as_deref(), Some("Ubuntu"));
    }
}
