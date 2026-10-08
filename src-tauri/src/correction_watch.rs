//! Owl learn-from-edits: after a paste, watch the focused text box for a short
//! while and turn the user's fixes into Dictionary entries (`correction.rs`
//! decides what counts as a fix). Each one is offered in the overlay with a
//! countdown: it is added unless the user presses X.
//!
//! Reads the text box through the macOS Accessibility API, which Owl already
//! needs for pasting. Never reads password fields, stops when the user
//! switches apps, and keeps nothing but the suggestions it finds. Apps that
//! don't expose their text (some terminals and Electron apps) are skipped.

use tauri::AppHandle;

#[cfg(target_os = "macos")]
pub fn start(app: AppHandle, pasted: String) {
    if crate::settings::get_settings(&app).learn_from_edits && !pasted.trim().is_empty() {
        std::thread::spawn(move || mac::watch(app, pasted));
    }
}

#[cfg(not(target_os = "macos"))]
pub fn start(_app: AppHandle, _pasted: String) {}

#[cfg(target_os = "macos")]
mod mac {
    use crate::correction::{edited_span, suggest};
    use crate::overlay;
    use crate::settings::{self, Replacement};
    use log::debug;
    use objc2_application_services::{AXError, AXUIElement};
    use objc2_core_foundation::{CFRetained, CFString, CFType};
    use std::ptr::NonNull;
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter};

    const WATCH_FOR: Duration = Duration::from_secs(45);
    const POLL_EVERY: Duration = Duration::from_millis(500);
    /// Compare once the text has stopped changing for this long.
    const QUIET_FOR: Duration = Duration::from_millis(1500);
    /// How long the overlay offers a fix before adding it.
    const COUNTDOWN: Duration = Duration::from_secs(5);

    pub fn watch(app: AppHandle, pasted: String) {
        // The paste can land a little later (the reliable paste path is
        // asynchronous), so look for the pasted text for up to 2 s.
        let Some((pid, baseline)) = (0..4).find_map(|_| {
            sleep(POLL_EVERY);
            focused_text().filter(|(_, text)| text.contains(&pasted))
        }) else {
            debug!("Learn from edits: pasted text not visible in the focused text box");
            return;
        };

        let started = Instant::now();
        let (mut last, mut checked) = (baseline.clone(), baseline.clone());
        let mut changed_at = Instant::now();
        while started.elapsed() < WATCH_FOR {
            sleep(POLL_EVERY);
            let Some((now_pid, text)) = focused_text() else {
                continue; // focus is briefly elsewhere, e.g. a menu
            };
            if now_pid != pid {
                return; // switched apps
            }
            if text != last {
                last = text;
                changed_at = Instant::now();
            } else if last != checked && changed_at.elapsed() >= QUIET_FOR {
                checked = last.clone();
                if let Some(edited) = edited_span(&baseline, &last, &pasted) {
                    record(&app, suggest(&pasted, &edited));
                }
            }
        }
    }

    fn record(app: &AppHandle, found: Vec<Replacement>) {
        if found.is_empty() {
            return;
        }
        let mut s = settings::get_settings(app);
        let added = settings::add_edit_suggestions(&mut s, found);
        if added.is_empty() {
            return;
        }
        settings::write_settings(app, s);
        settings_changed(app);
        for suggestion in &added {
            offer(app, suggestion);
        }
    }

    /// Shows the fix in the overlay and adds it when the countdown ends. X
    /// (or Add/Dismiss on the Dictionary page) resolves it first. If the
    /// overlay can't show it, it waits on the Dictionary page.
    fn offer(app: &AppHandle, suggestion: &Replacement) {
        // Don't cover a dictation in progress.
        let give_up = Instant::now() + WATCH_FOR;
        while overlay::showing_dictation() {
            if Instant::now() > give_up {
                return;
            }
            sleep(Duration::from_millis(250));
        }
        if !overlay::show_suggestion_overlay(app, suggestion, COUNTDOWN.as_secs()) {
            return;
        }

        let pending = || {
            settings::get_settings(app)
                .edit_suggestions
                .contains(suggestion)
        };
        let ends = Instant::now() + COUNTDOWN;
        while Instant::now() < ends {
            sleep(Duration::from_millis(100));
            if !pending() {
                overlay::hide_suggestion_overlay(app); // X pressed
                return;
            }
        }

        let mut s = settings::get_settings(app);
        settings::accept_edit_suggestion(&mut s, suggestion);
        settings::write_settings(app, s);
        settings_changed(app);
        let _ = app.emit_to("recording_overlay", "edit-suggestion-added", ());
        sleep(Duration::from_millis(1200)); // let the "Added" animation play
        overlay::hide_suggestion_overlay(app);
    }

    fn settings_changed(app: &AppHandle) {
        let _ = app.emit(
            "settings-changed",
            serde_json::json!({ "setting": "edit_suggestions" }),
        );
    }

    /// The process ID and text of the focused text box, unless it's a
    /// password field or doesn't expose its text.
    fn focused_text() -> Option<(i32, String)> {
        unsafe {
            let system = AXUIElement::new_system_wide();
            // A hung app must not stall the watcher.
            system.set_messaging_timeout(0.25);
            let focused = attribute(&system, "AXFocusedUIElement")?
                .downcast::<AXUIElement>()
                .ok()?;
            if string_attribute(&focused, "AXSubrole").as_deref() == Some("AXSecureTextField") {
                return None;
            }
            let text = string_attribute(&focused, "AXValue")?;
            let mut pid = 0;
            (focused.pid(NonNull::from(&mut pid)) == AXError::Success).then_some((pid, text))
        }
    }

    unsafe fn attribute(element: &AXUIElement, name: &str) -> Option<CFRetained<CFType>> {
        let mut value: *const CFType = std::ptr::null();
        let error =
            element.copy_attribute_value(&CFString::from_str(name), NonNull::from(&mut value));
        if error != AXError::Success {
            return None;
        }
        // Copy* functions return a +1 reference, which CFRetained takes over.
        NonNull::new(value as *mut CFType).map(|v| CFRetained::from_raw(v))
    }

    unsafe fn string_attribute(element: &AXUIElement, name: &str) -> Option<String> {
        Some(
            attribute(element, name)?
                .downcast::<CFString>()
                .ok()?
                .to_string(),
        )
    }
}
