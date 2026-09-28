//! Summarises the latest benchmark reports of a dataset as a Markdown table,
//! one column per backend variant.
//!
//! ```text
//! cargo run --release --example bench_compare -- --dataset 100k-seed42
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

use std::collections::BTreeMap;
use std::fmt::Write as _;

use clap::Parser;
use serde_json::Value;

use bench_common::paths::results_dir;
use bench_common::BResult;

#[derive(Parser)]
#[command(about = "Compare benchmark reports")]
struct Cli {
    #[arg(long)]
    dataset: String,
}

/// Column order: baseline first, OS index last.
const ORDER: &[&str] = &["current", "seekr", "native", "spotlight"];

/// Latest value of every scenario for one backend variant. S6 is kept twice:
/// from the run that built the index, and from the run after S5.
#[derive(Default)]
struct Column {
    backend: String,
    scenarios: BTreeMap<String, (i64, Value)>,
    dataset_info: Value,
}

impl Column {
    /// The baseline has no index: its S1 / S5 "times" are not comparable.
    fn no_index(&self) -> bool {
        self.backend == "current"
    }
}

impl Column {
    fn put(&mut self, key: &str, started: i64, value: &Value) {
        let newer = self.scenarios.get(key).map(|(t, _)| started > *t).unwrap_or(true);
        if newer {
            self.scenarios.insert(key.to_string(), (started, value.clone()));
        }
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.scenarios.get(key).map(|(_, v)| v).filter(|v| v.get("error").is_none())
    }
}

fn fmt_ms(v: Option<&Value>) -> String {
    match v.and_then(Value::as_f64) {
        None => "—".into(),
        Some(x) if x >= 1000.0 => format!("{:.2} s", x / 1000.0),
        Some(x) => format!("{x:.1} ms"),
    }
}

fn fmt_mb(v: Option<&Value>) -> String {
    v.and_then(Value::as_f64)
        .map(|b| format!("{:.1} MB", b / 1_048_576.0))
        .unwrap_or_else(|| "—".into())
}

fn fmt_diff(v: Option<&Value>) -> String {
    match v {
        None => "—".into(),
        Some(d) => {
            let (m, e, s) = (&d["missing"], &d["extra"], &d["staleSize"]);
            if m == 0 && e == 0 && s == 0 {
                "✓".into()
            } else {
                format!("缺 {m} / 多 {e} / 大小旧 {s}")
            }
        }
    }
}

