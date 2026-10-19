//! Owl dictionary replacements: "heard this → write that".

use crate::settings::{AppSettings, Replacement};
use regex::{Captures, Regex};
use std::sync::Mutex;

/// Tells an AI the user's Words and Dictionary terms, so it can fix one the
/// speech model misheard as a sound-alike ("ask cloud to review" → "ask
/// Claude to review"). None when there are none.
pub fn spelling_rule(settings: &AppSettings) -> Option<String> {
    let mut spellings: Vec<&str> = settings
        .replacements
        .iter()
        .map(|r| r.to.trim())
        .chain(settings.custom_words.iter().map(|w| w.trim()))
        .filter(|w| !w.is_empty())
        .collect();
    spellings.sort_unstable();
    spellings.dedup();
    (!spellings.is_empty()).then(|| {
        format!(
            "The speaker often says these names and terms: {}. Speech recognition can \
             mishear one as an ordinary word that sounds alike. Where such a word makes no \
             sense in the sentence but one of these terms does, write the term. Otherwise \
             leave the word as it is.",
            spellings.join(", ")
        )
    })
}

/// The compiled form of the last `replacements` seen. They run twice per
/// dictation (before and after the AI), and compiling a regex per entry
/// each time adds up on the paste path.
#[allow(clippy::type_complexity)]
static COMPILED: Mutex<(Vec<Replacement>, Vec<(Regex, String)>)> =
    Mutex::new((Vec::new(), Vec::new()));

fn compiled(replacements: &[Replacement]) -> Vec<(Regex, String)> {
    let mut cache = COMPILED.lock().unwrap_or_else(|e| e.into_inner());
    if cache.0.as_slice() != replacements {
        *cache = (
            replacements.to_vec(),
            replacements.iter().filter_map(compile).collect(),
        );
    }
    cache.1.clone()
}

/// One replacement as a pattern and what it writes. English (ASCII) terms
/// match whole words, ignoring case; other scripts such as Chinese match
/// exactly, since they have no spaces between words. Text already written
/// as the replacement is matched first and kept (group 1), so "claude →
/// Claude Code" never makes "Claude Code Code", even when this runs again
/// on the AI's reply.
fn compile(r: &Replacement) -> Option<(Regex, String)> {
    let from = r.from.trim();
    if from.is_empty() {
        return None;
    }
    let (flags, from) = if from.is_ascii() {
        // \b only where the term starts/ends with a word character, so
        // terms like "C++" still match.
        let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        let pattern = format!(
            "{}{}{}",
            if word(from.chars().next()) { r"\b" } else { "" },
            regex::escape(from),
            if word(from.chars().last()) { r"\b" } else { "" },
        );
        ("(?i)", pattern)
    } else {
        ("", regex::escape(from))
    };
    let keep = if r.to.is_empty() {
        String::new()
    } else {
        format!("({})|", regex::escape(&r.to))
    };
    Regex::new(&format!("{flags}{keep}{from}"))
        .ok()
        .map(|re| (re, r.to.clone()))
}

/// Applies every replacement in order.
pub fn apply_replacements(text: &str, replacements: &[Replacement]) -> String {
    let mut out = text.to_string();
    for (re, to) in compiled(replacements) {
        out = re
            .replace_all(&out, |c: &Captures| match c.get(1) {
                Some(kept) => kept.as_str().to_string(),
                None => to.clone(),
            })
            .into_owned();
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
    fn running_twice_keeps_a_replacement_that_contains_its_trigger() {
        let r = vec![rep("claude", "Claude Code"), rep("next", "Next.js")];
        let once = apply_replacements("ask claude about next", &r);
        assert_eq!(once, "ask Claude Code about Next.js");
        assert_eq!(apply_replacements(&once, &r), once);
        let r = vec![rep("teh", "the")];
        assert_eq!(
            apply_replacements("There is teh cat", &r),
            "There is the cat"
        );
    }

    #[test]
    fn empty_from_is_ignored() {
        let r = vec![rep("", "x")];
        assert_eq!(apply_replacements("hello", &r), "hello");
    }
}
