//! Beat ↔ time conversion with StepMania semantics.
//!
//! Event ordering on a shared beat follows `TimingData::FindEvent`:
//! warp destination, BPM change, delay, *the queried beat*, stop, warp.
//! So a stop on a row applies **after** that row's notes and a delay
//! **before** them, and notes inside a warp are skipped.

use serde::{Deserialize, Serialize};

use crate::model::Tick;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BpmSegment {
    pub tick: Tick,
    pub bpm: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StopSegment {
    pub tick: Tick,
    pub seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WarpSegment {
    pub tick: Tick,
    pub length: Tick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeedUnit {
    Beats,
    Seconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeedSegment {
    pub tick: Tick,
    pub ratio: f64,
    /// Interpolation length from the previous ratio, in `unit`s.
    pub delay: f64,
    pub unit: SpeedUnit,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScrollSegment {
    pub tick: Tick,
    pub ratio: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FakeSegment {
    pub tick: Tick,
    pub length: Tick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub tick: Tick,
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickCount {
    pub tick: Tick,
    pub ticks_per_beat: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComboSegment {
    pub tick: Tick,
    pub combo: u32,
    pub miss_combo: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub tick: Tick,
    pub text: String,
}

/// Result of `TimingMap::position_at`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimePosition {
    /// Fractional beat.
    pub beat: f64,
    pub in_stop: bool,
    pub in_delay: bool,
    pub in_warp: bool,
}

/// All timing segments of a song or chart, each sorted by tick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimingMap {
    /// SM sign convention: `time(beat 0) = -offset_seconds`.
    pub offset_seconds: f64,
    /// Non-empty; first segment is at tick 0.
    pub bpms: Vec<BpmSegment>,
    pub stops: Vec<StopSegment>,
    pub delays: Vec<StopSegment>,
    pub warps: Vec<WarpSegment>,
    pub speeds: Vec<SpeedSegment>,
    pub scrolls: Vec<ScrollSegment>,
    pub fakes: Vec<FakeSegment>,
    pub time_signatures: Vec<TimeSignature>,
    pub tick_counts: Vec<TickCount>,
    pub combos: Vec<ComboSegment>,
    pub labels: Vec<Label>,
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum EventKind {
    WarpDest = 0,
    Bpm = 1,
    Delay = 2,
    Marker = 3,
    Stop = 4,
    Warp = 5,
}

impl TimingMap {
    /// Constant-tempo map.
    pub fn constant(bpm: f64, offset_seconds: f64) -> TimingMap {
        TimingMap {
            offset_seconds,
            bpms: vec![BpmSegment {
                tick: Tick::ZERO,
                bpm,
            }],
            stops: Vec::new(),
            delays: Vec::new(),
            warps: Vec::new(),
            speeds: Vec::new(),
            scrolls: Vec::new(),
            fakes: Vec::new(),
            time_signatures: Vec::new(),
            tick_counts: Vec::new(),
            combos: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// Sort every segment list, drop segments before tick 0 (folding stops into
    /// the offset like StepMania), and make sure a BPM exists at tick 0.
    /// Two segments of one kind on the same tick keep the later one, as
    /// StepMania's `TimingData::AddSegment` overwrites.
    pub fn tidy(&mut self) {
        self.bpms.sort_by_key(|s| s.tick);
        self.bpms.retain(|s| s.bpm > 0.0);
        dedup_keep_last(&mut self.bpms, |s| s.tick);
        if self.bpms.is_empty() {
            self.bpms.push(BpmSegment {
                tick: Tick::ZERO,
                bpm: 60.0,
            });
        }
        // Drop BPMs before 0; force the first one to tick 0.
        while self.bpms.len() > 1 && self.bpms[1].tick <= Tick::ZERO {
            self.bpms.remove(0);
        }
        self.bpms[0].tick = Tick::ZERO;

        for list in [&mut self.stops, &mut self.delays] {
            list.sort_by_key(|s| s.tick);
            let mut folded = 0.0;
            list.retain(|s| {
                if s.tick < Tick::ZERO {
                    folded += s.seconds;
                    false
                } else {
                    s.seconds > 0.0
                }
            });
            dedup_keep_last(list, |s| s.tick);
            self.offset_seconds -= folded;
        }
        self.warps.sort_by_key(|s| s.tick);
        self.warps.retain(|w| w.length.0 > 0);
        self.speeds.sort_by_key(|s| s.tick);
        self.scrolls.sort_by_key(|s| s.tick);
        self.fakes.sort_by_key(|s| s.tick);
        self.fakes.retain(|f| f.length.0 > 0);
        self.time_signatures.sort_by_key(|s| s.tick);
        self.tick_counts.sort_by_key(|s| s.tick);
        self.combos.sort_by_key(|s| s.tick);
        self.labels.sort_by_key(|s| s.tick);
    }

    pub fn bpm_at_beat(&self, beat: f64) -> f64 {
        let mut bpm = self.bpms[0].bpm;
        for s in &self.bpms {
            if s.tick.beat() <= beat {
                bpm = s.bpm;
            } else {
                break;
            }
        }
        bpm
    }

    /// Seconds (song time) at which `tick` is judged.
    pub fn seconds_at(&self, tick: Tick) -> f64 {
        self.seconds_at_beat(tick.beat())
    }

    /// Seconds (song time) for a fractional beat.
    pub fn seconds_at_beat(&self, target: f64) -> f64 {
        let mut time = -self.offset_seconds;
        let mut beat = 0.0;
        let mut bps = self.bpms[0].bpm / 60.0;
        let (mut ib, mut id, mut is, mut iw) = (1usize, 0usize, 0usize, 0usize);
        let mut warping = false;
        let mut warp_dest = f64::NEG_INFINITY;
        loop {
            let mut ev_beat = target;
            let mut kind = EventKind::Marker;
            let mut consider = |b: f64, k: EventKind| {
                if b < ev_beat || (b == ev_beat && (k as u8) < (kind as u8)) {
                    ev_beat = b;
                    kind = k;
                }
            };
            if warping {
                consider(warp_dest, EventKind::WarpDest);
            }
            if let Some(s) = self.bpms.get(ib) {
                consider(s.tick.beat(), EventKind::Bpm);
            }
            if let Some(s) = self.delays.get(id) {
                consider(s.tick.beat(), EventKind::Delay);
            }
            if let Some(s) = self.stops.get(is) {
                consider(s.tick.beat(), EventKind::Stop);
            }
            if let Some(s) = self.warps.get(iw) {
                consider(s.tick.beat(), EventKind::Warp);
            }
            if !warping {
                time += (ev_beat - beat) / bps;
            }
            beat = ev_beat;
            match kind {
                EventKind::WarpDest => warping = false,
                EventKind::Bpm => {
                    bps = self.bpms[ib].bpm / 60.0;
                    ib += 1;
                }
                EventKind::Delay => {
                    time += self.delays[id].seconds;
                    id += 1;
                }
                EventKind::Marker => return time,
                EventKind::Stop => {
                    time += self.stops[is].seconds;
                    is += 1;
                }
                EventKind::Warp => {
                    let w = self.warps[iw];
                    warping = true;
                    warp_dest = warp_dest.max((w.tick + w.length).beat());
                    iw += 1;
                }
            }
        }
    }

    /// Beat position for a song time, with freeze flags.
    pub fn position_at(&self, target: f64) -> TimePosition {
        let mut time = -self.offset_seconds;
        let mut beat = 0.0;
        let mut bps = self.bpms[0].bpm / 60.0;
        let (mut ib, mut id, mut is, mut iw) = (1usize, 0usize, 0usize, 0usize);
        let mut warping = false;
        let mut warp_dest = f64::NEG_INFINITY;
        loop {
            let mut ev_beat = f64::INFINITY;
            let mut kind = EventKind::Marker;
            let mut consider = |b: f64, k: EventKind| {
                if b < ev_beat || (b == ev_beat && (k as u8) < (kind as u8)) {
                    ev_beat = b;
                    kind = k;
                }
            };
            if warping {
                consider(warp_dest, EventKind::WarpDest);
            }
            if let Some(s) = self.bpms.get(ib) {
                consider(s.tick.beat(), EventKind::Bpm);
            }
            if let Some(s) = self.delays.get(id) {
                consider(s.tick.beat(), EventKind::Delay);
            }
            if let Some(s) = self.stops.get(is) {
                consider(s.tick.beat(), EventKind::Stop);
            }
            if let Some(s) = self.warps.get(iw) {
                consider(s.tick.beat(), EventKind::Warp);
            }
            if kind == EventKind::Marker {
                // No more events.
                return TimePosition {
                    beat: beat + (target - time) * bps,
                    in_stop: false,
                    in_delay: false,
                    in_warp: false,
                };
            }
            let ev_time = if warping {
                time
            } else {
                time + (ev_beat - beat) / bps
            };
            if ev_time > target {
                if warping {
                    return TimePosition {
                        beat: warp_dest,
                        in_stop: false,
                        in_delay: false,
                        in_warp: true,
                    };
                }
                return TimePosition {
                    beat: beat + (target - time) * bps,
                    in_stop: false,
                    in_delay: false,
                    in_warp: false,
                };
            }
            time = ev_time;
            beat = ev_beat;
            match kind {
                EventKind::WarpDest => warping = false,
                EventKind::Bpm => {
                    bps = self.bpms[ib].bpm / 60.0;
                    ib += 1;
                }
                EventKind::Delay | EventKind::Stop => {
                    let (secs, is_delay) = if kind == EventKind::Delay {
                        id += 1;
                        (self.delays[id - 1].seconds, true)
                    } else {
                        is += 1;
                        (self.stops[is - 1].seconds, false)
                    };
                    if time + secs > target {
                        return TimePosition {
                            beat,
                            in_stop: !is_delay,
                            in_delay: is_delay,
                            in_warp: false,
                        };
                    }
                    time += secs;
                }
                EventKind::Marker => unreachable!(),
                EventKind::Warp => {
                    let w = self.warps[iw];
                    warping = true;
                    warp_dest = warp_dest.max((w.tick + w.length).beat());
                    iw += 1;
                }
            }
        }
    }

    pub fn beat_at(&self, seconds: f64) -> f64 {
        self.position_at(seconds).beat
    }

    /// Displayed beat after applying scroll segments (`#SCROLLS`).
    pub fn displayed_beat(&self, beat: f64) -> f64 {
        if self.scrolls.is_empty() {
            return beat;
        }
        let mut out = 0.0;
        let mut prev_beat = 0.0;
        let mut ratio = 1.0;
        for s in &self.scrolls {
            let b = s.tick.beat();
            if b >= beat {
                break;
            }
            out += (b - prev_beat) * ratio;
            prev_beat = b;
            ratio = s.ratio;
        }
        out + (beat - prev_beat) * ratio
    }

    /// Seconds of `#DELAYS` sitting exactly on `tick` (0 when none).
    pub fn delay_at(&self, tick: Tick) -> f64 {
        self.delays
            .iter()
            .filter(|d| d.tick == tick)
            .map(|d| d.seconds)
            .sum()
    }

    /// Speed multiplier from `#SPEEDS` at a song position.
    ///
    /// As `TimingData::GetDisplayedSpeedPercent`: the ramp from the previous
    /// ratio runs over `delay` beats or seconds measured from the segment's
    /// start, and is interpolated **in time** for both units (a beat-length
    /// ramp is converted with the timing map, so stops inside it stretch the
    /// ramp). The start time excludes any delay on the segment's own row.
    pub fn speed_ratio_at(&self, seconds: f64, beat: f64) -> f64 {
        let Some(idx) = self.speeds.iter().rposition(|s| s.tick.beat() <= beat) else {
            return 1.0;
        };
        let seg = self.speeds[idx];
        let prev = if idx == 0 {
            1.0
        } else {
            self.speeds[idx - 1].ratio
        };
        if seg.delay <= 0.0 {
            return seg.ratio;
        }
        let start_time = self.seconds_at(seg.tick) - self.delay_at(seg.tick);
        let end_time = match seg.unit {
            SpeedUnit::Beats => {
                let end_beat = seg.tick.beat() + seg.delay;
                self.seconds_at_beat(end_beat) - self.delay_at(Tick::from_beat_f64(end_beat))
            }
            SpeedUnit::Seconds => start_time + seg.delay,
        };
        let duration = end_time - start_time;
        if duration <= 0.0 {
            return seg.ratio;
        }
        let progress = ((seconds - start_time) / duration).clamp(0.0, 1.0);
        prev + (seg.ratio - prev) * progress
    }

    pub fn in_warp(&self, tick: Tick) -> bool {
        // A stop or delay exactly on the row makes it judgeable again
        // ("stop, warp, stop, warp" gimmicks).
        if self.stops.iter().any(|s| s.tick == tick) || self.delays.iter().any(|s| s.tick == tick) {
            return false;
        }
        self.warps
            .iter()
            .any(|w| tick >= w.tick && tick < w.tick + w.length)
    }

    pub fn in_fake(&self, tick: Tick) -> bool {
        self.fakes
            .iter()
            .any(|f| tick >= f.tick && tick < f.tick + f.length)
    }

    /// Whether a note at `tick` is judged at all.
    pub fn judgeable(&self, tick: Tick) -> bool {
        !self.in_warp(tick) && !self.in_fake(tick)
    }

    /// Lowest and highest BPM actually used.
    pub fn bpm_range(&self) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for s in &self.bpms {
            lo = lo.min(s.bpm);
            hi = hi.max(s.bpm);
        }
        (lo, hi)
    }
}

/// Removes consecutive elements with equal keys, keeping the **last** one.
fn dedup_keep_last<T, K: PartialEq>(list: &mut Vec<T>, key: impl Fn(&T) -> K) {
    list.reverse();
    list.dedup_by_key(|s| key(s));
    list.reverse();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// Deterministic LCG for the generated-map tests.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }

        fn unit(&mut self) -> f64 {
            self.next() as f64 / (1u64 << 31) as f64
        }
    }

    /// A random map with BPM changes, stops, delays and warps (some sharing
    /// rows), tidied.
    fn random_map(rng: &mut Lcg) -> TimingMap {
        let mut t = TimingMap::constant(60.0 + rng.unit() * 240.0, rng.unit() - 0.5);
        for _ in 0..rng.below(5) {
            t.bpms.push(BpmSegment {
                tick: Tick(rng.below(64 * 48) as i64),
                bpm: 30.0 + rng.unit() * 400.0,
            });
        }
        for _ in 0..rng.below(4) {
            t.stops.push(StopSegment {
                tick: Tick(rng.below(64 * 48) as i64),
                seconds: 0.05 + rng.unit() * 2.0,
            });
        }
        for _ in 0..rng.below(4) {
            t.delays.push(StopSegment {
                tick: Tick(rng.below(64 * 48) as i64),
                seconds: 0.05 + rng.unit() * 2.0,
            });
        }
        for _ in 0..rng.below(3) {
            t.warps.push(WarpSegment {
                tick: Tick(rng.below(64 * 48) as i64),
                length: Tick(1 + rng.below(8 * 48) as i64),
            });
        }
        t.tidy();
        t
    }

    #[test]
    fn beat_time_beat_roundtrip_on_generated_maps() {
        let mut rng = Lcg(0xDD1_C0FFEE);
        for _ in 0..400 {
            let t = random_map(&mut rng);
            for _ in 0..40 {
                let beat = rng.unit() * 70.0;
                let tick = Tick::from_beat_f64(beat);
                // Beats skipped by a warp have no time of their own.
                if t.warps
                    .iter()
                    .any(|w| tick >= w.tick && tick < w.tick + w.length)
                {
                    continue;
                }
                let seconds = t.seconds_at(tick);
                let back = t.position_at(seconds);
                // Times must never decrease along the beat axis.
                let later = t.seconds_at(Tick(tick.0 + 1));
                assert!(later >= seconds - 1e-9, "{t:?}: time decreased at {tick:?}");
                let is_warp_dest = t.warps.iter().any(|w| w.tick + w.length == tick);
                if is_warp_dest {
                    // The warp's start and destination share one instant; the
                    // reported beat is the destination.
                    assert!(
                        back.beat >= tick.beat() - 1e-6,
                        "{t:?}: {tick:?} -> {back:?}"
                    );
                    continue;
                }
                assert!(
                    (back.beat - tick.beat()).abs() < 1e-6,
                    "{t:?}: beat {} -> {seconds} -> {back:?}",
                    tick.beat()
                );
            }
        }
    }

    #[test]
    fn time_beat_time_roundtrip_is_monotonic_on_generated_maps() {
        let mut rng = Lcg(0xBEEF);
        for _ in 0..400 {
            let t = random_map(&mut rng);
            let mut prev_beat = f64::NEG_INFINITY;
            let end = t.seconds_at_beat(70.0);
            let mut s = -t.offset_seconds - 1.0;
            while s < end {
                let p = t.position_at(s);
                assert!(p.beat >= prev_beat - 1e-9, "{t:?}: beat decreased at {s}");
                prev_beat = p.beat;
                if !p.in_stop && !p.in_delay && !p.in_warp {
                    let again = t.seconds_at_beat(p.beat);
                    assert!(
                        (again - s).abs() < 1e-6,
                        "{t:?}: {s} -> {} -> {again}",
                        p.beat
                    );
                }
                s += 0.0371;
            }
        }
    }

    #[test]
    fn stop_and_delay_on_one_row_and_bpm_change_inside_it() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.bpms.push(BpmSegment {
            tick: Tick::from_beats(4),
            bpm: 120.0,
        });
        t.stops.push(StopSegment {
            tick: Tick::from_beats(4),
            seconds: 1.0,
        });
        t.delays.push(StopSegment {
            tick: Tick::from_beats(4),
            seconds: 0.5,
        });
        t.tidy();
        // Row 4 is judged after the delay and before the stop.
        assert!(approx(t.seconds_at_beat(4.0), 4.5));
        // The new BPM applies after the row: beat 5 = 4.5 + 1 (stop) + 0.5.
        assert!(approx(t.seconds_at_beat(5.0), 6.0));
        let p = t.position_at(4.2);
        assert!(p.in_delay && !p.in_stop && approx(p.beat, 4.0));
        let p = t.position_at(5.0);
        assert!(p.in_stop && !p.in_delay && approx(p.beat, 4.0));
        assert!(approx(t.beat_at(6.0), 5.0));
    }

    #[test]
    fn overlapping_warps_extend_the_destination() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.warps.push(WarpSegment {
            tick: Tick::from_beats(2),
            length: Tick::from_beats(2),
        });
        t.warps.push(WarpSegment {
            tick: Tick::from_beats(3),
            length: Tick::from_beats(3),
        });
        t.tidy();
        assert!(approx(t.seconds_at_beat(6.0), 2.0));
        assert!(approx(t.seconds_at_beat(7.0), 3.0));
        assert!(approx(t.beat_at(2.0), 6.0));
        assert!(!t.judgeable(Tick::from_beats(5)));
    }

    #[test]
    fn same_tick_segments_keep_the_last_one() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.bpms.push(BpmSegment {
            tick: Tick::from_beats(4),
            bpm: 100.0,
        });
        t.bpms.push(BpmSegment {
            tick: Tick::from_beats(4),
            bpm: 120.0,
        });
        t.stops.push(StopSegment {
            tick: Tick::from_beats(2),
            seconds: 1.0,
        });
        t.stops.push(StopSegment {
            tick: Tick::from_beats(2),
            seconds: 0.25,
        });
        t.tidy();
        assert_eq!(t.bpms.len(), 2);
        assert_eq!(t.bpms[1].bpm, 120.0);
        assert_eq!(t.stops.len(), 1);
        assert_eq!(t.stops[0].seconds, 0.25);
    }

    #[test]
    fn displayed_beat_with_zero_and_negative_ratios() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.scrolls.push(ScrollSegment {
            tick: Tick::from_beats(2),
            ratio: 0.0,
        });
        t.scrolls.push(ScrollSegment {
            tick: Tick::from_beats(4),
            ratio: -1.0,
        });
        t.tidy();
        assert!(approx(t.displayed_beat(1.0), 1.0));
        assert!(approx(t.displayed_beat(3.0), 2.0)); // frozen
        assert!(approx(t.displayed_beat(4.0), 2.0));
        assert!(approx(t.displayed_beat(5.0), 1.0)); // scrolls backwards
    }

    #[test]
    fn speed_ratio_interpolates_in_time_and_skips_the_rows_delay() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.speeds.push(SpeedSegment {
            tick: Tick::from_beats(2),
            ratio: 3.0,
            delay: 2.0,
            unit: SpeedUnit::Seconds,
        });
        t.delays.push(StopSegment {
            tick: Tick::from_beats(2),
            seconds: 1.0,
        });
        t.tidy();
        // Row 2 is judged at 3.0 s (after its 1 s delay) but the ramp starts
        // when the row is reached, at 2.0 s, and ends at 4.0 s.
        assert!(approx(t.speed_ratio_at(1.0, 1.0), 1.0));
        assert!(approx(t.speed_ratio_at(3.0, 2.0), 2.0));
        assert!(approx(t.speed_ratio_at(3.5, 2.5), 2.5));
        assert!(approx(t.speed_ratio_at(9.0, 8.0), 3.0));

        // Beat-unit ramps are measured in time too: a stop inside stretches them.
        let mut t = TimingMap::constant(60.0, 0.0);
        t.speeds.push(SpeedSegment {
            tick: Tick::from_beats(2),
            ratio: 2.0,
            delay: 2.0,
            unit: SpeedUnit::Beats,
        });
        t.stops.push(StopSegment {
            tick: Tick::from_beats(3),
            seconds: 2.0,
        });
        t.tidy();
        // Ramp from beat 2 (2 s) to beat 4 (6 s): halfway at 4 s (inside the stop).
        let p = t.position_at(4.0);
        assert!(p.in_stop);
        assert!(approx(t.speed_ratio_at(4.0, p.beat), 1.5));
        assert!(approx(t.speed_ratio_at(6.0, 4.0), 2.0));
    }

    #[test]
    fn constant_bpm_roundtrip() {
        let t = TimingMap::constant(120.0, -0.5);
        // beat 0 at 0.5 s, 2 beats per second
        assert!(approx(t.seconds_at_beat(0.0), 0.5));
        assert!(approx(t.seconds_at_beat(4.0), 2.5));
        assert!(approx(t.beat_at(2.5), 4.0));
    }

    #[test]
    fn stop_applies_after_row_delay_before() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.stops.push(StopSegment {
            tick: Tick::from_beats(2),
            seconds: 1.0,
        });
        t.delays.push(StopSegment {
            tick: Tick::from_beats(4),
            seconds: 0.5,
        });
        t.tidy();
        assert!(approx(t.seconds_at_beat(2.0), 2.0)); // before the stop
        assert!(approx(t.seconds_at_beat(3.0), 4.0)); // 1 s stop included
        assert!(approx(t.seconds_at_beat(4.0), 5.5)); // delay included at its own row
        let p = t.position_at(2.5);
        assert!(p.in_stop && approx(p.beat, 2.0));
        let p = t.position_at(5.2);
        assert!(p.in_delay && approx(p.beat, 4.0));
        assert!(approx(t.beat_at(6.5), 5.0));
    }

    #[test]
    fn warp_skips_time() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.warps.push(WarpSegment {
            tick: Tick::from_beats(2),
            length: Tick::from_beats(2),
        });
        t.tidy();
        assert!(approx(t.seconds_at_beat(2.0), 2.0));
        assert!(approx(t.seconds_at_beat(3.0), 2.0));
        assert!(approx(t.seconds_at_beat(4.0), 2.0));
        assert!(approx(t.seconds_at_beat(5.0), 3.0));
        assert!(!t.judgeable(Tick::from_beats(3)));
        assert!(t.judgeable(Tick::from_beats(4)));
        assert!(approx(t.beat_at(2.5), 4.5));
    }

    #[test]
    fn bpm_change() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.bpms.push(BpmSegment {
            tick: Tick::from_beats(4),
            bpm: 120.0,
        });
        t.tidy();
        assert!(approx(t.seconds_at_beat(4.0), 4.0));
        assert!(approx(t.seconds_at_beat(8.0), 6.0));
        assert!(approx(t.beat_at(5.0), 6.0));
    }

    #[test]
    fn quantization_buckets() {
        assert_eq!(Tick(0).quantization(), Quantization::N4);
        assert_eq!(Tick(24).quantization(), Quantization::N8);
        assert_eq!(Tick(16).quantization(), Quantization::N12);
        assert_eq!(Tick(12).quantization(), Quantization::N16);
        assert_eq!(Tick(8).quantization(), Quantization::N24);
        assert_eq!(Tick(6).quantization(), Quantization::N32);
        assert_eq!(Tick(4).quantization(), Quantization::N48);
        assert_eq!(Tick(3).quantization(), Quantization::N64);
        assert_eq!(Tick(1).quantization(), Quantization::N192);
    }
    use crate::model::Quantization;
}