fn main() -> BResult<()> {
    let cli = Cli::parse();
    let mut columns: BTreeMap<String, Column> = BTreeMap::new();

    for entry in std::fs::read_dir(results_dir())?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") {
            continue;
        }
        let report: Value = match serde_json::from_str(&std::fs::read_to_string(entry.path())?) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if report["dataset"] != cli.dataset.as_str() {
            continue;
        }
        let backend = report["backend"].as_str().unwrap_or("?");
        let variant = report["options"]["variant"].as_str().unwrap_or("");
        let key = if variant.is_empty() {
            backend.to_string()
        } else {
            format!("{backend} ({variant})")
        };
        let started = report["startedAt"].as_i64().unwrap_or(0);
        let col = columns.entry(key).or_default();
        col.backend = backend.to_string();
        if !report["datasetInfo"].is_null() {
            col.dataset_info = report["datasetInfo"].clone();
        }
        let scenarios = report["scenarios"].as_object().cloned().unwrap_or_default();
        let after_resume = scenarios.contains_key("s5");
        for (s, v) in &scenarios {
            let k = if s == "s6" && after_resume { "s6-after-s5" } else { s.as_str() };
            col.put(k, started, v);
        }
    }
    if columns.is_empty() {
        return Err(format!("no reports for {} in {}", cli.dataset, results_dir().display()).into());
    }

    let mut keys: Vec<&String> = columns.keys().collect();
    keys.sort_by_key(|k| {
        let b = k.split(' ').next().unwrap_or("");
        (ORDER.iter().position(|o| *o == b).unwrap_or(ORDER.len()), k.to_string())
    });
    let cols: Vec<&Column> = keys.iter().map(|k| &columns[*k]).collect();

    // Rule names in report order, from any column that has them.
    let rule_names = |scenario: &str| -> Vec<String> {
        cols.iter()
            .find_map(|c| c.get(scenario).and_then(Value::as_object).map(|o| o.keys().cloned().collect()))
            .unwrap_or_default()
    };

    let mut md = String::new();
    let info = cols.iter().map(|c| &c.dataset_info).find(|i| !i.is_null());
    writeln!(md, "# 搜索方案基准对比：{}\n", cli.dataset)?;
    if let Some(i) = info {
        writeln!(md, "数据集：{} 个文件，{} 个目录。\n", i["files"], i["dirs"])?;
    }
    writeln!(md, "| 指标 | {} |", keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(" | "))?;
    writeln!(md, "|---|{}", "---|".repeat(keys.len()))?;
    let mut row = |label: String, f: &dyn Fn(&Column) -> String| -> std::fmt::Result {
        writeln!(md, "| {label} | {} |", cols.iter().map(|c| f(c)).collect::<Vec<_>>().join(" | "))
    };

    row("**S1** 首次索引".into(), &|c| {
        if c.no_index() {
            "无索引（每次都全量扫描）".into()
        } else {
            fmt_ms(c.get("s1").and_then(|v| v.get("elapsedMs")))
        }
    })?;
    row("S1 内存峰值".into(), &|c| fmt_mb(c.get("s1").and_then(|v| v.get("peakRssBytes"))))?;
    row("S1 索引占用磁盘".into(), &|c| fmt_mb(c.get("s1").and_then(|v| v.get("stateBytes"))))?;
    for r in rule_names("s2") {
        row(format!("**S2** 列出 `{r}`（中位数）"), &|c| {
            let v = c.get("s2").and_then(|v| v.get(&r));
            match v {
                Some(v) => format!("{}（{} 个）", fmt_ms(v.get("medianMs")), v["total"]),
                None => "—".into(),
            }
        })?;
    }
    for r in rule_names("s3") {
        row(format!("**S3** 搜索 `{r}`（各词中位数）"), &|c| {
            fmt_ms(c.get("s3").and_then(|v| v.get(&r)).and_then(|v| v.get("medianOfTermsMs")))
        })?;
    }
    for (i, phase) in ["新建 500 个文件", "改 / 重命名 / 删", "删除整个目录"].iter().enumerate() {
        row(format!("**S4** 实时反映：{phase}"), &|c| {
            match c.get("s4").and_then(|v| v.get("phases")).and_then(|p| p.get(i)) {
                Some(p) if p["catchUpMs"].is_null() => "超时".into(),
                Some(p) => fmt_ms(p.get("catchUpMs")),
                None => "—".into(),
            }
        })?;
    }
    row("**S5** 重启后追平（resume 耗时）".into(), &|c| {
        if c.no_index() {
            "无索引".into()
        } else {
            fmt_ms(c.get("s5").and_then(|v| v.get("resumeMs")))
        }
    })?;
    row("S5 文件集合一致".into(), &|c| fmt_ms(c.get("s5").and_then(|v| v.get("setConsistentMs"))))?;
    row("S5 可见文件完全一致".into(), &|c| fmt_ms(c.get("s5").and_then(|v| v.get("visibleConsistentMs"))))?;
    row("S5 完全一致（含大小）".into(), &|c| fmt_ms(c.get("s5").and_then(|v| v.get("fullyConsistentMs"))))?;
    for r in rule_names("s6") {
        row(format!("**S6** 正确性 `{r}`"), &|c| fmt_diff(c.get("s6").and_then(|v| v.get(&r))))?;
    }
    for r in rule_names("s6-after-s5") {
        row(format!("S6 追平后 `{r}`"), &|c| fmt_diff(c.get("s6-after-s5").and_then(|v| v.get(&r))))?;
    }
    writeln!(
        md,
        "\n“—” 表示该方案没有跑这个场景或不适用；“缺 / 多 / 大小旧”是与 app 当前扫描结果（scan.rs）相比的差异。"
    )?;

    print!("{md}");
    let out = results_dir().join(format!("compare-{}.md", cli.dataset));
    std::fs::write(&out, &md)?;
    eprintln!("written to {}", out.display());
    Ok(())
}
