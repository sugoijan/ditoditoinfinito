//! Stand-ins for StepMania's own `dance/default` and `common/common`, the
//! fallbacks every dance skin ends in. Their art is not under an explicit
//! licence (`docs/research/noteskins.md`, section 10), so their files are
//! listed by name only: a lookup lands on them exactly as in StepMania, and
//! what it finds is drawn by the built-in skin. Their redirects, the
//! behaviour of their Lua (reimplemented natively) and their metrics are
//! StepMania 5.1's (`NoteSkins/dance/default/`, `NoteSkins/common/common/`
//! at `5_1-new` 825467b; sm-ssc licence, `Docs/Licenses.txt`).

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

const DANCE_DEFAULT_METRICS: &str = r#"[Global]
FallbackNoteSkin=common

[NoteDisplay]
TapNoteAnimationIsVivid=0
DrawHoldHeadForTapsOnSameRow=1
TapNoteAnimationLength=1
TapAdditionAnimationLength=1
TapMineAnimationLength=1
TapLiftAnimationLength=1
TapFakeAnimationLength=1
HoldHeadAnimationLength=1
HoldTopCapAnimationLength=1
HoldBottomCapAnimationLength=1
HoldBodyAnimationLength=1
HoldTailAnimationLength=1
StartDrawingHoldBodyOffsetFromHead=0
StopDrawingHoldBodyOffsetFromTail=0
HoldLetGoGrayPercent=.25
HoldHeadIsAboveWavyParts=1
HoldTailIsAboveWavyParts=1
ReverseDrawOrder=1101
FlipHeadAndTailWhenReverse=1
FlipHoldBodyWhenReverse=1
TapNoteAdditionTextureCoordOffsetX=0
TapNoteAdditionTextureCoordOffsetY=0
TapNoteNoteColorTextureCoordSpacingX=0
TapNoteNoteColorTextureCoordSpacingY=.125
TapFakeNoteColorTextureCoordSpacingX=0
TapFakeNoteColorTextureCoordSpacingY=.125
HoldHeadNoteColorTextureCoordSpacingX=0
HoldHeadNoteColorTextureCoordSpacingY=.125

[GhostArrowDim]
InitCommand=
NoneCommand=
HitMineCommand=finishtweening;blend,'BlendMode_Add';diffuse,color("1,1,1,1");zoom,1;rotationz,0;decelerate,.3;rotationz,90;linear,.3;rotationz,180;diffusealpha,0
MissCommand=
W5Command=finishtweening;diffuse,color("1,.5,1,1");sleep,.1;decelerate,.5;diffusealpha,0
W4Command=finishtweening;diffuse,color(".5,1,1,1");sleep,.1;decelerate,.5;diffusealpha,0
W3Command=finishtweening;diffuse,color(".5,1,.75,1");sleep,.1;decelerate,.5;diffusealpha,0
W2Command=finishtweening;diffuse,color("1,1,.85,1");sleep,.1;decelerate,.5;diffusealpha,0
W1Command=finishtweening;diffuse,color("1,1,1,1");sleep,.1;decelerate,.5;diffusealpha,0
LetGoCommand=stoptweening;decelerate,.2;diffusealpha,0;
HeldCommand=finishtweening;diffuse,color("1,1,1,1");zoom,1;linear,.2;zoom,.9;linear,.1;diffusealpha,0

[GhostArrowBright]
InitCommand=
NoneCommand=
HitMineCommand=finishtweening;blend,'BlendMode_Add';diffuse,color("1,1,1,1");zoom,1;rotationz,0;decelerate,.3;rotationz,90;linear,.3;rotationz,180;diffusealpha,0
MissCommand=
W5Command=finishtweening;diffuse,color("1,.6,1,1");sleep,.1;decelerate,.5;diffusealpha,0
W4Command=finishtweening;diffuse,color(".6,1,1,1");sleep,.1;decelerate,.5;diffusealpha,0
W3Command=finishtweening;diffuse,color(".6,1,.8,1");sleep,.1;decelerate,.5;diffusealpha,0
W2Command=finishtweening;diffuse,color("1,1,.9,1");sleep,.1;decelerate,.5;diffusealpha,0
W1Command=finishtweening;diffuse,color("1,1,1,1");sleep,.1;decelerate,.5;diffusealpha,0
LetGoCommand=stoptweening;decelerate,.2;diffusealpha,0;
HeldCommand=finishtweening;diffuse,color("1,1,1,1");zoom,1;linear,.2;zoom,.9;linear,.1;diffusealpha,0

