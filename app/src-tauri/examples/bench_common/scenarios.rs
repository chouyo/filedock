//! The benchmark scenarios, identical for every backend.
//!
//! | id | measures |
//! |----|----------|
//! | s1 | building the index from scratch |
//! | s2 | a category's count and first page (switching categories) |
//! | s3 | name search inside a category |
//! | s4 | how fast live changes show up while watching |
//! | s5 | catching up after changes made while nothing ran (restart) |
//! | s6 | correctness against the app's current scanner |

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::backend::{Backend, Entry};
use super::cli::CommonArgs;
use super::metrics::{median, ms, peak_rss_bytes, time};
use super::mutate::{live_change, live_create};
use super::paths::{dataset_root, manifest_path, mutation_journal_path};
use super::report::Report;
use super::rules::{self, everything_rule, Rule, RuleSet};
use super::truth::{diff, in_hidden_dir, scan_dir, scan_rule, Diff};
use super::BResult;

/// Runs the requested scenarios and saves the report.
pub fn run(backend: &mut dyn Backend, args: &CommonArgs, options: Map<String, Value>) -> BResult<()> {
    let root = dataset_root(&args.dataset);
    if !root.is_dir() {
        return Err(format!(
            "dataset {} not found; create it with `cargo run --release --example bench_dataset -- gen`",
            root.display()
        )
        .into());
    }
    let manifest = std::fs::read_to_string(manifest_path(&args.dataset))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or(Value::Null);
    let rules = rules::load(&args.rules, &root)?;
    let scenarios = args.scenarios();
    let mutated = mutation_journal_path(&args.dataset).exists();

    let mut report = Report::new(backend.name(), &args.dataset, manifest);
    report.options = options;
    report.options.insert("scenarios".into(), json!(scenarios));
    report.options.insert("page".into(), json!(args.page));
    report.options.insert("repeat".into(), json!(args.repeat));
    report.options.insert("timeoutSecs".into(), json!(args.timeout));
    report.options.insert("datasetMutated".into(), json!(mutated));

    if scenarios.iter().any(|s| s == "s1") && mutated {
        eprintln!("warning: dataset has pending offline mutations; run `bench_dataset revert` for comparable S1 numbers");
    }
    if scenarios.iter().any(|s| s == "s5") && !mutated {
        eprintln!("warning: dataset is not mutated; run `bench_dataset mutate` before S5");
    }
    if !scenarios.iter().any(|s| s == "s1") {
        backend.open(&rules.rules)?;
    }

    for s in &scenarios {
        eprintln!("[{}] {s} …", backend.name());
        let started = Instant::now();
        let result = match s.as_str() {
            "s1" => s1_index(backend, &rules),
            "s2" => s2_list(backend, &rules, args),
            "s3" => s3_search(backend, &rules, args),
            "s4" => s4_live(backend, &rules, &root, args),
            "s5" => s5_resume(backend, &rules, args),
            "s6" => s6_correctness(backend, &rules),
            other => Err(format!("unknown scenario {other}").into()),
        };
        let value = match result {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[{}] {s} failed: {e}", backend.name());
                json!({ "error": e.to_string() })
            }
        };
        eprintln!("[{}] {s} done in {:.1}s", backend.name(), started.elapsed().as_secs_f64());
        report.scenarios.insert(s.clone(), value);
    }

    let path = report.save()?;
    println!("{}", serde_json::to_string_pretty(&Value::Object(report.scenarios.clone()))?);
    eprintln!("report: {}", path.display());
    Ok(())
}

fn s1_index(backend: &mut dyn Backend, rules: &RuleSet) -> BResult<Value> {
    let (result, elapsed) = time(|| backend.index(&rules.rules));
    let notes = result?;
    Ok(json!({
        "elapsedMs": ms(elapsed),
        "peakRssBytes": peak_rss_bytes(),
        "stateBytes": backend.state_bytes(),
        "notes": notes,
    }))
}

fn s2_list(backend: &mut dyn Backend, rules: &RuleSet, args: &CommonArgs) -> BResult<Value> {
    let mut out = Map::new();
    for rule in &rules.rules {
        let mut runs = Vec::new();
        let mut total = 0;
        for _ in 0..args.repeat.max(1) {
            let (listing, d) = time(|| backend.list(rule, args.page));
            total = listing?.total;
            runs.push(ms(d));
        }
        out.insert(
            rule.name.clone(),
            json!({ "total": total, "firstMs": runs[0], "medianMs": median(runs.clone()) }),
        );
    }
    Ok(Value::Object(out))
}

fn s3_search(backend: &mut dyn Backend, rules: &RuleSet, args: &CommonArgs) -> BResult<Value> {
    let mut out = Map::new();
    for rule in &rules.rules {
        let mut per_term = Map::new();
        let mut medians = Vec::new();
        for term in &rules.search_terms {
            let mut runs = Vec::new();
            let mut hits = 0;
            for _ in 0..args.repeat.max(1) {
                let (count, d) = time(|| backend.search(rule, term));
                hits = count?;
                runs.push(ms(d));
            }
            let m = median(runs.clone());
            medians.push(m);
            per_term.insert(term.clone(), json!({ "hits": hits, "firstMs": runs[0], "medianMs": m }));
        }
        out.insert(
            rule.name.clone(),
            json!({ "medianOfTermsMs": median(medians), "terms": per_term }),
        );
    }
    Ok(Value::Object(out))
}

