use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DEFAULT_TERMINAL_FONT_SIZE: u16 = 15;
const MIN_TERMINAL_FONT_SIZE: u16 = 10;
const MAX_TERMINAL_FONT_SIZE: u16 = 32;

/// 应用设置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub proxy: ProxySettings,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    pub terminal: TerminalSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
    #[serde(default)]
    pub general: GeneralSettings,
    #[serde(default)]
    pub notification: NotificationSettings,
    #[serde(default)]
    pub screenshot: ScreenshotSettings,
    #[serde(default)]
    pub voice: VoiceSettings,
    #[serde(default)]
    pub ccchan: CCChanSettings,
}

impl AppSettings {
    pub fn merge_missing_defaults(&mut self) {
        self.terminal.merge_missing_defaults();
        self.shortcuts.merge_missing_defaults();
        self.voice.merge_missing_defaults();
        self.ccchan.merge_missing_defaults();
    }
}

/// 代理设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettings {
    pub enabled: bool,
    pub proxy_type: String, // "http" | "socks5"
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub no_proxy: Option<String>,
}

/// 主题设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSettings {
    pub mode: String, // "light" | "dark" | "system"
}

/// 终端设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub font_size: u16,
    pub font_family: String,
    pub cursor_style: String, // "block" | "underline" | "bar"
    pub cursor_blink: bool,
    pub scrollback: u32,
    /// 终端主题: "followApp" | "dark" | "light"
    #[serde(default = "default_terminal_theme_mode")]
    pub theme_mode: String,
    /// 终端渲染器: "auto" | "webgl" | "dom"
    #[serde(default = "default_terminal_renderer_mode")]
    pub renderer_mode: String,
    /// 用户选择的 Shell ID（如 "pwsh", "cmd", "git-bash"），None 表示自动探测
    #[serde(default)]
    pub shell: Option<String>,
    /// 禁用 ConPTY 输出 sanitize（默认 true，即禁用 sanitize，因为 dwFlags=0 已解决根本问题）
    #[serde(default)]
    pub disable_conpty_sanitize: Option<bool>,
}

impl TerminalSettings {
    pub fn merge_missing_defaults(&mut self) {
        if self.scrollback == crate::constants::terminal::LEGACY_DEFAULT_SCROLLBACK {
            self.scrollback = crate::constants::terminal::DEFAULT_SCROLLBACK;
        }
        if self.font_size < MIN_TERMINAL_FONT_SIZE || self.font_size > MAX_TERMINAL_FONT_SIZE {
            self.font_size = DEFAULT_TERMINAL_FONT_SIZE;
        }
        if !matches!(self.theme_mode.as_str(), "followApp" | "dark" | "light") {
            self.theme_mode = default_terminal_theme_mode();
        }
        if !matches!(self.renderer_mode.as_str(), "auto" | "webgl" | "dom") {
            self.renderer_mode = default_terminal_renderer_mode();
        }
    }
}

fn default_terminal_theme_mode() -> String {
    "followApp".to_string()
}

fn default_terminal_renderer_mode() -> String {
    "auto".to_string()
}

/// 快捷键设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutSettings {
    pub bindings: HashMap<String, String>, // actionId -> keyCombo
}

impl ShortcutSettings {
    pub fn merge_missing_defaults(&mut self) {
        let defaults = Self::default();
        for (action_id, key_combo) in defaults.bindings {
            if self.bindings.contains_key(&action_id) {
                continue;
            }
            if self.bindings.values().any(|value| value == &key_combo) {
                continue;
            }
            self.bindings.insert(action_id, key_combo);
        }
    }
}

/// 通知设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    pub enabled: bool,
    pub on_exit: bool,
    pub on_waiting_input: bool,
    pub only_when_unfocused: bool,
}

/// 搜索范围
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum SearchScope {
    #[default]
    Workspace,
    FullDisk,
}

/// 通用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
    pub auto_start: bool,
    pub language: String,
    #[serde(default)]
    pub data_dir: Option<String>,
    #[serde(default)]
    pub search_scope: SearchScope,
    /// 日志级别: "error" | "warn" | "info" | "debug" | "trace"
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// 新手引导是否已完成
    #[serde(default)]
    pub onboarding_completed: bool,
    /// 默认 CLI 工具（用于自我对话等场景）: "claude" | "codex"
    #[serde(default = "default_cli_tool")]
    pub default_cli_tool: String,
    /// 页面顶部显示的常用启动项
    #[serde(default = "default_launch_favorites")]
    pub launch_favorites: Vec<String>,
    /// 工作空间右键菜单中隐藏非常用启动项
    #[serde(default)]
    pub hide_non_favorite_launch_actions: bool,
}

