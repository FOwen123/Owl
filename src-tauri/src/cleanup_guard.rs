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
/// reason it was rejected.
pub fn check(said: &str, reply: &str) -> Result<String, &'static str> {
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

    let heard: HashSet<&String> = before.iter().collect();
    let meaningful: Vec<&String> = after
        .iter()
        .filter(|u| u.chars().count() > 3 || u.chars().any(|c| c.is_ascii_digit() || !c.is_ascii()))
        .collect();
    let new = meaningful.iter().filter(|u| !heard.contains(*u)).count();
    if new * 4 > meaningful.len() {
        return Err("reply added too many new words");
    }

    Ok(reply.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn too_many_new_words_are_rejected() {
        let said = "send the report to anna tomorrow morning please";
        assert!(check(
            said,
            "Transmit the document to Anna tomorrow morning, kindly."
        )
        .is_err());
    }
}
