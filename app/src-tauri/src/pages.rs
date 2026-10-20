//! Paged file listing: filtering by name, sorting and counting happen here,
//! and the frontend fetches only the rows it shows.
//!
//! The full ordered result of a query (category + search + sort) is computed
//! once and cached; pages are slices of it. With the index the cache holds
//! file ids and is keyed by the index generation, so any write invalidates
//! it. Without the index (fallback) it holds the scanned entries and is
//! rebuilt when the frontend reloads.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::config::Category;
use crate::file_entry::FileEntry;
use crate::index::query::RowRef;
use crate::index::Index;

/// Rows per request at most.
const MAX_LIMIT: usize = 1000;

/// Results that take longer than this to compute are not recomputed on
/// every index change (see `refresh_window`).
const EXPENSIVE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileQuery {
    pub category_id: String,
    #[serde(default)]
    pub search: String,
    /// A column key (`name`, `size`, `createdAt`, …).
    #[serde(default)]
    pub sort_col: String,
    #[serde(default)]
    pub sort_desc: bool,
    pub offset: usize,
    pub limit: usize,
    /// Changes when the frontend reloads; only the fallback uses it.
    #[serde(default)]
    pub reload: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePage {
    /// Number of rows the whole query matches.
    pub total: usize,
    pub offset: usize,
    pub rows: Vec<FileEntry>,
    /// Set when the rows come from a result older than the index: fetch
    /// again after this many milliseconds to see the latest state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortCol {
    Name,
    Extension,
    Size,
    CreatedAt,
    ModifiedAt,
    AccessedAt,
    ParentDir,
    IsReadonly,
}

impl SortCol {
    fn parse(key: &str) -> Self {
        match key {
            "name" => SortCol::Name,
            "extension" => SortCol::Extension,
            "size" => SortCol::Size,
            "modifiedAt" => SortCol::ModifiedAt,
            "accessedAt" => SortCol::AccessedAt,
            "parentDir" => SortCol::ParentDir,
            "isReadonly" => SortCol::IsReadonly,
            _ => SortCol::CreatedAt,
        }
    }
}

/// The fields filtering and sorting need, from either source.
trait Row: Sync {
    fn name(&self) -> &str;
    fn dir(&self) -> &str;
    fn size(&self) -> i64;
    fn created(&self) -> i64;
    fn modified(&self) -> i64;
    fn accessed(&self) -> i64;
    fn readonly(&self) -> bool;
}

impl Row for RowRef<'_> {
    fn name(&self) -> &str {
        self.name
    }
    fn dir(&self) -> &str {
        self.dir
    }
    fn size(&self) -> i64 {
        self.size
    }
    fn created(&self) -> i64 {
        self.created
    }
    fn modified(&self) -> i64 {
        self.modified
    }
    fn accessed(&self) -> i64 {
        self.accessed
    }
    fn readonly(&self) -> bool {
        self.readonly
    }
}

