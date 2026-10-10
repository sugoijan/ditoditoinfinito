//! The `[NoteDisplay]` metrics the engine reads, per button
//! (`NoteMetricCache_t::Load`, `src/NoteDisplay.cpp:107-137`, 5_1-new).

use crate::ini::{string_to_float, string_to_int};
use crate::skin::Skin;

/// A note part with its own animation and colour metrics (`NotePartNames`,
/// `src/NoteDisplay.cpp:36-46`). Rolls use the hold parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    TapNote,
    TapMine,
    TapLift,
    TapFake,
    HoldHead,
    HoldTail,
    HoldTopCap,
    HoldBody,
    HoldBottomCap,
}

impl Part {
    pub const ALL: [Part; 9] = [
        Part::TapNote,
        Part::TapMine,
        Part::TapLift,
        Part::TapFake,
        Part::HoldHead,
        Part::HoldTail,
        Part::HoldTopCap,
        Part::HoldBody,
        Part::HoldBottomCap,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Part::TapNote => "TapNote",
            Part::TapMine => "TapMine",
            Part::TapLift => "TapLift",
            Part::TapFake => "TapFake",
            Part::HoldHead => "HoldHead",
            Part::HoldTail => "HoldTail",
            Part::HoldTopCap => "HoldTopCap",
            Part::HoldBody => "HoldBody",
            Part::HoldBottomCap => "HoldBottomCap",
        }
    }
}

/// How a part's colour is chosen (`NoteColorType`, `src/NoteDisplay.h:38-45`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteColorType {
    /// By the note's beat division (`BeatToNoteType`).
    Denominator,
    /// By the note's beat position, cycling.
    Progress,
    /// The same, with notes on a whole step moved to the end of the cycle.
    ProgressAlternate,
}

impl NoteColorType {
    /// `StringToNoteColorType`: the whole value, ignoring case; anything else
    /// is invalid (StepMania then stops with an error when it colours the
    /// part).
    fn parse(s: &str) -> Option<NoteColorType> {
        [
            ("Denominator", NoteColorType::Denominator),
            ("Progress", NoteColorType::Progress),
            ("ProgressAlternate", NoteColorType::ProgressAlternate),
        ]
        .into_iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(s))
        .map(|(_, t)| t)
    }
}

/// One part's metrics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartMetrics {
    /// Beats (or seconds) for all of a sprite's frames.
    pub animation_length: f32,
    pub animation_is_vivid: bool,
    pub addition_offset: [f32; 2],
    /// Texture translation per colour, in padded-texture units.
    pub color_spacing: [f32; 2],
    /// As read; StepMania divides by it when colouring, so 0 or less
    /// colours nothing.
    pub color_count: i32,
    /// `None` for a name StepMania does not know.
    pub color_type: Option<NoteColorType>,
}

/// The `[NoteDisplay]` metrics of one button.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteDisplay {
    pub parts: [PartMetrics; 9],
    pub animation_is_beat_based: bool,
    pub draw_hold_head_for_taps_on_same_row: bool,
    pub draw_roll_head_for_taps_on_same_row: bool,
    pub tap_hold_roll_on_row_means_hold: bool,
    /// Pixels, integers as StepMania reads them.
    pub start_drawing_hold_body_offset_from_head: i32,
    pub stop_drawing_hold_body_offset_from_tail: i32,
    pub hold_let_go_gray_percent: f32,
    pub hold_head_is_above_wavy_parts: bool,
    pub hold_tail_is_above_wavy_parts: bool,
    pub flip_head_and_tail_when_reverse: bool,
    pub flip_hold_body_when_reverse: bool,
    pub top_hold_anchor_when_reverse: bool,
    pub hold_active_is_add_layer: bool,
}

impl NoteDisplay {
    pub fn part(&self, part: Part) -> &PartMetrics {
        &self.parts[part as usize]
    }

