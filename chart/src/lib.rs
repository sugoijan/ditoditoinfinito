//! Chart model and importers.
//!
//! Holds the format-independent song/chart representation (notes on a
//! 48-ticks-per-beat grid, a [`TimingMap`] converting beats to seconds and
//! back, lane [`Layout`]s as data) and the importers for external formats
//! (StepMania `.sm`/`.ssc`, DWI, Dancing☆Onigiri, FFR). No I/O: importers
//! take text or bytes.
//!
//! See `docs/PLAN.md` §3.2.

pub mod formats;
pub mod layout;
pub mod model;
pub mod song_ext;
pub mod timing;

pub use layout::{BUILTIN_LAYOUTS, Glyph, Lane, Layout, LayoutFamily};
pub use model::{
    Chart, Color, Difficulty, DisplayBpm, EffectEvent, EffectTime, Note, NoteKind, Quantization,
    Song, SourceFormat, SourceInfo, Tick,
};
pub use timing::{
    BpmSegment, ComboSegment, FakeSegment, Label, ScrollSegment, SpeedSegment, SpeedUnit,
    StopSegment, TickCount, TimePosition, TimeSignature, TimingMap, WarpSegment,
};

/// Ticks per beat. StepMania's `ROWS_PER_BEAT`; exact for every quantization
/// from 4ths to 192nds and for every DWI bracket.
pub const TICKS_PER_BEAT: i64 = 48;
