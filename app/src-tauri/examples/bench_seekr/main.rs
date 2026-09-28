//! Approach 1: seekr (github.com/muhammad-fiaz/seekr, pinned in Cargo.toml),
//! used through its public API without modifying it.
//!
//! seekr has no notion of a category (directory + glob + recursion), does not
//! write watcher events back to its index, and has no restart catch-up
//! beyond re-walking. The glue below fills those gaps with the smallest code
//! seekr's API allows; every piece is marked `GLUE:` and listed in the
//! report's notes so its cost is attributed correctly.
//!
//! ```text
//! cargo run --release --example bench_seekr -- --dataset 100k-seed42
//! cargo run --release --example bench_seekr -- --dataset 100k-seed42 --mode indexer
//! ```

#[allow(dead_code)]
#[path = "../../src/util.rs"]
mod util;
#[allow(dead_code)]
#[path = "../../src/matcher.rs"]
mod matcher;
#[allow(dead_code)]
#[path = "../../src/file_entry.rs"]
mod file_entry;
#[allow(dead_code)]
#[path = "../../src/config.rs"]
mod config;
#[allow(dead_code)]
#[path = "../../src/scan.rs"]
mod scan;
#[path = "../bench_common/mod.rs"]
mod bench_common;

use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use serde_json::{json, Map};

use seekr::core::SeekrApp;
use seekr::database::Database;
use seekr::types::{AppConfig, FileEvent, IndexerConfig, SearchQuery, WatchConfig};
use seekr::watcher::{watch_directory, WatcherHandle};

use bench_common::backend::{first_page, name_contains, Backend, Entry, Listing, Notes};
use bench_common::cli::CommonArgs;
use bench_common::metrics::{ms, time};
use bench_common::paths::{disk_usage, state_dir};
use bench_common::rules::{covering_roots, Rule};
use bench_common::{scenarios, BResult};

#[derive(ValueEnum, Clone, Copy, Debug)]
enum Mode {
    /// `SeekrApp::index_full` (what library users call; also rebuilds the
    /// TF-IDF semantic encoder after every index).
    App,
    /// `seekr::indexer::index_directory` directly (walk + SQLite only).
    Indexer,
}

#[derive(Parser)]
#[command(about = "Benchmark seekr as a library")]
struct Cli {
    #[command(flatten)]
    common: CommonArgs,

    #[arg(long, value_enum, default_value = "app")]
    mode: Mode,

    /// Disable seekr's search result cache.
    #[arg(long)]
    no_cache: bool,
}

/// Rows are fetched with no effective limit (seekr defaults to 50).
const NO_LIMIT: i64 = i64::MAX;

struct Seekr {
    mode: Mode,
    cache: bool,
    dir: PathBuf,
    app: Option<SeekrApp>,
    watchers: Vec<(WatcherHandle, JoinHandle<()>)>,
}

impl Seekr {
    fn db_path(&self) -> PathBuf {
        self.dir.join("seekr.db")
    }

    fn meta_path(&self) -> PathBuf {
        self.dir.join("bench-meta.json")
    }

    fn app(&self) -> BResult<&SeekrApp> {
        self.app.as_ref().ok_or_else(|| "index not open".into())
    }

    fn open_app(&mut self) -> BResult<()> {
        let config = AppConfig {
            database_path: Some(self.db_path()),
            cache_enabled: self.cache,
            ..AppConfig::default()
        };
        self.app = Some(SeekrApp::new(config)?);
        Ok(())
    }

    /// seekr's defaults skip `*.exe`, `*.dll`, `*.bin` and more; FileDock
    /// categories may contain any file, so nothing is ignored.
    fn indexer_config(recursive: bool) -> IndexerConfig {
        IndexerConfig {
            ignore_dirs: Vec::new(),
            ignore_patterns: Vec::new(),
            follow_links: false,
            max_depth: if recursive { None } else { Some(1) },
            max_file_size: None,
        }
    }

