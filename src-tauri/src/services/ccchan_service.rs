//! ccchan mascot backend service.
//!
//! Sprite attribution: Homie spritesheet from oc-claw (MIT), Copyright (c) rainnoon.

use crate::models::settings::CCChanSettings;
use crate::models::{CliTool, LaunchProviderSelection, WslLaunchInfo};
use crate::services::{SettingsService, TerminalService};
use crate::utils::{AppError, AppPaths, AppResult};
use cc_panes_core::events::SessionNotifier;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::{debug, warn};

const CCCHAN_WINDOW_LABEL: &str = "ccchan";
const CCCHAN_EVENT: &str = "ccchan-event";
const CCCHAN_SETTINGS_UPDATED_EVENT: &str = "ccchan:settings-updated";
const CCCHAN_HELPER_PROMPT: &str =
    include_str!("../../resources/claude-bundle/default-skills/ccchan-helper.md");
const MAX_PET_PACKAGE_BYTES: usize = 30 * 1024 * 1024;
const MAX_PET_FILES: usize = 128;
const PET_DOWNLOAD_TIMEOUT_SECS: u64 = 30;
const PET_DOWNLOAD_MAX_REDIRECTS: usize = 5;
const AWESOME_CODEX_PET_BASE_URL: &str =
    "https://raw.githubusercontent.com/legeling/awesome-codex-pet/main";

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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwesomeCodexPetEntry {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default, rename = "author_handle")]
    pub author_handle: String,
    #[serde(default, rename = "author_url")]
    pub author_url: String,
    #[serde(default, rename = "primary_category")]
    pub primary_category: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub description: String,
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

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CustomPetDirStatus {
    pub path: String,
    pub status: String,
    pub pet_count: usize,
    pub message: String,
}

#[derive(Debug)]
struct PetInstallLink {
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
    chat_lifecycle_lock: Mutex<()>,
    chat_session_id: Mutex<Option<String>>,
}

impl CCChanService {
    pub fn new(settings_service: Arc<SettingsService>, app_paths: Arc<AppPaths>) -> Self {
        Self {
            settings_service,
            app_paths,
            app_handle: Mutex::new(None),
            chat_lifecycle_lock: Mutex::new(()),
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

    pub fn emit_settings_updated(&self) {
        let app_handle = self
            .app_handle
            .lock()
            .ok()
            .and_then(|handle| handle.clone());
        let Some(app) = app_handle else {
            debug!("ccchan settings update skipped before app handle is set");
            return;
        };
        if let Err(error) = app.emit(CCCHAN_SETTINGS_UPDATED_EVENT, serde_json::json!({})) {
            warn!(error = %error, "failed to emit ccchan settings update");
        }
    }

    pub fn save_settings(&self, settings: CCChanSettings) -> AppResult<()> {
        let mut app_settings = self.settings_service.get_settings();
        app_settings.ccchan = settings;
        self.settings_service.update_settings(app_settings)?;
        self.emit_settings_updated();
        Ok(())
    }

    pub fn show_window(&self, app: &AppHandle) -> AppResult<()> {
        self.show_window_inner(app)?;
        self.set_window_visible(true)?;
        Ok(())
    }

    pub fn hide_window(&self, app: &AppHandle) -> AppResult<()> {
        self.hide_window_inner(app)?;
        self.set_window_visible(false)?;
        Ok(())
    }

    pub fn sync_saved_window_visibility(
        &self,
        app: &AppHandle,
        was_visible: bool,
        settings: &CCChanSettings,
    ) -> AppResult<()> {
        match (was_visible, settings.window_visible) {
            (false, true) => self.show_window_with_settings(app, settings)?,
            (true, false) => self.hide_window_inner(app)?,
            _ => {}
        }
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

    pub async fn list_awesome_codex_pets(&self) -> AppResult<Vec<AwesomeCodexPetEntry>> {
        let catalog_url = awesome_codex_pet_url("pets.json")?;
        let download = download_limited_pet_url(catalog_url, "Awesome Codex pet catalog").await?;
        let mut entries: Vec<AwesomeCodexPetEntry> = serde_json::from_slice(&download.bytes)
            .map_err(|error| {
                AppError::from(format!("Invalid Awesome Codex pet catalog: {error}"))
            })?;
        entries.retain(|entry| awesome_codex_pet_slug_is_safe(&entry.slug));
        entries.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.slug.cmp(&right.slug))
        });
        Ok(entries)
    }

