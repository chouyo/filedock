use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

use crate::config::Category;
use crate::file_entry::{file_entry_from_path, FileEntry};
use crate::matcher::build_matcher;

pub fn scan_category(category: &Category) -> Vec<FileEntry> {
    let mut set: HashMap<String, FileEntry> = HashMap::new();

    for target in &category.targets {
        let dir = Path::new(&target.dir);
        if !dir.exists() || !dir.is_dir() {
            continue;
        }
        let matcher = match build_matcher(&target.match_type, &target.pattern) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let walker = if target.recursive {
            WalkDir::new(dir).into_iter()
        } else {
            WalkDir::new(dir).max_depth(1).into_iter()
        };

        for entry in walker.filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                if matcher.matches(&name) {
                    let fe = file_entry_from_path(entry.path());
                    set.insert(fe.path.clone(), fe);
                }
            }
        }
    }

    set.into_values().collect()
}
