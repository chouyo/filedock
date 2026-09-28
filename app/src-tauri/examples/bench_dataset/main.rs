//! Dataset tool for the search benchmarks.
//!
//! ```text
//! cargo run --release --example bench_dataset -- gen --size 100k
//! cargo run --release --example bench_dataset -- mutate --dataset 100k-seed42
//! cargo run --release --example bench_dataset -- revert --dataset 100k-seed42
//! cargo run --release --example bench_dataset -- clean --dataset 100k-seed42
//! cargo run --release --example bench_dataset -- list
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

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::{Parser, Subcommand};
use rayon::prelude::*;
use serde_json::json;

use bench_common::mutate::{mutate_offline, revert, Op};
use bench_common::paths::{
    bench_home, dataset_root, datasets_dir, disk_usage, manifest_path, mutation_journal_path,
};
use bench_common::rng::Rng;
use bench_common::BResult;

#[derive(Parser)]
#[command(about = "Create, mutate and remove benchmark datasets")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a dataset (name: <size>-seed<seed>).
    Gen {
        /// Number of files: 100k, 1m, 5m, or a plain number.
        #[arg(long)]
        size: String,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Replace an existing dataset of the same name.
        #[arg(long)]
        force: bool,
    },
    /// Change a share of the files while no backend runs (for S5).
    Mutate {
        #[arg(long)]
        dataset: String,
        /// Share of files to change.
        #[arg(long, default_value_t = 0.01)]
        ratio: f64,
        #[arg(long, default_value_t = 7)]
        seed: u64,
    },
    /// Undo the last `mutate`.
    Revert {
        #[arg(long)]
        dataset: String,
    },
    /// Delete a dataset and every backend's state for it.
    Clean {
        #[arg(long)]
        dataset: String,
    },
    /// List datasets.
    List,
}

fn main() -> BResult<()> {
    match Cli::parse().command {
        Command::Gen { size, seed, force } => gen(&size, seed, force),
        Command::Mutate { dataset, ratio, seed } => mutate(&dataset, ratio, seed),
        Command::Revert { dataset } => revert_cmd(&dataset),
        Command::Clean { dataset } => clean(&dataset),
        Command::List => list(),
    }
}

fn parse_size(spec: &str) -> BResult<(u64, String)> {
    let s = spec.trim().to_lowercase();
    let (num, mult) = if let Some(n) = s.strip_suffix('k') {
        (n, 1_000)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 1_000_000)
    } else {
        (s.as_str(), 1)
    };
    let n: u64 = num.parse().map_err(|_| format!("invalid size {spec}"))?;
    Ok((n * mult, s))
}

/// Shape of one top-level area of the dataset.
struct Profile {
    top: &'static str,
    /// Share of files, in percent.
    share: u64,
    dir_names: &'static [&'static str],
    exts: &'static [(&'static str, u32)],
    /// Share of the area's files placed directly in the top directory.
    top_level_share: f64,
    max_depth: usize,
}

const WORDS: &[&str] = &[
    "report", "invoice", "notes", "draft", "summary", "contract", "budget", "plan", "meeting",
    "photo", "backup", "export", "data", "config", "readme", "design", "spec", "review",
];

