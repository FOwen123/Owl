//! Owl: fixed cleanup rules for the Light and Medium levels. They are instant,
//! offline and run before any AI cleanup (`TODO-CLEANUP.md`).

use crate::correction::is_cjk;
use once_cell::sync::Lazy;
use regex::Regex;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

/// Runs every rule in order. `voice_commands` turns on spoken commands such as
/// "new line" (opt-in, because the words can be meant literally).
pub fn apply(text: &str, voice_commands: bool) -> String {
    if is_silence_phrase(text) {
        return String::new();
    }
    let mut t = text.to_string();
    if voice_commands {
        t = voice(&t);
    }
    t = chinese_fillers(&t);
    t = chinese_stutters(&t);
    t = english_fillers(&t);
    t = english_doubles(&t);
    t = self_corrections(&t);
    t = full_width_punctuation(&t);
    t = space_chinese_english(&t);
    tidy(&t, text)
}

// --- Silence phrases ---------------------------------------------------------

/// Phrases Whisper-family models invent on near-silent audio, compared with
/// case, spaces and punctuation removed. "Thank you." is left out on purpose:
/// people really dictate it.
const SILENCE_PHRASES: &[&str] = &[
    "thanksforwatching",
    "thankyouforwatching",
    "thankyousomuchforwatching",
    "字幕由amaraorg社群提供",
    "字幕由amaraorg社区提供",
    "請不吝點讚訂閱轉發打賞支持明鏡與點點欄目",
    "请不吝点赞订阅转发打赏支持明镜与点点栏目",
    "謝謝觀看",
    "谢谢观看",
];

fn is_silence_phrase(text: &str) -> bool {
    let key: String = text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    SILENCE_PHRASES.contains(&key.as_str())
}

// --- Voice commands ----------------------------------------------------------

static NEW_PARAGRAPH: Lazy<Regex> =
    Lazy::new(|| re(r"(?i)[ \t,]*(?:\bnew paragraph\b|新段落)[\s,.，。]*"));
static NEW_LINE: Lazy<Regex> =
    Lazy::new(|| re(r"(?i)[ \t,]*(?:\bnew line\b|換行|换行)[\s,.，。]*"));
static SPOKEN_PUNCTUATION: Lazy<Regex> = Lazy::new(|| {
    re(r"(?i)[ \t,]*\b(comma|question mark|exclamation mark|exclamation point|full stop)\b[ \t,.]*")
});
static SPOKEN_PUNCTUATION_ZH: Lazy<Regex> = Lazy::new(|| re(r"(逗號|逗号|句號|句号|問號|问号)"));
static SCRATCH_THAT: Lazy<Regex> =
    Lazy::new(|| re(r"(?i)(?:\bscratch that\b|刪掉上一句|删掉上一句)[.,!?。，！？]*"));

fn voice(text: &str) -> String {
    let t = NEW_PARAGRAPH.replace_all(text, "\n\n");
    let t = NEW_LINE.replace_all(&t, "\n");
    let t = SPOKEN_PUNCTUATION.replace_all(&t, |c: &regex::Captures| {
        let symbol = match c[1].to_lowercase().as_str() {
            "comma" => ",",
            "question mark" => "?",
            "full stop" => ".",
            _ => "!",
        };
        format!("{symbol} ")
    });
    let mut t = SPOKEN_PUNCTUATION_ZH
        .replace_all(&t, |c: &regex::Captures| match &c[1] {
            "逗號" | "逗号" => "，",
            "句號" | "句号" => "。",
            _ => "？",
        })
        .into_owned();
    // "Scratch that" deletes the sentence before it.
    while let Some(m) = SCRATCH_THAT.find(&t) {
        let before = t[..m.start()]
            .trim_end()
            .trim_end_matches(|c: char| ".!?。！？".contains(c));
        // Keep through the whole sentence-ending character (。 is 3 bytes).
        let keep = before
            .char_indices()
            .rfind(|(_, c)| ".!?。！？\n".contains(*c))
            .map_or("", |(i, c)| &before[..i + c.len_utf8()]);
        let gap = if keep.ends_with(|c: char| c.is_ascii()) {
            " "
        } else {
            ""
        };
        t = format!("{keep}{gap}{}", t[m.end()..].trim_start());
    }
    t
}

