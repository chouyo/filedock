//! Catching up after changes made while nothing was running.
//!
//! * Level 2 (`dirs`): `stat` every indexed directory in parallel; only those
//!   whose modification time changed are listed again. Adding, removing or
//!   renaming an entry updates its directory's mtime on NTFS, APFS and the
//!   common Linux file systems. Rewriting a file's content does not, so sizes
//!   of files modified in place can stay stale (S5/S6 report them).
//! * Level 3 (`full`): list every directory again and compare.

use std::path::PathBuf;
use std::time::Instant;

use clap::ValueEnum;
use rayon::prelude::*;
use rusqlite::Connection;
use serde_json::json;

use super::store::dir_mtime;
use super::sync::{apply_listing, delete_subtree, read_listing, ApplyStats, DirListing, Roots};
use crate::bench_common::backend::Notes;
use crate::bench_common::BResult;

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum ResumeMode {
    /// Level 2: relist only directories whose mtime changed.
    Dirs,
    /// Level 3: relist every directory.
    Full,
}

enum Status {
    Same,
    Missing,
    Changed(DirListing),
}

pub fn reconcile(conn: &Connection, roots: &Roots, mode: ResumeMode) -> BResult<Notes> {
    let started = Instant::now();
    let dirs: Vec<(i64, PathBuf, i64)> = {
        let mut stmt = conn.prepare("SELECT id, path, mtime FROM dirs")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, PathBuf::from(r.get::<_, String>(1)?), r.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;

    // Parallel file system pass: no database access here.
    let scan_started = Instant::now();
    let statuses: Vec<Status> = dirs
        .par_iter()
        .map(|(_, path, mtime)| match std::fs::symlink_metadata(path) {
            Err(_) => Status::Missing,
            Ok(m) if !m.is_dir() => Status::Missing,
            Ok(m) => {
                if matches!(mode, ResumeMode::Dirs) && dir_mtime(&m) == *mtime {
                    Status::Same
                } else {
                    match read_listing(path) {
                        Ok(listing) => Status::Changed(listing),
                        Err(_) => Status::Missing,
                    }
                }
            }
        })
        .collect();
    let scan_ms = scan_started.elapsed().as_secs_f64() * 1000.0;

    // Sequential apply in one transaction.
    let apply_started = Instant::now();
    let mut stats = ApplyStats::default();
    let (mut missing, mut changed) = (0u64, 0u64);
    conn.execute_batch("BEGIN")?;
    for ((id, path, _), status) in dirs.iter().zip(&statuses) {
        match status {
            Status::Same => {}
            Status::Missing => {
                delete_subtree(conn, path)?;
                missing += 1;
            }
            Status::Changed(listing) => {
                // An ancestor that disappeared may already have removed it.
                if super::sync::dir_id(conn, path)?.is_some() {
                    apply_listing(conn, roots, *id, listing, &mut stats)?;
                }
                changed += 1;
            }
        }
    }
    conn.execute_batch("COMMIT")?;
    let apply_ms = apply_started.elapsed().as_secs_f64() * 1000.0;

    let mut notes = Notes::new();
    notes.insert("mode".into(), json!(format!("{mode:?}")));
    notes.insert("dirsChecked".into(), json!(dirs.len()));
    notes.insert("dirsChanged".into(), json!(changed));
    notes.insert("dirsMissing".into(), json!(missing));
    notes.insert("loadMs".into(), json!(load_ms.round()));
    notes.insert("fsPassMs".into(), json!(scan_ms.round()));
    notes.insert("applyMs".into(), json!(apply_ms.round()));
    notes.insert("applied".into(), json!(stats));
    Ok(notes)
}
