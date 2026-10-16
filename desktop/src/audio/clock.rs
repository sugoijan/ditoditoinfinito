//! The audio clock: each output callback says which frame its buffer
//! starts on and when that frame is heard; [`Reading::sample`] turns the
//! latest into the web's `getOutputTimestamp` pairing
//! ([`ddi_platform::ClockSample`]), so the engine's `SongClock` works
//! unchanged (`docs/plans/desktop.md`, section 1, and step 7.0's
//! results).
//!
//! When a frame is heard, by host:
//!
//! - **CoreAudio.** cpal's `callback` is the render timestamp's
//!   `mHostTime`: not the call's time but when the buffer's first frame
//!   reaches the device, already a buffer and the safety offset ahead of
//!   the call. Its `playback` adds the buffer, the device latency and the
//!   safety offset again and leaves the stream latency out (measured in
//!   7.0: 6.9 ms late at 256 frames, 12.6 ms at 512). By Apple's model
//!   the frame is heard at `mHostTime` + device latency + stream latency,
//!   read from the HAL ([`super::hal`]). `mHostTime` is
//!   `mach_absolute_time`, the clock of Rust's `Instant` on macOS, so it
//!   converts to host time exactly ([`Timebase::Host`]).
//! - **Elsewhere** (WASAPI, ALSA: not measured yet): `playback` as cpal
//!   gives it, converted through an anchor: the smallest `entry −
//!   callback` over the last second or so, taking the earliest callback
//!   as running at its own `callback` instant ([`Timebase::Anchored`]).
//!   Holds whichever clock the host's instants use. The alternative, not
//!   taken until it can be checked: WASAPI's instants are QPC and ALSA's
//!   `CLOCK_MONOTONIC`, both the clock of Rust's `Instant` there, so
//!   [`Timebase::Host`] with an offset measured from `Stream::now()`, as on
//!   CoreAudio, may be exact too; what then decides is what `playback`
//!   means on each. A maintainer's check on such a machine (or a CI job
//!   with a loopback device) would log, over a minute, `Stream::now()`
//!   against `Instant` (a constant offset says the same clock) and the
//!   anchor's `entry − callback` against `playback − callback` (whether
//!   `callback` is the call's time or, as on CoreAudio, later), and only
//!   then switch.
//!
//! The latency reported ([`ClockSample::output_latency`]) is from the
//! moment a frame counts as current (the buffer it is in starts being
//! written) to when it is heard, so `context_time` is, at a callback's
//! start, the first frame of its buffer: Web Audio's `currentTime`.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use ddi_platform::{ClockSample, HostTime};

/// How long the anchor's and the lead's smallest values are kept (two
/// windows of this, so between one and two of them).
const WINDOW_NS: u64 = 500_000_000;

/// How a host's stream instants become host time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Timebase {
    /// Host nanoseconds are stream nanoseconds less `offset_ns` (the
    /// same clock: CoreAudio).
    Host { offset_ns: i64 },
    /// Another epoch: anchored on when callbacks run.
    Anchored,
}

/// One callback, as the audio thread saw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Callback {
    /// The output frame its buffer starts on.
    pub frame: u64,
    /// cpal's `callback` and `playback` instants, in nanoseconds of the
    /// stream's clock.
    pub callback_ns: u64,
    pub playback_ns: u64,
    /// Host nanoseconds when the callback started.
    pub entry_ns: u64,
    /// CoreAudio: the device's and its stream's latency, nanoseconds
    /// (read from the HAL); `None` when unknown (cpal's `playback` used).
    pub hal_ns: Option<i64>,
}

/// The latest callback's answer: frame `frame` is heard at host
/// nanosecond `heard_ns`, `latency_ns` after it counted as current.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub frame: u64,
    pub heard_ns: i64,
    pub latency_ns: i64,
    pub rate: u32,
}

impl Reading {
    /// The clock at host nanosecond `now_ns`: the heard position moves on
    /// at one second a second from the latest callback (the device's
    /// drift against the host is the engine's to absorb).
    pub fn sample(&self, now_ns: u64) -> ClockSample {
        let heard = self.frame as f64 / f64::from(self.rate.max(1))
            + (now_ns as i64 - self.heard_ns) as f64 / 1e9;
        let latency = self.latency_ns as f64 / 1e9;
        ClockSample {
            context_time: heard + latency,
            host_time: HostTime(now_ns as f64 / 1e9),
            output_latency: latency,
        }
    }
}

/// The smallest value over the last window or two.
#[derive(Clone, Copy, Debug)]
struct WindowMin {
    current: i64,
    previous: i64,
    since: u64,
}

impl WindowMin {
    fn new() -> WindowMin {
        WindowMin {
            current: i64::MAX,
            previous: i64::MAX,
            since: 0,
        }
    }

    fn push(&mut self, at: u64, value: i64) -> i64 {
        if at.saturating_sub(self.since) >= WINDOW_NS {
            self.previous = self.current;
            self.current = i64::MAX;
            self.since = at;
        }
        self.current = self.current.min(value);
        self.current.min(self.previous)
    }
}

