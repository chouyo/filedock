//! JSON report written by every backend run, read back by `bench_compare`.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use super::metrics::logical_cpus;
use super::paths::results_dir;
use super::BResult;

pub struct Report {
    pub backend: String,
    pub dataset: String,
    pub started_at: i64,
    pub options: Map<String, Value>,
    pub dataset_info: Value,
    pub scenarios: Map<String, Value>,
}

impl Report {
    pub fn new(backend: &str, dataset: &str, dataset_info: Value) -> Self {
        Report {
            backend: backend.to_string(),
            dataset: dataset.to_string(),
            started_at: crate::util::now_millis(),
            options: Map::new(),
            dataset_info,
            scenarios: Map::new(),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "backend": self.backend,
            "dataset": self.dataset,
            "startedAt": self.started_at,
            "machine": {
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "logicalCpus": logical_cpus(),
            },
            "options": self.options,
            "datasetInfo": self.dataset_info,
            "scenarios": self.scenarios,
        })
    }

    /// Writes `<results>/<backend>-<dataset>-<timestamp>.json`.
    pub fn save(&self) -> BResult<PathBuf> {
        let dir = results_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}-{}-{}.json", self.backend, self.dataset, self.started_at));
        std::fs::write(&path, serde_json::to_string_pretty(&self.to_json())?)?;
        Ok(path)
    }
}
