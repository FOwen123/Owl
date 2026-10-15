//! Owl: the floating notetaker window (`src/meeting/`). During a meeting it
//! shows the live transcript with Stop always in reach; when a call seems to
//! start it asks whether to take notes (`meeting_detect.rs`). Like Wispr
//! Flow's, it's hidden from screen sharing and screen recordings.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::managers::meeting::MeetingManager;

const LABEL: &str = "meeting";
const WIDTH: f64 = 360.0;
const PROMPT_HEIGHT: f64 = 108.0;
const LIVE_HEIGHT: f64 = 480.0;
/// Distance from the screen's top-right corner when first shown.
const MARGIN: f64 = 16.0;

/// Whether a meeting was being recorded at the last `sync`.
static WAS_RECORDING: AtomicBool = AtomicBool::new(false);

/// Shows the window as the live transcript (`live`) or the "Take notes?" prompt.
pub fn show(app: &AppHandle, live: bool) {
    let (mode, height) = if live {
        ("live", LIVE_HEIGHT)
    } else {
        ("prompt", PROMPT_HEIGHT)
    };
    let window = match app.get_webview_window(LABEL) {
        Some(window) => {
            let _ = app.emit_to(LABEL, "meeting-window-mode", mode);
            window
        }
        None => match build(app, height) {
            Ok(window) => window,
            Err(e) => return log::error!("Meeting window: can't open: {e}"),
        },
    };
    let _ = window.set_size(LogicalSize::new(WIDTH, height));
    let _ = window.show();
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
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

fn build(app: &AppHandle, height: f64) -> tauri::Result<tauri::WebviewWindow> {
    let mut builder =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("src/meeting/index.html".into()))
            .title("Owl")
            .inner_size(WIDTH, height)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            // Clicks work without taking focus from the call.
            .focusable(false)
            .accept_first_mouse(true)
            .content_protected(true)
            .visible(false);

    if let Some(monitor) = app.primary_monitor().ok().flatten() {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let right = (area.position.x as f64 + area.size.width as f64) / scale;
        builder = builder.position(
            right - WIDTH - MARGIN,
            area.position.y as f64 / scale + MARGIN,
        );
    }
    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }
    builder.build()
}
