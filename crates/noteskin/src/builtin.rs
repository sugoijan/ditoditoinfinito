//! Stand-ins for StepMania's own `dance/default`, `pump/default` and
//! `common/common`, the fallbacks every dance and pump skin ends in. Their art is not under an explicit
//! licence (`docs/research/noteskins.md`, section 10), so their files are
//! listed by name only: a lookup lands on them exactly as in StepMania, and
//! what it finds is drawn by the built-in skin. Their redirects, the
//! behaviour of their Lua (reimplemented natively) and their metrics are
//! StepMania 5.1's (`NoteSkins/dance/default/`, `NoteSkins/common/common/`
//! at `5_1-new` 825467b). Their `metrics.ini` and Lua files are StepMania's,
//! kept byte for byte under `builtin/stepmania/` with StepMania's licence
//! (`LICENSES/LicenseRef-StepMania.txt`, credited in `THIRD_PARTY.toml`):
//! the Lua runs when a skin's Lua does (`crate::lua`), the native versions
//! stand in when it cannot. Skins in StepMania 3.9's and OpenITG's format
//! end in stand-ins of their own (`crate::legacy`): OpenITG's
//! `dance/default` by its layout, with StepMania 3.9's metrics
//! (`builtin/stepmania39/`, the same licence).

/// The stand-ins' Lua files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeLua {
    /// common's `Fallback Explosion.lua`: the explosion row's parts with the
    /// `[GhostArrow…]` and `[HoldGhostArrow]` commands.
    Explosion,
    /// common's `Fallback Receptor.lua`: `_Tap Lead-in Receptor.lua`, the
    /// Ready and Go receptors with the `[ReceptorArrow]` commands.
    Receptor,
    /// common's `Fallback Tap Mine.lua`: `_mine` with `[Mine] InitCommand`.
    TapMine,
    /// common's private helpers (`_Tap Receptor.lua` and the like).
    Helper,
    /// dance/default's `Down Tap Note.lua`, `Down Tap Lift.lua` and
    /// `Down Receptor.lua`: its own look, drawn by the built-in skin.
    DanceDefaultLook,
    /// pump/default's Lua (its receptor, explosion, the right side's notes
    /// and receptors taken from the left's, the lifts): it runs when the
    /// skin's Lua runs, which a pump skin's always does (pump/default's
    /// `NoteSkin.lua` builds every element); otherwise drawn by the
    /// built-in skin.
    PumpDefaultLook,
}

/// One of the stand-in folders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltIn {
    /// `NoteSkins/dance/default`.
    DanceDefault,
    /// `NoteSkins/pump/default`.
    PumpDefault,
    /// `NoteSkins/common/common`.
    Common,
    /// StepMania 3.9's and OpenITG's `NoteSkins/dance/default`, for skins in
    /// their format (`crate::legacy`; `docs/research/noteskins.md` section
    /// 11): OpenITG's files by name, StepMania 3.9's metrics.
    LegacyDefault,
    /// StepMania 3.9's `NoteSkins/dance/flat`: metrics only.
    LegacyFlat,
    /// OpenITG's `NoteSkins/common/default`, by name, with metrics of ours.
    LegacyCommon,
}

/// What a stand-in file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Virtual {
    /// An image of the stand-in's art: the built-in skin draws it.
    Art,
    /// A Lua file, reimplemented natively or drawn by the built-in skin.
    Lua(NativeLua),
    /// A `.redir` with this target.
    Redir(&'static str),
    /// An `.actor` of StepMania 3.9's format whose file is
    /// `@NOTESKIN:GetPath(button, element)`, turned `rotation` degrees.
    Actor {
        button: &'static str,
        element: &'static str,
        rotation: i16,
    },
    /// `metrics.ini`, `NoteSkin.lua`: never an element.
    Other,
}

