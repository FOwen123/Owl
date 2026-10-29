//! Owl: asks the OS spell checker which words are real, so learning never
//! turns an ordinary word into a rule ("skip" → "keep") and the Words fix-up
//! never rewrites one ("cloud" → "Claude").

use std::collections::HashSet;
use tauri::AppHandle;

/// The words (lowercased) the spell checker knows. `None` when there is no
/// spell checker to ask; callers then keep Owl's behaviour.
pub fn known_words(app: &AppHandle, words: &[String]) -> Option<HashSet<String>> {
    if words.is_empty() {
        return Some(HashSet::new());
    }
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        // Asked on the calling thread, one caller at a time. Dispatching to the
        // main thread stalls whenever it is busy (and always in headless runs),
        // which silently switched the guard off. Chromium checks spelling off
        // the main thread the same way.
        static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        Some(objc2::rc::autoreleasepool(|_| check(words)))
    }
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        check(words)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        None
    }
}

/// The English-looking words in `text`, once each, for `known_words`.
pub fn words_in(texts: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = texts
        .iter()
        .flat_map(|t| t.split(|c: char| !(c.is_ascii_alphanumeric() || c == '\'')))
        .map(|w| w.trim_matches('\''))
        .filter(|w| w.chars().any(|c| c.is_ascii_alphabetic()))
        .map(str::to_string)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(target_os = "macos")]
fn check(words: &[String]) -> HashSet<String> {
    use objc2_app_kit::NSSpellChecker;
    use objc2_foundation::{NSNotFound, NSString};
    let checker = NSSpellChecker::sharedSpellChecker();
    words
        .iter()
        .filter(|w| {
            let range = checker.checkSpellingOfString_startingAt(&NSString::from_str(w), 0);
            range.location == NSNotFound as usize
        })
        .map(|w| w.to_lowercase())
        .collect()
}

/// Windows' own spell checker, in US English. None when that dictionary
/// isn't installed.
#[cfg(target_os = "windows")]
fn check(words: &[String]) -> Option<HashSet<String>> {
    use windows::core::HSTRING;
    use windows::Win32::Globalization::{ISpellCheckerFactory, SpellCheckerFactory};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    unsafe {
        // A thread that already set up COM keeps its own mode.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let factory: ISpellCheckerFactory =
            CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let checker = factory.CreateSpellChecker(&HSTRING::from("en-US")).ok()?;
        Some(
            words
                .iter()
                // Known: the checker finds no error in it.
                .filter(|w| {
                    checker
                        .Check(&HSTRING::from(w.as_str()))
                        .is_ok_and(|errors| {
                            let mut error = None;
                            errors.Next(&mut error).is_ok() && error.is_none()
                        })
                })
                .map(|w| w.to_lowercase())
                .collect(),
        )
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// The real checker. `cargo test --lib spelling -- --ignored`
    #[test]
    #[ignore]
    fn macos_knows_ordinary_words_but_not_brands() {
        let known = check(&["skip".into(), "keep".into(), "Owel".into(), "depick".into()]);
        assert!(known.contains("skip") && known.contains("keep"));
        assert!(!known.contains("owel") && !known.contains("depick"));
    }

    #[test]
    fn words_in_keeps_each_english_word_once() {
        assert_eq!(
            words_in(&["Skip it, skip it", "我用 Claude's 寫"]),
            vec!["Claude's", "Skip", "it", "skip"]
        );
    }
}
