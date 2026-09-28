//! The interface every search approach implements, so all of them run the
//! exact same scenarios.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::rules::Rule;
use super::BResult;

/// One file as the scenarios compare it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub size: u64,
    /// Milliseconds since the Unix epoch.
    pub modified: i64,
}

pub struct Listing {
    /// Number of files in the category.
    pub total: usize,
    /// The first `limit` files, newest first (what the file table shows first).
    pub page: Vec<Entry>,
}

/// Free-form notes a backend attaches to a scenario result (mode used, fallbacks,
/// workarounds applied), stored verbatim in the report.
pub type Notes = serde_json::Map<String, Value>;

pub trait Backend {
    /// Stable identifier used in state paths and reports.
    fn name(&self) -> &'static str;

    /// Builds the index for `rules` from scratch (S1).
    fn index(&mut self, rules: &[Rule]) -> BResult<Notes>;

    /// Brings a persisted index up to date after changes made while no process
    /// was running (S5). Called in a fresh process after [`Backend::open`].
    fn resume(&mut self, rules: &[Rule]) -> BResult<Notes>;

    /// Opens an index a previous process built (before S5 / S6 on their own).
    fn open(&mut self, rules: &[Rule]) -> BResult<()>;

    /// A category's file count and first page (S2).
    fn list(&mut self, rule: &Rule, limit: usize) -> BResult<Listing>;

    /// Number of files in the category whose name contains `text`,
    /// case-insensitively (S3).
    fn search(&mut self, rule: &Rule, text: &str) -> BResult<usize>;

    /// Every file of the category (S5 / S6).
    fn snapshot(&mut self, rule: &Rule) -> BResult<Vec<Entry>>;

    /// Every file under `dir`, recursively and regardless of rules (S4).
    fn probe(&mut self, dir: &Path) -> BResult<Vec<Entry>>;

    /// Starts keeping the index current while running (S4).
    fn start_watch(&mut self, rules: &[Rule]) -> BResult<Notes>;

    fn stop_watch(&mut self);

    /// Bytes the backend's own index occupies on disk, if it has one.
    fn state_bytes(&self) -> Option<u64>;

    /// Whether the index keeps changing on its own after `resume` returns (an
    /// OS-maintained index catching up), so S5 must poll for consistency.
    fn settles_in_background(&self) -> bool {
        false
    }
}

/// Sorts newest first (ties by path) and keeps `limit` entries, without
/// sorting the whole list.
pub fn first_page(mut entries: Vec<Entry>, limit: usize) -> Listing {
    let total = entries.len();
    let cmp = |a: &Entry, b: &Entry| b.modified.cmp(&a.modified).then_with(|| a.path.cmp(&b.path));
    if entries.len() > limit {
        entries.select_nth_unstable_by(limit, cmp);
        entries.truncate(limit);
    }
    entries.sort_unstable_by(cmp);
    Listing { total, page: entries }
}

/// Case-insensitive name filter shared by backends that post-filter.
pub fn name_contains(path: &Path, needle_lower: &str) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().to_lowercase().contains(needle_lower))
        .unwrap_or(false)
}
