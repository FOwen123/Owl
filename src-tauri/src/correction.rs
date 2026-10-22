//! Learn from edits: after Owl pastes, compare the pasted text with what the
//! user turned it into and suggest Dictionary replacements for misheard words.

use crate::chinese_script::{convert_chinese_script, ChineseVariety};
use crate::settings::{ChineseScript, Replacement};
use pinyin::ToPinyinMulti;
use std::ops::Range;
use strsim::jaro_winkler;

/// Longer texts are skipped (the alignment table grows with length squared).
const MAX_TOKENS: usize = 512;
/// How alike two English spellings must be to count as a misheard word.
const MIN_SIMILARITY: f64 = 0.64;
const MAX_SUGGESTIONS: usize = 3;
/// Longest Chinese change (in characters) that can become a dictionary entry.
const MAX_CHINESE_CHARS: usize = 4;

const COMMON_WORDS: &[&str] = &[
    "a", "about", "after", "all", "also", "am", "an", "and", "are", "as", "at", "be", "because",
    "but", "by", "can", "could", "did", "do", "does", "for", "from", "get", "go", "had", "has",
    "have", "he", "her", "here", "him", "his", "how", "i", "if", "in", "is", "it", "its", "just",
    "like", "me", "my", "not", "now", "of", "on", "or", "our", "out", "she", "so", "that", "the",
    "their", "them", "then", "there", "they", "this", "to", "up", "us", "was", "we", "were",
    "what", "when", "where", "which", "who", "will", "with", "would", "you", "your",
];

/// Chinese characters that often stand alone as words. A one-character fix is
/// widened with a neighbour into a two-character entry; these make poor
/// neighbours, so the other side is used when possible.
// ponytail: fixed list instead of word segmentation; add a segmenter (e.g. jieba-rs) if widening picks bad pairs.
const STANDALONE_CHARS: &str = "的了在是我你他她它們们去有和也就都要會会說说這这那很不";

/// The user's edited version of the pasted text, or `None` when the edit
/// happened outside it. `baseline` is the text box right after the paste,
/// `current` is the text box now.
pub fn edited_span(baseline: &str, current: &str, pasted: &str) -> Option<String> {
    if pasted.trim().is_empty() {
        return None;
    }
    let start = baseline.rfind(pasted)?;
    let (head, tail) = (&baseline[..start], &baseline[start + pasted.len()..]);
    if current.len() < head.len() + tail.len()
        || !current.starts_with(head)
        || !current.ends_with(tail)
    {
        return None;
    }
    Some(current[head.len()..current.len() - tail.len()].to_string())
}

/// What the change from `pasted` to `edited` teaches. A suggestion with an
/// empty `from` is a Words entry (a name or term to spell right), the rest
/// are Dictionary replacements. `is_word` says whether the OS spell checker
/// knows a word: a real word is never made into a replacement, since the rule
/// would rewrite it everywhere ("skip" → "keep").
pub fn suggest(pasted: &str, edited: &str, is_word: &dyn Fn(&str) -> bool) -> Vec<Replacement> {
    let (a, b) = (tokens(pasted), tokens(edited));
    if a.is_empty() || b.is_empty() || a.len() > MAX_TOKENS || b.len() > MAX_TOKENS {
        return Vec::new();
    }
    let (matched, runs) = diff(&a, &b);
    if !shares_context(&a, &b, matched) {
        return Vec::new();
    }

    let mut out: Vec<Replacement> = Vec::new();
    for (old, new) in runs {
        if old.is_empty() || new.is_empty() {
            continue; // pure insertions and deletions aren't misheard words
        }
        let changed = || a[old.clone()].iter().chain(&b[new.clone()]);
        let found = if changed().all(|t| is_cjk_token(t)) {
            chinese_fix(&a, &b, old, new).into_iter().collect()
        } else if changed().all(|t| !is_cjk_token(t)) {
            english_fixes(&a[old], &b[new], is_word)
        } else {
            Vec::new() // mixed scripts, e.g. 二十 → 20, is formatting
        };
        for r in found {
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    out.truncate(MAX_SUGGESTIONS);
    out
}

// --- Tokens and alignment ---------------------------------------------------

pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{2FFFF}')
}

pub(crate) fn is_cjk_token(t: &str) -> bool {
    t.chars().all(is_cjk)
}

/// English words and single Chinese characters. Spaces and punctuation are
/// dropped, so punctuation-only edits never produce a suggestion.
pub(crate) fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<String>| {
        let w = word.trim_matches(['\'', '-', '_', '/']);
        if !w.is_empty() {
            out.push(w.to_string());
        }
        word.clear();
    };
    for c in text.chars() {
        if !is_cjk(c) && (c.is_alphanumeric() || "'-_/+#".contains(c)) {
            word.push(c);
            continue;
        }
        flush(&mut word, &mut out);
        if is_cjk(c) {
            out.push(c.to_string());
        }
    }
    flush(&mut word, &mut out);
    out
}

