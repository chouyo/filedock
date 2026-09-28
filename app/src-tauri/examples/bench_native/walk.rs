//! Directory traversal for the initial index: `ignore`'s parallel walker, or
//! `walkdir` on one thread for comparison. Results stream to the writer in
//! batches.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;

use clap::ValueEnum;

use super::store::{dir_mtime, FileRow};

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Walker {
    /// `ignore::WalkBuilder::build_parallel` (reads directories on all threads).
    Parallel,
    /// `walkdir` on one thread (what the app uses today).
    Walkdir,
}

pub enum Item {
    Dir { path: PathBuf, mtime: i64 },
    File { dir: PathBuf, row: FileRow },
}

const BATCH: usize = 4096;

/// Per-thread buffer that forwards full batches and flushes on drop.
struct Batch {
    tx: SyncSender<Vec<Item>>,
    items: Vec<Item>,
}

impl Batch {
    fn new(tx: SyncSender<Vec<Item>>) -> Self {
        Batch {
            tx,
            items: Vec::with_capacity(BATCH),
        }
    }

    fn push(&mut self, item: Item) {
        self.items.push(item);
        if self.items.len() >= BATCH {
            let full = std::mem::replace(&mut self.items, Vec::with_capacity(BATCH));
            let _ = self.tx.send(full);
        }
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        if !self.items.is_empty() {
            let _ = self.tx.send(std::mem::take(&mut self.items));
        }
    }
}

fn to_item(path: &Path, is_dir: bool, is_file: bool, meta: Option<std::fs::Metadata>) -> Option<Item> {
    let meta = meta?;
    if is_dir {
        Some(Item::Dir {
            path: path.to_path_buf(),
            mtime: dir_mtime(&meta),
        })
    } else if is_file {
        Some(Item::File {
            dir: path.parent()?.to_path_buf(),
            row: FileRow::from_metadata(path.file_name()?.to_string_lossy().into_owned(), &meta),
        })
    } else {
        // Symlinks are skipped, as in the app's scanner.
        None
    }
}

/// Walks `root` and sends every directory and file; returns the number of
/// entries that could not be read.
pub fn walk(root: &Path, recursive: bool, walker: Walker, threads: usize, tx: &SyncSender<Vec<Item>>) -> u64 {
    let errors = AtomicU64::new(0);
    match walker {
        Walker::Parallel => {
            let mut builder = ignore::WalkBuilder::new(root);
            builder
                .standard_filters(false)
                .follow_links(false)
                .threads(threads);
            if !recursive {
                builder.max_depth(Some(1));
            }
            builder.build_parallel().run(|| {
                let mut batch = Batch::new(tx.clone());
                let errors = &errors;
                Box::new(move |result| {
                    match result {
                        Ok(entry) => {
                            let ft = entry.file_type();
                            let is_dir = ft.map(|t| t.is_dir()).unwrap_or(false);
                            let is_file = ft.map(|t| t.is_file()).unwrap_or(false);
                            if let Some(item) = to_item(entry.path(), is_dir, is_file, entry.metadata().ok()) {
                                batch.push(item);
                            }
                        }
                        Err(_) => {
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    ignore::WalkState::Continue
                })
            });
        }
        Walker::Walkdir => {
            let mut batch = Batch::new(tx.clone());
            let wd = walkdir::WalkDir::new(root).max_depth(if recursive { usize::MAX } else { 1 });
            for result in wd {
                match result {
                    Ok(entry) => {
                        let ft = entry.file_type();
                        if let Some(item) = to_item(entry.path(), ft.is_dir(), ft.is_file(), entry.metadata().ok()) {
                            batch.push(item);
                        }
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    }
    errors.into_inner()
}
