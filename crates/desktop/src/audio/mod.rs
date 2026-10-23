//! Sound on desktop (`docs/plans/desktop.md`, section 1 and step 7.2):
//! the default output device through cpal, our own [`mixer`] in its
//! callback, the [`clock`] the timing model trusts, and a [`decoder`]
//! thread.
//!
//! [`Audio`] is the backend: `ddi_platform::AudioBackend` (the clock,
//! voices started at a context time) and, as inherent methods named as
//! the trait will name them when step 7.1 grows it, what the play session
//! uses Web Audio for: the sample rate, the master volume and its ramps,
//! stopping everything, whether sound runs, buffers from PCM, the
//! device's fingerprint.
//!
//! Times are the web's: `context_time` is the stream's timeline in
//! seconds (`currentTime`), sounds start on a frame of it, and
//! [`Audio::now`] pairs it with host time as `getOutputTimestamp` does.
//! The stream opens at the device's own rate (decoders resample to it),
//! `f32` where offered, with a 256-frame buffer where the device allows
//! one (about 5 ms). Without a device (CI) nothing sounds and the clock
//! is the host's.
//!
//! When the device goes (unplugged) or, outside macOS, the default
//! output changes, the stream is opened again on the new default within
//! a second ([`Audio::tick`]); its timeline goes on from where the old
//! one was. Sounds playing then stop (a preview starts again on its
//! next loop). On macOS the system moves the stream itself and only the
//! latencies are read again.

pub mod clock;
pub mod decoder;
pub mod hal;
pub mod mixer;

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ddi_platform::{AudioBackend, ClockSample, DeviceFingerprint, SoundId};

use self::clock::{Callback, ClockMath, Published, Reading, Timebase};
use self::mixer::{Command, Mixer, Voice};
pub use self::mixer::{Event, Pcm, Sound};

/// The buffer asked for, frames (about 5 ms at 48 kHz).
const BUFFER_FRAMES: u32 = 256;
/// Commands in flight to the audio thread.
const RING: usize = 256;
/// Voices handed back. The main thread empties this ring before every
/// command it sends, so it never holds more than the voices that exist
/// (those in the command ring, at most [`RING`], and the mixer's, at most
/// [`mixer::MAX_VOICES`]): it cannot fill, and the audio thread never
/// frees a voice.
const RETURNED: usize = RING + mixer::MAX_VOICES + 64;
/// Frames mixed at a time in the callback (its scratch buffer).
const SCRATCH: usize = 4096;
/// The master volume's smoothing (Web Audio's `setTargetAtTime(…, 0.015)`).
const MASTER_TAU: f64 = 0.015;
/// How often [`Audio::tick`] looks at the device.
const CHECK: Duration = Duration::from_secs(1);

/// What the audio thread shares with the others.
#[derive(Debug, Default)]
struct Shared {
    published: Published,
    /// CoreAudio's device and stream latency, nanoseconds; -1: unknown.
    hal_ns: AtomicI64,
    /// The stream failed (the device went, the host or the stream is
    /// gone): open it again.
    failed: AtomicBool,
    /// Underruns and overloads the host reported (cpal's `Xrun`); the
    /// stream goes on.
    xruns: AtomicU64,
    /// Other errors that need nothing done (busy, unsupported…).
    other_errors: AtomicU64,
    /// Voices the return ring had no room for (never, by its size),
    /// leaked rather than freed on the audio thread.
    leaked: AtomicU64,
    /// The system moved the stream to another device (macOS).
    rerouted: AtomicBool,
    /// Host nanoseconds of the last callback, and how many there were.
    last_entry: AtomicU64,
    callbacks: AtomicU64,
    /// The loudest sample since read ([`Audio::take_peak`]), `f32` bits
    /// (the web's output meter: "no sound" told apart).
    peak: AtomicU64,
}

/// The open stream.
struct Live {
    _stream: cpal::Stream,
    rate: u32,
    channels: u16,
    /// The buffer asked for (`None`: the device's default).
    buffer: Option<u32>,
    host: &'static str,
    device: String,
    commands: rtrb::Producer<Command>,
    returned: rtrb::Consumer<Box<Voice>>,
    shared: Arc<Shared>,
    timebase: Timebase,
}

/// What a stream is, for the record.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamInfo {
    pub host: String,
    pub device: String,
    pub rate: u32,
    pub channels: u16,
    pub buffer: Option<u32>,
    /// `true`: heard times from CoreAudio's host time and the HAL's
    /// latencies; `false`: cpal's `playback`, anchored.
    pub host_clock: bool,
    pub hal: Option<hal::Latency>,
}

