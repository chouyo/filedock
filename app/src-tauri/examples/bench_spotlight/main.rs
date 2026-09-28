//! Approach 3 (macOS only): query the system Spotlight index through
//! CoreServices `MDQuery`. The OS builds and maintains the index, so there is
//! nothing to index or catch up on our side; what matters is how long
//! Spotlight takes to reflect changes and what it does not cover.
//!
//! Generate the dataset before running this backend and run it first: S1
//! measures how long Spotlight needs to index the new files, and that
//! background indexing would otherwise skew the other backends' timings.
//!
//! ```text
//! cargo run --release --example bench_spotlight -- --dataset 100k-seed42
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

#[cfg(target_os = "macos")]
mod ffi;
#[cfg(target_os = "macos")]
mod query;

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("bench_spotlight uses the macOS Spotlight index and only runs on macOS");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() -> bench_common::BResult<()> {
    spotlight::main()
}

#[cfg(target_os = "macos")]
mod spotlight {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use clap::Parser;
    use serde_json::{json, Map};

    use crate::bench_common::backend::{first_page, name_contains, Backend, Entry, Listing, Notes};
    use crate::bench_common::cli::CommonArgs;
    use crate::bench_common::paths::dataset_root;
    use crate::bench_common::rules::{self, Rule};
    use crate::bench_common::truth::{in_hidden_dir, scan_rule};
    use crate::bench_common::{scenarios, BResult};
    use crate::ffi::{self, Attrs};
    use crate::query;

    #[derive(Parser)]
    #[command(about = "Benchmark queries against the macOS Spotlight index")]
    struct Cli {
        #[command(flatten)]
        common: CommonArgs,

        /// Longest S1 wait for Spotlight to index the dataset, in seconds.
        #[arg(long, default_value_t = 1800)]
        index_timeout: u64,

        /// Seconds between S1 progress checks.
        #[arg(long, default_value_t = 2)]
        poll_secs: u64,

        /// S1 stops early once counts stay unchanged for this many checks.
        #[arg(long, default_value_t = 15)]
        stable_polls: u32,
    }

    struct Spotlight {
        attrs: Attrs,
        root: std::path::PathBuf,
        /// Per-rule file counts from the app's scanner outside hidden
        /// directories, taken before S1.
        truth_counts: Vec<usize>,
        index_timeout: Duration,
        poll: Duration,
        stable_polls: u32,
    }

    impl Spotlight {
        fn hits(&self, query: &str, scope: &Path) -> BResult<Vec<Entry>> {
            Ok(ffi::run(query, &[scope], true, &self.attrs)?
                .1
                .into_iter()
                .map(|h| Entry {
                    path: h.path.into(),
                    size: h.size,
                    modified: h.modified_ms,
                })
                .collect())
        }

        fn rule_entries(&self, rule: &Rule) -> BResult<Vec<Entry>> {
            let matcher = rule.matcher();
            Ok(self
                .hits(&query::for_rule(rule), &rule.dir)?
                .into_iter()
                .filter(|e| rule.accepts(&e.path, &matcher))
                .collect())
        }

        /// `mdutil -s` for the dataset's volume (it only accepts mount
        /// points), so the report shows whether indexing is enabled at all.
        fn mdutil_status(&self) -> String {
            let run = |cmd: &str, args: &[&std::ffi::OsStr]| {
                std::process::Command::new(cmd)
                    .args(args)
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_else(|e| format!("{cmd} failed: {e}"))
            };
            let df = run("df", &["-P".as_ref(), self.root.as_os_str()]);
            let mount = df
                .lines()
                .last()
                .and_then(|l| l.split_whitespace().last())
                .unwrap_or("/")
                .to_string();
            run("mdutil", &["-s".as_ref(), mount.as_ref()]).replace('\n', " ").replace('\t', "")
        }
    }

    impl Backend for Spotlight {
        fn name(&self) -> &'static str {
            "spotlight"
        }

