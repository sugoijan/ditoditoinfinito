//! Generative backgrounds (`docs/plans/phase-8-gaps.md`, steps 4 and 5):
//! what stands in for a song's missing background, drawn from its music
//! alone (decision 6), never from the play. Four looks, as the design
//! study chose them (`tools/background-design/approved.json`), each in
//! one of the study's palettes:
//!
//! - [`Look::Bars`]: segmented spectrum bars along the bottom;
//! - [`Look::Kaleidoscope`]: a rosette of two mirrors over a turning,
//!   wandering piece of a picture (pieces of glass without one);
//! - [`Look::Field`]: a still, quiet field with lines waving with the
//!   bands;
//! - [`Look::Rain`]: the start menu's outline rain, some outlines
//!   zooming with the bass, others waving with the treble.
//!
//! What a frame follows is [`Signals`]: the song's beat, and what
//! [`Series`] made of its analysis ([`crate::music`]) once at load. Every
//! smoothing, peak and clock is made over the analysis' frames, so a
//! frame is a function of the song second alone: a replay, an autoplay
//! and both shells draw the same picture.

use ddi_chart::{Song, SourceFormat};
use serde::{Deserialize, Serialize};

use crate::music::{BANDS, Music};

/// Whether a generative background may stand in for `song`'s missing
/// background (decision 4): every song but a Dancing☆Onigiri work,
/// whose back layers are its background and which plays on the plain
/// ground without one (decision 5). A work the chart editor saved keeps
/// its source format, so it stays one.
pub fn stands_in(song: &Song) -> bool {
    !matches!(song.source.format, SourceFormat::Danoni)
}

/// A generative background's look.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Look {
    Bars,
    Kaleidoscope,
    #[default]
    Field,
    Rain,
}

impl Look {
    pub const ALL: [Look; 4] = [Look::Bars, Look::Kaleidoscope, Look::Field, Look::Rain];
}

/// The study's palettes, mostly dark and of few hues: its Vivid left out
/// (the maintainer, 2026-11-06), the screens using enough of it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Palette {
    #[default]
    Ink,
    Dusk,
    Moss,
    Ember,
    Mono,
}

impl Palette {
    pub const ALL: [Palette; 5] = [
        Palette::Ink,
        Palette::Dusk,
        Palette::Moss,
        Palette::Ember,
        Palette::Mono,
    ];

    /// Ground, low, middle and high, sRGB.
    pub fn colors(self) -> [[u8; 3]; 4] {
        match self {
            Palette::Ink => [
                [0x05, 0x02, 0x0d],
                [0x14, 0x1a, 0x2b],
                [0x4b, 0x58, 0x75],
                [0xaa, 0xb4, 0xc9],
            ],
            Palette::Dusk => [
                [0x07, 0x05, 0x0c],
                [0x1c, 0x15, 0x30],
                [0x5b, 0x4a, 0x7c],
                [0xcf, 0xa7, 0xc6],
            ],
            Palette::Moss => [
                [0x04, 0x08, 0x07],
                [0x11, 0x21, 0x1c],
                [0x3c, 0x68, 0x55],
                [0x9c, 0xc8, 0xb1],
            ],
            Palette::Ember => [
                [0x0a, 0x05, 0x05],
                [0x27, 0x13, 0x11],
                [0x7a, 0x3b, 0x2e],
                [0xd7, 0x90, 0x70],
            ],
            Palette::Mono => [
                [0x05, 0x05, 0x05],
                [0x17, 0x17, 0x17],
                [0x55, 0x55, 0x55],
                [0xc9, 0xc9, 0xc9],
            ],
        }
    }
}

// The study's choices (`approved.json`, exported 2026-11-05). Lengths
// the study gave in pixels are of a 540-pixel-high screen, as the
// menu's rain measures; the shells scale them to theirs.

