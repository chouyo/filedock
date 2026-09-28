//! Approach 2: own persistent index — parallel walk (`ignore`) + `notify`
//! + SQLite. This is the architecture proposed in
//! `docs/文件搜索性能优化方案.md` §4–5.
//!
//! ```text
//! cargo run --release --example bench_native -- --dataset 100k-seed42
//! cargo run --release --example bench_native -- --dataset 100k-seed42 --walker walkdir --fts
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

mod reconcile;
mod store;
mod sync;
mod walk;
mod watch;
mod writer;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Parser;
use rusqlite::Connection;
use serde_json::{json, Map};

use bench_common::backend::{Backend, Entry, Listing, Notes};
use bench_common::cli::CommonArgs;
use bench_common::metrics::logical_cpus;
use bench_common::paths::{disk_usage, state_dir};
use bench_common::rules::{covering_roots, Rule};
use bench_common::{scenarios, BResult};
use reconcile::ResumeMode;
use sync::Roots;
use walk::Walker;

#[derive(Parser)]
#[command(about = "Benchmark the walk + notify + SQLite index")]
struct Cli {
    #[command(flatten)]
    common: CommonArgs,

    /// Directory walker for the initial index.
    #[arg(long, value_enum, default_value = "parallel")]
    walker: Walker,

    /// Walker threads (default: logical CPUs).
    #[arg(long)]
    threads: Option<usize>,

    /// Build a trigram FTS5 index for name search.
    #[arg(long)]
    fts: bool,

    /// How S5 catches up: `dirs` (level 2) or `full` (level 3).
    #[arg(long, value_enum, default_value = "dirs")]
    resume_mode: ResumeMode,

    /// Quiet period before applying a batch of watcher events.
    #[arg(long, default_value_t = 100)]
    debounce_ms: u64,
}

struct Native {
    walker: Walker,
    threads: usize,
    fts: bool,
    resume_mode: ResumeMode,
    debounce: Duration,
    dir: PathBuf,
    conn: Option<Connection>,
    roots: Roots,
    watch: Option<watch::Watch>,
}

impl Native {
    fn db_path(&self) -> PathBuf {
        self.dir.join("index.db")
    }

    fn conn(&self) -> BResult<&Connection> {
        self.conn.as_ref().ok_or_else(|| "index not open".into())
    }
}