// --- Chinese -----------------------------------------------------------------

static ZH_FILLER: Lazy<Regex> =
    Lazy::new(|| re(r"(^|[，。！？；：、\n])\s*(?:嗯|呃|欸|誒|诶)+\s*[，、,]?\s*"));
/// 啊 is a filler only when it stands alone between commas; 好啊 is a word.
static ZH_FILLER_AH: Lazy<Regex> = Lazy::new(|| re(r"([，,])\s*啊+\s*[，,]"));
/// A short clause (1–4 characters) right before a comma.
static ZH_CLAUSE: Lazy<Regex> =
    Lazy::new(|| re(r"(?:^|[，。！？；：、,\n\s])(\p{Han}{1,4})[，,]\s*"));

fn chinese_fillers(text: &str) -> String {
    let t = ZH_FILLER.replace_all(text, "$1");
    ZH_FILLER_AH.replace_all(&t, "$1").into_owned()
}

fn chinese_stutters(text: &str) -> String {
    // One character said 3+ times: 我我我 → 我. Doubles are real words (謝謝,
    // 看看) and laughter is meant, so both stay.
    let chars: Vec<char> = text.chars().collect();
    let mut t = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let run = chars[i..].iter().take_while(|&&x| x == c).count();
        let stutter = run >= 3 && is_cjk(c) && !"哈呵嘻嘿".contains(c);
        t.extend(std::iter::repeat_n(c, if stutter { 1 } else { run }));
        i += run;
    }
    // A short clause repeated across a comma: 我們，我們去 → 我們去.
    while let Some((start, end)) = ZH_CLAUSE.captures_iter(&t).find_map(|c| {
        let (clause, all) = (c.get(1)?, c.get(0)?);
        t[all.end()..]
            .starts_with(clause.as_str())
            .then_some((clause.start(), all.end()))
    }) {
        t.replace_range(start..end, "");
    }
    t
}

/// 「好的,我們走?」 → 「好的，我們走？」: half-width punctuation touching Chinese.
fn full_width_punctuation(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut t = String::new();
    let mut skip_spaces = false;
    for (i, &c) in chars.iter().enumerate() {
        if skip_spaces && c == ' ' {
            continue;
        }
        skip_spaces = false;
        let Some(wide) = ",?!:;".find(c).map(|k| ["，", "？", "！", "：", "；"][k]) else {
            t.push(c);
            continue;
        };
        let prev = chars[..i].iter().rev().find(|c| **c != ' ');
        let next = chars[i + 1..].iter().find(|c| **c != ' ');
        if prev.is_some_and(|c| is_cjk(*c)) || next.is_some_and(|c| is_cjk(*c)) {
            t.truncate(t.trim_end_matches(' ').len());
            t.push_str(wide);
            skip_spaces = true;
        } else {
            t.push(c);
        }
    }
    t
}

/// 「用Claude Code寫」 → 「用 Claude Code 寫」. Numbers stay as they are.
fn space_chinese_english(text: &str) -> String {
    let mut t = String::new();
    let mut prev: Option<char> = None;
    for c in text.chars() {
        if let Some(p) = prev {
            if (is_cjk(p) && c.is_ascii_alphabetic()) || (p.is_ascii_alphabetic() && is_cjk(c)) {
                t.push(' ');
            }
        }
        t.push(c);
        prev = Some(c);
    }
    t
}

// --- English -----------------------------------------------------------------

