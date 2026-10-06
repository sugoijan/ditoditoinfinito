//! Song clock: maps host timestamps to heard song time.
//!
//! The audio clock (`AudioContext.currentTime`) is the truth for what the
//! player hears, but inputs and frames are stamped with the host clock.
//! [`SongClock`] keeps a low-pass filtered estimate of
//! `context_time − host_time` from [`ClockSample`]s and uses it to convert.

use ddi_platform::{ClockSample, HostTime};
use serde::{Deserialize, Serialize};

/// User-facing clock settings.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClockOptions {
    /// Calibration in seconds, added to the estimated output latency:
    /// positive means the player hears (and so hits) later than the audio
    /// clock says.
    pub audio_offset: f64,
    /// Seconds added to the render time only (DDR "DISPLAY TIMING").
    pub visual_offset: f64,
    /// Music rate: song seconds per real second.
    pub rate: f64,
}

impl Default for ClockOptions {
    fn default() -> ClockOptions {
        ClockOptions {
            audio_offset: 0.0,
            visual_offset: 0.0,
            rate: 1.0,
        }
    }
}

/// Smoothing factor of the drift estimate per accepted sample.
const FILTER_ALPHA: f64 = 0.1;
/// A sample whose drift differs from the estimate by more than this is
/// rejected as a glitch…
const MAX_JUMP: f64 = 0.05;
/// …unless this many consecutive samples agree with each other, in which
/// case the clock really did jump and we re-anchor.
const REANCHOR_AFTER: u32 = 8;
/// The drift actually applied to conversions chases the filtered estimate
/// at most this fast, in seconds per host second. Song time therefore
/// advances at between `1 − SLEW_RATE` and `1 + SLEW_RATE` times the host
/// rate (× `rate`) and can never run backwards or jump when a sample
/// nudges the estimate; only a re-anchor jumps.
const SLEW_RATE: f64 = 0.05;

/// Anchor between the audio clock and the host clock.
#[derive(Clone, Debug, PartialEq)]
pub struct SongClock {
    options: ClockOptions,
    /// Filtered `context_time − host_time`.
    drift: Option<f64>,
    /// Drift in effect at `applied_at`; moves towards `drift` by at most
    /// `SLEW_RATE` × elapsed host time.
    applied: f64,
    /// Host time of the sample that last updated `applied`.
    applied_at: f64,
    output_latency: f64,
    rejected: u32,
    started: bool,
    start_context_time: f64,
    song_offset: f64,
}

impl SongClock {
    pub fn new(options: ClockOptions) -> SongClock {
        SongClock {
            options,
            drift: None,
            applied: 0.0,
            applied_at: 0.0,
            output_latency: 0.0,
            rejected: 0,
            started: false,
            start_context_time: 0.0,
            song_offset: 0.0,
        }
    }

    pub fn options(&self) -> &ClockOptions {
        &self.options
    }

    pub fn set_options(&mut self, options: ClockOptions) {
        self.options = options;
    }

    /// The audio was scheduled to play from `song_offset` seconds into the
    /// song at `start_context_time` on the audio clock.
    pub fn start(&mut self, start_context_time: f64, song_offset: f64) {
        self.started = true;
        self.start_context_time = start_context_time;
        self.song_offset = song_offset;
    }

    pub fn started(&self) -> bool {
        self.started
    }

    /// Latest accepted output latency estimate.
    pub fn output_latency(&self) -> f64 {
        self.output_latency
    }

    /// Current drift estimate (`context_time − host_time`), if any.
    pub fn drift(&self) -> Option<f64> {
        self.drift
    }

