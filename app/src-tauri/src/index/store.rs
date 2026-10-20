//! SQLite schema, connection setup and path helpers.
//!
//! Paths are normalised: `dirs` holds every directory under the covering
//! roots, `files` holds every file as `(dir_id, name)`. Category rules are
//! applied at query time, so changing a pattern needs no rescan and
//! overlapping categories share rows. `roots` records which covering roots
//! have been fully indexed.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::Connection;

use super::IResult;
use crate::util::system_time_to_millis;

/// Bump on any schema change; an index with another version is rebuilt.
pub const SCHEMA_VERSION: i64 = 1;

const PRAGMAS: &str = "PRAGMA journal_mode = WAL;
     PRAGMA synchronous = NORMAL;
     PRAGMA temp_store = MEMORY;
     PRAGMA cache_size = -65536;
     PRAGMA mmap_size = 268435456;
     PRAGMA busy_timeout = 5000;";

const SCHEMA: &str = "CREATE TABLE roots (
         path TEXT NOT NULL,
         recursive INTEGER NOT NULL,
         indexed_at INTEGER NOT NULL,
         walk_errors INTEGER NOT NULL DEFAULT 0,
         PRIMARY KEY (path, recursive)
     );
     CREATE TABLE dirs (
         id INTEGER PRIMARY KEY,
         path TEXT NOT NULL,
         parent_id INTEGER,
         mtime INTEGER NOT NULL DEFAULT 0
     );
     CREATE TABLE files (
         id INTEGER PRIMARY KEY,
         dir_id INTEGER NOT NULL,
         name TEXT NOT NULL,
         ext TEXT NOT NULL,
         size INTEGER NOT NULL,
         created INTEGER NOT NULL,
         modified INTEGER NOT NULL,
         accessed INTEGER NOT NULL,
         readonly INTEGER NOT NULL
     );
     CREATE UNIQUE INDEX dirs_path ON dirs(path);
     CREATE INDEX dirs_parent ON dirs(parent_id);
     CREATE UNIQUE INDEX files_dir_name ON files(dir_id, name);
     CREATE INDEX files_ext ON files(ext);";

fn connect(path: &Path) -> IResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(PRAGMAS)?;
    Ok(conn)
}

fn try_open_writer(path: &Path) -> IResult<Connection> {
    let conn = connect(path)?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version == 0 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA)?;
        tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
        tx.commit()?;
    } else if version != SCHEMA_VERSION {
        return Err(format!("index schema version {version}, expected {SCHEMA_VERSION}").into());
    }
    Ok(conn)
}

fn remove_db_files(path: &Path) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(p));
    }
}

/// Opens (creating if needed) the writer connection. The index is only a
/// cache: an outdated schema or a file that cannot be opened is deleted and
/// rebuilt from scratch.
pub fn open_writer(path: &Path) -> IResult<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match try_open_writer(path) {
        Ok(conn) => Ok(conn),
        Err(_) => {
            remove_db_files(path);
            try_open_writer(path)
        }
    }
}

pub fn open_reader(path: &Path) -> IResult<Connection> {
    let conn = connect(path)?;
    conn.execute_batch("PRAGMA query_only = 1;")?;
    Ok(conn)
}

/// One file's stored metadata.
#[derive(Clone, Debug)]
pub struct FileRow {
    pub name: String,
    /// Lowercase, without the dot; used to push `*.ext` patterns down to SQL.
    pub ext: String,
    pub size: i64,
    pub created: i64,
    pub modified: i64,
    pub accessed: i64,
    pub readonly: bool,
}

fn millis(t: std::io::Result<SystemTime>) -> Option<i64> {
    t.ok().map(system_time_to_millis)
}

impl FileRow {
    /// Same fallbacks as `file_entry_from_metadata`.
    pub fn from_metadata(name: String, meta: &std::fs::Metadata) -> Self {
        let modified = millis(meta.modified()).unwrap_or(0);
        let ext = Path::new(&name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        FileRow {
            ext,
            size: meta.len() as i64,
            created: millis(meta.created()).unwrap_or(modified),
            modified,
            accessed: millis(meta.accessed()).unwrap_or(modified),
            readonly: meta.permissions().readonly(),
            name,
        }
    }
}

/// Directory modification time in nanoseconds (finer than file times, so two
/// changes within a millisecond are still told apart).
pub fn dir_mtime(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Canonical spelling of a configured directory: no trailing separator, no
/// `.` components. Walkers started from this path report children as
/// `root/<name>`, so stored paths and query scopes always agree.
pub fn normalize_dir(dir: &str) -> PathBuf {
    Path::new(dir).components().collect()
}

pub fn path_s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// `(lo, hi)` bounds so `path >= lo AND path < hi` selects every descendant
/// of `dir` using the `dirs_path` index.
pub fn descendant_bounds(dir: &str) -> (String, String) {
    let sep = std::path::MAIN_SEPARATOR;
    let next = char::from_u32(sep as u32 + 1).unwrap();
    // Drive and file system roots (`C:\`, `/`) already end with a separator.
    let base = dir.strip_suffix(sep).unwrap_or(dir);
    (format!("{base}{sep}"), format!("{base}{next}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_trailing_separator() {
        let sep = std::path::MAIN_SEPARATOR;
        let raw = format!("{sep}a{sep}b{sep}");
        assert_eq!(path_s(&normalize_dir(&raw)), format!("{sep}a{sep}b"));
    }

    #[test]
    fn bounds_select_only_descendants() {
        let sep = std::path::MAIN_SEPARATOR;
        let dir = format!("{sep}a");
        let (lo, hi) = descendant_bounds(&dir);
        let inside = format!("{sep}a{sep}b");
        let sibling = format!("{sep}ab");
        assert!(inside >= lo && inside < hi);
        assert!(!(sibling >= lo && sibling < hi));
        // A root that already ends with a separator.
        let (lo, _) = descendant_bounds(&sep.to_string());
        assert_eq!(lo, sep.to_string());
    }
}
