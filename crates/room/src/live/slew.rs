//! A device's song clock kept in step with the room's during a stage
//! (`docs/plans/multiplayer-online-play.md`, "The clocks' drift"):
//! sans-IO, one per live play.
//!
//! A play's song clock is its audio clock, mapped from the room's once,
//! at its start (the stage's song zero heard at the room's instant), and
//! its judging stays on it: the player's presses, the notes' times and the
//! music are never re-timed. Clocks drift (a sound card's crystal against
//! the room's), so over a long song two devices' song times move apart by
//! their drift, and with them each other's streams, which a view then has
//! to wait for (the view delay rises by the drift).
//!
//! The stream's times and the views' are the room's song clock instead:
//! the device's song time plus a correction ([`SongSlew::correction_us`])
//! that follows the room's clock as its estimate is refreshed during the
//! stage (the song time the room's clock says now, less the device's):
//! smoothed (the estimate's own noise and a frame's jitter averaged out),
//! moved at a bounded rate (a viewer never sees a jump), and never making
//! the corrected time go backwards (the rate is far below the clock's
//! own). Display and streaming alone: nothing judged depends on it.
//!
//! Its numbers are [`SLEW_V0`] (PROVISIONAL), named and versioned.

/// How the correction follows the room's clock. PROVISIONAL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlewRules {
    pub id: &'static str,
    /// The fastest the correction moves: µs of correction a second of
    /// song (2,000: the corrected clock runs at most 0.2% fast or slow,
    /// four times the 500 millionths of a bad crystal).
    pub max_rate_ppm: i64,
    /// The disagreement is averaged over about this long (ms of song): an
    /// exponential smoothing's time constant.
    pub smoothing_ms: f64,
    /// A smoothed disagreement smaller than this is left alone (µs).
    pub deadband_us: i64,
    /// A disagreement is said (the device's log, its debug state) once it
    /// is more than the estimate's uncertainty and this (µs).
    pub said_over_us: i64,
}

/// `slew-v0` (PROVISIONAL, 2026-11-13): at most 2 ms a second, averaged
/// over 4 s, 2 ms left alone, said past the uncertainty and 20 ms.
pub const SLEW_V0: SlewRules = SlewRules {
    id: "slew-v0",
    max_rate_ppm: 2_000,
    smoothing_ms: 4_000.0,
    deadband_us: 2_000,
    said_over_us: 20_000,
};

/// One play's correction from its song clock to the room's.
#[derive(Clone, Debug)]
pub struct SongSlew {
    rules: SlewRules,
    correction_us: f64,
    /// The smoothed disagreement (room − device, µs).
    smoothed_us: Option<f64>,
    last_local_us: Option<i64>,
    last_out_us: Option<i64>,
    /// The raw disagreement less the correction, last step (µs).
    off_us: i64,
    uncertainty_us: i64,
}

impl SongSlew {
    pub fn new(rules: SlewRules) -> SongSlew {
        SongSlew {
            rules,
            correction_us: 0.0,
            smoothed_us: None,
            last_local_us: None,
            last_out_us: None,
            off_us: 0,
            uncertainty_us: 0,
        }
    }

    pub fn rules(&self) -> SlewRules {
        self.rules
    }

    /// One step at the device's song time `local_us` (its audio clock),
    /// `room_us` the song time the room's clock says now as estimated
    /// (none while it is not known: the correction holds),
    /// `uncertainty_us` the estimate's. The room's song time now as this
    /// device streams and draws it: never less than the last step's.
    pub fn step(&mut self, local_us: i64, room_us: Option<i64>, uncertainty_us: i64) -> i64 {
        let dt = self.last_local_us.map_or(0, |l| local_us - l);
        if dt >= 0 {
            self.last_local_us = Some(local_us);
        }
        if let Some(room) = room_us {
            let raw = (room - local_us) as f64;
            self.uncertainty_us = uncertainty_us.max(0);
            self.off_us = (raw - self.correction_us).round() as i64;
            let s = match self.smoothed_us {
                None => raw,
                Some(s) if dt > 0 => {
                    let a = 1.0 - (-(dt as f64) / (self.rules.smoothing_ms * 1_000.0)).exp();
                    s + a * (raw - s)
                }
                Some(s) => s,
            };
            self.smoothed_us = Some(s);
            let gap = s - self.correction_us;
            if dt > 0 && gap.abs() > self.rules.deadband_us as f64 {
                let most = dt as f64 * self.rules.max_rate_ppm as f64 / 1e6;
                self.correction_us += gap.clamp(-most, most);
            }
        }
        let out = self.to_room(local_us);
        let out = self.last_out_us.map_or(out, |l| out.max(l));
        self.last_out_us = Some(out);
        out
    }

    /// The correction now (µs): room song time less the device's.
    pub fn correction_us(&self) -> i64 {
        self.correction_us.round() as i64
    }

    /// A time of this device's song clock (a judged event's) on the
    /// room's, as streamed.
    pub fn to_room(&self, local_us: i64) -> i64 {
        local_us + self.correction_us()
    }

    /// How far the device's corrected clock and the room's (as estimated)
    /// disagreed at the last step (µs, room − corrected).
    pub fn off_us(&self) -> i64 {
        self.off_us
    }

    /// The disagreement is more than the estimate's uncertainty and the
    /// rules' threshold: said in the device's log and debug state (the
    /// room is told nothing more: the stream's times are its clock's).
    pub fn disagrees(&self) -> bool {
        self.off_us.abs() > self.uncertainty_us.max(self.rules.said_over_us)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device 500 millionths slow against the room, the estimate noisy
    /// by a few ms: the corrected clock follows the room's within the
    /// noise after the smoothing's time, moves at most at the rate, and
    /// never goes backwards.
    #[test]
    fn the_correction_follows_a_drifting_clock_smoothly() {
        let mut s = SongSlew::new(SLEW_V0);
        let mut last = i64::MIN;
        let mut worst_late = 0i64;
        for frame in 0..(300 * 60) {
            let local = frame * 16_667;
            let room = local + local / 2_000 + ((frame * 7_919) % 7 - 3) * 1_000;
            let out = s.step(local, Some(room), 5_000);
            assert!(out >= last, "never backwards");
            if frame > 0 {
                let step = out - last;
                assert!((step - 16_667).abs() <= 16_667 * 2_000 / 1_000_000 + 2);
            }
            last = out;
            if frame > 60 * 30 {
                worst_late = worst_late.max((local + local / 2_000 - out).abs());
            }
        }
        assert!(worst_late < 6_000, "{worst_late}");
        assert!(!s.disagrees());
    }

    /// A sudden disagreement (a clock re-estimated far off) is said, and
    /// followed at the rate alone.
    #[test]
    fn a_large_disagreement_is_said_and_slewed() {
        let mut s = SongSlew::new(SLEW_V0);
        s.step(0, Some(0), 5_000);
        let out = s.step(16_667, Some(16_667 + 200_000), 5_000);
        assert!(s.disagrees());
        assert!(out - 16_667 <= 34);
        let mut out = 0;
        for f in 2..(60 * 200) {
            out = s.step(f * 16_667, Some(f * 16_667 + 200_000), 5_000);
        }
        assert!((out - (60 * 200 - 1) * 16_667 - 200_000).abs() < 3_000);
        assert!(!s.disagrees());
        assert_eq!(s.step(0, None, 0), out, "never backwards");
    }
}
