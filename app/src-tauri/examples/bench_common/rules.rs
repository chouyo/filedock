//! Benchmark categories, loaded from `examples/bench_rules.json`.
//!
//! Each rule is one FileDock category with a single target, so it maps 1:1
//! onto the app's `Category` / `TargetRule`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::BResult;
use crate::config::{Category, TargetRule};
use crate::matcher::{build_matcher, Matcher};

pub const DEFAULT_RULES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/bench_rules.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RulesFile {
    rules: Vec<RawRule>,
    search_terms: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRule {
    name: String,
    /// Relative to the dataset root (`.` is the root itself).
    dir: String,
    match_type: String,
    pattern: String,
    recursive: bool,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub name: String,
    /// Absolute directory.
    pub dir: PathBuf,
    pub match_type: String,
    pub pattern: String,
    pub recursive: bool,
}

pub struct RuleSet {
    pub rules: Vec<Rule>,
    pub search_terms: Vec<String>,
}

pub fn load(path: &Path, dataset_root: &Path) -> BResult<RuleSet> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read rules file {}: {e}", path.display()))?;
    let raw: RulesFile = serde_json::from_str(&text)?;
    let rules = raw
        .rules
        .into_iter()
        .map(|r| {
            let dir = if r.dir == "." {
                dataset_root.to_path_buf()
            } else {
                dataset_root.join(&r.dir)
            };
            Rule {
                name: r.name,
                dir,
                match_type: r.match_type,
                pattern: r.pattern,
                recursive: r.recursive,
            }
        })
        .collect();
    Ok(RuleSet {
        rules,
        search_terms: raw.search_terms,
    })
}

impl Rule {
    pub fn matcher(&self) -> Matcher {
        build_matcher(&self.match_type, &self.pattern).expect("invalid rule pattern")
    }

    /// The same rule as an app `Category`, for the app's own scanner.
    pub fn to_category(&self) -> Category {
        Category {
            id: self.name.clone(),
            name: self.name.clone(),
            order: 0,
            targets: vec![TargetRule {
                id: self.name.clone(),
                dir: self.dir.to_string_lossy().into_owned(),
                match_type: self.match_type.clone(),
                pattern: self.pattern.clone(),
                recursive: self.recursive,
            }],
            created_at: 0,
            updated_at: 0,
        }
    }

    /// Whether `path` (a file) lies in this rule's scope, ignoring the pattern.
    pub fn in_scope(&self, path: &Path) -> bool {
        match path.parent() {
            Some(parent) if self.recursive => parent.starts_with(&self.dir),
            Some(parent) => parent == self.dir,
            None => false,
        }
    }

    /// Scope and pattern check, as the app's scanner applies it.
    pub fn accepts(&self, path: &Path, matcher: &Matcher) -> bool {
        self.in_scope(path)
            && path
                .file_name()
                .map(|n| matcher.matches(&n.to_string_lossy()))
                .unwrap_or(false)
    }

    /// Extensions the pattern is limited to, if it is purely `*.ext;*.ext…`.
    /// Lets index-backed backends push the filter down.
    pub fn simple_extensions(&self) -> Option<Vec<String>> {
        if self.match_type != "glob" {
            return None;
        }
        let mut exts = Vec::new();
        for part in self.pattern.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let ext = part.strip_prefix("*.")?;
            if ext.is_empty() || ext.contains(['*', '?', '[', ']', '.']) {
                return None;
            }
            exts.push(ext.to_lowercase());
        }
        if exts.is_empty() {
            None
        } else {
            Some(exts)
        }
    }
}

/// Minimal set of `(dir, recursive)` roots that covers every rule, so each
/// directory is walked and watched once.
pub fn covering_roots(rules: &[Rule]) -> Vec<(PathBuf, bool)> {
    let mut roots: Vec<(PathBuf, bool)> = Vec::new();
    // Recursive roots first, shortest first, so ancestors absorb descendants.
    let mut sorted: Vec<&Rule> = rules.iter().collect();
    sorted.sort_by_key(|r| (!r.recursive, r.dir.components().count()));
    for rule in sorted {
        let covered = roots.iter().any(|(dir, recursive)| {
            (*recursive && rule.dir.starts_with(dir)) || (dir == &rule.dir && !rule.recursive)
        });
        if !covered {
            roots.push((rule.dir.clone(), rule.recursive));
        }
    }
    roots
}

/// A recursive rule over the whole dataset (`dir: "."`, pattern `*`), which the
/// live-update scenario needs to observe changes anywhere.
pub fn everything_rule<'a>(rules: &'a [Rule], dataset_root: &Path) -> Option<&'a Rule> {
    rules
        .iter()
        .find(|r| r.recursive && r.dir == dataset_root && r.match_type == "glob" && r.pattern.trim() == "*")
}