/// Spectrum bars: across the bottom, segmented, a peak held on each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bars {
    /// Of the screen's height, at the loudest.
    pub height: f32,
    /// Of a bar's width, between bars.
    pub gap: f32,
    /// Segments at the full height, and the gap between them (of a
    /// segment's pitch).
    pub segments: f32,
    pub segment_gap: f32,
    /// Seconds a bar takes to rise and to fall.
    pub attack: f64,
    pub release: f64,
    /// Seconds a peak holds, then how fast it falls (heights a second).
    pub peak_hold: f64,
    pub peak_fall: f64,
    pub opacity: f32,
}

/// Bars across the screen, from the low bands on the left.
pub const BAR_COUNT: usize = 24;

pub const BARS: Bars = Bars {
    height: 0.42,
    gap: 0.35,
    segments: 16.0,
    segment_gap: 0.3,
    attack: 0.03,
    release: 0.28,
    peak_hold: 0.5,
    peak_fall: 0.6,
    opacity: 0.7,
};

/// The kaleidoscope: a rosette of two mirrors, mirrored outward in
/// rings, over a small piece of a picture that turns a step each beat
/// and wanders, faster with the treble. The mirrors never move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kaleidoscope {
    /// The rosette's radius, of the screen's height.
    pub radius: f32,
    /// Wedges round it (each mirrored: twice as many reflections).
    pub segments: f32,
    /// The picture's height a radius shows.
    pub piece: f32,
    /// Degrees the piece turns each beat, eased.
    pub step: f32,
    /// How fast the piece wanders (the study's `wander`, a quarter of a
    /// radian a second at 1), faster with the treble.
    pub wander: f64,
    pub desaturate: f32,
    pub darken: f32,
    pub tint: f32,
    /// Brightening on each beat.
    pub pulse: f32,
}

pub const KALEIDOSCOPE: Kaleidoscope = Kaleidoscope {
    radius: 1.0,
    segments: 8.0,
    piece: 0.22,
    step: 15.0,
    wander: 0.3,
    desaturate: 0.55,
    darken: 0.6,
    tint: 0.3,
    pulse: 0.12,
};

/// The field: a still, domain-warped field brighter with the loudness,
/// and lines across the screen waving with their share of the bands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field {
    pub scale: f32,
    pub contrast: f32,
    /// Its brightness, and the share of it that follows the loudness.
    pub bright: f32,
    pub bright_amount: f32,
    pub lines: usize,
    /// Pixels of 540 at the loudest.
    pub amplitude: f32,
    /// Waves across the screen.
    pub cycles: f32,
    pub line_speed: f32,
    /// Pixels of 540.
    pub line_width: f32,
    pub line_opacity: f32,
    /// Seconds the lines' bands take to rise and to fall.
    pub attack: f64,
    pub release: f64,
}

pub const FIELD: Field = Field {
    scale: 2.63,
    contrast: 0.49,
    bright: 0.35,
    bright_amount: 0.35,
    lines: 10,
    amplitude: 40.0,
    cycles: 3.8,
    line_speed: 0.15,
    line_width: 1.0,
    line_opacity: 0.67,
    attack: 0.04,
    release: 0.3,
};

/// The rain: the start menu's outlines falling, a share of them moving,
/// each chosen as it comes in at the top: half zoom with the bass, the
/// others wave round their outline with the treble. Both follow what
/// stands out of the song: a signal above its median over the song, its
/// 95th percentile full.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rain {
    pub count: usize,
    pub size: f32,
    pub speed: f64,
    /// The share of the outlines that move, and of those, the share that
    /// zoom.
    pub moving: f64,
    pub zooming: f64,
    /// Of an outline's size, at the loudest.
    pub zoom: f32,
    /// Units of 64 (an outline's side), at the loudest; waves round an
    /// outline; their speed, Hz.
    pub amplitude: f32,
    pub waves: f32,
    pub frequency: f32,
    pub opacity: f32,
    /// Seconds the zoom and the waves take to rise and to fall.
    pub zoom_attack: f64,
    pub zoom_release: f64,
    pub wave_attack: f64,
    pub wave_release: f64,
}

