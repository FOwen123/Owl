//! System audio (everyone else on a call) on Windows: WASAPI loopback of the
//! default output device, through cpal. Unlike the macOS tap it also hears
//! Owl's own sounds. Needs no permission.

use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::cpal::{self, SampleFormat};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Seconds of audio the ring holds if the writer falls behind.
const RING_SECONDS: usize = 10;

/// Audio arriving on a real-time thread, read through a ring buffer.
pub struct Source {
    pub rate: u32,
    pub consumer: rtrb::Consumer<f32>,
    /// Samples dropped because the reader fell behind the ring buffer.
    pub lost: Arc<AtomicUsize>,
}

/// The running capture; dropping it stops it.
pub struct SystemTap {
    _loopback: cpal::Stream,
    _silence: cpal::Stream,
}

/// Start recording everything the default output device plays.
pub fn start() -> Result<(Source, SystemTap), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("There is no output device")?;
    let supported = device
        .default_output_config()
        .map_err(|e| format!("Can't read the output device's format: {e}"))?;
    // Loopback takes the device's own mix format, which is float in shared mode.
    if supported.sample_format() != SampleFormat::F32 {
        return Err(format!(
            "The output device's format ({}) isn't supported",
            supported.sample_format()
        ));
    }
    let config = supported.config();
    let (rate, channels) = (config.sample_rate, config.channels.max(1) as usize);

    let (mut producer, consumer) = rtrb::RingBuffer::new(rate as usize * RING_SECONDS);
    let lost = Arc::new(AtomicUsize::new(0));
    let dropped = lost.clone();
    // An output device as input: cpal opens it in loopback mode.
    let loopback = device
        .build_input_stream::<f32, _, _>(
            config,
            move |data: &[f32], _| {
                let mut missed = 0;
                for frame in data.chunks_exact(channels) {
                    if producer
                        .push(frame.iter().sum::<f32>() / channels as f32)
                        .is_err()
                    {
                        missed += 1;
                    }
                }
                if missed > 0 {
                    dropped.fetch_add(missed, Ordering::Relaxed);
                }
            },
            |e| log::warn!("System audio: {e}"),
            None,
        )
        .map_err(|e| format!("Couldn't record the output device: {e}"))?;
    // Loopback delivers nothing while nothing plays, which would leave the
    // call's track behind the microphone's; playing silence keeps it flowing.
    let silence = device
        .build_output_stream::<f32, _, _>(
            config,
            |_: &mut [f32], _| {},
            |e| log::warn!("System audio keep-alive: {e}"),
            None,
        )
        .map_err(|e| format!("Couldn't keep the output device running: {e}"))?;
    silence
        .play()
        .map_err(|e| format!("Couldn't start the output device: {e}"))?;
    loopback
        .play()
        .map_err(|e| format!("Couldn't start recording the output device: {e}"))?;
    log::info!("System audio loopback started: {rate} Hz, {channels} channel(s)");
    Ok((
        Source {
            rate,
            consumer,
            lost,
        },
        SystemTap {
            _loopback: loopback,
            _silence: silence,
        },
    ))
}