        /// Waits until Spotlight's counts for every rule reach the app
        /// scanner's counts, stop changing, or the timeout passes.
        fn index(&mut self, rules: &[Rule]) -> BResult<Notes> {
            let started = Instant::now();
            let mut last: Vec<usize> = Vec::new();
            let mut unchanged = 0;
            let status = loop {
                let counts: Vec<usize> = rules
                    .iter()
                    .map(|r| ffi::run(&query::for_rule(r), &[&r.dir], false, &self.attrs).map(|(c, _)| c))
                    .collect::<BResult<_>>()?;
                // Glob-only rules can be compared directly; regex rules are
                // post-filtered, so only their stability counts.
                let done = counts
                    .iter()
                    .zip(rules)
                    .zip(&self.truth_counts)
                    .all(|((c, r), t)| r.match_type == "regex" || c >= t);
                let same = counts == last;
                last = counts;
                if done {
                    break "caught-up";
                }
                if same && last.iter().any(|c| *c > 0) {
                    unchanged += 1;
                    if unchanged >= self.stable_polls {
                        break "stable-incomplete";
                    }
                } else {
                    unchanged = 0;
                }
                if started.elapsed() > self.index_timeout {
                    break "timeout";
                }
                eprintln!("  spotlight counts {last:?} / visible truth {:?}", self.truth_counts);
                std::thread::sleep(self.poll);
            };
            let mut n = Notes::new();
            n.insert("status".into(), json!(status));
            n.insert("visibleTruthCounts".into(), json!(self.truth_counts));
            n.insert("finalCounts".into(), json!(last));
            n.insert("mdutil".into(), json!(self.mdutil_status()));
            n.insert(
                "note".into(),
                json!("elapsed = time until Spotlight's index reflected the dataset; nothing is built by us"),
            );
            Ok(n)
        }

        fn open(&mut self, _rules: &[Rule]) -> BResult<()> {
            Ok(())
        }

        fn resume(&mut self, _rules: &[Rule]) -> BResult<Notes> {
            let mut n = Notes::new();
            n.insert("note".into(), json!("maintained by the OS; S5 polls until results are consistent"));
            n.insert("mdutil".into(), json!(self.mdutil_status()));
            Ok(n)
        }

        fn list(&mut self, rule: &Rule, limit: usize) -> BResult<Listing> {
            Ok(first_page(self.rule_entries(rule)?, limit))
        }

        fn search(&mut self, rule: &Rule, text: &str) -> BResult<usize> {
            let matcher = rule.matcher();
            let needle = text.to_lowercase();
            Ok(self
                .hits(&query::name_contains(text), &rule.dir)?
                .into_iter()
                .filter(|e| rule.accepts(&e.path, &matcher) && name_contains(&e.path, &needle))
                .count())
        }

        fn snapshot(&mut self, rule: &Rule) -> BResult<Vec<Entry>> {
            self.rule_entries(rule)
        }

        fn probe(&mut self, dir: &Path) -> BResult<Vec<Entry>> {
            if !dir.exists() {
                // A scope that no longer exists: ask about its parent instead,
                // so deleted files still indexed under `dir` show up.
                let parent = dir.parent().unwrap_or(dir);
                return Ok(self
                    .hits(&query::all_files(), parent)?
                    .into_iter()
                    .filter(|e| e.path.starts_with(dir))
                    .collect());
            }
            self.hits(&query::all_files(), dir)
        }

        fn start_watch(&mut self, _rules: &[Rule]) -> BResult<Notes> {
            let mut n = Notes::new();
            n.insert(
                "note".into(),
                json!("no watcher needed: S4 measures how long Spotlight's own index takes to reflect changes"),
            );
            Ok(n)
        }

        fn stop_watch(&mut self) {}

        fn state_bytes(&self) -> Option<u64> {
            None
        }

        fn settles_in_background(&self) -> bool {
            true
        }
    }

    pub fn main() -> BResult<()> {
        let cli = Cli::parse();
        let root = dataset_root(&cli.common.dataset);
        let mut truth_counts = Vec::new();
        if cli.common.scenarios().iter().any(|s| s == "s1") && root.is_dir() {
            eprintln!("counting files with the app's scanner (not timed) …");
            let rules = rules::load(&cli.common.rules, &root)?;
            // Spotlight never indexes hidden directories (S6 reports that
            // gap); S1 waits for the files it can cover.
            truth_counts = rules
                .rules
                .iter()
                .map(|r| scan_rule(r).iter().filter(|e| !in_hidden_dir(&e.path)).count())
                .collect();
        }
        let mut backend = Spotlight {
            attrs: Attrs::new(),
            root,
            truth_counts,
            index_timeout: Duration::from_secs(cli.index_timeout),
            poll: Duration::from_secs(cli.poll_secs),
            stable_polls: cli.stable_polls,
        };
        let mut options = Map::new();
        options.insert("variant".into(), json!("mdquery"));
        scenarios::run(&mut backend, &cli.common, options)
    }
}
