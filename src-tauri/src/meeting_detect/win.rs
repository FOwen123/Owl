//! Windows: which apps hold the microphone, from the list Windows keeps for
//! its microphone-in-use icon, and the browsers' window titles.

use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// The macOS list's call apps on Windows: lowercase program names, or the
/// package name of a Store app (new Teams).
pub const CALL_APPS: &[&str] = &[
    "ciscocollabhost.exe",
    "atmgr.exe",
    "webex.exe",
    "gather.exe",
    "lark.exe",
    "feishu.exe",
    "ms-teams.exe",
    "msteams_",
    "teams.exe",
    "wemeetapp.exe",
    "zoom.exe",
];

pub const BROWSERS: &[&str] = &[
    "chrome.exe",
    "chromium.exe",
    "msedge.exe",
    "brave.exe",
    "firefox.exe",
    "opera.exe",
    "vivaldi.exe",
    "arc.exe",
    "thebrowsercompany.arc",
    "zen.exe",
    "comet.exe",
];

const CONSENT_STORE: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

/// Lowercase names of the apps recording from a microphone right now.
pub fn mic_holders() -> Vec<String> {
    let Ok(store) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(CONSENT_STORE) else {
        return Vec::new();
    };
    // Store apps sit under their package name, desktop apps under
    // NonPackaged by path, with # for \.
    let mut holders: Vec<String> = in_use(&store).map(|name| name.to_lowercase()).collect();
    if let Ok(desktop) = store.open_subkey("NonPackaged") {
        holders.extend(
            in_use(&desktop).map(|path| path.rsplit('#').next().unwrap_or("").to_lowercase()),
        );
    }
    holders
}

/// The apps under `key` using the microphone: started and not yet stopped.
fn in_use(key: &RegKey) -> impl Iterator<Item = String> + '_ {
    key.enum_keys().flatten().filter(|name| {
        key.open_subkey(name)
            .and_then(|app| app.get_value::<u64, _>("LastUsedTimeStop"))
            .is_ok_and(|stop| stop == 0)
    })
}

/// Whether a browser window shows Google Meet, Zoom or Teams. A browser's
/// window title is its open tab's.
pub fn call_tab_open() -> bool {
    let mut windows: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut windows as *mut Vec<HWND> as isize),
        );
    }
    windows.into_iter().any(|window| unsafe {
        if !IsWindowVisible(window).as_bool() {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if !program(pid).is_some_and(|name| super::one_of(BROWSERS, &name)) {
            return false;
        }
        let mut title = [0u16; 512];
        let len = GetWindowTextW(window, &mut title);
        super::is_call_title(&String::from_utf16_lossy(&title[..len.max(0) as usize]))
    })
}

unsafe extern "system" fn collect(window: HWND, list: LPARAM) -> BOOL {
    (*(list.0 as *mut Vec<HWND>)).push(window);
    true.into()
}

/// A process's lowercase program name ("chrome.exe").
fn program(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let read = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(path.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        read.ok()?;
        let path = String::from_utf16_lossy(&path[..len as usize]);
        Some(path.rsplit('\\').next()?.to_lowercase())
    }
}
