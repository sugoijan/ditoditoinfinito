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
    /// Opt-in: phase descendant sprites/models with this note part's clock.
    /// Otherwise ActorFrame children retain their own animation clocks.
    pub animation_on_children: bool,
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

/// Opt-in continuous material over alpha masks for a hold's parts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoldMaterial {
    pub pitch: f32,
    pub period: f32,
    pub angle: f32,
    pub phase: f32,
    pub speed: f32,
    pub wave: [f32; 3],
    pub colors: [[f32; 3]; 3],
}

/// Metric `text` as a number from `lo` to `hi`, `default` when it is none.
fn bounded(text: &str, default: f32, lo: f32, hi: f32) -> f32 {
    text.parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(default)
        .clamp(lo, hi)
}

impl HoldMaterial {
    fn read(value: impl Fn(&str) -> String) -> Option<Self> {
        (string_to_int(&value("HoldStencilMaterial")) != 0).then(|| Self::params("Hold", value))
    }

    /// The material's parameters, `<prefix>MaterialPitch` and the rest.
    fn params(prefix: &str, value: impl Fn(&str) -> String) -> Self {
        let number = |key: &str, default: f32, lo: f32, hi: f32| {
            bounded(&value(&format!("{prefix}Material{key}")), default, lo, hi)
        };
        let color = |key: &str, default: [f32; 3]| {
            let text = value(&format!("{prefix}Material{key}"));
            let text = text.trim().strip_prefix('#').unwrap_or("");
            if text.len() != 6 || !text.is_ascii() {
                return default;
            }
            let mut rgb = [0.0; 3];
            for (i, v) in rgb.iter_mut().enumerate() {
                let Ok(n) = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16) else {
                    return default;
                };
                *v = n as f32 / 255.0;
            }
            rgb
        };
        Self {
            pitch: number("Pitch", 16.0, 4.0, 64.0),
            period: number("Period", 128.0, 16.0, 2048.0),
            angle: number("Angle", 0.0, -180.0, 180.0).to_radians(),
            phase: number("Phase", 0.0, -100.0, 100.0),
            speed: number("Speed", 0.0, -4.0, 4.0),
            wave: [
                number("Wave", 0.0, 0.0, 128.0),
                number("WaveWidth", 128.0, 16.0, 2048.0),
                number("WaveLength", 128.0, 16.0, 2048.0),
            ],
            colors: [
                color("Green", [0.2, 0.7, 0.2]),
                color("Lime", [0.4, 0.9, 0.2]),
                color("Highlight", [0.9, 1.0, 0.4]),
            ],
        }
    }
}

/// Optional continuous roll material plus the repeated-charge pulse envelope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RollMaterial {
    pub material: HoldMaterial,
    pub duty: f32,
    pub rest: f32,
    pub brightness: f32,
}
impl RollMaterial {
    fn read(value: impl Fn(&str) -> String) -> Option<Self> {
        if string_to_int(&value("RollStencilMaterial")) == 0 {
            return None;
        }
        let material = HoldMaterial::params("Roll", &value);
        let number = |key: &str, default, lo, hi| bounded(&value(key), default, lo, hi);
        Some(Self {
            material,
            duty: number("RollPulseDuty", 0.25, 0.01, 1.0),
            rest: number("RollPulseRest", 0.08, 0.0, 1.0),
            brightness: number("RollPulseBrightness", 0.85, 0.0, 1.0),
        })
    }
    pub fn opacity(self, phase: f32) -> f32 {
        let pulse = (1.0 - phase.rem_euclid(1.0) / self.duty)
            .max(0.0)
            .powf(0.65);
        self.rest + (self.brightness - self.rest) * pulse
    }
}

/// Opt-in lift trails leading their lifts, in the hold material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiftMaterial {
    pub material: HoldMaterial,
    pub seconds: f32,
    pub fade: f32,
    pub opacity: f32,
    pub white: f32,
}

impl LiftMaterial {
    fn read(value: impl Fn(&str) -> String) -> Option<Self> {
        if string_to_int(&value("LiftStencilMaterial")) == 0 {
            return None;
        }
        let material = HoldMaterial::params("Hold", &value);
        let number = |key: &str, default, lo, hi| bounded(&value(key), default, lo, hi);
        Some(Self {
            material,
            seconds: number("LiftPreparationSeconds", 0.28, 0.05, 2.0),
            fade: number("LiftTrailFade", 2.0, 0.1, 8.0),
            opacity: number("LiftTrailOpacity", 1.0, 0.0, 1.0),
            white: number("LiftBandWhite", 0.7, 0.0, 1.0),
        })
    }
}

