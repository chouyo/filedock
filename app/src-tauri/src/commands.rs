use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager, State};

use crate::config::{self, Category, Config, Session};
use crate::file_entry::FileEntry;
use crate::scan;
use crate::settings::{self, Settings};
use crate::util;
use crate::watch;
use crate::window_state;

pub struct AppState {
    pub scan_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub config: Mutex<Config>,
    pub settings: Mutex<Settings>,
    pub file_watch: tokio::sync::Mutex<Option<watch::FileWatch>>,
}

#[tauri::command]
pub fn load_config(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.config.lock().unwrap().clone())
}

#[tauri::command]
pub fn load_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().unwrap().clone())
}

#[tauri::command]
pub async fn save_window_state(app: tauri::AppHandle) -> Result<(), String> {
    window_state::collect_and_save_window_state(&app)
}

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> Result<Vec<Category>, String> {
    let cfg = state.config.lock().unwrap();
    let mut categories = cfg.categories.clone();
    categories.sort_by_key(|c| c.order);
    Ok(categories)
}

#[tauri::command]
pub fn save_category(
    category: Category,
    state: State<'_, AppState>,
) -> Result<Vec<Category>, String> {
    let mut cfg = state.config.lock().unwrap();
    let now = util::now_millis();

    if category.id.is_empty() {
        let mut new_targets = category.targets;
        for target in &mut new_targets {
            if target.id.is_empty() {
                target.id = uuid::Uuid::new_v4().to_string();
            }
        }
        let new_category = Category {
            id: uuid::Uuid::new_v4().to_string(),
            name: category.name,
            order: cfg.categories.len() as i64,
            targets: new_targets,
            created_at: now,
            updated_at: now,
        };
        cfg.categories.push(new_category);
    } else {
        if let Some(c) = cfg.categories.iter_mut().find(|c| c.id == category.id) {
            c.name = category.name;
            let mut targets = category.targets;
            for target in &mut targets {
                if target.id.is_empty() {
                    target.id = uuid::Uuid::new_v4().to_string();
                }
            }
            c.targets = targets;
            c.updated_at = now;
        } else {
            return Err("category not found".to_string());
        }
    }

    config::save_config_file(&cfg)?;

    let mut categories = cfg.categories.clone();
    categories.sort_by_key(|c| c.order);
    Ok(categories)
}

#[tauri::command]
pub fn delete_category(id: String, state: State<'_, AppState>) -> Result<Vec<Category>, String> {
    let mut cfg = state.config.lock().unwrap();
    cfg.categories.retain(|c| c.id != id);
    for (i, c) in cfg.categories.iter_mut().enumerate() {
        c.order = i as i64;
    }
    config::save_config_file(&cfg)?;

    let mut categories = cfg.categories.clone();
    categories.sort_by_key(|c| c.order);
    Ok(categories)
}

#[tauri::command]
pub fn reorder_categories(
    ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Category>, String> {
    let mut cfg = state.config.lock().unwrap();
    for (i, id) in ids.iter().enumerate() {
        if let Some(c) = cfg.categories.iter_mut().find(|c| &c.id == id) {
            c.order = i as i64;
        }
    }
    config::save_config_file(&cfg)?;

    let mut categories = cfg.categories.clone();
    categories.sort_by_key(|c| c.order);
    Ok(categories)
}

#[tauri::command]
pub async fn list_files(
    category_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileEntry>, String> {
    let lock = {
        let mut map = state.scan_locks.lock().unwrap();
        map.entry(category_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let _guard = lock.lock().await;

    let cfg = state.config.lock().unwrap();
    let category = cfg
        .categories
        .iter()
        .find(|c| c.id == category_id)
        .ok_or_else(|| "category not found".to_string())?;

    Ok(scan::scan_category(category))
}

#[tauri::command]
pub fn save_session(session: Session, state: State<'_, AppState>) -> Result<(), String> {
    let mut cfg = state.config.lock().unwrap();
    cfg.session = session;
    config::save_config_file(&cfg)
}

#[tauri::command]
pub fn save_settings(
    settings: Settings,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    let mut settings = settings;

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
        .retain(|c| settings::VALID_COLUMN_KEYS.contains(&c.as_str()));
    if settings.columns.is_empty() {
        settings.columns = settings::default_columns();
    }

    {
        let existing = state.settings.lock().unwrap();
        if settings.window_state.is_none() && existing.window_state.is_some() {
            settings.window_state = existing.window_state.clone();
        }
    }

    {
        let mut s = state.settings.lock().unwrap();
        *s = settings.clone();
    }

    settings::save_settings_file(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn open_in_explorer(path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err("file not found".into());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        // explorer.exe does not use the standard C-runtime command-line parser,
        // so raw_arg preserves the /select,<path> syntax for paths with spaces.
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path))
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    Err("opening a file in the system file manager is not supported on this platform".into())
}

#[tauri::command]
pub fn open_devtools(app: tauri::AppHandle) {
    #[cfg(debug_assertions)]
    {
        if let Some(w) = app.get_webview_window("main") {
            w.open_devtools();
        }
    }
}

#[tauri::command]
pub fn hide_main_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
        let _ = app.emit("window-visibility", false);
    }
}

#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn get_config_dir() -> String {
    crate::config::app_config_dir().to_string_lossy().to_string()
}

#[tauri::command]
pub fn open_config_directory() -> Result<(), String> {
    let dir = crate::config::app_config_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    Err("opening the config directory is not supported on this platform".into())
}

#[tauri::command]
pub fn minimize_main_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
}

#[tauri::command]
pub fn toggle_maximize_main_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_maximized().unwrap_or(false) {
            let _ = w.unmaximize();
        } else {
            let _ = w.maximize();
        }
    }
}

#[tauri::command]
pub fn close_main_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.close();
    }
}