/// The desktop's audio backend.
pub struct Audio {
    live: Option<Live>,
    next_id: SoundId,
    master: f32,
    hal: Option<hal::Latency>,
    checked: Instant,
    /// Why there is no stream, when there is none.
    pub error: Option<String>,
    /// Commands dropped because the ring was full (never, in practice).
    pub dropped: u64,
    /// The timeline as last read: what the clock goes on from while no
    /// stream gives readings (none open, or a new one not yet called), so
    /// `context_time` never jumps when a stream reopens.
    timeline: Cell<Reading>,
    /// Underruns of streams closed since (added to the open one's).
    xruns_before: u64,
    /// Never opens a device ([`Audio::silent`]): tests stay off the
    /// machine's sound.
    offline: bool,
    /// Plays with no device ([`Audio::headless`]): running, silent, its
    /// timeline the host clock's (or `clock`'s).
    headless: bool,
    /// Where the host clock is read (`crate::clock::now_ns`, or a test's).
    clock: Option<HostClock>,
}

/// No device is opened by this process ([`set_headless`]).
static HEADLESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// From now on every [`Audio::new`] is [`Audio::headless`]: no sound
/// device is opened (`--silent`), plays run on the host clock.
pub fn set_headless(on: bool) {
    HEADLESS.store(on, Ordering::Relaxed);
}

/// A host clock in nanoseconds, from any thread (a test's fake one).
pub type HostClock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The clock every [`Audio::new`] runs on under [`set_headless`]
/// (`None`: the host's).
static HEADLESS_CLOCK: std::sync::Mutex<Option<HostClock>> = std::sync::Mutex::new(None);

/// Under [`set_headless`], [`Audio::new`] runs on `clock` (a test's fake
/// one) from now on.
pub fn set_headless_clock(clock: Option<HostClock>) {
    if let Ok(mut c) = HEADLESS_CLOCK.lock() {
        *c = clock;
    }
}

impl Audio {
    /// Opens the default output at master volume `master` (none under
    /// [`set_headless`]: [`Audio::headless`] instead).
    pub fn new(master: f32) -> Audio {
        if HEADLESS.load(Ordering::Relaxed) {
            let clock = HEADLESS_CLOCK.lock().ok().and_then(|c| c.clone());
            return Audio::headless(master, clock);
        }
        let mut audio = Audio::silent(master);
        audio.error = None;
        audio.offline = false;
        audio.reopen();
        audio
    }

    /// No device, and none ever opened: silent; the timeline starts at 0
    /// now and runs with the host's clock (tests).
    pub fn silent(master: f32) -> Audio {
        Audio {
            live: None,
            next_id: 1,
            master: mixer::gain(master),
            hal: None,
            checked: Instant::now(),
            error: Some("no output (silent)".into()),
            dropped: 0,
            timeline: Cell::new(Reading {
                frame: 0,
                heard_ns: crate::clock::now_ns() as i64,
                latency_ns: 0,
                rate: 48_000,
            }),
            xruns_before: 0,
            offline: true,
            headless: false,
            clock: None,
        }
    }

    /// No device, but running: sounds are dropped, and the timeline is
    /// the host clock's, with no latency (`--silent`, and plays in tests,
    /// where `clock`, when given, stands for the host clock). A play on it
    /// is judged as on a device that is always on time.
    pub fn headless(master: f32, clock: Option<HostClock>) -> Audio {
        let mut audio = Audio::silent(master);
        audio.headless = true;
        audio.error = Some("no output (headless)".into());
        if let Some(c) = &clock {
            audio.timeline.set(Reading {
                frame: 0,
                heard_ns: c() as i64,
                latency_ns: 0,
                rate: 48_000,
            });
        }
        audio.clock = clock;
        audio
    }

    /// The host clock now, nanoseconds.
    fn now_ns(&self) -> u64 {
        match &self.clock {
            Some(c) => c(),
            None => crate::clock::now_ns(),
        }
    }

    /// Closes the stream and opens the default output again, the
    /// timeline going on (what a failed device does; for tests).
    pub fn force_reopen(&mut self) {
        self.reopen();
    }

