//! SQLite schema and read queries.
//!
//! Paths are normalised: `dirs` holds every directory under the covering
//! roots, `files` holds every file as `(dir_id, name)`. Rules are applied at
//! query time, so changing a category's pattern needs no rescan and
//! overlapping categories share rows.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::{params_from_iter, Connection};

use crate::bench_common::backend::{Entry, Listing};
use crate::bench_common::rules::Rule;
use crate::bench_common::BResult;

pub const SCHEMA_VERSION: i64 = 1;

pub fn open(path: &Path) -> BResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA temp_store = MEMORY;
         PRAGMA cache_size = -65536;
         PRAGMA mmap_size = 268435456;",
    )?;
    Ok(conn)
}

/// Tables only; indexes are created after the bulk load (much faster).
pub fn create_tables(conn: &Connection) -> BResult<()> {
    conn.execute_batch(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
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
         );",
    )?;
    Ok(())
}

pub fn create_indexes(conn: &Connection) -> BResult<()> {
    conn.execute_batch(
        "CREATE UNIQUE INDEX dirs_path ON dirs(path);
         CREATE INDEX dirs_parent ON dirs(parent_id);
         CREATE UNIQUE INDEX files_dir_name ON files(dir_id, name);
         CREATE INDEX files_ext ON files(ext);",
    )?;
    Ok(())
}

/// Trigram full-text index over file names, kept in sync by triggers.
pub fn create_fts(conn: &Connection) -> BResult<()> {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE names_fts USING fts5(
             name, content = 'files', content_rowid = 'id', tokenize = 'trigram'
         );
         INSERT INTO names_fts(names_fts) VALUES ('rebuild');
         CREATE TRIGGER files_ai AFTER INSERT ON files BEGIN
             INSERT INTO names_fts(rowid, name) VALUES (new.id, new.name);
         END;
         CREATE TRIGGER files_ad AFTER DELETE ON files BEGIN
             INSERT INTO names_fts(names_fts, rowid, name) VALUES ('delete', old.id, old.name);
         END;",
    )?;
    Ok(())
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> BResult<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

pub fn get_meta(conn: &Connection, key: &str) -> BResult<Option<String>> {
    let mut stmt = conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = stmt.query([key])?;
    Ok(match rows.next()? {
        Some(r) => Some(r.get(0)?),
        None => None,
    })
}

/// One file's stored metadata.
#[derive(Clone, Debug)]
pub struct FileRow {
    pub name: String,
    pub ext: String,
    pub size: i64,
    pub created: i64,
    pub modified: i64,
    pub accessed: i64,
    pub readonly: bool,
}

fn millis(t: std::io::Result<SystemTime>) -> Option<i64> {
    t.ok().map(crate::util::system_time_to_millis)
}

impl FileRow {
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

/// `(lo, hi)` bounds so `path >= lo AND path < hi` selects every descendant
/// of `dir` using the `dirs_path` index.
pub fn descendant_bounds(dir: &str) -> (String, String) {
    let sep = std::path::MAIN_SEPARATOR;
    let next = char::from_u32(sep as u32 + 1).unwrap();
    (format!("{dir}{sep}"), format!("{dir}{next}"))
}

/// `WHERE` fragment and parameters selecting the rows in a directory scope.
fn scope(dir: &Path, recursive: bool) -> (String, Vec<String>) {
    let d = dir.to_string_lossy().into_owned();
    if recursive {
        let (lo, hi) = descendant_bounds(&d);
        ("(d.path = ? OR (d.path >= ? AND d.path < ?))".into(), vec![d, lo, hi])
    } else {
        ("d.path = ?".into(), vec![d])
    }
}

/// Visits `(path, size, modified)` of every file in `dir`'s scope, with
/// optional extension and name pre-filters pushed down to SQL.
fn for_each_file(
    conn: &Connection,
    dir: &Path,
    recursive: bool,
    exts: Option<&[String]>,
    name_filter: Option<(&str, String)>,
    mut f: impl FnMut(PathBuf, i64, i64),
) -> BResult<()> {
    let (where_scope, mut params) = scope(dir, recursive);
    let mut sql = format!(
        "SELECT d.path, f.name, f.size, f.modified FROM files f JOIN dirs d ON d.id = f.dir_id WHERE {where_scope}"
    );
    if let Some(exts) = exts {
        sql.push_str(&format!(" AND f.ext IN ({})", vec!["?"; exts.len()].join(",")));
        params.extend(exts.iter().cloned());
    }
    if let Some((clause, param)) = name_filter {
        sql.push_str(" AND ");
        sql.push_str(clause);
        params.push(param);
    }
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    while let Some(row) = rows.next()? {
        let dir: String = row.get(0)?;
        let name: String = row.get(1)?;
        f(Path::new(&dir).join(name), row.get(2)?, row.get(3)?);
    }
    Ok(())
}

/// Every file of a rule (scope + pattern).
pub fn rule_entries(conn: &Connection, rule: &Rule, name_filter: Option<(&str, String)>) -> BResult<Vec<Entry>> {
    let matcher = rule.matcher();
    let exts = rule.simple_extensions();
    let mut out = Vec::new();
    for_each_file(conn, &rule.dir, rule.recursive, exts.as_deref(), name_filter, |path, size, modified| {
        let ok = path
            .file_name()
            .map(|n| matcher.matches(&n.to_string_lossy()))
            .unwrap_or(false);
        if ok {
            out.push(Entry {
                path,
                size: size as u64,
                modified,
            });
        }
    })?;
    Ok(out)
}

pub fn list(conn: &Connection, rule: &Rule, limit: usize) -> BResult<Listing> {
    Ok(crate::bench_common::backend::first_page(rule_entries(conn, rule, None)?, limit))
}

/// Escapes `%`, `_` and `\` for `LIKE … ESCAPE '\'`.
fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn search(conn: &Connection, rule: &Rule, text: &str, fts: bool) -> BResult<usize> {
    // The trigram index needs at least three characters.
    let filter = if fts && text.chars().count() >= 3 {
        (
            "f.id IN (SELECT rowid FROM names_fts WHERE names_fts MATCH ?)",
            format!("\"{}\"", text.replace('"', "\"\"")),
        )
    } else {
        ("f.name LIKE ? ESCAPE '\\'", format!("%{}%", like_escape(text)))
    };
    let needle = text.to_lowercase();
    Ok(rule_entries(conn, rule, Some(filter))?
        .iter()
        .filter(|e| crate::bench_common::backend::name_contains(&e.path, &needle))
        .count())
}

pub fn probe(conn: &Connection, dir: &Path) -> BResult<Vec<Entry>> {
    let mut out = Vec::new();
    for_each_file(conn, dir, true, None, None, |path, size, modified| {
        out.push(Entry {
            path,
            size: size as u64,
            modified,
        })
    })?;
    Ok(out)
}

/// Stored covering roots, as JSON `[[path, recursive], …]`.
pub fn roots_json(roots: &[(PathBuf, bool)]) -> String {
    serde_json::to_string(
        &roots
            .iter()
            .map(|(p, r)| (p.to_string_lossy().into_owned(), *r))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
