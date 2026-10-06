//! Note scroll positions.
//!
//! Positions are in **arrow heights above the receptor** (negative = past
//! it). At X-mod 1 one beat is one arrow height, matching StepMania.

use ddi_chart::{Tick, TimingMap};
use serde::{Deserialize, Serialize};

/// Scroll speed.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SpeedMod {
    /// Beat-based multiplier (`x2`): arrows move with the BPM.
    XMod(f64),
    /// Constant speed: arrows move as if the song were at this BPM with an
    /// X-mod of 1, ignoring BPM changes and stops.
    CMod(f64),
}

/// DDR "SCROLL ACTION" / DWI boost mods.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrollAction {
    Normal,
    /// TODO: accelerate near the receptor; currently behaves as `Normal`.
    Boost,
    /// TODO: decelerate near the receptor; currently behaves as `Normal`.
    Brake,
    /// TODO: alternate speeds at fixed points; currently behaves as `Normal`.
    Wave,
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScrollOptions {
    pub speed: SpeedMod,
    /// Receptor at the top, arrows scroll downwards. The renderer applies
    /// it; positions here are always "above the receptor".
    pub reverse: bool,
    pub scroll_action: ScrollAction,
}

impl Default for ScrollOptions {
    fn default() -> ScrollOptions {
        ScrollOptions {
            speed: SpeedMod::XMod(1.0),
            reverse: false,
            scroll_action: ScrollAction::Normal,
        }
    }
}

/// Per-frame scroll state, so each note's position is a subtraction.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScrollState {
    options: ScrollOptions,
    song_time: f64,
    /// Displayed beat (after `#SCROLLS`) at `song_time`.
    displayed_beat: f64,
    /// `#SPEEDS` ratio at `song_time`.
    speed_ratio: f64,
}

impl ScrollState {
    pub fn at(options: ScrollOptions, timing: &TimingMap, song_time: f64) -> ScrollState {
        let beat = timing.beat_at(song_time);
        ScrollState {
            options,
            song_time,
            displayed_beat: timing.displayed_beat(beat),
            speed_ratio: timing.speed_ratio_at(song_time, beat),
        }
    }

    pub fn song_time(&self) -> f64 {
        self.song_time
    }

    /// Height of a note above the receptor. `note_seconds` is the note's
    /// judged second (`timing.seconds_at(note_tick)`), precomputed by the
    /// caller.
    pub fn y(&self, timing: &TimingMap, note_tick: Tick, note_seconds: f64) -> f32 {
        let y = match self.options.speed {
            SpeedMod::XMod(mult) => {
                (timing.displayed_beat(note_tick.beat()) - self.displayed_beat)
                    * mult
                    * self.speed_ratio
            }
            SpeedMod::CMod(bpm) => (note_seconds - self.song_time) * bpm / 60.0,
        };
        match self.options.scroll_action {
            // TODO: Boost/Brake/Wave curves.
            ScrollAction::Normal
            | ScrollAction::Boost
            | ScrollAction::Brake
            | ScrollAction::Wave => y as f32,
        }
    }
}

/// Height of `note_tick` above the receptor at `song_time`. Convenience
/// for one-off queries; frames use [`ScrollState`].
pub fn note_y(options: ScrollOptions, timing: &TimingMap, note_tick: Tick, song_time: f64) -> f32 {
    ScrollState::at(options, timing, song_time).y(timing, note_tick, timing.seconds_at(note_tick))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::{BpmSegment, ScrollSegment, SpeedSegment, SpeedUnit, StopSegment};

    #[test]
    fn xmod_distance_in_beats() {
        let t = TimingMap::constant(120.0, 0.0);
        let opts = ScrollOptions {
            speed: SpeedMod::XMod(2.0),
            ..Default::default()
        };
        // Note at beat 4 (2.0 s), now at 1.0 s (beat 2): 2 beats × 2 = 4.
        assert!((note_y(opts, &t, Tick::from_beats(4), 1.0) - 4.0).abs() < 1e-6);
        assert!((note_y(opts, &t, Tick::from_beats(4), 2.5) + 2.0).abs() < 1e-6);
    }

    #[test]
    fn cmod_ignores_bpm() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.bpms.push(BpmSegment {
            tick: Tick::from_beats(2),
            bpm: 240.0,
        });
        t.tidy();
        let opts = ScrollOptions {
            speed: SpeedMod::CMod(120.0),
            ..Default::default()
        };
        // Beat 4 is at 2.0 + 2/4 = 2.5 s; at 0.5 s that is 2 s away = 4 beats at 120.
        assert!((note_y(opts, &t, Tick::from_beats(4), 0.5) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn zero_at_judged_time_with_bpm_change_and_stop() {
        let mut t = TimingMap::constant(100.0, -0.25);
        t.bpms.push(BpmSegment {
            tick: Tick::from_beats(3),
            bpm: 180.0,
        });
        t.stops.push(StopSegment {
            tick: Tick::from_beats(5),
            seconds: 0.7,
        });
        t.tidy();
        let opts = ScrollOptions::default();
        for beat in 0..9 {
            let tick = Tick::from_beats(beat);
            let y = note_y(opts, &t, tick, t.seconds_at(tick));
            assert!(y.abs() < 1e-5, "beat {beat}: y = {y}");
        }
        // During the stop the row-5 note sits on the receptor.
        let y = note_y(
            opts,
            &t,
            Tick::from_beats(5),
            t.seconds_at(Tick::from_beats(5)) + 0.3,
        );
        assert!(y.abs() < 1e-5);
    }

    #[test]
    fn scrolls_and_speeds_apply() {
        let mut t = TimingMap::constant(60.0, 0.0);
        t.scrolls.push(ScrollSegment {
            tick: Tick::from_beats(2),
            ratio: 0.5,
        });
        t.speeds.push(SpeedSegment {
            tick: Tick::ZERO,
            ratio: 2.0,
            delay: 0.0,
            unit: SpeedUnit::Beats,
        });
        t.tidy();
        // Beat 4 displayed at 2 + 2×0.5 = 3; now beat 1 → 2 displayed beats × speed 2.
        assert!(
            (note_y(ScrollOptions::default(), &t, Tick::from_beats(4), 1.0) - 4.0).abs() < 1e-6
        );
    }
}
