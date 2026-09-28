//! Shared code for the search benchmark examples.
//!
//! This directory has no `main.rs`, so Cargo does not treat it as an example.
//! Each `bench_*` example mounts it with `#[path]` at its crate root, together
//! with the app's own `util`, `matcher`, `file_entry`, `config` and `scan`
//! modules (their `crate::` paths expect them there). Rule matching and the
//! ground-truth scan are therefore the exact code the app ships:
//!
//! ```ignore
//! #[path = "../../src/util.rs"] mod util;
//! #[path = "../../src/matcher.rs"] mod matcher;
//! #[path = "../../src/file_entry.rs"] mod file_entry;
//! #[path = "../../src/config.rs"] mod config;
//! #[path = "../../src/scan.rs"] mod scan;
//! #[path = "../bench_common/mod.rs"] mod bench_common;
//! ```

#![allow(dead_code)]

pub mod backend;
pub mod cli;
pub mod metrics;
pub mod mutate;
pub mod paths;
pub mod report;
pub mod rng;
pub mod rules;
pub mod scenarios;
pub mod truth;

pub type BResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
