//! Owl learner: improves Owl for its user from the learning log, without
//! retraining the speech model. It proposes changes to what Owl can tune
//! (hint words and their order, a context passage for the speech model,
//! word swaps, examples for the AI cleanup), replays the newest corrected
//! dictations with each change (`eval.rs`), and keeps only the changes that
//! leave less for the user to fix and make nothing else worse. Every kept
//! change is listed with an Undo on the Learning page.

use crate::eval::{self, AiCache, Summary};
use crate::learning::Learning;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, write_settings, AppSettings, Replacement};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Corrected dictations needed before learning starts.
pub const MIN_KEYED: usize = 10;
/// At most this many of the newest dictations judge each change.
const MAX_JUDGE: usize = 40;
/// A fix seen this many times becomes a candidate.
const MIN_SEEN: usize = 2;
/// Changes without direct evidence must improve the score by this much.
const MIN_GAIN: f64 = 0.002;

static RUNNING: AtomicBool = AtomicBool::new(false);

/// One change the learner can make, with what it replaced, so it can be undone.
#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    AddWord {
        word: String,
    },
    AddSwap {
        from: String,
        to: String,
    },
    RemoveSwap {
        from: String,
        to: String,
    },
    Priority {
        before: Vec<String>,
        after: Vec<String>,
    },
    Context {
        before: String,
        after: String,
    },
    WordLimit {
        before: Option<u32>,
        after: Option<u32>,
    },
    Examples {
        before: bool,
        after: bool,
    },
}

impl Change {
    pub fn apply(&self, s: &mut AppSettings) {
        match self {
            Change::AddWord { word } => {
                if !s.custom_words.iter().any(|w| w.eq_ignore_ascii_case(word)) {
                    s.custom_words.push(word.clone());
                }
            }
            Change::AddSwap { from, to } => {
                if !s.replacements.iter().any(|r| &r.from == from) {
                    s.replacements.push(Replacement {
                        from: from.clone(),
                        to: to.clone(),
                    });
                }
            }
            Change::RemoveSwap { from, to } => {
                s.replacements.retain(|r| !(&r.from == from && &r.to == to))
            }
            Change::Priority { after, .. } => s.hearing_priority = after.clone(),
            Change::Context { after, .. } => s.hearing_context = after.clone(),
            Change::WordLimit { after, .. } => s.hearing_word_limit = after.map(|n| n as usize),
            Change::Examples { after, .. } => s.cleanup_examples = *after,
        }
    }

    pub fn undo(&self, s: &mut AppSettings) {
        match self {
            Change::AddWord { word } => s.custom_words.retain(|w| w != word),
            Change::AddSwap { from, to } => {
                s.replacements.retain(|r| !(&r.from == from && &r.to == to))
            }
            Change::RemoveSwap { from, to } => Change::AddSwap {
                from: from.clone(),
                to: to.clone(),
            }
            .apply(s),
            Change::Priority { before, .. } => s.hearing_priority = before.clone(),
            Change::Context { before, .. } => s.hearing_context = before.clone(),
            Change::WordLimit { before, .. } => s.hearing_word_limit = before.map(|n| n as usize),
            Change::Examples { before, .. } => s.cleanup_examples = *before,
        }
    }

    /// What the change touches: one value, one Word or one replacement. Only
    /// the newest change to a thing can be undone, since undoing an older one
    /// would overwrite the newer (or be a no-op the newer then contradicts).
    pub fn target(&self) -> String {
        match self {
            Change::AddWord { word } => format!("word:{}", word.to_lowercase()),
            Change::AddSwap { from, .. } | Change::RemoveSwap { from, .. } => {
                format!("swap:{from}")
            }
            Change::Priority { .. } => "priority".into(),
            Change::Context { .. } => "context".into(),
            Change::WordLimit { .. } => "word_limit".into(),
            Change::Examples { .. } => "examples".into(),
        }
    }

    /// Whether `s` still holds the value this change replaces. List changes
    /// (Words, swaps) always apply.
    fn still_before(&self, s: &AppSettings) -> bool {
        match self {
            Change::Priority { before, .. } => &s.hearing_priority == before,
            Change::Context { before, .. } => &s.hearing_context == before,
            Change::WordLimit { before, .. } => s.hearing_word_limit.map(|n| n as u32) == *before,
            Change::Examples { before, .. } => s.cleanup_examples == *before,
            _ => true,
        }
    }