impl BuiltIn {
    pub fn name(self) -> &'static str {
        match self {
            BuiltIn::DanceDefault
            | BuiltIn::PumpDefault
            | BuiltIn::LegacyDefault
            | BuiltIn::LegacyCommon => "default",
            BuiltIn::Common => "common",
            BuiltIn::LegacyFlat => "flat",
        }
    }

    /// The game folder it sits in.
    pub fn game(self) -> &'static str {
        match self {
            BuiltIn::Common | BuiltIn::LegacyCommon => "common",
            BuiltIn::PumpDefault => "pump",
            _ => "dance",
        }
    }

    /// The folder's files.
    pub fn files(self) -> &'static [(&'static str, Virtual)] {
        match self {
            BuiltIn::DanceDefault => DANCE_DEFAULT_FILES,
            BuiltIn::PumpDefault => PUMP_DEFAULT_FILES,
            BuiltIn::Common => COMMON_FILES,
            BuiltIn::LegacyDefault => legacy_default_files(),
            BuiltIn::LegacyFlat => LEGACY_FLAT_FILES,
            BuiltIn::LegacyCommon => LEGACY_COMMON_FILES,
        }
    }

    /// Its metrics as the chain merges them: its `metrics.ini`, but for
    /// StepMania 3.9's `dance/default` without `[ReceptorArrow]`, which
    /// OpenITG's `dance/default` (the stand-in's files) does not have, so
    /// its empty commands do not silence `common/default`'s.
    pub fn ini(self) -> crate::ini::Ini {
        let mut ini = crate::ini::Ini::parse(self.metrics());
        if self == BuiltIn::LegacyDefault {
            ini.remove_section("ReceptorArrow");
        }
        ini
    }

    /// Its `metrics.ini`.
    pub fn metrics(self) -> &'static str {
        match self {
            BuiltIn::DanceDefault => DANCE_DEFAULT_METRICS,
            BuiltIn::PumpDefault => PUMP_DEFAULT_METRICS,
            BuiltIn::Common => COMMON_METRICS,
            BuiltIn::LegacyDefault => LEGACY_DEFAULT_METRICS,
            BuiltIn::LegacyFlat => LEGACY_FLAT_METRICS,
            BuiltIn::LegacyCommon => LEGACY_COMMON_METRICS,
        }
    }
}

