//! Song, chart and note model.

use serde::{Deserialize, Serialize};

use crate::TICKS_PER_BEAT;
use crate::timing::TimingMap;

/// A position on the beat grid, in 1/48ths of a beat.
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Tick(pub i64);

impl Tick {
    pub const ZERO: Tick = Tick(0);

    pub const fn from_beats(beats: i64) -> Tick {
        Tick(beats * TICKS_PER_BEAT)
    }

    /// Nearest tick to a fractional beat (StepMania's `BeatToNoteRow`).
    pub fn from_beat_f64(beat: f64) -> Tick {
        Tick((beat * TICKS_PER_BEAT as f64).round() as i64)
    }

    /// Row `index` of `rows` in a 4-beat measure number `measure` (SM note data layout).
    /// Exact when `rows` divides 192; otherwise rounds like StepMania.
    pub fn from_measure_row(measure: i64, index: i64, rows: i64) -> Tick {
        let measure_ticks = 4 * TICKS_PER_BEAT;
        if measure_ticks % rows == 0 {
            Tick(measure * measure_ticks + index * (measure_ticks / rows))
        } else {
            let beat = measure as f64 * 4.0 + 4.0 * index as f64 / rows as f64;
            Tick::from_beat_f64(beat)
        }
    }

    pub fn beat(self) -> f64 {
        self.0 as f64 / TICKS_PER_BEAT as f64
    }

    /// Quantization bucket of this tick within its beat.
    pub fn quantization(self) -> Quantization {
        let r = self.0.rem_euclid(TICKS_PER_BEAT);
        if r % 48 == 0 {
            Quantization::N4
        } else if r % 24 == 0 {
            Quantization::N8
        } else if r % 16 == 0 {
            Quantization::N12
        } else if r % 12 == 0 {
            Quantization::N16
        } else if r % 8 == 0 {
            Quantization::N24
        } else if r % 6 == 0 {
            Quantization::N32
        } else if r % 4 == 0 {
            Quantization::N48
        } else if r % 3 == 0 {
            Quantization::N64
        } else {
            Quantization::N192
        }
    }
}

impl core::ops::Add for Tick {
    type Output = Tick;
    fn add(self, rhs: Tick) -> Tick {
        Tick(self.0 + rhs.0)
    }
}

impl core::ops::Sub for Tick {
    type Output = Tick;
    fn sub(self, rhs: Tick) -> Tick {
        Tick(self.0 - rhs.0)
    }
}

/// Note subdivision, as StepMania's `NoteType`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Quantization {
    N4,
    N8,
    N12,
    N16,
    N24,
    N32,
    N48,
    N64,
    N192,
}

/// RGBA colour in 0..=1.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoteKind {
    Tap,
    /// Hold ("freeze arrow"): must be held until `end`.
    HoldHead {
        end: Tick,
    },
    /// Roll: must be re-tapped until `end`.
    RollHead {
        end: Tick,
    },
    /// Must not be pressed while it passes.
    Mine,
    /// Judged on release.
    Lift,
    /// Drawn, never judged.
    Fake,
    /// DDR shock arrow: whole-row mine.
    Shock,
    /// Plays its keysound, never judged.
    AutoKeysound,
    /// Visual-only note (Dancing☆Onigiri dummy charts).
    Dummy,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub tick: Tick,
    pub lane: u8,
    pub kind: NoteKind,
    /// Index into `Song::keysounds`.
    pub keysound: Option<u16>,
    /// Explicit colour (FFR per-note colours, DanOni group colours).
    pub color: Option<Color>,
    /// Per-note scroll multiplier (DanOni `boost_data`).
    pub speed_mul: Option<f32>,
}

impl Note {
    pub fn new(tick: Tick, lane: u8, kind: NoteKind) -> Note {
        Note {
            tick,
            lane,
            kind,
            keysound: None,
            color: None,
            speed_mul: None,
        }
    }

    /// Tick at which the note stops mattering (tail for holds/rolls, else `tick`).
    pub fn end_tick(&self) -> Tick {
        match self.kind {
            NoteKind::HoldHead { end } | NoteKind::RollHead { end } => end,
            _ => self.tick,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    Beginner,
    Easy,
    Medium,
    Hard,
    Challenge,
    Edit,
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DisplayBpm {
    /// Use the chart's actual BPM range.
    Actual,
    Single(f64),
    Range(f64, f64),
    Random,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    /// `Layout::id`, e.g. `dance-single`.
    pub layout: String,
    pub difficulty: Difficulty,
    pub meter: u32,
    /// SSC `#CHARTNAME` (falls back to description).
    pub name: String,
    /// SM description / author field.
    pub description: String,
    pub credit: String,
    /// Sorted by `(tick, lane)`.
    pub notes: Vec<Note>,
    /// SSC split timing; `None` means use `Song::timing`.
    pub timing: Option<TimingMap>,
    pub display_bpm: Option<DisplayBpm>,
}

impl Chart {
    pub fn timing<'a>(&'a self, song: &'a Song) -> &'a TimingMap {
        self.timing.as_ref().unwrap_or(&song.timing)
    }

    /// Number of judged "steps" the way DDR/SM count them: taps, hold and roll
    /// heads and lifts (jumps count per note here; rulesets decide per-row).
    pub fn judged_note_count(&self) -> usize {
        self.notes
            .iter()
            .filter(|n| {
                matches!(
                    n.kind,
                    NoteKind::Tap
                        | NoteKind::HoldHead { .. }
                        | NoteKind::RollHead { .. }
                        | NoteKind::Lift
                )
            })
            .count()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EffectTime {
    Beat(Tick),
    Seconds(f64),
}

/// Background/foreground changes, attacks, lyrics, etc., preserved as
/// structured but otherwise opaque records.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectEvent {
    pub at: EffectTime,
    /// e.g. `bgchange`, `fgchange`, `attack`, `danoni:word`, `danoni:back`.
    pub kind: String,
    pub layer: u8,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SourceFormat {
    Sm,
    Ssc { version: f32 },
    Dwi,
    Danoni,
    Ffr,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    pub format: SourceFormat,
    /// Tags the importer did not understand, in order: `(name, raw value)`.
    pub unknown_tags: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Song {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub title_translit: String,
    pub subtitle_translit: String,
    pub artist_translit: String,
    pub genre: String,
    pub credit: String,
    /// Relative path of the audio file.
    pub music: Option<String>,
    pub preview_start: f64,
    pub preview_length: f64,
    pub banner: Option<String>,
    pub background: Option<String>,
    pub jacket: Option<String>,
    pub cd_title: Option<String>,
    pub timing: TimingMap,
    pub display_bpm: DisplayBpm,
    pub charts: Vec<Chart>,
    pub effects: Vec<EffectEvent>,
    pub keysounds: Vec<String>,
    /// Layouts the file defines itself (Dancing☆Onigiri custom keys);
    /// charts find theirs with [`Song::layout_of`].
    #[serde(default)]
    pub layouts: Vec<crate::Layout>,
    pub source: SourceInfo,
}
