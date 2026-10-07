//! System audio (everyone else on a call) through a Core Audio process tap,
//! macOS 14.2+. The tap mixes every app's output except Owl's own (feedback
//! sounds) down to mono; a private aggregate device wraps it so an IOProc can
//! read it. Needs the "System Audio Recording" permission
//! (`NSAudioCaptureUsageDescription` in Info.plist), not Screen Recording.
//! Without it the tap delivers silence.
//!
//! Adapted from Felix (https://github.com/kz364/felix, MIT), a Handy fork.

/// Audio arriving on a real-time thread, read through a ring buffer.
pub struct Source {
    pub rate: u32,
    pub consumer: rtrb::Consumer<f32>,
}

use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_core_audio::{
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceIsStackedKey,
    kAudioAggregateDeviceMainSubDeviceKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceSubDeviceListKey, kAudioAggregateDeviceTapAutoStartKey,
    kAudioAggregateDeviceTapListKey, kAudioAggregateDeviceUIDKey, kAudioDevicePropertyDeviceUID,
    kAudioDevicePropertyNominalSampleRate, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioHardwarePropertyTranslatePIDToProcessObject, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioObjectUnknown,
    kAudioSubDeviceUIDKey, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
    kAudioTapPropertyFormat, AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID,
    AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
    AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
    AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectPropertySelector, CATapDescription, CATapMuteBehavior,
};
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSString, NSUUID};
use std::cell::UnsafeCell;
use std::ffi::{c_void, CStr};
use std::mem::size_of;
use std::ptr::NonNull;

/// Seconds of audio the ring holds if the writer falls behind.
const RING_SECONDS: usize = 10;

/// Read one fixed-size property of a Core Audio object.
unsafe fn get_property<T: Copy>(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    qualifier: Option<&i32>,
    initial: T,
) -> Result<T, String> {
    let address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut value = initial;
    let mut size = size_of::<T>() as u32;
    let (qualifier_size, qualifier_ptr) = match qualifier {
        Some(q) => (size_of::<i32>() as u32, q as *const i32 as *const c_void),
        None => (0, std::ptr::null()),
    };
    let status = AudioObjectGetPropertyData(
        object,
        NonNull::from(&address),
        qualifier_size,
        qualifier_ptr,
        NonNull::from(&mut size),
        NonNull::from(&mut value).cast(),
    );
    if status != 0 {
        return Err(format!("property {selector:#x} failed ({status})"));
    }
    Ok(value)
}

fn key(k: &CStr) -> Retained<NSString> {
    NSString::from_str(k.to_str().unwrap_or_default())
}

/// The UID of the default output device, the aggregate's clock.
unsafe fn default_output_uid() -> Result<Retained<NSString>, String> {
    let device: AudioObjectID = get_property(
        kAudioObjectSystemObject as AudioObjectID,
        kAudioHardwarePropertyDefaultOutputDevice,
        None,
        kAudioObjectUnknown,
    )?;
    let uid: *mut NSString = get_property(
        device,
        kAudioDevicePropertyDeviceUID,
        None,
        std::ptr::null_mut(),
    )?;
    // The property returns a +1 CFString, toll-free bridged to NSString.
    Retained::from_raw(uid).ok_or_else(|| "The output device has no UID".into())
}

/// State the IOProc reads on the audio thread.
struct Callback {
    producer: UnsafeCell<rtrb::Producer<f32>>,
}