static EN_FILLER: Lazy<Regex> = Lazy::new(|| re(r"(?i)\b(?:er|erm)\b,?\s*"));
static MM: Lazy<Regex> = Lazy::new(|| re(r"(?i)\bmm\b,?\s*"));
static YOU_KNOW_MIDDLE: Lazy<Regex> = Lazy::new(|| re(r"(?i),\s*you know\s*,\s*"));
static YOU_KNOW_END: Lazy<Regex> = Lazy::new(|| re(r"(?i),\s*you know\s*([.!?]|$)"));
static YOU_KNOW_START: Lazy<Regex> = Lazy::new(|| re(r"(?i)^\s*you know\s*,\s*"));
static WORD: Lazy<Regex> = Lazy::new(|| re(r"[A-Za-z']+"));
/// Doubles people say on purpose.
const KEPT_DOUBLES: &[&str] = &[
    "that", "had", "is", "very", "really", "long", "bye", "no", "ha",
];

fn english_fillers(text: &str) -> String {
    let t = EN_FILLER.replace_all(text, "");
    // "mm" is a filler unless it follows a number ("5 mm").
    let mut out = String::new();
    let mut last = 0;
    for m in MM.find_iter(&t) {
        let after_number = t[..m.start()]
            .trim_end()
            .ends_with(|c: char| c.is_ascii_digit());
        if !after_number {
            out.push_str(&t[last..m.start()]);
            last = m.end();
        }
    }
    out.push_str(&t[last..]);
    let t = YOU_KNOW_MIDDLE.replace_all(&out, " ");
    let t = YOU_KNOW_END.replace_all(&t, "$1");
    YOU_KNOW_START.replace_all(&t, "").into_owned()
}

/// "I I think" → "I think", keeping emphatic doubles like "very very".
fn english_doubles(text: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    let mut prev: Option<regex::Match> = None;
    for m in WORD.find_iter(text) {
        let doubled = prev.is_some_and(|p| {
            text[p.end()..m.start()].chars().all(|c| c == ' ')
                && p.as_str().eq_ignore_ascii_case(m.as_str())
                && !KEPT_DOUBLES.contains(&m.as_str().to_lowercase().as_str())
        });
        if doubled {
            out.push_str(&text[last..prev.map_or(0, |p| p.end())]);
            last = m.end();
        } else {
            prev = Some(m);
        }
    }
    out.push_str(&text[last..]);
    out
}

static TOKEN: Lazy<Regex> = Lazy::new(|| re(r"[\p{L}\p{M}\p{N}_'’-]+"));
const REPAIR_CUES: &[&[&str]] = &[
    &["sorry"],
    &["no"],
    &["wait"],
    &["oops"],
    &["actually"],
    &["i", "mean"],
];
const SUBJECT_PRONOUNS: &[&str] = &["i", "we", "you", "he", "she", "it", "they"];

/// "fly to Pune, sorry, no, to Delhi" → "fly to Delhi": only when the
/// correction restarts with a word said in the last 4 words. Anything else
/// is left for the AI.
fn self_corrections(text: &str) -> String {
    let matches: Vec<_> = TOKEN.find_iter(text).collect();
    let Some(first) = matches.first() else {
        return text.to_string();
    };
    let lead = &text[..first.start()];
    let mut toks: Vec<(String, String)> = matches
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let end = matches.get(i + 1).map_or(text.len(), |n| n.start());
            (m.as_str().to_string(), text[m.end()..end].to_string())
        })
        .collect();
    let ends_sentence = |s: &str| s.contains(['.', '?', '!', '…']);
    let lower = |s: &str| s.to_lowercase();

    let mut i = 1;
    while i < toks.len() {
        let onset = toks[i - 1]
            .1
            .contains(',')
            .then(|| repair_onset(&toks, i))
            .flatten();
        let window = i.saturating_sub(4)..i;
        let sentence_start = window
            .clone()
            .rev()
            .find(|&j| ends_sentence(&toks[j].1))
            .map_or(window.start, |j| j + 1);
        let anchor = onset.and_then(|o| {
            (sentence_start..i).rev().find(|&j| {
                lower(&toks[j].0) == lower(&toks[o].0)
                    && !SUBJECT_PRONOUNS
                        .contains(&lower(&toks[j].0).split(['\'', '’']).next().unwrap_or(""))
            })
        });
        match (onset, anchor) {
            (Some(o), Some(a)) => {
                toks[a].1 = toks[o].1.clone();
                toks.drain(a + 1..=o);
                i = a + 1;
            }
            _ => i += 1,
        }
    }
    lead.to_string()
        + &toks
            .iter()
            .map(|(w, s)| format!("{w}{s}"))
            .collect::<String>()
}