    /// GLUE: seekr cannot query "directory + pattern + recursion". Its
    /// closest API is `search_by_path` (`LIKE '%dir%'`, a substring match);
    /// the rule is applied afterwards with the app's matcher.
    fn files_under(&self, dir: &Path) -> BResult<Vec<seekr::types::FileEntry>> {
        let rows = self
            .app()?
            .database()
            .search_by_path(&dir.to_string_lossy(), false, NO_LIMIT, 0)?;
        Ok(rows
            .into_iter()
            .filter(|e| !e.is_dir && e.path.starts_with(dir) && e.path != dir)
            .collect())
    }

    fn rule_entries(&self, rule: &Rule) -> BResult<Vec<Entry>> {
        let matcher = rule.matcher();
        Ok(self
            .files_under(&rule.dir)?
            .into_iter()
            .filter(|e| rule.accepts(&e.path, &matcher))
            .map(to_entry)
            .collect())
    }
}

fn to_entry(e: seekr::types::FileEntry) -> Entry {
    Entry {
        size: e.size,
        modified: e.modified.map(|d| d.timestamp_millis()).unwrap_or(0),
        path: e.path,
    }
}

/// GLUE: seekr's watcher only reports events; this writes them back with
/// the per-path upsert/remove calls its `Database` offers. seekr's own
/// indexer builds entries privately, so the entry is rebuilt here the same
/// way (`indexer.rs` `build_entry`).
fn apply_event(db: &Database, event: FileEvent) {
    let upsert_or_remove = |path: &Path| {
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.is_file() || m.is_dir() => {
                let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let entry = seekr::types::FileEntry {
                    id: None,
                    path: path.to_path_buf(),
                    extension: if m.is_dir() {
                        None
                    } else {
                        path.extension().map(|e| e.to_string_lossy().into_owned())
                    },
                    parent_dir: path.parent().map(Path::to_path_buf).unwrap_or_default(),
                    size: m.len(),
                    modified: m.modified().ok().map(DateTime::<Utc>::from),
                    accessed: m.accessed().ok().map(DateTime::<Utc>::from),
                    is_hidden: file_name.starts_with('.'),
                    is_dir: m.is_dir(),
                    hash: None,
                    file_name,
                };
                let _ = db.upsert_file(&entry);
            }
            _ => {
                // Renames often arrive as a plain event on the old path.
                let _ = db.remove_file(path);
            }
        }
    };
    match event {
        FileEvent::Created(p) | FileEvent::Modified(p) => upsert_or_remove(&p),
        FileEvent::Deleted(p) => {
            let _ = db.remove_file(&p);
        }
        FileEvent::Renamed { from, to } => {
            let _ = db.remove_file(&from);
            upsert_or_remove(&to);
        }
    }
}

