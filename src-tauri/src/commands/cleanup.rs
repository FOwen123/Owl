//! Owl: the None / Light / Medium cleanup level and spoken voice commands.

use crate::settings::{self, CleanupLevel};
use tauri::AppHandle;

#[tauri::command]
#[specta::specta]
pub fn change_cleanup_level_setting(app: AppHandle, level: CleanupLevel) {
    let mut s = settings::get_settings(&app);
    settings::set_cleanup_level(&mut s, level);
    settings::write_settings(&app, s);
}

#[tauri::command]
#[specta::specta]
pub fn change_voice_commands_setting(app: AppHandle, enabled: bool) {
    let mut s = settings::get_settings(&app);
    s.voice_commands_enabled = enabled;
    settings::write_settings(&app, s);
}
