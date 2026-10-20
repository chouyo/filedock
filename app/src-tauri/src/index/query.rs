//! Listing a category from the index, with the same result as `scan.rs`.

use std::collections::HashMap;
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

pub fn list_category(conn: &Connection, category: &Category) -> IResult<Vec<FileEntry>> {
    let mut set: HashMap<String, FileEntry> = HashMap::new();

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
            "SELECT d.path, f.name, f.size, f.created, f.modified, f.accessed, f.readonly
             FROM files f JOIN dirs d ON d.id = f.dir_id WHERE {sql}"
        );

        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(params_from_iter(params.iter()))?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if !matcher.matches(&name) {
                continue;
            }
            let parent_dir: String = row.get(0)?;
            let path = Path::new(&parent_dir).join(&name);
            let extension = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            let fe = FileEntry {
                path: path_s(&path),
                name,
                extension,
                parent_dir,
                size: row.get::<_, i64>(2)? as u64,
                created_at: row.get(3)?,
                modified_at: row.get(4)?,
                accessed_at: row.get(5)?,
                is_readonly: row.get(6)?,
            };
            set.insert(fe.path.clone(), fe);
        }
    }

    Ok(set.into_values().collect())
}