    fn reopen(&mut self) {
        let now = AudioBackend::now(self);
        if let Some(l) = self.live.take() {
            self.xruns_before += l.shared.xruns.load(Ordering::Relaxed);
        }
        if self.offline {
            return;
        }
        self.hal = hal::output_latency();
        match open(self.master, now, self.hal) {
            Ok(live) => {
                self.live = Some(live);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        self.checked = Instant::now();
    }

    /// The output's rate (48 kHz without one).
    pub fn sample_rate(&self) -> u32 {
        self.live.as_ref().map_or(48_000, |l| l.rate)
    }

    /// The stream's timeline now (`currentTime`).
    pub fn context_time(&self) -> f64 {
        self.now().context_time
    }

    /// The output frame at context time `t`.
    fn frame(&self, t: f64) -> u64 {
        (t.max(0.0) * f64::from(self.sample_rate())).round() as u64
    }

    fn send(&mut self, c: Command) {
        if let Some(live) = &mut self.live {
            // Room for whatever comes back (see `RETURNED`).
            while live.returned.pop().is_ok() {}
            if live.commands.push(c).is_err() {
                self.dropped += 1;
            }
        }
    }

    /// Plays `sound` from `offset` seconds into it, starting at context
    /// time `at` (at once if that is past), at `gain`.
    pub fn play_sound(&mut self, sound: &Sound, at: f64, offset: f64, gain: f32) -> SoundId {
        let gain = mixer::gain(gain);
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let rate = self.sample_rate();
        let voice = Voice::new(id, sound.clone(), self.frame(at), offset, gain, rate);
        self.send(Command::Play(Box::new(voice)));
        id
    }

    /// Sound `id`'s gain becomes `value` at context time `at`.
    pub fn gain_at(&mut self, id: SoundId, value: f32, at: f64) {
        let at = self.frame(at);
        self.send(Command::Gain {
            id,
            event: Event::Set { at, value },
        });
    }

    /// Sound `id`'s gain ramps linearly to `value`, reaching it at
    /// context time `end`, from its previous event.
    pub fn gain_ramp(&mut self, id: SoundId, value: f32, end: f64) {
        let end = self.frame(end);
        self.send(Command::Gain {
            id,
            event: Event::Ramp { end, value },
        });
    }

    /// Stops sound `id` at context time `at`, or at once (with a 5 ms
    /// fade).
    pub fn stop_at(&mut self, id: SoundId, at: Option<f64>) {
        let at = at.map(|t| self.frame(t));
        self.send(Command::Stop { id, at });
    }

    /// Stops every sound (each with a 5 ms fade).
    pub fn stop_all(&mut self) {
        self.send(Command::StopAll);
    }

    /// Moves the master volume smoothly to `volume` (15 ms), cancelling
    /// what was scheduled.
    pub fn set_master_volume(&mut self, volume: f32) {
        let volume = mixer::gain(volume);
        self.master = volume;
        let now = self.frame(self.context_time());
        let tau = (MASTER_TAU * f64::from(self.sample_rate())) as f32;
        self.send(Command::CancelMaster { from: 0 });
        self.send(Command::Master(Event::Target {
            at: now,
            value: volume,
            tau,
        }));
    }

    /// The master gain becomes `value` at context time `at`.
    pub fn master_set_at(&mut self, value: f32, at: f64) {
        let at = self.frame(at);
        self.send(Command::Master(Event::Set {
            at,
            value: mixer::gain(value),
        }));
    }

    /// From context time `at`, the master gain ramps from what it is then
    /// to `to` over `seconds` (the fail fade, `#STARTFRAME`'s fade-in,
    /// `#ENDFRAME`'s stop).
    pub fn master_ramp(&mut self, to: f32, at: f64, seconds: f64) {
        let start = self.frame(at);
        let end = self.frame(at + seconds.max(0.01));
        self.send(Command::CancelMaster { from: start });
        self.send(Command::Master(Event::Hold { at: start }));
        self.send(Command::Master(Event::Ramp {
            end,
            value: mixer::gain(to),
        }));
    }

    /// Whether sound is running: callbacks came in the last half second.
    pub fn running(&self) -> bool {
        if self.headless {
            return true;
        }
        self.live.as_ref().is_some_and(|l| {
            let last = l.shared.last_entry.load(Ordering::Relaxed);
            l.shared.callbacks.load(Ordering::Relaxed) > 0
                && crate::clock::now_ns().saturating_sub(last) < 500_000_000
        })
    }

    /// A sound from planar PCM at `rate`, resampled to the output's
    /// (calibration's click track, the developer screen's tone).
    pub fn buffer_from_pcm(&self, channels: Vec<Vec<f32>>, rate: u32) -> Result<Sound, String> {
        let audio = ddi_platform::decode_all::DecodedAudio {
            sample_rate: rate,
            channels,
            skipped_packets: 0,
        };
        let audio = Pcm::fold(audio);
        let channels = audio.channels.len() as u16;
        let rate = self.sample_rate();
        let data = ddi_platform::decode_all::resample_interleaved(audio, rate)
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(Pcm {
            rate,
            channels,
            data,
        }))
    }

    /// Underruns and overloads reported since the start (the stream
    /// goes on; a play can be flagged by the change over it).
    pub fn xruns(&self) -> u64 {
        self.xruns_before
            + self
                .live
                .as_ref()
                .map_or(0, |l| l.shared.xruns.load(Ordering::Relaxed))
    }

    /// The output path's identity, once sound runs (`None` before): host,
    /// device, rate and the latency in 5 ms buckets (offsets are kept per
    /// fingerprint). The latency is the stream's nominal one, so a device
    /// near a bucket's edge cannot flip between profiles: on CoreAudio
    /// the buffer, safety offset and the HAL's latencies; elsewhere
    /// cpal's `playback − callback`, fixed for a stream.
    pub fn fingerprint(&self) -> Option<DeviceFingerprint> {
        if !self.running() {
            return None;
        }
        let live = self.live.as_ref()?;
        let latency = match (live.timebase, self.hal) {
            (Timebase::Host { .. }, Some(h)) => {
                // The stream's own buffer (the HAL's reading can predate it).
                let buffer = live.buffer.unwrap_or(h.buffer);
                f64::from(h.frames + h.safety + buffer) / h.rate.max(1.0) * 1000.0
            }
            _ => self.now().output_latency * 1000.0,
        };
        let bucket = (latency / 5.0).round() as u32 * 5;
        Some(DeviceFingerprint::new(
            format!("audio:{}:{}:{}:{bucket}", live.host, live.device, live.rate),
            format!(
                "{} · {} kHz · ~{} ms output latency",
                live.device,
                f64::from(live.rate) / 1000.0,
                latency.round()
            ),
        ))
    }

    /// What the stream is.
    pub fn info(&self) -> Option<StreamInfo> {
        self.live.as_ref().map(|l| StreamInfo {
            host: l.host.to_string(),
            device: l.device.clone(),
            rate: l.rate,
            channels: l.channels,
            buffer: l.buffer,
            host_clock: matches!(l.timebase, Timebase::Host { .. }),
            hal: self.hal,
        })
    }

    /// The clock, readable from any thread (measurements).
    pub fn probe(&self) -> Option<ClockProbe> {
        self.live.as_ref().map(|l| ClockProbe(l.shared.clone()))
    }

    /// The loudest sample output since last asked (0: silence).
    pub fn take_peak(&self) -> f32 {
        self.live.as_ref().map_or(0.0, |l| {
            f32::from_bits(l.shared.peak.swap(0, Ordering::Relaxed) as u32)
        })
    }

    /// One line for logs and the debug overlay; the output's peak since
    /// the last.
    pub fn status_line(&self) -> String {
        match self.info() {
            Some(i) => {
                let s = self.now();
                let peak = self.take_peak();
                let level = if peak > 0.0 {
                    format!("{:.1} dBFS", 20.0 * peak.log10())
                } else {
                    "silent".into()
                };
                format!(
                    "audio: {} · {} · {} Hz · {} ch · buffer {} · latency {:.2} ms{} · master {:.3} · output peak {level} · xruns {}",
                    i.host,
                    i.device,
                    i.rate,
                    i.channels,
                    i.buffer.map_or("default".into(), |b| b.to_string()),
                    s.output_latency * 1000.0,
                    if i.host_clock {
                        " (host clock + HAL)"
                    } else {
                        " (anchored)"
                    },
                    self.master,
                    self.xruns(),
                )
            }
            None => format!(
                "audio: none ({})",
                self.error.as_deref().unwrap_or("not opened")
            ),
        }
    }

    /// Housekeeping, called from the event loop: voices that ended are
    /// dropped here; about once a second the HAL's latencies are read
    /// again and a failed or replaced device is opened again (whether it
    /// was: the sounds playing are gone).
    pub fn tick(&mut self) -> bool {
        if let Some(live) = &mut self.live {
            while live.returned.pop().is_ok() {}
        }
        // No device, and none to open.
        if self.offline {
            return false;
        }
        let rerouted = self
            .live
            .as_ref()
            .is_some_and(|l| l.shared.rerouted.swap(false, Ordering::Relaxed));
        if !rerouted && self.checked.elapsed() < CHECK {
            return false;
        }
        self.checked = Instant::now();
        let failed = self
            .live
            .as_ref()
            .is_none_or(|l| l.shared.failed.load(Ordering::Relaxed));
        let replaced = !failed && self.live.as_ref().is_some_and(default_changed);
        if failed || replaced {
            self.reopen();
            return true;
        }
        self.hal = hal::output_latency();
        if let Some(live) = &mut self.live {
            live.shared
                .hal_ns
                .store(self.hal.map_or(-1, |h| h.nanos()), Ordering::Relaxed);
            if rerouted && let Some(d) = cpal::default_host().default_output_device() {
                live.device = device_name(&d);
            }
        }
        false
    }
}

/// The clock from another thread.
#[derive(Clone)]
pub struct ClockProbe(Arc<Shared>);

impl ClockProbe {
    pub fn now(&self) -> Option<ClockSample> {
        let r = self.0.published.read()?;
        Some(r.sample(crate::clock::now_ns()))
    }
}

/// How steady the clock was: `context_time − host_time` against host
/// time, about a fitted line (the jitter), the line's slope (the device's
/// drift against the host) and the latency reported.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClockStats {
    pub samples: usize,
    pub jitter_sd: f64,
    pub jitter_max: f64,
    pub drift_ppm: f64,
    pub latency_mean: f64,
    pub latency_min: f64,
    pub latency_max: f64,
}

