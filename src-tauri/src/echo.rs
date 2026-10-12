//! Owl: takes the call's sound out of the microphone in meetings, so without
//! earphones the other side doesn't show up as "You". LocalVQE v1.4-AEC
//! (Apache-2.0, https://github.com/localai-org/LocalVQE), a 3 MB model on the
//! CPU, is fed 16 ms at a time with the call audio as the reference. The
//! library is built by `scripts/build_localvqe.sh`; without it meetings skip
//! echo removal.

use anyhow::{anyhow, Result};
use libloading::Library;
use std::ffi::{c_char, c_int, CString};
use std::path::Path;

/// Samples per LocalVQE step (16 ms at 16 kHz).
const HOP: usize = 256;
/// How far the call audio may lag the microphone (0.5 s) before the gap is
/// treated as silence, so live lines never wait on a stalled system tap.
const MAX_LAG: usize = 8_000;
const MODEL: &str = "localvqe-v1.4-aec-200K-f32.gguf";
#[cfg(target_os = "macos")]
const LIBRARY: &str = "liblocalvqe.dylib";
#[cfg(target_os = "windows")]
const LIBRARY: &str = "localvqe.dll";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const LIBRARY: &str = "liblocalvqe.so";

type Process = unsafe extern "C" fn(usize, *const f32, *const f32, c_int, *mut f32) -> c_int;
type Free = unsafe extern "C" fn(usize);

struct LocalVqe {
    ctx: usize,
    process: Process,
    free: Free,
    // Keeps the functions above valid; dropped after `free` runs.
    _library: Library,
}

// The context is only ever used by the thread that owns this value.
unsafe impl Send for LocalVqe {}

impl LocalVqe {
    /// Loads the library and model from `dir` (`resources/localvqe`).
    fn load(dir: &Path) -> Result<Self> {
        type OptionsNew = unsafe extern "C" fn() -> usize;
        type SetPath = unsafe extern "C" fn(usize, *const c_char) -> c_int;
        type SetThreads = unsafe extern "C" fn(usize, c_int) -> c_int;
        type NewWith = unsafe extern "C" fn(usize) -> usize;
        type OptionsFree = unsafe extern "C" fn(usize);

        let model = CString::new(dir.join(MODEL).to_string_lossy().as_bytes())?;
        // SAFETY: the symbols are LocalVQE's C API (localvqe_api.h) with the
        // signatures above; the library outlives every call through `_library`.
        unsafe {
            let library = Library::new(dir.join(LIBRARY))?;
            let options_new = *library.get::<OptionsNew>(b"localvqe_options_new\0")?;
            let set_path = *library.get::<SetPath>(b"localvqe_options_set_model_path\0")?;
            let set_threads = *library.get::<SetThreads>(b"localvqe_options_set_threads\0")?;
            let new_with = *library.get::<NewWith>(b"localvqe_new_with_options\0")?;
            let options_free = *library.get::<OptionsFree>(b"localvqe_options_free\0")?;
            let process = *library.get::<Process>(b"localvqe_process_frame_f32\0")?;
            let free = *library.get::<Free>(b"localvqe_free\0")?;

            let options = options_new();
            set_path(options, model.as_ptr());
            // One thread: 16 ms steps are tiny, and transcription needs the CPU.
            set_threads(options, 1);
            let ctx = new_with(options);
            options_free(options);
            if ctx == 0 {
                return Err(anyhow!("LocalVQE couldn't load {MODEL}"));
            }
            Ok(Self {
                ctx,
                process,
                free,
                _library: library,
            })
        }
    }

    fn process(&mut self, mic: &[f32], reference: &[f32], out: &mut [f32]) -> bool {
        // SAFETY: all three slices hold exactly HOP samples.
        unsafe {
            (self.process)(
                self.ctx,
                mic.as_ptr(),
                reference.as_ptr(),
                HOP as c_int,
                out.as_mut_ptr(),
            ) == 0
        }
    }
}

impl Drop for LocalVqe {
    fn drop(&mut self) {
        // SAFETY: `ctx` came from `localvqe_new_with_options` and is freed once.
        unsafe { (self.free)(self.ctx) }
    }
}

