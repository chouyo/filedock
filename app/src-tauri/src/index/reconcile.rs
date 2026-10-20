//! Bringing an indexed root up to date with the disk.
//!
//! Every stored directory of the root is checked in parallel without
//! touching the database, then the differences are applied in one
//! transaction.
//!
//! * Level 2 (`Level::Dirs`): `stat` each directory and list again only those
//!   whose modification time changed. Adding, removing or renaming an entry
//!   updates its directory's mtime on NTFS, APFS and the common Linux file
//!   systems, so the set of files is exact. Rewriting a file's content does
//!   not, so size and times of files modified in place can stay stale.
//! * Level 3 (`Level::Full`): list every directory again.
//!
//! Platform change journals (level 1) come in later phases.

use std::path::PathBuf;

use rayon::prelude::*;
use rusqlite::{params, Connection};

use super::store::{descendant_bounds, dir_mtime, path_s};
use super::sync::{
    apply_listing, delete_dir, delete_subtree, dir_id, read_listing, ApplyStats, DirListing, Root,
    Roots,
};
use super::IResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Level 2: list again only directories whose mtime changed.
    Dirs,
    /// Level 3: list every directory again.
    Full,
}

pub enum Status {
    Same,
    Missing,
    Listed(DirListing),
}

/// A stored directory: id, path and recorded mtime (nanoseconds).
pub struct StoredDir {
    pub id: i64,
    pub path: PathBuf,
    pub mtime: i64,
}

/// Stored directories of `root`.
pub fn load_dirs(conn: &Connection, root: &Root) -> IResult<Vec<StoredDir>> {
    let p = path_s(&root.path);
    let mut out = Vec::new();
    let mut push = |r: &rusqlite::Row| -> rusqlite::Result<()> {
        out.push(StoredDir {
            id: r.get(0)?,
            path: PathBuf::from(r.get::<_, String>(1)?),
            mtime: r.get(2)?,
        });
        Ok(())
    };
    if root.recursive {
        let (lo, hi) = descendant_bounds(&p);
        let mut stmt = conn.prepare_cached(
            "SELECT id, path, mtime FROM dirs WHERE path = ?1 OR (path >= ?2 AND path < ?3)",
        )?;
        let mut rows = stmt.query(params![p, lo, hi])?;
        while let Some(r) = rows.next()? {
            push(r)?;
        }
    } else {
        let mut stmt = conn.prepare_cached("SELECT id, path, mtime FROM dirs WHERE path = ?1")?;
        let mut rows = stmt.query([p])?;
        while let Some(r) = rows.next()? {
            push(r)?;
        }
    }
    Ok(out)
}

/// Whether directory times on this volume are too coarse for level 2.
///
/// FAT keeps 2-second and exFAT 10-millisecond directory times, so two
/// changes close together look like one and the second would be missed.
/// NTFS (100 ns) and APFS / ext4 (1 ns) times are practically never a whole
/// number of milliseconds, so a root whose recorded mtime is one is checked
/// in full. A false positive only costs speed.
pub fn coarse_times(dirs: &[StoredDir], root: &Root) -> bool {
    dirs.iter()
        .find(|d| d.path == root.path)
        .is_some_and(|d| d.mtime % 1_000_000 == 0)
}

/// Checks every directory in `dirs` against the disk, in parallel.
pub fn scan(dirs: &[StoredDir], level: Level) -> Vec<Status> {
    dirs.par_iter()
        .map(|d| {
            if level == Level::Dirs {
                match std::fs::symlink_metadata(&d.path) {
                    Ok(m) if !m.is_dir() => return Status::Missing,
                    Ok(m) if dir_mtime(&m) == d.mtime => return Status::Same,
                    Ok(_) => {}
                    Err(_) => return Status::Missing,
                }
            }
            match read_listing(&d.path) {
                Ok(listing) => Status::Listed(listing),
                Err(_) => Status::Missing,
            }
        })
        .collect()
}

/// Applies a `scan` result. Returns whether anything changed.
pub fn apply(
    conn: &Connection,
    roots: &Roots,
    dirs: &[StoredDir],
    statuses: &[Status],
) -> IResult<bool> {
    let mut stats = ApplyStats::default();
    let tx = conn.unchecked_transaction()?;
    for (d, status) in dirs.iter().zip(statuses) {
        if matches!(status, Status::Same) {
            continue;
        }
        let (id, path) = (&d.id, &d.path);
        // An ancestor that disappeared may already have removed it, and the
        // root may have been dropped or rebuilt since the scan.
        if dir_id(&tx, path)? != Some(*id) {
            continue;
        }
        match status {
            Status::Same => {}
            Status::Missing => {
                // Only a non-recursive root can contain other roots.
                if roots.0.iter().any(|r| r.path == *path && !r.recursive) {
                    delete_dir(&tx, *id)?;
                } else {
                    delete_subtree(&tx, path)?;
                }
                stats.dirs_removed += 1;
            }
            Status::Listed(listing) => apply_listing(&tx, roots, *id, listing, &mut stats)?,
        }
    }
    tx.commit()?;
    Ok(stats.changed())
}
