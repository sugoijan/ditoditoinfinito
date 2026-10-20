//! What CoreAudio's HAL says of the default output device: its latency
//! and its output stream's, the part of the path after a frame reaches
//! the device that cpal leaves out or counts wrongly
//! (`super::clock`'s doc). Read on the main thread (property reads are
//! not for the audio thread), about once a second, so a device the
//! system switches to (headphones plugged in) is followed.

/// The default output device's latencies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Latency {
    /// Device latency plus its first output stream's, in frames at
    /// `rate`.
    pub frames: u32,
    pub rate: f64,
    /// The safety offset and I/O buffer, frames, for the record.
    pub safety: u32,
    pub buffer: u32,
}

impl Latency {
    pub fn nanos(&self) -> i64 {
        (f64::from(self.frames) / self.rate.max(1.0) * 1e9).round() as i64
    }
}

#[cfg(target_os = "macos")]
pub fn output_latency() -> Option<Latency> {
    use objc2_core_audio::{
        AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress,
        kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyLatency,
        kAudioDevicePropertyNominalSampleRate, kAudioDevicePropertySafetyOffset,
        kAudioDevicePropertyStreams, kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject, kAudioStreamPropertyLatency,
    };
    use std::ffi::c_void;
    use std::ptr::NonNull;

    /// Reads a plain value (`u32`, `f64`, an array of ids) into `out`;
    /// the bytes written, or `None` on an error.
    fn get<T: Copy>(
        object: AudioObjectID,
        selector: u32,
        scope: u32,
        out: &mut [T],
    ) -> Option<usize> {
        let address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: scope,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut size = std::mem::size_of_val(out) as u32;
        // SAFETY: `out` is valid for `size` bytes of plain data, the
        // property's documented type (UInt32, Float64 or AudioObjectIDs);
        // CoreAudio writes at most `size` bytes and says how many.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                NonNull::from(&address),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::new(out.as_mut_ptr().cast::<c_void>())?,
            )
        };
        (status == 0).then_some(size as usize)
    }

    let mut device = [0u32];
    get(
        kAudioObjectSystemObject as AudioObjectID,
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
        &mut device,
    )?;
    let device = device[0];
    if device == 0 {
        return None;
    }
    let one = |selector, scope| {
        let mut v = [0u32];
        get(device, selector, scope, &mut v).map(|_| v[0])
    };
    let latency = one(kAudioDevicePropertyLatency, kAudioObjectPropertyScopeOutput)?;
    let safety = one(
        kAudioDevicePropertySafetyOffset,
        kAudioObjectPropertyScopeOutput,
    )
    .unwrap_or(0);
    let buffer = one(
        kAudioDevicePropertyBufferFrameSize,
        kAudioObjectPropertyScopeGlobal,
    )
    .unwrap_or(0);
    let mut rate = [0f64];
    get(
        device,
        kAudioDevicePropertyNominalSampleRate,
        kAudioObjectPropertyScopeGlobal,
        &mut rate,
    )?;
    let mut streams = [0u32; 16];
    let n = get(
        device,
        kAudioDevicePropertyStreams,
        kAudioObjectPropertyScopeOutput,
        &mut streams,
    )
    .unwrap_or(0)
        / 4;
    let stream = streams[..n]
        .first()
        .and_then(|&s| {
            let mut v = [0u32];
            get(
                s,
                kAudioStreamPropertyLatency,
                kAudioObjectPropertyScopeGlobal,
                &mut v,
            )
            .map(|_| v[0])
        })
        .unwrap_or(0);
    Some(Latency {
        frames: latency + stream,
        rate: rate[0],
        safety,
        buffer,
    })
}

/// Elsewhere there is no HAL: cpal's `playback` is used.
#[cfg(not(target_os = "macos"))]
pub fn output_latency() -> Option<Latency> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_default_device_where_there_is_one() {
        // Machines without an output device (CI) read nothing.
        if let Some(l) = super::output_latency() {
            assert!(l.rate >= 8_000.0 && l.rate <= 384_000.0, "{l:?}");
            assert!(l.frames < 48_000, "{l:?}");
            assert!(l.nanos() >= 0);
        }
    }
}