pub const RAIN: Rain = Rain {
    count: 50,
    size: 1.0,
    speed: 1.0,
    moving: 0.35,
    zooming: 0.5,
    zoom: 0.3,
    amplitude: 3.0,
    waves: 6.0,
    frequency: 17.3,
    opacity: 1.8,
    zoom_attack: 0.02,
    zoom_release: 0.18,
    wave_attack: 0.02,
    wave_release: 0.25,
};

/// The bands the study calls the bass and the treble.
const BASS: std::ops::Range<usize> = 0..6;
const TREBLE: std::ops::Range<usize> = 18..32;

/// What a frame of a generative background follows: the song second and
/// beat, and its music there. Levels are 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Signals {
    pub time: f32,
    pub beat: f32,
    pub loudness: f32,
    /// The bars' heights and their peaks.
    pub bars: [f32; BAR_COUNT],
    pub peaks: [f32; BAR_COUNT],
    /// The bands as the field's lines follow them.
    pub lines: [f32; BANDS],
    /// The rain's zoom and waves.
    pub zoom: f32,
    pub wave: f32,
    /// The kaleidoscope's wandering, radians run.
    pub wander: f32,
}

impl Signals {
    /// The song second and beat alone, the music silent: before the
    /// analysis is in, and held still for reduced motion.
    pub fn beat_only(time: f64, beat: f64) -> Signals {
        Signals {
            time: time as f32,
            beat: beat as f32,
            loudness: 0.0,
            bars: [0.0; BAR_COUNT],
            peaks: [0.0; BAR_COUNT],
            lines: [0.0; BANDS],
            zoom: 0.0,
            wave: 0.0,
            wander: 0.0,
        }
    }
}

/// The signals of a song's music, a frame of its analysis at a time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Series {
    /// The song second of frame 0, and seconds between frames.
    start: f64,
    per: f64,
    frames: usize,
    /// [`BAR_COUNT`] bytes a frame each.
    bars: Vec<u8>,
    peaks: Vec<u8>,
    /// [`BANDS`] bytes a frame.
    lines: Vec<u8>,
    loudness: Vec<u8>,
    zoom: Vec<u8>,
    wave: Vec<u8>,
    wander: Vec<f32>,
}

