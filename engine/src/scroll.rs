//! Note scroll positions.
//!
//! Positions are in **arrow heights above the receptor** (negative = past
//! it). At X-mod 1 one beat is one arrow height, matching StepMania.

use ddi_chart::{DisplayBpm, Song, SourceFormat, Tick, TimingMap};
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

/// DDR "SCROLL ACTION" / StepMania accel mods. They bend how a note's
/// distance from the receptor is drawn, never when it arrives: every
/// adjustment is zero at the receptor and absent past it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScrollAction {
    #[default]
    Normal,
    /// Notes accelerate as they approach the receptor.
    Boost,
    /// Notes decelerate as they approach the receptor.
    Brake,
    /// Notes speed up and slow down in a wave.
    Wave,
}

/// StepMania's note-field height (`SCREEN_HEIGHT`, 480 px) in arrow
/// spacings (64 px): the distance over which Boost and Brake act.
const EFFECT_HEIGHT: f64 = 480.0 / 64.0;
/// `BoostModMinClamp`/`BoostModMaxClamp` and the Brake equivalents (±400 px).
const ACCEL_CLAMP: f64 = 400.0 / 64.0;
/// `WaveModMagnitude` (20 px).
const WAVE_MAGNITUDE: f64 = 20.0 / 64.0;
/// `WaveModHeight` (38 px).
const WAVE_HEIGHT: f64 = 38.0 / 64.0;

impl ScrollAction {
    /// StepMania `ArrowEffects::GetYOffset` (`src/ArrowEffects.cpp` at
    /// `825467b`, metrics from `Themes/_fallback/metrics.ini`): `d` is the
    /// distance in beats before the speed multiplier, and the result is
    /// what gets multiplied. Notes past the receptor (`d < 0`) are left
    /// alone ("don't mess with the arrows after they've crossed 0").
    pub fn adjust(self, d: f64) -> f64 {
        if d < 0.0 {
            return d;
        }
        let h = EFFECT_HEIGHT;
        let adjust = match self {
            ScrollAction::Normal => 0.0,
            ScrollAction::Boost => {
                let boosted = d * 1.5 / ((d + h / 1.2) / h);
                (boosted - d).clamp(-ACCEL_CLAMP, ACCEL_CLAMP)
            }
            ScrollAction::Brake => {
                let braked = d * (d / h);
                (braked - d).clamp(-ACCEL_CLAMP, ACCEL_CLAMP)
            }
            ScrollAction::Wave => WAVE_MAGNITUDE * (d / WAVE_HEIGHT).sin(),
        };
        d + adjust
    }
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

/// Reference tempo of a frame-based work that declares none.
pub const FRAME_CHART_REFERENCE_BPM: f64 = 150.0;

/// Highest declared tempo taken as a reference.
const MAX_REFERENCE_BPM: f64 = 1000.0;

/// The speed a song is played at (decision 23). Frame-based works
/// (Dancing☆Onigiri) run on a synthetic tempo, so an x-mod there means a
/// constant speed at the work's declared BPM (the first value of a range;
/// [`FRAME_CHART_REFERENCE_BPM`] without one): it plays like the same x-mod
/// on a StepMania song at that tempo. It stays an x-mod against the
/// synthetic tempo, which is the same constant speed and keeps the work's
/// own speed changes (scroll segments, which a c-mod ignores); a c-mod
/// becomes the same.
pub fn speed_for_song(song: &Song, speed: SpeedMod) -> SpeedMod {
    if song.source.format != SourceFormat::Danoni {
        return speed;
    }
    let synthetic = ddi_chart::formats::danoni::SYNTHETIC_BPM;
    let reference = match song.display_bpm {
        // Bounded, so a nonsense declaration cannot make an infinite speed.
        DisplayBpm::Single(b) | DisplayBpm::Range(b, _) if b.is_finite() && b > 0.0 => {
            b.clamp(1.0, MAX_REFERENCE_BPM)
        }
        _ => FRAME_CHART_REFERENCE_BPM,
    };
    match speed {
        SpeedMod::XMod(x) => SpeedMod::XMod(x * reference / synthetic),
        SpeedMod::CMod(bpm) => SpeedMod::XMod(bpm / synthetic),
    }
}

/// Arrow heights a second a note moves per unit of danoniplus speed: it
/// moves `2 × speed` px a frame (`setSpeedOnFrame`, `dataLoader.js`
/// 1386–1398, at the default `baseSpeed` 1) on 50 px arrows
/// (`C_ARW_WIDTH`), 60 frames a second. Its default field travels 430 px
/// (8.6 arrows) to the step zone, about the 8.5 arrow heights of ours, so
/// the same rate also gives the same reading time.
pub const DANONI_SPEED_ARROWS_PER_SECOND: f64 = 2.0 * 60.0 / 50.0;

/// Lowest and highest chart speed taken as is.
const DANONI_SPEED_RANGE: (f64, f64) = (0.25, 20.0);

/// Where the scroll speed comes from.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeedSource {
    /// The player's speed, meaning the same on every song ([`speed_for_song`]).
    #[default]
    Player,
    /// A chart's own suggested speed when it has one (a Dancing☆Onigiri
    /// chart's `difData` speed), else the player's.
    Chart,
}

