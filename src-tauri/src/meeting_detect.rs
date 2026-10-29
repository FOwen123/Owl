//! Owl: notices a call starting and asks whether to take notes, and the call
//! ending, then stops the notes unless Resume is pressed. Like Wispr Flow: a
//! call app on the microphone, or a browser on it while a window shows Meet,
//! Zoom or Teams; dictation apps never count. Reads Core Audio's per-app
//! list, macOS 14.4+; on older systems nothing is ever asked or stopped.
//! On Windows, reads the list behind its microphone-in-use icon (`win.rs`).

#[cfg(target_os = "macos")]
use std::mem::size_of;
#[cfg(target_os = "macos")]
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use objc2_app_kit::NSWorkspace;
#[cfg(target_os = "macos")]
use objc2_core_audio::{
    kAudioHardwarePropertyProcessObjectList, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioProcessPropertyBundleID,
    kAudioProcessPropertyIsRunningInput, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
};
#[cfg(target_os = "macos")]
use objc2_core_foundation::{CFRetained, CFString};
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
use crate::audio_toolkit::audio::system_capture::get_property;
#[cfg(target_os = "macos")]
use crate::correction_watch::window_titles;
use crate::managers::meeting::MeetingManager;
use crate::meeting_window;
use crate::settings::get_settings;

const POLL: Duration = Duration::from_secs(2);
/// Polls in a row with a call on before asking, or off before it counts as
/// over (4–6 s).
const ASK_AFTER: u32 = 3;
/// How long "Meeting finished" waits for Resume before the notes stop.
const GRACE: Duration = Duration::from_secs(8);

/// The meeting being recorded, as far as its call goes.
struct Watch {
    /// Which meeting; a new one starts the watch over.
    id: Option<i64>,
    /// A call has been on the microphone: only then can it ending stop the
    /// notes (an in-person meeting never stops by itself).
    in_call: bool,
    /// Polls in a row the call has been off.
    off: u32,
    /// When "Meeting finished" showed, while it waits for Resume.
    ending: Option<Instant>,
}

const IDLE: Watch = Watch {
    id: None,
    in_call: false,
    off: 0,
    ending: None,
};

static WATCH: Mutex<Watch> = Mutex::new(IDLE);

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // Polls in a row a call has had the microphone; at ASK_AFTER or
        // more this call has been asked about (or is being recorded).
        let mut held = 0;
        loop {
            std::thread::sleep(POLL);
            let meetings = app.state::<Arc<MeetingManager>>();
            if let Some(id) = meetings.active_id() {
                held = ASK_AFTER;
                watch_end(&app, &meetings, id);
                continue;
            }
            *WATCH.lock().unwrap() = IDLE;
            if !get_settings(&app).meeting_detection {
                // Turned off while asking: take the question away and start
                // fresh, so the next call is asked about once it's on again.
                if held >= ASK_AFTER {
                    meeting_window::hide(&app);
                }
                held = 0;
                continue;
            }
            if call_on_mic(false) {
                held += 1;
                if held == ASK_AFTER {
                    meeting_window::show(&app, "prompt");
                }
            } else {
                // The call ended without notes: take the question away.
                if held >= ASK_AFTER {
                    meeting_window::hide(&app);
                }
                held = 0;
            }
        }
    });
}

/// Shows "Meeting finished" once the call has been off a few polls, and
/// stops the notes when nobody pressed Resume in time.
fn watch_end(app: &AppHandle, meetings: &Arc<MeetingManager>, id: i64) {
    let in_call = {
        let mut watch = WATCH.lock().unwrap();
        // Stopped and started again between polls: the old meeting's watch
        // doesn't carry over.
        if watch.id != Some(id) {
            *watch = Watch {
                id: Some(id),
                ..IDLE
            };
        }
        watch.in_call
    };
    let on = call_on_mic(in_call);
    let mut watch = WATCH.lock().unwrap();
    if on {
        watch.in_call = true;
        watch.off = 0;
        // The call is back (it only dropped for a moment): carry on.
        if watch.ending.take().is_some() {
            meeting_window::show(app, "live");
        }
    } else if let Some(since) = watch.ending {
        if since.elapsed() >= GRACE {
            watch.ending = None;
            watch.in_call = false;
            drop(watch);
            if let Err(e) = meetings.stop() {
                log::warn!("Meeting: can't stop after the call ended: {e}");
            }
        }
    } else if watch.in_call {
        watch.off += 1;
        if watch.off == ASK_AFTER {
            watch.ending = Some(Instant::now());
            meeting_window::show(app, "ended");
        }
    }
}

/// Whether "Meeting finished" is up, counting down.
pub fn ending() -> bool {
    WATCH.lock().unwrap().ending.is_some()
}

