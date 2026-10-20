//! Bringing an indexed root up to date with the disk.
//!
//! Level 3 of the proposal: list every stored directory of the root again
//! (in parallel, without touching the database), then apply the differences
//! in one transaction. Level 2 (skip directories whose mtime is unchanged)
//! and platform change journals come in later phases.

use std::path::PathBuf;

use rayon::prelude::*;
use rusqlite::{params, Connection};

use super::store::{descendant_bounds, path_s};
use super::sync::{
    apply_listing, delete_dir, delete_subtree, dir_id, read_listing, ApplyStats, DirListing, Root,
    Roots,
};
use super::IResult;

pub enum Status {
    Missing,
    Listed(DirListing),
}

/// Stored directories of `root`: `(id, path)`.
pub fn load_dirs(conn: &Connection, root: &Root) -> IResult<Vec<(i64, PathBuf)>> {
    let p = path_s(&root.path);
    let mut out = Vec::new();
    let mut push = |r: &rusqlite::Row| -> rusqlite::Result<()> {
        out.push((r.get(0)?, PathBuf::from(r.get::<_, String>(1)?)));
        Ok(())
    };
    if root.recursive {
        let (lo, hi) = descendant_bounds(&p);
        let mut stmt = conn.prepare_cached(
            "SELECT id, path FROM dirs WHERE path = ?1 OR (path >= ?2 AND path < ?3)",
        )?;
        let mut rows = stmt.query(params![p, lo, hi])?;
        while let Some(r) = rows.next()? {
            push(r)?;
        }
    } else {
        let mut stmt = conn.prepare_cached("SELECT id, path FROM dirs WHERE path = ?1")?;
        let mut rows = stmt.query([p])?;
        while let Some(r) = rows.next()? {
            push(r)?;
        }
    }
    Ok(out)
}

/// Reads every directory in `dirs` from disk, in parallel.
pub fn scan(dirs: &[(i64, PathBuf)]) -> Vec<Status> {
    dirs.par_iter()
        .map(|(_, path)| match read_listing(path) {
            Ok(listing) => Status::Listed(listing),
            Err(_) => Status::Missing,
        })
        .collect()
}

/// Applies a `scan` result. Returns whether anything changed.
pub fn apply(
    conn: &Connection,
    roots: &Roots,
    dirs: &[(i64, PathBuf)],
    statuses: &[Status],
) -> IResult<bool> {
    let mut stats = ApplyStats::default();
    let tx = conn.unchecked_transaction()?;
    for ((id, path), status) in dirs.iter().zip(statuses) {
        // An ancestor that disappeared may already have removed it, and the
        // root may have been dropped or rebuilt since the scan.
        if dir_id(&tx, path)? != Some(*id) {
            continue;
        }
        match status {
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
