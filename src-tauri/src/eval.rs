//! Owl dictation tests: replay recorded dictations through Owl and score the
//! result. One runner for every source (Owl's learning log, the owner's
//! Wispr Flow history, public recordings), used by `--eval` on the command
//! line and by the learner (`learner.rs`) to judge a change before keeping it.
//!
//! A test set is a folder with `set.jsonl` (one `Item` per line) and the
//! audio it names. Scores are script-agnostic for Chinese (Traditional and
//! Simplified count as the same text) and count numbers either way ("twenty"
//! and 20), so they measure hearing, not formatting.

use crate::actions::{process_with, CleanupRun};
use crate::chinese_script::{convert_chinese_script, ChineseVariety};
use crate::correction::{is_cjk, tokens};
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{AppSettings, ChineseScript};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Manager};

/// One recorded dictation and its answer keys.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    /// "speech", or "empty" for a recording with nothing said in it.
    #[serde(default = "speech")]
    pub kind: String,
    /// Audio path, relative to the set folder.
    pub wav: String,
    /// What was said, word for word, without cleanup.
    #[serde(default)]
    pub hearing: Option<String>,
    /// The text the user kept.
    #[serde(default, rename = "final")]
    pub kept: Option<String>,
    /// "owl", "wispr" or "public:<name>".
    #[serde(default)]
    pub source: Option<String>,
    /// Unix seconds, for splitting by date and for picking examples.
    #[serde(default)]
    pub created_at: Option<i64>,
    /// Wispr sets: the date as text.
    #[serde(default)]
    pub when: Option<String>,
}

fn speech() -> String {
    "speech".to_string()
}

impl Item {
    fn source(&self) -> &str {
        self.source.as_deref().unwrap_or("wispr")
    }

    /// The reference for hearing: what was said, else what the user kept.
    fn heard_key(&self) -> Option<&str> {
        self.hearing.as_deref().or(self.kept.as_deref())
    }

    fn order(&self) -> (i64, &str) {
        (
            self.created_at.unwrap_or(0),
            self.when.as_deref().unwrap_or(""),
        )
    }
}

pub fn load_set(dir: &Path) -> Result<Vec<Item>> {
    let text = std::fs::read_to_string(dir.join("set.jsonl"))
        .with_context(|| format!("no set.jsonl in {}", dir.display()))?;
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).context("bad line in set.jsonl"))
        .collect()
}

/// Oldest 80% to learn from, newest 20% to judge with, so a change is never
/// judged on the dictations it was learned from.
pub fn split(mut items: Vec<Item>) -> (Vec<Item>, Vec<Item>) {
    items.sort_by(|a, b| a.order().cmp(&b.order()));
    let judge = if items.len() >= 2 {
        (items.len() / 5).max(1)
    } else {
        0
    };
    let study = items.len() - judge;
    let judged = items.split_off(study);
    (items, judged)
}

// --- Scoring ------------------------------------------------------------

/// Words and Chinese characters, numbers as digits, Chinese as Simplified,
/// lowercase: what two texts are compared on.
pub fn units(text: &str) -> Vec<String> {
    let text = crate::numbers::format_numbers(text);
    let text = convert_chinese_script(&text, ChineseVariety::Mandarin, ChineseScript::Simplified);
    tokens(&text)
        .into_iter()
        .map(|t| t.to_lowercase())
        .collect()
}

/// Edits to turn `hyp` into `reference`, and the reference's length.
pub fn edits(hyp: &str, reference: &str) -> (usize, usize) {
    let (h, r) = (units(hyp), units(reference));
    let mut row: Vec<usize> = (0..=r.len()).collect();
    for (i, hw) in h.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, rw) in r.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = if hw == rw {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j + 1])
            };
            prev = cur;
        }
    }
    (row[r.len()], r.len())
}

/// Chinese characters in `text`, and how many of them only exist in
/// Simplified (wrong when the user writes Traditional).
fn simplified_only(text: &str) -> (usize, usize) {
    let mut total = 0;
    let mut simplified = 0;
    for c in text.chars().filter(|c| is_cjk(*c)) {
        total += 1;
        let one = c.to_string();
        if convert_chinese_script(&one, ChineseVariety::Mandarin, ChineseScript::Traditional) != one
        {
            simplified += 1;
        }
    }
    (simplified, total)
}

fn has_term(text: &str, term: &str) -> bool {
    let (text, term) = (text.to_lowercase(), term.to_lowercase());
    if term.chars().any(is_cjk) {
        return text.contains(&term);
    }
    text.match_indices(&term).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + term.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric()) && !after.is_some_and(|c| c.is_alphanumeric())
    })
}

