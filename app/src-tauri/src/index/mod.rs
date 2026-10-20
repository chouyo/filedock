//! Persistent file index (phase 1 of `docs/文件搜索性能优化方案.md`).
//!
//! Every directory the categories cover is stored in SQLite, so listing a
//! category is a query instead of a walk, and the list from the last run is
//! available immediately after a restart. Category rules are applied at
//! query time; only changes to the covered directories (the roots) need
//! indexing work.
//!
//! Concurrency: one writer connection behind a mutex serialises all writes;
//! queries use a separate reader connection (WAL lets them run during a
//! write). Disk access for reconciliation happens outside both locks.

mod build;
mod query;
mod reconcile;
mod store;
mod sync;
pub mod tasks;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::config::Category;
use crate::file_entry::FileEntry;
pub use reconcile::Level;
pub use sync::{Root, Roots};

pub type IResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Default)]
struct ReconcileState {
    running: bool,
    /// Requested while running; run once more at this level before finishing.
    rerun: Option<Level>,
    last_done: Option<Instant>,
}

pub struct Index {
    writer: Mutex<Connection>,
    reader: Mutex<Connection>,
    roots: Mutex<Roots>,
    reconciles: Mutex<HashMap<Root, ReconcileState>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A panic while holding the lock leaves the data usable (SQLite rolls back
    // the open transaction), so recover instead of poisoning the index.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Index {
    pub fn open(db_path: &Path, categories: &[Category]) -> IResult<Self> {
        let writer = store::open_writer(db_path)?;
        let reader = store::open_reader(db_path)?;
        Ok(Index {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
            roots: Mutex::new(Roots::from_categories(categories)),
            reconciles: Mutex::new(HashMap::new()),
        })
    }

    pub fn default_path() -> PathBuf {
        crate::config::app_config_dir().join("index.db")
    }

    pub fn roots(&self) -> Roots {
        lock(&self.roots).clone()
    }

    /// Roots holding the data of `category`'s targets.
    pub fn roots_for(&self, category: &Category) -> Vec<Root> {
        let roots = lock(&self.roots);
        let mut out: Vec<Root> = Vec::new();
        for t in &category.targets {
            if t.dir.trim().is_empty() {
                continue;
            }
            let dir = store::normalize_dir(&t.dir);
            if let Some(r) = roots.covering(&dir, t.recursive) {
                if !out.contains(r) {
                    out.push(r.clone());
                }
            }
        }
        out
    }

    /// Adopts the roots of a new configuration. Data outside them is removed
    /// by `collect_garbage`; new roots are indexed by `ensure_indexed`.
    pub fn set_categories(&self, categories: &[Category]) {
        *lock(&self.roots) = Roots::from_categories(categories);
    }

    /// Deletes rows no current root tracks, and `roots` entries that are no
    /// longer roots.
    pub fn collect_garbage(&self) -> IResult<()> {
        let roots = self.roots();
        let conn = lock(&self.writer);
        let tx = conn.unchecked_transaction()?;
        let stale: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT id, path FROM dirs")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            let mut out = Vec::new();
            for row in rows {
                let (id, path) = row?;
                if !roots.tracks_dir(Path::new(&path)) {
                    out.push(id);
                }
            }
            out
        };
        for id in stale {
            sync::delete_dir(&tx, id)?;
        }
        let stored = indexed_roots(&tx)?;
        for r in stored.difference(&roots.0.iter().cloned().collect()) {
            tx.prepare_cached("DELETE FROM roots WHERE path = ?1 AND recursive = ?2")?
                .execute(rusqlite::params![store::path_s(&r.path), r.recursive])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Indexes those of `wanted` that have no data yet. Returns the roots
    /// that were built.
    pub fn ensure_indexed(&self, wanted: &[Root]) -> IResult<Vec<Root>> {
        let roots = self.roots();
        let conn = lock(&self.writer);
        let stored = indexed_roots(&conn)?;
        let mut built = Vec::new();
        for root in wanted {
            if stored.contains(root) || !roots.0.contains(root) {
                continue;
            }
            build::build_root(&conn, &roots, root)?;
            built.push(root.clone());
        }
        Ok(built)
    }

    /// Roots that are configured but not indexed yet.
    pub fn cold_roots(&self) -> IResult<Vec<Root>> {
        let stored = indexed_roots(&lock(&self.reader))?;
        Ok(self
            .roots()
            .0
            .into_iter()
            .filter(|r| !stored.contains(r))
            .collect())
    }

    /// Brings `root` up to date with the disk at `level`. Calls arriving
    /// while the same root is being reconciled are folded into that run,
    /// which repeats once at the highest level asked for; a call within
    /// `min_interval` of the last run is skipped. Returns whether anything
    /// changed.
    pub fn reconcile(&self, root: &Root, level: Level, min_interval: Duration) -> IResult<bool> {
        {
            let mut map = lock(&self.reconciles);
            let st = map.entry(root.clone()).or_default();
            if st.running {
                st.rerun = st.rerun.max(Some(level));
                return Ok(false);
            }
            if st.last_done.is_some_and(|t| t.elapsed() < min_interval) {
                return Ok(false);
            }
            st.running = true;
        }
        let mut changed = false;
        let mut level = level;
        let result = loop {
            match self.reconcile_once(root, level) {
                Ok(c) => changed |= c,
                Err(e) => break Err(e),
            }
            let mut map = lock(&self.reconciles);
            let st = map.entry(root.clone()).or_default();
            if let Some(next) = st.rerun.take() {
                level = next;
                continue;
            }
            break Ok(changed);
        };
        let mut map = lock(&self.reconciles);
        let st = map.entry(root.clone()).or_default();
        st.running = false;
        st.rerun = None;
        st.last_done = Some(Instant::now());
        result
    }

    fn reconcile_once(&self, root: &Root, level: Level) -> IResult<bool> {
        let dirs = reconcile::load_dirs(&lock(&self.reader), root)?;
        if dirs.is_empty() {
            // The directory did not exist when it was indexed; build it now
            // if it has appeared.
            if !root.path.is_dir() {
                return Ok(false);
            }
            let roots = self.roots();
            if !roots.0.contains(root) {
                return Ok(false);
            }
            build::build_root(&lock(&self.writer), &roots, root)?;
            return Ok(true);
        }
        let level = if reconcile::coarse_times(&dirs, root) {
            Level::Full
        } else {
            level
        };
        let statuses = reconcile::scan(&dirs, level);
        let roots = self.roots();
        reconcile::apply(&lock(&self.writer), &roots, &dirs, &statuses)
    }

    /// Lists a category from the index. The caller makes sure its roots are
    /// indexed first.
    pub fn list(&self, category: &Category) -> IResult<Vec<FileEntry>> {
        query::list_category(&lock(&self.reader), category)
    }
}

fn indexed_roots(conn: &Connection) -> IResult<HashSet<Root>> {
    let mut stmt = conn.prepare_cached("SELECT path, recursive FROM roots")?;
    let rows = stmt.query_map([], |r| {
        Ok(Root {
            path: PathBuf::from(r.get::<_, String>(0)?),
            recursive: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests;
