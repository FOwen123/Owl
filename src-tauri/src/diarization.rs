//! Owl: who spoke when in a meeting's call audio. NVIDIA's Sortformer v2.1
//! (up to 4 speakers) runs in a short-lived copy of Owl started with
//! `--owl-diarize`, so a crash there (a GPU driver, a bad model) only ends
//! that process. The model is downloaded once, on request.

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use specta::Type;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// First argument that turns Owl into a one-shot speaker-labelling process:
/// `owl --owl-diarize <model> <16 kHz wav> <output json>`.
pub const WORKER_FLAG: &str = "--owl-diarize";
/// Long enough for a long meeting on the CPU (about 25x real time).
const WORKER_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Pinned so the bytes always match `SHA256`.
const URL: &str = "https://huggingface.co/handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf/resolve/ae4afbb5c3d33b71cf2dbf600022b655ee706dd0/diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf";
const FILE: &str = "diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf";
pub const SIZE: u64 = 139_310_336;
const SHA256: &str = "a5dacdc650790266c7a362e54e6bf51952015487edaa606c4e11632bc32442a9";

/// A part of a meeting spoken by one person. Times in ms; `speaker` counts
/// from 1 in the order people first speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker: u32,
}

/// Download progress, for the Notetaker page.
#[derive(Serialize, Clone, Type)]
pub struct SpeakerModelProgress {
    pub downloaded: u64,
    pub total: u64,
}

fn model_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(crate::portable::app_data_dir(app)?
        .join("models")
        .join("speakers")
        .join(FILE))
}

pub fn is_installed(app: &AppHandle) -> bool {
    model_path(app)
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .is_some_and(|m| m.len() == SIZE)
}

/// Downloads the speaker model, checking its SHA-256 before it's used.
pub async fn download(app: &AppHandle) -> Result<()> {
    let path = model_path(app)?;
    std::fs::create_dir_all(path.parent().expect("model path has a folder"))?;
    let partial = path.with_extension("part");
    let fetched = fetch(app, &partial).await;
    if fetched.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    fetched?;
    std::fs::rename(&partial, &path)?;
    Ok(())
}

/// Downloads the model to `partial`: never more than its known size, giving
/// up when the server stalls for a minute, and checked against its hash.
async fn fetch(app: &AppHandle, partial: &Path) -> Result<()> {
    let damaged = || anyhow!("The speaker model download was damaged; try again.");
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .build()?;
    let response = client.get(URL).send().await?.error_for_status()?;
    let mut file = std::fs::File::create(partial)?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        if downloaded > SIZE {
            return Err(damaged());
        }
        file.write_all(&chunk)?;
        hasher.update(&chunk);
        let _ = app.emit(
            "speaker-model-progress",
            SpeakerModelProgress {
                downloaded,
                total: SIZE,
            },
        );
    }
    file.flush()?;
    if downloaded != SIZE || format!("{:x}", hasher.finalize()) != SHA256 {
        return Err(damaged());
    }
    Ok(())
}

/// Who spoke when in the 16 kHz mono WAV at `wav`.
pub fn diarize(app: &AppHandle, wav: &Path) -> Result<Vec<Turn>> {
    let output = wav.with_extension("speakers.json");
    let _ = std::fs::remove_file(&output);
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg(WORKER_FLAG)
        .arg(model_path(app)?)
        .arg(wav)
        .arg(&output)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > WORKER_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("Speaker labels took too long"));
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    if !status.success() {
        return Err(anyhow!("Speaker labels failed ({status})"));
    }
    let raw: Vec<(i64, i64, u32)> = serde_json::from_slice(&std::fs::read(&output)?)?;
    let _ = std::fs::remove_file(&output);
    let mut turns: Vec<Turn> = raw
        .into_iter()
        .filter(|&(start, end, speaker)| end > start && speaker > 0)
        .map(|(start_ms, end_ms, speaker)| Turn {
            start_ms,
            end_ms,
            speaker,
        })
        .collect();
    turns.sort_by_key(|t| t.start_ms);
    Ok(renumber(turns))
}

