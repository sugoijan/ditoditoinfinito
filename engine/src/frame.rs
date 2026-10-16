//! Render snapshot: everything the renderer needs for one frame.

use ddi_chart::{Color, Quantization};

use crate::appearance::Appearance;
use crate::rules::{Judgement, ScoreView};

/// One lane's receptor.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ReceptorState {
    pub pressed: bool,
    /// Most recent hit in this lane and its age in seconds.
    pub flash: Option<(Judgement, f32)>,
    /// A hold (or roll) of this lane is held with life left (StepMania's
    /// `HoldingOn`/`RollOn`, `GhostArrowRow::Update`).
    pub holding: Option<HoldKind>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HoldKind {
    Hold,
    Roll,
}

/// What a lane's receptor and explosion react to, in a noteskin
/// (`ReceptorArrow::Step`, `GhostArrowRow::DidTapNote`/`DidHoldNote`).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SkinEventKind {
    /// The lane's button went down (`Press`).
    Press,
    /// … and up (`Lift`).
    Lift,
    /// What a press did, for the receptor: the judgement of the note it
    /// hit, `None` when it hit nothing.
    Step(Option<Judgement>),
    /// A press set off a mine (the receptor's `HitMine`).
    StepMine,
    /// A tap judged; with per-row combo, every lane of a row once a press
    /// completes it, with the row's last judgement (`FlashGhostRow`).
    Tap { judgement: Judgement, bright: bool },
    /// A hold or roll kept to its end.
    Held { bright: bool },
    /// A mine went off (the explosion's `HitMine`).
    HitMine,
}

/// One event, numbered in order.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SkinEvent {
    pub seq: u64,
    pub lane: u8,
    pub kind: SkinEventKind,
    pub song_time: f64,
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

/// Where a play shows its holds' (and shocks') outcomes, O.K. and N.G.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum HoldPlace {
    /// Not shown: StepMania's pump game (`HoldJudgmentY…` −99999 there).
    /// Ticked holds elsewhere show none either (their checkpoints are
    /// their judgement), but the shocks of their charts still do.
    #[default]
    None,
    /// Over each lane, a little past its receptor towards the field's
    /// middle (StepMania's `HoldJudgment`, one a column).
    Lanes,
    /// Dancing☆Onigiri's freeze judgement: one at a time, beside the
    /// tap judgement (100 px right of it and 70 px below in its 50 px
    /// arrows, `mainWindow.js`), shown still for 60 frames.
    Centre,
}

/// A hold's or a shock's outcome being shown.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct HoldFlash {
    pub lane: u8,
    /// Kept (O.K.), else dropped (N.G.).
    pub held: bool,
    /// Seconds since it.
    pub age: f32,
}

/// The holds' outcomes to show, and where.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HoldJudgements {
    pub place: HoldPlace,
    /// One a lane at most ([`HoldPlace::Centre`]: the latest only).
    pub flashes: Vec<HoldFlash>,
}

/// How long a hold's outcome shows (seconds): StepMania's `_fallback`
/// `HoldJudgmentHeldCommand` (0.3 + 0.5 + 0.1) and
/// `HoldJudgmentLetGoCommand` (0.8 + 0.5 + 0.1).
pub const HELD_SECONDS: f32 = 0.9;
pub const LET_GO_SECONDS: f32 = 1.4;
/// Dancing☆Onigiri's freeze judgement shows for 60 frames
/// (`g_workObj.fjudgeTimer`, `mainWindow.js`), at its 60 frames a second.
pub const CENTRE_SECONDS: f32 = 1.0;

impl HoldPlace {
    /// How long an outcome shows here (seconds).
    pub fn seconds(self, held: bool) -> f32 {
        match self {
            HoldPlace::None => 0.0,
            HoldPlace::Centre => CENTRE_SECONDS,
            HoldPlace::Lanes if held => HELD_SECONDS,
            HoldPlace::Lanes => LET_GO_SECONDS,
        }
    }
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
    /// 0 or 1 under Hidden/Sudden/Stealth, else 1.
    pub alpha: f32,
    /// White glow where a note appears or vanishes, `0..=1.3`
    /// ([`Appearance::visibility`]).
    pub glow: f32,
    /// The note's beat (noteskin animation and colours).
    pub beat: f32,
    /// A hold's life, `0..=1`: 1 until it is let go, then what is left (a
    /// noteskin greys it); 1 for other notes.
    pub hold_life: f32,
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
    /// Holds' and shocks' outcomes (O.K., N.G.).
    pub holds: HoldJudgements,
    pub combo: u32,
    pub score: ScoreView,
    /// `0..=1`.
    pub life: f32,
    pub danger: bool,
    pub failed: bool,
    pub finished: bool,
    /// Arrow heights above the receptor the renderer should draw.
    pub visible_range: f32,
    /// For clipping hold bodies ([`Appearance::visible_spans`]); note alphas
    /// already account for it.
    pub appearance: Appearance,
}
