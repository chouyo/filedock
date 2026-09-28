//! Locations of datasets, backend state and reports.
//!
//! Everything lives outside the repository (default `~/FileDockBench`,
//! overridable with `FILEDOCK_BENCH_HOME`): datasets hold millions of files,
//! and writing under `src-tauri/` would also retrigger `tauri dev` rebuilds.

use std::path::{Path, PathBuf};

pub fn bench_home() -> PathBuf {
    if let Some(p) = std::env::var_os("FILEDOCK_BENCH_HOME") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("FileDockBench")
}

pub fn datasets_dir() -> PathBuf {
    bench_home().join("datasets")
}

pub fn dataset_root(name: &str) -> PathBuf {
    datasets_dir().join(name)
}

/// Dataset metadata, kept next to (not inside) the dataset so it is never
/// indexed as data.
pub fn manifest_path(name: &str) -> PathBuf {
    datasets_dir().join(format!("{name}.manifest.json"))
}

/// Undo journal of the last `bench_dataset mutate`.
pub fn mutation_journal_path(name: &str) -> PathBuf {
    datasets_dir().join(format!("{name}.mutations.json"))
}

pub fn state_dir(backend: &str, dataset: &str) -> PathBuf {
    bench_home().join("state").join(backend).join(dataset)
}

pub fn results_dir() -> PathBuf {
    bench_home().join("results")
}

/// Total size in bytes of every file under `path` (0 if it does not exist).
pub fn disk_usage(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Path as the string form every backend stores and compares.
pub fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
