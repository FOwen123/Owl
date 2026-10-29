//! Owl learn-from-edits: after a paste, watch the focused text box for a short
//! while and turn the user's fixes into Dictionary entries (`correction.rs`
//! decides what counts as a fix). Each one is offered in the overlay with a
//! countdown: it is added unless the user presses X.
//!
//! Reads the text box through the macOS Accessibility API, which Owl already
//! needs for pasting, or Windows UI Automation (`correction_watch/win.rs`).
//! Never reads password fields, stops when the user switches apps, and keeps
//! nothing but the suggestions it finds. Apps that don't expose their text
//! (some terminals and Electron apps) are skipped.
//! The paste also asks it for the character before the cursor.

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::AppHandle;

#[cfg(target_os = "macos")]
use mac as os;
#[cfg(target_os = "windows")]
mod win;
#[cfg(target_os = "windows")]
use win as os;

/// Bumped by every paste, so an older watcher stops before it mistakes
/// Owl's next paste for the user's edit.
#[cfg(any(target_os = "macos", target_os = "windows"))]
static PASTES: AtomicU64 = AtomicU64::new(0);

#[cfg(any(target_os = "macos", target_os = "windows"))]
/// `learning_id`: the dictation's row in the learning log, which gets the
/// user's edit as its answer key.
pub fn start(app: AppHandle, pasted: String, learning_id: Option<i64>) {
    let paste = PASTES.fetch_add(1, Ordering::SeqCst) + 1;
    let wanted = crate::settings::get_settings(&app).learn_from_edits || learning_id.is_some();
    if wanted && !pasted.trim().is_empty() {
        std::thread::spawn(move || watcher::watch(app, pasted, paste, learning_id));
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn start(_app: AppHandle, _pasted: String, _learning_id: Option<i64>) {}

/// Stops the watcher of the last dictation, so text Owl itself changes next
/// (Edit by voice) isn't learned as the user's fix.
pub fn stop() {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    PASTES.fetch_add(1, Ordering::SeqCst);
}

/// The character just before the cursor in the focused text box, for the
/// space before a dictation. None for password fields and apps that hide
/// their text.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn char_before_cursor() -> Option<char> {
    os::char_before_cursor()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn char_before_cursor() -> Option<char> {
    None
}

/// Where Edit by voice works: the focused text box (as a hash), the
/// selected range and the selected text ("" when nothing is selected). None
/// for password fields and apps that hide their text.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn selection_spot() -> Option<(u64, (isize, isize), String)> {
    os::selection_spot()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn selection_spot() -> Option<(u64, (isize, isize), String)> {
    None
}

/// Whether the focus is plainly not a text box: a web page itself, a link,
/// a button, a file list. False when Owl can't tell (apps that hide their
/// text), so those still get the paste.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn no_text_box() -> bool {
    os::no_text_box()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn no_text_box() -> bool {
    false
}

/// Asks an Electron app (Slack, VS Code, …) to expose its text, as screen
/// readers do. Other apps ignore it.
#[cfg(target_os = "macos")]
pub fn expose_text(pid: i32) {
    mac::expose_text(pid)
}

#[cfg(not(target_os = "macos"))]
pub fn expose_text(_pid: i32) {}

/// The titles of an app's windows (a browser's: each window's open tab).
/// Empty without the Accessibility permission.
#[cfg(target_os = "macos")]
pub fn window_titles(pid: i32) -> Vec<String> {
    mac::window_titles(pid)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod watcher {
    use super::os::{focused_text, same};
    use crate::correction::{edited_span, suggest};
    use crate::overlay;
    use crate::settings::{self, Replacement};
    use log::debug;
    use std::sync::atomic::Ordering;
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter, Manager};

    const WATCH_FOR: Duration = Duration::from_secs(45);
    const POLL_EVERY: Duration = Duration::from_millis(500);
    /// Compare once the text has stopped changing for this long.
    const QUIET_FOR: Duration = Duration::from_millis(1500);
    /// How long the overlay offers a fix before adding it.
    const COUNTDOWN: Duration = Duration::from_secs(5);

    pub fn watch(app: AppHandle, pasted: String, paste: u64, learning_id: Option<i64>) {
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
            let s = settings::get_settings(&app);
            let (suggest_fixes, log_edit) = (
                s.learn_from_edits,
                learning_id.is_some() && s.learning_enabled,
            );
            if !suggest_fixes && !log_edit {
                return; // turned off while watching
            }
            let Some((now_field, text)) = focused_text() else {
                continue; // focus is briefly elsewhere, e.g. a menu
            };
            // Another app, document or field: its text was never pasted.
            if !same(&field, &now_field) {
                return;
            }
            if text != last {
                last = text;
                changed_at = Instant::now();
            } else if last != checked && changed_at.elapsed() >= QUIET_FOR {
                checked = last.clone();
                let Some(edited) = edited_span(&baseline, &last, &pasted) else {
                    continue;
                };
                // Only a fix becomes the answer key: a box cleared by sending
                // the message, or a new message typed over it, is not one.
                let fix = crate::correction::is_fix(&pasted, &edited);
                if let (true, true, Some(id)) = (log_edit, fix, learning_id) {
                    if let Some(learning) =
                        app.try_state::<std::sync::Arc<crate::learning::Learning>>()
                    {
                        match learning.set_edited(id, edited.trim()) {
                            Ok(()) => {
                                let _ = app.emit("learning-changed", ());
                            }
                            Err(e) => debug!("Learning: couldn't save the edit: {e:#}"),
                        }
                    }
                }
                if suggest_fixes {
                    let words = crate::spelling::words_in(&[&pasted, &edited]);
                    let known = crate::spelling::known_words(&app, &words);
                    // No spell checker: Owl's rules, which treat no word as known.
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
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2_application_services::{AXError, AXUIElement, AXValue, AXValueType};
    use objc2_core_foundation::{CFArray, CFBoolean, CFRange, CFRetained, CFString, CFType};
    use std::ptr::NonNull;

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
    pub fn focused_text() -> Option<(CFRetained<AXUIElement>, String)> {
        let focused = focused_field()?;
        let text = unsafe { string_attribute(&focused, "AXValue")? };
        Some((focused, text))
    }

    /// Whether two reads came from the same text box.
    pub fn same(a: &AXUIElement, b: &AXUIElement) -> bool {
        let (a, b): (&CFType, &CFType) = (a, b);
        a == b
    }

    pub fn selection_spot() -> Option<(u64, (isize, isize), String)> {
        use std::hash::{Hash, Hasher};
        let field = focused_field()?;
        let text = unsafe { string_attribute(&field, "AXSelectedText")? };
        let range = selected_range(&field)?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        field.hash(&mut hasher); // CFHash: the same element hashes the same
        Some((hasher.finish(), (range.location, range.length), text))
    }

    pub fn no_text_box() -> bool {
        // Roles that never take typing. Anything else (a group, a window, no
        // focus at all) may be an app hiding its text box, so it's pasted to.
        const NOT_TEXT: [&str; 15] = [
            "AXWebArea",
            "AXLink",
            "AXButton",
            "AXImage",
            "AXList",
            "AXOutline",
            "AXTable",
            "AXBrowser",
            "AXCheckBox",
            "AXRadioButton",
            "AXPopUpButton",
            "AXMenuButton",
            "AXTabGroup",
            "AXSlider",
            "AXStaticText",
        ];
        let Some(field) = focused_field() else {
            return false;
        };
        unsafe {
            let role = string_attribute(&field, "AXRole");
            if !role.is_some_and(|r| NOT_TEXT.contains(&r.as_str())) {
                return false;
            }
            // A page you can type into (contenteditable) is still a text box.
            if attribute(&field, "AXEditableAncestor").is_some() {
                return false;
            }
            let mut settable: u8 = 0;
            field.is_attribute_settable(
                &CFString::from_str("AXValue"),
                NonNull::from(&mut settable),
            );
            settable == 0
        }
    }

    pub fn expose_text(pid: i32) {
        unsafe {
            let app = AXUIElement::new_application(pid);
            app.set_attribute_value(
                &CFString::from_str("AXManualAccessibility"),
                CFBoolean::new(true),
            );
        }
    }

    pub fn window_titles(pid: i32) -> Vec<String> {
        unsafe {
            let app = AXUIElement::new_application(pid);
            let Some(windows) =
                attribute(&app, "AXWindows").and_then(|w| w.downcast::<CFArray>().ok())
            else {
                return Vec::new();
            };
            (0..windows.count())
                .filter_map(|i| {
                    let window = windows.value_at_index(i) as *const AXUIElement;
                    string_attribute(window.as_ref()?, "AXTitle")
                })
                .collect()
        }
    }

    /// The selection (or cursor, at length 0), in UTF-16 units.
    fn selected_range(field: &AXUIElement) -> Option<CFRange> {
        let mut range = CFRange {
            location: 0,
            length: 0,
        };
        unsafe {
            let value = attribute(field, "AXSelectedTextRange")?
                .downcast::<AXValue>()
                .ok()?;
            value
                .value(AXValueType::CFRange, NonNull::from(&mut range).cast())
                .then_some(range)
        }
    }

    pub fn char_before_cursor() -> Option<char> {
        let (field, text) = focused_text()?;
        let range = selected_range(&field)?;
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