/// The token after the repair cues ("sorry, no,"), if they correct something.
fn repair_onset(toks: &[(String, String)], start: usize) -> Option<usize> {
    let mut end = start;
    let mut cues: Vec<&[&str]> = Vec::new();
    while let Some(cue) = REPAIR_CUES.iter().find(|cue| {
        end + cue.len() <= toks.len()
            && cue
                .iter()
                .enumerate()
                .all(|(k, w)| toks[end + k].0.to_lowercase() == *w)
    }) {
        cues.push(cue);
        end += cue.len();
    }
    let corrects = cues.iter().any(|c| *c == ["no"] || *c == ["i", "mean"]);
    let set_off = cues.len() > 1 || end > start && toks[end - 1].1.contains(',');
    let in_one_sentence = toks[start..end]
        .iter()
        .all(|(_, s)| !s.contains(['.', '?', '!', '…']));
    (end > start && end < toks.len() && corrects && set_off && in_one_sentence).then_some(end)
}

// --- Tidy ----------------------------------------------------------------------

static SPACES_AROUND_NEWLINE: Lazy<Regex> = Lazy::new(|| re(r"[ \t]*\n[ \t]*"));
static MULTI_SPACE: Lazy<Regex> = Lazy::new(|| re(r"[ \t]{2,}"));
static SPACE_BEFORE_PUNCT: Lazy<Regex> = Lazy::new(|| re(r"[ \t]+([,.!?;:])"));

fn tidy(text: &str, original: &str) -> String {
    let t = SPACES_AROUND_NEWLINE.replace_all(text, "\n");
    let t = MULTI_SPACE.replace_all(&t, " ");
    let t = SPACE_BEFORE_PUNCT.replace_all(&t, "$1");
    let mut t = t
        .trim_start_matches(|c: char| c.is_whitespace() || "，,、".contains(c))
        .trim_end_matches(|c: char| c.is_whitespace() || "，,、".contains(c))
        .to_string();
    // Keep a capital the speaker started with after removing a leading filler.
    let started_capital = original
        .trim_start()
        .starts_with(|c: char| c.is_ascii_uppercase());
    if started_capital && t.starts_with(|c: char| c.is_ascii_lowercase()) {
        t.replace_range(0..1, &t[0..1].to_uppercase());
    }
    t
}

#[cfg(test)]
mod tests {
    use super::apply;

    fn light(text: &str) -> String {
        apply(text, false)
    }

    fn voice(text: &str) -> String {
        apply(text, true)
    }

    // --- Chinese -----------------------------------------------------------

    #[test]
    fn chinese_fillers() {
        assert_eq!(light("嗯，我覺得可以"), "我覺得可以");
        assert_eq!(light("我覺得，呃，可以"), "我覺得，可以");
        assert_eq!(light("欸 我們走吧"), "我們走吧");
        assert_eq!(light("我覺得，啊，可以"), "我覺得，可以");
        assert_eq!(light("好啊"), "好啊");
        assert_eq!(light("那個就是然後"), "那個就是然後");
    }

    #[test]
    fn chinese_stutters() {
        assert_eq!(light("我我我覺得"), "我覺得");
        assert_eq!(light("我們，我們去"), "我們去");
        assert_eq!(light("謝謝你，看看天天媽媽"), "謝謝你，看看天天媽媽");
        assert_eq!(light("哈哈哈好好笑"), "哈哈哈好好笑");
    }

    #[test]
    fn space_between_chinese_and_english() {
        assert_eq!(light("用Claude Code寫"), "用 Claude Code 寫");
        assert_eq!(light("我用Claude，很好"), "我用 Claude，很好");
        assert_eq!(light("買了3個"), "買了3個");
    }

