use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager, State};

use crate::config::{self, Category, Config, Session};
use crate::index::{self, Index, Level};
use crate::maximize;
use crate::pages::{self, FilePage, FileQuery};
use crate::settings::{self, Settings};
use crate::util;
use crate::watch;
use crate::window_state;

pub struct AppState {
    pub config: Mutex<Config>,
    pub settings: Mutex<Settings>,
    pub file_watch: Mutex<Option<watch::FileWatch>>,
    /// `None` when the index could not be opened; listing then falls back to
    /// walking the directories on every call.
    pub index: Option<Arc<Index>>,
    pub pages: pages::PageCache,
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
    app: tauri::AppHandle,
    category: Category,
    state: State<'_, AppState>,
) -> Result<Vec<Category>, String> {
    let categories = save_category_inner(category, &state)?;
    index::tasks::spawn_config_changed(app);
    Ok(categories)
}

fn save_category_inner(
    category: Category,
    state: &State<'_, AppState>,
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
pub fn delete_category(
    app: tauri::AppHandle,
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Category>, String> {
    let categories = {
        let mut cfg = state.config.lock().unwrap();
        cfg.categories.retain(|c| c.id != id);
        for (i, c) in cfg.categories.iter_mut().enumerate() {
            c.order = i as i64;
        }
        config::save_config_file(&cfg)?;

        let mut categories = cfg.categories.clone();
        categories.sort_by_key(|c| c.order);
        categories
    };
    index::tasks::spawn_config_changed(app);
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

fn find_category(state: &State<'_, AppState>, id: &str) -> Result<Category, String> {
    // Clone the category and release the config lock before touching the
    // disk. Sync commands such as `save_session` run on the main thread and
    // take this lock, so holding it longer would freeze the window.
    let cfg = state.config.lock().unwrap();
    cfg.categories
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .ok_or_else(|| "category not found".to_string())
}

/// One page of a category's files, filtered by name and sorted. Answered
/// from the index (roots without data are indexed first); the watcher keeps
/// it current.
#[tauri::command]
pub async fn query_files(
    app: tauri::AppHandle,
    query: FileQuery,
    state: State<'_, AppState>,
) -> Result<FilePage, String> {
    let category = find_category(&state, &query.category_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let index = state.index.as_deref();
        if let Some(index) = index {
            if let Err(e) = index.ensure_indexed(&index.roots_for(&category)) {
                eprintln!("index: building roots of {} failed: {e}", category.id);
            }
        }
        pages::query(index, &state.pages, &category, &query)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Manual refresh: checks the category's directories fully (level 3, which
/// also catches files rewritten in place) before the frontend reloads.
#[tauri::command]
pub async fn rescan_category(
    category_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let category = find_category(&state, &category_id)?;
    let Some(index) = state.index.clone() else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || -> index::IResult<()> {
        let roots = index.roots_for(&category);
        let built = index.ensure_indexed(&roots)?;
        for root in roots.iter().filter(|r| !built.contains(r)) {
            index.reconcile(root, Level::Full, std::time::Duration::ZERO)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInfo {
    pub path: String,
    /// "dir" | "file" | "missing"
    pub kind: String,
    pub name: String,
    /// Lowercase, without the leading dot; empty when there is none.
    pub extension: String,
    pub parent_dir: String,
}

fn inspect_path(raw: &str) -> PathInfo {
    let path = std::path::Path::new(raw);
    let kind = if path.is_dir() {
        "dir"
    } else if path.is_file() {
        "file"
    } else {
        "missing"
    };
    // Drive roots such as `D:\` have no file name; fall back to the path
    // itself without trailing separators (`D:`).
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| raw.trim_end_matches(['\\', '/']).to_string());
    // `Path::extension` already treats dotfiles like `.gitignore` as having
    // no extension.
    let extension = if kind == "file" {
        path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let parent_dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    PathInfo {
        path: raw.to_string(),
        kind: kind.to_string(),
        name,
        extension,
        parent_dir,
    }
}

#[tauri::command]
pub fn inspect_paths(paths: Vec<String>) -> Vec<PathInfo> {
    paths.iter().map(|p| inspect_path(p)).collect()
}

#[tauri::command]
pub fn is_valid_directory(path: String) -> bool {
    let path = std::path::Path::new(&path);
    path.is_dir()
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
        maximize::toggle(&w);
    }
}

#[tauri::command]
pub fn close_main_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.close();
    }
}
