//! App-wide file watcher.
//!
//! Watches every covering root of every category for the whole life of the
//! app (hidden window included), writes each change into the index path by
//! path, and, when auto refresh is on, emits `files-changed` for the
//! categories whose files changed. If the platform drops events (buffer
//! overflow, FSEvents "must rescan"), the affected roots are reconciled at
//! level 2 instead.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Manager};

use crate::commands::AppState;
use crate::index::{self, Change, Level, Root, Roots};

/// Under a continuous stream of events, apply them at most this long after
/// the quiet period would have ended.
const MAX_EXTRA_DELAY: Duration = Duration::from_secs(2);
/// How often roots that could not be watched (missing directory, no
/// permission) are tried again.
const RETRY_INTERVAL: Duration = Duration::from_secs(30);

pub struct FileWatch {
    watcher: RecommendedWatcher,
    watched: HashSet<Root>,
}

enum Msg {
    Paths(Vec<PathBuf>),
    /// Events were lost below these paths (all roots when empty).
    Rescan(Vec<PathBuf>),
}

/// Paths of the index database itself. Writing the index changes them, so
/// reacting to them would loop whenever a root covers the config directory.
fn is_own_file(path: &Path, own_prefix: &str) -> bool {
    path.to_string_lossy().starts_with(own_prefix)
}

/// Starts the watcher and its worker thread. Called once at startup.
pub fn start(app: &AppHandle) {
    let (tx, rx) = channel::<Msg>();
    let own_prefix = index::Index::default_path().to_string_lossy().into_owned();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let msg = match res {
            Ok(ev) if ev.need_rescan() => Msg::Rescan(ev.paths),
            Ok(ev) if matches!(ev.kind, EventKind::Access(_)) => return,
            Ok(ev) => {
                let paths: Vec<PathBuf> = ev
                    .paths
                    .into_iter()
                    .filter(|p| !is_own_file(p, &own_prefix))
                    .collect();
                if paths.is_empty() {
                    return;
                }
                Msg::Paths(paths)
            }
            Err(e) => Msg::Rescan(e.paths),
        };
        let _ = tx.send(msg);
    });
    let watcher = match watcher {
        Ok(w) => w,
        Err(e) => {
            eprintln!("watch: cannot create watcher: {e}");
            return;
        }
    };
    *app.state::<AppState>().file_watch.lock().unwrap() = Some(FileWatch {
        watcher,
        watched: HashSet::new(),
    });
    sync_roots(app);

    let app = app.clone();
    std::thread::Builder::new()
        .name("file-watch".into())
        .spawn(move || run(app, rx))
        .expect("spawn file-watch thread");
}

/// Watches the roots of the current categories and stops watching the
/// others. Called after categories change and periodically, to pick up
/// directories that appeared.
pub fn sync_roots(app: &AppHandle) {
    let state = app.state::<AppState>();
    let wanted: HashSet<Root> = {
        let cfg = state.config.lock().unwrap();
        Roots::from_categories(&cfg.categories)
            .0
            .into_iter()
            .collect()
    };
    let mut guard = state.file_watch.lock().unwrap();
    let Some(fw) = guard.as_mut() else { return };
    let gone: Vec<Root> = fw.watched.difference(&wanted).cloned().collect();
    for root in gone {
        let _ = fw.watcher.unwatch(&root.path);
        fw.watched.remove(&root);
    }
    for root in wanted {
        if fw.watched.contains(&root) || !root.path.is_dir() {
            continue;
        }
        let mode = if root.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        if fw.watcher.watch(&root.path, mode).is_ok() {
            fw.watched.insert(root);
        }
    }
}

/// When a pending batch is due: after the quiet period, but no later than
/// `MAX_EXTRA_DELAY` past the quiet period of its first event.
fn due(first: Instant, last: Instant, debounce: Duration) -> Instant {
    (last + debounce).min(first + debounce + MAX_EXTRA_DELAY)
}

fn run(app: AppHandle, rx: Receiver<Msg>) {
    let mut paths: HashSet<PathBuf> = HashSet::new();
    let mut rescan: HashSet<PathBuf> = HashSet::new();
    let mut rescan_all = false;
    // First and latest event of the pending batch.
    let mut pending: Option<(Instant, Instant)> = None;
    let mut last_retry = Instant::now();

    loop {
        let debounce = Duration::from_millis(
            app.state::<AppState>()
                .settings
                .lock()
                .unwrap()
                .watch_debounce_ms
                .max(100),
        );
        let timeout = match pending {
            Some((first, last)) => {
                due(first, last, debounce).saturating_duration_since(Instant::now())
            }
            None => RETRY_INTERVAL.saturating_sub(last_retry.elapsed()),
        };
        match rx.recv_timeout(timeout) {
            Ok(msg) => {
                match msg {
                    Msg::Paths(p) => paths.extend(p),
                    Msg::Rescan(p) if p.is_empty() => rescan_all = true,
                    Msg::Rescan(p) => rescan.extend(p),
                }
                let now = Instant::now();
                pending = Some(pending.map_or((now, now), |(first, _)| (first, now)));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        // Checked after every message too, so a steady stream of events
        // cannot postpone the batch forever.
        match pending {
            Some((first, last)) if Instant::now() >= due(first, last, debounce) => {
                flush(
                    &app,
                    std::mem::take(&mut paths),
                    std::mem::take(&mut rescan),
                    std::mem::take(&mut rescan_all),
                );
                pending = None;
            }
            Some(_) => {}
            None if last_retry.elapsed() >= RETRY_INTERVAL => {
                sync_roots(&app);
                last_retry = Instant::now();
            }
            None => {}
        }
    }
}

fn flush(app: &AppHandle, paths: HashSet<PathBuf>, rescan: HashSet<PathBuf>, rescan_all: bool) {
    let state = app.state::<AppState>();
    let notify = state.settings.lock().unwrap().auto_refresh_enabled;
    let mut changes: HashSet<Change> = HashSet::new();

    let Some(index) = state.index.clone() else {
        // No index: tell the categories that may list these paths.
        if notify {
            for p in paths.iter().chain(&rescan) {
                changes.insert(Change::Tree(p.clone()));
                if let Some(parent) = p.parent() {
                    changes.insert(Change::Files(parent.to_path_buf()));
                }
            }
            index::tasks::notify_changes(app, &changes, rescan_all);
        }
        return;
    };

    let mut rescan = rescan;
    match index.apply_paths(&paths) {
        Ok(c) => changes.extend(c),
        Err(e) => {
            eprintln!("watch: applying events failed, rescanning: {e}");
            rescan.extend(paths);
        }
    }
    let roots: Vec<Root> = index
        .roots()
        .0
        .into_iter()
        .filter(|r| {
            rescan_all
                || rescan
                    .iter()
                    .any(|p| p.starts_with(&r.path) || r.path.starts_with(p))
        })
        .collect();
    for root in roots {
        match index.reconcile(&root, Level::Dirs, Duration::ZERO) {
            Ok(true) => {
                changes.insert(Change::Tree(root.path.clone()));
            }
            Ok(false) => {}
            Err(e) => eprintln!("watch: reconcile {} failed: {e}", root.path.display()),
        }
    }
    if notify {
        index::tasks::notify_changes(app, &changes, false);
    }
}