/// Real-time callback: average the tap's channels and push mono samples.
/// Allocation-, lock- and syscall-free.
unsafe extern "C-unwind" fn io_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    let callback = &*(client as *const Callback);
    let producer = &mut *callback.producer.get();
    let list = input.as_ref();
    let buffers = std::slice::from_raw_parts(list.mBuffers.as_ptr(), list.mNumberBuffers as usize);
    let Some(first) = buffers.first() else {
        return 0;
    };
    if first.mData.is_null() {
        return 0;
    }
    if buffers.len() == 1 {
        // Interleaved (or mono): one buffer with every channel.
        let channels = first.mNumberChannels.max(1) as usize;
        let samples = std::slice::from_raw_parts(
            first.mData as *const f32,
            first.mDataByteSize as usize / size_of::<f32>(),
        );
        for frame in samples.chunks_exact(channels) {
            let _ = producer.push(frame.iter().sum::<f32>() / channels as f32);
        }
    } else {
        // Non-interleaved: one buffer per channel.
        let frames = first.mDataByteSize as usize / size_of::<f32>();
        for i in 0..frames {
            let mut sum = 0.0;
            for buffer in buffers {
                if !buffer.mData.is_null() && i * size_of::<f32>() < buffer.mDataByteSize as usize {
                    sum += *(buffer.mData as *const f32).add(i);
                }
            }
            let _ = producer.push(sum / buffers.len() as f32);
        }
    }
    0
}

/// A running tap; stops and tears down on drop.
pub struct SystemTap {
    tap: AudioObjectID,
    aggregate: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    callback: *mut Callback,
}

// The raw pointers are only touched on start/drop; the IOProc owns its side.
unsafe impl Send for SystemTap {}

impl Drop for SystemTap {
    fn drop(&mut self) {
        unsafe {
            if self.proc_id.is_some() {
                AudioDeviceStop(self.aggregate, self.proc_id);
                AudioDeviceDestroyIOProcID(self.aggregate, self.proc_id);
            }
            if self.aggregate != kAudioObjectUnknown {
                AudioHardwareDestroyAggregateDevice(self.aggregate);
            }
            if self.tap != kAudioObjectUnknown {
                AudioHardwareDestroyProcessTap(self.tap);
            }
            if !self.callback.is_null() {
                drop(Box::from_raw(self.callback));
            }
        }
    }
}

/// Start tapping every app's audio output except Owl's.
pub fn start() -> Result<(Source, SystemTap), String> {
    unsafe { start_inner() }
}

