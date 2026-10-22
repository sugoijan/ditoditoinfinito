//! The `[NoteDisplay]` metrics the engine reads, per button
//! (`NoteMetricCache_t::Load`, `src/NoteDisplay.cpp:107-137`, 5_1-new).

use crate::ini::{string_to_float, string_to_int};
use crate::skin::Skin;

/// A note part with its own animation and color metrics (`NotePartNames`,
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

/// How a part's color is chosen (`NoteColorType`, `src/NoteDisplay.h:38-45`).
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
    /// is invalid (StepMania then stops with an error when it colors the
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
    /// Texture translation per color, in padded-texture units.
    pub color_spacing: [f32; 2],
    /// As read; StepMania divides by it when coloring, so 0 or less
    /// colors nothing.
    pub color_count: i32,
    /// `None` for a name StepMania does not know.
    pub color_type: Option<NoteColorType>,
    /// StepMania 3.9's format: the part has a file per note type
    /// (`<Part>AnimationIsNoteColor`), and vivid animation is OpenITG's,
    /// rounded a quarter of the animation's step early
    /// (`NoteDisplay::SetActiveFrame`).
    pub note_color: bool,
    pub vivid_rounded: bool,
}

/// The `[NoteDisplay]` metrics of one button.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteDisplay {
    pub parts: [PartMetrics; 9],
    pub animation_is_beat_based: bool,
    /// Opt-in roll body/cap cycle as a fraction of the active roll timing window.
    /// Zero disables it; valid fractions are greater than zero and at most one.
    pub roll_animation_timing_window_ratio: f32,
    /// Reserve a mine actor's direct `MineLink` child for position-based row links.
    pub mine_connect_rows: bool,
    pub draw_hold_head_for_taps_on_same_row: bool,
    pub draw_roll_head_for_taps_on_same_row: bool,
    pub tap_hold_roll_on_row_means_hold: bool,
    /// Pixels, integers as StepMania reads them.
    pub start_drawing_hold_body_offset_from_head: i32,
    pub stop_drawing_hold_body_offset_from_tail: i32,
    pub hold_let_go_gray_percent: f32,
    /// Our opt-in extension: load separate `Hold/Roll … Dropped` actors.
    pub hold_use_dropped_state: bool,
    /// Our opt-in extension: flash individual hits rather than completed rows.
    pub tap_judgments_per_note: bool,
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
        if skin.legacy {
            return Self::read_legacy(skin, button);
        }
        let s = |name: &str| skin.metric(button, name).unwrap_or("");
        let f = |name: &str| string_to_float(s(name));
        let i = |name: &str| string_to_int(s(name));
        let b = |name: &str| i(name) != 0;
        let part = |p: Part| {
            let n = p.name();
            PartMetrics {
                note_color: false,
                vivid_rounded: false,
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
            roll_animation_timing_window_ratio: {
                let ratio = f("RollAnimationTimingWindowRatio");
                if ratio.is_finite() && ratio > 0.0 && ratio <= 1.0 {
                    ratio
                } else {
                    0.0
                }
            },
            draw_hold_head_for_taps_on_same_row: b("DrawHoldHeadForTapsOnSameRow"),
            mine_connect_rows: b("MineConnectRows"),
            draw_roll_head_for_taps_on_same_row: b("DrawRollHeadForTapsOnSameRow"),
            tap_hold_roll_on_row_means_hold: b("TapHoldRollOnRowMeansHold"),
            start_drawing_hold_body_offset_from_head: i("StartDrawingHoldBodyOffsetFromHead"),
            stop_drawing_hold_body_offset_from_tail: i("StopDrawingHoldBodyOffsetFromTail"),
            hold_let_go_gray_percent: f("HoldLetGoGrayPercent"),
            hold_use_dropped_state: b("HoldUseDroppedState"),
            tap_judgments_per_note: b("TapJudgmentsPerNote"),
            hold_head_is_above_wavy_parts: b("HoldHeadIsAboveWavyParts"),
            hold_tail_is_above_wavy_parts: b("HoldTailIsAboveWavyParts"),
            flip_head_and_tail_when_reverse: b("FlipHeadAndTailWhenReverse"),
            flip_hold_body_when_reverse: b("FlipHoldBodyWhenReverse"),
            top_hold_anchor_when_reverse: b("TopHoldAnchorWhenReverse"),
            hold_active_is_add_layer: b("HoldActiveIsAddLayer"),
        }
    }

    /// StepMania 3.9's and OpenITG's metrics (`NoteMetricCache_t::Load`,
    /// OpenITG `src/NoteDisplay.cpp:61-83`): animations always on the beat,
    /// no color by texture translation, lifts and fakes as tap notes. Its
    /// `Part` enum lists the hold's tail after the head where its names
    /// (`PartNames`) list it last, so each hold part but the head reads the
    /// metrics of the name after its own: the tail `HoldTopCap…`, the top
    /// cap `HoldBody…`, the body `HoldBottomCap…`, the bottom cap
    /// `HoldTail…` (`src/NoteDisplay.cpp:12-37`).
    fn read_legacy(skin: &Skin<'_>, button: &str) -> NoteDisplay {
        let s = |name: &str| skin.metric(button, name).unwrap_or("");
        let f = |name: &str| string_to_float(s(name));
        let i = |name: &str| string_to_int(s(name));
        let b = |name: &str| i(name) != 0;
        let part = |p: Part| {
            let n = match p {
                Part::TapLift | Part::TapFake => Part::TapNote.name(),
                Part::HoldTail => Part::HoldTopCap.name(),
                Part::HoldTopCap => Part::HoldBody.name(),
                Part::HoldBody => Part::HoldBottomCap.name(),
                Part::HoldBottomCap => Part::HoldTail.name(),
                p => p.name(),
            };
            PartMetrics {
                animation_length: f(&format!("{n}AnimationLengthInBeats")),
                animation_is_vivid: b(&format!("{n}AnimationIsVivid")),
                addition_offset: [0.0; 2],
                color_spacing: [0.0; 2],
                color_count: 0,
                color_type: Some(NoteColorType::Denominator),
                note_color: b(&format!("{n}AnimationIsNoteColor")),
                vivid_rounded: true,
            }
        };
        let hold_head = b("DrawHoldHeadForTapsOnSameRow");
        NoteDisplay {
            parts: Part::ALL.map(part),
            animation_is_beat_based: true,
            roll_animation_timing_window_ratio: 0.0,
            mine_connect_rows: false,
            draw_hold_head_for_taps_on_same_row: hold_head,
            draw_roll_head_for_taps_on_same_row: hold_head,
            tap_hold_roll_on_row_means_hold: false,
            start_drawing_hold_body_offset_from_head: i("StartDrawingHoldBodyOffsetFromHead"),
            stop_drawing_hold_body_offset_from_tail: i("StopDrawingHoldBodyOffsetFromTail"),
            hold_let_go_gray_percent: f("HoldNGGrayPercent"),
            hold_use_dropped_state: false,
            tap_judgments_per_note: false,
            hold_head_is_above_wavy_parts: b("HoldHeadIsAboveWavyParts"),
            hold_tail_is_above_wavy_parts: b("HoldTailIsAboveWavyParts"),
            flip_head_and_tail_when_reverse: b("FlipHeadAndTailWhenReverse"),
            flip_hold_body_when_reverse: false,
            top_hold_anchor_when_reverse: false,
            hold_active_is_add_layer: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::tests::folder;

    #[test]
    fn mine_connections_are_opt_in_and_button_specific() {
        let folders = [folder(
            "dance",
            "s",
            &[(
                "metrics.ini",
                "[Global]\nFallbackNoteSkin=default\n[Left]\nMineConnectRows=1\n",
            )],
        )];
        let skin = Skin::load(&folders, "dance", "s").unwrap();
        assert!(!NoteDisplay::read(&skin, "Down").mine_connect_rows);
        assert!(NoteDisplay::read(&skin, "Left").mine_connect_rows);
    }

    #[test]
    fn roll_window_animation_is_opt_in_and_rejects_unsafe_ratios() {
        for (value, expected) in [
            ("", 0.0),
            ("0.75", 0.75),
            ("1", 1.0),
            ("-1", 0.0),
            ("1.1", 0.0),
            ("NaN", 0.0),
            ("inf", 0.0),
        ] {
            let text = format!(
                "[Global]\nFallbackNoteSkin=default\n[NoteDisplay]\nRollAnimationTimingWindowRatio={value}\n"
            );
            let folders = [folder("dance", "s", &[("metrics.ini", &text)])];
            let skin = Skin::load(&folders, "dance", "s").unwrap();
            assert_eq!(
                NoteDisplay::read(&skin, "Down").roll_animation_timing_window_ratio,
                expected
            );
        }
    }

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

    #[test]
    fn stepmania_39_skins_have_their_own_names() {
        let folders = [folder(
            "dance",
            "old",
            &[
                (
                    "metrics.ini",
                    "[NoteDisplay]\nTapNoteAnimationLengthInBeats=2\nTapNoteAnimationIsNoteColor=1\n\
                     HoldNGGrayPercent=0.3\n[Up]\nHoldBottomCapAnimationIsNoteColor=1\n",
                ),
                ("Down Tap Note 4th.sprite", ""),
            ],
        )];
        let skin = Skin::load(&folders, "dance", "old").unwrap();
        let down = NoteDisplay::read(&skin, "Down");
        let tap = down.part(Part::TapNote);
        assert_eq!(tap.animation_length, 2.0);
        assert!(tap.note_color && tap.vivid_rounded);
        assert_eq!(tap.color_spacing, [0.0; 2]);
        // Fakes are tap notes.
        assert_eq!(down.part(Part::TapFake), tap);
        assert!(down.animation_is_beat_based);
        assert_eq!(down.hold_let_go_gray_percent, 0.3);
        // StepMania 3.9's dance/default under it: vivid tap notes, holds
        // 4 beats long.
        assert!(tap.animation_is_vivid);
        assert_eq!(down.part(Part::HoldBody).animation_length, 4.0);
        assert_eq!(down.stop_drawing_hold_body_offset_from_tail, -32);
        assert!(!down.part(Part::HoldBody).note_color);
        assert!(
            NoteDisplay::read(&skin, "Up")
                .part(Part::HoldBody)
                .note_color
        );
    }
}
