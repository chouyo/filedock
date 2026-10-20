//! Background index work and the `files-changed` notifications it produces.
//!
//! The frontend already reloads the active category silently on
//! `files-changed`, so showing the stored list first and reconciling
//! afterwards needs no frontend changes.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use super::{normalize_dir, Change, Index, Level, Root};
use crate::commands::AppState;

/// Re-listing a category within this window reuses the last reconcile, so
/// the reload that `files-changed` triggers does not start another one.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

fn index(app: &AppHandle) -> Option<Arc<Index>> {
    app.state::<AppState>().index.clone()
}

/// Emits `files-changed` for every category whose data lives in `roots`.
fn notify(app: &AppHandle, index: &Index, roots: &HashSet<Root>) {
    if roots.is_empty() {
        return;
    }
    let categories = app
        .state::<AppState>()
        .config
        .lock()
        .unwrap()
        .categories
        .clone();
    for c in &categories {
        if index.roots_for(c).iter().any(|r| roots.contains(r)) {
            let _ = app.emit("files-changed", &c.id);
        }
    }
}

/// Emits `files-changed` for every category with a target that `changes`
/// touch (every category when `all`).
pub fn notify_changes(app: &AppHandle, changes: &HashSet<Change>, all: bool) {
    if changes.is_empty() && !all {
        return;
    }
    let categories = app
        .state::<AppState>()
        .config
        .lock()
        .unwrap()
        .categories
        .clone();
    for c in &categories {
        let hit = all
            || c.targets.iter().any(|t| {
                let dir = normalize_dir(&t.dir);
                changes.iter().any(|ch| ch.affects(&dir, t.recursive))
            });
        if hit {
            let _ = app.emit("files-changed", &c.id);
        }
    }
}

/// Reconciles `roots` and notifies the categories that changed. Blocking.
pub fn reconcile_and_notify(
    app: &AppHandle,
    index: &Index,
    roots: &[Root],
    level: Level,
    min_interval: Duration,
) {
    let mut changed = HashSet::new();
    for root in roots {
        match index.reconcile(root, level, min_interval) {
            Ok(true) => {
                changed.insert(root.clone());
            }
            Ok(false) => {}
            Err(e) => eprintln!("index: reconcile {} failed: {e}", root.path.display()),
        }
    }
    notify(app, index, &changed);
}

fn build_cold_and_notify(app: &AppHandle, index: &Index) {
    let cold = match index.cold_roots() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("index: {e}");
            return;
        }
    };
    let mut built = HashSet::new();
    // One at a time, so a category the user opens meanwhile waits for at
    // most one other root.
    for root in cold {
        match index.ensure_indexed(std::slice::from_ref(&root)) {
            Ok(b) => built.extend(b),
            Err(e) => eprintln!("index: building {} failed: {e}", root.path.display()),
        }
    }
    notify(app, index, &built);
}

/// After launch: drop data of removed rules, bring the stored roots up to
/// date (level 2), then index roots that have none yet.
pub fn spawn_startup(app: AppHandle) {
    let Some(index) = index(&app) else { return };
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = index.collect_garbage() {
            eprintln!("index: garbage collection failed: {e}");
        }
        let cold: HashSet<Root> = index.cold_roots().unwrap_or_default().into_iter().collect();
        let warm: Vec<Root> = index
            .roots()
            .0
            .into_iter()
            .filter(|r| !cold.contains(r))
            .collect();
        reconcile_and_notify(&app, &index, &warm, Level::Dirs, Duration::ZERO);
        build_cold_and_notify(&app, &index);
    });
}

/// After a category's list was served from the index: check those roots in
/// the background (level 2).
pub fn spawn_refresh(app: AppHandle, roots: Vec<Root>) {
    let Some(index) = index(&app) else { return };
    tauri::async_runtime::spawn_blocking(move || {
        reconcile_and_notify(&app, &index, &roots, Level::Dirs, REFRESH_INTERVAL);
    });
}

/// After categories were saved or deleted: watch the new roots, drop data
/// of removed ones and index new ones.
pub fn spawn_config_changed(app: AppHandle) {
    {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || crate::watch::sync_roots(&app));
    }
    let Some(index) = index(&app) else { return };
    let categories = app
        .state::<AppState>()
        .config
        .lock()
        .unwrap()
        .categories
        .clone();
    index.set_categories(&categories);
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = index.collect_garbage() {
            eprintln!("index: garbage collection failed: {e}");
        }
        build_cold_and_notify(&app, &index);
    });
}