impl ClockStats {
    /// From clock samples (taken a millisecond or so apart).
    pub fn of(samples: &[ClockSample]) -> ClockStats {
        let n = samples.len();
        if n < 3 {
            return ClockStats {
                samples: n,
                ..ClockStats::default()
            };
        }
        let x: Vec<f64> = samples.iter().map(|s| s.host_time.0).collect();
        let y: Vec<f64> = samples
            .iter()
            .map(|s| s.context_time - s.output_latency - s.host_time.0)
            .collect();
        let mx = x.iter().sum::<f64>() / n as f64;
        let my = y.iter().sum::<f64>() / n as f64;
        let (mut sxy, mut sxx) = (0.0, 0.0);
        for (a, b) in x.iter().zip(&y) {
            sxy += (a - mx) * (b - my);
            sxx += (a - mx) * (a - mx);
        }
        let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
        let resid: Vec<f64> = x
            .iter()
            .zip(&y)
            .map(|(a, b)| b - (my + slope * (a - mx)))
            .collect();
        let sd = (resid.iter().map(|r| r * r).sum::<f64>() / n as f64).sqrt();
        let lat: Vec<f64> = samples.iter().map(|s| s.output_latency).collect();
        ClockStats {
            samples: n,
            jitter_sd: sd,
            jitter_max: resid.iter().fold(0f64, |m, r| m.max(r.abs())),
            drift_ppm: slope * 1e6,
            latency_mean: lat.iter().sum::<f64>() / n as f64,
            latency_min: lat.iter().copied().fold(f64::INFINITY, f64::min),
            latency_max: lat.iter().copied().fold(0f64, f64::max),
        }
    }
}

