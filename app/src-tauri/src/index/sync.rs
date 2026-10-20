//! Covering roots and incremental index updates shared by indexing and
//! reconciliation. Callers wrap the updates in a transaction.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use super::store::{descendant_bounds, dir_mtime, path_s, FileRow};
use super::IResult;
use crate::config::{Category, TargetRule};

/// A directory the index tracks: its own files, plus everything below it
/// when recursive.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Root {
    pub path: PathBuf,
    pub recursive: bool,
}

impl Root {
    /// Whether this root's data contains everything `target` can list.
    pub fn covers(&self, dir: &Path, recursive: bool) -> bool {
        (self.recursive && dir.starts_with(&self.path)) || (self.path == dir && !recursive)
    }
}

/// The minimal set of roots covering every target of every category, so each
/// directory is walked and stored once.
#[derive(Clone, Debug, Default)]
pub struct Roots(pub Vec<Root>);

impl Roots {
    pub fn from_categories(categories: &[Category]) -> Self {
        let targets: Vec<&TargetRule> = categories.iter().flat_map(|c| &c.targets).collect();
        Self::from_targets(&targets)
    }

    pub fn from_targets(targets: &[&TargetRule]) -> Self {
        let mut wanted: Vec<Root> = targets
            .iter()
            .filter(|t| !t.dir.trim().is_empty())
            .map(|t| Root {
                path: super::store::normalize_dir(&t.dir),
                recursive: t.recursive,
            })
            .collect();
        // Recursive roots first, shortest first, so ancestors absorb descendants.
        wanted.sort_by_key(|r| (!r.recursive, r.path.components().count()));
        let mut roots: Vec<Root> = Vec::new();
        for root in wanted {
            if !roots.iter().any(|r| r.covers(&root.path, root.recursive)) {
                roots.push(root);
            }
        }
        Roots(roots)
    }

    /// The root holding the data for a target directory, if any.
    pub fn covering(&self, dir: &Path, recursive: bool) -> Option<&Root> {
        self.0.iter().find(|r| r.covers(dir, recursive))
    }

    /// Whether `dir` itself belongs in `dirs` (a root, or below a recursive one).
    pub fn tracks_dir(&self, dir: &Path) -> bool {
        self.0
            .iter()
            .any(|r| dir == r.path || (r.recursive && dir.starts_with(&r.path)))
    }

    pub fn is_root(&self, dir: &Path) -> bool {
        self.0.iter().any(|r| dir == r.path)
    }
}

pub fn dir_id(conn: &Connection, path: &Path) -> IResult<Option<i64>> {
    Ok(conn
        .prepare_cached("SELECT id FROM dirs WHERE path = ?1")?
        .query_row([path_s(path)], |r| r.get(0))
        .optional()?)
}

/// Returns the directory's id, inserting it (and missing ancestors up to the
/// root) if needed.
pub fn ensure_dir(conn: &Connection, roots: &Roots, path: &Path) -> IResult<i64> {
    if let Some(id) = dir_id(conn, path)? {
        return Ok(id);
    }
    let parent_id = match path.parent() {
        Some(parent) if !roots.is_root(path) && roots.tracks_dir(parent) => {
            Some(ensure_dir(conn, roots, parent)?)
        }
        _ => None,
    };
    let mtime = std::fs::symlink_metadata(path)
        .map(|m| dir_mtime(&m))
        .unwrap_or(0);
    conn.prepare_cached("INSERT INTO dirs(path, parent_id, mtime) VALUES (?1, ?2, ?3)")?
        .execute(params![path_s(path), parent_id, mtime])?;
    Ok(conn.last_insert_rowid())
}

pub fn set_dir_mtime(conn: &Connection, id: i64, mtime: i64) -> IResult<()> {
    conn.prepare_cached("UPDATE dirs SET mtime = ?2 WHERE id = ?1")?
        .execute(params![id, mtime])?;
    Ok(())
}

