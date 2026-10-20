//! First-time indexing of a root: `ignore`'s parallel walker reads
//! directories on all cores and streams batches to the calling thread, which
//! inserts them while the walk is still running.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};

use rusqlite::{params, Connection};

use super::store::{dir_mtime, path_s, FileRow};
use super::sync::{clear_root, insert_file, Root, Roots};
use super::IResult;
use crate::util::now_millis;

enum Item {
    Dir { path: PathBuf, mtime: i64 },
    File { dir: PathBuf, row: FileRow },
}

const BATCH: usize = 4096;

/// Rows per transaction: large enough to amortise commits, small enough to
/// keep the WAL bounded.
const ROWS_PER_TX: u64 = 200_000;

/// Per-thread buffer that forwards full batches and flushes on drop.
struct Batch {
    tx: SyncSender<Vec<Item>>,
    items: Vec<Item>,
}

impl Batch {
    fn push(&mut self, item: Item) {
        self.items.push(item);
        if self.items.len() >= BATCH {
            let full = std::mem::replace(&mut self.items, Vec::with_capacity(BATCH));
            let _ = self.tx.send(full);
        }
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        if !self.items.is_empty() {
            let _ = self.tx.send(std::mem::take(&mut self.items));
        }
    }
}

fn to_item(entry: &ignore::DirEntry) -> Option<Item> {
    let ft = entry.file_type()?;
    let path = entry.path();
    if ft.is_dir() {
        Some(Item::Dir {
            path: path.to_path_buf(),
            mtime: dir_mtime(&entry.metadata().ok()?),
        })
    } else if ft.is_file() {
        Some(Item::File {
            dir: path.parent()?.to_path_buf(),
            row: FileRow::from_metadata(
                path.file_name()?.to_string_lossy().into_owned(),
                &entry.metadata().ok()?,
            ),
        })
    } else {
        // Symlinks are not followed or listed, as in `scan.rs`.
        None
    }
}

/// Walks `root` and sends every directory and file; returns the number of
/// entries that could not be read.
fn walk(root: &Root, tx: SyncSender<Vec<Item>>) -> u64 {
    let errors = AtomicU64::new(0);
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let mut builder = ignore::WalkBuilder::new(&root.path);
    builder
        .standard_filters(false)
        .follow_links(false)
        .threads(threads);
    if !root.recursive {
        builder.max_depth(Some(1));
    }
    builder.build_parallel().run(|| {
        let mut batch = Batch {
            tx: tx.clone(),
            items: Vec::with_capacity(BATCH),
        };
        let errors = &errors;
        Box::new(move |result| {
            match result {
                Ok(entry) => {
                    if let Some(item) = to_item(&entry) {
                        batch.push(item);
                    }
                }
                Err(_) => {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            ignore::WalkState::Continue
        })
    });
    errors.into_inner()
}

struct Writer<'c> {
    conn: &'c Connection,
    roots: &'c Roots,
    root: &'c Root,
    ids: HashMap<PathBuf, i64>,
    in_tx: u64,
}

impl Writer<'_> {
    /// Id of `path`, inserted (with ancestors) if the walker has not sent it
    /// yet. Parallel walkers usually send a directory before its children,
    /// so this is normally a map lookup.
    fn dir_id(&mut self, path: &Path, mtime: i64) -> IResult<i64> {
        if let Some(id) = self.ids.get(path) {
            return Ok(*id);
        }
        let parent_id = match path.parent() {
            Some(parent) if !self.roots.is_root(path) && self.roots.tracks_dir(parent) => {
                Some(self.dir_id(parent, 0)?)
            }
            _ => None,
        };
        self.conn
            .prepare_cached("INSERT INTO dirs(path, parent_id, mtime) VALUES (?1, ?2, ?3)")?
            .execute(params![path_s(path), parent_id, mtime])?;
        let id = self.conn.last_insert_rowid();
        self.ids.insert(path.to_path_buf(), id);
        self.bump()?;
        Ok(id)
    }

    fn put(&mut self, item: Item) -> IResult<()> {
        match item {
            Item::Dir { path, mtime } => {
                // A non-recursive root's walk also reports its subdirectories,
                // which are not tracked (or belong to another root).
                if !self.roots.tracks_dir(&path)
                    || (path != self.root.path && self.roots.is_root(&path))
                {
                    return Ok(());
                }
                if let Some(id) = self.ids.get(&path) {
                    // Inserted early as a parent placeholder; record its mtime.
                    self.conn
                        .prepare_cached("UPDATE dirs SET mtime = ?2 WHERE id = ?1")?
                        .execute(params![id, mtime])?;
                } else {
                    self.dir_id(&path, mtime)?;
                }
            }
            Item::File { dir, row } => {
                let dir_id = self.dir_id(&dir, 0)?;
                insert_file(self.conn, dir_id, &row)?;
                self.bump()?;
            }
        }
        Ok(())
    }

    fn bump(&mut self) -> IResult<()> {
        self.in_tx += 1;
        if self.in_tx >= ROWS_PER_TX {
            self.conn.execute_batch("COMMIT; BEGIN")?;
            self.in_tx = 0;
        }
        Ok(())
    }
}

/// Clears whatever the root's area held and indexes it from scratch, then
/// records it in `roots`. A crash midway leaves no `roots` row, so the next
/// start indexes it again.
pub fn build_root(conn: &Connection, roots: &Roots, root: &Root) -> IResult<()> {
    conn.execute_batch("BEGIN")?;
    let result = (|| {
        clear_root(conn, root)?;
        let (tx, rx) = sync_channel::<Vec<Item>>(64);
        std::thread::scope(|s| -> IResult<()> {
            let walker = s.spawn(move || walk(root, tx));
            let mut w = Writer {
                conn,
                roots,
                root,
                ids: HashMap::new(),
                in_tx: 0,
            };
            if root.path.is_dir() {
                let mtime = std::fs::symlink_metadata(&root.path)
                    .map(|m| dir_mtime(&m))
                    .unwrap_or(0);
                w.dir_id(&root.path, mtime)?;
            }
            let mut result = Ok(());
            for batch in rx {
                if result.is_err() {
                    continue; // drain so the walker can finish
                }
                for item in batch {
                    if let Err(e) = w.put(item) {
                        result = Err(e);
                        break;
                    }
                }
            }
            let errors = walker.join().map_err(|_| "walker panicked")?;
            result?;
            conn.prepare_cached(
                "INSERT INTO roots(path, recursive, indexed_at, walk_errors) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(path, recursive) DO UPDATE SET
                     indexed_at = excluded.indexed_at, walk_errors = excluded.walk_errors",
            )?
            .execute(params![path_s(&root.path), root.recursive, now_millis(), errors as i64])?;
            Ok(())
        })
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}
