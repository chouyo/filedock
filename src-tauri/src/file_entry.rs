use std::path::Path;

use crate::util::system_time_to_millis;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub parent_dir: String,
    pub size: u64,
    pub created_at: i64,
    pub modified_at: i64,
    pub accessed_at: i64,
    pub is_readonly: bool,
}

pub fn file_entry_from_path(path: &Path) -> FileEntry {
    let (size, created_at, modified_at, accessed_at, is_readonly) = match std::fs::metadata(path) {
        Ok(m) => {
            let modified = m.modified().map(system_time_to_millis).unwrap_or(0);
            let created = m.created().map(system_time_to_millis).unwrap_or(modified);
            let accessed = m.accessed().map(system_time_to_millis).unwrap_or(modified);
            (m.len(), created, modified, accessed, m.permissions().readonly())
        }
        Err(_) => (0, 0, 0, 0, false),
    };

    let path_str = path.to_string_lossy().to_string();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent_dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    FileEntry {
        path: path_str,
        name,
        extension,
        parent_dir,
        size,
        created_at,
        modified_at,
        accessed_at,
        is_readonly,
    }
}
