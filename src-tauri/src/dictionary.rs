//! Owl dictionary replacements: "heard this → write that".

use crate::settings::Replacement;
use regex::{NoExpand, Regex};

/// Applies every replacement in order. English (ASCII) terms match whole
/// words, ignoring case; other scripts such as Chinese match exactly, since
/// they have no spaces between words.
pub fn apply_replacements(text: &str, replacements: &[Replacement]) -> String {
    let mut out = text.to_string();
    for r in replacements.iter().filter(|r| !r.from.trim().is_empty()) {
        let from = r.from.trim();
        out = if from.is_ascii() {
            // \b only where the term starts/ends with a word character, so
            // terms like "C++" still match.
            let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
            let pattern = format!(
                "(?i){}{}{}",
                if word(from.chars().next()) { r"\b" } else { "" },
                regex::escape(from),
                if word(from.chars().last()) { r"\b" } else { "" },
            );
            match Regex::new(&pattern) {
                Ok(re) => re.replace_all(&out, NoExpand(&r.to)).into_owned(),
                Err(_) => out,
            }
        } else {
            out.replace(from, &r.to)
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(from: &str, to: &str) -> Replacement {
        Replacement {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    #[test]
    fn english_terms_match_whole_words_ignoring_case() {
        let r = vec![rep("cloud code", "Claude Code")];
        assert_eq!(
            apply_replacements("Open Cloud Code now", &r),
            "Open Claude Code now"
        );
        assert_eq!(apply_replacements("cloud coder", &r), "cloud coder");
    }

    #[test]
    fn chinese_terms_match_inside_text() {
        let r = vec![rep("台積店", "台積電")];
        assert_eq!(
            apply_replacements("我買了台積店的股票", &r),
            "我買了台積電的股票"
        );
    }

    #[test]
    fn replacement_text_is_literal() {
        let r = vec![rep("price", "$1")];
        assert_eq!(apply_replacements("the price", &r), "the $1");
    }

    #[test]
    fn empty_from_is_ignored() {
        let r = vec![rep("", "x")];
        assert_eq!(apply_replacements("hello", &r), "hello");
    }
}
