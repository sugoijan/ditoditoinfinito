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
//! - [`bindings`]: saved bindings per device and layout.
//! - [`rules`]: judge windows, combo, score, gauge, grades, presets.
//! - [`judge`]: the per-note state machine.
//! - [`scroll`]: note positions and scroll actions.
//! - [`appearance`]: Hidden / Sudden / Stealth visibility.
//! - [`transform`]: Turn and Cut chart transforms.
//! - [`autoplay`]: the autoplayer's press script.
//! - [`frame`]: the render snapshot.
//! - [`player`]: the orchestrator.
//!
//! See `docs/PLAN.md` §3.3.

pub mod appearance;
pub mod autoplay;
pub mod bindings;
pub mod calibration;
pub mod clock;
pub mod frame;
pub mod input;
pub mod judge;
pub mod player;
pub mod rules;
pub mod scroll;
pub mod transform;

pub use ddi_chart;
pub use ddi_platform;

pub use appearance::Appearance;
pub use bindings::{ConnectedPad, ControlBindings, LaneTable, PadBindings, PadUse};
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
pub use transform::{TimingCut, TransformOptions, Turn};

/// Deserializes a field, falling back to its default when the stored value
/// is not understood (an option a newer build saved, say), so one stale
/// field never discards a whole settings object. Use with
/// `#[serde(deserialize_with = "ddi_engine::lenient")]` and `#[serde(default)]`.
pub fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de> + Default,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum OrAny<T> {
        Known(T),
        Unknown(serde::de::IgnoredAny),
    }
    Ok(
        match <OrAny<T> as serde::Deserialize>::deserialize(deserializer)? {
            OrAny::Known(v) => v,
            OrAny::Unknown(_) => T::default(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_option_values_fall_back_per_field() {
        let t: TransformOptions = serde_json::from_str(
            r#"{"turn":"Sideways","cut":"Quarters","no_jumps":true,"later":1}"#,
        )
        .unwrap();
        assert_eq!(
            t,
            TransformOptions {
                turn: Turn::Off,
                cut: TimingCut::Quarters,
                no_jumps: true,
                no_holds: false,
            }
        );
        // Older saved options without the new fields load with defaults.
        let p: PlayOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(p, PlayOptions::default());
        let round: PlayOptions =
            serde_json::from_str(&serde_json::to_string(&PlayOptions::default()).unwrap()).unwrap();
        assert_eq!(round, PlayOptions::default());
    }
}