/// The speed `song.charts[chart]` is played at: the player's `speed` as
/// [`speed_for_song`] reads it, or with [`SpeedSource::Chart`] the chart's
/// own Dancing☆Onigiri speed, converted by
/// [`DANONI_SPEED_ARROWS_PER_SECOND`] (the chart's speed changes still
/// apply on top).
pub fn speed_for_chart(
    song: &Song,
    chart: usize,
    speed: SpeedMod,
    source: SpeedSource,
) -> SpeedMod {
    let own = song
        .charts
        .get(chart)
        .and_then(|c| c.danoni.as_ref())
        .map(|d| d.init_speed)
        .filter(|s| s.is_finite() && *s > 0.0);
    match (source, own) {
        (SpeedSource::Chart, Some(s)) if song.source.format == SourceFormat::Danoni => {
            let s = s.clamp(DANONI_SPEED_RANGE.0, DANONI_SPEED_RANGE.1);
            let beats_per_second = ddi_chart::formats::danoni::SYNTHETIC_BPM / 60.0;
            SpeedMod::XMod(s * DANONI_SPEED_ARROWS_PER_SECOND / beats_per_second)
        }
        _ => speed_for_song(song, speed),
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
        self.y_displayed(timing.displayed_beat(note_tick.beat()), note_seconds)
    }

    /// [`ScrollState::y`] with the note's displayed beat
    /// (`timing.displayed_beat`) already known: it never changes during a
    /// play, so the player computes it once per note rather than once per
    /// note and frame (a chart may have hundreds of scroll segments).
    pub fn y_displayed(&self, note_displayed_beat: f64, note_seconds: f64) -> f32 {
        // StepMania's order: distance (with `#SPEEDS` for beat spacing),
        // then the scroll action, then the speed multiplier.
        match self.options.speed {
            SpeedMod::XMod(mult) => {
                let d = (note_displayed_beat - self.displayed_beat) * self.speed_ratio;
                (self.options.scroll_action.adjust(d) * mult) as f32
            }
            SpeedMod::CMod(bpm) => {
                let d = (note_seconds - self.song_time) * bpm / 60.0;
                self.options.scroll_action.adjust(d) as f32
            }
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

    const ACTIONS: [ScrollAction; 4] = [
        ScrollAction::Normal,
        ScrollAction::Boost,
        ScrollAction::Brake,
        ScrollAction::Wave,
    ];

    #[test]
    fn actions_match_stepmania_formulas() {
        // Values from GetYOffset in pixels, divided by the 64 px spacing.
        let px = |v: f64| v / 64.0;
        // Boost: 64 px → 64 × 1.5 / ((64 + 400) / 480) = 99.31 px.
        assert!((ScrollAction::Boost.adjust(1.0) - px(64.0 * 1.5 / (464.0 / 480.0))).abs() < 1e-9);
        // Brake: 192 px → 192 × 192 / 480 = 76.8 px.
        assert!((ScrollAction::Brake.adjust(3.0) - px(76.8)).abs() < 1e-9);
        // Wave: 64 px → 64 + 20 sin(64 / 38).
        assert!(
            (ScrollAction::Wave.adjust(1.0) - px(64.0 + 20.0 * (64.0f64 / 38.0).sin())).abs()
                < 1e-9
        );
        // The ±400 px clamp.
        assert!((ScrollAction::Boost.adjust(100.0) - (100.0 - 6.25)).abs() < 1e-9);
        assert!((ScrollAction::Brake.adjust(100.0) - (100.0 + 6.25)).abs() < 1e-9);
        // Past the receptor nothing changes.
        for a in ACTIONS {
            assert_eq!(a.adjust(-1.5), -1.5);
            assert_eq!(a.adjust(0.0), 0.0);
        }
    }

    #[test]
    fn actions_never_reorder_notes() {
        for a in ACTIONS {
            let mut prev = a.adjust(0.0);
            let mut d = 0.001;
            while d < 60.0 {
                let y = a.adjust(d);
                assert!(y > prev, "{a:?} at {d}: {y} <= {prev}");
                prev = y;
                d += 0.001;
            }
        }
    }

    #[test]
    fn every_action_lands_on_the_receptor_at_the_judged_time() {
        let mut t = TimingMap::constant(100.0, -0.25);
        t.bpms.push(BpmSegment {
            tick: Tick::from_beats(3),
            bpm: 180.0,
        });
        t.stops.push(StopSegment {
            tick: Tick::from_beats(5),
            seconds: 0.7,
        });
        t.scrolls.push(ScrollSegment {
            tick: Tick::from_beats(6),
            ratio: 0.5,
        });
        t.tidy();
        for scroll_action in ACTIONS {
            for speed in [SpeedMod::XMod(2.5), SpeedMod::CMod(300.0)] {
                let opts = ScrollOptions {
                    speed,
                    reverse: false,
                    scroll_action,
                };
                for beat in 0..9 {
                    let tick = Tick::from_beats(beat);
                    let y = note_y(opts, &t, tick, t.seconds_at(tick));
                    assert!(
                        y.abs() < 1e-5,
                        "{scroll_action:?} {speed:?} beat {beat}: {y}"
                    );
                    // Approaching notes are above the receptor, passed ones below.
                    let before = note_y(opts, &t, tick, t.seconds_at(tick) - 0.05);
                    let after = note_y(opts, &t, tick, t.seconds_at(tick) + 0.05);
                    // (A note on a stop stays on the receptor through it.)
                    assert!(
                        before > 0.0 && after <= 0.0,
                        "{scroll_action:?} {speed:?} {beat}"
                    );
                }
            }
        }
    }

    #[test]
    fn boost_and_brake_apply_before_the_speed_multiplier() {
        let t = TimingMap::constant(120.0, 0.0);
        let at = |scroll_action, mult| {
            let opts = ScrollOptions {
                speed: SpeedMod::XMod(mult),
                reverse: false,
                scroll_action,
            };
            note_y(opts, &t, Tick::from_beats(3), 0.0)
        };
        let boost = ScrollAction::Boost.adjust(3.0) as f32;
        assert!((at(ScrollAction::Boost, 1.0) - boost).abs() < 1e-5);
        assert!((at(ScrollAction::Boost, 2.0) - 2.0 * boost).abs() < 1e-5);
    }
}