    pub async fn preview_awesome_codex_pet(&self, slug: String) -> AppResult<PetInstallPreview> {
        let slug = normalize_awesome_codex_pet_slug(&slug)?;
        let pet_json_url = awesome_codex_pet_url(&format!("pets/{slug}/pet.json"))?;
        let pet_json =
            download_limited_pet_url(pet_json_url.clone(), "Awesome Codex pet metadata").await?;
        let definition: PetDefinition = serde_json::from_slice(&pet_json.bytes)
            .map_err(|error| AppError::from(format!("Invalid Awesome Codex pet.json: {error}")))?;
        let sprite_path = awesome_codex_sprite_path(&definition.spritesheet_path)?;
        let sprite_url =
            awesome_codex_pet_url(&format!("pets/{slug}/{}", sprite_path.to_string_lossy()))?;
        let sprite = download_limited_pet_url(sprite_url, "Awesome Codex pet spritesheet").await?;

        let staging_id = uuid::Uuid::new_v4().to_string();
        let staging_dir = self.pet_staging_dir().join(&staging_id);
        let pet_dir = staging_dir.join(&slug);
        std::fs::create_dir_all(&pet_dir)?;
        std::fs::write(pet_dir.join("pet.json"), pet_json.bytes)?;
        let sprite_target = pet_dir.join(&sprite_path);
        if let Some(parent) = sprite_target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(sprite_target, sprite.bytes)?;

        let pet = load_pet_from_dir(&pet_dir, PetSource::User)?;
        Ok(PetInstallPreview {
            staging_id,
            pet,
            source_path: pet_json_url.to_string(),
        })
    }

