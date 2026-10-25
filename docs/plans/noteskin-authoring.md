# Noteskin authoring with SkinCraft

[SkinCraft](https://github.com/sugoijan/skincraft) is a separate native application.
It embeds PhotoCraft and VectorCraft and draws the real DDI field preview.
Its source, architecture notes, launchers, MCP bridge and tests live in that
repository. See its README for Git pins and optional local Cargo patches.

DDI retains the shared loader, SVG support, Lua host, field renderer, demo
and bundled noteskins. SkinCraft consumes these crates as pinned Git dependencies;
DDI has no dependency on the editors. `ddi_render::HUD_FONT` supplies the same
font used by the game. `ddi-desktop` stages the dependency checkout's assets
beside the executable, so a Git build needs no sibling game checkout.

When changing a shared host API, test SkinCraft with local patches before
pushing DDI, then update SkinCraft's DDI revision and test without patches.

The shared demo exercises holds and rolls in every lane on layouts other
than four lanes, including center/onigiri, so authoring can inspect long-note
artwork alongside the arrows. The four-lane demonstration keeps its original
phrase.

## SVG images (2026-10-24)

Both shells draw a skin's SVGs with resvg (`docs/plans/noteskin-svg.md`),
so what SkinCraft's preview shows is what every browser shows. Shadows and
glows can be filters instead of stacked contours: `feGaussianBlur`,
`feDropShadow`, `feOffset`, `feFlood`, `feComposite`, `feMerge`, `feBlend`,
`feColorMatrix`, `feComponentTransfer` and `feMorphology`, at most eight
primitives a filter. They are drawn once, at load, into the texture, not
animated per frame; a filter's region is its cost, so keep it to the
shape's box plus the blur (`x`/`y`/`width`/`height` on the `<filter>`).
`ddi_noteskin::report` lists each image's decoded size, hash and budget
share, and why one was refused. Images are drawn for the screen (up to
four times their size, less on most screens); a sheet that need not be
that fine says so on its root, `data-raster-scale="2"` (0.5 to 4), and
keeps its `width`/`height` as the size its actors see.

## Roll tap cues from judging (2026-10-22)

A modern skin may set `[NoteDisplay] RollAnimationTimingWindowRatio=0.75`.
This opt-in runs a roll’s body, top-cap and bottom-cap sprite cycle once per
75% of the active ruleset’s **scaled roll window**, in song seconds. All
lanes and those parts share the same phase. The play snapshot carries the
window directly from `JudgeTable::roll_window_seconds()`, so play and
SkinCraft’s actual demo follow the same rule, including timing scale/add.
BPM changes and stops do not change the cadence. The ratio leaves time
between the suggested tap and the actual drop deadline; it is a visual cue,
not an input requirement and does not alter judging.

The metric accepts finite fractions `(0, 1]`; missing or invalid values keep
the existing hold-part animation metrics. Holds, heads, taps and legacy
skins retain their current animation. A synthetic snapshot with no positive
roll window also keeps the existing animation. No Lua update callback is
needed. The Peters raster roll study sets the ratio to `0.75`: ITG cycles in
262.5 ms, SM5 in 375 ms, DDR A in 187.5 ms at their default timing settings.

## Layered mine electricity (2026-10-22)

`[NoteDisplay] MineConnectRows=1` opts a modern skin into position-based
mine links. `Tap Mine.lua` remains an ActorFrame containing the normal
mine core and its complete local effect. A direct child named `MineLink`
is reserved for connections and omitted from the individual mine drawing.
Without the metric, names have no special meaning. The root should have
neutral base orientation/scale; pre-rotate the core's own child as needed.

The link is authored horizontally across 64 logical pixels, centered on
the actor origin. The renderer stretches its length between neighboring
visible mines' drawn centers, preserving its perpendicular scale. It draws
links behind the mine actors, then draws each mine's ordinary local layers.
All children keep their normal actor animation clock; no per-frame Lua
lookup of other notes is needed. Temporary visibility is restored after
both passes, so commands and later frames keep their actor state.

Neighbors are sorted by actual field x within the same chart beat and
visible y row (tolerance: one eighth of a note). Lane names and chart lane
indices do not determine adjacency. Connections span at most one lane width,
so empty lanes break the electricity even when the remaining mines share a row.
Hidden mines are excluded, different
chart rows never connect, and a lone mine draws only its complete local
effect. This is presentation only: mine hits, shocks and chart data are
unchanged. Single, reordered, reversed and separated visible rows are
covered by renderer tests.

Wider demo phrases retain their taps/holds/rolls and add isolated, spaced
pair and full-row mines afterward. The four-lane phrase is unchanged.

## Beat/Vivid phase for layered notes (2026-10-24)

`TapNoteAnimationOnChildren=1` opts the part into applying its ordinary
`AnimationLength`, `AnimationIsVivid` and `AnimationIsBeatBased` phase to
descendant sprites and models as well as the root. The same suffix works for
`TapLift`, `TapFake`, `HoldHead` and the other note parts. Each sprite maps the
common fraction through its own state durations: interleaved color sheets
and a sixteen-frame charge sheet therefore stay synchronized, even inside
alignment ActorFrames. No per-frame Lua callback is required.

The default is off; existing skins and legacy skins keep their child clocks.
A layered SVG skin may opt in for taps, lifts, fakes and heads, leaving mine
effects, receptor blink/press animations, hit effects and the timing-window
roll body on their existing clocks. A four-beat color loop uses the host's
ordinary Vivid subdivision offset; a repeated charge sequence still completes
once per beat. BPM changes, stops, negative lead-in beats and seeking use the
musical position rather than elapsed sprite playback.

A skin falling back on Peters-DDR overrides the inherited `[Center]` animation
length, Vivid flags and texture-coordinate offsets for taps/lifts/fakes: Peters'
center uses a horizontal quantized-color sheet, valid there but not for
layered sheets. Peters' own metrics are unchanged.

## Continuous masked hold material (2026-10-24)

`[NoteDisplay] HoldStencilMaterial=1` opts a skin's holds into a continuous
material pass using the existing stencil and per-corner color renderer. Rolls
and skins without this opt-in retain their existing rendering. It is
presentation only. Body and bottom-cap sprites declare three states: color
alpha mask, solid exterior envelope (enclosed holes filled), then black exterior
shadow. Top-cap sprites declare two masks at the head: clear the head interior
and outside contour from the color mask; clear only the outside contour from
the exterior envelope. Heads use ordinary actors: `MaskDest` for the exterior
shadow, `MaskSource` for the band, and normal sprites for charge regions.
The renderer phases descendant head sprites from the hold's beat clock.

The body repeats from the tail, with its frame height overridden by
`HoldMaterialPitch` (4–64 logical pixels), meeting a 96 px bottom cap whose
center is the tail. Head and cap masks are 96 logical pixels. The head can
clear the repeats behind its transparent charge areas. A separate envelope
pass excludes all interior shadow; open terminal notches remain exterior.
Reverse mirrors the authored hold along its length. Hidden/Sudden spans and
dropped-hold brightness remain in effect.

The common material is sampled relative to the head, across all parts, with
4 px cells clipped to the visible field. `HoldMaterialPeriod` (16–2048),
`HoldMaterialAngle` (degrees), `HoldMaterialPhase` (cycles), and
`HoldMaterialSpeed` (cycles per song second) set the base field.
`HoldMaterialWave`, `HoldMaterialWaveWidth` and `HoldMaterialWaveLength` bend it
with one spatial sine wave; zero amplitude keeps a straight gradient. Three
hex colors (`HoldMaterialGreen`, `HoldMaterialLime`, `HoldMaterialHighlight`)
form the looping green → lime → highlight → lime → green palette. Numeric
inputs are bounded and nonfinite values use defaults. No gameplay clock or
judging behavior changes.


#### Continuous roll materials

`RollStencilMaterial=1` uses the same bounded material fields with a `Roll`
prefix (`RollMaterialGreen/Lime/Highlight` are the three palette slots).
Roll body and bottomcap sprites contain five states: colored mask, exterior
envelope, exterior shadow, overlap shadow, and repeated charge mask. Topcap
contains four states: head knockout, outside-head knockout, head-area overlap
shadow, and head-area repeated charge mask. The latter two contain only the
visible repeated body beneath the head; the head retains its regular beat
charge. Overlap shadows are drawn over the continuous colored field through
its stencil; body charge is a separate transparent white layer.

`RollPulseDuty` (0.01–1), `RollPulseRest` and `RollPulseBrightness` (0–1)
shape a quick pulse with power-0.65 decay, matching the study. Its phase uses
song seconds divided by the current scaled roll window times
`RollAnimationTimingWindowRatio`; ruleset/song window changes apply every
frame. There is no noteskin judging change. Two alternating sideways offsets
are baked into a 25 px tile for the selected 12.5 px repeat spacing. Reverse
and visibility clipping follow the hold material path, including the roll's
full tail extent.

For masked holds, short head/cap rectangles overlap geometrically. Mask
passes retain the full cap; exterior overlays crop it at head +48 px with
adjusted UVs. Rolls instead keep all repeated layers tail-anchored over their
body/cap geometry, cropped at the head plane with UVs preserved. Their topcap slots 0/1 carry the protected head
envelope; slots 2/3 are retained for compatibility but never drawn. Only a
standalone head silhouette casts a head-anchored exterior/overlap shadow. Body
shadows are clipped against the combined exterior envelope and colored-body
stencil respectively, with the head removed; repeated charge is clipped outside
the protected head envelope. No fixed head-area crop of the body is drawn.
A regression compares band, envelope, exterior shadow, overlap shadow and pulse
positions/UV phases as the roll shrinks, for lengths 32, 64, 96 and 173 px in
both scroll directions, including the full cap extent.

The masked material path reads the topcap actor's Y placement as the repeat
start relative to the head, bounded to ±48 logical pixels. All body/cap layers
are cropped there with their UVs preserved. This lets a factory declare a
shape-specific tangent height after resolving parameters and aliases. The
current masks use protected head envelopes in topcap states 0/1 for both holds
and rolls; head exterior shadows come only from the standalone silhouette.
The repeat-start regression covers negative, zero and positive crop heights.

### Merged mine outlines

With `MineConnectRows`, a `Tap Mine` ActorFrame may include direct children
`MineRimLeft` and `MineRimRight` alongside `MineLink`. Only the left/right arcs
facing a visible connected neighbor are suppressed; an isolated mine retains
both halves. Physical horizontal positions determine the sides, independently
of lane indices, layout or reverse. Suppression is temporary for each note and
restored after drawing, including when a neighbor disappears or changes row.
Existing `LocalElectricity` actors retain their previous behavior. A bridge can
therefore draw the top and bottom of a combined contour while local half-arcs
form its end caps. No judging changes are involved.

Split-rim mines render in two passes: all note bodies first, followed by an
electrical overlay containing both local half-arcs and the bridges. The
renderer keeps legacy unsplit mine ordering unchanged. This preserves visible
electricity even when the chosen radius/neck passes inside a neighboring body.
A regression uses distinct body/rim/link tints to assert the complete overlay
order, alongside visibility restoration and physical adjacency tests.

### Leading lift material

`LiftStencilMaterial=1` opts a lift ActorFrame into a leading, distance-faded
trail. Direct children `LiftTrail` (band and combined exterior shadow states),
`LiftBand` (head band), `LiftCharge` (animated charge stencil) and `LiftColors`
(ordinary color actors) supply the artwork. `LiftTrail`'s Y placement records
the head origin within the exported texture. The regular lift path remains
unchanged without the opt-in or when a required layer is missing.

`LiftPreparationSeconds` (0.05–2) sizes the trail from the current local field
speed, with a quarter-beat minimum and a 256 logical pixel cap. `Frame.note_motion`
carries local arrow heights per song second and seconds per beat; the probe uses
the same scroll action, speed ratio and scroll segments as notes. This is visual
preparation, never a hold or a change to lift judging. Reverse mirrors the
leading artwork along the field. `LiftTrailFade` (0.1–8), `LiftTrailOpacity`
(0–1) and `LiftBandWhite` (0–1) control the fade and head overlay. The continuous
green material uses the existing `HoldMaterial*` fields so head and trail match.
Charge masks and color children share the lift's Vivid clock.

The authoring demo includes lifts in four-lane and wider layouts.
