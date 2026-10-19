//! Owl: the floating notetaker window (`src/meeting/`). During a meeting it
//! shows the live transcript with Stop always in reach; when a call seems to
//! start it asks whether to take notes, and when it ends, offers to carry on
//! (`meeting_detect.rs`). Like Wispr Flow's, it's hidden from screen sharing
//! and screen recordings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl};

use crate::managers::meeting::MeetingManager;

#[cfg(target_os = "macos")]
use tauri_nspanel::{
    tauri_panel, CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask,
};

// A panel, like the recording overlay's, so its cards can sit over a call
// that's in full screen; an ordinary window stays on its own desktop.
#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(MeetingPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

const LABEL: &str = "meeting";
const WIDTH: f64 = 360.0;
/// The prompt card is 52 tall (RecordingOverlay.css `.ssug`), with room for its pop.
const PROMPT_HEIGHT: f64 = 64.0;
const LIVE_HEIGHT: f64 = 480.0;
/// Distance from the screen's top-right corner when first shown.
const MARGIN: f64 = 16.0;

/// Whether a meeting was being recorded at the last `sync`.
static WAS_RECORDING: AtomicBool = AtomicBool::new(false);
/// Whether the window shows the transcript (rather than a card).
static SHOWING_LIVE: AtomicBool = AtomicBool::new(false);
/// Where the transcript was when "Meeting finished" took its place, for Resume.
static LIVE_SPOT: Mutex<Option<PhysicalPosition<i32>>> = Mutex::new(None);

/// Builds the window hidden at startup, as the recording overlay does: a
/// panel built and shown in one go doesn't appear the first time.
pub fn create(app: &AppHandle) {
    if let Err(e) = build(app, PROMPT_HEIGHT) {
        log::error!("Meeting window: can't create: {e}");
    }
    #[cfg(target_os = "macos")]
    if let Ok(panel) = app.get_webview_panel(LABEL) {
        panel.hide();
    }
}

/// Shows the window as `mode`: the live transcript (`"live"`), the "Take
/// notes?" card (`"prompt"`) or the "Meeting finished" card (`"ended"`).
pub fn show(app: &AppHandle, mode: &'static str) {
    // Panels are only safe to build and show from the main thread.
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || show_on_main(&handle, mode));
}

fn show_on_main(app: &AppHandle, mode: &'static str) {
    let live = mode == "live";
    let height = if live { LIVE_HEIGHT } else { PROMPT_HEIGHT };
    if app.get_webview_window(LABEL).is_some() {
        let _ = app.emit_to(LABEL, "meeting-window-mode", mode);
    } else if let Err(e) = build(app, height) {
        return log::error!("Meeting window: can't open: {e}");
    }
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if mode == "ended" && SHOWING_LIVE.load(Ordering::SeqCst) {
        *LIVE_SPOT.lock().unwrap() = window.outer_position().ok();
    }
    SHOWING_LIVE.store(live, Ordering::SeqCst);
    let _ = window.set_size(LogicalSize::new(WIDTH, height));
    // The cards sit where dictation's overlay does; the transcript top right,
    // or back where it was before "Meeting finished".
    let back = live.then(|| LIVE_SPOT.lock().unwrap().take()).flatten();
    let spot = if live {
        top_right(app)
    } else {
        crate::overlay::calculate_overlay_position(app, WIDTH, height)
    };
    if let Some(at) = back {
        let _ = window.set_position(at);
    } else if let Some((x, y)) = spot {
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    }
    #[cfg(target_os = "macos")]
    if let Ok(panel) = app.get_webview_panel(LABEL) {
        // The cards float over everything, on every desktop. The transcript
        // is an ordinary window, like Wispr Flow's: it joins every desktop
        // just long enough to open on this one, then stays there, so it can
        // be moved to another.
        let level = if live {
            PanelLevel::Normal
        } else {
            PanelLevel::Status
        };
        panel.set_level(level.value());
        panel.set_collection_behavior(every_desktop().value());
        // Ordering the panel front leaves a full-screen call on screen; a
        // plain show would switch to Owl's desktop.
        panel.show();
        if live {
            let handle = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let main = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    if !SHOWING_LIVE.load(Ordering::SeqCst) {
                        return;
                    }
                    if let Ok(panel) = main.get_webview_panel(LABEL) {
                        let one = CollectionBehavior::new().managed().full_screen_auxiliary();
                        panel.set_collection_behavior(one.value());
                    }
                });
            });
        }
        return;
    }
    let _ = window.show();
}

#[cfg(target_os = "macos")]
fn every_desktop() -> CollectionBehavior {
    CollectionBehavior::new()
        .can_join_all_spaces()
        .full_screen_auxiliary()
}

pub fn hide(app: &AppHandle) {
    LIVE_SPOT.lock().unwrap().take();
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        if let Ok(panel) = handle.get_webview_panel(LABEL) {
            return panel.hide();
        }
        if let Some(window) = handle.get_webview_window(LABEL) {
            let _ = window.hide();
        }
    });
}

/// Opens the live window when a meeting starts (from anywhere: the Notetaker
/// page, the menu bar, the prompt) and closes it when it stops. Says whether
/// a meeting started or stopped since the last call.
pub fn sync(app: &AppHandle) -> bool {
    let recording = app
        .try_state::<Arc<MeetingManager>>()
        .is_some_and(|m| m.active_id().is_some());
    let changed = WAS_RECORDING.swap(recording, Ordering::SeqCst) != recording;
    if changed {
        if recording {
            show(app, "live");
        } else {
            hide(app);
        }
    }
    changed
}

/// Where the window first appears: the main screen's top-right corner.
fn top_right(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let right = (area.position.x as f64 + area.size.width as f64) / scale;
    Some((
        right - WIDTH - MARGIN,
        area.position.y as f64 / scale + MARGIN,
    ))
}

#[cfg(target_os = "macos")]
fn build(app: &AppHandle, height: f64) -> tauri::Result<()> {
    let mut builder = PanelBuilder::<_, MeetingPanel>::new(app, LABEL)
        .url(WebviewUrl::App("src/meeting/index.html".into()))
        .title("Owl")
        .size(tauri::Size::Logical(LogicalSize::new(WIDTH, height)))
        .level(PanelLevel::Status)
        .transparent(true)
        // Clicks work without taking focus from the call.
        .no_activate(true)
        .style_mask(StyleMask::empty().borderless().nonactivating_panel())
        .collection_behavior(every_desktop())
        .with_window(|w| {
            w.decorations(false)
                .transparent(true)
                .accept_first_mouse(true)
                .skip_taskbar(true)
                .content_protected(true)
                .visible(false)
        });
    if let Some((x, y)) = top_right(app) {
        builder = builder.position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
    }
    builder.build()?;
    // Set again on the finished panel, so screen sharing never sees it.
    if let Some(window) = app.get_webview_window(LABEL) {
        window.set_content_protected(true)?;
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn build(app: &AppHandle, height: f64) -> tauri::Result<()> {
    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App("src/meeting/index.html".into()),
    )
    .title("Owl")
    .inner_size(WIDTH, height)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .visible_on_all_workspaces(true)
    .skip_taskbar(true)
    .focusable(false)
    .accept_first_mouse(true)
    .content_protected(true)
    .visible(false);
    if let Some((x, y)) = top_right(app) {
        builder = builder.position(x, y);
    }
    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }
    builder.build().map(|_| ())
}