impl Row for FileEntry {
    fn name(&self) -> &str {
        &self.name
    }
    fn dir(&self) -> &str {
        &self.parent_dir
    }
    fn size(&self) -> i64 {
        self.size as i64
    }
    fn created(&self) -> i64 {
        self.created_at
    }
    fn modified(&self) -> i64 {
        self.modified_at
    }
    fn accessed(&self) -> i64 {
        self.accessed_at
    }
    fn readonly(&self) -> bool {
        self.is_readonly
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Int(i64),
    /// Lowercased, so case does not split the order.
    Str(String),
}

fn key<R: Row>(r: &R, col: SortCol) -> Key {
    match col {
        SortCol::Name => Key::Str(r.name().to_lowercase()),
        SortCol::Extension => Key::Str(
            Path::new(r.name())
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
        ),
        SortCol::ParentDir => Key::Str(r.dir().to_lowercase()),
        SortCol::Size => Key::Int(r.size()),
        SortCol::CreatedAt => Key::Int(r.created()),
        SortCol::ModifiedAt => Key::Int(r.modified()),
        SortCol::AccessedAt => Key::Int(r.accessed()),
        SortCol::IsReadonly => Key::Int(r.readonly() as i64),
    }
}

/// Indices of the rows whose name contains `search` (case-insensitive), in
/// the requested order; ties are broken by path so the order is stable.
fn filter_sort<R: Row>(rows: &[R], search: &str, col: SortCol, desc: bool) -> Vec<usize> {
    let needle = search.trim().to_lowercase();
    let mut keyed: Vec<(Key, usize)> = rows
        .par_iter()
        .enumerate()
        .filter(|(_, r)| needle.is_empty() || r.name().to_lowercase().contains(&needle))
        .map(|(i, r)| (key(r, col), i))
        .collect();
    keyed.par_sort_unstable_by(|(ka, a), (kb, b)| {
        let by_key = if desc { kb.cmp(ka) } else { ka.cmp(kb) };
        by_key.then_with(|| {
            let (ra, rb) = (&rows[*a], &rows[*b]);
            ra.dir()
                .cmp(rb.dir())
                .then_with(|| ra.name().cmp(rb.name()))
        })
    });
    keyed.into_iter().map(|(_, i)| i).collect()
}

#[derive(Clone, PartialEq, Eq)]
enum Version {
    Index(u64),
    Scan(u64),
}

#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    category_id: String,
    /// The category's targets, so editing its rules invalidates the result.
    targets: String,
    search: String,
    sort_col: SortCol,
    sort_desc: bool,
    version: Version,
}

impl CacheKey {
    /// Equal apart from the version.
    fn same_query(&self, other: &CacheKey) -> bool {
        self.category_id == other.category_id
            && self.targets == other.targets
            && self.search == other.search
            && self.sort_col == other.sort_col
            && self.sort_desc == other.sort_desc
            && matches!(
                (&self.version, &other.version),
                (Version::Index(_), Version::Index(_))
            )
    }
}

enum Ordered {
    Ids(Vec<i64>),
    Entries(Vec<FileEntry>),
}

impl Ordered {
    fn len(&self) -> usize {
        match self {
            Ordered::Ids(v) => v.len(),
            Ordered::Entries(v) => v.len(),
        }
    }
}

struct Cached {
    key: CacheKey,
    ordered: Arc<Ordered>,
    computed_at: Instant,
    cost: Duration,
}

/// The last query's full result. One table is shown at a time, so one entry
/// is enough; concurrent page requests wait for the first to compute it.
#[derive(Default)]
pub struct PageCache(Mutex<Option<Cached>>);

/// How long an expensive result is reused after the index changed. A busy
/// directory under a category of millions of files changes every few
/// seconds; recomputing each time would keep a core busy, so recomputation
/// is limited to a fifth of the time (at most 10 s stale).
fn refresh_window(cost: Duration) -> Duration {
    if cost < EXPENSIVE {
        Duration::ZERO
    } else {
        (cost * 5).min(Duration::from_secs(10))
    }
}

fn compute(index: Option<&Index>, category: &Category, q: &FileQuery) -> Result<Ordered, String> {
    let col = SortCol::parse(&q.sort_col);
    match index {
        Some(index) => {
            // Only the sort key and id of each match are kept: the whole
            // category is never in memory at once. Ties go by id.
            let needle = q.search.trim().to_lowercase();
            let mut keyed: Vec<(Key, i64)> = Vec::new();
            index
                .visit(category, |r| {
                    if needle.is_empty() || r.name.to_lowercase().contains(&needle) {
                        keyed.push((key(r, col), r.id));
                    }
                })
                .map_err(|e| e.to_string())?;
            let desc = q.sort_desc;
            keyed.par_sort_unstable_by(|(ka, a), (kb, b)| {
                let by_key = if desc { kb.cmp(ka) } else { ka.cmp(kb) };
                by_key.then(a.cmp(b))
            });
            Ok(Ordered::Ids(keyed.into_iter().map(|(_, id)| id).collect()))
        }
        None => {
            let entries = crate::scan::scan_category(category);
            let order = filter_sort(&entries, &q.search, col, q.sort_desc);
            let mut slots: Vec<Option<FileEntry>> = entries.into_iter().map(Some).collect();
            Ok(Ordered::Entries(
                order.into_iter().filter_map(|i| slots[i].take()).collect(),
            ))
        }
    }
}

