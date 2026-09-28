//! Incremental index updates shared by the watcher and resume.
//! Callers wrap them in a transaction.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use super::store::{descendant_bounds, dir_mtime, FileRow};
use crate::bench_common::BResult;

/// Covering roots: which directories the index tracks.
#[derive(Clone)]
pub struct Roots(pub Vec<(PathBuf, bool)>);

impl Roots {
    /// Whether `dir` itself belongs in `dirs` (a root, or below a recursive one).
    pub fn tracks_dir(&self, dir: &Path) -> bool {
        self.0
            .iter()
            .any(|(root, recursive)| dir == root || (*recursive && dir.starts_with(root)))
    }

    pub fn is_root(&self, dir: &Path) -> bool {
        self.0.iter().any(|(root, _)| dir == root)
    }
}

fn path_s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

pub fn dir_id(conn: &Connection, path: &Path) -> BResult<Option<i64>> {
    Ok(conn
        .prepare_cached("SELECT id FROM dirs WHERE path = ?1")?
        .query_row([path_s(path)], |r| r.get(0))
        .optional()?)
}

/// Returns the directory's id, inserting it (and missing ancestors up to the
/// root) if needed; the flag is true when `path` itself was inserted.
pub fn ensure_dir(conn: &Connection, roots: &Roots, path: &Path) -> BResult<(i64, bool)> {
    if let Some(id) = dir_id(conn, path)? {
        return Ok((id, false));
    }
    let parent_id = match path.parent() {
        Some(parent) if !roots.is_root(path) && roots.tracks_dir(parent) => {
            Some(ensure_dir(conn, roots, parent)?.0)
        }
        _ => None,
    };
    let mtime = std::fs::symlink_metadata(path).map(|m| dir_mtime(&m)).unwrap_or(0);
    conn.prepare_cached("INSERT INTO dirs(path, parent_id, mtime) VALUES (?1, ?2, ?3)")?
        .execute(params![path_s(path), parent_id, mtime])?;
    Ok((conn.last_insert_rowid(), true))
}

pub fn set_dir_mtime(conn: &Connection, id: i64, mtime: i64) -> BResult<()> {
    conn.prepare_cached("UPDATE dirs SET mtime = ?2 WHERE id = ?1")?
        .execute(params![id, mtime])?;
    Ok(())
}

pub fn upsert_file(conn: &Connection, dir_id: i64, row: &FileRow) -> BResult<()> {
    conn.prepare_cached(
        "INSERT INTO files(dir_id, name, ext, size, created, modified, accessed, readonly)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(dir_id, name) DO UPDATE SET
             size = excluded.size, created = excluded.created, modified = excluded.modified,
             accessed = excluded.accessed, readonly = excluded.readonly",
    )?
    .execute(params![
        dir_id, row.name, row.ext, row.size, row.created, row.modified, row.accessed, row.readonly
    ])?;
    Ok(())
}

pub fn delete_file(conn: &Connection, dir_id: i64, name: &str) -> BResult<()> {
    conn.prepare_cached("DELETE FROM files WHERE dir_id = ?1 AND name = ?2")?
        .execute(params![dir_id, name])?;
    Ok(())
}

/// Removes a directory, everything below it and their files.
pub fn delete_subtree(conn: &Connection, path: &Path) -> BResult<()> {
    let p = path_s(path);
    let (lo, hi) = descendant_bounds(&p);
    conn.prepare_cached(
        "DELETE FROM files WHERE dir_id IN
             (SELECT id FROM dirs WHERE path = ?1 OR (path >= ?2 AND path < ?3))",
    )?
    .execute(params![p, lo, hi])?;
    conn.prepare_cached("DELETE FROM dirs WHERE path = ?1 OR (path >= ?2 AND path < ?3)")?
        .execute(params![p, lo, hi])?;
    Ok(())
}

/// Indexes a directory that appeared (created or moved in) and all its content.
pub fn insert_subtree(conn: &Connection, roots: &Roots, path: &Path) -> BResult<()> {
    for entry in walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        let ft = entry.file_type();
        if ft.is_dir() {
            if roots.tracks_dir(entry.path()) {
                let (id, _) = ensure_dir(conn, roots, entry.path())?;
                if let Ok(m) = entry.metadata() {
                    set_dir_mtime(conn, id, dir_mtime(&m))?;
                }
            }
        } else if ft.is_file() {
            let parent = entry.path().parent().unwrap();
            if !roots.tracks_dir(parent) {
                continue;
            }
            if let Ok(m) = entry.metadata() {
                let (id, _) = ensure_dir(conn, roots, parent)?;
                let row = FileRow::from_metadata(entry.file_name().to_string_lossy().into_owned(), &m);
                upsert_file(conn, id, &row)?;
            }
        }
    }
    Ok(())
}

/// A directory's current content on disk.
pub struct DirListing {
    pub mtime: i64,
    pub files: Vec<FileRow>,
    pub subdirs: Vec<PathBuf>,
}

/// Reads one directory level (no database access, so it can run in parallel).
pub fn read_listing(path: &Path) -> std::io::Result<DirListing> {
    let mtime = dir_mtime(&std::fs::symlink_metadata(path)?);
    let mut files = Vec::new();
    let mut subdirs = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            subdirs.push(entry.path());
        } else if ft.is_file() {
            if let Ok(m) = entry.metadata() {
                files.push(FileRow::from_metadata(entry.file_name().to_string_lossy().into_owned(), &m));
            }
        }
    }
    Ok(DirListing { mtime, files, subdirs })
}

#[derive(Default, Debug, serde::Serialize)]
pub struct ApplyStats {
    pub files_upserted: u64,
    pub files_deleted: u64,
    pub dirs_added: u64,
    pub dirs_removed: u64,
}

/// Makes the index match `listing` for directory `id`.
pub fn apply_listing(
    conn: &Connection,
    roots: &Roots,
    id: i64,
    listing: &DirListing,
    stats: &mut ApplyStats,
) -> BResult<()> {
    let mut stored: HashMap<String, (i64, i64)> = HashMap::new();
    {
        let mut stmt = conn.prepare_cached("SELECT name, size, modified FROM files WHERE dir_id = ?1")?;
        let mut rows = stmt.query([id])?;
        while let Some(r) = rows.next()? {
            stored.insert(r.get(0)?, (r.get(1)?, r.get(2)?));
        }
    }
    for row in &listing.files {
        match stored.remove(&row.name) {
            Some((size, modified)) if size == row.size && modified == row.modified => {}
            _ => {
                upsert_file(conn, id, row)?;
                stats.files_upserted += 1;
            }
        }
    }
    for name in stored.keys() {
        delete_file(conn, id, name)?;
        stats.files_deleted += 1;
    }

    let mut stored_dirs: HashSet<PathBuf> = HashSet::new();
    {
        let mut stmt = conn.prepare_cached("SELECT path FROM dirs WHERE parent_id = ?1")?;
        let mut rows = stmt.query([id])?;
        while let Some(r) = rows.next()? {
            stored_dirs.insert(PathBuf::from(r.get::<_, String>(0)?));
        }
    }
    for sub in &listing.subdirs {
        if !roots.tracks_dir(sub) {
            continue;
        }
        if !stored_dirs.remove(sub) {
            insert_subtree(conn, roots, sub)?;
            stats.dirs_added += 1;
        }
    }
    for gone in &stored_dirs {
        delete_subtree(conn, gone)?;
        stats.dirs_removed += 1;
    }
    set_dir_mtime(conn, id, listing.mtime)?;
    Ok(())
}
