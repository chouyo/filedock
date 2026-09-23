use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use crate::commands::AppState;
use crate::config::Category;

pub struct FileWatch {
    watcher: RecommendedWatcher,
    join: tokio::task::JoinHandle<()>,
}

async fn stop(state: &State<'_, AppState>) {
    let mut guard = state.file_watch.lock().await;
    if let Some(w) = guard.take() {
        drop(w.watcher);
        w.join.abort();
    }
}

#[tauri::command]
pub async fn start_file_watch(
    app: AppHandle,
    category_id: String,
    debounce_ms: u64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    stop(&state).await;

    let category: Category = {
        let cfg = state.config.lock().unwrap();
        cfg.categories
            .iter()
            .find(|c| c.id == category_id)
            .cloned()
            .ok_or_else(|| "category not found".to_string())?
    };

    let (tx, rx) = mpsc::channel::<()>(128);
    let mut watcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if matches!(
                    ev.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                ) {
                    let _ = tx.blocking_send(());
                }
            }
        })
        .map_err(|e| e.to_string())?;

    for target in &category.targets {
        let path = std::path::Path::new(&target.dir);
        if path.is_dir() {
            let mode = if target.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            let _ = watcher.watch(path, mode);
        }
    }

    let app2 = app.clone();
    let cat = category_id.clone();
    let debounce = Duration::from_millis(debounce_ms.max(100));
    let join = tokio::spawn(async move {
        let mut rx = rx;
        let mut last: Option<Instant> = None;
        loop {
            if last.is_none() {
                if rx.recv().await.is_none() {
                    break;
                }
                last = Some(Instant::now());
                continue;
            }
            tokio::select! {
                maybe = rx.recv() => {
                    if maybe.is_none() {
                        break;
                    }
                    last = Some(Instant::now());
                }
                _ = tokio::time::sleep(debounce) => {
                    let _ = app2.emit("files-changed", &cat);
                    last = None;
                }
            }
        }
    });

    let mut guard = state.file_watch.lock().await;
    *guard = Some(FileWatch { watcher, join });
    Ok(())
}

#[tauri::command]
pub async fn stop_file_watch(state: State<'_, AppState>) -> Result<(), String> {
    stop(&state).await;
    Ok(())
}