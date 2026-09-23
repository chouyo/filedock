use std::fs;
use std::path::PathBuf;

use crate::config::atomic_write;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_version")]
    pub version: i32,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_auto_refresh")]
    pub auto_refresh_enabled: bool,
    #[serde(default = "default_debounce")]
    pub watch_debounce_ms: u64,
    #[serde(default = "default_close_action")]
    pub close_action: String,
    #[serde(default = "default_columns_serde")]
    pub columns: Vec<String>,
    #[serde(default)]
    pub window_state: Option<WindowState>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub monitor_name: Option<String>,
    pub monitor_position: MonitorPos,
    pub monitor_size: MonitorSize,
    pub window_x: f64,
    pub window_y: f64,
    pub window_width: f64,
    pub window_height: f64,
    pub maximized: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MonitorPos {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MonitorSize {
    pub width: u32,
    pub height: u32,
}

pub const VALID_COLUMN_KEYS: &[&str] = &[
    "name",
    "extension",
    "size",
    "createdAt",
    "modifiedAt",
    "accessedAt",
    "parentDir",
    "path",
    "isReadonly",
];

fn default_version() -> i32 {
    1
}

fn default_language() -> String {
    "en".to_string()
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_auto_refresh() -> bool {
    false
}

fn default_debounce() -> u64 {
    1000
}

fn default_close_action() -> String {
    "ask".to_string()
}

fn default_columns_serde() -> Vec<String> {
    default_columns()
}

pub fn default_columns() -> Vec<String> {
    vec![
        "name".to_string(),
        "createdAt".to_string(),
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: 1,
            language: "en".to_string(),
            theme: "system".to_string(),
            auto_refresh_enabled: false,
            watch_debounce_ms: 1000,
            close_action: "ask".to_string(),
            columns: default_columns(),
            window_state: None,
        }
    }
}

pub fn settings_path() -> PathBuf {
    crate::config::app_config_dir().join("settings.json")
}

pub fn load_settings_file() -> Settings {
    let path = settings_path();
    if !path.exists() {
        let default = Settings::default();
        return default;
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Settings::default(),
    };
    let mut settings: Settings = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => {
            let default = Settings::default();
            return default;
        }
    };

    if settings.language != "en" && settings.language != "zh-CN" {
        settings.language = "en".to_string();
    }

    if !["system", "light", "dark"].contains(&settings.theme.as_str()) {
        settings.theme = "system".to_string();
    }

    if settings.watch_debounce_ms < 100 {
        settings.watch_debounce_ms = 100;
    }
    if settings.watch_debounce_ms > 10000 {
        settings.watch_debounce_ms = 10000;
    }

    if !["ask", "close", "minimize"].contains(&settings.close_action.as_str()) {
        settings.close_action = "ask".to_string();
    }

    settings
        .columns
        .retain(|c| VALID_COLUMN_KEYS.contains(&c.as_str()));
    if settings.columns.is_empty() {
        settings.columns = default_columns();
    }

    settings
}

pub fn save_settings_file(settings: &Settings) -> Result<(), String> {
    atomic_write(
        &settings_path(),
        &serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?,
    )
}

pub fn init_settings_file() -> Result<(), String> {
    let path = settings_path();
    if !path.exists() {
        let default = Settings::default();
        atomic_write(
            &path,
            &serde_json::to_string_pretty(&default).map_err(|e| e.to_string())?,
        )?;
    } else {
        let settings = load_settings_file();
        atomic_write(
            &path,
            &serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?,
        )?;
    }

    Ok(())
}
