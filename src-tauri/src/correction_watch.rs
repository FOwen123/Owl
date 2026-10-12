//! Owl learn-from-edits: after a paste, watch the focused text box for a short
//! while and turn the user's fixes into Dictionary entries (`correction.rs`
//! decides what counts as a fix). Each one is offered in the overlay with a
//! countdown: it is added unless the user presses X.
//!
//! Reads the text box through the macOS Accessibility API, which Owl already
//! needs for pasting. Never reads password fields, stops when the user
//! switches apps, and keeps nothing but the suggestions it finds. Apps that
//! don't expose their text (some terminals and Electron apps) are skipped.
//! The paste also asks it for the character before the cursor.

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::AppHandle;

/// Bumped by every paste, so an older watcher stops before it mistakes
/// Owl's next paste for the user's edit.
#[cfg(target_os = "macos")]
static PASTES: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
pub fn start(app: AppHandle, pasted: String) {
    let paste = PASTES.fetch_add(1, Ordering::SeqCst) + 1;
    if crate::settings::get_settings(&app).learn_from_edits && !pasted.trim().is_empty() {
        std::thread::spawn(move || mac::watch(app, pasted, paste));
    }
}

#[cfg(not(target_os = "macos"))]
pub fn start(_app: AppHandle, _pasted: String) {}

/// The character just before the cursor in the focused text box, for the
/// space before a dictation. None for password fields and apps that hide
/// their text.
#[cfg(target_os = "macos")]
pub fn char_before_cursor() -> Option<char> {
    mac::char_before_cursor()
}

#[cfg(not(target_os = "macos"))]
pub fn char_before_cursor() -> Option<char> {
    None
}

/// The selected text in the focused text box, `Some("")` when nothing is
/// selected, and None for password fields and apps that hide their text.
#[cfg(target_os = "macos")]
pub fn selected_text() -> Option<String> {
    mac::selected_text()
}

#[cfg(not(target_os = "macos"))]
pub fn selected_text() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
mod mac {
    use crate::correction::{edited_span, suggest};
    use crate::overlay;
    use crate::settings::{self, Replacement};
    use log::debug;
    use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
    use objc2_core_foundation::{CFRange, CFRetained, CFString, CFType};
    use std::ptr::NonNull;
    use std::sync::atomic::Ordering;
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter};

    const WATCH_FOR: Duration = Duration::from_secs(45);
    const POLL_EVERY: Duration = Duration::from_millis(500);
    /// Compare once the text has stopped changing for this long.
    const QUIET_FOR: Duration = Duration::from_millis(1500);
    /// How long the overlay offers a fix before adding it.
    const COUNTDOWN: Duration = Duration::from_secs(5);

    pub fn watch(app: AppHandle, pasted: String, paste: u64) {
        // The paste can land a little later (the reliable paste path is
        // asynchronous), so look for the pasted text for up to 2 s.
        let Some((field, baseline)) = (0..4).find_map(|_| {
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
            if super::PASTES.load(Ordering::SeqCst) != paste {
                return; // a newer dictation was pasted
            }
            if !settings::get_settings(&app).learn_from_edits {
                return; // turned off while watching
            }
            let Some((now_field, text)) = focused_text() else {
                continue; // focus is briefly elsewhere, e.g. a menu
            };
            // Another app, document or field: its text was never pasted.
            let (a, b): (&CFType, &CFType) = (&field, &now_field);
            if a != b {
                return;
            }
            if text != last {
                last = text;
                changed_at = Instant::now();
            } else if last != checked && changed_at.elapsed() >= QUIET_FOR {
                checked = last.clone();
                if let Some(edited) = edited_span(&baseline, &last, &pasted) {
                    let words = crate::spelling::words_in(&[&pasted, &edited]);
                    let known = crate::spelling::known_words(&app, &words);
                    // No spell checker: Handy's rules, which treat no word as known.
                    let is_word = |w: &str| {
                        known
                            .as_ref()
                            .is_some_and(|k| k.contains(&w.to_lowercase()))
                    };
                    record(&app, suggest(&pasted, &edited, &is_word));
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

        let ends = Instant::now() + COUNTDOWN;
        while Instant::now() < ends {
            sleep(Duration::from_millis(100));
            let s = settings::get_settings(app);
            if !s.learn_from_edits || !s.edit_suggestions.contains(suggestion) {
                overlay::hide_suggestion_overlay(app); // X pressed or learning off
                return;
            }
            if !overlay::showing_suggestion() {
                return; // a dictation took the overlay; the fix waits on the Dictionary page
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

    /// The focused element, unless it's a password field.
    fn focused_field() -> Option<CFRetained<AXUIElement>> {
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
            Some(focused)
        }
    }

    /// The focused text box and its text, unless it's a password field or
    /// doesn't expose its text.
    fn focused_text() -> Option<(CFRetained<AXUIElement>, String)> {
        let focused = focused_field()?;
        let text = unsafe { string_attribute(&focused, "AXValue")? };
        Some((focused, text))
    }

    pub fn selected_text() -> Option<String> {
        let focused = focused_field()?;
        unsafe { string_attribute(&focused, "AXSelectedText") }
    }

    pub fn char_before_cursor() -> Option<char> {
        let (field, text) = focused_text()?;
        let mut range = CFRange {
            location: 0,
            length: 0,
        };
        unsafe {
            let value = attribute(&field, "AXSelectedTextRange")?
                .downcast::<AXValue>()
                .ok()?;
            if !value.value(AXValueType::CFRange, NonNull::from(&mut range).cast()) {
                return None;
            }
        }
        // The range counts UTF-16 units, as macOS strings do.
        let before: Vec<u16> = text
            .encode_utf16()
            .take(usize::try_from(range.location).ok()?)
            .collect();
        String::from_utf16_lossy(&before).chars().last()
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