fn default_cli_tool() -> String {
    "claude".to_string()
}

fn default_close_to_tray() -> bool {
    !cfg!(target_os = "linux")
}

fn default_launch_favorites() -> Vec<String> {
    vec![
        "terminal-default".to_string(),
        "claude-local".to_string(),
        "codex-local".to_string(),
    ]
}

fn default_log_level() -> String {
    "info".to_string()
}

/// 截图设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSettings {
    pub shortcut: String,
    pub retention_days: u32,
}

/// 语音输入设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    #[serde(default = "default_voice_provider")]
    pub provider: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub dashscope_api_key: String,
    /// Qwen-ASR OpenAI 兼容 API 地域: "cn" | "intl"
    #[serde(default = "default_voice_region")]
    pub region: String,
    #[serde(default = "default_voice_model")]
    pub model: String,
    #[serde(default)]
    pub mimo_api_key: String,
    #[serde(default = "default_voice_mimo_base_url")]
    pub mimo_base_url: String,
    #[serde(default = "default_voice_mimo_model")]
    pub mimo_model: String,
    /// 可选语种；为空时交给模型自动识别
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub enable_itn: bool,
    #[serde(default = "default_voice_max_record_seconds")]
    pub max_record_seconds: u32,
}

impl VoiceSettings {
    pub fn merge_missing_defaults(&mut self) {
        if !matches!(self.provider.as_str(), "dashscope" | "mimo") {
            self.provider = default_voice_provider();
        }
        if !matches!(self.region.as_str(), "cn" | "intl") {
            self.region = default_voice_region();
        }
        if self.model.trim().is_empty() {
            self.model = default_voice_model();
        }
        if self.mimo_base_url.trim().is_empty() {
            self.mimo_base_url = default_voice_mimo_base_url();
        } else {
            self.mimo_base_url = self.mimo_base_url.trim().trim_end_matches('/').to_string();
        }
        if self.mimo_model.trim().is_empty() {
            self.mimo_model = default_voice_mimo_model();
        }
        if let Some(language) = self.language.as_ref() {
            if language.trim().is_empty() {
                self.language = None;
            }
        }
        if !(1..=300).contains(&self.max_record_seconds) {
            self.max_record_seconds = default_voice_max_record_seconds();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CCChanSettings {
    #[serde(default = "default_ccchan_ai_engine")]
    pub ai_engine: String,
    #[serde(default = "default_ccchan_pet_id")]
    pub default_pet_id: String,
    #[serde(default = "default_ccchan_role_id")]
    pub active_role_id: String,
    #[serde(default)]
    pub roles: Vec<CCChanRolePreset>,
    #[serde(default = "default_ccchan_scope_mode")]
    pub scope_mode: String,
    #[serde(default)]
    pub pet_sources: CCChanPetSources,
    #[serde(default)]
    pub custom_pet_dirs: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_start: bool,
    #[serde(default = "default_true")]
    pub sound_enabled: bool,
    #[serde(default = "default_true")]
    pub window_visible: bool,
    #[serde(default)]
    pub window_x: Option<f64>,
    #[serde(default)]
    pub window_y: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CCChanRolePreset {
    pub id: String,
    pub name: String,
    pub ai_engine: String,
    pub pet_id: String,
    pub system_prompt: String,
    #[serde(default = "default_ccchan_role_runtime_kind")]
    pub runtime_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wsl_remote_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wsl_distro: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CCChanPetSources {
    #[serde(default = "default_true")]
    pub builtin: bool,
    #[serde(default = "default_true")]
    pub user: bool,
    #[serde(default = "default_true")]
    pub codex_home: bool,
}

impl CCChanSettings {
    pub fn merge_missing_defaults(&mut self) {
        if !matches!(self.ai_engine.as_str(), "claude" | "codex") {
            self.ai_engine = default_ccchan_ai_engine();
        }
        if self.default_pet_id.trim().is_empty() {
            self.default_pet_id = default_ccchan_pet_id();
        }
        self.roles = normalize_ccchan_roles(&self.roles, &self.ai_engine, &self.default_pet_id);
        if self.active_role_id.trim().is_empty() {
            self.active_role_id = default_ccchan_role_id();
        }
        if !self.roles.iter().any(|role| role.id == self.active_role_id) {
            self.active_role_id = self
                .roles
                .first()
                .map(|role| role.id.clone())
                .unwrap_or_else(default_ccchan_role_id);
        }
        if let Some(active_role) = self
            .roles
            .iter()
            .find(|role| role.id == self.active_role_id)
        {
            self.ai_engine = active_role.ai_engine.clone();
            self.default_pet_id = active_role.pet_id.clone();
        }
        if !matches!(self.scope_mode.as_str(), "global" | "focusedWindow") {
            self.scope_mode = default_ccchan_scope_mode();
        }
        self.custom_pet_dirs = normalize_ccchan_custom_pet_dirs(&self.custom_pet_dirs);
    }
}

fn default_ccchan_ai_engine() -> String {
    "claude".to_string()
}

fn default_ccchan_pet_id() -> String {
    "doro.codex-pet".to_string()
}

fn default_ccchan_role_id() -> String {
    "default".to_string()
}

fn default_ccchan_scope_mode() -> String {
    "global".to_string()
}

fn default_ccchan_role_runtime_kind() -> String {
    "local".to_string()
}

fn default_ccchan_role_prompt() -> String {
    [
        "You are ccchan, the desktop mascot assistant inside CC-Panes.",
        "Help the user operate CC-Panes, inspect sessions, explain stuck panes, and coordinate Claude Code or Codex work.",
        "Keep replies concise, practical, and in the user's language.",
    ]
    .join("\n")
}

fn default_ccchan_role(ai_engine: &str, pet_id: &str) -> CCChanRolePreset {
    CCChanRolePreset {
        id: default_ccchan_role_id(),
        name: "默认助手".to_string(),
        ai_engine: ai_engine.to_string(),
        pet_id: pet_id.to_string(),
        system_prompt: default_ccchan_role_prompt(),
        runtime_kind: default_ccchan_role_runtime_kind(),
        wsl_remote_path: None,
        wsl_distro: None,
    }
}

fn normalize_ccchan_roles(
    roles: &[CCChanRolePreset],
    fallback_ai_engine: &str,
    fallback_pet_id: &str,
) -> Vec<CCChanRolePreset> {
    let fallback = default_ccchan_role(fallback_ai_engine, fallback_pet_id);
    let mut normalized: Vec<CCChanRolePreset> = roles
        .iter()
        .map(|role| {
            let mut next = role.clone();
            if next.id.trim().is_empty() {
                next.id = fallback.id.clone();
            } else {
                next.id = next.id.trim().to_string();
            }
            if next.name.trim().is_empty() {
                next.name = fallback.name.clone();
            } else {
                next.name = next.name.trim().to_string();
            }
            if !matches!(next.ai_engine.as_str(), "claude" | "codex") {
                next.ai_engine = fallback.ai_engine.clone();
            }
            if next.pet_id.trim().is_empty() {
                next.pet_id = fallback.pet_id.clone();
            } else {
                next.pet_id = next.pet_id.trim().to_string();
            }
            if next.system_prompt.trim().is_empty() {
                next.system_prompt = fallback.system_prompt.clone();
            }
            if !matches!(next.runtime_kind.as_str(), "local" | "wsl") {
                next.runtime_kind = fallback.runtime_kind.clone();
            }
            next.wsl_remote_path = next
                .wsl_remote_path
                .as_ref()
                .map(|path| path.trim().to_string())
                .filter(|path| !path.is_empty());
            next.wsl_distro = next
                .wsl_distro
                .as_ref()
                .map(|distro| distro.trim().to_string())
                .filter(|distro| !distro.is_empty());
            next
        })
        .collect();

    if normalized.is_empty() {
        normalized.push(fallback);
    } else if !normalized
        .iter()
        .any(|role| role.id == default_ccchan_role_id())
    {
        normalized.insert(0, fallback);
    }
    normalized
}

fn normalize_ccchan_custom_pet_dirs(dirs: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for dir in dirs {
        let trimmed = dir.trim();
        if trimmed.is_empty() || normalized.iter().any(|item| item == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_string());
    }
    normalized
}

fn default_true() -> bool {
    true
}

fn default_voice_provider() -> String {
    "dashscope".to_string()
}

fn default_voice_region() -> String {
    "cn".to_string()
}

fn default_voice_model() -> String {
    "qwen3-asr-flash".to_string()
}

fn default_voice_mimo_base_url() -> String {
    "https://api.xiaomimimo.com/v1".to_string()
}

fn default_voice_mimo_model() -> String {
    "mimo-v2.5".to_string()
}

fn default_voice_max_record_seconds() -> u32 {
    60
}

// ---- 默认值实现 ----

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            proxy_type: "http".to_string(),
            host: String::new(),
            port: 7890,
            username: None,
            password: None,
            no_proxy: Some("localhost,127.0.0.1".to_string()),
        }
    }
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            mode: "dark".to_string(),
        }
    }
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_TERMINAL_FONT_SIZE,
            font_family: "Consolas, \"Courier New\", monospace".to_string(),
            cursor_style: "block".to_string(),
            cursor_blink: true,
            scrollback: crate::constants::terminal::DEFAULT_SCROLLBACK,
            theme_mode: default_terminal_theme_mode(),
            renderer_mode: default_terminal_renderer_mode(),
            shell: None,
            disable_conpty_sanitize: None,
        }
    }
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        bindings.insert("toggle-sidebar".to_string(), "Ctrl+B".to_string());
        bindings.insert("toggle-fullscreen".to_string(), "F11".to_string());
        bindings.insert("new-tab".to_string(), "Ctrl+T".to_string());
        bindings.insert("close-tab".to_string(), "Ctrl+W".to_string());
        bindings.insert("settings".to_string(), "Ctrl+,".to_string());
        bindings.insert("split-right".to_string(), "Ctrl+\\".to_string());
        bindings.insert("split-down".to_string(), "Ctrl+-".to_string());
        bindings.insert("focus-pane-left".to_string(), "Alt+Left".to_string());
        bindings.insert("focus-pane-right".to_string(), "Alt+Right".to_string());
        bindings.insert("focus-pane-up".to_string(), "Alt+Up".to_string());
        bindings.insert("focus-pane-down".to_string(), "Alt+Down".to_string());
        bindings.insert("next-tab".to_string(), "Ctrl+Tab".to_string());
        bindings.insert("prev-tab".to_string(), "Ctrl+Shift+Tab".to_string());
        bindings.insert("toggle-mini-mode".to_string(), "Ctrl+M".to_string());
        bindings.insert("voice-input".to_string(), "Ctrl+Alt+M".to_string());
        for i in 1..=9 {
            bindings.insert(format!("switch-tab-{}", i), format!("Ctrl+{}", i));
        }
        Self { bindings }
    }
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            on_exit: true,
            on_waiting_input: true,
            only_when_unfocused: true,
        }
    }
}