/// Resume on "Meeting finished": the notes carry on, and stop by themselves
/// again only after a call comes back and ends.
pub fn resume(app: &AppHandle) {
    let mut watch = WATCH.lock().unwrap();
    if watch.ending.take().is_some() {
        watch.in_call = false;
        watch.off = 0;
        meeting_window::show(app, "live");
    }
}

#[cfg(target_os = "windows")]
mod win;
#[cfg(target_os = "windows")]
use win::{call_tab_open, mic_holders, BROWSERS, CALL_APPS};

/// Apps that only use the microphone for calls, as Wispr Flow lists them.
/// Lowercase bundle ids; helper processes share the prefix.
#[cfg(target_os = "macos")]
const CALL_APPS: &[&str] = &[
    "com.apple.facetime",
    "com.apple.avconferenced",
    "com.cisco.webexmeetingsapp",
    "cisco-systems.spark",
    "com.electron.uberconference",
    "com.gather.gather",
    "com.larksuite.larkapp",
    "com.microsoft.teams",
    "com.tencent.tencentmeeting",
    "us.zoom.xos",
];

/// Browsers count only while a window shows a call (`call_tab_open`).
#[cfg(target_os = "macos")]
const BROWSERS: &[&str] = &[
    "com.google.chrome",
    "org.chromium.chromium",
    "com.apple.safari",
    "com.apple.webkit.gpu",
    "company.thebrowser.browser",
    "company.thebrowser.dia",
    "com.brave.browser",
    "com.microsoft.edgemac",
    "org.mozilla.firefox",
    "com.operasoftware.opera",
    "com.vivaldi.vivaldi",
    "app.zen-browser.zen",
    "ai.perplexity.comet",
    "com.openai.atlas",
];

fn one_of(list: &[&str], bundle: &str) -> bool {
    list.iter().any(|prefix| bundle.starts_with(prefix))
}

/// Whether a call is on the microphone.
/// `in_call`: a call is being recorded, so any browser on the microphone
/// still counts; another tab shown over the call hides its title.
fn call_on_mic(in_call: bool) -> bool {
    let holders = mic_holders();
    holders.iter().any(|b| one_of(CALL_APPS, b))
        || (holders.iter().any(|b| one_of(BROWSERS, b)) && (in_call || call_tab_open()))
}

/// Lowercase bundle ids of the apps recording from a microphone.
#[cfg(target_os = "macos")]
fn mic_holders() -> Vec<String> {
    let processes = unsafe { process_objects() };
    processes
        .into_iter()
        .filter(|&p| unsafe {
            get_property::<u32>(p, kAudioProcessPropertyIsRunningInput, None, 0) == Ok(1)
        })
        .filter_map(|p| unsafe { bundle_id(p) })
        .collect()
}

#[cfg(target_os = "macos")]
unsafe fn bundle_id(process: AudioObjectID) -> Option<String> {
    let id = get_property::<*const CFString>(
        process,
        kAudioProcessPropertyBundleID,
        None,
        std::ptr::null(),
    )
    .ok()?;
    // Core Audio hands over a +1 reference.
    let id = CFRetained::from_raw(NonNull::new(id as *mut CFString)?);
    Some(id.to_string().to_lowercase())
}

/// Whether a browser window shows Google Meet, Zoom or Teams.
#[cfg(target_os = "macos")]
fn call_tab_open() -> bool {
    NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|app| {
            app.bundleIdentifier()
                .is_some_and(|id| one_of(BROWSERS, &id.to_string().to_lowercase()))
        })
        .any(|app| {
            window_titles(app.processIdentifier())
                .iter()
                .any(|t| is_call_title(t))
        })
}

// shortcut: reads each window's open tab title, where Wispr Flow reads the
// tab's address; switch to the address if a call tab turns out unnamed.
fn is_call_title(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    t.starts_with("meet - ")
        || t.starts_with("meet – ")
        || t.contains("google meet")
        || t.contains("zoom meeting")
        || t == "zoom"
        || t.contains("microsoft teams")
}

/// Every app Core Audio knows about; empty when the list can't be read.
#[cfg(target_os = "macos")]
unsafe fn process_objects() -> Vec<AudioObjectID> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyProcessObjectList,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let system = kAudioObjectSystemObject as AudioObjectID;
    let mut size = 0u32;
    let status = AudioObjectGetPropertyDataSize(
        system,
        NonNull::from(&address),
        0,
        std::ptr::null(),
        NonNull::from(&mut size),
    );
    if status != 0 || size == 0 {
        return Vec::new();
    }
    let mut ids: Vec<AudioObjectID> = vec![0; size as usize / size_of::<AudioObjectID>()];
    let status = AudioObjectGetPropertyData(
        system,
        NonNull::from(&address),
        0,
        std::ptr::null(),
        NonNull::from(&mut size),
        NonNull::from(ids.as_mut_slice()).cast(),
    );
    if status != 0 {
        return Vec::new();
    }
    ids.truncate(size as usize / size_of::<AudioObjectID>());
    ids
}