/// The numbers written in `text`: "20%" and "v2.5" give "20" and "2.5".
fn numbers(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars().chain([' ']) {
        if c.is_ascii_digit() || (c == '.' && !cur.is_empty()) {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(cur.trim_end_matches('.').to_string());
            cur.clear();
        }
    }
    out
}

/// One dictation's result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemResult {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub heard: String,
    pub output: String,
    pub ai_rejected: Option<String>,
    /// Edits from what the speech model heard to what was said, and its length.
    pub hearing_edits: (usize, usize),
    /// Edits from what Owl pasted to what the user kept, and its length.
    pub final_edits: (usize, usize),
    pub terms_heard: usize,
    pub terms_missed: usize,
    pub terms_inserted: usize,
    pub numbers_said: usize,
    pub numbers_missed: usize,
    pub simplified_chars: (usize, usize),
    pub ms: u64,
}

/// Totals over a run. Lower error rates are better.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Summary {
    pub items: usize,
    /// Hearing mistakes per word said (WER/CER/MER).
    pub hearing_error: Option<f64>,
    /// Edits still needed per word kept: what the user would have to fix.
    pub final_error: Option<f64>,
    /// Share of the user's Words that were said and came out right.
    pub term_recall: Option<f64>,
    /// Words put in that weren't said.
    pub terms_inserted: usize,
    /// Share of numbers in the keys that came out as the same digits.
    pub numbers_right: Option<f64>,
    pub ai_rejected: usize,
    /// Text made up on recordings with nothing said.
    pub silence_made_up: usize,
    /// Share of Chinese characters in Simplified when the user writes Traditional.
    pub simplified_share: Option<f64>,
    pub avg_ms: f64,
}

fn rate(num: usize, den: usize) -> Option<f64> {
    (den > 0).then(|| (num as f64 / den as f64 * 10_000.0).round() / 10_000.0)
}

pub fn summarize(results: &[ItemResult], traditional: bool) -> Summary {
    let sum = |f: &dyn Fn(&ItemResult) -> usize| results.iter().map(f).sum::<usize>();
    let speech: Vec<&ItemResult> = results.iter().filter(|r| r.kind != "empty").collect();
    let ssum = |f: &dyn Fn(&&ItemResult) -> usize| speech.iter().map(f).sum::<usize>();
    let terms = ssum(&|r| r.terms_heard + r.terms_missed);
    let simplified = ssum(&|r| r.simplified_chars.0);
    let cjk = ssum(&|r| r.simplified_chars.1);
    Summary {
        items: results.len(),
        hearing_error: rate(ssum(&|r| r.hearing_edits.0), ssum(&|r| r.hearing_edits.1)),
        final_error: rate(ssum(&|r| r.final_edits.0), ssum(&|r| r.final_edits.1)),
        term_recall: rate(ssum(&|r| r.terms_heard), terms),
        terms_inserted: sum(&|r| r.terms_inserted),
        numbers_right: rate(
            ssum(&|r| r.numbers_said - r.numbers_missed),
            ssum(&|r| r.numbers_said),
        ),
        ai_rejected: sum(&|r| usize::from(r.ai_rejected.is_some())),
        silence_made_up: results
            .iter()
            .filter(|r| r.kind == "empty" && !r.output.trim().is_empty())
            .count(),
        simplified_share: rate(simplified, cjk).filter(|_| traditional),
        avg_ms: if results.is_empty() {
            0.0
        } else {
            (results.iter().map(|r| r.ms).sum::<u64>() as f64 / results.len() as f64).round()
        },
    }
}

/// Scores one dictation's output against its keys.
pub fn score(
    item: &Item,
    heard: String,
    output: String,
    ai_rejected: Option<&str>,
    words: &[String],
    ms: u64,
) -> ItemResult {
    let speech = item.kind != "empty";
    let hearing_edits = match (speech, item.heard_key()) {
        (true, Some(key)) => edits(&heard, key),
        (false, _) => (units(&heard).len(), 0),
        _ => (0, 0),
    };
    let final_edits = match (speech, item.kept.as_deref()) {
        (true, Some(key)) => edits(&output, key),
        _ => (0, 0),
    };
    let key = item
        .kept
        .as_deref()
        .or(item.hearing.as_deref())
        .unwrap_or("");
    let (mut terms_heard, mut terms_missed, mut terms_inserted) = (0, 0, 0);
    for w in words {
        match (has_term(key, w), has_term(&output, w)) {
            (true, true) => terms_heard += 1,
            (true, false) => terms_missed += 1,
            (false, true) if speech && !key.is_empty() => terms_inserted += 1,
            (false, true) if !speech => terms_inserted += 1,
            _ => {}
        }
    }
    // Keys spelled out ("twenty percent", 百分之二十) count as numbers too.
    let digits = crate::numbers::format_numbers;
    let (said, have) = (numbers(&digits(key)), numbers(&digits(&output)));
    ItemResult {
        id: item.id.clone(),
        source: item.source().to_string(),
        kind: item.kind.clone(),
        heard,
        numbers_said: said.len(),
        numbers_missed: said.iter().filter(|n| !have.contains(n)).count(),
        simplified_chars: simplified_only(&output),
        output,
        ai_rejected: ai_rejected.map(str::to_string),
        hearing_edits,
        final_edits,
        terms_heard,
        terms_missed,
        terms_inserted,
        ms,
    }
}

