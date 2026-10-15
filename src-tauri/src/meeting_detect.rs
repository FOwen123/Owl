//! Owl: notices a call starting and asks whether to take notes. Like Wispr
//! Flow: a call app on the microphone, or a browser on it while a window shows
//! Meet, Zoom or Teams; dictation apps never count. Reads Core Audio's per-app
//! list, macOS 14.4+; on older systems nothing is ever asked.

use std::mem::size_of;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::Duration;

use objc2_app_kit::NSWorkspace;
use objc2_core_audio::{
    kAudioHardwarePropertyProcessObjectList, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioProcessPropertyBundleID,
    kAudioProcessPropertyIsRunningInput, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
};
use objc2_core_foundation::{CFRetained, CFString};
use tauri::{AppHandle, Manager};

use crate::audio_toolkit::audio::system_capture::get_property;
use crate::correction_watch::window_titles;
use crate::managers::meeting::MeetingManager;
use crate::meeting_window;
use crate::settings::get_settings;

const POLL: Duration = Duration::from_secs(2);
/// Polls in a row with a call on before asking (4–6 s).
const ASK_AFTER: u32 = 3;

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // Polls in a row a call has had the microphone; at ASK_AFTER or
        // more this call has been asked about (or is being recorded).
        let mut held = 0;
        loop {
            std::thread::sleep(POLL);
            if app.state::<Arc<MeetingManager>>().active_id().is_some() {
                held = ASK_AFTER;
                continue;
            }
            if !get_settings(&app).meeting_detection {
                // Turned off while asking: take the question away and start
                // fresh, so the next call is asked about once it's on again.
                if held >= ASK_AFTER {
                    meeting_window::hide(&app);
                }
                held = 0;
                continue;
            }
            if call_on_mic() {
                held += 1;
                if held == ASK_AFTER {
                    meeting_window::show(&app, false);
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

/// Apps that only use the microphone for calls, as Wispr Flow lists them.
/// Lowercase bundle ids; helper processes share the prefix.
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
fn call_on_mic() -> bool {
    let holders = mic_holders();
    holders.iter().any(|b| one_of(CALL_APPS, b))
        || (holders.iter().any(|b| one_of(BROWSERS, b)) && call_tab_open())
}

/// Lowercase bundle ids of the apps recording from a microphone.
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
