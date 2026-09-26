use glob::{MatchOptions, Pattern};
use regex::{Regex, RegexBuilder};

const GLOB_OPTIONS: MatchOptions = MatchOptions {
    case_sensitive: false,
    require_literal_separator: false,
    require_literal_leading_dot: false,
};

pub enum Matcher {
    Glob(Vec<Pattern>),
    Regex(Regex),
}

impl Matcher {
    pub fn matches(&self, name: &str) -> bool {
        match self {
            Matcher::Glob(ps) => ps.iter().any(|p| p.matches_with(name, GLOB_OPTIONS)),
            Matcher::Regex(r) => r.is_match(name),
        }
    }
}

pub fn build_matcher(match_type: &str, pattern: &str) -> Result<Matcher, String> {
    match match_type {
        "regex" => {
            let re = RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|e| e.to_string())?;
            Ok(Matcher::Regex(re))
        }
        _ => {
            let patterns: Vec<Pattern> = pattern
                .split(';')
                .filter(|s| !s.trim().is_empty())
                .filter_map(|s| Pattern::new(s.trim()).ok())
                .collect();
            Ok(Matcher::Glob(patterns))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_is_case_insensitive() {
        let m = build_matcher("glob", "*.png;*.jpg").unwrap();
        assert!(m.matches("a.PNG"));
        assert!(m.matches("b.jpg"));
        assert!(!m.matches("c.gif"));
    }

    #[test]
    fn regex_is_case_insensitive() {
        let m = build_matcher("regex", r"^(?:report\.pdf|notes)$").unwrap();
        assert!(m.matches("REPORT.PDF"));
        assert!(m.matches("Notes"));
        assert!(!m.matches("report.pdf.bak"));
    }

    #[test]
    fn regex_can_opt_out_of_case_insensitivity() {
        let m = build_matcher("regex", r"(?-i)^A\.txt$").unwrap();
        assert!(m.matches("A.txt"));
        assert!(!m.matches("a.txt"));
    }

    #[test]
    fn empty_glob_matches_nothing() {
        let m = build_matcher("glob", "").unwrap();
        assert!(!m.matches("a.txt"));
    }
}