/// The `--owl-diarize` process: runs the model and writes
/// `[[start_ms, end_ms, speaker], …]` to the output file. Returns the exit code.
pub fn run_worker() -> i32 {
    let args: Vec<std::path::PathBuf> = std::env::args_os().skip(2).map(Into::into).collect();
    let [model, wav, output] = args.as_slice() else {
        eprintln!("usage: {WORKER_FLAG} <model> <wav> <output>");
        return 2;
    };
    let result = (|| -> Result<()> {
        transcribe_cpp::init_logging();
        transcribe_cpp::init_backends_default()?;
        let model = transcribe_cpp::Model::load(model)?;
        let mut session = model.diarize_session()?;
        let pcm = crate::audio_toolkit::audio::read_wav_samples(wav)?;
        let segments = session.run(&pcm, &transcribe_cpp::DiarizeOptions::default())?;
        let turns: Vec<(i64, i64, i32)> = segments
            .iter()
            .map(|s| (s.t0_ms, s.t1_ms, s.speaker_id))
            .collect();
        std::fs::write(output, serde_json::to_vec(&turns)?)?;
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    }
}

/// Speakers counted from 1 in the order they first speak.
fn renumber(mut turns: Vec<Turn>) -> Vec<Turn> {
    let mut order: Vec<u32> = Vec::new();
    for t in &mut turns {
        let n = match order.iter().position(|&s| s == t.speaker) {
            Some(i) => i,
            None => {
                order.push(t.speaker);
                order.len() - 1
            }
        };
        t.speaker = n as u32 + 1;
    }
    turns
}

/// A part shorter than this is counted as the neighbouring speaker's
/// ("mm-hm" over someone else), so lines aren't chopped into scraps.
const MIN_PART_MS: i64 = 1_000;
const STEP_MS: i64 = 100;

/// Who speaks in `[start, end)`: one `(start, end, speaker)` per stretch,
/// so a line where the speaker changes can be split. A line no turn covers
/// goes to the nearest speaker; empty when there are no turns at all.
pub fn speaker_parts(start: i64, end: i64, turns: &[Turn]) -> Vec<(i64, i64, u32)> {
    if turns.is_empty() {
        return Vec::new();
    }
    let nearby: Vec<&Turn> = turns
        .iter()
        .filter(|t| t.end_ms > start && t.start_ms < end)
        .collect();
    if nearby.is_empty() || end <= start {
        let distance = |t: &&Turn| (t.start_ms - end).max(start - t.end_ms).max(0);
        let nearest = turns
            .iter()
            .min_by_key(distance)
            .expect("turns isn't empty");
        return vec![(start, end.max(start), nearest.speaker)];
    }

    // Who is talking every 100 ms; while two overlap, the one already
    // talking keeps the line.
    let mut steps: Vec<u32> = Vec::new();
    let mut t = start;
    while t < end {
        let mid = t + STEP_MS / 2;
        let talking: Vec<u32> = nearby
            .iter()
            .filter(|x| x.start_ms <= mid && mid < x.end_ms)
            .map(|x| x.speaker)
            .collect();
        let current = steps.last().copied();
        steps.push(match current {
            Some(c) if talking.contains(&c) || talking.is_empty() => c,
            _ => talking.first().copied().unwrap_or(nearby[0].speaker),
        });
        t += STEP_MS;
    }

    let mut parts: Vec<(i64, i64, u32)> = Vec::new();
    for (i, &speaker) in steps.iter().enumerate() {
        let (s, e) = (
            start + i as i64 * STEP_MS,
            (start + (i as i64 + 1) * STEP_MS).min(end),
        );
        match parts.last_mut() {
            Some(last) if last.2 == speaker => last.1 = e,
            _ => parts.push((s, e, speaker)),
        }
    }
    // Fold short parts into a neighbour, then join neighbours that now match.
    while let Some(i) = parts.iter().position(|p| p.1 - p.0 < MIN_PART_MS) {
        if parts.len() == 1 {
            break;
        }
        let short = parts.remove(i);
        if i > 0 {
            parts[i - 1].1 = short.1;
        } else {
            parts[0].0 = short.0;
        }
        let mut joined: Vec<(i64, i64, u32)> = Vec::new();
        for p in parts.drain(..) {
            match joined.last_mut() {
                Some(last) if last.2 == p.2 => last.1 = p.1,
                _ => joined.push(p),
            }
        }
        parts = joined;
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(start_ms: i64, end_ms: i64, speaker: u32) -> Turn {
        Turn {
            start_ms,
            end_ms,
            speaker,
        }
    }

    #[test]
    fn speakers_are_numbered_by_first_appearance() {
        let turns = renumber(vec![turn(0, 10, 3), turn(10, 20, 1), turn(20, 30, 3)]);
        let speakers: Vec<u32> = turns.iter().map(|t| t.speaker).collect();
        assert_eq!(speakers, vec![1, 2, 1]);
    }

    #[test]
    fn a_line_with_one_speaker_is_one_part() {
        let turns = [turn(0, 5_000, 1), turn(5_000, 9_000, 2)];
        assert_eq!(speaker_parts(500, 4_000, &turns), vec![(500, 4_000, 1)]);
    }

    #[test]
    fn a_line_where_the_speaker_changes_is_split() {
        let turns = [turn(0, 5_000, 1), turn(5_000, 9_000, 2)];
        assert_eq!(
            speaker_parts(1_000, 8_000, &turns),
            vec![(1_000, 5_000, 1), (5_000, 8_000, 2)]
        );
    }

    #[test]
    fn a_short_interjection_stays_with_the_main_speaker() {
        let turns = [
            turn(0, 4_000, 1),
            turn(4_000, 4_400, 2),
            turn(4_400, 8_000, 1),
        ];
        assert_eq!(speaker_parts(0, 8_000, &turns), vec![(0, 8_000, 1)]);
    }

    #[test]
    fn overlap_keeps_the_person_already_talking() {
        let turns = [turn(0, 6_000, 1), turn(3_000, 9_000, 2)];
        assert_eq!(
            speaker_parts(0, 9_000, &turns),
            vec![(0, 6_000, 1), (6_000, 9_000, 2)]
        );
    }

    #[test]
    fn an_uncovered_line_goes_to_the_nearest_speaker() {
        let turns = [turn(0, 1_000, 1), turn(20_000, 21_000, 2)];
        assert_eq!(
            speaker_parts(18_000, 19_000, &turns),
            vec![(18_000, 19_000, 2)]
        );
        assert!(speaker_parts(0, 1_000, &[]).is_empty());
    }
}
