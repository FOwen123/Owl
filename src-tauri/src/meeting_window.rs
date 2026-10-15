//! Owl: the floating notetaker window (`src/meeting/`). During a meeting it
//! shows the live transcript with Stop always in reach; when a call seems to
//! start it asks whether to take notes (`meeting_detect.rs`). Like Wispr
//! Flow's, it's hidden from screen sharing and screen recordings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewUrl};

use crate::managers::meeting::MeetingManager;

#[cfg(target_os = "macos")]
use tauri_nspanel::{
    tauri_panel, CollectionBehavior, ManagerExt, PanelBuilder, PanelLevel, StyleMask,
};

// A panel, like the recording overlay's, so it can sit over a call that's in
// full screen; an ordinary window stays on its own desktop.
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

/// Shows the window as the live transcript (`live`) or the "Take notes?" prompt.
pub fn show(app: &AppHandle, live: bool) {
    // Panels are only safe to build and show from the main thread.
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || show_on_main(&handle, live));
}

fn show_on_main(app: &AppHandle, live: bool) {
    let (mode, height) = if live {
        ("live", LIVE_HEIGHT)
    } else {
        ("prompt", PROMPT_HEIGHT)
    };
    if app.get_webview_window(LABEL).is_some() {
        let _ = app.emit_to(LABEL, "meeting-window-mode", mode);
    } else if let Err(e) = build(app, height) {
        return log::error!("Meeting window: can't open: {e}");
    }
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    let _ = window.set_size(LogicalSize::new(WIDTH, height));
    // The question sits where dictation's overlay does; the transcript top right.
    let spot = if live {
        top_right(app)
    } else {
        crate::overlay::calculate_overlay_position(app, WIDTH, height)
    };
    if let Some((x, y)) = spot {
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
    }
    // Ordering the panel front leaves a full-screen call on screen; a plain
    // show would switch to Owl's desktop.
    #[cfg(target_os = "macos")]
    if let Ok(panel) = app.get_webview_panel(LABEL) {
        return panel.show();
    }
    let _ = window.show();
}

pub fn hide(app: &AppHandle) {
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
/// page, the menu bar, the prompt) and closes it when it stops.
pub fn sync(app: &AppHandle) {
    let recording = app
        .try_state::<Arc<MeetingManager>>()
        .is_some_and(|m| m.active_id().is_some());
    if WAS_RECORDING.swap(recording, Ordering::SeqCst) != recording {
        if recording {
            show(app, true);
        } else {
            hide(app);
        }
    }
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
        .collection_behavior(
            CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary(),
        )
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
