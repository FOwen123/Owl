//! Owl: which Words the speech model hears as hints, in what order. Speech
//! LLMs heed the start of the list most and get worse when flooded with
//! terms, so the words the learner saw misheard go first and the list can be
//! capped (`learner.rs` picks the cap that scores best).

/// `priority` words first (those still in `words`), then the rest, newest
/// first when `newest_first`, cut to `limit`.
pub fn hearing_words(
    words: &[String],
    priority: &[String],
    newest_first: bool,
    limit: Option<usize>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in priority {
        let word = words.iter().find(|w| w.eq_ignore_ascii_case(p));
        if let Some(w) = word.filter(|w| !out.contains(w)) {
            out.push(w.clone());
        }
    }
    let rest: Box<dyn Iterator<Item = &String>> = if newest_first {
        Box::new(words.iter().rev())
    } else {
        Box::new(words.iter())
    };
    for w in rest {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(w)) {
            out.push(w.clone());
        }
    }
    if let Some(limit) = limit {
        out.truncate(limit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::hearing_words;

    fn v(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn misheard_words_go_first_then_newest_and_the_list_is_capped() {
        let words = v(&["Claude", "Supabase", "Owl", "Jevons"]);
        assert_eq!(
            hearing_words(&words, &v(&["owl", "Gone"]), true, None),
            v(&["Owl", "Jevons", "Supabase", "Claude"])
        );
        assert_eq!(
            hearing_words(&words, &[], false, Some(2)),
            v(&["Claude", "Supabase"])
        );
    }
}