// --- AI reply cache -----------------------------------------------------

/// Saved AI cleanup replies, keyed by everything that shapes the reply, so
/// replaying a set only pays for calls whose input changed.
pub struct AiCache {
    path: Option<PathBuf>,
    map: Mutex<HashMap<String, String>>,
}

impl AiCache {
    pub fn open(path: PathBuf) -> Self {
        let map = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            map: Mutex::new(map),
        }
    }

    pub fn key(settings: &AppSettings, text: &str, examples: &[(String, String)]) -> String {
        let provider = settings.post_process_provider_id.as_str();
        let prompt = settings
            .post_process_selected_prompt_id
            .as_ref()
            .and_then(|id| settings.post_process_prompts.iter().find(|p| &p.id == id))
            .map(|p| p.prompt.as_str());
        // Where the request goes and how its reply is read, so a different
        // server (a custom endpoint moved) isn't graded on old replies.
        let endpoint = settings
            .active_post_process_provider()
            .map(|p| (p.base_url.as_str(), p.supports_structured_output));
        let input = serde_json::json!([
            provider,
            endpoint,
            settings.post_process_models.get(provider),
            prompt,
            format!("{:?}", settings.cleanup_level),
            // The spelling rule in the prompt lists Words and replacement targets.
            settings.custom_words,
            settings.replacements,
            examples,
            text,
        ]);
        format!("{:x}", Sha256::digest(input.to_string().as_bytes()))
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.map.lock().ok()?.get(key).cloned()
    }

    pub fn put(&self, key: &str, reply: &str) {
        if let Ok(mut map) = self.map.lock() {
            map.insert(key.to_string(), reply.to_string());
        }
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let map = self.map.lock().map_err(|_| anyhow::anyhow!("cache lock"))?;
        std::fs::write(path, serde_json::to_vec(&*map)?)?;
        Ok(())
    }
}

// --- Runner -------------------------------------------------------------

/// `replay` stopped because `stop` said so (the user started dictating).
#[derive(Debug)]
pub struct Interrupted;

impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stopped for a dictation")
    }
}

impl std::error::Error for Interrupted {}

/// Replays `items` (audio under `dir`) through Owl with `settings`. Blocks;
/// call it off the async runtime. The model must be loaded. Stops before
/// the next item once `stop` is true.
#[allow(clippy::too_many_arguments)]
pub fn replay(
    app: &AppHandle,
    settings: &AppSettings,
    items: &[Item],
    dir: &Path,
    post_process: bool,
    cache: &AiCache,
    stop: &dyn Fn() -> bool,
    mut progress: impl FnMut(usize, &ItemResult),
) -> Result<Vec<ItemResult>> {
    let tm = app.state::<Arc<TranscriptionManager>>();
    let model = tm.get_current_model().context("no model is loaded")?;
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        if stop() {
            return Err(Interrupted.into());
        }
        // An "unload immediately" setting drops the model after each run.
        if !tm.is_model_loaded() {
            tm.load_model(&model)?;
        }
        let wav = dir.join(&item.wav);
        let samples = crate::audio_toolkit::read_wav_samples(&wav)
            .with_context(|| format!("can't read {}", wav.display()))?;
        let started = Instant::now();
        let heard = tm.transcribe_with(samples, settings)?;
        let processed = tauri::async_runtime::block_on(process_with(
            app,
            settings,
            &heard,
            post_process,
            CleanupRun {
                cache: Some(cache),
                examples_before: Some(item.created_at.unwrap_or(0)),
            },
        ));
        let ms = started.elapsed().as_millis() as u64;
        let result = score(
            item,
            heard,
            processed.final_text,
            processed.ai_rejected,
            &settings.custom_words,
            ms,
        );
        progress(i, &result);
        out.push(result);
    }
    Ok(out)
}