pub fn insert_file(conn: &Connection, dir_id: i64, row: &FileRow) -> IResult<()> {
    conn.prepare_cached(
        "INSERT INTO files(dir_id, name, ext, size, created, modified, accessed, readonly)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?
    .execute(params![
        dir_id,
        row.name,
        row.ext,
        row.size,
        row.created,
        row.modified,
        row.accessed,
        row.readonly
    ])?;
    Ok(())
}

pub fn upsert_file(conn: &Connection, dir_id: i64, row: &FileRow) -> IResult<()> {
    conn.prepare_cached(
        "INSERT INTO files(dir_id, name, ext, size, created, modified, accessed, readonly)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(dir_id, name) DO UPDATE SET
             size = excluded.size, created = excluded.created, modified = excluded.modified,
             accessed = excluded.accessed, readonly = excluded.readonly",
    )?
    .execute(params![
        dir_id,
        row.name,
        row.ext,
        row.size,
        row.created,
        row.modified,
        row.accessed,
        row.readonly
    ])?;
    Ok(())
}

pub fn delete_file(conn: &Connection, dir_id: i64, name: &str) -> IResult<()> {
    conn.prepare_cached("DELETE FROM files WHERE dir_id = ?1 AND name = ?2")?
        .execute(params![dir_id, name])?;
    Ok(())
}

/// Removes a directory, everything below it and their files.
pub fn delete_subtree(conn: &Connection, path: &Path) -> IResult<()> {
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

/// Removes one directory row and its files, leaving subdirectories alone.
pub fn delete_dir(conn: &Connection, id: i64) -> IResult<()> {
    conn.prepare_cached("DELETE FROM files WHERE dir_id = ?1")?
        .execute([id])?;
    conn.prepare_cached("DELETE FROM dirs WHERE id = ?1")?
        .execute([id])?;
    Ok(())
}

/// Removes whatever a root's data occupies, so it can be indexed afresh.
/// Covering roots never nest inside a recursive root, but a non-recursive
/// root can contain other roots, so only its own row goes.
pub fn clear_root(conn: &Connection, root: &Root) -> IResult<()> {
    if root.recursive {
        delete_subtree(conn, &root.path)
    } else {
        match dir_id(conn, &root.path)? {
            Some(id) => delete_dir(conn, id),
            None => Ok(()),
        }
    }
}

/// Indexes a directory that appeared (created or moved in) and all its content.
pub fn insert_subtree(conn: &Connection, roots: &Roots, path: &Path) -> IResult<()> {
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let ft = entry.file_type();
        if ft.is_dir() {
            if roots.tracks_dir(entry.path()) {
                let id = ensure_dir(conn, roots, entry.path())?;
                if let Ok(m) = entry.metadata() {
                    set_dir_mtime(conn, id, dir_mtime(&m))?;
                }
            }
        } else if ft.is_file() {
            let Some(parent) = entry.path().parent() else {
                continue;
            };
            if !roots.tracks_dir(parent) {
                continue;
            }
            if let Ok(m) = entry.metadata() {
                let id = ensure_dir(conn, roots, parent)?;
                let row =
                    FileRow::from_metadata(entry.file_name().to_string_lossy().into_owned(), &m);
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
                files.push(FileRow::from_metadata(
                    entry.file_name().to_string_lossy().into_owned(),
                    &m,
                ));
            }
        }
    }
    Ok(DirListing {
        mtime,
        files,
        subdirs,
    })
}

#[derive(Default, Debug)]
pub struct ApplyStats {
    pub files_upserted: u64,
    pub files_deleted: u64,
    pub dirs_added: u64,
    pub dirs_removed: u64,
}

impl ApplyStats {
    pub fn changed(&self) -> bool {
        self.files_upserted + self.files_deleted + self.dirs_added + self.dirs_removed > 0
    }
}

/// Makes the index match `listing` for directory `id`.
pub fn apply_listing(
    conn: &Connection,
    roots: &Roots,
    id: i64,
    listing: &DirListing,
    stats: &mut ApplyStats,
) -> IResult<()> {
    let mut stored: HashMap<String, (i64, i64, i64, bool)> = HashMap::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT name, size, modified, created, readonly FROM files WHERE dir_id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        while let Some(r) = rows.next()? {
            stored.insert(r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?));
        }
    }
    for row in &listing.files {
        match stored.remove(&row.name) {
            Some(old) if old == (row.size, row.modified, row.created, row.readonly) => {}
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
        // Another root's directory is that root's business (roots have no
        // parent link, so it would otherwise look new every time).
        if !roots.tracks_dir(sub) || roots.is_root(sub) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn target(dir: &str, recursive: bool) -> TargetRule {
        TargetRule {
            id: String::new(),
            dir: dir.into(),
            match_type: "glob".into(),
            pattern: "*".into(),
            recursive,
        }
    }

    #[test]
    fn covering_roots_absorb_descendants() {
        let sep = std::path::MAIN_SEPARATOR;
        let a = format!("{sep}a");
        let ab = format!("{sep}a{sep}b");
        let c = format!("{sep}c");
        let ts = [
            target(&ab, true),
            target(&a, true),
            target(&a, false),
            target(&c, false),
            target(&format!("{c}{sep}d"), true),
        ];
        let refs: Vec<&TargetRule> = ts.iter().collect();
        let roots = Roots::from_targets(&refs);
        let got: Vec<(String, bool)> = roots
            .0
            .iter()
            .map(|r| (path_s(&r.path), r.recursive))
            .collect();
        assert_eq!(
            got,
            vec![
                (a.clone(), true),
                (format!("{c}{sep}d"), true),
                (c.clone(), false)
            ]
        );
        assert!(roots.covering(Path::new(&ab), false).is_some());
        assert!(!roots.tracks_dir(Path::new(&format!("{c}{sep}e"))));
    }
}