/// Lines the microphone up with the call audio and removes the echo. Without
/// LocalVQE (or without call audio) the microphone passes through unchanged.
pub struct EchoStage {
    vqe: Option<LocalVqe>,
    mic: Vec<f32>,
    reference: Vec<f32>,
}

impl EchoStage {
    /// `dir` holds the library and model; `None` turns echo removal off.
    pub fn new(dir: Option<&Path>) -> Self {
        let vqe = dir.and_then(|dir| match LocalVqe::load(dir) {
            Ok(vqe) => Some(vqe),
            Err(e) => {
                log::warn!("Meeting: no echo removal: {e}");
                None
            }
        });
        Self {
            vqe,
            mic: Vec::new(),
            reference: Vec::new(),
        }
    }

    /// Adds 16 kHz audio from both tracks; returns the microphone, without
    /// the echo, as far as both tracks have arrived.
    pub fn push(&mut self, mic: &[f32], reference: &[f32]) -> Vec<f32> {
        self.mic.extend_from_slice(mic);
        let Some(vqe) = self.vqe.as_mut() else {
            return std::mem::take(&mut self.mic);
        };
        self.reference.extend_from_slice(reference);
        // A late call track counts as silence; one far ahead loses its oldest part.
        if self.mic.len() > self.reference.len() + MAX_LAG {
            self.reference.resize(self.mic.len() - MAX_LAG, 0.0);
        } else if self.reference.len() > self.mic.len() + MAX_LAG {
            self.reference
                .drain(..self.reference.len() - self.mic.len() - MAX_LAG);
        }
        let ready = self.mic.len().min(self.reference.len()) / HOP * HOP;
        let mut out = vec![0.0; ready];
        for i in (0..ready).step_by(HOP) {
            let range = i..i + HOP;
            if !vqe.process(
                &self.mic[range.clone()],
                &self.reference[range.clone()],
                &mut out[range.clone()],
            ) {
                out[range.clone()].copy_from_slice(&self.mic[range]);
            }
        }
        self.mic.drain(..ready);
        self.reference.drain(..ready);
        out
    }

    /// The rest of the microphone, at the end of a recording.
    pub fn finish(&mut self) -> Vec<f32> {
        self.reference.clear();
        std::mem::take(&mut self.mic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/localvqe")
    }

    #[test]
    fn without_localvqe_the_microphone_passes_through() {
        let mut stage = EchoStage::new(None);
        assert_eq!(stage.push(&[0.1, 0.2], &[0.5; 2]), vec![0.1, 0.2]);
        assert!(stage.finish().is_empty());
    }

    /// Needs `scripts/build_localvqe.sh`: `cargo test --lib echo -- --ignored`.
    #[test]
    #[ignore]
    fn localvqe_removes_a_delayed_echo_and_keeps_up_in_real_time() {
        let mut stage = EchoStage::new(Some(&dir()));
        assert!(stage.vqe.is_some());
        let n = 16_000 * 4;
        let far: Vec<f32> = (0..n)
            .map(|i| 0.3 * ((i as f32 * 0.031).sin() + (i as f32 * 0.0073).sin()))
            .collect();
        let delay = 640; // 40 ms between the speakers and the microphone
        let mic: Vec<f32> = (0..n)
            .map(|i| {
                if i >= delay {
                    0.6 * far[i - delay]
                } else {
                    0.0
                }
            })
            .collect();
        let started = std::time::Instant::now();
        let mut out = Vec::new();
        for (m, r) in mic.chunks(800).zip(far.chunks(800)) {
            out.extend(stage.push(m, r));
        }
        let elapsed = started.elapsed().as_secs_f32();
        let rms = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
        // After a second to adapt, the echo is at least 20 dB quieter.
        let (before, after) = (rms(&mic[16_000..]), rms(&out[16_000..]));
        assert!(after < before / 10.0, "echo {before} -> {after}");
        assert!(elapsed < 2.0, "4 s of audio took {elapsed} s");
    }
}
