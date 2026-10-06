//! Platform traits and plain data types shared by the engine and the shells.
//!
//! The engine never sees `AudioContext`, DOM events or OS handles; it gets
//! [`ClockSample`]s and [`RawInput`]s and talks to the song library through
//! [`SongStore`]. Web and desktop shells implement these.
//!
//! See `docs/PLAN.md` §3.5.

/// Seconds on the host's monotonic clock (`performance.now()/1000` on web,
/// `Instant` since app start natively).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct HostTime(pub f64);

/// One reading that pairs the audio clock with the host clock.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClockSample {
    /// `AudioContext.currentTime` equivalent: seconds of audio that have been
    /// handed to the output path.
    pub context_time: f64,
    /// Host time at which `context_time` was (estimated to be) current.
    pub host_time: HostTime,
    /// Estimated seconds between `context_time` and the sound leaving the
    /// speakers. Zero when unknown.
    pub output_latency: f64,
}

/// A device-level button or key edge.
#[derive(Clone, Debug, PartialEq)]
pub struct RawInput {
    pub device: DeviceId,
    /// `KeyboardEvent.code` for keyboards, `button:<n>` for gamepads, etc.
    pub control: String,
    pub pressed: bool,
    pub host_time: HostTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DeviceId {
    Keyboard,
    Gamepad(u32),
    Touch,
    Other(String),
}

pub type SoundId = u32;

/// Audio playback and the audio clock.
pub trait AudioBackend {
    type Buffer;
    type Error: core::fmt::Debug;

    /// Current reading of the audio and host clocks.
    fn now(&self) -> ClockSample;
    /// Schedule `buffer` to start at `start_at_context_time`, from `offset`
    /// seconds into the sound.
    fn play(
        &mut self,
        buffer: &Self::Buffer,
        start_at_context_time: f64,
        offset: f64,
        volume: f32,
    ) -> Result<SoundId, Self::Error>;
    fn stop(&mut self, id: SoundId);
    fn set_volume(&mut self, id: SoundId, volume: f32);
    fn duration(&self, buffer: &Self::Buffer) -> f64;
}

/// Polled source of input edges.
pub trait InputSource {
    fn poll(&mut self, out: &mut Vec<RawInput>);
}

/// Host clock.
pub trait HostClock {
    fn now(&self) -> HostTime;
}

/// Summary row for a song list.
#[derive(Clone, Debug, PartialEq)]
pub struct SongSummary {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub banner: Option<String>,
}

/// An identity for the audio output path or the display the game is shown
/// on. On the web it is derived from observable features (latencies, sample
/// rate, screen geometry, refresh rate); a native shell derives it from the
/// real device id. Offsets are stored per `id`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceFingerprint {
    /// Stable key, e.g. `audio:48000:5:170` or `display:1512x982@2:120`.
    pub id: String,
    /// Human-readable description shown in the UI.
    pub label: String,
}

impl DeviceFingerprint {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> DeviceFingerprint {
        DeviceFingerprint {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// The audio path and display a session runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceProfile {
    pub audio: DeviceFingerprint,
    pub display: DeviceFingerprint,
}
