//! Applying file system events to the index path by path.
//!
//! Event kinds differ between platforms (FSEvents coalesces, Windows reports
//! a moved directory but not its content), so they are not trusted: every
//! reported path is looked up on disk and the index is made to match it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::store::FileRow;
use super::sync::{
    apply_listing, delete_dir, delete_subtree, dir_id, ensure_dir, insert_subtree, read_listing,
    upsert_file, ApplyStats, DirListing, Roots,
};
use super::IResult;

/// What a path is on disk right now.
pub enum PathState {
    Gone,
    File(FileRow),
    Dir(DirListing),
}

/// Reads the state of each path (no database access).
pub fn probe(paths: &[PathBuf]) -> Vec<PathState> {
    paths
        .iter()
        .map(|p| match std::fs::symlink_metadata(p) {
            Ok(m) if m.is_file() => match p.file_name() {
                Some(name) => PathState::File(FileRow::from_metadata(
                    name.to_string_lossy().into_owned(),
                    &m,
                )),
                None => PathState::Gone,
            },
            Ok(m) if m.is_dir() => match read_listing(p) {
                Ok(listing) => PathState::Dir(listing),
                Err(_) => PathState::Gone,
            },
            // Symlinks are not listed, as in `scan.rs`.
            _ => PathState::Gone,
        })
        .collect()
}

/// A part of the tree whose content changed, for deciding which categories
/// to notify.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    /// Files directly in this directory.
    Files(PathBuf),
    /// This directory and everything below it.
    Tree(PathBuf),
}

impl Change {
    /// Whether a target `(dir, recursive)` can list anything this change touched.
    pub fn affects(&self, dir: &Path, recursive: bool) -> bool {
        match self {
            Change::Files(d) => d == dir || (recursive && d.starts_with(dir)),
            // The target lies inside the changed tree, or a recursive target
            // contains it.
            Change::Tree(d) => dir.starts_with(d) || (recursive && d.starts_with(dir)),
        }
    }
}

fn stored_file(
    conn: &Connection,
    dir_id: i64,
    name: &str,
) -> IResult<Option<(i64, i64, i64, bool)>> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .prepare_cached(
            "SELECT size, modified, created, readonly FROM files WHERE dir_id = ?1 AND name = ?2",
        )?
        .query_row(rusqlite::params![dir_id, name], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .optional()?)
}

/// Removes the file `name` from `parent`, if stored. Returns whether it was.
fn remove_file(conn: &Connection, parent: &Path, name: &str) -> IResult<bool> {
    let Some(pid) = dir_id(conn, parent)? else {
        return Ok(false);
    };
    Ok(conn
        .prepare_cached("DELETE FROM files WHERE dir_id = ?1 AND name = ?2")?
        .execute(rusqlite::params![pid, name])?
        > 0)
}

/// Removes the directory `path` and what it holds, if stored. Returns
/// whether it was.
fn remove_dir(conn: &Connection, roots: &Roots, path: &Path) -> IResult<bool> {
    let Some(id) = dir_id(conn, path)? else {
        return Ok(false);
    };
    // Only a non-recursive root can contain other roots.
    if roots.0.iter().any(|r| r.path == path && !r.recursive) {
        delete_dir(conn, id)?;
    } else {
        delete_subtree(conn, path)?;
    }
    Ok(true)
}

/// Makes the index match `states` (from `probe`) for `paths`, in one
/// transaction. Returns what changed.
pub fn apply(
    conn: &Connection,
    roots: &Roots,
    paths: &[PathBuf],
    states: &[PathState],
) -> IResult<HashSet<Change>> {
    let mut changes = HashSet::new();
    let tx = conn.unchecked_transaction()?;
    for (path, state) in paths.iter().zip(states) {
        let parent = path.parent();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        match state {
            PathState::Gone => {
                if let (Some(parent), Some(name)) = (parent, &name) {
                    if remove_file(&tx, parent, name)? {
                        changes.insert(Change::Files(parent.to_path_buf()));
                    }
                }
                if remove_dir(&tx, roots, path)? {
                    changes.insert(Change::Tree(path.clone()));
                }
            }
            PathState::File(row) => {
                let (Some(parent), Some(name)) = (parent, &name) else {
                    continue;
                };
                // A directory replaced by a file of the same name.
                if remove_dir(&tx, roots, path)? {
                    changes.insert(Change::Tree(path.clone()));
                }
                if !roots.tracks_dir(parent) {
                    continue;
                }
                let pid = ensure_dir(&tx, roots, parent)?;
                let same = stored_file(&tx, pid, name)?
                    == Some((row.size, row.modified, row.created, row.readonly));
                if !same {
                    upsert_file(&tx, pid, row)?;
                    changes.insert(Change::Files(parent.to_path_buf()));
                }
            }
            PathState::Dir(listing) => {
                // A file replaced by a directory of the same name.
                if let (Some(parent), Some(name)) = (parent, &name) {
                    if remove_file(&tx, parent, name)? {
                        changes.insert(Change::Files(parent.to_path_buf()));
                    }
                }
                if !roots.tracks_dir(path) {
                    continue;
                }
                match dir_id(&tx, path)? {
                    None => {
                        insert_subtree(&tx, roots, path)?;
                        changes.insert(Change::Tree(path.clone()));
                    }
                    Some(id) => {
                        let mut stats = ApplyStats::default();
                        apply_listing(&tx, roots, id, listing, &mut stats)?;
                        if stats.changed() {
                            changes.insert(Change::Tree(path.clone()));
                        }
                    }
                }
            }
        }
    }
    tx.commit()?;
    Ok(changes)
}