    /// The change with its `before` taken from `current`.
    fn rebased(self, current: &AppSettings) -> Change {
        match self {
            Change::Priority { after, .. } => Change::Priority {
                before: current.hearing_priority.clone(),
                after,
            },
            Change::Context { after, .. } => Change::Context {
                before: current.hearing_context.clone(),
                after,
            },
            Change::WordLimit { after, .. } => Change::WordLimit {
                before: current.hearing_word_limit.map(|n| n as u32),
                after,
            },
            Change::Examples { after, .. } => Change::Examples {
                before: current.cleanup_examples,
                after,
            },
            other => other,
        }
    }

    /// Whether the change rests on fixes the user made, so it may be kept
    /// when it doesn't change the score (the judged dictations may not
    /// contain the word). The rest must improve it.
    fn has_evidence(&self) -> bool {
        matches!(
            self,
            Change::AddWord { .. }
                | Change::AddSwap { .. }
                | Change::RemoveSwap { .. }
                | Change::Priority { .. }
        )
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type, PartialEq)]
pub struct LearnReport {
    /// "learned", "nothing_new", "interrupted" (a dictation started; the rest waits), "need_more", "off" or "busy".
    pub outcome: String,
    pub keyed: u32,
    pub judged: u32,
    /// Edits still needed per word, before and after (lower is better).
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub kept: Vec<Change>,
    pub tried: u32,
}

/// Runs the learner once. Blocks for as long as replaying takes; call it
/// off the async runtime.
pub fn learn(app: &AppHandle) -> Result<LearnReport> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(LearnReport {
            outcome: "busy".into(),
            ..Default::default()
        });
    }
    struct Done;
    impl Drop for Done {
        fn drop(&mut self) {
            RUNNING.store(false, Ordering::SeqCst);
        }
    }
    let _done = Done;
    // Fixes made while this run works count toward the next one.
    let started = chrono::Utc::now().timestamp();
    let report = learn_once(app);
    // A run a dictation interrupted didn't judge anything; don't record it,
    // so the next check tries again.
    let finished = report
        .as_ref()
        .is_ok_and(|r| !matches!(r.outcome.as_str(), "busy" | "interrupted"));
    if let (true, Ok(report), Some(learning)) =
        (finished, &report, app.try_state::<Arc<Learning>>())
    {
        let saved = learning.add_run(
            started,
            report.keyed as i64,
            &serde_json::to_string(report)?,
        );
        if let Err(e) = saved {
            log::warn!("Learner: couldn't save the run: {e:#}");
        }
    }
    let _ = app.emit("learning-changed", ());
    report
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