/// `v` following `target` over `dt` seconds: `attack` seconds rising,
/// `release` falling (the study's `follow`).
fn follow(v: f32, target: f32, dt: f64, attack: f64, release: f64) -> f32 {
    let k = if target > v { attack } else { release };
    let share = if k <= 0.0 { 1.0 } else { 1.0 - (-dt / k).exp() };
    v + (target - v) * share as f32
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The mean of `bands[range]`.
fn mean(bands: &[f32], range: std::ops::Range<usize>) -> f32 {
    let n = range.len().max(1) as f32;
    bands[range].iter().sum::<f32>() / n
}

/// The values at the median and the 95th percentile of `values`.
fn typical(values: &mut [f32]) -> (f32, f32) {
    if values.is_empty() {
        return (0.0, 1.0);
    }
    values.sort_by(f32::total_cmp);
    let at = |p: f64| values[(p * (values.len() - 1) as f64).floor() as usize];
    (at(0.5), at(0.95))
}

/// `v` above `low`, 1 at `high`.
fn lift(v: f32, (low, high): (f32, f32)) -> f32 {
    ((v - low) / (high - low).max(1e-3)).clamp(0.0, 1.0)
}

impl Series {
    /// Every look's signals over `music`.
    pub fn new(music: &Music) -> Series {
        let n = music.frames();
        let dt = music.per;
        let mut s = Series {
            start: music.start,
            per: music.per,
            frames: n,
            bars: Vec::with_capacity(n * BAR_COUNT),
            peaks: Vec::with_capacity(n * BAR_COUNT),
            lines: Vec::with_capacity(n * BANDS),
            loudness: music.loudness.iter().map(|v| byte(*v)).collect(),
            zoom: Vec::with_capacity(n),
            wave: Vec::with_capacity(n),
            wander: Vec::with_capacity(n),
        };
        let (mut bands, mut lines) = ([0.0f32; BANDS], [0.0f32; BANDS]);
        let (mut peak, mut hold) = ([0.0f32; BAR_COUNT], [0.0f64; BAR_COUNT]);
        let bass_of = |k: usize| mean(music.frame(k), BASS);
        let treble_of = |k: usize| mean(music.frame(k), TREBLE);
        let bass = typical(&mut (0..n).map(bass_of).collect::<Vec<_>>());
        let treble = typical(&mut (0..n).map(treble_of).collect::<Vec<_>>());
        let (mut zoom, mut wave, mut wander) = (0.0f32, 0.0f32, 0.0f64);
        for k in 0..n {
            let frame = music.frame(k);
            for b in 0..BANDS {
                bands[b] = follow(bands[b], frame[b], dt, BARS.attack, BARS.release);
                lines[b] = follow(lines[b], frame[b], dt, FIELD.attack, FIELD.release);
            }
            for i in 0..BAR_COUNT {
                // The bar's bands: its share of them, one at least.
                let a = i * BANDS / BAR_COUNT;
                let z = ((i + 1) * BANDS / BAR_COUNT).max(a + 1);
                let v = mean(&bands, a..z);
                if v >= peak[i] {
                    (peak[i], hold[i]) = (v, BARS.peak_hold);
                } else if hold[i] > 0.0 {
                    hold[i] -= dt;
                } else {
                    peak[i] = v.max(peak[i] - (dt * BARS.peak_fall) as f32);
                }
                s.bars.push(byte(v));
                s.peaks.push(byte(peak[i]));
            }
            s.lines.extend(lines.iter().map(|v| byte(*v)));
            zoom = follow(
                zoom,
                lift(bass_of(k), bass),
                dt,
                RAIN.zoom_attack,
                RAIN.zoom_release,
            );
            wave = follow(
                wave,
                lift(treble_of(k), treble),
                dt,
                RAIN.wave_attack,
                RAIN.wave_release,
            );
            s.zoom.push(byte(zoom));
            s.wave.push(byte(wave));
            // The study's clock: faster with the treble as it is.
            wander += dt * KALEIDOSCOPE.wander * 0.25 * (0.3 + 1.4 * f64::from(treble_of(k)));
            s.wander.push(wander as f32);
        }
        s
    }

    /// Frames of the analysis.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The signals at song second `t` and beat `beat`, each between the
    /// frames either side of it; silence outside the music.
    pub fn at(&self, t: f64, beat: f64) -> Signals {
        let mut out = Signals::beat_only(t, beat);
        if self.frames == 0 || self.per <= 0.0 {
            return out;
        }
        let x = (t - self.start) / self.per;
        let k = x.floor();
        let f = (x - k) as f32;
        let k = k as i64;
        let n = self.frames as i64;
        // Frame `k`'s byte `i` of `width`, 0 outside the music.
        let at = |data: &[u8], width: usize, k: i64, i: usize| -> f32 {
            if (0..n).contains(&k) {
                f32::from(data[k as usize * width + i]) / 255.0
            } else {
                0.0
            }
        };
        let mix = |data: &[u8], width: usize, i: usize| {
            let (a, b) = (at(data, width, k, i), at(data, width, k + 1, i));
            a + (b - a) * f
        };
        for i in 0..BAR_COUNT {
            out.bars[i] = mix(&self.bars, BAR_COUNT, i);
            out.peaks[i] = mix(&self.peaks, BAR_COUNT, i);
        }
        for (i, v) in out.lines.iter_mut().enumerate() {
            *v = mix(&self.lines, BANDS, i);
        }
        out.loudness = mix(&self.loudness, 1, 0);
        out.zoom = mix(&self.zoom, 1, 0);
        out.wave = mix(&self.wave, 1, 0);
        // The clock holds before the music and after it.
        let clock = |k: i64| self.wander[k.clamp(0, n - 1) as usize];
        out.wander = if k < 0 {
            0.0
        } else {
            clock(k) + (clock(k + 1) - clock(k)) * f
        };
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Music of `frames` frames, every band at `level(k)` in frame `k`.
    fn music(frames: usize, level: impl Fn(usize) -> f32) -> Music {
        Music {
            start: 0.01,
            per: 0.02,
            bands: (0..frames)
                .flat_map(|k| std::iter::repeat_n(level(k), BANDS))
                .collect(),
            loudness: (0..frames).map(&level).collect(),
        }
    }

    #[test]
    fn a_work_never_gets_one() {
        let text = "#TITLE:t;#BPMS:0=120;\n#NOTES:dance-single::Easy:1::\n1000\n;";
        let mut song = ddi_chart::formats::sm::parse_simfile(text, "sm").unwrap();
        assert!(stands_in(&song));
        song.source.format = SourceFormat::Danoni;
        assert!(!stands_in(&song));
        song.source.format = SourceFormat::Ffr;
        assert!(stands_in(&song));
    }

    #[test]
    fn the_bars_rise_fast_fall_slowly_and_their_peaks_hold() {
        // Loud for half a second, then quiet.
        let s = Series::new(&music(100, |k| if k < 25 { 1.0 } else { 0.0 }));
        let at = |t: f64| s.at(t, 0.0);
        // Risen within a few frames (30 ms attack)…
        assert!(at(0.15).bars[0] > 0.95, "{}", at(0.15).bars[0]);
        // …fallen slowly (280 ms release)…
        let after = at(0.5 + 0.28 + 0.01).bars[0];
        assert!((after - (-1.0f32).exp()).abs() < 0.06, "{after}");
        // …its peak held half a second, then falling 0.6 a second.
        assert!(at(0.95).peaks[0] > 0.95);
        let falling = at(1.5).peaks[0];
        assert!(falling < 0.95 && falling > 0.5, "{falling}");
        assert!(at(1.5).peaks[0] >= at(1.5).bars[0]);
    }

    #[test]
    fn the_rains_motions_follow_what_stands_out_of_the_song() {
        // Mostly at 0.6, every 100th frame at 1: the loud frames stand out.
        let s = Series::new(&music(500, |k| if k % 100 == 0 { 1.0 } else { 0.6 }));
        let quiet = s.at(0.01 + 90.0 * 0.02, 0.0);
        let loud = s.at(0.01 + 101.5 * 0.02, 0.0);
        assert!(quiet.zoom < 0.05 && quiet.wave < 0.05, "{quiet:?}");
        assert!(
            loud.zoom > 0.5 && loud.wave > 0.5,
            "{} {}",
            loud.zoom,
            loud.wave
        );
    }

    #[test]
    fn a_frame_is_between_its_neighbours_and_silent_outside_the_music() {
        let s = Series::new(&music(50, |k| k as f32 / 49.0));
        let mid = s.at(0.01 + 10.5 * 0.02, 3.0);
        assert!(
            (mid.loudness - 10.5 / 49.0).abs() < 0.01,
            "{}",
            mid.loudness
        );
        assert_eq!(mid.beat, 3.0);
        let before = s.at(-1.0, -2.0);
        assert_eq!(
            (before.loudness, before.wander, before.bars[0]),
            (0.0, 0.0, 0.0)
        );
        let after = s.at(5.0, 0.0);
        assert_eq!(after.loudness, 0.0);
        // The wander clock runs on, holding after the music.
        assert!(after.wander > 0.0 && after.wander == s.at(9.0, 0.0).wander);
        assert!(s.at(0.5, 0.0).wander < after.wander);
    }
}