impl Backend for Native {
    fn name(&self) -> &'static str {
        "native"
    }

    fn index(&mut self, rules: &[Rule]) -> BResult<Notes> {
        self.conn = None;
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        std::fs::create_dir_all(&self.dir)?;
        let conn = store::open(&self.db_path())?;
        store::create_tables(&conn)?;
        self.roots = Roots(covering_roots(rules));

        let started = Instant::now();
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<walk::Item>>(256);
        let (write_result, walk_errors, walk_ms) = std::thread::scope(|s| {
            let roots = &self.roots;
            let writer = s.spawn(move || writer::run(conn, roots, rx));
            let mut errors = 0;
            for (root, recursive) in &self.roots.0 {
                errors += walk::walk(root, *recursive, self.walker, self.threads, &tx);
            }
            let walk_ms = started.elapsed().as_millis() as u64;
            drop(tx);
            (writer.join().expect("writer panicked"), errors, walk_ms)
        });
        let (conn, written) = write_result?;
        let write_ms = started.elapsed().as_millis() as u64;

        let t = Instant::now();
        store::create_indexes(&conn)?;
        let index_ms = t.elapsed().as_millis() as u64;
        let t = Instant::now();
        if self.fts {
            store::create_fts(&conn)?;
        }
        let fts_ms = t.elapsed().as_millis() as u64;

        store::set_meta(&conn, "schemaVersion", &store::SCHEMA_VERSION.to_string())?;
        store::set_meta(&conn, "roots", &store::roots_json(&self.roots.0))?;
        store::set_meta(&conn, "fts", if self.fts { "1" } else { "0" })?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        self.conn = Some(conn);

        let mut n = Notes::new();
        n.insert("walker".into(), json!(format!("{:?}", self.walker)));
        n.insert("threads".into(), json!(self.threads));
        n.insert("roots".into(), json!(self.roots.0.len()));
        n.insert("dirs".into(), json!(written.dirs));
        n.insert("files".into(), json!(written.files));
        n.insert("walkErrors".into(), json!(walk_errors));
        n.insert("walkDoneMs".into(), json!(walk_ms));
        n.insert("writeDoneMs".into(), json!(write_ms));
        n.insert("createIndexesMs".into(), json!(index_ms));
        n.insert("ftsMs".into(), json!(fts_ms));
        Ok(n)
    }

    fn open(&mut self, rules: &[Rule]) -> BResult<()> {
        let db = self.db_path();
        if !db.exists() {
            return Err(format!("no index at {}; run s1 first", db.display()).into());
        }
        let conn = store::open(&db)?;
        let expected = store::roots_json(&covering_roots(rules));
        if store::get_meta(&conn, "roots")?.as_deref() != Some(expected.as_str()) {
            return Err("rules changed since the index was built; run s1 again".into());
        }
        self.fts = store::get_meta(&conn, "fts")?.as_deref() == Some("1");
        self.roots = Roots(covering_roots(rules));
        self.conn = Some(conn);
        Ok(())
    }

    fn resume(&mut self, _rules: &[Rule]) -> BResult<Notes> {
        reconcile::reconcile(self.conn()?, &self.roots, self.resume_mode)
    }

    fn list(&mut self, rule: &Rule, limit: usize) -> BResult<Listing> {
        store::list(self.conn()?, rule, limit)
    }

    fn search(&mut self, rule: &Rule, text: &str) -> BResult<usize> {
        store::search(self.conn()?, rule, text, self.fts)
    }

    fn snapshot(&mut self, rule: &Rule) -> BResult<Vec<Entry>> {
        store::rule_entries(self.conn()?, rule, None)
    }

    fn probe(&mut self, dir: &Path) -> BResult<Vec<Entry>> {
        store::probe(self.conn()?, dir)
    }

    fn start_watch(&mut self, _rules: &[Rule]) -> BResult<Notes> {
        self.watch = Some(watch::Watch::start(&self.db_path(), self.roots.clone(), self.debounce)?);
        let mut n = Notes::new();
        n.insert("debounceMs".into(), json!(self.debounce.as_millis() as u64));
        n.insert("roots".into(), json!(self.roots.0.len()));
        Ok(n)
    }

    fn stop_watch(&mut self) {
        if let Some(mut w) = self.watch.take() {
            w.stop();
        }
    }

    fn state_bytes(&self) -> Option<u64> {
        Some(disk_usage(&self.dir))
    }
}

fn main() -> BResult<()> {
    let cli = Cli::parse();
    let threads = cli.threads.unwrap_or_else(logical_cpus);
    let mut backend = Native {
        walker: cli.walker,
        threads,
        fts: cli.fts,
        resume_mode: cli.resume_mode,
        debounce: Duration::from_millis(cli.debounce_ms),
        dir: state_dir("native", &cli.common.dataset),
        conn: None,
        roots: Roots(Vec::new()),
        watch: None,
    };
    let mut options = Map::new();
    options.insert(
        "variant".into(),
        json!(format!(
            "{}{}{}",
            match cli.walker {
                Walker::Parallel => "parallel",
                Walker::Walkdir => "walkdir",
            },
            if cli.fts { "+fts" } else { "" },
            match cli.resume_mode {
                ResumeMode::Dirs => "",
                ResumeMode::Full => "+resume-full",
            }
        )),
    );
    options.insert("threads".into(), json!(threads));
    options.insert("resumeMode".into(), json!(format!("{:?}", cli.resume_mode)));
    options.insert("debounceMs".into(), json!(cli.debounce_ms));
    scenarios::run(&mut backend, &cli.common, options)
}