fn learn_once(app: &AppHandle) -> Result<LearnReport> {
    let settings = get_settings(app);
    if !settings.learning_enabled {
        return Ok(LearnReport {
            outcome: "off".into(),
            ..Default::default()
        });
    }
    let learning = app
        .try_state::<Arc<Learning>>()
        .ok_or_else(|| anyhow!("the learning log is unavailable"))?;
    let items = eval::learning_items(&learning)?;
    let keyed = items.len() as u32;
    if items.len() < MIN_KEYED {
        return Ok(LearnReport {
            outcome: "need_more".into(),
            keyed,
            ..Default::default()
        });
    }
    let dictations: HashMap<String, crate::learning::Dictation> = learning
        .keyed()?
        .into_iter()
        .map(|d| (format!("owl-{}", d.id), d))
        .collect();
    let (study, mut judge) = eval::split(items);
    if judge.len() > MAX_JUDGE {
        judge = judge.split_off(judge.len() - MAX_JUDGE);
    }
    let study: Vec<&crate::learning::Dictation> =
        study.iter().filter_map(|i| dictations.get(&i.id)).collect();

    let tm = app.state::<Arc<TranscriptionManager>>();
    let post_process = crate::actions::ai_cleanup_on(&settings);
    let cache = AiCache::open(learning.file("ai_replies.json"));
    // Dictation comes first: learning stops when the user starts one.
    let dictating = || {
        tm.is_streaming()
            || app
                .try_state::<Arc<crate::managers::audio::AudioRecordingManager>>()
                .is_some_and(|rm| rm.is_recording())
    };
    let replay = |s: &AppSettings| -> Result<Summary> {
        // An "unload immediately" setting drops the model after every run.
        if !tm.is_model_loaded() {
            tm.load_model(&settings.selected_model)?;
        }
        // The judged items' audio paths are absolute.
        let results = eval::replay(
            app,
            s,
            &judge,
            std::path::Path::new(""),
            post_process,
            &cache,
            &dictating,
            |_, _| {},
        )?;
        Ok(eval::summarize(&results, false))
    };
    let interrupted_early = || LearnReport {
        outcome: "interrupted".into(),
        keyed,
        ..Default::default()
    };
    let mut interrupted = false;

    let mut current = settings.clone();
    let baseline = match replay(&current) {
        Err(e) if e.is::<eval::Interrupted>() => return Ok(interrupted_early()),
        other => other?,
    };
    let mut best = baseline.clone();
    let mut kept = Vec::new();
    let candidates = propose(app, &settings, &study);
    let tried = candidates.len() as u32;
    for change in candidates {
        // Undo must restore what was in place right before this change,
        // which may be an earlier change from this run.
        let change = change.rebased(&current);
        let mut trial = current.clone();
        change.apply(&mut trial);
        if trial.custom_words == current.custom_words
            && trial.replacements == current.replacements
            && same_tuning(&trial, &current)
        {
            continue; // already in place
        }
        let summary = match replay(&trial) {
            Ok(s) => s,
            // Keep what was already proven; the rest waits for the next run.
            Err(e) if e.is::<eval::Interrupted>() => {
                interrupted = true;
                break;
            }
            Err(e) => {
                log::warn!("Learner: couldn't test {change:?}: {e:#}");
                continue;
            }
        };
        if keeps(&best, &summary, change.has_evidence()) {
            log::info!(
                "Learner: keeping {change:?} ({:?} → {:?})",
                best.final_error,
                summary.final_error
            );
            current = trial;
            best = summary;
            kept.push(change);
        } else {
            log::info!(
                "Learner: dropping {change:?} ({:?} → {:?})",
                best.final_error,
                summary.final_error
            );
        }
    }
    if let Err(e) = cache.save() {
        log::warn!("Learner: couldn't save AI replies: {e:#}");
    }

    // Applied to the saved settings one by one, so anything the user changed
    // meanwhile stays: a value they edited during the run is theirs, and its
    // change is dropped (its Undo would restore a stale value).
    let mut live = get_settings(app);
    kept.retain(|c| {
        let applies = c.still_before(&live);
        if applies {
            c.apply(&mut live);
        }
        applies
    });
    if !kept.is_empty() {
        write_settings(app, live);
        let (before, after) = (score(&baseline), score(&best));
        for change in &kept {
            learning.add_learned(&serde_json::to_string(change)?, before, after)?;
        }
        let _ = app.emit(
            "settings-changed",
            serde_json::json!({ "setting": "learning" }),
        );
    }
    Ok(LearnReport {
        // Interrupted: what was proven is kept, the rest is tried next time.
        outcome: if interrupted {
            "interrupted"
        } else if kept.is_empty() {
            "nothing_new"
        } else {
            "learned"
        }
        .into(),
        keyed,
        judged: judge.len() as u32,
        before: baseline.final_error,
        after: best.final_error,
        kept,
        tried,
    })
}

fn same_tuning(a: &AppSettings, b: &AppSettings) -> bool {
    a.hearing_priority == b.hearing_priority
        && a.hearing_context == b.hearing_context
        && a.hearing_word_limit == b.hearing_word_limit
        && a.cleanup_examples == b.cleanup_examples
}

/// The score a change is judged on: edits the user would still make per word.
fn score(s: &Summary) -> f64 {
    s.final_error.or(s.hearing_error).unwrap_or(1.0)
}

/// Whether `new` is good enough to keep over `old`.
fn keeps(old: &Summary, new: &Summary, evidence: bool) -> bool {
    let (a, b) = (score(old), score(new));
    let protected = new.terms_inserted <= old.terms_inserted
        && new.ai_rejected <= old.ai_rejected + 1
        && new.silence_made_up <= old.silence_made_up
        && new.term_recall.unwrap_or(1.0) >= old.term_recall.unwrap_or(1.0) - 1e-9;
    protected
        && if evidence {
            b <= a + 1e-9
        } else {
            b + MIN_GAIN <= a
        }
}