impl Default for ScreenshotSettings {
    fn default() -> Self {
        Self {
            shortcut: if cfg!(debug_assertions) {
                "Ctrl+Alt+Shift+S".to_string() // dev 用不同的默认快捷键，避免与 release 冲突
            } else {
                "Ctrl+Shift+S".to_string()
            },
            retention_days: 7,
        }
    }
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            provider: default_voice_provider(),
            enabled: false,
            dashscope_api_key: String::new(),
            region: default_voice_region(),
            model: default_voice_model(),
            mimo_api_key: String::new(),
            mimo_base_url: default_voice_mimo_base_url(),
            mimo_model: default_voice_mimo_model(),
            language: None,
            enable_itn: false,
            max_record_seconds: default_voice_max_record_seconds(),
        }
    }
}

impl Default for CCChanSettings {
    fn default() -> Self {
        Self {
            ai_engine: default_ccchan_ai_engine(),
            default_pet_id: default_ccchan_pet_id(),
            active_role_id: default_ccchan_role_id(),
            roles: vec![default_ccchan_role(
                &default_ccchan_ai_engine(),
                &default_ccchan_pet_id(),
            )],
            scope_mode: default_ccchan_scope_mode(),
            pet_sources: CCChanPetSources::default(),
            custom_pet_dirs: Vec::new(),
            auto_start: true,
            sound_enabled: true,
            window_visible: true,
            window_x: None,
            window_y: None,
        }
    }
}