unsafe fn start_inner() -> Result<(Source, SystemTap), String> {
    let mut session = SystemTap {
        tap: kAudioObjectUnknown,
        aggregate: kAudioObjectUnknown,
        proc_id: None,
        callback: std::ptr::null_mut(),
    };

    // Leave out Owl's own sounds. The process object exists only once the
    // process has used audio; before that there's nothing to exclude.
    let pid = std::process::id() as i32;
    let own: AudioObjectID = get_property(
        kAudioObjectSystemObject as AudioObjectID,
        kAudioHardwarePropertyTranslatePIDToProcessObject,
        Some(&pid),
        kAudioObjectUnknown,
    )
    .unwrap_or(kAudioObjectUnknown);
    let excluded: Vec<Retained<NSNumber>> = if own == kAudioObjectUnknown {
        vec![]
    } else {
        vec![NSNumber::new_u32(own)]
    };
    let excluded = NSArray::from_retained_slice(&excluded);

    let description = CATapDescription::initMonoGlobalTapButExcludeProcesses(
        CATapDescription::alloc(),
        &excluded,
    );
    let tap_uuid = NSUUID::new();
    description.setUUID(&tap_uuid);
    description.setName(&NSString::from_str("Owl meeting"));
    description.setPrivate(true);
    description.setMuteBehavior(CATapMuteBehavior::Unmuted);

    let status = AudioHardwareCreateProcessTap(Some(&description), &mut session.tap);
    if status != 0 || session.tap == kAudioObjectUnknown {
        return Err(format!("Couldn't create the system audio tap ({status})"));
    }

    let format: AudioStreamBasicDescription = get_property(
        session.tap,
        kAudioTapPropertyFormat,
        None,
        std::mem::zeroed(),
    )?;
    let rate = format.mSampleRate.round() as u32;
    if rate == 0 {
        return Err("The system audio tap has no sample rate".into());
    }

    let output_uid = default_output_uid()?;
    let sub_device: Retained<NSDictionary<NSString, NSObject>> =
        NSDictionary::from_retained_objects(
            &[&*key(kAudioSubDeviceUIDKey)],
            &[Retained::into_super(output_uid.clone())],
        );
    let sub_tap: Retained<NSDictionary<NSString, NSObject>> = NSDictionary::from_retained_objects(
        &[
            &*key(kAudioSubTapUIDKey),
            &*key(kAudioSubTapDriftCompensationKey),
        ],
        &[
            Retained::into_super(tap_uuid.UUIDString()),
            Retained::into_super(Retained::into_super(NSNumber::new_bool(true))),
        ],
    );
    let aggregate_uid = NSUUID::new().UUIDString();
    let yes = || Retained::into_super(Retained::into_super(NSNumber::new_bool(true)));
    let no = || Retained::into_super(Retained::into_super(NSNumber::new_bool(false)));
    let description: Retained<NSDictionary<NSString, NSObject>> =
        NSDictionary::from_retained_objects(
            &[
                &*key(kAudioAggregateDeviceNameKey),
                &*key(kAudioAggregateDeviceUIDKey),
                &*key(kAudioAggregateDeviceMainSubDeviceKey),
                &*key(kAudioAggregateDeviceIsPrivateKey),
                &*key(kAudioAggregateDeviceIsStackedKey),
                &*key(kAudioAggregateDeviceTapAutoStartKey),
                &*key(kAudioAggregateDeviceSubDeviceListKey),
                &*key(kAudioAggregateDeviceTapListKey),
            ],
            &[
                Retained::into_super(NSString::from_str("Owl meeting")),
                Retained::into_super(aggregate_uid),
                Retained::into_super(output_uid),
                yes(),
                no(),
                yes(),
                Retained::into_super(NSArray::from_retained_slice(&[sub_device])),
                Retained::into_super(NSArray::from_retained_slice(&[sub_tap])),
            ],
        );
    // NSDictionary is toll-free bridged to CFDictionary.
    let cf = &*(Retained::as_ptr(&description) as *const CFDictionary);
    let status = AudioHardwareCreateAggregateDevice(cf, NonNull::from(&mut session.aggregate));
    if status != 0 || session.aggregate == kAudioObjectUnknown {
        return Err(format!(
            "Couldn't create the tap's aggregate device ({status})"
        ));
    }

    // The tap's format can say 48 kHz while the output it's built on runs
    // slower (AirPods on a call: 24 kHz); the device's own rate is what
    // arrives.
    let rate = match get_property::<f64>(
        session.aggregate,
        kAudioDevicePropertyNominalSampleRate,
        None,
        0.0,
    ) {
        Ok(device_rate) if device_rate >= 8_000.0 && device_rate.round() as u32 != rate => {
            log::warn!(
                "System audio tap says {rate} Hz but its device runs at {device_rate} Hz; using the device's"
            );
            device_rate.round() as u32
        }
        _ => rate,
    };
    let (producer, consumer) = rtrb::RingBuffer::new(rate as usize * RING_SECONDS);
    session.callback = Box::into_raw(Box::new(Callback {
        producer: UnsafeCell::new(producer),
    }));
    let status = AudioDeviceCreateIOProcID(
        session.aggregate,
        Some(io_proc),
        session.callback as *mut c_void,
        NonNull::from(&mut session.proc_id),
    );
    if status != 0 || session.proc_id.is_none() {
        return Err(format!("Couldn't read the tap ({status})"));
    }
    let status = AudioDeviceStart(session.aggregate, session.proc_id);
    if status != 0 {
        return Err(format!("Couldn't start the tap ({status})"));
    }
    log::info!(
        "System audio tap started: {} Hz, {} channel(s), excluding Owl: {}",
        rate,
        format.mChannelsPerFrame,
        own != kAudioObjectUnknown
    );
    Ok((Source { rate, consumer }, session))
}

#[cfg(test)]
mod tests {
    /// Real devices: `cargo test --lib system_tap_delivers -- --ignored`.
    #[test]
    #[ignore]
    fn system_tap_delivers_samples() {
        let (source, _tap) = super::start().expect("tap starts");
        std::thread::sleep(std::time::Duration::from_secs(1));
        println!(
            "rate {} Hz, {} samples after 1 s",
            source.rate,
            source.consumer.slots()
        );
        assert!(source.consumer.slots() > 0);
    }
}