/// OpenITG's `dance/default` as skins in its format find it
/// (`assets/d4/NoteSkins/dance/default` at `f2c129fe`, ITG's data: its
/// layout only, the art drawn built in). It draws Down's elements for the
/// other buttons, turned (`.actor` files on `@NOTESKIN:GetPath`), so a skin
/// may have Down's only; it has the 4th to 64th's tap notes of its own and
/// sends the 24th, 48th and 192nd to the 12th's; a hold's or roll's head is
/// the tap note, inactive as active; Down's bright explosions are its dim
/// ones, OK and NG the Perfect's; the mine is `_mine.model`, which the skin
/// brings.
fn legacy_default_files() -> &'static [(&'static str, Virtual)] {
    static FILES: std::sync::LazyLock<Vec<(&'static str, Virtual)>> =
        std::sync::LazyLock::new(|| {
            let leak = |s: String| -> &'static str { s.leak() };
            let mut files = vec![
                ("_Down Receptor Go.xml", Art),
                ("_Down Receptor tex 4x1.png", Art),
                ("_Tap Explosion HitMine.png", Art),
                ("_blank.png", Art),
                ("metrics.ini", Other),
            ];
            let judged = ["Boo", "Good", "Great", "Perfect", "Marvelous"];
            for (button, rotation) in [("Down", 0), ("Left", 90), ("Right", -90), ("Up", 180)] {
                let down = button == "Down";
                let mut add = |name: String, virt: Virtual| {
                    files.push((leak(format!("{button} {name}")), virt));
                };
                for note_type in LEGACY_NOTE_TYPES {
                    let own = match note_type {
                        "24th" | "48th" | "192nd" => "12th",
                        t => t,
                    };
                    let turned = |element: String| Virtual::Actor {
                        button: "Down",
                        element: leak(element),
                        rotation,
                    };
                    if !down {
                        add(
                            format!("Tap Note {note_type}.actor"),
                            turned(format!("Tap Note {own}")),
                        );
                    } else if own != note_type {
                        add(
                            format!("Tap Note {note_type}.redir"),
                            Redir(leak(format!("Down Tap Note {own}.model"))),
                        );
                    }
                    for hold in ["Hold", "Roll"] {
                        add(
                            format!(
                                "{hold} Head Active {note_type}.{}",
                                if down { "redir" } else { "actor" }
                            ),
                            if down {
                                Redir(leak(format!("Down Tap Note {own}.model")))
                            } else {
                                turned(format!("Hold Head Active {own}"))
                            },
                        );
                        add(
                            format!("{hold} Head Inactive {note_type}.actor"),
                            turned(format!("Hold Head Active {own}")),
                        );
                    }
                }
                for hold in ["Hold", "Roll"] {
                    if down {
                        continue;
                    }
                    for part in ["Body", "BottomCap", "TopCap"] {
                        for active in ["Active", "Inactive"] {
                            add(
                                format!("{hold} {part} {active}.redir"),
                                Redir(leak(format!("Down {hold} {part} {active}"))),
                            );
                        }
                    }
                    add(format!("{hold} Explosion.sprite"), Art);
                }
                if down {
                    add("Hold Explosion.png".into(), Art);
                }
                add("KeypressBlock.redir".into(), Redir("_blank"));
                add("Receptor Go.actor".into(), Art);
                add("Receptor Waiting.actor".into(), Art);
                add("Tap Mine.redir".into(), Redir("_mine.model"));
                for row in ["Dim", "Bright"] {
                    add(format!("Tap Explosion {row} Miss.redir"), Redir("_blank"));
                    add(
                        format!("Tap Explosion {row} HitMine.redir"),
                        Redir("_Tap Explosion HitMine"),
                    );
                    for &score in judged.iter().chain(&["OK", "NG"]) {
                        let image = if judged.contains(&score) {
                            score
                        } else {
                            "Perfect"
                        };
                        let name = format!("Tap Explosion {row} {score}");
                        if !down {
                            add(format!("{name}.sprite"), Art);
                        } else if row == "Dim" && image == score {
                            add(format!("{name}.png"), Art);
                        } else {
                            add(
                                format!("{name}.redir"),
                                Redir(leak(format!("Down Tap Explosion Dim {image}"))),
                            );
                        }
                    }
                }
            }
            files
        });
    &FILES
}

/// OpenITG's note types, the names of its per-color files
/// (`NoteTypeNames`); it loads the first eight (`NOTE_COLOR_IMAGES`), a
/// 192nd drawn as a 64th.
pub const LEGACY_NOTE_TYPES: [&str; 9] = [
    "4th", "8th", "12th", "16th", "24th", "32nd", "48th", "64th", "192nd",
];

const LEGACY_FLAT_FILES: &[(&str, Virtual)] = &[("metrics.ini", Other)];

/// OpenITG's `common/default` (`assets/d4`, ITG's data: names and
/// redirects only): what a skin leaves out lands here as `Fallback …`.
const LEGACY_COMMON_FILES: &[(&str, Virtual)] = &[
    ("Fallback Hold Body Active.png", Art),
    ("Fallback Hold Body Inactive.png", Art),
    ("Fallback Hold BottomCap Active.png", Art),
    ("Fallback Hold BottomCap Inactive.png", Art),
    ("Fallback Hold Explosion 2x1.png", Art),
    ("Fallback Hold Head Active.png", Art),
    ("Fallback Hold Head Inactive.png", Art),
    ("Fallback Hold Tail Active.redir", Redir("_blank.png")),
    ("Fallback Hold Tail Inactive.redir", Redir("_blank.png")),
    ("Fallback Hold TopCap Active.redir", Redir("_blank.png")),
    ("Fallback Hold TopCap Inactive.redir", Redir("_blank.png")),
    ("Fallback KeypressBlock.redir", Redir("_blank.png")),
    ("Fallback Receptor go.sprite", Art),
    ("Fallback Receptor waiting.png", Art),
    ("Fallback Roll Tail Active.redir", Redir("_blank.png")),
    ("Fallback Roll Tail Inactive.redir", Redir("_blank.png")),
    ("Fallback Roll TopCap Active.redir", Redir("_blank.png")),
    ("Fallback Roll TopCap Inactive.redir", Redir("_blank.png")),
    (
        "Fallback Tap Explosion Bright AvoidMine.redir",
        Redir("_blank"),
    ),
    ("Fallback Tap Explosion Bright None.redir", Redir("_blank")),
    (
        "Fallback Tap Explosion Dim AvoidMine.redir",
        Redir("_blank"),
    ),
    ("Fallback Tap Explosion Dim None.redir", Redir("_blank")),
    ("_blank.png", Art),
    ("metrics.ini", Other),
];

const LEGACY_DEFAULT_METRICS: &str = include_str!("builtin/stepmania39/dance/default/metrics.ini");

const LEGACY_FLAT_METRICS: &str = include_str!("builtin/stepmania39/dance/flat/metrics.ini");

/// Ours: what OpenITG's `common/default` sets that StepMania 3.9's
/// `dance/default` does not (`docs/research/noteskins.md` section 11): no
/// lighting, the hold explosion's command, a receptor that shrinks and
/// grows back on a step, and a held hold's flash (OK, in Perfect's color
/// and 3.9's manner; NG shows nothing).
const LEGACY_COMMON_METRICS: &str = "[NoteDisplay]
TapNoteUseLighting=0
TapAdditionUseLighting=0
TapMineUseLighting=0
HoldHeadUseLighting=0
HoldTailUseLighting=0
FlipHeadAndTailWhenReverse=0

[ReceptorArrow]
NoneCommand=zoom,0.75;linear,0.11;zoom,1
BooCommand=zoom,0.75;linear,0.11;zoom,1
GoodCommand=zoom,0.75;linear,0.11;zoom,1
GreatCommand=zoom,0.75;linear,0.11;zoom,1
PerfectCommand=zoom,0.75;linear,0.11;zoom,1
MarvelousCommand=zoom,0.75;linear,0.11;zoom,1

[GhostArrowDim]
NGCommand=
OKCommand=diffuse,1.0,1.0,0.3,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0

[GhostArrowBright]
NGCommand=
OKCommand=diffuse,1.0,1.0,0.3,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0

[HoldGhostArrow]
OnCommand=
";

use NativeLua::{DanceDefaultLook, Explosion, Helper, PumpDefaultLook, Receptor, TapMine};
use Virtual::{Art, Lua, Other, Redir};

const DANCE_DEFAULT_FILES: &[(&str, Virtual)] = &[
    ("Down Hold Body Active (doubleres).png", Art),
    ("Down Hold Body Inactive (doubleres).png", Art),
    ("Down Hold BottomCap Active (doubleres).png", Art),
    ("Down Hold BottomCap Inactive (doubleres).png", Art),
    ("Down Hold TopCap Active (doubleres).png", Art),
    ("Down Hold TopCap Inactive (doubleres).png", Art),
    ("Down Receptor.lua", Lua(DanceDefaultLook)),
    ("Down Roll Body Active (doubleres).png", Art),
    ("Down Roll Body Inactive (doubleres).png", Art),
    ("Down Roll BottomCap Active (doubleres).png", Art),
    ("Down Roll BottomCap Inactive (doubleres).png", Art),
    ("Down Roll TopCap Active (doubleres).png", Art),
    ("Down Roll TopCap Inactive (doubleres).png", Art),
    ("Down Tap Explosion Bright.redir", Redir("_glow")),
    (
        "Down Tap Explosion Dim.redir",
        Redir("Down Tap Explosion Bright"),
    ),
    ("Down Tap Lift.lua", Lua(DanceDefaultLook)),
    ("Down Tap Note.lua", Lua(DanceDefaultLook)),
    ("NoteSkin.lua", Other),
    ("_arrow 1x8 (doubleres).png", Art),
    ("_circle (doubleres).png", Art),
    ("_glow (doubleres).png", Art),
    ("_receptor (doubleres).png", Art),
    ("_rflash (doubleres).png", Art),
    ("metrics.ini", Other),
];

/// StepMania 5.1's `NoteSkins/pump/default`: its art by name (drawn by
/// the built-in skin), its Lua and metrics as they are.
const PUMP_DEFAULT_FILES: &[(&str, Virtual)] = &[
    ("Center Hold Body Active 6x1.png", Art),
    ("Center Hold BottomCap Active 6x1.png", Art),
    ("Center Outline Receptor.png", Art),
    ("Center Ready Receptor 1x3.png", Art),
    ("Center Tap Lift.lua", Lua(PumpDefaultLook)),
    ("Center Tap Note 3x2.png", Art),
    ("DownLeft Hold Body Active 6x1.png", Art),
    ("DownLeft Hold BottomCap Active 6x1.png", Art),
    ("DownLeft Outline Receptor.redir", Redir("_blank")),
    ("DownLeft Ready Receptor 1x3.png", Art),
    ("DownLeft Tap Lift.lua", Lua(PumpDefaultLook)),
    ("DownLeft Tap Note 3x2.png", Art),
    ("DownRight Hold Body Active 6x1.png", Art),
    ("DownRight Hold BottomCap Active 6x1.png", Art),
    ("DownRight Ready Receptor.lua", Lua(PumpDefaultLook)),
    ("DownRight Tap Lift.lua", Lua(PumpDefaultLook)),
    ("DownRight Tap Note.lua", Lua(PumpDefaultLook)),
    ("NoteSkin.lua", Other),
    ("UpLeft Explosion.lua", Lua(PumpDefaultLook)),
    ("UpLeft Hold Body Active 6x1.png", Art),
    ("UpLeft Hold BottomCap Active 6x1.png", Art),
    ("UpLeft Ready Receptor 1x3.png", Art),
    ("UpLeft Receptor.lua", Lua(PumpDefaultLook)),
    ("UpLeft Tap Lift.lua", Lua(PumpDefaultLook)),
    ("UpLeft Tap Note 3x2.png", Art),
    ("UpRight Hold Body Active 6x1.png", Art),
    ("UpRight Hold BottomCap Active 6x1.png", Art),
    ("UpRight Ready Receptor.lua", Lua(PumpDefaultLook)),
    ("UpRight Tap Lift.lua", Lua(PumpDefaultLook)),
    ("UpRight Tap Note.lua", Lua(PumpDefaultLook)),
    ("_flash (doubleres).png", Art),
    ("metrics.ini", Other),
];

const COMMON_FILES: &[(&str, Virtual)] = &[
    ("Fallback Explosion.lua", Lua(Explosion)),
    (
        "Fallback Go Receptor.redir",
        Redir("Fallback Ready Receptor"),
    ),
    ("Fallback HitMine Explosion.png", Art),
    ("Fallback Hold Body active (doubleres).png", Art),
    ("Fallback Hold Body inactive (doubleres).png", Art),
    ("Fallback Hold BottomCap active (doubleres).png", Art),
    ("Fallback Hold BottomCap inactive (doubleres).png", Art),
    ("Fallback Hold Explosion 2x1.png", Art),
    (
        "Fallback Hold Head Active.redir",
        Redir("Fallback Tap Note"),
    ),
    (
        "Fallback Hold Head Inactive.redir",
        Redir("Fallback Tap Note"),
    ),
    ("Fallback Hold Tail Active.redir", Redir("_blank")),
    ("Fallback Hold Tail Inactive.redir", Redir("_blank")),
    ("Fallback Hold TopCap Active (doubleres).png", Art),
    ("Fallback Hold TopCap Inactive (doubleres).png", Art),
    ("Fallback Ready Receptor 3x1.png", Art),
    ("Fallback Receptor.lua", Lua(Receptor)),
    ("Fallback Roll Body active (doubleres).png", Art),
    ("Fallback Roll Body inactive (doubleres).png", Art),
    ("Fallback Roll BottomCap active (doubleres).png", Art),
    ("Fallback Roll BottomCap inactive (doubleres).png", Art),
    (
        "Fallback Roll Explosion.redir",
        Redir("Fallback Hold Explosion"),
    ),
    (
        "Fallback Roll Head Active.redir",
        Redir("Fallback Tap Note"),
    ),
    (
        "Fallback Roll Head Inactive.redir",
        Redir("Fallback Tap Note"),
    ),
    ("Fallback Roll Tail Active.redir", Redir("_blank")),
    ("Fallback Roll Tail Inactive.redir", Redir("_blank")),
    ("Fallback Roll TopCap Active (doubleres).png", Art),
    ("Fallback Roll TopCap Inactive (doubleres).png", Art),
    ("Fallback Tap Explosion Bright.png", Art),
    ("Fallback Tap Explosion Dim.png", Art),
    ("Fallback Tap Fake 4x1.png", Art),
    ("Fallback Tap Lift 4x1.png", Art),
    ("Fallback Tap Mine.lua", Lua(TapMine)),
    ("Fallback Tap Note 4x1.png", Art),
    ("NoteSkin.lua", Other),
    ("_Tap Lead-in Receptor.lua", Lua(Helper)),
    ("_Tap Press.lua", Lua(Helper)),
    ("_Tap Receptor.lua", Lua(Helper)),
    ("_blank.png", Art),
    ("_mine (doubleres).png", Art),
    ("metrics.ini", Other),
];

/// StepMania's files, byte for byte (`REUSE.toml` names their licence).
/// StepMania's Lua for the stand-ins, byte for byte (run when a skin's Lua
/// runs; the native versions above stand in when it cannot).
const LUA_SOURCES: &[(BuiltIn, &str, &str)] = &[
    (
        BuiltIn::Common,
        "NoteSkin.lua",
        include_str!("builtin/stepmania/common/common/NoteSkin.lua"),
    ),
    (
        BuiltIn::Common,
        "Fallback Explosion.lua",
        include_str!("builtin/stepmania/common/common/Fallback Explosion.lua"),
    ),
    (
        BuiltIn::Common,
        "Fallback Receptor.lua",
        include_str!("builtin/stepmania/common/common/Fallback Receptor.lua"),
    ),
    (
        BuiltIn::Common,
        "Fallback Tap Mine.lua",
        include_str!("builtin/stepmania/common/common/Fallback Tap Mine.lua"),
    ),
    (
        BuiltIn::Common,
        "_Tap Lead-in Receptor.lua",
        include_str!("builtin/stepmania/common/common/_Tap Lead-in Receptor.lua"),
    ),
    (
        BuiltIn::Common,
        "_Tap Press.lua",
        include_str!("builtin/stepmania/common/common/_Tap Press.lua"),
    ),
    (
        BuiltIn::Common,
        "_Tap Receptor.lua",
        include_str!("builtin/stepmania/common/common/_Tap Receptor.lua"),
    ),
    (
        BuiltIn::DanceDefault,
        "NoteSkin.lua",
        include_str!("builtin/stepmania/dance/default/NoteSkin.lua"),
    ),
    (
        BuiltIn::DanceDefault,
        "Down Receptor.lua",
        include_str!("builtin/stepmania/dance/default/Down Receptor.lua"),
    ),
    (
        BuiltIn::DanceDefault,
        "Down Tap Lift.lua",
        include_str!("builtin/stepmania/dance/default/Down Tap Lift.lua"),
    ),
    (
        BuiltIn::DanceDefault,
        "Down Tap Note.lua",
        include_str!("builtin/stepmania/dance/default/Down Tap Note.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "NoteSkin.lua",
        include_str!("builtin/stepmania/pump/default/NoteSkin.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "Center Tap Lift.lua",
        include_str!("builtin/stepmania/pump/default/Center Tap Lift.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "DownLeft Tap Lift.lua",
        include_str!("builtin/stepmania/pump/default/DownLeft Tap Lift.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "DownRight Ready Receptor.lua",
        include_str!("builtin/stepmania/pump/default/DownRight Ready Receptor.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "DownRight Tap Lift.lua",
        include_str!("builtin/stepmania/pump/default/DownRight Tap Lift.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "DownRight Tap Note.lua",
        include_str!("builtin/stepmania/pump/default/DownRight Tap Note.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "UpLeft Explosion.lua",
        include_str!("builtin/stepmania/pump/default/UpLeft Explosion.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "UpLeft Receptor.lua",
        include_str!("builtin/stepmania/pump/default/UpLeft Receptor.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "UpLeft Tap Lift.lua",
        include_str!("builtin/stepmania/pump/default/UpLeft Tap Lift.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "UpRight Ready Receptor.lua",
        include_str!("builtin/stepmania/pump/default/UpRight Ready Receptor.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "UpRight Tap Lift.lua",
        include_str!("builtin/stepmania/pump/default/UpRight Tap Lift.lua"),
    ),
    (
        BuiltIn::PumpDefault,
        "UpRight Tap Note.lua",
        include_str!("builtin/stepmania/pump/default/UpRight Tap Note.lua"),
    ),
];

impl BuiltIn {
    /// The source of the stand-in's Lua file `path`.
    pub fn lua_source(self, path: &str) -> Option<&'static str> {
        LUA_SOURCES
            .iter()
            .find(|(b, p, _)| *b == self && p.eq_ignore_ascii_case(path))
            .map(|(_, _, s)| *s)
    }
}

const DANCE_DEFAULT_METRICS: &str = include_str!("builtin/stepmania/dance/default/metrics.ini");

const COMMON_METRICS: &str = include_str!("builtin/stepmania/common/common/metrics.ini");

const PUMP_DEFAULT_METRICS: &str = include_str!("builtin/stepmania/pump/default/metrics.ini");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::Ini;

    #[test]
    fn the_stand_ins_are_stepmanias_folders() {
        let common = Ini::parse(BuiltIn::Common.metrics());
        assert_eq!(common.get("Global", "FallbackNoteSkin"), None);
        assert_eq!(
            common.get("NoteDisplay", "StopDrawingHoldBodyOffsetFromTail"),
            Some("-32\t// top of tail")
        );
        let dance = Ini::parse(BuiltIn::DanceDefault.metrics());
        assert_eq!(dance.get("Global", "FallbackNoteSkin"), Some("common"));
        assert_eq!(
            dance.get("NoteDisplay", "FlipHeadAndTailWhenReverse"),
            Some("1")
        );
    }
}