impl Default for CCChanPetSources {
    fn default() -> Self {
        Self {
            builtin: true,
            user: true,
            codex_home: true,
        }
    }
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            close_to_tray: default_close_to_tray(),
            auto_start: false,
            language: "zh-CN".to_string(),
            data_dir: None,
            search_scope: SearchScope::default(),
            log_level: default_log_level(),
            onboarding_completed: false,
            default_cli_tool: default_cli_tool(),
            launch_favorites: default_launch_favorites(),
            hide_non_favorite_launch_actions: false,
        }
    }
}

impl ProxySettings {
    /// 将代理配置转换为环境变量
    pub fn to_env_vars(&self) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        if !self.enabled || self.host.is_empty() {
            return vars;
        }

        let auth = match (&self.username, &self.password) {
            (Some(user), Some(pass)) if !user.is_empty() => {
                format!(
                    "{}:{}@",
                    urlencoding::encode(user),
                    urlencoding::encode(pass)
                )
            }
            _ => String::new(),
        };

        let proxy_url = format!("{}://{}{}:{}", self.proxy_type, auth, self.host, self.port);

        vars.insert("HTTP_PROXY".to_string(), proxy_url.clone());
        vars.insert("HTTPS_PROXY".to_string(), proxy_url.clone());
        vars.insert("http_proxy".to_string(), proxy_url.clone());
        vars.insert("https_proxy".to_string(), proxy_url.clone());
        vars.insert("ALL_PROXY".to_string(), proxy_url);