/// Changes worth testing, most promising first.
fn propose(
    app: &AppHandle,
    settings: &AppSettings,
    study: &[&crate::learning::Dictation],
) -> Vec<Change> {
    let mut out = Vec::new();

    // The user's repeated fixes, as Words and swaps (the same rules as
    // learn-from-edits: a real word is never made into a swap).
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    for d in study {
        let Some(kept) = d.kept.as_deref() else {
            continue;
        };
        let words = crate::spelling::words_in(&[&d.pasted, kept]);
        let known = crate::spelling::known_words(app, &words);
        let is_word = |w: &str| {
            known
                .as_ref()
                .is_some_and(|k| k.contains(&w.to_lowercase()))
        };
        for r in crate::correction::suggest(&d.pasted, kept, &is_word) {
            *seen.entry((r.from, r.to)).or_default() += 1;
        }
    }
    let mut fixes: Vec<((String, String), usize)> =
        seen.into_iter().filter(|(_, n)| *n >= MIN_SEEN).collect();
    fixes.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for ((from, to), _) in fixes {
        out.push(if from.is_empty() {
            Change::AddWord { word: to }
        } else {
            Change::AddSwap { from, to }
        });
    }

    // Swaps the user undid: Owl wrote the right side and they put the left
    // side back. A swap that only looks risky (its left side is a real
    // word) is the user's call, on the Dictionary page.
    for r in &settings.replacements {
        let undone = study.iter().any(|d| {
            d.kept.as_deref().is_some_and(|k| {
                contains_word(&d.pasted, &r.to)
                    && contains_word(k, &r.from)
                    && !contains_word(k, &r.to)
            })
        });
        if undone {
            out.push(Change::RemoveSwap {
                from: r.from.clone(),
                to: r.to.clone(),
            });
        }
    }

    // Words the speech model missed go first in its hint list.
    let mut missed: HashMap<String, usize> = HashMap::new();
    for d in study {
        let Some(kept) = d.kept.as_deref() else {
            continue;
        };
        for w in &settings.custom_words {
            if contains_word(kept, w) && !contains_word(&d.heard, w) {
                *missed.entry(w.clone()).or_default() += 1;
            }
        }
    }
    let mut priority: Vec<(String, usize)> = missed.into_iter().collect();
    priority.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let after: Vec<String> = priority.into_iter().map(|(w, _)| w).take(20).collect();
    if !after.is_empty() && after != settings.hearing_priority {
        out.push(Change::Priority {
            before: settings.hearing_priority.clone(),
            after,
        });
    }

    // Fewer hints can hear better than many.
    if settings.hearing_word_limit.is_none() {
        for cap in [40u32, 20] {
            if settings.custom_words.len() > cap as usize {
                out.push(Change::WordLimit {
                    before: None,
                    after: Some(cap),
                });
            }
        }
    }

    // A context passage about the user's topics, written by their AI, so
    // only when they've let dictations go to it (AI cleanup on).
    let passage = crate::actions::ai_cleanup_on(settings)
        .then(|| context_passage(settings, study))
        .flatten();
    if let Some(passage) = passage {
        if passage != settings.hearing_context {
            out.push(Change::Context {
                before: settings.hearing_context.clone(),
                after: passage,
            });
        }
    }

    // The user's past fixes as examples for the AI cleanup.
    let edited = study
        .iter()
        .filter(|d| d.kept.as_deref().is_some_and(|k| k != d.pasted))
        .count();
    if crate::actions::ai_cleanup_on(settings) && !settings.cleanup_examples && edited >= 3 {
        out.push(Change::Examples {
            before: false,
            after: true,
        });
    }
    out
}