    /// Refreshes the anchor with a new reading of both clocks.
    pub fn update(&mut self, sample: ClockSample) {
        if !sample.context_time.is_finite() || !sample.host_time.0.is_finite() {
            return;
        }
        let d = sample.context_time - sample.host_time.0;
        match self.drift {
            None => {
                self.drift = Some(d);
                self.applied = d;
                self.applied_at = sample.host_time.0;
            }
            Some(f) if (d - f).abs() > MAX_JUMP => {
                self.rejected += 1;
                if self.rejected >= REANCHOR_AFTER {
                    self.drift = Some(d);
                    self.applied = d;
                    self.applied_at = sample.host_time.0;
                    self.rejected = 0;
                }
                return;
            }
            Some(f) => {
                self.rejected = 0;
                // Bank the slew progress up to this sample, then move the
                // target; `applied_at` never goes backwards so an out-of-order
                // sample cannot rewind conversions.
                let host = sample.host_time.0.max(self.applied_at);
                self.applied = self.applied_drift_at(host);
                self.applied_at = host;
                self.drift = Some(f + FILTER_ALPHA * (d - f));
            }
        }
        if sample.output_latency.is_finite() && sample.output_latency >= 0.0 {
            self.output_latency = sample.output_latency;
        }
    }

    /// Drift used for conversions at `host`: the last applied value moved
    /// towards the filtered estimate by at most `SLEW_RATE` per host second.
    fn applied_drift_at(&self, host: f64) -> f64 {
        let Some(target) = self.drift else {
            return 0.0;
        };
        let max_step = SLEW_RATE * (host - self.applied_at).abs();
        self.applied + (target - self.applied).clamp(-max_step, max_step)
    }

    /// Estimated audio-clock reading at a host time.
    pub fn context_time_at(&self, host: HostTime) -> f64 {
        host.0 + self.applied_drift_at(host.0)
    }

    /// Song time the player was hearing at `host`:
    /// `song_offset + (context_time_at(host) − start − output_latency − audio_offset) × rate`.
    pub fn song_time_at_host(&self, host: HostTime) -> f64 {
        let elapsed = self.context_time_at(host)
            - self.start_context_time
            - self.output_latency
            - self.options.audio_offset;
        self.song_offset + elapsed * self.options.rate
    }

    /// Song time to draw for a frame that will be presented at
    /// `predicted_present_host` (adds `visual_offset`).
    pub fn render_time(&self, predicted_present_host: HostTime) -> f64 {
        self.song_time_at_host(predicted_present_host) + self.options.visual_offset
    }