const PROFILES: &[Profile] = &[
    Profile {
        top: "photos",
        share: 25,
        dir_names: &["2019", "2020", "2021", "2022", "2023", "2024", "trip", "family", "party", "work", "raw", "edited"],
        exts: &[("jpg", 60), ("png", 20), ("heic", 15), ("mov", 5)],
        top_level_share: 0.02,
        max_depth: 4,
    },
    Profile {
        top: "docs",
        share: 15,
        dir_names: &["work", "personal", "finance", "school", "archive", "clients", "2022", "2023", "2024", "shared"],
        exts: &[("pdf", 35), ("docx", 25), ("xlsx", 15), ("txt", 15), ("md", 10)],
        top_level_share: 0.05,
        max_depth: 5,
    },
    Profile {
        top: "downloads",
        share: 5,
        dir_names: &["installers", "unzipped", "temp", "old"],
        exts: &[("zip", 25), ("dmg", 10), ("exe", 15), ("pdf", 20), ("png", 10), ("dll", 5), ("msi", 5), ("jpg", 10)],
        top_level_share: 0.6,
        max_depth: 3,
    },
    Profile {
        top: "projects",
        share: 45,
        dir_names: &["src", "tests", "docs", "node_modules", "target", ".git", "assets", "lib", "bin", "examples", "build", "objects"],
        exts: &[("rs", 25), ("js", 25), ("ts", 10), ("json", 10), ("md", 5), ("toml", 3), ("lock", 2), ("o", 5), ("", 15)],
        top_level_share: 0.0,
        max_depth: 8,
    },
    Profile {
        top: "misc",
        share: 10,
        dir_names: &["stuff", "old", "new", "tmp", "sorted", "unsorted", "backup"],
        exts: &[("txt", 20), ("log", 15), ("csv", 15), ("jpg", 10), ("pdf", 10), ("bin", 10), ("xml", 10), ("", 10)],
        top_level_share: 0.05,
        max_depth: 6,
    },
];

const AVG_FILES_PER_DIR: u64 = 40;

struct PlannedDir {
    path: PathBuf,
    files: Vec<(String, u32)>,
}

fn file_name(profile: &Profile, rng: &mut Rng, seq: u64, ext: &str) -> String {
    let stem = match profile.top {
        "photos" => {
            if rng.chance(0.5) {
                format!("IMG_{seq:06}")
            } else {
                format!("img_{seq:06}")
            }
        }
        "docs" => {
            let v = if rng.chance(0.15) { "_v2" } else { "" };
            format!("{}_{}_{seq}{v}", rng.pick(WORDS), rng.range(2018, 2025))
        }
        "projects" if ext.is_empty() => format!("{:016x}", rng.next_u64()),
        "projects" => format!("{}_{}_{seq}", rng.pick(WORDS), rng.pick(WORDS)),
        _ => format!("{}-{seq}", rng.pick(WORDS)),
    };
    if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{ext}")
    }
}

/// Builds the whole dataset plan deterministically from the seed.
fn plan(root: &Path, total: u64, seed: u64) -> Vec<PlannedDir> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    let mut seq = 0u64;
    for profile in PROFILES {
        let quota = total * profile.share / 100;
        let top = root.join(profile.top);
        let n_dirs = (quota / AVG_FILES_PER_DIR).max(1) as usize;

        // Grow a tree: each new directory hangs off a recent one, which gives
        // a realistic mix of depths.
        let mut dirs: Vec<(PathBuf, usize)> = vec![(top.clone(), 0)];
        let mut seen: HashSet<PathBuf> = HashSet::from([top.clone()]);
        while dirs.len() < n_dirs + 1 {
            let window = dirs.len().min(64);
            let mut parent_idx = dirs.len() - 1 - rng.below(window as u64) as usize;
            if dirs[parent_idx].1 >= profile.max_depth {
                parent_idx = rng.below(dirs.len() as u64) as usize;
                if dirs[parent_idx].1 >= profile.max_depth {
                    parent_idx = 0;
                }
            }
            let (parent, depth) = dirs[parent_idx].clone();
            let base = rng.pick(profile.dir_names);
            let mut path = parent.join(base);
            if !seen.insert(path.clone()) {
                path = parent.join(format!("{base}_{}", dirs.len()));
                seen.insert(path.clone());
            }
            dirs.push((path, depth + 1));
        }

        let mut planned: Vec<PlannedDir> = dirs
            .into_iter()
            .map(|(path, _)| PlannedDir { path, files: Vec::new() })
            .collect();
        for _ in 0..quota {
            let idx = if planned.len() == 1 || rng.chance(profile.top_level_share) {
                0
            } else {
                1 + rng.below((planned.len() - 1) as u64) as usize
            };
            let ext = *rng.weighted(profile.exts);
            let name = file_name(profile, &mut rng, seq, ext);
            // Most files are empty so large datasets stay small on disk; the
            // rest get a size the scenarios can see change.
            let size = if rng.chance(0.25) { rng.range(1, 1024) as u32 } else { 0 };
            planned[idx].files.push((name, size));
            seq += 1;
        }
        out.extend(planned);
    }
    out
}