[HoldGhostArrow]
HoldingOnCommand=diffusealpha,1;glowshift;effectcolor1,color("1,1,1,0");effectcolor2,color("1,1,1,.7");effectperiod,.1
HoldingOffCommand=diffusealpha,0
InitCommand=finishtweening;playcommand,"HoldingOff"

[ReceptorArrow]
InitCommand=effectclock,"beat";diffuseramp;effectcolor1,color(".8,.8,.8,1");effectcolor2,color("1,1,1,1");effecttiming,.2,0,.8,0;effectoffset,.05
NoneCommand=finishtweening;zoom,.85;diffusealpha,.9;linear,.11;diffusealpha,1;zoom,1

[ReceptorOverlay]
InitCommand=diffusealpha,0
PressCommand=finishtweening;zoom,1;blend,"BlendMode_Add";diffusealpha,.8;decelerate,0.2;diffusealpha,0.4
LiftCommand=stoptweening;decelerate,.2;diffusealpha,0;zoom,1.2
"#;

const COMMON_METRICS: &str = r#"[NoteDisplay]
DrawHoldHeadForTapsOnSameRow=1
DrawRollHeadForTapsOnSameRow=1
TapHoldRollOnRowMeansHold=1
AnimationIsBeatBased=1
TapNoteAnimationLength=1
TapMineAnimationLength=1
TapLiftAnimationLength=1
TapFakeAnimationLength=1
HoldHeadAnimationLength=1
HoldTopCapAnimationLength=1
HoldBottomCapAnimationLength=1
HoldBodyAnimationLength=4
HoldTailAnimationLength=4
TapNoteAnimationIsVivid=1
TapMineAnimationIsVivid=0
TapLiftAnimationIsVivid=0
TapFakeAnimationIsVivid=0
HoldHeadAnimationIsVivid=0
HoldTopCapAnimationIsVivid=0
HoldBodyAnimationIsVivid=0
HoldBottomCapAnimationIsVivid=0
HoldTailAnimationIsVivid=0
TapNoteAdditionTextureCoordOffsetX=0
TapNoteAdditionTextureCoordOffsetY=0
TapNoteNoteColorTextureCoordSpacingX=0
TapNoteNoteColorTextureCoordSpacingY=0
TapNoteNoteColorCount=8
TapNoteNoteColorType=Denominator
TapMineAdditionTextureCoordOffsetX=0
TapMineAdditionTextureCoordOffsetY=0
TapMineNoteColorTextureCoordSpacingX=0
TapMineNoteColorTextureCoordSpacingY=0
TapMineNoteColorCount=8
TapMineNoteColorType=Denominator
TapLiftAdditionTextureCoordOffsetX=0
TapLiftAdditionTextureCoordOffsetY=0
TapLiftNoteColorTextureCoordSpacingX=0
TapLiftNoteColorTextureCoordSpacingY=0
TapLiftNoteColorCount=8
TapLiftNoteColorType=Denominator
TapFakeAdditionTextureCoordOffsetX=0
TapFakeAdditionTextureCoordOffsetY=0
TapFakeNoteColorTextureCoordSpacingX=0
TapFakeNoteColorTextureCoordSpacingY=0
TapFakeNoteColorCount=8
TapFakeNoteColorType=Denominator
HoldHeadAdditionTextureCoordOffsetX=0
HoldHeadAdditionTextureCoordOffsetY=0
HoldHeadNoteColorTextureCoordSpacingX=0
HoldHeadNoteColorTextureCoordSpacingY=0
HoldHeadNoteColorCount=8
HoldHeadNoteColorType=Denominator
HoldTopCapAdditionTextureCoordOffsetX=0
HoldTopCapAdditionTextureCoordOffsetY=0
HoldTopCapNoteColorTextureCoordSpacingX=0
HoldTopCapNoteColorTextureCoordSpacingY=0
HoldTopCapNoteColorCount=8
HoldTopCapNoteColorType=Denominator
HoldBodyAdditionTextureCoordOffsetX=0
HoldBodyAdditionTextureCoordOffsetY=0
HoldBodyNoteColorTextureCoordSpacingX=0
HoldBodyNoteColorTextureCoordSpacingY=0
HoldBodyNoteColorCount=8
HoldBodyNoteColorType=Denominator
HoldBottomCapAdditionTextureCoordOffsetX=0
HoldBottomCapAdditionTextureCoordOffsetY=0
HoldBottomCapNoteColorTextureCoordSpacingX=0
HoldBottomCapNoteColorTextureCoordSpacingY=0
HoldBottomCapNoteColorCount=8
HoldBottomCapNoteColorType=Denominator
HoldTailAdditionTextureCoordOffsetX=0
HoldTailAdditionTextureCoordOffsetY=0
HoldTailNoteColorTextureCoordSpacingX=0
HoldTailNoteColorTextureCoordSpacingY=0
HoldTailNoteColorCount=8
HoldTailNoteColorType=Denominator
StartDrawingHoldBodyOffsetFromHead=0
StopDrawingHoldBodyOffsetFromTail=-32	// top of tail
HoldLetGoGrayPercent=0.25
ReverseDrawOrder=1101
HoldHeadIsAboveWavyParts=1
HoldTailIsAboveWavyParts=1
FlipHeadAndTailWhenReverse=0
FlipHoldBodyWhenReverse=0
TopHoldAnchorWhenReverse=0
HoldActiveIsAddLayer=0