/// Answers one page of `q`. With the index, the caller makes sure the
/// category's roots are indexed first.
pub fn query(
    index: Option<&Index>,
    cache: &PageCache,
    category: &Category,
    q: &FileQuery,
) -> Result<FilePage, String> {
    let key = CacheKey {
        category_id: category.id.clone(),
        targets: serde_json::to_string(&category.targets).unwrap_or_default(),
        search: q.search.trim().to_string(),
        sort_col: SortCol::parse(&q.sort_col),
        sort_desc: q.sort_desc,
        version: match index {
            Some(i) => Version::Index(i.generation()),
            None => Version::Scan(q.reload),
        },
    };
    let (ordered, retry_after) = {
        let mut guard = cache.0.lock().unwrap_or_else(|e| e.into_inner());
        let reuse = match guard.as_ref() {
            Some(c) if c.key == key => Some((c.ordered.clone(), None)),
            // Same query, only the index moved on: reuse an expensive result
            // for a while, and tell the caller when to look again.
            Some(c) if c.key.same_query(&key) => {
                let window = refresh_window(c.cost);
                let age = c.computed_at.elapsed();
                (age < window).then(|| (c.ordered.clone(), Some(window - age)))
            }
            _ => None,
        };
        match reuse {
            Some(r) => r,
            None => {
                let started = Instant::now();
                let o = Arc::new(compute(index, category, q)?);
                *guard = Some(Cached {
                    key,
                    ordered: o.clone(),
                    computed_at: Instant::now(),
                    cost: started.elapsed(),
                });
                (o, None)
            }
        }
    };

    let total = ordered.len();
    let start = q.offset.min(total);
    let end = (start + q.limit.min(MAX_LIMIT)).min(total);
    let rows = match ordered.as_ref() {
        Ordered::Ids(ids) => index
            .expect("ids come from the index")
            .rows(&ids[start..end])
            .map_err(|e| e.to_string())?,
        Ordered::Entries(entries) => entries[start..end].to_vec(),
    };
    Ok(FilePage {
        total,
        offset: start,
        rows,
        retry_after_ms: retry_after.map(|d| d.as_millis() as u64 + 1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fe(dir: &str, name: &str, size: u64, created: i64) -> FileEntry {
        FileEntry {
            path: format!("{dir}/{name}"),
            name: name.into(),
            extension: Path::new(name)
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default(),
            parent_dir: dir.into(),
            size,
            created_at: created,
            modified_at: 0,
            accessed_at: 0,
            is_readonly: false,
        }
    }

    fn names(rows: &[FileEntry], order: &[usize]) -> Vec<String> {
        order.iter().map(|&i| rows[i].name.clone()).collect()
    }

    #[test]
    fn sorts_filters_and_breaks_ties_by_path() {
        let rows = vec![
            fe("/b", "Beta.txt", 3, 20),
            fe("/a", "alpha.PNG", 1, 10),
            fe("/a", "gamma.txt", 2, 20),
            fe("/c", "Alpha.txt", 5, 30),
        ];
        let o = filter_sort(&rows, "", SortCol::Name, false);
        assert_eq!(names(&rows, &o), ["alpha.PNG", "Alpha.txt", "Beta.txt", "gamma.txt"]);
        let o = filter_sort(&rows, "", SortCol::CreatedAt, true);
        assert_eq!(names(&rows, &o), ["Alpha.txt", "gamma.txt", "Beta.txt", "alpha.PNG"]);
        let o = filter_sort(&rows, "ALPHA", SortCol::Size, false);
        assert_eq!(names(&rows, &o), ["alpha.PNG", "Alpha.txt"]);
        let o = filter_sort(&rows, "", SortCol::Extension, false);
        assert_eq!(names(&rows, &o), ["alpha.PNG", "gamma.txt", "Beta.txt", "Alpha.txt"]);
    }

    #[test]
    fn only_expensive_results_are_reused_after_changes() {
        assert_eq!(refresh_window(Duration::from_millis(60)), Duration::ZERO);
        assert_eq!(refresh_window(Duration::from_millis(1500)), Duration::from_millis(7500));
        assert_eq!(refresh_window(Duration::from_secs(4)), Duration::from_secs(10));
    }

    fn all_pages(
        index: Option<&Index>,
        cache: &PageCache,
        c: &Category,
        q: &FileQuery,
    ) -> (usize, Vec<FileEntry>) {
        let mut out = Vec::new();
        let mut offset = 0;
        loop {
            let page = query(index, cache, c, &FileQuery { offset, limit: 3, ..q.clone() }).unwrap();
            out.extend(page.rows);
            offset += 3;
            if offset >= page.total {
                return (page.total, out);
            }
        }
    }

    #[test]
    fn index_pages_match_the_scan_fallback() {
        use crate::config::TargetRule;
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        for (rel, body) in [("a.txt", "1"), ("B.txt", "22"), ("c.png", "333"), ("sub/d.txt", "4"), ("sub/e.TXT", "55555"), ("sub/f.md", "")] {
            let p = data.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        let c = Category {
            id: "c".into(),
            name: "c".into(),
            order: 0,
            targets: vec![TargetRule {
                id: String::new(),
                dir: data.to_string_lossy().into_owned(),
                match_type: "glob".into(),
                pattern: "*.txt;*.png".into(),
                recursive: true,
            }],
            created_at: 0,
            updated_at: 0,
        };
        let index = Index::open(&tmp.path().join("index.db"), std::slice::from_ref(&c)).unwrap();
        index.ensure_indexed(&index.roots_for(&c)).unwrap();
        let (cache_i, cache_s) = (PageCache::default(), PageCache::default());

        for (search, col, desc) in [("", "name", false), ("", "size", true), ("T", "parentDir", false), ("zzz", "name", false)] {
            let q = FileQuery {
                category_id: "c".into(),
                search: search.into(),
                sort_col: col.into(),
                sort_desc: desc,
                offset: 0,
                limit: 0,
                reload: 0,
            };
            // Ties may come in a different order (by id vs by path), so
            // compare the files and the sequence of sort keys.
            let (ti, ri) = all_pages(Some(&index), &cache_i, &c, &q);
            let (ts, rs) = all_pages(None, &cache_s, &c, &q);
            let what = format!("{search:?} {col} {desc}");
            assert_eq!(ti, ts, "{what}");
            let mut pi: Vec<&str> = ri.iter().map(|r| r.path.as_str()).collect();
            let mut ps: Vec<&str> = rs.iter().map(|r| r.path.as_str()).collect();
            pi.sort();
            ps.sort();
            assert_eq!(pi, ps, "{what}");
            let sort_col = SortCol::parse(col);
            let keys = |rows: &[FileEntry]| rows.iter().map(|r| key(r, sort_col)).collect::<Vec<_>>();
            assert!(keys(&ri) == keys(&rs), "{what}");
        }

        // A write bumps the generation, so the cached result is recomputed.
        let q = FileQuery {
            category_id: "c".into(),
            search: String::new(),
            sort_col: "name".into(),
            sort_desc: false,
            offset: 0,
            limit: 10,
            reload: 0,
        };
        assert_eq!(query(Some(&index), &cache_i, &c, &q).unwrap().total, 5);
        std::fs::write(data.join("new.txt"), "n").unwrap();
        index.apply_paths(&[data.join("new.txt")].into_iter().collect()).unwrap();
        assert_eq!(query(Some(&index), &cache_i, &c, &q).unwrap().total, 6);
    }
}
