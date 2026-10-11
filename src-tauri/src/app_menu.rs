//! Owl: the macOS menu bar: Owl, Edit, Dictation, Notetaker and Window.
//!
//! Tauri sends every menu's clicks to the tray's handler in lib.rs, so the
//! ids shared with the tray menu (settings, quit, copy_last_transcript,
//! lang:*) work here unchanged and `handle` takes the rest. The menu is
//! rebuilt whenever settings are written, so its ticks stay current.

use crate::audio_toolkit::audio::list_input_devices;
use crate::managers::meeting::MeetingManager;
use crate::settings::{self, AppSettings, CleanupLevel};
use crate::tray_i18n::get_tray_translations;
use log::error;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};

/// Listing microphones is slow, so it happens when Owl's window comes to the
/// front instead of on every rebuild.
static MICROPHONES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Lists the microphones again, then rebuilds the menu.
pub fn refresh_microphones(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        match list_input_devices() {
            Ok(devices) => {
                *MICROPHONES.lock().unwrap() = devices.into_iter().map(|d| d.name).collect()
            }
            Err(e) => error!("Menu bar: failed to list microphones: {e}"),
        }
        refresh(&app);
    });
}

/// Rebuilds the menu from the current settings and meeting state.
pub fn refresh(app: &AppHandle) {
    let settings = settings::get_settings(app);
    let meeting = app
        .try_state::<Arc<MeetingManager>>()
        .is_some_and(|m| m.active_id().is_some());
    let microphones = MICROPHONES.lock().unwrap().clone();
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let menu = build(&handle, &settings, meeting, &microphones);
        if let Err(e) = menu.and_then(|menu| handle.set_menu(menu).map(|_| ())) {
            error!("Failed to set the menu bar: {e}");
        }
    });
}

fn build(
    app: &AppHandle,
    s: &AppSettings,
    meeting: bool,
    microphones: &[String],
) -> tauri::Result<Menu<Wry>> {
    let t = get_tray_translations(Some(s.app_language.clone()));
    let sep = || PredefinedMenuItem::separator(app);
    let item = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, None::<&str>);
    let check = |id: &str, label: &str, on: bool| {
        CheckMenuItem::with_id(app, id, label, true, on, None::<&str>)
    };

    let owl = Submenu::with_items(
        app,
        "Owl",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &sep()?,
            &MenuItem::with_id(app, "settings", &t.settings, true, Some("Cmd+,"))?,
            &sep()?,
            &PredefinedMenuItem::services(app, None)?,
            &sep()?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &sep()?,
            &MenuItem::with_id(app, "quit", &t.quit, true, Some("Cmd+Q"))?,
        ],
    )?;

    // Without an Edit menu, macOS drops Cmd+C/V/X/Z/A in Owl's text boxes.
    let edit = Submenu::with_items(
        app,
        &t.edit,
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &sep()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let microphone = Submenu::with_items(
        app,
        &t.microphone,
        true,
        &[&check(
            "mic:default",
            &t.default_microphone,
            s.selected_microphone.is_none(),
        )?],
    )?;
    for name in microphones {
        let on = s.selected_microphone.as_deref() == Some(name.as_str());
        microphone.append(&check(&format!("mic:{name}"), name, on)?)?;
    }

    let level = s.cleanup_level;
    let cleanup = Submenu::with_items(
        app,
        &t.cleanup,
        true,
        &[
            &check("cleanup:none", &t.cleanup_none, level == CleanupLevel::None)?,
            &check(
                "cleanup:light",
                &t.cleanup_light,
                level == CleanupLevel::Light,
            )?,
            &check(
                "cleanup:medium",
                &t.cleanup_medium,
                level == CleanupLevel::Medium,
            )?,
            &sep()?,
            &check("ai_cleanup", &t.ai_cleanup, s.post_process_enabled)?,
        ],
    )?;

    let dictation = Submenu::with_items(
        app,
        &t.dictation,
        true,
        &[
            &item("copy_last_transcript", &t.copy_last_transcript)?,
            &sep()?,
            &microphone,
            &crate::tray::language_submenu(app, &t, settings::dictation_language(s))?,
            &cleanup,
        ],
    )?;

    let notetaker = Submenu::with_items(
        app,
        &t.notetaker,
        true,
        &[
            &item(
                "meeting_toggle",
                if meeting {
                    &t.stop_meeting
                } else {
                    &t.start_meeting
                },
            )?,
            &item("show_meetings", &t.show_meetings)?,
        ],
    )?;

    let window = Submenu::with_items(
        app,
        &t.window,
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &sep()?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    Menu::with_items(app, &[&owl, &edit, &dictation, &notetaker, &window])
}

/// Handles the items only the menu bar has. Other ids are ignored.
pub fn handle(app: &AppHandle, id: &str) {
    if let Some(level) = id.strip_prefix("cleanup:") {
        let level = match level {
            "none" => CleanupLevel::None,
            "medium" => CleanupLevel::Medium,
            _ => CleanupLevel::Light,
        };
        crate::commands::cleanup::change_cleanup_level_setting(app.clone(), level);
        settings_changed(app, "cleanup_level");
    } else if id == "ai_cleanup" {
        let on = settings::get_settings(app).post_process_enabled;
        let _ = crate::shortcut::change_post_process_enabled_setting(app.clone(), !on);
        settings_changed(app, "post_process_enabled");
    } else if let Some(name) = id.strip_prefix("mic:") {
        let (app, name) = (app.clone(), name.to_string());
        tauri::async_runtime::spawn(async move {
            if let Err(e) = crate::commands::audio::set_selected_microphone(app.clone(), name).await
            {
                error!("Menu bar: failed to switch microphone: {e}");
            }
            settings_changed(&app, "selected_microphone");
        });
    } else if id == "meeting_toggle" {
        let meetings = app.state::<Arc<MeetingManager>>();
        let result = if meetings.active_id().is_some() {
            meetings.stop()
        } else {
            meetings.start().map(|_| ())
        };
        if let Err(e) = result {
            error!("Menu bar: failed to start or stop the meeting: {e}");
        }
    }
}

/// Lets the settings window pick up a change made from the menu.
fn settings_changed(app: &AppHandle, setting: &str) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": setting }),
    );
}