impl AudioBackend for Audio {
    type Buffer = Sound;
    type Error = String;

    fn sample_rate(&self) -> f64 {
        f64::from(Audio::sample_rate(self))
    }

    fn master_ramp(&mut self, from: Option<f32>, to: f32, at: Option<f64>, seconds: f64) {
        match (at, from) {
            (None, _) => {
                let now = self.context_time();
                Audio::master_ramp(self, to, now, seconds);
            }
            (Some(at), None) => Audio::master_ramp(self, to, at, seconds),
            (Some(at), Some(from)) => {
                let end = self.frame(at + seconds.max(0.01));
                self.master_set_at(from, at);
                self.send(Command::Master(Event::Ramp {
                    end,
                    value: mixer::gain(to),
                }));
            }
        }
    }

    fn stop_all(&mut self) {
        Audio::stop_all(self);
    }

    fn running(&self) -> bool {
        Audio::running(self)
    }

    fn buffer_from_pcm(&self, channels: Vec<Vec<f32>>, rate: f64) -> Result<Sound, String> {
        Audio::buffer_from_pcm(self, channels, rate.round() as u32)
    }

    fn fingerprint(&self) -> Option<DeviceFingerprint> {
        Audio::fingerprint(self)
    }

    fn now(&self) -> ClockSample {
        // The stream's latest reading (a new stream's is seeded from the
        // old timeline), else the timeline as last read, extrapolated.
        if let Some(r) = self.live.as_ref().and_then(|l| l.shared.published.read()) {
            self.timeline.set(r);
        }
        self.timeline.get().sample(self.now_ns())
    }

    fn play(
        &mut self,
        buffer: &Sound,
        start_at_context_time: f64,
        offset: f64,
        volume: f32,
    ) -> Result<SoundId, String> {
        Ok(self.play_sound(buffer, start_at_context_time, offset, volume))
    }

    fn duration(&self, buffer: &Sound) -> f64 {
        buffer.duration()
    }
}

fn device_name(d: &cpal::Device) -> String {
    d.description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "output".into())
}

/// Outside macOS, whether the system's default output is now another
/// device than the stream's (macOS moves the stream itself).
fn default_changed(live: &Live) -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    cpal::default_host()
        .default_output_device()
        .is_some_and(|d| device_name(&d) != live.device)
}