/// The clock's arithmetic, run on the audio thread for each callback.
#[derive(Clone, Copy, Debug)]
pub struct ClockMath {
    timebase: Timebase,
    rate: u32,
    /// `Anchored`: `entry − callback`. `Host`: `entry − callback`'s host
    /// time (minus the lead the callback runs ahead).
    window: WindowMin,
}

impl ClockMath {
    pub fn new(timebase: Timebase, rate: u32) -> ClockMath {
        ClockMath {
            timebase,
            rate,
            window: WindowMin::new(),
        }
    }

    pub fn on_callback(&mut self, cb: &Callback) -> Reading {
        let (heard_ns, latency_ns) = match self.timebase {
            Timebase::Host { offset_ns } => {
                let callback = cb.callback_ns as i64 - offset_ns;
                // How far ahead of the call its buffer reaches the device,
                // at most over the window (the call's own lateness is
                // jitter, the earliest call is the schedule).
                let lead = -self.window.push(cb.entry_ns, cb.entry_ns as i64 - callback);
                match cb.hal_ns {
                    Some(hal) => (callback + hal, lead + hal),
                    None => {
                        let playback = cb.playback_ns as i64 - offset_ns;
                        (playback, lead + (playback - callback))
                    }
                }
            }
            Timebase::Anchored => {
                let anchor = self
                    .window
                    .push(cb.entry_ns, cb.entry_ns as i64 - cb.callback_ns as i64);
                (
                    cb.playback_ns as i64 + anchor,
                    cb.playback_ns as i64 - cb.callback_ns as i64,
                )
            }
        };
        Reading {
            frame: cb.frame,
            heard_ns,
            latency_ns: latency_ns.max(0),
            rate: self.rate,
        }
    }
}

/// The latest [`Reading`], written by the audio thread and read by any
/// other through a sequence lock (no lock, no allocation).
#[derive(Debug, Default)]
pub struct Published {
    seq: AtomicU64,
    frame: AtomicU64,
    heard: AtomicI64,
    latency: AtomicI64,
    rate: AtomicU64,
}

impl Published {
    /// From the one writer.
    pub fn write(&self, r: &Reading) {
        let s = self.seq.load(Ordering::Relaxed);
        self.seq.store(s.wrapping_add(1), Ordering::Relaxed);
        std::sync::atomic::fence(Ordering::Release);
        self.frame.store(r.frame, Ordering::Relaxed);
        self.heard.store(r.heard_ns, Ordering::Relaxed);
        self.latency.store(r.latency_ns, Ordering::Relaxed);
        self.rate.store(u64::from(r.rate), Ordering::Relaxed);
        self.seq.store(s.wrapping_add(2), Ordering::Release);
    }