    /// Song time being heard right now.
    pub fn heard_now(&self, host_now: HostTime) -> f64 {
        self.song_time_at_host(host_now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(context: f64, host: f64, latency: f64) -> ClockSample {
        ClockSample {
            context_time: context,
            host_time: HostTime(host),
            output_latency: latency,
        }
    }

    #[test]
    fn zero_latency_identity() {
        let mut c = SongClock::new(ClockOptions::default());
        c.update(sample(10.0, 10.0, 0.0));
        c.start(11.0, 0.0);
        assert!((c.song_time_at_host(HostTime(11.5)) - 0.5).abs() < 1e-12);
        assert!((c.heard_now(HostTime(13.0)) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn latency_offsets_and_rate() {
        let mut c = SongClock::new(ClockOptions {
            audio_offset: 0.02,
            visual_offset: -0.01,
            rate: 1.5,
        });
        // Audio clock runs 100 s behind the host clock.
        c.update(sample(5.0, 105.0, 0.1));
        c.start(5.0, 30.0);
        // At host 106: context 6.0; elapsed 1.0 − 0.1 − 0.02 = 0.88; × 1.5 = 1.32.
        assert!((c.song_time_at_host(HostTime(106.0)) - 31.32).abs() < 1e-9);
        assert!((c.render_time(HostTime(106.0)) - 31.31).abs() < 1e-9);
    }

    #[test]
    fn jittery_anchor_is_smoothed() {
        let mut c = SongClock::new(ClockOptions::default());
        let mut seed: u64 = 12345;
        let mut noise = || {
            // Deterministic LCG in ±3 ms.
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 0.006
        };
        for i in 0..200 {
            let host = i as f64 * 0.016;
            c.update(sample(host + 0.5 + noise(), host, 0.0));
        }
        c.start(0.5, 0.0);
        // Ideal: song_time = host. The estimate should be within 2 ms.
        for host in [3.2, 3.25, 4.0] {
            assert!((c.song_time_at_host(HostTime(host)) - host).abs() < 0.002);
        }
    }

    #[test]
    fn glitches_rejected_then_reanchored() {
        let mut c = SongClock::new(ClockOptions::default());
        for i in 0..20 {
            let host = i as f64 * 0.016;
            c.update(sample(host + 1.0, host, 0.0));
        }
        let before = c.drift().unwrap();
        c.update(sample(10.0, 0.5, 0.0)); // implausible jump
        assert_eq!(c.drift().unwrap(), before);
        for i in 0..REANCHOR_AFTER {
            c.update(sample(2.0 + i as f64 * 0.016, 0.6 + i as f64 * 0.016, 0.0));
        }
        assert!((c.drift().unwrap() - 1.4).abs() < 1e-9);
    }

    #[test]
    fn no_samples_means_no_drift() {
        let mut c = SongClock::new(ClockOptions::default());
        c.start(2.0, 0.0);
        assert!((c.song_time_at_host(HostTime(3.0)) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn song_time_is_monotonic_and_continuous_under_jitter() {
        let mut c = SongClock::new(ClockOptions {
            rate: 1.5,
            ..ClockOptions::default()
        });
        let mut seed: u64 = 99;
        let mut rnd = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        // True drift 0.5 s; samples every 16 ms with ±8 ms jitter and an
        // occasional 40 ms outlier (accepted: below MAX_JUMP).
        c.update(sample(0.5, 0.0, 0.0));
        c.start(0.5, 0.0);
        let mut prev = f64::NEG_INFINITY;
        let mut host_ms = 1;
        while host_ms <= 10_000 {
            let host = f64::from(host_ms) / 1000.0;
            if host_ms % 16 == 0 {
                let noise = if host_ms % 1600 == 0 {
                    0.040
                } else {
                    (rnd() - 0.5) * 0.016
                };
                c.update(sample(host + 0.5 + noise, host, 0.0));
            }
            let t = c.song_time_at_host(HostTime(host));
            assert!(
                t >= prev,
                "song time went backwards at host {host}: {prev} -> {t}"
            );
            // Continuous: one host millisecond moves song time by 1.5 ms ± slew.
            if prev.is_finite() {
                assert!(
                    (t - prev - 0.0015).abs() <= 0.0015 * SLEW_RATE + 1e-12,
                    "jump at host {host}: {}",
                    t - prev
                );
            }
            // And it tracks the truth (host × 1.5): an accepted 40 ms outlier
            // pulls the EMA by FILTER_ALPHA × 40 ms = 4 ms, × rate.
            assert!(
                (t - host * 1.5).abs() < 0.0075,
                "drifted at host {host}: {t}"
            );
            prev = t;
            host_ms += 1;
        }
    }

    #[test]
    fn output_latency_and_offsets_apply_where_they_should() {
        let mut c = SongClock::new(ClockOptions {
            audio_offset: 0.02,
            visual_offset: 0.03,
            rate: 1.0,
        });
        c.update(sample(0.0, 0.0, 0.1));
        c.start(0.0, 0.0);
        // Heard time: host − latency − audio_offset. Render adds visual only.
        assert!((c.heard_now(HostTime(1.0)) - 0.88).abs() < 1e-12);
        assert!((c.render_time(HostTime(1.0)) - 0.91).abs() < 1e-12);
        // A new latency estimate shifts heard time at once, render by the same.
        c.update(sample(1.0, 1.0, 0.2));
        assert!((c.heard_now(HostTime(1.0)) - 0.78).abs() < 1e-12);
        assert!((c.render_time(HostTime(1.0)) - 0.81).abs() < 1e-12);
    }
}
