//! Owl: the Learning page (`learning.rs`, `learner.rs`), fixes typed in
//! History, the Dictionary's risky-swap warning, and Sign in with ChatGPT.

use crate::learner::{Change, LearnReport};
use crate::learning::{Learning, LearningStats};
use crate::managers::history::HistoryManager;
use crate::settings::{self, Replacement};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

fn learning(app: &AppHandle) -> Result<Arc<Learning>, String> {
    app.try_state::<Arc<Learning>>()
        .map(|l| l.inner().clone())
        .ok_or_else(|| "The learning log is unavailable".to_string())
}

fn update(app: &AppHandle, change: impl FnOnce(&mut settings::AppSettings)) {
    let mut s = settings::get_settings(app);
    change(&mut s);
    settings::write_settings(app, s);
}

#[derive(Serialize, Deserialize, Type, Clone, Debug)]
pub struct LearnedChange {
    pub id: i64,
    pub at: i64,
    pub change: Change,
    pub before: f64,
    pub after: f64,
    pub undone: bool,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug)]
pub struct LearningOverview {
    pub stats: LearningStats,
    pub min_keyed: u32,
    pub running: bool,
    pub last_run_at: Option<i64>,
    pub last_report: Option<LearnReport>,
    pub changes: Vec<LearnedChange>,
}

#[tauri::command]
#[specta::specta]
pub fn get_learning_overview(app: AppHandle) -> Result<LearningOverview, String> {
    let learning = learning(&app)?;
    let last = learning.last_run().map_err(|e| e.to_string())?;
    let changes = learning
        .learned()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter_map(|(id, at, change, before, after, undone)| {
            Some(LearnedChange {
                id,
                at,
                change: serde_json::from_str(&change).ok()?,
                before,
                after,
                undone,
            })
        })
        .collect();
    Ok(LearningOverview {
        stats: learning.stats().map_err(|e| e.to_string())?,
        min_keyed: crate::learner::MIN_KEYED as u32,
        running: crate::learner::is_running(),
        last_run_at: last.as_ref().map(|(at, _, _)| *at),
        last_report: last.and_then(|(_, _, report)| serde_json::from_str(&report).ok()),
        changes,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn run_learning_now(app: AppHandle) -> Result<LearnReport, String> {
    let _ = app.emit("learning-changed", ());
    tauri::async_runtime::spawn_blocking(move || crate::learner::learn(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
pub fn undo_learned_change(app: AppHandle, id: i64) -> Result<(), String> {
    let learning = learning(&app)?;
    let Some(change) = learning.undo_learned(id).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    let change: Change = serde_json::from_str(&change).map_err(|e| e.to_string())?;
    update(&app, |s| change.undo(s));
    let _ = app.emit("learning-changed", ());
    // So the settings store (Words, replacements, context) shows the undo.
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "learning" }),
    );
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_learning_enabled_setting(app: AppHandle, enabled: bool) {
    update(&app, |s| s.learning_enabled = enabled);
}

#[tauri::command]
#[specta::specta]
pub fn change_hearing_context_setting(app: AppHandle, context: String) {
    update(&app, |s| s.hearing_context = context.trim().to_string());
}

#[tauri::command]
#[specta::specta]
pub fn change_cleanup_examples_setting(app: AppHandle, enabled: bool) {
    update(&app, |s| s.cleanup_examples = enabled);
}

/// Saves the text a History entry should have been. With Learning on, it
/// becomes that dictation's answer key; returns whether it was in the log.
#[tauri::command]
#[specta::specta]
pub async fn fix_history_entry(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i64,
    text: String,
) -> Result<bool, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("The fix is empty".into());
    }
    let entry = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("History entry not found")?;
    let logged = learning(&app)?
        .set_fixed(&entry.file_name, &text)
        .map_err(|e| e.to_string())?;
    let _ = app.emit("learning-changed", ());
    Ok(logged)
}

/// History entries that are in the learning log, so History can offer Fix.
#[tauri::command]
#[specta::specta]
pub fn logged_history_files(app: AppHandle, files: Vec<String>) -> Result<Vec<String>, String> {
    let learning = learning(&app)?;
    Ok(files
        .into_iter()
        .filter(|f| learning.has_history_file(f).unwrap_or(false))
        .collect())
}

/// Dictionary swaps that rewrite a real word every time it's said
/// ("threat → threads").
#[tauri::command]
#[specta::specta]
pub fn risky_replacements(app: AppHandle) -> Vec<Replacement> {
    let s = settings::get_settings(&app);
    let single = |r: &Replacement| !r.from.contains(' ') && !r.from.eq_ignore_ascii_case(&r.to);
    let froms: Vec<&str> = s
        .replacements
        .iter()
        .filter(|r| single(r))
        .map(|r| r.from.as_str())
        .collect();
    let Some(known) = crate::spelling::known_words(&app, &crate::spelling::words_in(&froms)) else {
        return Vec::new();
    };
    s.replacements
        .iter()
        .filter(|r| single(r) && known.contains(&r.from.to_lowercase()))
        .cloned()
        .collect()
}

#[tauri::command]
#[specta::specta]
pub fn chatgpt_status() -> crate::chatgpt::ChatGptStatus {
    crate::chatgpt::status()
}

#[tauri::command]
#[specta::specta]
pub async fn chatgpt_sign_in(app: AppHandle) -> Result<crate::chatgpt::ChatGptStatus, String> {
    let opener = app.clone();
    let status = crate::chatgpt::sign_in(move |url| {
        opener
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| anyhow::anyhow!("Couldn't open the browser: {e}"))
    })
    .await
    .map_err(|e| format!("{e:#}"))?;
    let _ = app.emit("chatgpt-changed", ());
    Ok(status)
}

#[tauri::command]
#[specta::specta]
pub async fn chatgpt_sign_out(app: AppHandle) -> Result<(), String> {
    crate::chatgpt::sign_out()
        .await
        .map_err(|e| format!("{e:#}"))?;
    let _ = app.emit("chatgpt-changed", ());
    Ok(())
}
