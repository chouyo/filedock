//! File system changes the scenarios apply.
//!
//! * Offline mutation (S5): changes a share of the dataset while no benchmark
//!   process runs, and records an undo journal so the next backend starts
//!   from the same dataset.
//! * Live phases (S4): changes inside a scratch directory while a backend is
//!   watching, removed again at the end.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::rng::Rng;
use super::truth::scan_dir;
use super::BResult;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Op {
    Create { path: PathBuf },
    CreateDir { path: PathBuf },
    Modify { path: PathBuf, original_size: u64 },
    Rename { from: PathBuf, to: PathBuf },
    Delete { path: PathBuf, size: u64 },
}

#[derive(Default, Debug, Serialize)]
pub struct Summary {
    pub created: usize,
    pub created_dirs: usize,
    pub modified: usize,
    pub renamed: usize,
    pub deleted: usize,
}

fn write_bytes(path: &Path, len: usize) -> std::io::Result<()> {
    fs::write(path, vec![b'x'; len])
}

fn append(path: &Path, len: usize) -> std::io::Result<()> {
    OpenOptions::new().append(true).open(path)?.write_all(&vec![b'y'; len])
}

/// Name with the same extension, so the rules that matched the original also
/// match the new file.
fn sibling_name(prefix: &str, k: usize, original: &Path) -> String {
    match original.extension() {
        Some(ext) => format!("{prefix}_{k}.{}", ext.to_string_lossy()),
        None => format!("{prefix}_{k}"),
    }
}

/// Changes `ratio` of the files under `root` and returns the undo journal.
pub fn mutate_offline(root: &Path, ratio: f64, seed: u64) -> BResult<(Vec<Op>, Summary)> {
    let mut files = scan_dir(root);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    if files.is_empty() {
        return Err("dataset is empty".into());
    }
    let n = ((files.len() as f64 * ratio).round() as usize).clamp(1, files.len());

    // Partial Fisher-Yates: the first `n` indices become a random sample.
    let mut rng = Rng::new(seed);
    let mut idx: Vec<usize> = (0..files.len()).collect();
    for i in 0..n {
        let j = i + rng.below((idx.len() - i) as u64) as usize;
        idx.swap(i, j);
    }

    let mut ops = Vec::with_capacity(n);
    let mut sum = Summary::default();
    for (k, &i) in idx[..n].iter().enumerate() {
        let file = &files[i];
        let dir = file.path.parent().unwrap().to_path_buf();
        match rng.weighted(&[(0u8, 25), (1, 30), (2, 25), (3, 20)]) {
            0 => {
                let path = dir.join(sibling_name("offline_new", k, &file.path));
                write_bytes(&path, 32)?;
                ops.push(Op::Create { path });
                sum.created += 1;
            }
            1 => {
                append(&file.path, 64)?;
                ops.push(Op::Modify {
                    path: file.path.clone(),
                    original_size: file.size,
                });
                sum.modified += 1;
            }
            2 => {
                let to = dir.join(sibling_name("offline_renamed", k, &file.path));
                fs::rename(&file.path, &to)?;
                ops.push(Op::Rename {
                    from: file.path.clone(),
                    to,
                });
                sum.renamed += 1;
            }
            _ => {
                fs::remove_file(&file.path)?;
                ops.push(Op::Delete {
                    path: file.path.clone(),
                    size: file.size,
                });
                sum.deleted += 1;
            }
        }
        // One new directory (with files) per 200 changes: exercises detecting
        // subtrees that appeared while nothing was running.
        if k % 200 == 0 {
            let new_dir = dir.join(format!("offline_dir_{k}"));
            fs::create_dir(&new_dir)?;
            for j in 0..5 {
                write_bytes(&new_dir.join(sibling_name("offline_file", j, &file.path)), 16)?;
            }
            ops.push(Op::CreateDir { path: new_dir });
            sum.created_dirs += 1;
        }
    }
    Ok((ops, sum))
}

/// Undoes a journal from [`mutate_offline`] (sizes and names are restored;
/// timestamps are not, which no scenario depends on across backends).
pub fn revert(ops: &[Op]) -> BResult<()> {
    for op in ops.iter().rev() {
        match op {
            Op::Create { path } => fs::remove_file(path)?,
            Op::CreateDir { path } => fs::remove_dir_all(path)?,
            Op::Modify { path, original_size } => {
                OpenOptions::new().write(true).open(path)?.set_len(*original_size)?
            }
            Op::Rename { from, to } => fs::rename(to, from)?,
            Op::Delete { path, size } => write_bytes(path, *size as usize)?,
        }
    }
    Ok(())
}

pub const LIVE_DIRS: usize = 10;
pub const LIVE_FILES_PER_DIR: usize = 50;

/// S4 phase A: new subdirectories with files.
pub fn live_create(live: &Path) -> BResult<Vec<PathBuf>> {
    let mut created = Vec::new();
    for d in 0..LIVE_DIRS {
        let dir = live.join(format!("live_dir_{d}"));
        fs::create_dir_all(&dir)?;
        for i in 0..LIVE_FILES_PER_DIR {
            let path = dir.join(format!("live_{d}_{i}.txt"));
            write_bytes(&path, 16)?;
            created.push(path);
        }
    }
    Ok(created)
}

/// S4 phase B: modify, rename and delete some of the files phase A created.
pub fn live_change(files: &[PathBuf], seed: u64) -> BResult<Summary> {
    let mut rng = Rng::new(seed);
    let mut sum = Summary::default();
    for (k, path) in files.iter().enumerate() {
        match rng.weighted(&[(0u8, 30), (1, 30), (2, 20), (3, 20)]) {
            0 => {
                append(path, 64)?;
                sum.modified += 1;
            }
            1 => {
                fs::rename(path, path.with_file_name(format!("live_renamed_{k}.txt")))?;
                sum.renamed += 1;
            }
            2 => {
                fs::remove_file(path)?;
                sum.deleted += 1;
            }
            _ => {}
        }
    }
    Ok(sum)
}