    /// The latest reading; `None` before the first callback.
    pub fn read(&self) -> Option<Reading> {
        loop {
            let s0 = self.seq.load(Ordering::Acquire);
            if s0 % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let r = Reading {
                frame: self.frame.load(Ordering::Relaxed),
                heard_ns: self.heard.load(Ordering::Relaxed),
                latency_ns: self.latency.load(Ordering::Relaxed),
                rate: self.rate.load(Ordering::Relaxed) as u32,
            };
            std::sync::atomic::fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == s0 {
                return (s0 != 0).then_some(r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000_000;

    /// CoreAudio's callbacks as 7.0 measured them on the headphone jack
    /// at 256 frames, 44.1 kHz: called every 5.805 ms with up to 0.3 ms
    /// of lateness, `mHostTime` 7.38 ms after the call's schedule, cpal's
    /// `playback` 392 frames after that, the HAL's device and stream
    /// latencies 66 + 24 frames. Stream instants on the host's clock,
    /// 5 s ahead of the epoch.
    fn coreaudio(n: u64) -> Vec<Callback> {
        let rate = 44_100.0;
        let period = 256.0 / rate * 1e9;
        (0..n)
            .map(|i| {
                let schedule = (1e9 + i as f64 * period) as u64;
                let late = (i * 7919 % 300) * 1_000; // 0–0.3 ms
                let host = schedule + 7_380_000;
                let callback = host + 5_000 * MS;
                Callback {
                    frame: i * 256,
                    callback_ns: callback,
                    playback_ns: callback + (392.0 / rate * 1e9) as u64,
                    entry_ns: schedule + late,
                    hal_ns: Some((90.0 / rate * 1e9) as i64),
                }
            })
            .collect()
    }

    #[test]
    fn coreaudio_hears_a_frame_at_its_host_time_plus_the_hal_latencies() {
        let mut m = ClockMath::new(
            Timebase::Host {
                offset_ns: 5_000 * MS as i64,
            },
            44_100,
        );
        let cbs = coreaudio(400);
        let mut last = None;
        for cb in &cbs {
            last = Some(m.on_callback(cb));
        }
        let r = last.unwrap();
        let cb = cbs.last().unwrap();
        let host = cb.callback_ns as i64 - 5_000 * MS as i64;
        // Heard at mHostTime + 90 frames (2.04 ms), not cpal's
        // callback + 392 frames (8.9 ms).
        assert_eq!(r.heard_ns, host + (90.0 / 44_100.0 * 1e9) as i64);
        // The latency: the lead (7.38 ms, the earliest call's) plus 2.04 ms.
        let want = 7_380_000 + (90.0 / 44_100.0 * 1e9) as i64;
        assert!((r.latency_ns - want).abs() < 1_000, "{}", r.latency_ns);
        // At the callback's schedule, context_time is its first frame
        // (`currentTime`), and the heard position that less the latency.
        let i = cbs.len() as u64 - 1;
        let schedule = cb.entry_ns - (i * 7919 % 300) * 1_000;
        let s = r.sample(schedule);
        assert!(
            (s.context_time - r.frame as f64 / 44_100.0).abs() < 2e-6,
            "{} {}",
            s.context_time,
            r.frame as f64 / 44_100.0
        );
        let heard = s.context_time - s.output_latency;
        assert!((heard - (r.frame as f64 / 44_100.0 - want as f64 / 1e9)).abs() < 2e-6);
    }

    #[test]
    fn heard_time_does_not_jitter_with_the_calls() {
        // The heard position against host time is a line through every
        // callback: the calls' lateness never reaches it.
        let mut m = ClockMath::new(
            Timebase::Host {
                offset_ns: 5_000 * MS as i64,
            },
            44_100,
        );
        let cbs = coreaudio(1_000);
        let first = -(cbs[0].callback_ns as f64 - 5e9 + 90.0 / 44_100.0 * 1e9) / 1e9;
        for cb in &cbs {
            let r = m.on_callback(cb);
            // frame / rate − heard time is the same for every callback.
            let line = r.frame as f64 / 44_100.0 - r.heard_ns as f64 / 1e9;
            assert!((line - first).abs() < 2e-9, "{line} {first}");
        }
    }

    #[test]
    fn without_the_hal_cpal_s_playback_is_used() {
        let mut m = ClockMath::new(Timebase::Host { offset_ns: 0 }, 48_000);
        let r = m.on_callback(&Callback {
            frame: 0,
            callback_ns: 10 * MS,
            playback_ns: 18 * MS,
            entry_ns: 3 * MS,
            hal_ns: None,
        });
        assert_eq!(r.heard_ns, 18 * MS as i64);
        assert_eq!(r.latency_ns, (7 + 8) * MS as i64);
    }

    #[test]
    fn the_anchor_takes_the_earliest_call_and_forgets_old_ones() {
        // Stream instants on their own epoch (1000 s); each callback's
        // `callback` is its call time there; playback 10 ms later.
        let mut m = ClockMath::new(Timebase::Anchored, 48_000);
        let epoch = 1_000_000 * MS;
        let mut r = None;
        for i in 0..100u64 {
            let at = 2_000 * MS + i * 5 * MS;
            // Called late by 0–2 ms, once 0.
            let late = if i == 37 { 0 } else { (1 + i % 2) * MS };
            r = Some(m.on_callback(&Callback {
                frame: i * 240,
                callback_ns: epoch + at,
                playback_ns: epoch + at + 10 * MS,
                entry_ns: at + late,
                hal_ns: None,
            }));
        }
        let r = r.unwrap();
        // The anchor is the exact one (i = 37's): heard 10 ms after the
        // call's schedule.
        assert_eq!(r.heard_ns, (2_000 + 99 * 5 + 10) as i64 * MS as i64);
        assert_eq!(r.latency_ns, 10 * MS as i64);
        // A second and more later, that callback is forgotten.
        for i in 100..400u64 {
            let at = 2_000 * MS + i * 5 * MS;
            m.on_callback(&Callback {
                frame: i * 240,
                callback_ns: epoch + at,
                playback_ns: epoch + at + 10 * MS,
                entry_ns: at + MS,
                hal_ns: None,
            });
        }
        let last = m.on_callback(&Callback {
            frame: 400 * 240,
            callback_ns: epoch + 4_000 * MS,
            playback_ns: epoch + 4_010 * MS,
            entry_ns: 4_000 * MS + MS,
            hal_ns: None,
        });
        assert_eq!(last.heard_ns, (4_011 * MS) as i64);
    }

    #[test]
    fn readings_cross_threads_whole() {
        let p = std::sync::Arc::new(Published::default());
        assert_eq!(p.read(), None);
        let w = p.clone();
        let writer = std::thread::spawn(move || {
            for i in 1..200_000u64 {
                w.write(&Reading {
                    frame: i,
                    heard_ns: i as i64 * 3,
                    latency_ns: i as i64 * 5,
                    rate: 48_000,
                });
            }
        });
        let mut seen = 0;
        while !writer.is_finished() {
            if let Some(r) = p.read() {
                assert_eq!(r.heard_ns, r.frame as i64 * 3);
                assert_eq!(r.latency_ns, r.frame as i64 * 5);
                seen += 1;
            }
        }
        writer.join().unwrap();
        assert!(seen > 0);
        assert_eq!(p.read().unwrap().frame, 199_999);
    }
}
