//! Owl: notices a call starting (another app keeps the microphone on) and
//! asks whether to take notes. Reads Core Audio's per-app list, macOS 14.4+;
//! on older systems the list is missing and nothing is ever asked.

use std::mem::size_of;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::Duration;

use objc2_core_audio::{
    kAudioHardwarePropertyProcessObjectList, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioProcessPropertyIsRunningInput,
    kAudioProcessPropertyPID, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
    AudioObjectID, AudioObjectPropertyAddress,
};
use tauri::{AppHandle, Manager};

use crate::audio_toolkit::audio::system_capture::get_property;
use crate::managers::meeting::MeetingManager;
use crate::meeting_window;
use crate::settings::get_settings;

const POLL: Duration = Duration::from_secs(2);
/// Polls in a row with the microphone on before asking (4–6 s), so a short
/// voice message or dictation doesn't count as a call.
const ASK_AFTER: u32 = 3;

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // Polls in a row another app has had the microphone; at ASK_AFTER or
        // more this call has been asked about (or is being recorded).
        let mut held = 0;
        loop {
            std::thread::sleep(POLL);
            if !get_settings(&app).meeting_detection {
                continue;
            }
            if app.state::<Arc<MeetingManager>>().active_id().is_some() {
                held = ASK_AFTER;
                continue;
            }
            if other_app_using_mic() {
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

/// Whether any app other than Owl is recording from a microphone.
fn other_app_using_mic() -> bool {
    let own = std::process::id() as i32;
    let processes = unsafe { process_objects() };
    processes.into_iter().any(|process| unsafe {
        get_property::<u32>(process, kAudioProcessPropertyIsRunningInput, None, 0) == Ok(1)
            && get_property::<i32>(process, kAudioProcessPropertyPID, None, own) != Ok(own)
    })
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
