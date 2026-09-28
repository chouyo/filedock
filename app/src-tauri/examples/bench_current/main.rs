//! Baseline: the app's current behaviour. Every list, search and refresh is a
//! full walk with `scan.rs`; there is no index to build or keep current.
//!
//! ```text
//! cargo run --release --example bench_current -- --dataset 100k-seed42
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

use clap::Parser;
use serde_json::{json, Map};

use bench_common::backend::{first_page, name_contains, Backend, Entry, Listing, Notes};
use bench_common::cli::CommonArgs;
use bench_common::paths::dataset_root;
use bench_common::rules::Rule;
use bench_common::truth::{scan_dir, scan_rule};
use bench_common::{scenarios, BResult};

#[derive(Parser)]
#[command(about = "Benchmark the app's current full-walk scanning")]
struct Cli {
    #[command(flatten)]
    common: CommonArgs,
}

struct Current {
    root: PathBuf,
}

fn note(text: &str) -> Notes {
    let mut n = Map::new();
    n.insert("note".into(), json!(text));
    n
}

impl Backend for Current {
    fn name(&self) -> &'static str {
        "current"
    }

    fn index(&mut self, _rules: &[Rule]) -> BResult<Notes> {
        Ok(note("no index: every list is a full scan"))
    }

    fn resume(&mut self, _rules: &[Rule]) -> BResult<Notes> {
        Ok(note("no index: nothing to catch up"))
    }

    fn open(&mut self, _rules: &[Rule]) -> BResult<()> {
        Ok(())
    }

    fn list(&mut self, rule: &Rule, limit: usize) -> BResult<Listing> {
        Ok(first_page(scan_rule(rule), limit))
    }

    fn search(&mut self, rule: &Rule, text: &str) -> BResult<usize> {
        // The app filters in the front end after a scan; a scan is needed
        // whenever the list is not already loaded.
        let needle = text.to_lowercase();
        Ok(scan_rule(rule).iter().filter(|e| name_contains(&e.path, &needle)).count())
    }

    fn snapshot(&mut self, rule: &Rule) -> BResult<Vec<Entry>> {
        Ok(scan_rule(rule))
    }

    fn probe(&mut self, dir: &Path) -> BResult<Vec<Entry>> {
        // The app rescans the whole active category on every change event, so
        // the cost of catching up is one scan of that category. Scan the
        // dataset root (the `everything` category S4 changes) and keep `dir`.
        Ok(scan_dir(&self.root).into_iter().filter(|e| e.path.starts_with(dir)).collect())
    }

    fn start_watch(&mut self, _rules: &[Rule]) -> BResult<Notes> {
        Ok(note("the app rescans the whole category on each change; S4 catch-up = one full scan of the dataset root"))
    }

    fn stop_watch(&mut self) {}

    fn state_bytes(&self) -> Option<u64> {
        None
    }
}

fn main() -> BResult<()> {
    let cli = Cli::parse();
    let mut backend = Current {
        root: dataset_root(&cli.common.dataset),
    };
    scenarios::run(&mut backend, &cli.common, Map::new())
}