fn contains_word(text: &str, word: &str) -> bool {
    let (text, word) = (text.to_lowercase(), word.to_lowercase());
    if word.chars().any(crate::correction::is_cjk) {
        return text.contains(&word);
    }
    text.match_indices(&word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

const CONTEXT_PROMPT: &str = "You write context for a speech recognizer. From the user's past dictations below, write at most 50 words naming the topics, products, people and terms they talk about, in the languages they use. Plain sentences, no lists, no advice. Output only the passage.";

/// A few sentences about what the user dictates about, written by their AI.
fn context_passage(
    settings: &AppSettings,
    study: &[&crate::learning::Dictation],
) -> Option<String> {
    let mut sample = String::new();
    for d in study.iter().rev() {
        let Some(kept) = d.kept.as_deref() else {
            continue;
        };
        if sample.len() + kept.len() > 4000 {
            break;
        }
        sample.push_str(kept);
        sample.push('\n');
    }
    if sample.trim().is_empty() {
        return None;
    }
    let user = format!(
        "<words>\n{}\n</words>\n<dictations>\n{sample}</dictations>",
        settings.custom_words.join(", ")
    );
    let reply = tauri::async_runtime::block_on(async {
        tokio::time::timeout(
            Duration::from_secs(60),
            crate::actions::ask_ai(settings, CONTEXT_PROMPT, user),
        )
        .await
    });
    match reply {
        Ok(Ok(text)) => {
            let text = text.trim();
            // Hearing context must stay short; a long reply is a bad reply.
            (!text.is_empty() && text.split_whitespace().count() <= 80).then(|| text.to_string())
        }
        Ok(Err(e)) => {
            log::warn!("Learner: no context passage: {e}");
            None
        }
        Err(_) => None,
    }
}

/// Checks hourly and learns when Learning is on, there are new corrected
/// dictations since the last run, the last run was a day ago, and Owl isn't
/// recording.
pub fn start_background(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60 * 60));
        if !get_settings(&app).learning_enabled {
            continue;
        }
        let Some(learning) = app.try_state::<Arc<Learning>>() else {
            continue;
        };
        let due = match learning.last_run() {
            Ok(Some((at, _, _))) => {
                chrono::Utc::now().timestamp() - at >= 24 * 60 * 60
                    && learning.keyed_since(at).unwrap_or(0) >= 5
            }
            Ok(None) => learning.stats().map(|s| s.keyed).unwrap_or(0) >= MIN_KEYED as i64,
            Err(_) => false,
        };
        let recording = app
            .try_state::<Arc<crate::managers::audio::AudioRecordingManager>>()
            .is_some_and(|rm| rm.is_recording());
        if due && !recording {
            match learn(&app) {
                Ok(r) => log::info!(
                    "Learner: {} ({} kept of {})",
                    r.outcome,
                    r.kept.len(),
                    r.tried
                ),
                Err(e) => log::warn!("Learner: {e:#}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(final_error: f64) -> Summary {
        Summary {
            final_error: Some(final_error),
            term_recall: Some(0.5),
            ..Default::default()
        }
    }

    #[test]
    fn changes_undo_cleanly() {
        let mut s = crate::settings::get_default_settings();
        s.replacements = vec![Replacement {
            from: "threat".into(),
            to: "threads".into(),
        }];
        let original = serde_json::to_value(&s).unwrap();
        let changes = [
            Change::AddWord { word: "Owl".into() },
            Change::AddSwap {
                from: "Open Claw".into(),
                to: "OpenClaw".into(),
            },
            Change::RemoveSwap {
                from: "threat".into(),
                to: "threads".into(),
            },
            Change::Priority {
                before: vec![],
                after: vec!["Owl".into()],
            },
            Change::Context {
                before: String::new(),
                after: "Talks about Owl.".into(),
            },
            Change::WordLimit {
                before: None,
                after: Some(20),
            },
            Change::Examples {
                before: false,
                after: true,
            },
        ];
        for c in &changes {
            c.apply(&mut s);
        }
        assert_eq!(s.hearing_word_limit, Some(20));
        assert!(s.replacements.iter().all(|r| r.from != "threat"));
        for c in changes.iter().rev() {
            c.undo(&mut s);
        }
        assert_eq!(serde_json::to_value(&s).unwrap(), original);
    }

    #[test]
    fn undoing_a_later_cap_restores_the_earlier_one() {
        let mut s = crate::settings::get_default_settings();
        s.hearing_word_limit = Some(40);
        let change = Change::WordLimit {
            before: None,
            after: Some(20),
        }
        .rebased(&s);
        change.apply(&mut s);
        change.undo(&mut s);
        assert_eq!(s.hearing_word_limit, Some(40));
    }

    #[test]
    fn a_change_is_kept_only_if_it_helps_and_hurts_nothing() {
        let old = summary(0.20);
        assert!(keeps(&old, &summary(0.15), false));
        // No gain: kept only with the user's own fixes behind it.
        assert!(!keeps(&old, &summary(0.20), false));
        assert!(keeps(&old, &summary(0.20), true));
        assert!(!keeps(&old, &summary(0.21), true));
        // Better overall, but it inserts a Word that wasn't said.
        let mut worse = summary(0.10);
        worse.terms_inserted = 1;
        assert!(!keeps(&old, &worse, false));
    }

    #[test]
    fn words_match_whole_words_and_chinese_anywhere() {
        assert!(contains_word("Restart OpenClaw now", "openclaw"));
        assert!(!contains_word("threads", "threat"));
        assert!(contains_word("我們用台語", "台語"));
    }
}