    /// Reads `button`'s metrics from `skin` (`[<Button>]` first). A metric
    /// no folder has reads as an empty string, so 0, as in StepMania.
    pub fn read(skin: &Skin<'_>, button: &str) -> NoteDisplay {
        let s = |name: &str| skin.metric(button, name).unwrap_or("");
        let f = |name: &str| string_to_float(s(name));
        let i = |name: &str| string_to_int(s(name));
        let b = |name: &str| i(name) != 0;
        let part = |p: Part| {
            let n = p.name();
            PartMetrics {
                animation_length: f(&format!("{n}AnimationLength")),
                animation_is_vivid: b(&format!("{n}AnimationIsVivid")),
                addition_offset: [
                    f(&format!("{n}AdditionTextureCoordOffsetX")),
                    f(&format!("{n}AdditionTextureCoordOffsetY")),
                ],
                color_spacing: [
                    f(&format!("{n}NoteColorTextureCoordSpacingX")),
                    f(&format!("{n}NoteColorTextureCoordSpacingY")),
                ],
                color_count: i(&format!("{n}NoteColorCount")),
                color_type: NoteColorType::parse(s(&format!("{n}NoteColorType"))),
            }
        };
        NoteDisplay {
            parts: Part::ALL.map(part),
            animation_is_beat_based: b("AnimationIsBeatBased"),
            draw_hold_head_for_taps_on_same_row: b("DrawHoldHeadForTapsOnSameRow"),
            draw_roll_head_for_taps_on_same_row: b("DrawRollHeadForTapsOnSameRow"),
            tap_hold_roll_on_row_means_hold: b("TapHoldRollOnRowMeansHold"),
            start_drawing_hold_body_offset_from_head: i("StartDrawingHoldBodyOffsetFromHead"),
            stop_drawing_hold_body_offset_from_tail: i("StopDrawingHoldBodyOffsetFromTail"),
            hold_let_go_gray_percent: f("HoldLetGoGrayPercent"),
            hold_head_is_above_wavy_parts: b("HoldHeadIsAboveWavyParts"),
            hold_tail_is_above_wavy_parts: b("HoldTailIsAboveWavyParts"),
            flip_head_and_tail_when_reverse: b("FlipHeadAndTailWhenReverse"),
            flip_hold_body_when_reverse: b("FlipHoldBodyWhenReverse"),
            top_hold_anchor_when_reverse: b("TopHoldAnchorWhenReverse"),
            hold_active_is_add_layer: b("HoldActiveIsAddLayer"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::tests::folder;

    #[test]
    fn defaults_come_from_the_fallbacks_and_buttons_override() {
        let folders = [folder(
            "dance",
            "s",
            &[(
                "metrics.ini",
                "[Global]\nFallbackNoteSkin=default\n[NoteDisplay]\nTapNoteAnimationLength=3\n\
                 TapNoteNoteColorType=Progress\nHoldLetGoGrayPercent=0.5\n[Left]\nTapNoteAnimationLength=2\n",
            )],
        )];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        let down = NoteDisplay::read(&skin, "Down");
        let tap = down.part(Part::TapNote);
        assert_eq!(tap.animation_length, 3.0);
        assert_eq!(tap.color_type, Some(NoteColorType::Progress));
        // dance/default's.
        assert!(!tap.animation_is_vivid);
        assert_eq!(tap.color_spacing, [0.0, 0.125]);
        assert!(down.flip_head_and_tail_when_reverse);
        assert_eq!(down.stop_drawing_hold_body_offset_from_tail, 0);
        // common's.
        assert_eq!(down.part(Part::HoldBody).animation_length, 1.0);
        assert_eq!(tap.color_count, 8);
        assert!(down.animation_is_beat_based);
        assert_eq!(down.hold_let_go_gray_percent, 0.5);
        assert_eq!(
            NoteDisplay::read(&skin, "Left")
                .part(Part::TapNote)
                .animation_length,
            2.0
        );
        assert_eq!(
            NoteColorType::parse("progressalternate"),
            Some(NoteColorType::ProgressAlternate)
        );
        assert_eq!(NoteColorType::parse(" Progress"), None);
        // On common alone.
        let plain = Skin::load(&[], "dance", "common").unwrap();
        let c = NoteDisplay::read(&plain, "Down");
        assert!(c.part(Part::TapNote).animation_is_vivid);
        assert_eq!(c.stop_drawing_hold_body_offset_from_tail, -32);
        assert_eq!(c.part(Part::HoldBody).animation_length, 4.0);
        assert!(!c.flip_head_and_tail_when_reverse);
    }
}
