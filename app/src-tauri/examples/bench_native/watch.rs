//! Keeps the index current while running: notify events are debounced, then
//! each changed path is re-read and written back in one transaction.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::Connection;

use super::reconcile::{reconcile, ResumeMode};
use super::store::{self, dir_mtime, FileRow};
use super::sync::{
    apply_listing, delete_file, delete_subtree, dir_id, ensure_dir, insert_subtree, read_listing,
    set_dir_mtime, upsert_file, ApplyStats, Roots,
};
use crate::bench_common::BResult;

pub struct Watch {
    watcher: Option<RecommendedWatcher>,
    thread: Option<JoinHandle<()>>,
}

impl Watch {
    pub fn start(db: &Path, roots: Roots, debounce: Duration) -> BResult<Watch> {
        let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;
        for (root, recursive) in &roots.0 {
            let mode = if *recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            watcher.watch(root, mode)?;
        }
        let conn = store::open(db)?;
        let thread = std::thread::spawn(move || apply_loop(conn, roots, rx, debounce));
        Ok(Watch {
            watcher: Some(watcher),
            thread: Some(thread),
        })
    }

    pub fn stop(&mut self) {
        // Dropping the watcher drops the sender, which ends the apply loop.
        self.watcher.take();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.stop();
    }
}

fn apply_loop(
    conn: Connection,
    roots: Roots,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    debounce: Duration,
) {
    let max_wait = debounce * 10;
    while let Ok(first) = rx.recv() {
        let mut paths = BTreeSet::new();
        let mut rescan = false;
        let mut add = |res: notify::Result<notify::Event>| match res {
            Ok(ev) => {
                rescan |= ev.need_rescan();
                paths.extend(ev.paths);
            }
            // Dropped or overflowed events: fall back to a directory pass.
            Err(_) => rescan = true,
        };
        add(first);
        // Collect until quiet for `debounce`, but never longer than `max_wait`.
        let started = Instant::now();
        loop {
            match rx.recv_timeout(debounce) {
                Ok(res) => add(res),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if started.elapsed() >= max_wait {
                break;
            }
        }
        if let Err(e) = apply(&conn, &roots, &paths) {
            eprintln!("watch: apply failed: {e}");
        }
        if rescan {
            if let Err(e) = reconcile(&conn, &roots, ResumeMode::Dirs) {
                eprintln!("watch: rescan failed: {e}");
            }
        }
    }
}

fn apply(conn: &Connection, roots: &Roots, paths: &BTreeSet<PathBuf>) -> BResult<()> {
    let mut stats = ApplyStats::default();
    conn.execute_batch("BEGIN")?;
    for path in paths {
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.is_dir() => {
                if !roots.tracks_dir(path) {
                    continue;
                }
                let (id, created) = ensure_dir(conn, roots, path)?;
                if created {
                    insert_subtree(conn, roots, path)?;
                } else if let Ok(listing) = read_listing(path) {
                    // Relisting also repairs anything a coalesced event hid.
                    apply_listing(conn, roots, id, &listing, &mut stats)?;
                }
            }
            Ok(m) if m.is_file() => {
                let Some(parent) = path.parent() else { continue };
                if !roots.tracks_dir(parent) {
                    continue;
                }
                let (id, _) = ensure_dir(conn, roots, parent)?;
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                upsert_file(conn, id, &FileRow::from_metadata(name, &m))?;
            }
            Ok(_) => {}
            Err(_) => {
                // Gone: it was either a file or a directory.
                if let Some(parent) = path.parent() {
                    if let Some(id) = dir_id(conn, parent)? {
                        delete_file(conn, id, &path.file_name().unwrap().to_string_lossy())?;
                    }
                }
                delete_subtree(conn, path)?;
            }
        }
        // Keep parent mtimes current so a later level-2 pass skips them.
        if let Some(parent) = path.parent() {
            if let (Some(id), Ok(m)) = (dir_id(conn, parent)?, std::fs::symlink_metadata(parent)) {
                set_dir_mtime(conn, id, dir_mtime(&m))?;
            }
        }
    }
    conn.execute_batch("COMMIT")?;
    Ok(())
}