/// A changed run: (range in the pasted tokens, range in the edited tokens).
pub(crate) type Change = (Range<usize>, Range<usize>);

/// Longest-common-subsequence alignment. Returns the number of matching
/// tokens and the changed runs.
pub(crate) fn diff(a: &[String], b: &[String]) -> (usize, Vec<Change>) {
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut runs = Vec::new();
    let (mut i, mut j, mut si, mut sj) = (0, 0, 0, 0);
    loop {
        let at_end = i == n && j == m;
        if at_end || (i < n && j < m && a[i] == b[j]) {
            if si < i || sj < j {
                runs.push((si..i, sj..j));
            }
            if at_end {
                break;
            }
            i += 1;
            j += 1;
            (si, sj) = (i, j);
        } else if j == m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]) {
            i += 1;
        } else {
            j += 1;
        }
    }
    (lcs[0][0] as usize, runs)
}

/// The edit kept enough of the text that it is a fix, not a new message.
/// Owl: whether `edited` is a fix of `pasted`, not a cleared box (the message
/// was sent) or a different message typed over it.
pub fn is_fix(pasted: &str, edited: &str) -> bool {
    let (a, b) = (tokens(pasted), tokens(edited));
    if b.is_empty() || a.len() > MAX_TOKENS || b.len() > MAX_TOKENS {
        return false;
    }
    let (matched, _) = diff(&a, &b);
    shares_context(&a, &b, matched)
}

fn shares_context(a: &[String], b: &[String], matched: usize) -> bool {
    if matched > 0 && matched * 2 >= a.len() {
        return true;
    }
    // A very short paste may have no word left unchanged ("Owel" → "Owen").
    a.len() <= 3
        && jaro_winkler(&a.concat().to_lowercase(), &b.concat().to_lowercase()) >= MIN_SIMILARITY
}

// --- English -------------------------------------------------------------

fn english_fixes(
    old: &[String],
    new: &[String],
    is_word: &dyn Fn(&str) -> bool,
) -> Vec<Replacement> {
    // Same number of words changed: judge each word on its own.
    if old.len() == new.len() && old.len() > 1 {
        return old
            .iter()
            .zip(new)
            .filter_map(|(o, n)| {
                english_fix(std::slice::from_ref(o), std::slice::from_ref(n), is_word)
            })
            .collect();
    }
    english_fix(old, new, is_word).into_iter().collect()
}

fn english_fix(
    old: &[String],
    new: &[String],
    is_word: &dyn Fn(&str) -> bool,
) -> Option<Replacement> {
    let [to] = new else { return None };
    let from = old.join(" ");
    let plausible = if old.len() == 1 {
        single_word_fix(&from, to) || capitalized_name(&from, to, is_word)
    } else {
        joined_words_fix(old, to)
    };
    if !plausible {
        // A Word only nudges the speech model, so it's safe where a
        // replacement ("all" → "Owl") would rewrite every "all".
        return (old.len() == 1 && misheard_name(&from, to)).then(|| Replacement {
            from: String::new(),
            to: to.clone(),
        });
    }
    if has_strong_signal(to) || !is_word(to) {
        // A name or term ("iPhone", "depick"): learn the spelling as a Word.
        return Some(Replacement {
            from: String::new(),
            to: to.clone(),
        });
    }
    (!old.iter().all(|w| is_word(w))).then(|| Replacement {
        from,
        to: to.clone(),
    })
}

/// "depick" → "Depick": a capital on a word the spell checker doesn't know
/// marks a name. "then" → "Then" is just the start of a sentence.
fn capitalized_name(from: &str, to: &str, is_word: &dyn Fn(&str) -> bool) -> bool {
    from != to
        && from.to_lowercase() == to.to_lowercase()
        && to.chars().next().is_some_and(char::is_uppercase)
        && !is_common(from)
        && !is_word(to)
}