    #[test]
    fn full_width_punctuation_next_to_chinese() {
        assert_eq!(light("好的,我們走?"), "好的，我們走？");
        assert_eq!(light("好的, 我們走"), "好的，我們走");
        assert_eq!(
            light("version 3.5 of Node.js, a, b"),
            "version 3.5 of Node.js, a, b"
        );
    }

    // --- English -----------------------------------------------------------

    #[test]
    fn english_double_words() {
        assert_eq!(
            light("I I think the the plan works"),
            "I think the plan works"
        );
        assert_eq!(
            light("it was a very very long day"),
            "it was a very very long day"
        );
        assert_eq!(light("no no, bye bye"), "no no, bye bye");
    }

    #[test]
    fn english_fillers() {
        assert_eq!(light("er, I think so"), "I think so");
        assert_eq!(light("I think, erm, we should go"), "I think, we should go");
        assert_eq!(light("mm let me check"), "let me check");
        assert_eq!(light("a 5 mm screw"), "a 5 mm screw");
        assert_eq!(light("It was, you know, fine"), "It was fine");
        assert_eq!(light("Do you know the way?"), "Do you know the way?");
        assert_eq!(light("I like it"), "I like it");
    }

    #[test]
    fn simple_self_corrections() {
        assert_eq!(light("fly to Pune, sorry, no, to Delhi"), "fly to Delhi");
        assert_eq!(
            light("Let's meet Tuesday, sorry, I mean Wednesday"),
            "Let's meet Tuesday, sorry, I mean Wednesday"
        );
        assert_eq!(light("No, I disagree"), "No, I disagree");
    }

    // --- Silence phrases ---------------------------------------------------

    #[test]
    fn silence_phrases_are_dropped() {
        assert_eq!(light("Thanks for watching!"), "");
        assert_eq!(light("Thank you for watching."), "");
        assert_eq!(light("字幕由 Amara.org 社群提供"), "");
        assert_eq!(light("請不吝點讚 訂閱 轉發 打賞支持明鏡與點點欄目"), "");
        assert_eq!(light("謝謝觀看"), "");
    }

    #[test]
    fn real_sentences_with_those_words_are_kept() {
        assert_eq!(light("Thank you."), "Thank you.");
        assert_eq!(
            light("Thanks for watching the kids today"),
            "Thanks for watching the kids today"
        );
    }

    // --- Voice commands (opt-in) ------------------------------------------

    #[test]
    fn voice_commands_are_off_by_default() {
        assert_eq!(
            light("first item new line second item"),
            "first item new line second item"
        );
    }

    #[test]
    fn voice_command_line_breaks() {
        assert_eq!(
            voice("first item new line second item"),
            "first item\nsecond item"
        );
        assert_eq!(voice("Hello. New paragraph. Thanks"), "Hello.\n\nThanks");
        assert_eq!(voice("第一點換行第二點"), "第一點\n第二點");
        assert_eq!(voice("第一段新段落第二段"), "第一段\n\n第二段");
    }

    #[test]
    fn voice_command_punctuation() {
        assert_eq!(voice("are you coming question mark"), "are you coming?");
        assert_eq!(voice("wait comma what"), "wait, what");
        assert_eq!(voice("great exclamation mark"), "great!");
        assert_eq!(voice("done full stop"), "done.");
        assert_eq!(voice("你好逗號我是歐文句號"), "你好，我是歐文。");
        assert_eq!(voice("你要來嗎問號"), "你要來嗎？");
    }

    #[test]
    fn voice_command_scratch_that() {
        assert_eq!(
            voice("Send the report. Call Anna tomorrow. Scratch that. See you"),
            "Send the report. See you"
        );
        assert_eq!(voice("我們明天開會。刪掉上一句。後天見"), "後天見");
        assert_eq!(
            voice("第一句。第二句。刪掉上一句。第三句"),
            "第一句。第三句"
        );
    }
}