        if let Some(ref no_proxy) = self.no_proxy {
            if !no_proxy.is_empty() {
                vars.insert("NO_PROXY".to_string(), no_proxy.clone());
                vars.insert("no_proxy".to_string(), no_proxy.clone());
            }
        }

        vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_defaults_include_pane_focus_bindings() {
        let bindings = ShortcutSettings::default().bindings;

        assert_eq!(
            bindings.get("focus-pane-left"),
            Some(&"Alt+Left".to_string())
        );
        assert_eq!(
            bindings.get("focus-pane-right"),
            Some(&"Alt+Right".to_string())
        );
        assert_eq!(bindings.get("focus-pane-up"), Some(&"Alt+Up".to_string()));
        assert_eq!(
            bindings.get("focus-pane-down"),
            Some(&"Alt+Down".to_string())
        );
        assert_eq!(bindings.get("voice-input"), Some(&"Ctrl+Alt+M".to_string()));
    }

    #[test]
    fn merge_missing_defaults_preserves_existing_overrides() {
        let mut settings = ShortcutSettings {
            bindings: HashMap::from([("focus-pane-left".to_string(), "Ctrl+Alt+Left".to_string())]),
        };

        settings.merge_missing_defaults();

        assert_eq!(
            settings.bindings.get("focus-pane-left"),
            Some(&"Ctrl+Alt+Left".to_string())
        );
        assert_eq!(
            settings.bindings.get("focus-pane-right"),
            Some(&"Alt+Right".to_string())
        );
    }

    #[test]
    fn merge_missing_defaults_does_not_create_binding_conflicts() {
        let mut settings = ShortcutSettings {
            bindings: HashMap::from([("custom-action".to_string(), "Alt+Left".to_string())]),
        };

        settings.merge_missing_defaults();

        assert_eq!(
            settings.bindings.get("custom-action"),
            Some(&"Alt+Left".to_string())
        );
        assert!(!settings.bindings.contains_key("focus-pane-left"));
        assert_eq!(
            settings.bindings.get("focus-pane-right"),
            Some(&"Alt+Right".to_string())
        );
    }

    #[test]
    fn terminal_merge_missing_defaults_migrates_legacy_scrollback() {
        let mut settings = TerminalSettings::default();
        settings.scrollback = crate::constants::terminal::LEGACY_DEFAULT_SCROLLBACK;

        settings.merge_missing_defaults();

        assert_eq!(
            settings.scrollback,
            crate::constants::terminal::DEFAULT_SCROLLBACK
        );
    }

    #[test]
    fn terminal_merge_missing_defaults_preserves_custom_scrollback() {
        let mut settings = TerminalSettings::default();
        settings.scrollback = 5_000;

        settings.merge_missing_defaults();

        assert_eq!(settings.scrollback, 5_000);
    }

    #[test]
    fn terminal_merge_missing_defaults_resets_invalid_renderer_mode() {
        let mut settings = TerminalSettings::default();
        settings.renderer_mode = "unknown".to_string();

        settings.merge_missing_defaults();

        assert_eq!(settings.renderer_mode, "auto");
    }

    #[test]
    fn terminal_merge_missing_defaults_normalizes_appearance_values() {
        let mut settings = TerminalSettings::default();
        settings.font_size = 5;
        settings.theme_mode = "unknown".to_string();

        settings.merge_missing_defaults();

        assert_eq!(settings.font_size, DEFAULT_TERMINAL_FONT_SIZE);
        assert_eq!(settings.theme_mode, "followApp");
    }