/// "all" → "Owl": a name the speech model wrote as an everyday word that
/// sounds the same.
// shortcut: compares consonants only, not real phonetics; use a metaphone
// crate if it starts suggesting unrelated names.
fn misheard_name(from: &str, to: &str) -> bool {
    let sound = |w: &str| {
        let mut out = String::new();
        for c in w.to_lowercase().chars() {
            if c.is_ascii_alphabetic() && !"aeiouwhy".contains(c) && !out.ends_with(c) {
                out.push(c);
            }
        }
        out
    };
    to.chars().next().is_some_and(char::is_uppercase)
        && !from.chars().any(char::is_uppercase)
        && from.to_lowercase() != to.to_lowercase()
        && !is_common(to)
        && !sound(from).is_empty()
        && sound(from) == sound(to)
}

fn is_common(word: &str) -> bool {
    COMMON_WORDS.contains(&word.to_lowercase().as_str())
}

fn is_acronym(v: &str) -> bool {
    v.chars().count() >= 2 && v.chars().all(|c| c.is_ascii_uppercase())
}

/// Marks a word as a name or term: "iPhone", "OpenAI", "i18n", "API".
fn has_strong_signal(v: &str) -> bool {
    v.chars().skip(1).any(char::is_uppercase)
        || v.chars()
            .any(|c| c.is_ascii_digit() || c == '_' || c == '/')
        || is_acronym(v)
}

/// "a p i" → "API", "chat GPT" → "ChatGPT".
fn joined_words_fix(old: &[String], to: &str) -> bool {
    if is_acronym(to) && old.len() <= 6 {
        let initials: String = old.iter().filter_map(|t| t.chars().next()).collect();
        if initials.to_lowercase() == to.to_lowercase() {
            return true;
        }
    }
    old.len() <= 2
        && !old.iter().any(|t| is_common(t))
        && jaro_winkler(&old.concat().to_lowercase(), &to.to_lowercase()) >= 0.82
}

fn single_word_fix(from: &str, to: &str) -> bool {
    let (fl, tl) = (from.to_lowercase(), to.to_lowercase());
    if fl.chars().count() < 2 || tl.chars().count() < 2 {
        return false;
    }
    let strong = has_strong_signal(to);
    let similarity = jaro_winkler(&fl, &tl);
    if fl == tl {
        return strong; // "iphone" → "iPhone" yes, "then" → "Then" no
    }
    if is_common(from) || is_common(to) {
        return strong && similarity >= 0.82; // "the" → "a" is grammar
    }
    let lowercase = |v: &str| !v.chars().any(char::is_uppercase);
    if lowercase(from) && lowercase(to) && (fl.starts_with(&tl) || tl.starts_with(&fl)) {
        return false; // "test" → "testing" is grammar
    }
    similarity >= MIN_SIMILARITY || is_number_shorthand(&fl, &tl) || (strong && similarity >= 0.55)
}

/// "internationalization" → "i18n": first letter, count of skipped letters, last letter.
fn is_number_shorthand(from: &str, to: &str) -> bool {
    let letters: Vec<char> = from.chars().filter(|c| c.is_alphabetic()).collect();
    let short: Vec<char> = to.chars().collect();
    if letters.len() < 4 || short.len() < 3 || short.len() >= letters.len() / 2 {
        return false;
    }
    let middle: String = short[1..short.len() - 1].iter().collect();
    short[0] == letters[0]
        && short[short.len() - 1] == letters[letters.len() - 1]
        && middle.parse::<usize>().ok() == Some(letters.len() - 2)
}

// --- Chinese (Owl's rules) ------------------------------------------------

