//! Owl: reads an imported audio or video file as 16 kHz mono, the way meeting
//! tracks are saved. Symphonia (pure Rust): mp3, m4a/AAC, ALAC, wav, flac,
//! ogg/Vorbis, and the audio inside mp4/mov. It has no Opus decoder (WebM,
//! .opus, WhatsApp voice notes), so those fail with `UNSUPPORTED`.

use crate::audio_toolkit::audio::FrameResampler;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Error text the UI translates into "this format isn't supported yet".
pub const UNSUPPORTED: &str = "unsupported_format";

const RATE: usize = crate::managers::meeting::SAMPLE_RATE;

/// A file's demuxer and a decoder for its first audio track.
struct Opened {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
}

/// The file's first audio track and a decoder for it, or `UNSUPPORTED`.
fn open(path: &Path) -> Result<Opened> {
    let file = std::fs::File::open(path)?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| anyhow!(UNSUPPORTED))?
        .format;
    let codecs = symphonia::default::get_codecs();
    let (track_id, decoder) = format
        .tracks()
        .iter()
        .find_map(|t| {
            let decoder = codecs
                .make(&t.codec_params, &DecoderOptions::default())
                .ok()?;
            t.codec_params.sample_rate?;
            Some((t.id, decoder))
        })
        .ok_or_else(|| anyhow!(UNSUPPORTED))?;
    Ok(Opened {
        format,
        decoder,
        track_id,
    })
}

/// Whether `path` can be imported, without decoding it.
pub fn check(path: &Path) -> Result<()> {
    open(path).map(|_| ())
}

/// The file's audio as 16 kHz mono.
pub fn decode(path: &Path) -> Result<Vec<f32>> {
    let Opened {
        mut format,
        mut decoder,
        track_id,
    } = open(path)?;
    let mut resampler: Option<FrameResampler> = None;
    let mut out = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A damaged packet loses a moment of audio, not the whole file.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buffer.copy_interleaved_ref(decoded);
        let mono: Vec<f32> = buffer
            .samples()
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect();
        resampler
            .get_or_insert_with(|| {
                FrameResampler::new(spec.rate as usize, RATE, Duration::from_millis(30))
            })
            .push(&mono, |frame| out.extend_from_slice(frame));
    }
    if let Some(resampler) = resampler.as_mut() {
        resampler.finish(|frame| out.extend_from_slice(frame));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &Path, rate: u32, channels: u16, seconds: f32) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(rate as f32 * seconds) as u32 {
            let s = ((i as f32 * 0.05).sin() * 8_000.0) as i16;
            for _ in 0..channels {
                w.write_sample(s).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    #[test]
    fn a_stereo_48k_file_becomes_16k_mono_of_the_same_length() {
        let path = std::env::temp_dir().join("owl-media-test.wav");
        wav(&path, 48_000, 2, 2.0);
        let samples = decode(&path).unwrap();
        let seconds = samples.len() as f32 / RATE as f32;
        assert!((1.9..2.1).contains(&seconds), "{seconds} s");
        assert!(samples.iter().any(|s| s.abs() > 0.1));
    }

    /// Files made with ffmpeg from a `say` clip (see the PR notes):
    /// `OWL_TEST_AUDIO=/tmp/owl-audio cargo test --lib real_formats -- --ignored`.
    #[test]
    #[ignore]
    fn real_formats_decode_and_opus_is_unsupported() {
        let dir = std::path::PathBuf::from(
            std::env::var("OWL_TEST_AUDIO").unwrap_or("/tmp/owl-audio".into()),
        );
        for file in ["en.m4a", "en.mp3", "en.mp4"] {
            let seconds = decode(&dir.join(file)).unwrap().len() as f32 / RATE as f32;
            assert!((5.0..5.6).contains(&seconds), "{file}: {seconds} s");
        }
        assert_eq!(
            decode(&dir.join("en.opus")).unwrap_err().to_string(),
            UNSUPPORTED
        );
    }

    #[test]
    fn a_file_that_isnt_audio_is_unsupported() {
        let path = std::env::temp_dir().join("owl-media-test.opus");
        std::fs::write(&path, b"OggS not really audio").unwrap();
        assert_eq!(check(&path).unwrap_err().to_string(), UNSUPPORTED);
    }
}