    #[test]
    fn voice_merge_missing_defaults_normalizes_invalid_values() {
        let mut settings = VoiceSettings {
            provider: "unknown".to_string(),
            enabled: true,
            dashscope_api_key: "sk-test".to_string(),
            region: "invalid".to_string(),
            model: String::new(),
            mimo_api_key: "mimo-test".to_string(),
            mimo_base_url: " https://api.xiaomimimo.com/v1/ ".to_string(),
            mimo_model: String::new(),
            language: Some(" ".to_string()),
            enable_itn: true,
            max_record_seconds: 999,
        };

        settings.merge_missing_defaults();

        assert_eq!(settings.provider, "dashscope");
        assert_eq!(settings.region, "cn");
        assert_eq!(settings.model, "qwen3-asr-flash");
        assert_eq!(settings.mimo_base_url, "https://api.xiaomimimo.com/v1");
        assert_eq!(settings.mimo_model, "mimo-v2.5");
        assert_eq!(settings.language, None);
        assert_eq!(settings.max_record_seconds, 60);
    }

    #[test]
    fn ccchan_merge_missing_defaults_migrates_legacy_settings_to_default_role() {
        let mut settings: CCChanSettings = serde_json::from_value(serde_json::json!({
            "aiEngine": "codex",
            "defaultPetId": "doro.codex-pet",
            "autoStart": true,
            "soundEnabled": false,
            "windowVisible": true,
            "windowX": 12.0,
            "windowY": 34.0
        }))
        .expect("legacy ccchan settings should deserialize");

        settings.merge_missing_defaults();

        assert_eq!(settings.active_role_id, "default");
        assert_eq!(settings.scope_mode, "global");
        assert_eq!(settings.pet_sources, CCChanPetSources::default());
        assert_eq!(settings.roles.len(), 1);
        assert_eq!(settings.roles[0].id, "default");
        assert_eq!(settings.roles[0].ai_engine, "codex");
        assert_eq!(settings.roles[0].pet_id, "doro.codex-pet");
        assert_eq!(settings.roles[0].runtime_kind, "local");
        assert!(settings.roles[0].system_prompt.contains("CC-Panes"));
    }

    #[test]
    fn ccchan_merge_missing_defaults_syncs_legacy_fields_from_active_role() {
        let mut settings = CCChanSettings {
            active_role_id: "reviewer".to_string(),
            roles: vec![
                default_ccchan_role("claude", "homie"),
                CCChanRolePreset {
                    id: "reviewer".to_string(),
                    name: "审查员".to_string(),
                    ai_engine: "codex".to_string(),
                    pet_id: "doro.codex-pet".to_string(),
                    system_prompt: "Review the current work.".to_string(),
                    runtime_kind: "wsl".to_string(),
                    wsl_remote_path: Some(" /home/dev/repo ".to_string()),
                    wsl_distro: Some(" Ubuntu ".to_string()),
                },
            ],
            ..CCChanSettings::default()
        };

        settings.merge_missing_defaults();

        assert_eq!(settings.active_role_id, "reviewer");
        assert_eq!(settings.ai_engine, "codex");
        assert_eq!(settings.default_pet_id, "doro.codex-pet");
        assert_eq!(settings.roles[1].runtime_kind, "wsl");
        assert_eq!(
            settings.roles[1].wsl_remote_path.as_deref(),
            Some("/home/dev/repo")
        );
        assert_eq!(settings.roles[1].wsl_distro.as_deref(), Some("Ubuntu"));
    }

    #[test]
    fn ccchan_merge_missing_defaults_normalizes_custom_pet_dirs() {
        let mut settings = CCChanSettings {
            custom_pet_dirs: vec![
                " /home/dev/.codex/pets ".to_string(),
                String::new(),
                "/home/dev/.codex/pets".to_string(),
                "\\\\wsl.localhost\\Ubuntu-24.04\\home\\dev\\.codex\\pets".to_string(),
            ],
            ..CCChanSettings::default()
        };

        settings.merge_missing_defaults();

        assert_eq!(
            settings.custom_pet_dirs,
            vec![
                "/home/dev/.codex/pets".to_string(),
                "\\\\wsl.localhost\\Ubuntu-24.04\\home\\dev\\.codex\\pets".to_string(),
            ]
        );
    }
}