fn gen(size: &str, seed: u64, force: bool) -> BResult<()> {
    let (total, spec) = parse_size(size)?;
    let name = format!("{spec}-seed{seed}");
    let root = dataset_root(&name);
    if root.exists() {
        if !force {
            eprintln!("dataset {name} already exists at {} (use --force to regenerate)", root.display());
            println!("{name}");
            return Ok(());
        }
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(&root)?;

    let started = Instant::now();
    let dirs = plan(&root, total, seed);
    let files: usize = dirs.iter().map(|d| d.files.len()).sum();
    eprintln!("planned {files} files in {} directories", dirs.len());

    dirs.par_iter().try_for_each(|d| -> std::io::Result<()> {
        std::fs::create_dir_all(&d.path)?;
        for (name, size) in &d.files {
            std::fs::write(d.path.join(name), vec![b'x'; *size as usize])?;
        }
        Ok(())
    })?;

    let manifest = json!({
        "name": name,
        "files": files,
        "dirs": dirs.len(),
        "seed": seed,
        "size": spec,
        "createdAt": util::now_millis(),
        "generateMs": started.elapsed().as_millis() as u64,
        "bytes": disk_usage(&root),
    });
    std::fs::write(manifest_path(&name), serde_json::to_string_pretty(&manifest)?)?;
    eprintln!("created {} in {:.1}s", root.display(), started.elapsed().as_secs_f64());
    println!("{name}");
    Ok(())
}

fn mutate(dataset: &str, ratio: f64, seed: u64) -> BResult<()> {
    let journal = mutation_journal_path(dataset);
    if journal.exists() {
        return Err(format!("{dataset} already has pending mutations; run `revert` first").into());
    }
    let root = dataset_root(dataset);
    let (ops, summary) = mutate_offline(&root, ratio, seed)?;
    std::fs::write(&journal, serde_json::to_string(&ops)?)?;
    eprintln!("mutated {dataset}: {summary:?}");
    Ok(())
}

fn revert_cmd(dataset: &str) -> BResult<()> {
    let journal = mutation_journal_path(dataset);
    let text = match std::fs::read_to_string(&journal) {
        Ok(t) => t,
        Err(_) => {
            eprintln!("{dataset} has no pending mutations");
            return Ok(());
        }
    };
    let ops: Vec<Op> = serde_json::from_str(&text)?;
    revert(&ops)?;
    std::fs::remove_file(&journal)?;
    eprintln!("reverted {} changes in {dataset}", ops.len());
    Ok(())
}

fn clean(dataset: &str) -> BResult<()> {
    let root = dataset_root(dataset);
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    for p in [manifest_path(dataset), mutation_journal_path(dataset)] {
        if p.exists() {
            std::fs::remove_file(p)?;
        }
    }
    if let Ok(backends) = std::fs::read_dir(bench_home().join("state")) {
        for b in backends.flatten() {
            let state = b.path().join(dataset);
            if state.exists() {
                std::fs::remove_dir_all(state)?;
            }
        }
    }
    eprintln!("removed {dataset}");
    Ok(())
}

fn list() -> BResult<()> {
    let Ok(entries) = std::fs::read_dir(datasets_dir()) else {
        eprintln!("no datasets in {}", datasets_dir().display());
        return Ok(());
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(ds) = name.strip_suffix(".manifest.json") {
            let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(e.path())?)?;
            let mutated = mutation_journal_path(ds).exists();
            println!(
                "{ds}\tfiles={}\tdirs={}{}",
                manifest["files"],
                manifest["dirs"],
                if mutated { "\t(mutated)" } else { "" }
            );
        }
    }
    Ok(())
}
