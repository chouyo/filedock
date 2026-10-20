//! Reading a category's files from the index, with the same result as
//! `scan.rs`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{params_from_iter, Connection};

use super::store::{descendant_bounds, normalize_dir, path_s};
use super::IResult;
use crate::config::{Category, TargetRule};
use crate::file_entry::FileEntry;
use crate::matcher::build_matcher;

/// Extensions a glob is limited to, if it is purely `*.ext;*.ext…`, so the
/// filter can use the `files_ext` index.
fn simple_extensions(target: &TargetRule) -> Option<Vec<String>> {
    if target.match_type == "regex" {
        return None;
    }
    let mut exts = Vec::new();
    for part in target
        .pattern
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let ext = part.strip_prefix("*.")?;
        if ext.is_empty() || ext.contains(['*', '?', '[', ']', '.']) {
            return None;
        }
        exts.push(ext.to_lowercase());
    }
    if exts.is_empty() {
        None
    } else {
        Some(exts)
    }
}

/// One matching file as read from the database; borrowed, so visiting
/// millions of rows allocates nothing per row.
pub struct RowRef<'a> {
    pub id: i64,
    pub dir: &'a str,
    pub name: &'a str,
    pub size: i64,
    pub created: i64,
    pub modified: i64,
    pub accessed: i64,
    pub readonly: bool,
}

/// Calls `f` for every file `category` lists, each once.
pub fn visit(
    conn: &Connection,
    category: &Category,
    mut f: impl FnMut(&RowRef<'_>),
) -> IResult<()> {
    // Only overlapping targets can yield a file twice.
    let mut seen: Option<HashSet<i64>> = (category.targets.len() > 1).then(HashSet::new);

    for target in &category.targets {
        let dir = normalize_dir(&target.dir);
        if target.dir.trim().is_empty() || !dir.is_dir() {
            continue;
        }
        let matcher = match build_matcher(&target.match_type, &target.pattern) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let d = path_s(&dir);
        let (mut sql, mut params) = if target.recursive {
            let (lo, hi) = descendant_bounds(&d);
            (
                "(d.path = ? OR (d.path >= ? AND d.path < ?))".to_string(),
                vec![d, lo, hi],
            )
        } else {
            ("d.path = ?".to_string(), vec![d])
        };
        if let Some(exts) = simple_extensions(target) {
            // `*.png` also matches a file named `.png`, which has no
            // extension; the matcher below decides those.
            sql.push_str(&format!(
                " AND (f.ext IN ({}) OR f.ext = '')",
                vec!["?"; exts.len()].join(",")
            ));
            params.extend(exts);
        }
        let sql = format!(
            "SELECT f.id, d.path, f.name, f.size, f.created, f.modified, f.accessed, f.readonly
             FROM files f JOIN dirs d ON d.id = f.dir_id WHERE {sql}"
        );

        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(params_from_iter(params.iter()))?;
        while let Some(row) = rows.next()? {
            let name = row.get_ref(2)?.as_str()?;
            if !matcher.matches(name) {
                continue;
            }
            let id: i64 = row.get(0)?;
            if let Some(seen) = seen.as_mut() {
                if !seen.insert(id) {
                    continue;
                }
            }
            f(&RowRef {
                id,
                dir: row.get_ref(1)?.as_str()?,
                name,
                size: row.get(3)?,
                created: row.get(4)?,
                modified: row.get(5)?,
                accessed: row.get(6)?,
                readonly: row.get(7)?,
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn entry(
    parent_dir: String,
    name: String,
    size: i64,
    created_at: i64,
    modified_at: i64,
    accessed_at: i64,
    is_readonly: bool,
) -> FileEntry {
    let path = Path::new(&parent_dir).join(&name);
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    FileEntry {
        path: path_s(&path),
        name,
        extension,
        parent_dir,
        size: size as u64,
        created_at,
        modified_at,
        accessed_at,
        is_readonly,
    }
}

impl RowRef<'_> {
    #[cfg(test)]
    pub fn to_entry(&self) -> FileEntry {
        entry(
            self.dir.to_string(),
            self.name.to_string(),
            self.size,
            self.created,
            self.modified,
            self.accessed,
            self.readonly,
        )
    }
}

/// The files with these ids, in the same order; ids no longer stored are
/// skipped.
pub fn rows(conn: &Connection, ids: &[i64]) -> IResult<Vec<FileEntry>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT f.id, d.path, f.name, f.size, f.created, f.modified, f.accessed, f.readonly
         FROM files f JOIN dirs d ON d.id = f.dir_id WHERE f.id IN ({})",
        vec!["?"; ids.len()].join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(ids.iter()))?;
    let mut by_id: HashMap<i64, FileEntry> = HashMap::with_capacity(ids.len());
    while let Some(row) = rows.next()? {
        by_id.insert(
            row.get(0)?,
            entry(
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ),
        );
    }
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}