/// The `[NoteDisplay]` metrics of one button.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteDisplay {
    pub lift_material: Option<LiftMaterial>,
    pub hold_material: Option<HoldMaterial>,
    pub roll_material: Option<RollMaterial>,
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
                animation_on_children: b(&format!("{n}AnimationOnChildren")),
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
            lift_material: LiftMaterial::read(|key| s(key).to_owned()),
            hold_material: HoldMaterial::read(|key| s(key).to_owned()),
            roll_material: RollMaterial::read(|key| s(key).to_owned()),
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
                animation_on_children: false,
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
            lift_material: None,
            hold_material: None,
            roll_material: None,
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

    /// A skin of layered, four-beat Vivid art over Peters-DDR: its own
    /// `[NoteDisplay]` animation, and Peters' `[Center]` color sheet
    /// overridden for the buttons that section names.
    const LAYERED: &str = "[Global]\nFallbackNoteSkin=Peters-DDR\n\n[NoteDisplay]\n\
        AnimationIsBeatBased=1\n\
        TapNoteAnimationLength=4\nTapNoteAnimationIsVivid=1\nTapNoteAnimationOnChildren=1\n\
        TapNoteNoteColorTextureCoordSpacingX=0\nTapNoteNoteColorTextureCoordSpacingY=0\n\
        TapLiftAnimationLength=4\nTapLiftAnimationIsVivid=1\nTapLiftAnimationOnChildren=1\n\
        TapLiftNoteColorTextureCoordSpacingX=0\nTapLiftNoteColorTextureCoordSpacingY=0\n\
        TapFakeAnimationLength=4\nTapFakeAnimationIsVivid=1\nTapFakeAnimationOnChildren=1\n\
        TapFakeNoteColorTextureCoordSpacingX=0\nTapFakeNoteColorTextureCoordSpacingY=0\n\
        HoldHeadAnimationLength=4\nHoldHeadAnimationIsVivid=1\nHoldHeadAnimationOnChildren=1\n\
        HoldHeadNoteColorTextureCoordSpacingX=0\nHoldHeadNoteColorTextureCoordSpacingY=0\n\n\
        [Center]\n\
        TapNoteAnimationLength=4\nTapNoteAnimationIsVivid=1\n\
        TapNoteNoteColorTextureCoordSpacingX=0\nTapNoteNoteColorTextureCoordSpacingY=0\n\
        TapFakeAnimationLength=4\nTapFakeAnimationIsVivid=1\n\
        TapFakeNoteColorTextureCoordSpacingX=0\nTapFakeNoteColorTextureCoordSpacingY=0\n\
        TapLiftAnimationLength=4\nTapLiftAnimationIsVivid=1\n\
        TapLiftNoteColorTextureCoordSpacingX=0\nTapLiftNoteColorTextureCoordSpacingY=0\n";

    #[test]
    fn layered_art_overrides_its_fallbacks_center_color_sheet() {
        let folders = [
            folder("dance", "Layered", &[("metrics.ini", LAYERED)]),
            folder(
                "dance",
                "Peters-DDR",
                &[(
                    "metrics.ini",
                    include_str!("../../../assets/noteskins/dance/Peters-DDR/metrics.ini"),
                )],
            ),
        ];
        let skin = Skin::load(&folders, "dance", "Layered").unwrap();
        for button in ["Up", "Down", "Center", "Onigiri"] {
            let m = NoteDisplay::read(&skin, button);
            assert!(m.animation_is_beat_based);
            for part in [Part::TapNote, Part::TapLift, Part::TapFake, Part::HoldHead] {
                let p = m.part(part);
                assert_eq!(p.animation_length, 4.0);
                assert!(p.animation_is_vivid && p.animation_on_children);
                assert_eq!(p.color_spacing, [0.0; 2], "{button} {part:?}");
            }
            assert!(!m.part(Part::TapMine).animation_on_children);
        }
        let peters = Skin::load(&folders, "dance", "Peters-DDR").unwrap();
        let m = NoteDisplay::read(&peters, "Center");
        assert_eq!(m.part(Part::TapNote).color_spacing, [0.0625, 0.0]);
        assert!(!m.part(Part::TapNote).animation_on_children);
    }

    #[test]
    fn roll_material_is_opt_in_and_pulse_is_bounded() {
        assert!(RollMaterial::read(|_| String::new()).is_none());
        let r = RollMaterial::read(|key| {
            match key {
                "RollStencilMaterial" => "1",
                "RollMaterialPitch" => "25",
                "RollMaterialGreen" => "#3b18ee",
                "RollPulseDuty" => "NaN",
                "RollPulseRest" => "-1",
                "RollPulseBrightness" => "10",
                _ => "",
            }
            .to_string()
        })
        .unwrap();
        assert_eq!(r.material.pitch, 25.0);
        assert_eq!(r.duty, 0.25);
        assert_eq!(r.opacity(0.0), 1.0);
        assert_eq!(r.opacity(0.25), 0.0);
        assert!((r.opacity(0.1) - r.opacity(1.1)).abs() < 1e-5);
    }

    #[test]
    fn lift_material_is_opt_in_and_rejects_nonfinite_values() {
        assert!(LiftMaterial::read(|_| String::new()).is_none());
        let lift = LiftMaterial::read(|key| {
            match key {
                "LiftStencilMaterial" => "1",
                "LiftPreparationSeconds" => "NaN",
                "LiftTrailFade" => "-3",
                "LiftBandWhite" => "2",
                _ => "",
            }
            .to_owned()
        })
        .unwrap();
        assert_eq!(lift.seconds, 0.28);
        assert_eq!(lift.fade, 0.1);
        assert_eq!(lift.white, 1.0);
    }

    #[test]
    fn continuous_hold_material_is_opt_in_and_bounded() {
        let none = HoldMaterial::read(|_| String::new());
        assert!(none.is_none());
        let values = std::collections::HashMap::from([
            ("HoldStencilMaterial", "1"),
            ("HoldMaterialPitch", "12.5"),
            ("HoldMaterialGreen", "#2bab4b"),
            ("HoldMaterialWaveWidth", "NaN"),
            ("HoldMaterialSpeed", "100000"),
        ]);
        let m = HoldMaterial::read(|key| values.get(key).unwrap_or(&"").to_string()).unwrap();
        assert_eq!(m.pitch, 12.5);
        assert_eq!(m.colors[0], [43.0 / 255.0, 171.0 / 255.0, 75.0 / 255.0]);
        assert_eq!(m.wave[1], 128.0);
        assert_eq!(m.speed, 4.0);
    }

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
