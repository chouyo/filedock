//! Command-line options shared by every backend example.

use std::path::PathBuf;

use clap::Args;

use super::rules::DEFAULT_RULES;

#[derive(Args, Debug, Clone)]
pub struct CommonArgs {
    /// Dataset name under ~/FileDockBench/datasets (e.g. 100k-seed42).
    #[arg(long)]
    pub dataset: String,

    /// Comma-separated scenarios: s1..s6, or `all` (= s1,s2,s3,s4,s6).
    /// s5 needs an index from an earlier process: run it on its own after
    /// `bench_dataset mutate`.
    #[arg(long, default_value = "all")]
    pub scenario: String,

    /// Category rules file.
    #[arg(long, default_value = DEFAULT_RULES)]
    pub rules: PathBuf,

    /// Page size of the file table's first screen (S2).
    #[arg(long, default_value_t = 100)]
    pub page: usize,

    /// Repetitions per list/search measurement (the median is reported).
    #[arg(long, default_value_t = 5)]
    pub repeat: usize,

    /// Seconds to wait for the index to reflect live or offline changes.
    #[arg(long, default_value_t = 60)]
    pub timeout: u64,
}

impl CommonArgs {
    pub fn scenarios(&self) -> Vec<String> {
        let list = if self.scenario.trim() == "all" {
            "s1,s2,s3,s4,s6"
        } else {
            self.scenario.as_str()
        };
        list.split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    }
}
