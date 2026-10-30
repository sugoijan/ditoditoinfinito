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

## Spiral component port (2026-10-25)

Infinito now uses `Def.Component`, `Def.LongNote` and `Def.Lift` for its
ordinary notes, receptors and long notes. Named SVG roles replace numbered
mask states, and skin-owned WGSL paints the band/trail continuously. The roll
pulse binds to the active roll-window clock with a 0.75 cycle. The
0.65-power pulse uses the curve's power easing directly. Receptors zoom as
one component about their aligned pivot, driven by the lane's hit clock;
long-note bodies need no hidden legacy head/cap placeholders. The accepted
profiles and masks live in `assets/noteskins/dance/Infinito/`;
its metrics declare all ten supported buttons through `[Global] Buttons`,
without named forwarding files for the taps and receptors.
Judgment effects use independent component instances on the hit clock.
Mines keep actors for adjacent-lane links. See [the authoring reference](noteskin-scene.md#authoring-reference-2026-10-25-stable-enough-to-port-changes-noted-here).

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

## Long notes and lifts in a skin's material (2026-10-30)

Holds, rolls and lifts painted by a skin's own WGSL are components of its
SVG (`docs/plans/noteskin-scene.md`, steps 3 and 4). The earlier metrics
for them (`HoldStencilMaterial`, `RollStencilMaterial`,
`LiftStencilMaterial` and their `*Material*`, `RollPulse*` and `Lift*`
values, drawn by a wave built into the engine) were removed on
2026-10-30: Infinito, their only user, already drew its long notes this
way (`docs/plans/infinito-wave-material.md`).

A hold's or roll's body element returns `Def.LongNote { File, Head, Body,
Cap, Pitch, Join, Anchor, Parts, Material }`: `Head` (drawn at the note),
`Body` (one tile `Pitch` units tall, repeated from the head's `Join` to the
cap; its `viewBox` height when `Pitch` is not given) and `Cap` (around the
tail) are components whose parts' roles (`data-role`) say what they are:
`band` (where the fill paints), `envelope` (around which the shadows fall),
`shadow` (outside the envelope), `overlap-shadow` (on the band, a roll's),
`pulse` (a roll's, outside the band), and on the head `protect` (kept out
of the body's band and shading under the head) and `outside` (the
envelope around the head); a head's parts without a role are drawn over
the rest. `Anchor` (`head`, the default, or `tail`) is where the fill's
coordinates are 0 along the note. A roll's pulse is an `Opacity` binding
on the `roll` clock (the active scaled roll window, times
`RollAnimationTimingWindowRatio`). Reverse mirrors the note along its
length; Hidden/Sudden spans and a let-go hold's greying apply.

A lift element returns `Def.Lift { File, Id, Trail, Seconds, Fade, Opacity,
Material }`: the head (`Id`) drawn as a tap's component, and the `Trail`
component (its `viewBox` from the note's center, up the lane ahead of it)
shown over the distance the note travels in `Seconds` (0.05–2, at least a
quarter of a beat, at most 256 units), faded toward its far end by `Fade`
(0–8) at `Opacity` (0–1): its `trail` part a mask the fill paints, its
`shadow` drawn under it. This is visual preparation, never a hold or a
change to lift judging.

`Material = Def.Material { File, Space, Origin, Params, Images }` paints
the band or the trail: `File` the skin's WGSL `paint` function (checked
for WebGL2 at load, `crates/render/src/material.rs`), `Space` `"lane"` or
`"note"`, up to eight `vec4` `Params` (a hex color is its RGB) read as
`m.params[i]`, and a `Phase` binding read as `m.phase`. Infinito's files
are the example: `_spiral hold.lua` (holds and rolls, which differ only in
their parameters), `_spiral lift.lua` and `_spiral wave.wgsl`.

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

The authoring demo includes lifts in four-lane and wider layouts.