fn chinese_fix(
    a: &[String],
    b: &[String],
    old: Range<usize>,
    new: Range<usize>,
) -> Option<Replacement> {
    let (o, n) = (a[old.clone()].concat(), b[new].concat());
    let (o_len, n_len) = (o.chars().count(), n.chars().count());
    if o_len > MAX_CHINESE_CHARS || n_len > MAX_CHINESE_CHARS {
        return None;
    }
    if !(same_word_other_script(&o, &n) || sounds_alike(&o, &n)) {
        return None;
    }
    if o_len > 1 && n_len > 1 {
        return Some(Replacement { from: o, to: n });
    }
    // A one-character entry would replace that character everywhere; widen it
    // with an unchanged neighbour (the same token in both texts).
    let cjk = |t: Option<&String>| t.filter(|t| is_cjk_token(t)).cloned();
    let prev = cjk(old.start.checked_sub(1).and_then(|i| a.get(i)));
    let next = cjk(a.get(old.end));
    match (prev, next) {
        (Some(p), next) if !(STANDALONE_CHARS.contains(p.as_str()) && next.is_some()) => {
            Some(Replacement {
                from: format!("{p}{o}"),
                to: format!("{p}{n}"),
            })
        }
        (_, Some(x)) => Some(Replacement {
            from: format!("{o}{x}"),
            to: format!("{n}{x}"),
        }),
        _ => None,
    }
}

/// 臺北 / 台北: the same word in Traditional and Simplified spelling.
fn same_word_other_script(a: &str, b: &str) -> bool {
    let simplified =
        |t: &str| convert_chinese_script(t, ChineseVariety::Mandarin, ChineseScript::Simplified);
    simplified(a) == simplified(b)
}

/// Each character pair shares a reading, ignoring tones and the sounds
/// Taiwanese Mandarin blurs (模糊音).
fn sounds_alike(a: &str, b: &str) -> bool {
    a.chars().count() == b.chars().count()
        && a.chars().zip(b.chars()).all(|(x, y)| {
            x == y || {
                let (rx, ry) = (readings(x), readings(y));
                rx.iter().any(|r| ry.contains(r))
            }
        })
}

fn readings(c: char) -> Vec<String> {
    c.to_pinyin_multi()
        .map(|multi| multi.into_iter().map(|p| blur(p.plain())).collect())
        .unwrap_or_default()
}

