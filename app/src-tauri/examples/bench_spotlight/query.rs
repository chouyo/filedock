//! Translating FileDock rules into Spotlight query strings.
//!
//! Spotlight's language only has `*` wildcards (plus the `c` modifier for
//! case-insensitive matching), no regex, and scopes are always recursive.
//! Queries are therefore made at least as broad as the rule, and the app's
//! own matcher filters the hits afterwards.

use crate::bench_common::rules::Rule;

const NOT_FOLDER: &str = r#"kMDItemContentType != "public.folder""#;

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Glob → Spotlight wildcard: `?` and `[...]` widen to `*`.
fn widen_glob(pattern: &str) -> String {
    let mut out = String::new();
    let mut in_class = false;
    for c in pattern.chars() {
        match c {
            '[' => {
                in_class = true;
                out.push('*');
            }
            ']' if in_class => in_class = false,
            _ if in_class => {}
            '?' => out.push('*'),
            _ => out.push(c),
        }
    }
    out
}

fn name_is(pattern: &str) -> String {
    // A lone `*` matches nothing in Spotlight's language; match every item
    // by type instead.
    if pattern.chars().all(|c| c == '*') {
        return r#"kMDItemContentTypeTree == "public.item""#.to_string();
    }
    format!(r#"kMDItemFSName == "{}"c"#, escape(pattern))
}

/// Query for every file a rule could match.
pub fn for_rule(rule: &Rule) -> String {
    let names = if rule.match_type == "regex" {
        name_is("*")
    } else {
        let parts: Vec<String> = rule
            .pattern
            .split(';')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| name_is(&widen_glob(p)))
            .collect();
        if parts.is_empty() {
            // An empty glob matches nothing in the app.
            return format!(r#"kMDItemFSName == "" && {NOT_FOLDER}"#);
        }
        parts.join(" || ")
    };
    format!("({names}) && {NOT_FOLDER}")
}

/// Query for names containing `text` (case-insensitive).
pub fn name_contains(text: &str) -> String {
    format!("{} && {NOT_FOLDER}", name_is(&format!("*{text}*")))
}

/// Query for every file.
pub fn all_files() -> String {
    format!("{} && {NOT_FOLDER}", name_is("*"))
}