[GhostArrowDim]
HitMineCommand=blend,"BlendMode_Add";diffuse,1,1,1,1;zoom,1;rotationz,0;linear,0.3;rotationz,90;linear,0.3;rotationz,180;diffusealpha,0
W5Command=diffuse,0.8,0.0,0.6,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
W4Command=diffuse,0.3,0.8,1.0,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
W3Command=diffuse,0.0,1.0,0.4,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
W2Command=diffuse,1.0,1.0,0.3,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
W1Command=diffuse,1.0,1.0,1.0,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
HeldCommand=diffuse,1.0,1.0,1.0,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0

[GhostArrowBright]
NoneCommand=
HitMineCommand=blend,"BlendMode_Add";diffuse,1,1,1,1;zoom,1;rotationz,0;linear,0.3;rotationz,90;linear,0.3;rotationz,180;diffusealpha,0
AvoidMineCommand=
MissCommand=
W5Command=diffuse,0.8,0.0,0.6,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
W4Command=diffuse,0.3,0.8,1.0,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
W3Command=diffuse,0.0,1.0,0.4,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
W2Command=diffuse,1.0,1.0,0.3,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
W1Command=diffuse,1.0,1.0,1.0,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0
HeldCommand=diffuse,1.0,1.0,1.0,1;zoom,1;linear,0.06;zoom,1.1;linear,0.06;diffusealpha,0

[ReceptorArrow]
InitCommand=effectclock,'beat';diffuseramp;effectcolor1,color("0.35,0.35,0.35,1");effectcolor2,color("1,1,1,1");
NoneCommand=stoptweening;zoom,.75;linear,0.11;zoom,1
HitMineCommand=
AvoidMineCommand=
MissCommand=
W5Command=stoptweening;zoom,.75;linear,0.11;zoom,1
W4Command=stoptweening;zoom,.75;linear,0.11;zoom,1
W3Command=stoptweening;zoom,.75;linear,0.11;zoom,1
W2Command=stoptweening;zoom,.75;linear,0.11;zoom,1
W1Command=stoptweening;zoom,.75;linear,0.11;zoom,1
PressCommand=
LiftCommand=

[HoldGhostArrow]
HoldingOnCommand=visible,true
HoldingOffCommand=visible,false
RollOnCommand=visible,true
RollOffCommand=visible,false

[Press]
ReverseOnCommand=vertalign,bottom
ReverseOffCommand=vertalign,top
PressCommand=visible,true
LiftCommand=visible,false

[Mine]
InitCommand=spin;effectclock,'beat'
"#;

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