/// zh/z, ch/c, sh/s, n/l, ing/in, eng/en.
fn blur(syllable: &str) -> String {
    let mut s = syllable.to_string();
    for (from, to) in [("zh", "z"), ("ch", "c"), ("sh", "s"), ("n", "l")] {
        if let Some(rest) = s.strip_prefix(from) {
            s = format!("{to}{rest}");
            break;
        }
    }
    if s.ends_with("ing") || s.ends_with("eng") {
        s.pop(); // drop the final "g"
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words a spell checker knows, standing in for the OS one.
    fn is_word(w: &str) -> bool {
        const KNOWN: &str = "i met today let's have a meeting call tomorrow saw the dog outside \
            bought an work at now then we left early first asked about it need support are test \
            testing app hello world how you open code and run skip keep just simple send sent ship \
            shop depict cloud claude owen chat word";
        KNOWN.split_whitespace().any(|k| k == w.to_lowercase())
    }

    fn pairs(pasted: &str, edited: &str) -> Vec<(String, String)> {
        suggest(pasted, edited, &is_word)
            .into_iter()
            .map(|r| (r.from, r.to))
            .collect()
    }

    fn one(pasted: &str, edited: &str, from: &str, to: &str) {
        assert_eq!(
            pairs(pasted, edited),
            vec![(from.to_string(), to.to_string())],
            "{pasted:?} -> {edited:?}"
        );
    }

    fn none(pasted: &str, edited: &str) {
        assert_eq!(pairs(pasted, edited), vec![], "{pasted:?} -> {edited:?}");
    }

    #[test]
    fn a_sent_or_retyped_message_is_not_a_fix() {
        let pasted = "Restart Open Claw and check the logs.";
        assert!(super::is_fix(
            pasted,
            "Restart OpenClaw and check the logs."
        ));
        assert!(!super::is_fix(pasted, "")); // the box cleared when it was sent
        assert!(!super::is_fix(pasted, "lunch at noon tomorrow?"));
        assert!(super::is_fix("Owel", "Owen"));
    }

    #[test]
    fn a_name_heard_as_an_everyday_word_becomes_a_word() {
        one(
            "compare it with the current state of all",
            "compare it with the current state of Owl",
            "",
            "Owl",
        );
        none("the dog is good", "the dog is Great");
        none("then we left early", "Then we left early");
    }

    // --- Finding the edited copy of the pasted text -----------------------

    #[test]
    fn edited_span_is_the_pasted_text_after_the_edit() {
        assert_eq!(
            edited_span(
                "Dear team, I met Owel today. Thanks",
                "Dear team, I met Owen today. Thanks",
                "I met Owel today."
            )
            .as_deref(),
            Some("I met Owen today.")
        );
    }

    #[test]
    fn edited_span_ignores_edits_outside_the_pasted_text() {
        assert_eq!(
            edited_span(
                "Hi all. I met Owel.",
                "Hello all. I met Owel.",
                "I met Owel."
            ),
            None
        );
        assert_eq!(edited_span("abc", "abc", "missing"), None);
    }

    // --- English --------------------------------------------------------

    #[test]
    fn similar_spelling_is_a_fix() {
        one("I met Owel today", "I met Owen today", "Owel", "Owen");
    }

    #[test]
    fn a_different_word_is_a_rewrite() {
        none(
            "Let's have a meeting tomorrow",
            "Let's have a call tomorrow",
        );
    }

    #[test]
    fn common_words_are_grammar_edits() {
        none("I saw the dog outside", "I saw a dog outside");
    }

    #[test]
    fn names_and_brands_become_words() {
        one(
            "I bought an iphone today",
            "I bought an iPhone today",
            "",
            "iPhone",
        );
        one("I work at openai now", "I work at OpenAI now", "", "OpenAI");
        one("I used depict today", "I used depick today", "", "depick");
        one("we use depick daily", "we use Depick daily", "", "Depick");
        none("then we left early", "Then we left early");
    }

    #[test]
    fn real_words_never_become_replacements() {
        // The bug: dictating "just keep it simple" came out as "skip"; fixing it
        // must not teach Owl to turn every "skip" into "keep".
        none("just skip it simple", "just keep it simple");
        none("I send it today", "I sent it today");
        none("open the ship", "open the shop");
        none("open cloud code and run it", "open Claude Code and run it");
    }

    #[test]
    fn spelled_out_letters_become_an_acronym() {
        one("call the a p i first", "call the API first", "", "API");
    }

    #[test]
    fn split_words_joined_back() {
        one(
            "I asked chat GPT about it",
            "I asked ChatGPT about it",
            "",
            "ChatGPT",
        );
    }

    #[test]
    fn number_shorthand() {
        one(
            "we need internationalization support",
            "we need i18n support",
            "",
            "i18n",
        );
    }

    #[test]
    fn prefix_of_a_real_word_is_a_grammar_edit() {
        none("we are test the app", "we are testing the app");
    }

    #[test]
    fn punctuation_only_edits_are_ignored() {
        none("hello world how are you", "Hello world, how are you?");
    }

    #[test]
    fn unrelated_text_is_ignored() {
        none("I met Owel today", "Completely different message here");
    }

    #[test]
    fn several_words_fixed_in_one_run() {
        one(
            "open clode code and run it",
            "open Claude Code and run it",
            "clode",
            "Claude",
        );
    }

    #[test]
    fn very_long_texts_are_skipped() {
        let long = "word ".repeat(600);
        none(&format!("{long} Owel"), &format!("{long} Owen"));
    }

    // --- Chinese (Owl's rules) -------------------------------------------

    #[test]
    fn same_sound_is_a_misheard_character() {
        // 公事 and 公式 are both "gongshi"; one changed character is widened
        // with its neighbour so the entry never replaces a lone character.
        one("我們開會討論公事", "我們開會討論公式", "公事", "公式");
    }

    #[test]
    fn different_sound_is_a_rewrite() {
        none("我們明天去台北", "他們明天去台北");
    }

    #[test]
    fn near_sounds_count() {
        // 是 (shi) and 四 (si): Taiwanese Mandarin blurs sh and s.
        one("我有是個蘋果", "我有四個蘋果", "是個", "四個");
    }

    #[test]
    fn script_variants_count() {
        one("我住在臺北市", "我住在台北市", "臺北", "台北");
    }

    #[test]
    fn number_style_changes_are_ignored() {
        none("我買了二十個蘋果", "我買了20個蘋果");
    }

    #[test]
    fn long_chinese_rewrites_are_ignored() {
        none("今天天氣很好我們去公園", "今天下大雨我們待在家");
    }

    #[test]
    fn pinyin_covers_traditional_characters() {
        for c in ['臺', '們', '這', '說', '會', '電'] {
            assert!(!readings(c).is_empty(), "{c}");
        }
    }

    #[test]
    fn english_inside_chinese_uses_english_rules() {
        one("我用chat GPT寫的", "我用ChatGPT寫的", "", "ChatGPT");
    }
}
