//! Ground truth: what the app's current scanner (`scan.rs`) returns.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::backend::Entry;
use super::rules::Rule;
use crate::scan::scan_category;

/// The category's files exactly as the shipping app computes them.
pub fn scan_rule(rule: &Rule) -> Vec<Entry> {
    scan_category(&rule.to_category())
        .into_iter()
        .map(|fe| Entry {
            path: PathBuf::from(fe.path),
            size: fe.size,
            modified: fe.modified_at,
        })
        .collect()
}

/// Every file under `dir`, recursively.
pub fn scan_dir(dir: &Path) -> Vec<Entry> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let meta = e.metadata().ok();
            Entry {
                path: e.path().to_path_buf(),
                size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: meta
                    .and_then(|m| m.modified().ok())
                    .map(crate::util::system_time_to_millis)
                    .unwrap_or(0),
            }
        })
        .collect()
}

#[derive(Default, Debug)]
pub struct Diff {
    /// In the truth, absent from the backend.
    pub missing: Vec<PathBuf>,
    /// In the backend, absent from the truth.
    pub extra: Vec<PathBuf>,
    /// Present in both, but with a different size.
    pub stale_size: Vec<PathBuf>,
}

impl Diff {
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.extra.is_empty() && self.stale_size.is_empty()
    }

    pub fn to_json(&self, samples: usize) -> serde_json::Value {
        let sample = |v: &Vec<PathBuf>| -> Vec<String> {
            v.iter().take(samples).map(|p| p.to_string_lossy().into_owned()).collect()
        };
        serde_json::json!({
            "missing": self.missing.len(),
            "extra": self.extra.len(),
            "staleSize": self.stale_size.len(),
            "missingSample": sample(&self.missing),
            "extraSample": sample(&self.extra),
            "staleSizeSample": sample(&self.stale_size),
            "missingInHiddenDirs": self.missing.iter().filter(|p| in_hidden_dir(p)).count(),
        })
    }
}

pub fn diff(truth: &[Entry], actual: &[Entry]) -> Diff {
    let actual_map: HashMap<&Path, &Entry> = actual.iter().map(|e| (e.path.as_path(), e)).collect();
    let truth_map: HashMap<&Path, &Entry> = truth.iter().map(|e| (e.path.as_path(), e)).collect();
    let mut d = Diff::default();
    for t in truth {
        match actual_map.get(t.path.as_path()) {
            None => d.missing.push(t.path.clone()),
            Some(a) if a.size != t.size => d.stale_size.push(t.path.clone()),
            Some(_) => {}
        }
    }
    for a in actual {
        if !truth_map.contains_key(a.path.as_path()) {
            d.extra.push(a.path.clone());
        }
    }
    d.missing.sort();
    d.extra.sort();
    d.stale_size.sort();
    d
}

/// Whether any directory component starts with `.` (Spotlight skips these).
pub fn in_hidden_dir(path: &Path) -> bool {
    path.parent()
        .map(|p| {
            p.components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .unwrap_or(false)
}