impl Backend for Seekr {
    fn name(&self) -> &'static str {
        "seekr"
    }

    fn index(&mut self, rules: &[Rule]) -> BResult<Notes> {
        self.app = None;
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        std::fs::create_dir_all(&self.dir)?;
        let since = util::now_millis();
        self.open_app()?;

        let mut per_root = Vec::new();
        for (root, recursive) in covering_roots(rules) {
            let cfg = Self::indexer_config(recursive);
            let (result, d) = time(|| -> BResult<u64> {
                match self.mode {
                    Mode::App => Ok(self.app()?.index_full(&root, &cfg)?.total_files),
                    Mode::Indexer => Ok(seekr::indexer::index_directory(&self.app()?.database(), &root, &cfg)?),
                }
            });
            per_root.push(json!({ "root": root, "count": result?, "ms": ms(d) }));
        }
        std::fs::write(self.meta_path(), json!({ "indexedAt": since }).to_string())?;

        let mut n = Notes::new();
        n.insert("mode".into(), json!(format!("{:?}", self.mode)));
        n.insert("roots".into(), json!(per_root));
        n.insert("ignorePatterns".into(), json!("cleared (seekr skips *.exe, *.dll, *.bin… by default)"));
        Ok(n)
    }

    fn open(&mut self, _rules: &[Rule]) -> BResult<()> {
        if !self.db_path().exists() {
            return Err(format!("no index at {}; run s1 first", self.db_path().display()).into());
        }
        self.open_app()
    }

    /// seekr's only catch-up: `index_incremental` (re-walks everything and
    /// upserts files modified since the last index) plus `remove_stale`
    /// (checks at most 100,000 stored entries).
    fn resume(&mut self, rules: &[Rule]) -> BResult<Notes> {
        let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(self.meta_path())?)?;
        let since_ms = meta["indexedAt"].as_i64().ok_or("bad bench-meta.json")?;
        let since = DateTime::<Utc>::from_timestamp_millis(since_ms).ok_or("bad timestamp")?;
        let now = util::now_millis();

        let (incremental, inc_time) = time(|| -> BResult<()> {
            for (root, _) in covering_roots(rules) {
                self.app()?.index_incremental(&root, since)?;
            }
            Ok(())
        });
        incremental?;
        let (removed, stale_time) = time(|| -> BResult<u64> { Ok(self.app()?.remove_stale()?) });
        std::fs::write(self.meta_path(), json!({ "indexedAt": now }).to_string())?;

        let mut n = Notes::new();
        n.insert("indexIncrementalMs".into(), json!(ms(inc_time)));
        n.insert("removeStaleMs".into(), json!(ms(stale_time)));
        n.insert("staleRemoved".into(), json!(removed?));
        Ok(n)
    }

    fn list(&mut self, rule: &Rule, limit: usize) -> BResult<Listing> {
        Ok(first_page(self.rule_entries(rule)?, limit))
    }

    /// seekr's own search (name or path substring, cached), then restricted
    /// to the category.
    fn search(&mut self, rule: &Rule, text: &str) -> BResult<usize> {
        let query = SearchQuery {
            pattern: text.to_string(),
            // Not usize::MAX: seekr multiplies the limit by 3 as i64.
            limit: Some(1_000_000_000),
            include_hidden: true,
            include_dirs: false,
            ..SearchQuery::default()
        };
        let matcher = rule.matcher();
        let needle = text.to_lowercase();
        Ok(self
            .app()?
            .search(&query)?
            .into_iter()
            .filter(|r| rule.accepts(&r.entry.path, &matcher) && name_contains(&r.entry.path, &needle))
            .count())
    }

    fn snapshot(&mut self, rule: &Rule) -> BResult<Vec<Entry>> {
        self.rule_entries(rule)
    }

    fn probe(&mut self, dir: &Path) -> BResult<Vec<Entry>> {
        Ok(self.files_under(dir)?.into_iter().map(to_entry).collect())
    }

    fn start_watch(&mut self, rules: &[Rule]) -> BResult<Notes> {
        for (root, recursive) in covering_roots(rules) {
            let (rx, handle) = watch_directory(WatchConfig {
                path: root,
                recursive,
                debounce_ms: 100,
            })?;
            // A second connection: seekr's `Database` is not shareable.
            let db = Database::open(&self.db_path())?;
            let thread = std::thread::spawn(move || {
                while let Ok(event) = rx.recv() {
                    apply_event(&db, event);
                }
            });
            self.watchers.push((handle, thread));
        }
        let mut n = Notes::new();
        n.insert(
            "glue".into(),
            json!("seekr's watcher does not update its index; events are written back per path by bench glue (no batching, no subtree handling)"),
        );
        Ok(n)
    }

    fn stop_watch(&mut self) {
        for (handle, thread) in self.watchers.drain(..) {
            handle.stop();
            let _ = thread.join();
        }
    }

    fn state_bytes(&self) -> Option<u64> {
        Some(disk_usage(&self.dir))
    }
}

fn main() -> BResult<()> {
    let cli = Cli::parse();
    let mut backend = Seekr {
        mode: cli.mode,
        cache: !cli.no_cache,
        dir: state_dir("seekr", &cli.common.dataset),
        app: None,
        watchers: Vec::new(),
    };
    let mut options = Map::new();
    options.insert(
        "variant".into(),
        json!(format!(
            "{}{}",
            match cli.mode {
                Mode::App => "app",
                Mode::Indexer => "indexer",
            },
            if cli.no_cache { "+nocache" } else { "" }
        )),
    );
    options.insert("seekrRev".into(), json!("f11bc11f15f256163d41f188dac49ceaf4a445f7"));
    scenarios::run(&mut backend, &cli.common, options)
}
