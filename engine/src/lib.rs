//! Deterministic gameplay core.
//!
//! Takes a chart, a [`Ruleset`] and [`PlayOptions`], consumes timestamped
//! [`InputEvent`]s and [`ClockSample`](ddi_platform::ClockSample)s, and
//! produces a [`Frame`] for the renderer plus [`JudgeEvent`]s. No I/O, no
//! randomness, no platform code; tested natively with replay fixtures.
//!
//! Modules:
//! - [`clock`]: host ↔ audio clock anchor and offsets.
//! - [`input`]: lane events and device bindings.
//! - [`rules`]: judge windows, combo, score, gauge, grades, presets.
//! - [`judge`]: the per-note state machine.
//! - [`scroll`]: note positions.
//! - [`frame`]: the render snapshot.
//! - [`player`]: the orchestrator.
//!
//! See `docs/PLAN.md` §3.3.

pub mod calibration;
pub mod clock;
pub mod frame;
pub mod input;
pub mod judge;
pub mod player;
pub mod rules;
pub mod scroll;

pub use ddi_chart;
pub use ddi_platform;

pub use clock::{ClockOptions, SongClock};
pub use frame::{Frame, JudgementFlash, NoteSprite, ReceptorState, SpriteKind};
pub use input::{Binding, BindingDevice, Bindings, InputEvent, LaneInput};
pub use judge::{Judge, JudgeEvent, JudgeEventKind, JudgedNote, NoteState};
pub use player::{PlayOptions, Player, Results};
pub use rules::{
    ComboRules, EmptyPress, FailPolicy, FullCombo, GaugeRules, GradeBasis, GradeTable, JudgeNames,
    JudgeTable, Judgement, Ruleset, ScoreCtx, ScoreRules, ScoreView, Tally, Window, presets,
};
pub use scroll::{ScrollAction, ScrollOptions, ScrollState, SpeedMod, note_y};
