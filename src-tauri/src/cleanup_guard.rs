//! Owl: checks on the AI cleanup's reply. A reply that answers, adds content
//! or changes the length too much is thrown away and the rule-cleaned text is
//! pasted instead.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::time::Duration;

/// An English word or a single Chinese character: the unit lengths are
/// measured in, since Chinese has no spaces.
static UNIT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"[A-Za-z0-9'’]+|\p{Han}").expect("valid regex"));

const PREAMBLES: &[&str] = &[
    "here is",
    "here's",
    "sure,",
    "sure!",
    "certainly",
    "of course",
    "the cleaned",
    "cleaned text",
    "corrected text",
    "以下是",
    "這是",
    "这是",
    "當然",
    "当然",
    "好的，以下",
];

/// A line that heads one of several versions: "Option 1", "**Version 2:**", "方案二".
static VERSION_HEADING: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?im)^[\s>*#_-]*(option|version|alternative|選項|选项|方案|版本)\s*[0-9一二三四五]",
    )
    .expect("valid regex")
});

/// An instruction asking for more than one version: "give me three options".
static ASKS_FOR_SEVERAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(options|versions|alternatives|variations|ways)\b|選項|选项|版本|方案|幾種|几种",
    )
    .expect("valid regex")
});

/// Why an Edit by voice reply can't be pasted as it is: it opens like a chat
/// answer, or offers several versions to choose from when one edit was asked for.
pub fn edit_problem(
    instruction: &str,
    selection: Option<&str>,
    reply: &str,
) -> Option<&'static str> {
    let (given, reply_lower) = (
        selection.unwrap_or_default().trim().to_lowercase(),
        reply.trim().to_lowercase(),
    );
    if PREAMBLES
        .iter()
        .any(|p| reply_lower.starts_with(p) && !given.starts_with(p))
    {
        return Some("it started with a preamble");
    }
    if !ASKS_FOR_SEVERAL.is_match(instruction)
        && VERSION_HEADING.find_iter(reply).count() > VERSION_HEADING.find_iter(&given).count() + 1
    {
        return Some("it offered several versions");
    }
    None
}

fn units(text: &str) -> Vec<String> {
    UNIT.find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .collect()
}

/// Dictations under 4 words (or Chinese characters) go out as the rules left them.
pub fn worth_cleaning(text: &str) -> bool {
    units(text).len() >= 4
}

/// How long to wait for the AI before pasting the rule-cleaned text.
pub fn time_limit(text: &str) -> Duration {
    Duration::from_secs(10 + units(text).len() as u64 / 10)
}

/// The AI's reply if it looks like an edit of what was said, otherwise the
/// reason it was rejected. `words` (the user's Words) count as said: writing
/// one the speech model misheard is a fix, not new content.
pub fn check(said: &str, reply: &str, words: &[String]) -> Result<String, &'static str> {
    let reply = reply.trim();
    if reply.is_empty() {
        return Err("empty reply");
    }

    let (said_lower, reply_lower) = (said.trim().to_lowercase(), reply.to_lowercase());
    if PREAMBLES
        .iter()
        .any(|p| reply_lower.starts_with(p) && !said_lower.starts_with(p))
    {
        return Err("reply starts with a preamble");
    }

    let asks = |t: &str| t.contains('?') || t.contains('？');
    if asks(said) && !asks(reply) {
        return Err("reply dropped the question, so it likely answered it");
    }

    let (before, after) = (units(said), units(reply));
    let (b, a) = (before.len(), after.len());
    if a < (b * 2 / 5).max(1) || a > b + (b / 5).max(2) {
        return Err("reply changed the length too much");
    }

    let known: Vec<String> = words.iter().flat_map(|w| units(w)).collect();
    let heard: HashSet<&String> = before.iter().chain(&known).collect();
    let meaningful: Vec<&String> = after
        .iter()
        .filter(|u| u.chars().count() > 3 || u.chars().any(|c| c.is_ascii_digit() || !c.is_ascii()))
        .collect();
    // Digits don't count as new: the prompt turns "two" into 2 and lists into 1. 2. 3.
    let new = meaningful
        .iter()
        .filter(|u| !heard.contains(*u) && !u.chars().all(|c| c.is_ascii_digit()))
        .count();
    if new * 4 > meaningful.len() {
        return Err("reply added too many new words");
    }
    if invented(said, reply, &known) {
        return Err("reply inserted words that weren't said");
    }

    Ok(reply.to_string())
}