    pub async fn preview_pet_from_url(&self, url: String) -> AppResult<PetInstallPreview> {
        let parsed = reqwest::Url::parse(url.trim()).map_err(|error| {
            AppError::from(format!(
                "Invalid ccchan pet URL '{}': {}",
                url.trim(),
                error
            ))
        })?;
        if matches!(parsed.scheme(), "codex" | "ccpanes") {
            return self.preview_pet_from_install_link(parsed).await;
        }
        if parsed.scheme() != "https" {
            return Err(AppError::from(
                "ccchan pet URL installs require an https:// zip URL, codex://pets/install link, or ccpanes://pets/install link",
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

    async fn preview_pet_from_install_link(
        &self,
        parsed: reqwest::Url,
    ) -> AppResult<PetInstallPreview> {
        let link = parse_pet_install_link(&parsed)?;
        let download = download_limited_pet_url(link.image_url.clone(), "Pet image").await?;
        let image_ext =
            pet_image_extension_from_download(&link.image_url, &download.headers, &download.bytes)?;

        let pet_id = pet_install_link_pet_id(&link.name, link.image_url.as_str());
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
        if !source.is_file() {
            validate_pet_dir_copy_limits(&pet_root)?;
        }
        let pet = load_pet_from_dir(&pet_root, PetSource::User)?;
        Ok(PetInstallPreview {
            staging_id,
            pet,
            source_path: source.to_string_lossy().to_string(),
        })
    }

    pub fn install_pet_from_preview(&self, staging_id: String) -> AppResult<PetMeta> {
        let staging_id = sanitize_existing_staging_id(&staging_id)?;
        let staging_dir = self.pet_staging_dir().join(staging_id);
        let result = (|| {
            let pet_root = find_pet_root(&staging_dir)?;
            let pet = self.install_pet_dir(&pet_root)?;
            self.emit_settings_updated();
            Ok(pet)
        })();
        if staging_dir.exists() {
            if let Err(error) = std::fs::remove_dir_all(&staging_dir) {
                warn!(
                    path = %staging_dir.display(),
                    error = %error,
                    "failed to remove ccchan pet staging directory after preview install attempt"
                );
            }
        }
        result
    }

    pub fn cancel_pet_preview(&self, staging_id: String) -> AppResult<()> {
        if staging_id.trim().is_empty() {
            return Ok(());
        }
        let staging_id = sanitize_existing_staging_id(&staging_id)?;
        let staging_dir = self.pet_staging_dir().join(staging_id);
        if !staging_dir.exists() {
            return Ok(());
        }
        std::fs::remove_dir_all(&staging_dir).map_err(|error| {
            AppError::from(format!(
                "Failed to remove ccchan pet staging directory {}: {}",
                staging_dir.display(),
                error
            ))
        })
    }

    pub fn install_pet_from_path(&self, path: String) -> AppResult<PetMeta> {
        let source = PathBuf::from(path.trim());
        let mut staging_dir_to_cleanup = None;
        let result = (|| {
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
                staging_dir_to_cleanup = Some(staging_dir.clone());
                extract_pet_zip(&bytes, &staging_dir)?;
                find_pet_root(&staging_dir)?
            } else {
                find_pet_root(&source)?
            };
            let pet = self.install_pet_dir(&pet_root)?;
            self.emit_settings_updated();
            Ok(pet)
        })();
        if let Some(staging_dir) = staging_dir_to_cleanup {
            if let Err(error) = std::fs::remove_dir_all(&staging_dir) {
                warn!(
                    path = %staging_dir.display(),
                    error = %error,
                    "failed to remove ccchan pet staging directory after direct install"
                );
            }
        }
        result
    }

    pub fn install_pet_from_source(&self, pet_id: String, source: String) -> AppResult<PetMeta> {
        let source = parse_readonly_pet_source(&source)?;
        let pet_dir = self.find_readonly_pet_dir(&pet_id, source)?;
        let pet = self.install_pet_dir(&pet_dir)?;
        self.emit_settings_updated();
        Ok(pet)
    }

    pub fn custom_pet_dir_statuses(&self) -> Vec<CustomPetDirStatus> {
        self.settings()
            .custom_pet_dirs
            .iter()
            .map(|dir| custom_pet_dir_status(Path::new(dir), dir))
            .collect()
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
        })?;
        self.emit_settings_updated();
        Ok(())
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
        let _lifecycle = self
            .chat_lifecycle_lock
            .lock()
            .map_err(|_| AppError::from("ccchan chat lifecycle lock poisoned"))?;
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
        let _lifecycle = self
            .chat_lifecycle_lock
            .lock()
            .map_err(|_| AppError::from("ccchan chat lifecycle lock poisoned"))?;
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

    fn show_window_inner(&self, app: &AppHandle) -> AppResult<()> {
        let settings = self.settings();
        self.show_window_with_settings(app, &settings)
    }

    fn show_window_with_settings(
        &self,
        app: &AppHandle,
        settings: &CCChanSettings,
    ) -> AppResult<()> {
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
        position_window(&window, settings)?;
        window
            .show()
            .map_err(|error| AppError::from(error.to_string()))
    }

    fn hide_window_inner(&self, app: &AppHandle) -> AppResult<()> {
        let window = ccchan_window(app)?;
        window
            .hide()
            .map_err(|error| AppError::from(error.to_string()))
    }

    fn discover_pets(&self, app: &AppHandle) -> AppResult<Vec<PetMeta>> {
        let mut pets: HashMap<String, PetMeta> = HashMap::new();
        let settings = self.settings();

        if settings.pet_sources.builtin {
            let root = resolve_ccchan_root(app)?;
            for pet in load_manifest_pets(&root, PetSource::Builtin)? {
                merge_pet_by_source_rank(&mut pets, pet);
            }
        }

        if settings.pet_sources.user {
            for pet in self.load_pet_dir_children(&self.user_pets_dir(), PetSource::User)? {
                merge_pet_by_source_rank(&mut pets, pet);
            }
        }

        for dir in &settings.custom_pet_dirs {
            match self.load_pet_dir_children(Path::new(dir), PetSource::Custom) {
                Ok(custom_pets) => {
                    for pet in custom_pets {
                        merge_pet_by_source_rank(&mut pets, pet);
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
                    merge_pet_by_source_rank(&mut pets, pet);
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
        if root.join("pet.json").exists() {
            return Ok(vec![load_pet_from_dir(root, source)?]);
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

    fn find_readonly_pet_dir(&self, pet_id: &str, source: PetSource) -> AppResult<PathBuf> {
        let requested_pet_id = pet_id.trim();
        if requested_pet_id.is_empty() {
            return Err(AppError::from("ccchan pet id is required"));
        }
        match source {
            PetSource::Custom => {
                for dir in &self.settings().custom_pet_dirs {
                    if let Some(path) = find_pet_dir_by_id_in_root(
                        Path::new(dir),
                        PetSource::Custom,
                        requested_pet_id,
                    )? {
                        return Ok(path);
                    }
                }
                Err(AppError::NotFound(format!(
                    "ccchan custom pet '{}' not found",
                    requested_pet_id
                )))
            }
            PetSource::CodexHome => {
                let Some(codex_pets_dir) = codex_home_pets_dir() else {
                    return Err(AppError::NotFound(
                        "Codex Home pets directory was not found".to_string(),
                    ));
                };
                find_pet_dir_by_id_in_root(&codex_pets_dir, PetSource::CodexHome, requested_pet_id)?
                    .ok_or_else(|| {
                        AppError::NotFound(format!(
                            "ccchan Codex Home pet '{}' not found",
                            requested_pet_id
                        ))
                    })
            }
            PetSource::Builtin => Err(AppError::from(
                "Bundled ccchan pets are already available and cannot be installed from source",
            )),
            PetSource::User => Err(AppError::from(
                "User-installed ccchan pets are already installed",
            )),
        }
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

        let title = ccchan_session_title(session_id);
        let payload = serde_json::json!({
            "kind": kind,
            "sessionId": session_id,
            "title": title,
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
            if !remote_path.starts_with('/') {
                return Err(AppError::from(format!(
                    "ccchan WSL chat remote path must start with /: {remote_path}"
                )));
            }
            Ok(Some(WslLaunchInfo {
                remote_path: remote_path.to_string(),
                workspace_remote_path: None,
                hook_sync_project_path: Some(remote_path.to_string()),
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

fn merge_pet_by_source_rank(pets: &mut HashMap<String, PetMeta>, pet: PetMeta) {
    let should_insert = pets
        .get(&pet.id)
        .is_none_or(|existing| source_rank(pet.source) < source_rank(existing.source));
    if should_insert {
        pets.insert(pet.id.clone(), pet);
    }
}

fn custom_pet_dir_status(root: &Path, original_path: &str) -> CustomPetDirStatus {
    if !root.exists() {
        return CustomPetDirStatus {
            path: original_path.to_string(),
            status: "missing".to_string(),
            pet_count: 0,
            message: "目录不存在".to_string(),
        };
    }
    if !root.is_dir() {
        return CustomPetDirStatus {
            path: original_path.to_string(),
            status: "invalid".to_string(),
            pet_count: 0,
            message: "路径不是目录".to_string(),
        };
    }
    if root.join("pet.json").exists() {
        return match load_pet_from_dir(root, PetSource::Custom) {
            Ok(pet) => CustomPetDirStatus {
                path: original_path.to_string(),
                status: "ready".to_string(),
                pet_count: 1,
                message: format!("单个宠物目录：{} ({})", pet.display_name, pet.id),
            },
            Err(error) => CustomPetDirStatus {
                path: original_path.to_string(),
                status: "invalid".to_string(),
                pet_count: 0,
                message: error.to_string(),
            },
        };
    }

    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            return CustomPetDirStatus {
                path: original_path.to_string(),
                status: "invalid".to_string(),
                pet_count: 0,
                message: format!("无法读取目录: {error}"),
            };
        }
    };
    let mut pet_count = 0usize;
    let mut invalid_count = 0usize;
    for entry in entries {
        let Ok(entry) = entry else {
            invalid_count = invalid_count.saturating_add(1);
            continue;
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("pet.json").exists() {
            continue;
        }
        match load_pet_from_dir(&path, PetSource::Custom) {
            Ok(_) => pet_count = pet_count.saturating_add(1),
            Err(_) => invalid_count = invalid_count.saturating_add(1),
        }
    }
    if pet_count > 0 {
        return CustomPetDirStatus {
            path: original_path.to_string(),
            status: if invalid_count > 0 {
                "warning"
            } else {
                "ready"
            }
            .to_string(),
            pet_count,
            message: if invalid_count > 0 {
                format!("发现 {pet_count} 个可用宠物，另有 {invalid_count} 个无效宠物目录")
            } else {
                format!("发现 {pet_count} 个可用宠物")
            },
        };
    }
    CustomPetDirStatus {
        path: original_path.to_string(),
        status: "empty".to_string(),
        pet_count: 0,
        message: if invalid_count > 0 {
            format!("未发现可用宠物，另有 {invalid_count} 个无效宠物目录")
        } else {
            "未发现 pet.json；可选择单个宠物目录或包含多个宠物子目录的父目录".to_string()
        },
    }
}

fn parse_readonly_pet_source(source: &str) -> AppResult<PetSource> {
    match source.trim() {
        "custom" => Ok(PetSource::Custom),
        "codexHome" => Ok(PetSource::CodexHome),
        other => Err(AppError::from(format!(
            "ccchan can only install from readonly custom or codexHome pet sources, got '{}'",
            other
        ))),
    }
}

fn find_pet_dir_by_id_in_root(
    root: &Path,
    source: PetSource,
    requested_pet_id: &str,
) -> AppResult<Option<PathBuf>> {
    if !root.exists() {
        return Ok(None);
    }
    if root.join("pet.json").exists() {
        let pet = load_pet_from_dir(root, source)?;
        return Ok((pet.id == requested_pet_id).then(|| root.to_path_buf()));
    }
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
            Ok(pet) if pet.id == requested_pet_id => return Ok(Some(path)),
            Ok(_) => {}
            Err(error) => {
                warn!(path = %path.display(), error = %error, "skipping invalid ccchan pet")
            }
        }
    }
    Ok(None)
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
    let mut child_pet_roots = Vec::new();
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
            child_pet_roots.push(child);
        }
    }
    if child_pet_roots.len() == 1 {
        return Ok(child_pet_roots.remove(0));
    }
    if child_pet_roots.len() > 1 {
        return Err(AppError::from(format!(
            "Multiple pet.json children found in {}; select one pet folder instead",
            path.display()
        )));
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

fn parse_pet_install_link(parsed: &reqwest::Url) -> AppResult<PetInstallLink> {
    if !matches!(parsed.scheme(), "codex" | "ccpanes") {
        return Err(AppError::from(
            "ccchan only supports codex://pets/install or ccpanes://pets/install pet links",
        ));
    }
    let path = parsed.path().trim_end_matches('/');
    if parsed.host_str() != Some("pets") || path != "/install" {
        return Err(AppError::from(
            "ccchan only supports codex://pets/install or ccpanes://pets/install pet links",
        ));
    }
    let name = codex_pet_query_value(parsed, "name")
        .ok_or_else(|| AppError::from("pet install link requires name="))?;
    let image_url = codex_pet_query_value_any(
        parsed,
        &["imageUrl", "imageURL", "image_url", "image-url", "url"],
    )
    .ok_or_else(|| AppError::from("pet install link requires imageUrl="))?;
    let description = codex_pet_query_value_any(parsed, &["description", "desc"])
        .unwrap_or_else(|| format!("{name} imported from a pet install link"));

    let image_url = reqwest::Url::parse(&image_url)
        .map_err(|error| AppError::from(format!("Invalid pet install imageUrl: {error}")))?;
    if image_url.scheme() != "https" {
        return Err(AppError::from(
            "pet install imageUrl must be an https:// URL",
        ));
    }

    Ok(PetInstallLink {
        name,
        image_url,
        description,
    })
}

fn awesome_codex_pet_url(path: &str) -> AppResult<reqwest::Url> {
    reqwest::Url::parse(&format!(
        "{}/{}",
        AWESOME_CODEX_PET_BASE_URL,
        path.trim_start_matches('/')
    ))
    .map_err(|error| AppError::from(format!("Invalid Awesome Codex pet URL: {error}")))
}

fn awesome_codex_pet_slug_is_safe(slug: &str) -> bool {
    !slug.trim().is_empty()
        && slug.len() <= 128
        && slug
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        && !slug.starts_with('.')
        && !slug.contains("..")
}

fn normalize_awesome_codex_pet_slug(slug: &str) -> AppResult<String> {
    let trimmed = slug.trim();
    if !awesome_codex_pet_slug_is_safe(trimmed) {
        return Err(AppError::from(format!(
            "Invalid Awesome Codex pet slug: {}",
            slug
        )));
    }
    Ok(trimmed.to_string())
}

fn awesome_codex_sprite_path(path: &str) -> AppResult<PathBuf> {
    let path = safe_relative_pet_path(path)?;
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "webp" | "png" | "gif" | "jpg" | "jpeg" => Ok(path),
        _ => Err(AppError::from(
            "Awesome Codex pet spritesheet must be .webp, .png, .gif, .jpg, or .jpeg",
        )),
    }
}

fn codex_pet_query_value(parsed: &reqwest::Url, key: &str) -> Option<String> {
    parsed
        .query_pairs()
        .find_map(|(item_key, value)| (item_key == key).then(|| value.into_owned()))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn codex_pet_query_value_any(parsed: &reqwest::Url, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| codex_pet_query_value(parsed, key))
}

fn pet_install_link_pet_id(name: &str, image_url: &str) -> String {
    let sanitized = sanitize_path_segment(name);
    if !sanitized.is_empty() {
        return sanitized;
    }
    let digest = Sha256::digest(format!("{name}\n{image_url}").as_bytes());
    format!(
        "pet-{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3]
    )
}

async fn download_limited_pet_url(
    url: reqwest::Url,
    label: &'static str,
) -> AppResult<LimitedDownload> {
    let client = build_pet_download_client()?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| AppError::from(format!("Failed to download pet asset: {error}")))?;
    ensure_pet_download_url_is_https(response.url())?;
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

fn build_pet_download_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(PET_DOWNLOAD_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::custom(
            |attempt| match validate_pet_redirect(attempt.url(), attempt.previous().len()) {
                Ok(()) => attempt.follow(),
                Err(error) => attempt.error(error.to_string()),
            },
        ))
        .build()
        .map_err(|error| AppError::from(format!("Failed to create pet download client: {error}")))
}

fn validate_pet_redirect(next_url: &reqwest::Url, previous_len: usize) -> AppResult<()> {
    if previous_len > PET_DOWNLOAD_MAX_REDIRECTS {
        return Err(AppError::from(format!(
            "ccchan pet download followed too many redirects: {}",
            previous_len
        )));
    }
    ensure_pet_download_url_is_https(next_url)
}

fn ensure_pet_download_url_is_https(url: &reqwest::Url) -> AppResult<()> {
    if url.scheme() != "https" {
        return Err(AppError::from(format!(
            "ccchan pet downloads must stay on https:// URLs, got {}",
            url.as_str()
        )));
    }
    Ok(())
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
        serde_json::to_string_pretty(&pet_json)
            .map_err(|error| AppError::from(format!("Invalid generated pet.json: {error}")))?,
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

fn sanitize_existing_staging_id(staging_id: &str) -> AppResult<String> {
    let trimmed = staging_id.trim();
    let sanitized = sanitize_path_segment(trimmed);
    if sanitized.is_empty() || sanitized != trimmed {
        return Err(AppError::from(format!(
            "Invalid ccchan pet preview staging id: {}",
            staging_id
        )));
    }
    Ok(sanitized)
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

fn validate_pet_dir_copy_limits(source: &Path) -> AppResult<()> {
    let mut limits = PetCopyLimits::default();
    validate_pet_dir_copy_limits_inner(source, &mut limits)
}

fn validate_pet_dir_copy_limits_inner(source: &Path, limits: &mut PetCopyLimits) -> AppResult<()> {
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
        if file_type.is_symlink() {
            return Err(AppError::from(format!(
                "ccchan pet folders cannot contain symlinks: {}",
                source_path.display()
            )));
        }
        if file_type.is_dir() {
            validate_pet_dir_copy_limits_inner(&source_path, limits)?;
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
        } else {
            return Err(AppError::from(format!(
                "ccchan pet folders can only contain files and directories: {}",
                source_path.display()
            )));
        }
    }
    Ok(())
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

fn ccchan_session_title(session_id: &str) -> String {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return "Session".to_string();
    }
    let short: String = trimmed.chars().take(8).collect();
    format!("Session {short}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex as StdMutex;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

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

    fn pet_meta(id: &str, display_name: &str, source: PetSource) -> PetMeta {
        PetMeta {
            id: id.to_string(),
            display_name: display_name.to_string(),
            description: "test pet".to_string(),
            spritesheet_url: "asset://pet".to_string(),
            source,
            atlas: PetAtlas::default(),
            animations: default_pet_animations(),
        }
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
    fn find_pet_root_rejects_parent_folder_with_multiple_pet_children() {
        let temp = tempdir().expect("tempdir");
        write_minimal_pet(&temp.path().join("one"), "one");
        write_minimal_pet(&temp.path().join("two"), "two");

        let error = find_pet_root(temp.path()).expect_err("multiple pets rejected");

        assert!(error.to_string().contains("Multiple pet.json children"));
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
    fn pet_install_link_pet_id_falls_back_for_non_ascii_names() {
        let id = pet_install_link_pet_id("猫", "https://example.invalid/cat.webp");

        assert!(id.starts_with("pet-"));
        assert_eq!(id.len(), 12);
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
    fn parse_pet_install_link_accepts_codex_and_ccpanes_schemes() {
        let link = reqwest::Url::parse(
            "codex://pets/install?name=Doro&imageUrl=https%3A%2F%2Fexample.invalid%2Fdoro.webp&description=Hi",
        )
        .expect("url");
        let parsed = parse_pet_install_link(&link).expect("parse codex link");

        assert_eq!(parsed.name, "Doro");
        assert_eq!(
            parsed.image_url.as_str(),
            "https://example.invalid/doro.webp"
        );
        assert_eq!(parsed.description, "Hi");

        let ccpanes = reqwest::Url::parse(
            "ccpanes://pets/install/?name=Homie&image_url=https%3A%2F%2Fexample.invalid%2Fhomie.png&desc=Short",
        )
        .expect("url");
        let parsed = parse_pet_install_link(&ccpanes).expect("parse ccpanes link");
        assert_eq!(parsed.name, "Homie");
        assert_eq!(
            parsed.image_url.as_str(),
            "https://example.invalid/homie.png"
        );
        assert_eq!(parsed.description, "Short");

        let insecure = reqwest::Url::parse(
            "codex://pets/install?name=Doro&imageUrl=http://example.invalid/doro.webp",
        )
        .expect("url");
        assert!(parse_pet_install_link(&insecure)
            .expect_err("http imageUrl rejected")
            .to_string()
            .contains("https"));
    }

    #[test]
    fn awesome_codex_pet_slug_validation_blocks_path_traversal() {
        assert_eq!(
            normalize_awesome_codex_pet_slug("doro--author").expect("valid slug"),
            "doro--author"
        );
        assert!(normalize_awesome_codex_pet_slug("../doro").is_err());
        assert!(normalize_awesome_codex_pet_slug("doro/pet").is_err());
        assert!(normalize_awesome_codex_pet_slug(".hidden").is_err());
    }

    #[test]
    fn awesome_codex_sprite_path_rejects_unsafe_or_unsupported_paths() {
        assert!(awesome_codex_sprite_path("spritesheet.webp").is_ok());
        assert!(awesome_codex_sprite_path("assets/spritesheet.png").is_ok());
        assert!(awesome_codex_sprite_path("../spritesheet.webp").is_err());
        assert!(awesome_codex_sprite_path("spritesheet.svg").is_err());
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
    fn pet_download_https_guards_reject_plain_http_and_downgrades() {
        let https = reqwest::Url::parse("https://example.invalid/pet.zip").expect("https url");
        let http = reqwest::Url::parse("http://example.invalid/pet.zip").expect("http url");

        assert!(ensure_pet_download_url_is_https(&https).is_ok());
        assert!(ensure_pet_download_url_is_https(&http)
            .expect_err("http download rejected")
            .to_string()
            .contains("https"));
        assert!(validate_pet_redirect(&https, PET_DOWNLOAD_MAX_REDIRECTS).is_ok());
        assert!(validate_pet_redirect(&http, 1)
            .expect_err("http redirect rejected")
            .to_string()
            .contains("https"));
        assert!(
            validate_pet_redirect(&https, PET_DOWNLOAD_MAX_REDIRECTS + 1)
                .expect_err("redirect limit rejected")
                .to_string()
                .contains("too many redirects")
        );
        build_pet_download_client().expect("download client builds");
    }

    #[test]
    fn merge_pet_by_source_rank_prefers_user_then_builtin_then_custom_then_codex_home() {
        let mut pets = HashMap::new();

        merge_pet_by_source_rank(
            &mut pets,
            pet_meta("same", "Codex Home", PetSource::CodexHome),
        );
        merge_pet_by_source_rank(&mut pets, pet_meta("same", "Custom", PetSource::Custom));
        merge_pet_by_source_rank(&mut pets, pet_meta("same", "Builtin", PetSource::Builtin));
        merge_pet_by_source_rank(&mut pets, pet_meta("same", "User", PetSource::User));
        merge_pet_by_source_rank(
            &mut pets,
            pet_meta("same", "Another Custom", PetSource::Custom),
        );

        let pet = pets.get("same").expect("merged pet");
        assert_eq!(pet.display_name, "User");
        assert_eq!(pet.source, PetSource::User);
    }

    #[test]
    fn merge_pet_by_source_rank_keeps_first_pet_for_same_source_rank() {
        let mut pets = HashMap::new();

        merge_pet_by_source_rank(&mut pets, pet_meta("same", "First", PetSource::Custom));
        merge_pet_by_source_rank(&mut pets, pet_meta("same", "Second", PetSource::Custom));

        assert_eq!(pets["same"].display_name, "First");
    }

    #[test]
    fn load_pet_dir_children_accepts_single_pet_root() {
        let temp = tempdir().expect("tempdir");
        let pet_dir = temp.path().join("single");
        write_minimal_pet(&pet_dir, "single");
        let service = CCChanService::new(
            Arc::new(SettingsService::new()),
            Arc::new(AppPaths::new(Some(
                temp.path().join("data").to_string_lossy().to_string(),
            ))),
        );

        let pets = service
            .load_pet_dir_children(&pet_dir, PetSource::Custom)
            .expect("load single root");

        assert_eq!(pets.len(), 1);
        assert_eq!(pets[0].id, "single");
        assert_eq!(pets[0].source, PetSource::Custom);
    }

    #[test]
    fn custom_pet_dir_status_reports_single_parent_and_missing_dirs() {
        let temp = tempdir().expect("tempdir");
        let single = temp.path().join("single");
        let parent = temp.path().join("parent");
        write_minimal_pet(&single, "single");
        write_minimal_pet(&parent.join("one"), "one");
        write_minimal_pet(&parent.join("two"), "two");

        let single_status = custom_pet_dir_status(&single, "/pets/single");
        assert_eq!(single_status.status, "ready");
        assert_eq!(single_status.pet_count, 1);
        assert!(single_status.message.contains("single"));

        let parent_status = custom_pet_dir_status(&parent, "/pets/parent");
        assert_eq!(parent_status.status, "ready");
        assert_eq!(parent_status.pet_count, 2);

        let missing_status = custom_pet_dir_status(&temp.path().join("missing"), "/pets/missing");
        assert_eq!(missing_status.status, "missing");
        assert_eq!(missing_status.pet_count, 0);
    }

    #[test]
    fn install_pet_from_source_copies_custom_pet_into_user_dir() {
        let temp = tempdir().expect("tempdir");
        let data_dir = temp.path().join("data");
        let custom_root = temp.path().join("custom-pets");
        write_minimal_pet(&custom_root.join("sample"), "sample");
        let settings_service = Arc::new(SettingsService::new());
        let mut settings = settings_service.get_settings();
        settings.ccchan.custom_pet_dirs = vec![custom_root.to_string_lossy().to_string()];
        settings_service
            .update_settings(settings)
            .expect("save settings");
        let service = CCChanService::new(
            settings_service,
            Arc::new(AppPaths::new(Some(data_dir.to_string_lossy().to_string()))),
        );

        let pet = service
            .install_pet_from_source("sample".to_string(), "custom".to_string())
            .expect("install custom source pet");

        assert_eq!(pet.id, "sample");
        assert_eq!(pet.source, PetSource::User);
        assert!(data_dir.join("ccchan").join("pets").join("sample").exists());
    }

    #[test]
    fn install_pet_from_source_copies_codex_home_pet_into_user_dir() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_codex_home = std::env::var_os("CODEX_HOME");
        let temp = tempdir().expect("tempdir");
        let data_dir = temp.path().join("data");
        let codex_home = temp.path().join("codex-home");
        write_minimal_pet(&codex_home.join("pets").join("codex-pet"), "codex-pet");
        std::env::set_var("CODEX_HOME", &codex_home);
        let service = CCChanService::new(
            Arc::new(SettingsService::new()),
            Arc::new(AppPaths::new(Some(data_dir.to_string_lossy().to_string()))),
        );

        let result =
            service.install_pet_from_source("codex-pet".to_string(), "codexHome".to_string());
        match previous_codex_home {
            Some(value) => std::env::set_var("CODEX_HOME", value),
            None => std::env::remove_var("CODEX_HOME"),
        }

        let pet = result.expect("install codex home source pet");
        assert_eq!(pet.id, "codex-pet");
        assert_eq!(pet.source, PetSource::User);
        assert!(data_dir
            .join("ccchan")
            .join("pets")
            .join("codex-pet")
            .exists());
    }

    #[test]
    fn install_pet_from_source_rejects_non_readonly_sources() {
        let temp = tempdir().expect("tempdir");
        let service = CCChanService::new(
            Arc::new(SettingsService::new()),
            Arc::new(AppPaths::new(Some(
                temp.path().join("data").to_string_lossy().to_string(),
            ))),
        );

        let error = service
            .install_pet_from_source("sample".to_string(), "user".to_string())
            .expect_err("user source should be rejected");

        assert!(error.to_string().contains("readonly custom or codexHome"));
    }

    #[test]
    fn install_pet_from_path_cleans_direct_zip_staging_dir() {
        let temp = tempdir().expect("tempdir");
        let data_dir = temp.path().join("data");
        let package_path = temp.path().join("sample.zip");
        let bytes = zip_bytes(&[
            (
                "sample/pet.json",
                br#"{"id":"sample","spritesheetPath":"spritesheet.webp"}"#,
            ),
            ("sample/spritesheet.webp", &[1_u8, 2, 3]),
        ]);
        std::fs::write(&package_path, bytes).expect("write package");

        let service = CCChanService::new(
            Arc::new(SettingsService::new()),
            Arc::new(AppPaths::new(Some(data_dir.to_string_lossy().to_string()))),
        );

        let pet = service
            .install_pet_from_path(package_path.to_string_lossy().to_string())
            .expect("install pet from zip");

        assert_eq!(pet.id, "sample");
        assert!(data_dir.join("ccchan").join("pets").join("sample").exists());
        let staging_dir = data_dir.join("ccchan").join("pet-staging");
        let staging_entries = std::fs::read_dir(&staging_dir)
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(staging_entries, 0);
    }

    #[test]
    fn preview_pet_from_path_rejects_folder_before_confirm_when_copy_limits_fail() {
        let temp = tempdir().expect("tempdir");
        let data_dir = temp.path().join("data");
        let pet_dir = temp.path().join("oversized");
        write_minimal_pet(&pet_dir, "oversized");
        for index in 0..=MAX_PET_FILES {
            std::fs::write(pet_dir.join(format!("extra-{index}.txt")), b"x")
                .expect("write extra file");
        }
        let service = CCChanService::new(
            Arc::new(SettingsService::new()),
            Arc::new(AppPaths::new(Some(data_dir.to_string_lossy().to_string()))),
        );

        let error = service
            .preview_pet_from_path(pet_dir.to_string_lossy().to_string())
            .expect_err("folder limits should fail at preview time");

        assert!(error.to_string().contains("too many files"));
        assert!(!data_dir.join("ccchan").join("pet-staging").exists());
    }

    #[test]
    fn install_pet_from_preview_cleans_staging_after_install_failure() {
        let temp = tempdir().expect("tempdir");
        let data_dir = temp.path().join("data");
        let service = CCChanService::new(
            Arc::new(SettingsService::new()),
            Arc::new(AppPaths::new(Some(data_dir.to_string_lossy().to_string()))),
        );
        let staging_dir = data_dir
            .join("ccchan")
            .join("pet-staging")
            .join("stage-fail")
            .join("pet");
        write_minimal_pet(&staging_dir, "猫");

        let error = service
            .install_pet_from_preview("stage-fail".to_string())
            .expect_err("invalid pet id should fail install");

        assert!(error.to_string().contains("install directory"));
        assert!(!data_dir
            .join("ccchan")
            .join("pet-staging")
            .join("stage-fail")
            .exists());
        assert!(!data_dir.join("ccchan").join("pets").join("pet").exists());
    }

    #[test]
    fn cancel_pet_preview_rejects_unsafe_staging_ids() {
        let temp = tempdir().expect("tempdir");
        let data_dir = temp.path().join("data");
        let service = CCChanService::new(
            Arc::new(SettingsService::new()),
            Arc::new(AppPaths::new(Some(data_dir.to_string_lossy().to_string()))),
        );
        let staging_dir = data_dir.join("ccchan").join("pet-staging").join("stage-1");
        std::fs::create_dir_all(&staging_dir).expect("create staging");
        std::fs::write(staging_dir.join("pet.json"), b"{}").expect("write staging file");

        let error = service
            .cancel_pet_preview("stage-1/../../escape".to_string())
            .expect_err("unsafe staging id should be rejected");
        assert!(error
            .to_string()
            .contains("Invalid ccchan pet preview staging id"));

        assert!(
            staging_dir.exists(),
            "unsanitized sibling path should not be removed"
        );
        service
            .cancel_pet_preview("stage-1".to_string())
            .expect("cancel preview");
        assert!(!staging_dir.exists());
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
    fn ccchan_session_title_shortens_session_ids_for_notifications() {
        assert_eq!(
            ccchan_session_title("12345678-90ab-cdef"),
            "Session 12345678"
        );
        assert_eq!(ccchan_session_title("   "), "Session");
    }

    #[test]
    fn build_ccchan_wsl_launch_requires_remote_path() {
        let error = build_ccchan_wsl_launch("wsl", None, None).expect_err("remote path required");

        assert!(error.to_string().contains("WSL remote path"));
    }

    #[test]
    fn build_ccchan_wsl_launch_rejects_non_linux_remote_paths() {
        let windows_path =
            build_ccchan_wsl_launch("wsl", Some("D:\\my-project\\cc-pane".to_string()), None)
                .expect_err("Windows path rejected");
        assert!(windows_path.to_string().contains("must start with /"));

        let unc_path = build_ccchan_wsl_launch(
            "wsl",
            Some("\\\\wsl.localhost\\Ubuntu-24.04\\home\\me\\repo".to_string()),
            None,
        )
        .expect_err("UNC path rejected");
        assert!(unc_path.to_string().contains("must start with /"));

        let relative_path =
            build_ccchan_wsl_launch("wsl", Some("workspace/repo".to_string()), None)
                .expect_err("relative path rejected");
        assert!(relative_path.to_string().contains("must start with /"));
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

        let home =
            build_ccchan_wsl_launch("wsl", Some("~/repo".to_string()), None).expect_err("reject ~");
        assert!(home.to_string().contains("must start with /"));
    }
}
