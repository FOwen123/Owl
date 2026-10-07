//! Owl snippets: saying a trigger phrase pastes saved text instead.

use crate::settings::Snippet;

/// Returns the saved text when the whole utterance is a snippet trigger.
/// Case, spaces and punctuation are ignored, so "Insert my email." matches
/// "insert my email" and "我的地址。" matches "我的地址".
pub fn match_snippet<'a>(transcription: &str, snippets: &'a [Snippet]) -> Option<&'a str> {
    let spoken = normalize(transcription);
    if spoken.is_empty() {
        return None;
    }
    snippets
        .iter()
        .find(|s| normalize(&s.trigger) == spoken)
        .map(|s| s.text.as_str())
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(trigger: &str, text: &str) -> Snippet {
        Snippet {
            trigger: trigger.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn whole_utterance_matches_ignoring_case_and_punctuation() {
        let snippets = vec![snippet("insert my email", "me@example.com")];
        assert_eq!(
            match_snippet("Insert my email.", &snippets),
            Some("me@example.com")
        );
    }

    #[test]
    fn chinese_trigger_matches_without_spaces_or_full_width_punctuation() {
        let snippets = vec![snippet("我的地址", "台北市信義區")];
        assert_eq!(match_snippet("我的地址。", &snippets), Some("台北市信義區"));
        assert_eq!(match_snippet("我的 地址", &snippets), Some("台北市信義區"));
    }

    #[test]
    fn trigger_inside_a_longer_sentence_does_not_match() {
        let snippets = vec![snippet("my email", "me@example.com")];
        assert_eq!(match_snippet("please send it to my email", &snippets), None);
    }

    #[test]
    fn empty_trigger_never_matches() {
        let snippets = vec![snippet("  ", "oops")];
        assert_eq!(match_snippet("", &snippets), None);
        assert_eq!(match_snippet("...", &snippets), None);
    }
}