/// Opens the default output, its timeline going on from `now` (the old
/// one's reading): its first frame is `now`'s context time, and its clock
/// reads on from there until its first callback.
fn open(master: f32, now: ClockSample, hal: Option<hal::Latency>) -> Result<Live, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no audio output device")?;
    let name = device_name(&device);
    let supported = device
        .default_output_config()
        .map_err(|e| format!("{name}: {e}"))?;
    let rate = supported.sample_rate();
    let channels = supported.channels().max(1);
    let mut config: cpal::StreamConfig = supported.config();
    let buffer = match supported.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } if min <= max => {
            Some(BUFFER_FRAMES.clamp(*min, *max))
        }
        _ => None,
    };
    config.buffer_size = buffer.map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed);
    let host_clock = cfg!(target_os = "macos") && host.id().name() == "CoreAudio";
    let shared = Arc::new(Shared {
        hal_ns: AtomicI64::new(if host_clock {
            hal.map_or(-1, |h| h.nanos())
        } else {
            -1
        }),
        ..Shared::default()
    });
    let first = (now.context_time.max(0.0) * f64::from(rate)).round() as u64;
    // Seeded before the stream runs (the callback is the only writer
    // after): frame `first` counts as current now and is heard after the
    // latency the old timeline had.
    let latency_ns = (now.output_latency * 1e9).round() as i64;
    let seed = Reading {
        frame: first,
        heard_ns: (now.host_time.0 * 1e9).round() as i64 + latency_ns,
        latency_ns,
        rate,
    };
    shared.published.write(&seed);
    let (commands, rx) = rtrb::RingBuffer::new(RING);
    let (back, returned) = rtrb::RingBuffer::new(RETURNED);
    // The stream's instants on the host's clock: measured once the
    // stream exists (its `now()`), so a placeholder until then.
    let timebase_cell = Arc::new(AtomicI64::new(i64::MIN));
    let state = Callbacks {
        mixer: Mixer::new(rate, master),
        commands: rx,
        back,
        shared: shared.clone(),
        frame: first,
        seed: Some(seed),
        math: None,
        timebase: timebase_cell.clone(),
        host_clock,
        rate,
        channels: usize::from(channels),
        scratch: vec![0.0; SCRATCH * 2],
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(&device, config, state),
        cpal::SampleFormat::I16 => build::<i16>(&device, config, state),
        cpal::SampleFormat::I32 => build::<i32>(&device, config, state),
        cpal::SampleFormat::U16 => build::<u16>(&device, config, state),
        other => build::<f32>(&device, config, state)
            .map_err(|e| format!("{other:?} output not supported, f32 refused: {e}")),
    }
    .map_err(|e| format!("{name}: {e}"))?;
    let timebase = if host_clock {
        let offset = stream_offset(&stream);
        timebase_cell.store(offset, Ordering::Release);
        Timebase::Host { offset_ns: offset }
    } else {
        Timebase::Anchored
    };
    stream.play().map_err(|e| format!("{name}: {e}"))?;
    Ok(Live {
        _stream: stream,
        rate,
        channels,
        buffer,
        host: host.id().name(),
        device: name,
        commands,
        returned,
        shared,
        timebase,
    })
}

/// The stream's clock less the host's, nanoseconds, from the tightest of
/// a few back-to-back readings of both.
fn stream_offset(stream: &cpal::Stream) -> i64 {
    let mut best = (u64::MAX, 0i64);
    for _ in 0..16 {
        let a = crate::clock::now_ns();
        let s = stream.now().as_nanos() as i64;
        let b = crate::clock::now_ns();
        if b - a < best.0 {
            best = (b - a, s - (a + (b - a) / 2) as i64);
        }
    }
    best.1
}

/// What the callback owns.
struct Callbacks {
    mixer: Mixer,
    commands: rtrb::Consumer<Command>,
    back: rtrb::Producer<Box<Voice>>,
    shared: Arc<Shared>,
    /// The output frame the next buffer starts on.
    frame: u64,
    /// The timeline the stream continues, until its first reading: that
    /// reading's frames are renumbered so heard time goes on from it
    /// (the time the stream took to start is not a jump).
    seed: Option<Reading>,
    math: Option<ClockMath>,
    /// The stream's offset from the host clock once known (`Host`
    /// timebase), `i64::MIN` before.
    timebase: Arc<AtomicI64>,
    host_clock: bool,
    rate: u32,
    channels: usize,
    scratch: Vec<f32>,
}

