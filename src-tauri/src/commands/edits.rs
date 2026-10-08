//! Owl learn-from-edits: Add / Dismiss on the Dictionary page's suggestions.

use crate::settings::{self, Replacement};
use tauri::AppHandle;

fn update(app: &AppHandle, change: impl FnOnce(&mut settings::AppSettings)) {
    let mut s = settings::get_settings(app);
    change(&mut s);
    settings::write_settings(app, s);
}

#[tauri::command]
#[specta::specta]
pub fn accept_edit_suggestion(app: AppHandle, suggestion: Replacement) {
    update(&app, |s| settings::accept_edit_suggestion(s, &suggestion));
}

#[tauri::command]
#[specta::specta]
pub fn dismiss_edit_suggestion(app: AppHandle, suggestion: Replacement) {
    update(&app, |s| settings::dismiss_edit_suggestion(s, &suggestion));
}

#[tauri::command]
#[specta::specta]
pub fn change_learn_from_edits_setting(app: AppHandle, enabled: bool) {
    update(&app, |s| s.learn_from_edits = enabled);
}
