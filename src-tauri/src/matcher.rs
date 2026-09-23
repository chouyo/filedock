use glob::Pattern;
use regex::Regex;

pub enum Matcher {
    Glob(Vec<Pattern>),
    Regex(Regex),
}

impl Matcher {
    pub fn matches(&self, name: &str) -> bool {
        match self {
            Matcher::Glob(ps) => ps.iter().any(|p| p.matches(name)),
            Matcher::Regex(r) => r.is_match(name),
        }
    }
}

pub fn build_matcher(match_type: &str, pattern: &str) -> Result<Matcher, String> {
    match match_type {
        "regex" => {
            let re = Regex::new(pattern).map_err(|e| e.to_string())?;
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