impl Callbacks {
    fn run<T: cpal::SizedSample + cpal::FromSample<f32>>(
        &mut self,
        data: &mut [T],
        info: &cpal::OutputCallbackInfo,
    ) {
        let entry = crate::clock::now_ns();
        flush_denormals();
        let ts = info.timestamp();
        let n = data.len() / self.channels;
        // The clock: once the timebase is known.
        if self.math.is_none() {
            let offset = self.timebase.load(Ordering::Acquire);
            self.math = match (self.host_clock, offset) {
                (true, i64::MIN) => None,
                (true, offset_ns) => Some(ClockMath::new(Timebase::Host { offset_ns }, self.rate)),
                (false, _) => Some(ClockMath::new(Timebase::Anchored, self.rate)),
            };
        }
        if let Some(math) = &mut self.math {
            let hal = self.shared.hal_ns.load(Ordering::Relaxed);
            let mut r: Reading = math.on_callback(&Callback {
                frame: self.frame,
                callback_ns: ts.callback.as_nanos() as u64,
                playback_ns: ts.playback.as_nanos() as u64,
                entry_ns: entry,
                hal_ns: (hal >= 0).then_some(hal),
            });
            if let Some(seed) = self.seed.take() {
                // Where the old timeline is when this frame is heard.
                let at = seed.frame as i64
                    + ((r.heard_ns - seed.heard_ns) as f64 * f64::from(self.rate) / 1e9).round()
                        as i64;
                let shift = at - r.frame as i64;
                self.frame = (self.frame as i64 + shift).max(0) as u64;
                r.frame = self.frame;
            }
            self.shared.published.write(&r);
        }
        let now = self.frame;
        let back = &mut self.back;
        let shared = &self.shared;
        // A voice is never freed here (see `RETURNED`).
        let mut give_back = |v: Box<Voice>| {
            if let Err(rtrb::PushError::Full(v)) = back.push(v) {
                std::mem::forget(v);
                shared.leaked.fetch_add(1, Ordering::Relaxed);
            }
        };
        while let Ok(c) = self.commands.pop() {
            self.mixer.apply(c, now, &mut give_back);
        }
        let mut done = 0;
        let mut peak = 0f32;
        while done < n {
            let k = (n - done).min(SCRATCH);
            let buf = &mut self.scratch[..k * 2];
            self.mixer.render(buf, now + done as u64, &mut give_back);
            peak = buf.iter().fold(peak, |m, s| m.max(s.abs()));
            let out = &mut data[done * self.channels..(done + k) * self.channels];
            match self.channels {
                1 => {
                    for (o, p) in out.iter_mut().zip(buf.as_chunks::<2>().0) {
                        *o = T::from_sample_((p[0] + p[1]) * 0.5);
                    }
                }
                ch => {
                    for (o, p) in out.chunks_exact_mut(ch).zip(buf.as_chunks::<2>().0) {
                        o[0] = T::from_sample_(p[0]);
                        o[1] = T::from_sample_(p[1]);
                        for x in &mut o[2..] {
                            *x = T::EQUILIBRIUM;
                        }
                    }
                }
            }
            done += k;
        }
        self.frame += n as u64;
        if peak > f32::from_bits(self.shared.peak.load(Ordering::Relaxed) as u32) {
            self.shared
                .peak
                .store(u64::from(peak.to_bits()), Ordering::Relaxed);
        }
        self.shared.last_entry.store(entry, Ordering::Relaxed);
        self.shared.callbacks.fetch_add(1, Ordering::Relaxed);
    }
}

/// What a stream error asks of us.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamError {
    /// The system moved the stream to another device (macOS).
    Rerouted,
    /// An underrun or overload: counted, the stream goes on (cpal reports
    /// it on CoreAudio overloads and on WASAPI, ALSA and PipeWire).
    Xrun,
    /// The device, the stream or the host is gone: open it again.
    Failed,
    /// Nothing to do (real-time priority refused, busy, …).
    Ignored,
}

fn stream_error(kind: cpal::ErrorKind) -> StreamError {
    use cpal::ErrorKind as K;
    match kind {
        K::DeviceChanged => StreamError::Rerouted,
        K::Xrun => StreamError::Xrun,
        K::DeviceNotAvailable | K::StreamInvalidated | K::HostUnavailable | K::BackendError => {
            StreamError::Failed
        }
        _ => StreamError::Ignored,
    }
}

/// Subnormal floats flushed to zero on this thread (x86, where they are
/// slow; Arm's are as fast as any).
#[inline]
fn flush_denormals() {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: reads and writes this thread's MXCSR, setting FTZ (bit 15)
    // and DAZ (bit 6), which every x86-64 CPU has; nothing else changes.
    unsafe {
        let mut csr: u32 = 0;
        std::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack));
        csr |= 0x8040;
        std::arch::asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, readonly));
    }
}

fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut state: Callbacks,
) -> Result<cpal::Stream, String> {
    let shared = state.shared.clone();
    device
        .build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| state.run(data, info),
            move |e: cpal::Error| match stream_error(e.kind()) {
                StreamError::Rerouted => shared.rerouted.store(true, Ordering::Relaxed),
                StreamError::Xrun => {
                    shared.xruns.fetch_add(1, Ordering::Relaxed);
                }
                StreamError::Failed => shared.failed.store(true, Ordering::Relaxed),
                StreamError::Ignored => {
                    shared.other_errors.fetch_add(1, Ordering::Relaxed);
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_platform::HostTime;

    #[test]
    fn clock_stats_see_drift_and_jitter() {
        // 100 ppm fast, ±10 µs alternating, 9 ms of latency.
        let samples: Vec<ClockSample> = (0..1000)
            .map(|i| {
                let host = 1.0 + i as f64 * 0.001;
                let wobble = if i % 2 == 0 { 10e-6 } else { -10e-6 };
                ClockSample {
                    context_time: host * (1.0 + 100e-6) + wobble + 0.009 + 3.0,
                    host_time: HostTime(host),
                    output_latency: 0.009,
                }
            })
            .collect();
        let s = ClockStats::of(&samples);
        assert_eq!(s.samples, 1000);
        assert!((s.drift_ppm - 100.0).abs() < 0.5, "{}", s.drift_ppm);
        assert!((s.jitter_sd - 10e-6).abs() < 1e-7, "{}", s.jitter_sd);
        assert!((s.jitter_max - 10e-6).abs() < 1e-7);
        assert!((s.latency_mean - 0.009).abs() < 1e-12);
    }

    #[test]
    fn stream_errors_reopen_only_when_the_stream_is_gone() {
        use cpal::ErrorKind as K;
        assert_eq!(stream_error(K::Xrun), StreamError::Xrun);
        assert_eq!(stream_error(K::DeviceChanged), StreamError::Rerouted);
        for k in [
            K::DeviceNotAvailable,
            K::StreamInvalidated,
            K::HostUnavailable,
            K::BackendError,
        ] {
            assert_eq!(stream_error(k), StreamError::Failed, "{k:?}");
        }
        for k in [K::RealtimeDenied, K::DeviceBusy, K::Other] {
            assert_eq!(stream_error(k), StreamError::Ignored, "{k:?}");
        }
    }

    #[test]
    fn the_timeline_goes_on_across_a_reopen() {
        // Context time never jumps or reads 0 again. On the machine's
        // real output (master 0: digital silence) only when asked
        // (`DDI_AUDIO_DEVICE_TESTS=1`): a test run should not open the
        // sound device; otherwise without one, the reopens failing.
        let device = std::env::var_os("DDI_AUDIO_DEVICE_TESTS").is_some();
        let mut a = if device {
            Audio::new(0.0)
        } else {
            Audio::silent(0.0)
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        for _ in 0..3 {
            let before = a.now();
            a.force_reopen();
            let mut last = before;
            let until = Instant::now() + std::time::Duration::from_millis(150);
            while Instant::now() < until {
                let s = a.now();
                let elapsed = s.host_time.0 - before.host_time.0;
                let heard = |s: ClockSample| s.context_time - s.output_latency;
                // Heard time moves with the host's, within a few ms (the
                // new device's first readings).
                assert!(
                    (heard(s) - heard(before) - elapsed).abs() < 0.02,
                    "{before:?} → {s:?}"
                );
                assert!(s.context_time > 0.1 || before.context_time < 0.1);
                last = s;
            }
            assert!(last.context_time >= before.context_time);
        }
        // Its identity holds still once it runs (the latency it buckets
        // is the stream's nominal one, not the measured lead).
        if a.running() {
            let f = a.fingerprint();
            assert!(f.is_some());
            std::thread::sleep(std::time::Duration::from_millis(1100));
            a.tick();
            assert_eq!(a.fingerprint(), f);
        }
        // No device: it starts at 0 and runs.
        let a = Audio::silent(1.0);
        let s0 = a.now();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let s1 = a.now();
        assert!(s0.context_time < 0.01);
        assert!(
            (s1.context_time - s0.context_time - (s1.host_time.0 - s0.host_time.0)).abs() < 1e-9
        );
    }

    #[test]
    fn without_a_device_nothing_breaks() {
        let mut a = Audio::silent(0.8);
        let s0 = a.now();
        assert_eq!(s0.output_latency, 0.0);
        let pcm = a.buffer_from_pcm(vec![vec![0.0; 441]], 44_100).unwrap();
        assert_eq!(pcm.rate, 48_000);
        assert!((pcm.frames() as i64 - 480).abs() <= 1, "{}", pcm.frames());
        let id = a.play_sound(&pcm, 0.0, 0.0, 1.0);
        a.gain_ramp(id, 0.0, 1.0);
        a.set_master_volume(0.5);
        a.master_ramp(0.0, 1.0, 0.5);
        a.stop_all();
        a.tick();
        assert!(!a.running());
        assert_eq!(a.fingerprint(), None);
        assert_eq!(a.xruns(), 0);
        assert!(a.status_line().starts_with("audio: none"));
    }
}
