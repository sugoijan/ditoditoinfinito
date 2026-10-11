//! Stand-ins for StepMania's own `dance/default` and `common/common`, the
//! fallbacks every dance skin ends in. Their art is not under an explicit
//! licence (`docs/research/noteskins.md`, section 10), so their files are
//! listed by name only: a lookup lands on them exactly as in StepMania, and
//! what it finds is drawn by the built-in skin. Their redirects, the
//! behaviour of their Lua (reimplemented natively) and their metrics are
//! StepMania 5.1's (`NoteSkins/dance/default/`, `NoteSkins/common/common/`
//! at `5_1-new` 825467b). Their `metrics.ini` and Lua files are StepMania's,
//! kept byte for byte under `builtin/stepmania/` with StepMania's licence
//! (`LICENSES/LicenseRef-StepMania.txt`, credited in `THIRD_PARTY.toml`):
//! the Lua runs when a skin's Lua does (`crate::lua`), the native versions
//! stand in when it cannot.

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
}

/// One of the two stand-in folders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltIn {
    /// `NoteSkins/dance/default`.
    DanceDefault,
    /// `NoteSkins/common/common`.
    Common,
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
    /// `metrics.ini`, `NoteSkin.lua`: never an element.
    Other,
}

impl BuiltIn {
    pub fn name(self) -> &'static str {
        match self {
            BuiltIn::DanceDefault => "default",
            BuiltIn::Common => "common",
        }
    }

    /// The folder's files.
    pub fn files(self) -> &'static [(&'static str, Virtual)] {
        match self {
            BuiltIn::DanceDefault => DANCE_DEFAULT_FILES,
            BuiltIn::Common => COMMON_FILES,
        }
    }

    /// Its `metrics.ini`.
    pub fn metrics(self) -> &'static str {
        match self {
            BuiltIn::DanceDefault => DANCE_DEFAULT_METRICS,
            BuiltIn::Common => COMMON_METRICS,
        }
    }
}

use NativeLua::{DanceDefaultLook, Explosion, Helper, Receptor, TapMine};
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