/// Whether the reply inserts content that wasn't said. Fixing a misheard
/// word swaps it for another, so only insertions count: two new English
/// words, or three new Chinese characters, mean the AI is adding its own
/// content ("software is the" → "software is eating the world").
fn invented(said: &str, reply: &str, known: &[String]) -> bool {
    use crate::correction::{diff, is_cjk_token, tokens};
    let lower = |t: &str| -> Vec<String> { tokens(t).iter().map(|w| w.to_lowercase()).collect() };
    let (a, b) = (lower(said), lower(reply));
    // The alignment table grows with both lengths; past this (several
    // minutes of speech) the length and new-word checks have to do.
    const MAX_UNITS: usize = 1500;
    if a.len() > MAX_UNITS || b.len() > MAX_UNITS {
        return false;
    }
    // An inflection of a word that was said ("talk" → "talking") isn't new.
    let heard = |w: &str| {
        a.iter().chain(known).any(|h| {
            h == w
                || (h.chars().count() >= 4
                    && w.chars().count() >= 4
                    && h.chars().take(4).eq(w.chars().take(4)))
        })
    };
    let (mut english, mut chinese) = (0, 0);
    for (old, new) in diff(&a, &b).1 {
        if !old.is_empty() {
            continue;
        }
        for w in &b[new] {
            if heard(w) || w.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            if is_cjk_token(w) {
                chinese += 1;
            } else if w.chars().count() > 3 {
                english += 1;
            }
        }
    }
    english >= 2 || chinese >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(said: &str, reply: &str) -> Result<String, &'static str> {
        super::check(said, reply, &[])
    }

    #[test]
    fn a_dictionary_word_is_a_fix_not_new_content() {
        let said = "please restart open claw now";
        let reply = "Please restart OpenClaw now.";
        assert!(check(said, reply).is_err());
        assert!(super::check(said, reply, &["OpenClaw".to_string()]).is_ok());
    }

    #[test]
    fn short_dictations_skip_the_ai() {
        assert!(!worth_cleaning("ok thanks"));
        assert!(!worth_cleaning("好的謝"));
        assert!(worth_cleaning("send it to Anna"));
        assert!(worth_cleaning("我們明天"));
    }

    #[test]
    fn the_time_limit_grows_with_length() {
        assert_eq!(time_limit("hi"), Duration::from_secs(10));
        assert_eq!(time_limit(&"word ".repeat(40)), Duration::from_secs(14));
    }

    #[test]
    fn a_normal_edit_is_kept() {
        let said = "let's meet tuesday sorry i mean wednesday at noon";
        assert_eq!(
            check(said, "Let's meet Wednesday at noon."),
            Ok("Let's meet Wednesday at noon.".to_string())
        );
        assert!(check("我們明天，不是，後天開會討論預算", "我們後天開會討論預算。").is_ok());
    }

    #[test]
    fn an_empty_reply_is_rejected() {
        assert!(check("send the report to Anna", "").is_err());
        assert!(check("send the report to Anna", "  \n ").is_err());
    }

    #[test]
    fn a_preamble_is_rejected_unless_it_was_said() {
        let said = "send the report to Anna tomorrow";
        assert!(check(
            said,
            "Here is the cleaned text: Send the report to Anna tomorrow."
        )
        .is_err());
        assert!(check(
            "我們明天開會討論預算",
            "以下是整理後的文字：我們明天開會討論預算。"
        )
        .is_err());
        assert!(check(
            "sure, send the report to Anna",
            "Sure, send the report to Anna."
        )
        .is_ok());
    }

    #[test]
    fn an_answer_is_rejected() {
        assert!(check(
            "what time is the meeting tomorrow?",
            "The meeting is at 3 pm tomorrow."
        )
        .is_err());
        assert!(check("今天天氣怎麼樣？", "今天天氣很好。").is_err());
    }

    #[test]
    fn too_big_a_length_change_is_rejected() {
        let said = "please send the quarterly report to anna before the meeting tomorrow";
        assert!(check(said, "Send it.").is_err());
        assert!(check(
            said,
            "Please send the quarterly report to Anna before the meeting tomorrow, and make sure the numbers are double-checked by finance."
        )
        .is_err());
    }

    #[test]
    fn numbers_and_lists_are_kept() {
        assert!(check(
            "one buy milk two call mom three pay rent",
            "1. Buy milk\n2. Call mom\n3. Pay rent"
        )
        .is_ok());
        assert!(check("set the port to eighty eighty", "Set the port to 8080.").is_ok());
    }

    #[test]
    fn invented_content_is_rejected() {
        // 2026-10-21: the AI finished a half-said phrase and flipped its point.
        let said = "a lot of people are saying software is the, but then I think it's not like software is taking off";
        assert!(check(
            said,
            "A lot of people are saying software is eating the world, but I think it's not—software is taking off."
        )
        .is_err());
        assert!(check(
            "我們明天開會討論預算的問題",
            "我們明天開會討論明年行銷預算的問題。"
        )
        .is_err());
        // Fixing a misheard name is a swap, not an insertion.
        assert!(check(
            "then we talk about Stefan's paradox and the microchef versus the restaurant",
            "Then we talk about Jevons paradox and the microchef versus the restaurant."
        )
        .is_ok());
    }

    #[test]
    fn too_many_new_words_are_rejected() {
        let said = "send the report to anna tomorrow morning please";
        assert!(check(
            said,
            "Transmit the document to Anna tomorrow morning, kindly."
        )
        .is_err());
    }
}
