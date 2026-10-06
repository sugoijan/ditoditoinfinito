//! Render snapshot: everything the renderer needs for one frame.

use ddi_chart::{Color, Quantization};

use crate::rules::{Judgement, ScoreView};

/// One lane's receptor.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ReceptorState {
    pub pressed: bool,
    /// Most recent hit in this lane and its age in seconds.
    pub flash: Option<(Judgement, f32)>,
}

/// The judgement text to show.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct JudgementFlash {
    pub judgement: Judgement,
    /// Press − note in seconds (Fast/Slow display); `0.0` for misses.
    pub delta_seconds: f64,
    /// Seconds since the judgement.
    pub age: f32,
    /// Combo right after the judgement.
    pub combo_at: u32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SpriteKind {
    Tap,
    /// `tail_y` is the tail's height; `active` while being held (head
    /// pinned to the receptor); `dropped` once let go or missed.
    HoldHead {
        tail_y: f32,
        active: bool,
        dropped: bool,
    },
    RollHead {
        tail_y: f32,
        active: bool,
        dropped: bool,
    },
    Mine,
    Lift,
    Fake,
    Shock,
    Dummy,
}

/// One drawable note.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct NoteSprite {
    pub lane: u8,
    /// Arrow heights above the receptor (negative = past it).
    pub y: f32,
    pub kind: SpriteKind,
    pub quantization: Quantization,
    /// Fractional position within its beat, `0..1` (progress colouring).
    pub beat_frac: f32,
    /// Explicit colour from the chart, if any.
    pub color: Option<Color>,
    pub alpha: f32,
}

/// Snapshot of the play at one render time.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub song_time: f64,
    pub beat: f64,
    pub lanes: u8,
    /// `len() == lanes`.
    pub receptors: Vec<ReceptorState>,
    /// Only notes with `y` in `[-2, visible_range]` (tails included for holds).
    pub notes: Vec<NoteSprite>,
    pub judgement: Option<JudgementFlash>,
    pub combo: u32,
    pub score: ScoreView,
    /// `0..=1`.
    pub life: f32,
    pub danger: bool,
    pub failed: bool,
    pub finished: bool,
    /// Arrow heights above the receptor the renderer should draw.
    pub visible_range: f32,
}
