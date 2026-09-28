//! Single writer thread for the initial index: consumes walker batches and
//! inserts them in large transactions while the walk is still running.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use rusqlite::{params, Connection};

use super::sync::Roots;
use super::walk::Item;
use crate::bench_common::BResult;

/// Rows per transaction: large enough to amortise commits, small enough to
/// keep the WAL bounded.
const ROWS_PER_TX: u64 = 200_000;

pub struct WriteStats {
    pub dirs: u64,
    pub files: u64,
}

struct Writer<'c> {
    conn: &'c Connection,
    roots: &'c Roots,
    ids: HashMap<PathBuf, i64>,
    in_tx: u64,
    stats: WriteStats,
}

impl<'c> Writer<'c> {
    /// Id of `path`, inserted (with ancestors) if the walker has not sent it
    /// yet. Parallel walkers send a directory before its children, so this is
    /// normally a map lookup.
    fn dir_id(&mut self, path: &Path, mtime: i64) -> BResult<i64> {
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
            .execute(params![path.to_string_lossy(), parent_id, mtime])?;
        let id = self.conn.last_insert_rowid();
        self.ids.insert(path.to_path_buf(), id);
        self.stats.dirs += 1;
        self.bump()?;
        Ok(id)
    }

    fn put(&mut self, item: Item) -> BResult<()> {
        match item {
            Item::Dir { path, mtime } => {
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
                self.conn
                    .prepare_cached(
                        "INSERT INTO files(dir_id, name, ext, size, created, modified, accessed, readonly)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    )?
                    .execute(params![
                        dir_id, row.name, row.ext, row.size, row.created, row.modified, row.accessed, row.readonly
                    ])?;
                self.stats.files += 1;
                self.bump()?;
            }
        }
        Ok(())
    }

    fn bump(&mut self) -> BResult<()> {
        self.in_tx += 1;
        if self.in_tx >= ROWS_PER_TX {
            self.conn.execute_batch("COMMIT; BEGIN")?;
            self.in_tx = 0;
        }
        Ok(())
    }
}

/// Drains `rx` into the database until every sender is gone. Takes the
/// connection (it cannot be shared across threads) and hands it back.
pub fn run(conn: Connection, roots: &Roots, rx: Receiver<Vec<Item>>) -> BResult<(Connection, WriteStats)> {
    let stats = {
        let mut w = Writer {
            conn: &conn,
            roots,
            ids: HashMap::new(),
            in_tx: 0,
            stats: WriteStats { dirs: 0, files: 0 },
        };
        conn.execute_batch("BEGIN")?;
        for batch in rx {
            for item in batch {
                w.put(item)?;
            }
        }
        conn.execute_batch("COMMIT")?;
        w.stats
    };
    Ok((conn, stats))
}