/// Writes the learning log's keyed dictations as a test set in `out`.
/// Returns how many.
pub fn export_learning(learning: &crate::learning::Learning, out: &Path) -> Result<usize> {
    std::fs::create_dir_all(out.join("clips"))?;
    let mut lines = String::new();
    let rows = learning.keyed()?;
    for d in &rows {
        let name = d
            .audio
            .file_name()
            .and_then(|n| n.to_str())
            .context("audio without a name")?;
        std::fs::copy(&d.audio, out.join("clips").join(name))?;
        let item = Item {
            id: format!("owl-{}", d.id),
            kind: speech(),
            wav: format!("clips/{name}"),
            hearing: None,
            kept: d.kept.clone(),
            source: Some("owl".to_string()),
            created_at: Some(d.created_at),
            when: None,
        };
        lines.push_str(&serde_json::to_string(&item)?);
        lines.push('\n');
    }
    std::fs::write(out.join("set.jsonl"), lines)?;
    Ok(rows.len())
}

/// The learning log's keyed dictations as test items, audio paths absolute.
pub fn learning_items(learning: &crate::learning::Learning) -> Result<Vec<Item>> {
    Ok(learning
        .keyed()?
        .into_iter()
        .map(|d| Item {
            id: format!("owl-{}", d.id),
            kind: speech(),
            wav: d.audio.to_string_lossy().into_owned(),
            hearing: None,
            kept: d.kept,
            source: Some("owl".to_string()),
            created_at: Some(d.created_at),
            when: None,
        })
        .collect())
}

/// Summaries for the whole run and for each source.
pub fn report(results: &[ItemResult], traditional: bool) -> serde_json::Value {
    let mut by_source: std::collections::BTreeMap<&str, Vec<ItemResult>> = Default::default();
    for r in results {
        by_source.entry(&r.source).or_default().push(r.clone());
    }
    serde_json::json!({
        "all": summarize(results, traditional),
        "by_source": by_source
            .iter()
            .map(|(s, rs)| (s.to_string(), serde_json::to_value(summarize(rs, traditional)).unwrap_or_default()))
            .collect::<serde_json::Map<_, _>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kept: &str) -> Item {
        Item {
            id: "1".into(),
            kind: "speech".into(),
            wav: "a.wav".into(),
            hearing: None,
            kept: Some(kept.into()),
            source: Some("owl".into()),
            created_at: Some(1),
            when: None,
        }
    }

    #[test]
    fn scores_ignore_case_script_punctuation_and_number_form() {
        assert_eq!(edits("Ship it, twenty percent!", "ship it 20%"), (0, 3));
        assert_eq!(edits("我們開會", "我们开会"), (0, 4));
        assert_eq!(edits("send it to O", "Send it to Owl."), (1, 4));
        // Mixed English and Chinese count words and characters together.
        assert_eq!(edits("用 Supabase 存資料", "用 Supabase 存资料"), (0, 5));
    }

    #[test]
    fn one_result_counts_terms_numbers_and_script() {
        let words = vec!["Owl".to_string(), "Supabase".to_string()];
        let r = score(
            &item("Owl 2.5 is 20% faster than Supabase"),
            "o two point five".into(),
            "O 2.5 is 20% faster than Supabase 们".into(),
            None,
            &words,
            5,
        );
        assert_eq!((r.terms_heard, r.terms_missed, r.terms_inserted), (1, 1, 0));
        assert_eq!((r.numbers_said, r.numbers_missed), (2, 0));
        assert_eq!(r.simplified_chars, (1, 1));
        let s = summarize(&[r], true);
        assert_eq!(s.term_recall, Some(0.5));
        assert_eq!(s.numbers_right, Some(1.0));
        assert_eq!(s.simplified_share, Some(1.0));
        // A key spelled out still counts its numbers.
        let r = score(
            &item("about twenty percent"),
            String::new(),
            "about 30%".into(),
            None,
            &[],
            1,
        );
        assert_eq!((r.numbers_said, r.numbers_missed), (1, 1));
    }

    #[test]
    fn silence_that_produced_text_is_counted() {
        let mut empty = item("");
        empty.kind = "empty".into();
        empty.kept = None;
        let r = score(&empty, "请订阅".into(), "请订阅".into(), None, &[], 1);
        assert_eq!(summarize(&[r], false).silence_made_up, 1);
    }

    #[test]
    fn the_newest_fifth_judges() {
        let items: Vec<Item> = (0..10)
            .map(|i| Item {
                created_at: Some(10 - i),
                id: i.to_string(),
                ..item("x")
            })
            .collect();
        let (study, judge) = split(items);
        assert_eq!((study.len(), judge.len()), (8, 2));
        assert!(judge.iter().all(|j| j.created_at >= Some(9)));
    }
}