/// Polls `probe(live)` until it matches the file system, or times out.
fn wait_live(backend: &mut dyn Backend, live: &Path, timeout: Duration) -> BResult<Value> {
    let start = Instant::now();
    let mut polls = 0;
    loop {
        let (entries, probe_time) = time(|| backend.probe(live));
        let entries = entries?;
        polls += 1;
        let truth = if live.exists() { scan_dir(live) } else { Vec::new() };
        let d = diff(&truth, &entries);
        if d.is_clean() {
            return Ok(json!({ "catchUpMs": ms(start.elapsed()), "polls": polls }));
        }
        if start.elapsed() > timeout {
            return Ok(json!({ "catchUpMs": null, "polls": polls, "diffAtTimeout": d.to_json(3) }));
        }
        std::thread::sleep(probe_time.max(Duration::from_millis(50)));
    }
}

fn s4_live(backend: &mut dyn Backend, rules: &RuleSet, root: &Path, args: &CommonArgs) -> BResult<Value> {
    if everything_rule(&rules.rules, root).is_none() {
        return Ok(json!({ "skipped": "needs a recursive `*` rule on the dataset root" }));
    }
    let live = root.join(format!("_bench_live_{}", std::process::id()));
    // Leftovers of an interrupted run would otherwise be counted as changes.
    for entry in std::fs::read_dir(root)?.flatten() {
        if entry.file_name().to_string_lossy().starts_with("_bench_live_") {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    std::fs::create_dir(&live)?;

    let watch_notes = backend.start_watch(&rules.rules)?;
    // Let the watcher settle so directory creation above is not part of phase A.
    std::thread::sleep(Duration::from_secs(1));
    let timeout = Duration::from_secs(args.timeout);
    let mut phases = Vec::new();

    let (created, apply) = time(|| live_create(&live));
    let created = created?;
    let mut a = wait_live(backend, &live, timeout)?;
    a["name"] = json!("create");
    a["changes"] = json!(created.len());
    a["applyMs"] = json!(ms(apply));
    phases.push(a);

    let (summary, apply) = time(|| live_change(&created, 4242));
    let summary = summary?;
    let mut b = wait_live(backend, &live, timeout)?;
    b["name"] = json!("modify-rename-delete");
    b["changes"] = json!(summary);
    b["applyMs"] = json!(ms(apply));
    phases.push(b);

    let (removed, apply) = time(|| std::fs::remove_dir_all(&live));
    removed?;
    let mut c = wait_live(backend, &live, timeout)?;
    c["name"] = json!("remove-tree");
    c["applyMs"] = json!(ms(apply));
    phases.push(c);

    backend.stop_watch();
    Ok(json!({ "watchNotes": watch_notes, "phases": phases }))
}

fn diff_all(backend: &mut dyn Backend, rules: &[Rule], truth: &[Vec<Entry>]) -> BResult<Vec<Diff>> {
    rules
        .iter()
        .zip(truth)
        .map(|(rule, t)| Ok(diff(t, &backend.snapshot(rule)?)))
        .collect()
}

fn s5_resume(backend: &mut dyn Backend, rules: &RuleSet, args: &CommonArgs) -> BResult<Value> {
    let truth: Vec<Vec<Entry>> = rules.rules.iter().map(scan_rule).collect();
    let start = Instant::now();
    let (notes, resume_time) = time(|| backend.resume(&rules.rules));
    let notes = notes?;

    let timeout = Duration::from_secs(args.timeout);
    // Set: same files. Full: same sizes too. Visible: full, ignoring files in
    // hidden directories (which an OS index may never cover).
    let mut set_consistent_ms = None;
    let mut visible_consistent_ms = None;
    let mut fully_consistent_ms = None;
    let mut diffs;
    loop {
        diffs = diff_all(backend, &rules.rules, &truth)?;
        let now = Some(ms(start.elapsed()));
        if set_consistent_ms.is_none() && diffs.iter().all(|d| d.missing.is_empty() && d.extra.is_empty()) {
            set_consistent_ms = now;
        }
        let visible = |v: &Vec<std::path::PathBuf>| v.iter().all(|p| in_hidden_dir(p));
        if visible_consistent_ms.is_none()
            && diffs.iter().all(|d| visible(&d.missing) && visible(&d.extra) && visible(&d.stale_size))
        {
            visible_consistent_ms = now;
        }
        if diffs.iter().all(Diff::is_clean) {
            fully_consistent_ms = now;
            break;
        }
        if !backend.settles_in_background() || visible_consistent_ms.is_some() || start.elapsed() > timeout {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    let per_rule: Map<String, Value> = rules
        .rules
        .iter()
        .zip(&diffs)
        .map(|(r, d)| (r.name.clone(), d.to_json(3)))
        .collect();
    Ok(json!({
        "resumeMs": ms(resume_time),
        "setConsistentMs": set_consistent_ms,
        "visibleConsistentMs": visible_consistent_ms,
        "fullyConsistentMs": fully_consistent_ms,
        "notes": notes,
        "rules": per_rule,
    }))
}

fn s6_correctness(backend: &mut dyn Backend, rules: &RuleSet) -> BResult<Value> {
    let mut out = Map::new();
    for rule in &rules.rules {
        let truth = scan_rule(rule);
        let actual = backend.snapshot(rule)?;
        let mut v = diff(&truth, &actual).to_json(5);
        v["truth"] = json!(truth.len());
        v["actual"] = json!(actual.len());
        out.insert(rule.name.clone(), v);
    }
    Ok(Value::Object(out))
}
