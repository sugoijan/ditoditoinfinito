# Implementation plan: bundled noteskins and the built-in arrows in Lua (2026-10-12)

Ship a few freely licensed StepMania noteskins with the game, and re-create
the built-in arrows as a StepMania skin of our own whose Lua runs in every
dance play, so the Lua path (`docs/plans/lua.md`) is exercised all the
time rather than only with imported skins. Research: `docs/research/noteskins.md`
§10 (which skins carry an explicit licence on their art); the candidates
were fetched and run through `local_skins` on 2026-10-12 (below).

## What the candidates are

Added 2026-10-20: `DanOni`, a derived MIT proof of danoniplus CW Edition's
default 5/7-key appearance, with its original SVG masks archived and the
JavaScript/CSS behavior re-created in Lua. The reproducible conversion,
source pin and behavior mapping are recorded in
[`tools/noteskin-danoni/README.md`](../../tools/noteskin-danoni/README.md).
Dropped-freeze colors and manual/autoplay flashes now use opt-in dropped
actors and per-hit presentation metadata, shared by desktop, web and SkinCraft.

| Skin | Licence | Upstream | Loads today |
|---|---|---|---|
| Delta (shakesoda) | CC BY 3.0 (`readme.txt`) | stepmania/stepmania `5_1-new` @ `825467bc`, `NoteSkins/dance/Delta` | 156 of 156 elements through its Lua, no warnings, at most 21.8k fuel a frame; 64 px art |
| Alpha-Arrows (Aozora Tori) | Apache-2.0 (`[Readme].txt`, `LICENSE`) | Tiny-Foxes/Aozora-Tori-Assets @ `499ee273` | 156 of 156, no warnings, at most 27.8k fuel; 512 px art. Its readme says its Lua is "based from" Lirodon's OutFox skins (no licence) and its explosion Lua is Jose Valera's: the images and metrics are the author's, the Lua is not cleanly licensed |
| Dake (hanubeki) | Apache-2.0 (`README.md`, `LICENSE`) | hanubeki/dake @ `e26aa8d0` | every element fails: it calls OutFox's `INPUTFILTER`, `GAMESTATE:GetIsFieldReversed`, `THEME:GetMetric("NoteField", …)` and `_fallback`'s `JudgmentLineToColor`; with those four stubbed, 156 of 156 and at most 20.5k fuel |
| CF_CHROME, CF_VIBRANT, CF_ALLOY (Peter Lawrence) | Unlicense (CF_CHROME's mine explosion CC0, JangaFX) | Pete-Lawrence/Peters-Noteskins @ `6faea8e1` | bundled since 2026-10-12: taps, lifts and mines are MilkShape models (`docs/plans/models.md`, step 5) |
| (2026-10-13) CF_CHROME, CF_VIBRANT, CF_ALLOY's `Center` files | Unlicense | the same | restored byte for byte so the family draws Dancing☆Onigiri's onigiri as its own (`docs/plans/danoni-skins.md`); CF_ALLOY's `Center Tap Lift.lua` left out, CF_CHROME's |
| Peters-DDR (Peter Lawrence) | Unlicense | Pete-Lawrence/Peters-Noteskins @ `6faea8e1` | bundled since 2026-10-13: Vivid's taps a model whose colors cycle in vivid; originally falling back on Note (models for taps and lifts, Lua for the rest, 6.2 MB, most of it four mine sheets). Both load all 156 elements as their own; full autoplays all Fantastic on WebGPU, WebGL2 and in Firefox. Combined as Peters-DDR on 2026-10-22 (below) |
| (2026-10-22) Pump's center for every skin: Alpha-Arrows' and Dake's `_center` images; Delta's made from its art; Peters-DDR's the SkinCraft raster center (below) | Apache-2.0; CC BY 3.0 (adapted); the Unlicense with our MIT changes | the same | each skin draws pump's center panel as its own (and the onigiri with it, but for DanOni and Dito, which draw their own) (`docs/plans/pump.md`, "Centers for the bundled skins") |

The built-in arrows (`crates/render/src/scene.rs`, `shader.wgsl`,
`note_colors.rs`) are procedural: an SDF arrow with a darker rim, an
outline receptor that flashes the judgement's color for 0.25 s and
shrinks while pressed, hold and roll bodies in fixed green and grey,
a mine whose spikes turn with the song, five color schemes (Vivid by
default: a hue moving smoothly round 16 `midi-vivid` hues over four
beats, a ramp from tail to tip; Rainbow's gradient runs down the screen,
not along the arrow). They draw no explosions.

## Decisions

- **Licences for bundled art**: CC BY, CC BY-SA, Apache-2.0, Unlicense and
  CC0, each declared in the upstream files (the maintainer accepted
  Apache-2.0 and Unlicense art on 2026-10-12). Every bundled skin keeps
  its upstream files byte-identical with a `PROVENANCE.toml`, as songs
  do; Apache-2.0's full text ships in `LICENSES/` and the credits.
- **Code we may not take** (Alpha-Arrows' Lua) is replaced by our own
  Lua written from what the skin needs, MIT, in the bundled folder next to
  the upstream art and listed as a modification.
- **The built-in arrows in Lua** are a bundled skin, `Dito`, and the
  default for dance layouts (and, since 2026-10-13, Dancing☆Onigiri's,
  with a star for the onigiri: `docs/plans/danoni-skins.md`). The
  procedural arrows stay: they are what a skin's missing elements fall
  back on, what is drawn when a skin's Lua stops, the look when a chart's
  own colors are drawn, and a choice under Options ("built in, no Lua")
  for slow devices.
- **Bundled skins are read-only; each kind falls back on its own kind
  first**: an imported skin's chain looks in imported skins before bundled
  ones, so an import named like a bundled skin stands in for it there
  (StepMania's own folder wins too); a bundled skin's looks in bundled
  ones only (since step 2, so no import, not even an imported `default`
  or `common`, changes the default skin). The setting keys bundled skins as `bundled:<game>/<name>`,
  imported ones as before.

## Step 1. Bundled skins and Delta (commit 1)

- `assets/noteskins/<game>/<Name>/` holds a skin's files and a
  `PROVENANCE.toml` (the songs' schema: `excluded`, `modifications`,
  `[source]`, `[[licenses]]` per part). `xtask gen-noteskins` validates
  them and writes `target/noteskins/index.json` (game, name, file list,
  the texts of `metrics.ini`, `.redir` and `.lua`), run by Trunk before
  every build like `gen-songs`; `index.html` copies the folder and the
  index. `gen-credits` lists the skins; `REUSE.toml` annotates each folder.
- The app's skin catalogue merges the bundled index (fetched once, kept in
  memory; no IndexedDB wait, as for bundled songs) with the imported
  records. `decode` fetches a bundled skin's images over HTTP; `prepare`,
  `choices`, `summary` and the import report see both. Bundled skins
  cannot be removed; a stop of their Lua counts for the session only.
- Delta, byte-identical. Options lists it.
- Tests: the generator natively (schema, missing files, unknown keys);
  `local_skins` on `assets/noteskins`; headless Chrome (WebGPU, WebGL2)
  and Firefox: Delta from a fresh profile with no imports, with an import
  of the same name, and after a Lua stop.

**Done 2026-10-12.** `crates/noteskin/src/bundled.rs` (the index type),
`xtask gen-noteskins` (`xtask/src/noteskins.rs`: the provenance record
with unknown keys refused, `default`/`common` names refused, every skin
loaded against the others; tested), a Noteskins section in the credits,
Trunk's hook and `index.html`'s copy rules, the CI freshness step. In the
app (`crates/web/src/noteskins.rs`) the catalogue is imported records then the
bundled index (fetched once; a bundled skin plays when storage does not
open, and Options still lists it),
the chosen skin's folder put first so a shadowing import never plays in
its place, bundled images fetched by URL, Options listing "Delta (with
the game)" first and without a remove button. `crates/noteskin/tests/bundled.rs`
builds every bundled skin through its Lua in `just test` (156 elements,
no warning, taps and receptors its own). Checked: headless Chrome (WebGPU
and WebGL2) and the installed Firefox (WebGL2, through the verify skill's
`firefox.mjs`): Options lists Delta, a play draws it through its Lua with
no problems, a full autoplay all 329 Fantastic and 27 of 27 holds
(Firefox), the Credits screen lists it; an import named Delta is listed
next to it and each plays as itself (screenshots). Not checked at
runtime: a bundled skin's Lua stopping (no bundled skin stops; the path
is the imported one's with the strike kept in memory).

## Step 2. `Dito`: the built-in arrows as a skin (commit 2)

- **SVG in skins** (our extension; StepMania reads bitmaps): an `.svg`
  is an image wherever a bitmap is (`crates/noteskin/src/svg.rs`). Only a
  static subset is read, the one browsers and `resvg` draw alike: shapes,
  paths, groups, `use`, gradients, masks and clip paths; no text, images,
  filters, styles, scripts, animation, entities or references outside the
  file; the root's `width` and `height` are the size the skin sees. The
  app checks it and has the browser draw it (an image element, which runs
  no script and loads nothing) on a canvas at four times that size, so a
  64-pixel lane stays sharp at the 160-pixel arrow cap.
- **Art from the shaders' shapes.** `xtask gen-dito` writes the SVGs from
  `sd_arrow`'s skeleton (two round-capped strokes: the darker rim, a
  quarter of the color, and the core), the receptor's ring (a mask), the
  mine's spiked outline and core, the bodies and their rounded ends, and
  takes every color from `ddi_render::note_colors`. The files are
  committed (text, MIT) and CI checks they are fresh (`gen-dito --check`).
- **A skin per color scheme, sharing a base** by `FallbackNoteSkin`, the
  way StepMania's own skins share one: `Dito` (receptor, holds, rolls,
  mine, lift, fake, explosions; Quantized taps, `Denominator` over nine
  rows), `Dito Vivid` (32 frames over four beats, each with its
  tail-to-tip ramp, vivid), `Dito Flat` (Vivid's frames, not vivid),
  `Dito Rainbow` (`ProgressAlternate` over the four bands, 16 frames of
  the flow a beat), `Dito Single`. Each scheme has a tap, a lift (the
  tap with its dot) and a fake sheet (the scheme's color, flat, at
  60 %): the engine sets a note's color and animation frame on the
  note's own sprite, as StepMania does, so a lift or a fake cannot be Lua
  around a tap. One Lua file draws all three (and the heads), by the
  element asked for; a metric (`DitoTapFrames`) gives the frames. Variants
  are marked `variant_of` and not listed on their own; a Lua stop counts
  against Dito whichever variant played.
- **Behaviour in Lua**: the receptor's `Press`/`Lift` and its flash on
  `W1`…`W5` (not `Miss`) fading in 0.25 s; the mine turned anticlockwise
  by an update function from the song's seconds; the explosion turned
  with its button (`NoteSkin.lua`).
- **Explosions** (the built-in arrows have none): a hit's judgement
  color bursts from the receptor as a glow and the arrow's outline,
  larger past 100 combo (`ColumnJudgment`'s `Bright`); a completed hold
  bursts green; a held hold or roll glows green, pulsing on the beat (a
  roll twice as fast); a mine flashes red.
- **The default**: an unset `noteskin` setting is Dito, in the variant of
  the note colors; `builtin` draws the procedural arrows without a skin
  (Options: "built in, without a skin (lighter)"); a chart drawn in its
  own colors keeps the procedural arrows instead of the default (a
  chosen skin stays). A bundled skin falls back only on bundled skins.
  Decision 36 is revised.

**Done 2026-10-12.** What differs from the procedural arrows: Vivid and
Flat step through 32 frames (the hue is continuous there); Rainbow's
flow follows the beat only (the shader's also runs down the screen), and
`ProgressAlternate` puts notes between 16ths one band later (as
StepMania does); a hold's end is a half-disc rather than the shader's
stretched corners; a dropped hold dims to half (`HoldLetGoGrayPercent`)
rather than turning grey; a lift judged on release does not flash the
receptor (StepMania plays judgements on it for presses only);
explosions are new. Reviewed independently (two reviewers): fixed an SVG
checker that a processing instruction, whitespace before `=`, character
references or a `filter` attribute got past, and that let nested `use`
freeze the page (now a strict XML parser that bounds the elements drawn,
references expanded); fakes drawn opaque and lifts off the beat (Lua
around a sprite: now sheets of their own); the mine turning the wrong
way; an imported `default` or `common` changing Dito; chart colors
dropping a chosen skin; Dito's Lua stops counted per variant. Checked: `crates/noteskin/tests/bundled.rs`
(all 156 elements of every Dito variant its own, no warning), the SVG
subset's tests and every generated file against it; `local_skins` on
`assets/noteskins`: Dito loads in 8 ms and 277,000 fuel and plays at a
mean of 2,600 fuel a frame, at most 26,500 (of 200,000; Delta 21,800);
in headless Chrome every scheme next to the procedural arrows by
screenshot, explosions frame by frame, a synthetic chart (mines, lifts,
fakes, holds, rolls, jumps) under reverse, Hidden and Sudden, and the
three Arrows choices through Options; a full autoplay all 329 Fantastic
and 27 of 27 holds on WebGPU and WebGL2 and in the installed Firefox
(WebGL2), which draws the SVGs alike. In the browser a frame's Lua
measured at most 11,300 fuel on the synthetic chart. Not checked: a
phone (the maintainer), WebGPU in Firefox (headless has none), Safari.

## Step 3. Alpha-Arrows (commit 3)

Its images byte-identical; `NoteSkin.lua`, the explosion Lua and
`metrics.ini` written anew (what to load per button and element, the
rotations, the explosion's commands), checked against the original's
output in `local_skins` (same elements, same textures and frames)
without copying it. The 512 px sheets are decoded at their size (the
4096 px cap holds).

**Done 2026-10-12.** Bundled as `Alpha-Arrows`: its dance images, readme
and the repository's `LICENSE` (Apache-2.0) byte for byte; the `Center`
art (other games; bundled since 2026-10-22 for pump's center panel),
OutFox's mine-hold and lift-hold parts, and all its
Lua and `metrics.ini` left out. The metrics carry the explosions'
commands and are written in the same style as the Lua, so they went
with it: `NoteSkin.lua`, the tap, lift, mine, receptor and explosion Lua
and `metrics.ini` are ours (MIT), written for the images after reading
what the original does (Down's art turned per button, heads and fakes as
taps, top caps only for the diagonals, the receptor's beat pulse and
press flash, the lift's beat thump, the mine's spin, a turned glow and
upright rays for the best judgements, a pulsing hold glow); the receptor
and the explosion follow Dito's (a judgement's color flashing the
receptor's flash art, no beat pulse; the explosion's swell and fade),
with Dito's judgement colors. Reviewed independently: the first
receptor followed StepMania default's receptor commands with the numbers
changed, and the explosion's timing echoed the original's metrics; both
were rewritten.
A probe built both on our engine: every element draws the same images
at the same rotations, except the cardinal arrows' hold top caps (the
original's fall through to StepMania's `common`; ours are blank, the hold
starting under its head) and the explosion. Checked:
`crates/noteskin/tests/bundled.rs` (all 156 elements its own, no warning);
`local_skins`: loads in 7.6 ms, a frame's Lua at a mean of 700 fuel and
at most 24,600; headless Chrome on the synthetic chart (mines, lifts,
holds, rolls, explosions, reverse) by screenshot; a full autoplay all
329 Fantastic and 27 of 27 holds in Chrome (WebGPU) and the installed
Firefox (WebGL2); the Credits screen lists it with the licence, what is
left out and what was rewritten.

## Step 4. Dake (commit 4)

OutFox's `INPUTFILTER:GameButtonToKeyMapped`, `GAMESTATE:GetIsFieldReversed`,
`THEME:GetMetric("NoteField", "NoteFieldType" | "ShowBoard")` and the
`_fallback` function `JudgmentLineToColor` in the host (with tests), then
Dake's dance files byte-identical; the other games' files and the sounds
listed as excluded.

**Done 2026-10-12.** The host (`crates/noteskin/src/lua/prelude.lua`, with a
test) gives `Var "GameButton"` (the button: a style's buttons are the
game's), `INPUTFILTER:GameButtonToKeyMapped` (`""`: the player's bindings
are not shown to skins), `GAMESTATE:GetIsFieldReversed` (the play's
reverse setting, a new `Env` field the app fills), a `THEME` whose
`GetMetric` knows NoteField's `NoteFieldType` (`""`) and `ShowBoard`
(false), and `JudgmentLineToColor` in the game's judgement colors (`Pro`
judgements as the plain ones). With them Dake's `NoteSkin.lua` (2,910
lines, for every game) loads unchanged. Bundled as `Dake`: its
`NoteSkin.lua`, `metrics.ini`, `README.md`, `LICENSE` and the 74 images
dance asks for (found by building every element of all six dance
buttons), byte for byte, 476 KB of the repository's 5.6 MB; the other
games' art, `_common buttons/`, the two sounds and `extras/` left out.
Dake colors a note by `Var "Color"`, as ITGmania and OutFox load notes
once per quantization color; our host read it as "4th" for every note,
so its taps were all red (the reviewer caught that we had blamed Dake).
Now a note element whose Lua reads `Var "Color"` (taps, lifts, fakes,
mines and every hold and roll part, as ITGmania loads per color) is
loaded once per note type, 4ths to 192nds (`LuaSkin::element_colored_sliced`
says whether it read it), and each note draws its type's copy, a hold
all its parts in its head's (`crates/render/src/noteskin.rs`, tested). ITGmania
loads every such element per color whether or not it reads it; here
only those that do, so other skins load their notes once, as before. A
script's own `NOTESKIN:LoadActor` loads the 4ths' (as ITGmania's does),
and a copy that does not load as its 4ths' did (an error, the built-in
skin) is dropped for the 4ths'. A skin may make 16,384 actors (was
4,096), for the copies. Reviewed independently (a second reviewer for
this): fixed copies drawn by nobody when only they fell back to the
built-in skin, hold parts left uncolored, the actor cap, and nested
loads inheriting the color. Checked: `crates/noteskin/tests/bundled.rs` (all 156 elements its own,
no warning; no stand-in art); `local_skins`: loads in 16.7 ms, a frame's
Lua at a mean of 600 fuel and at most 21,300; headless Chrome on the
synthetic chart with and without reverse by screenshot; a full autoplay
all 329 Fantastic and 27 of 27 holds in Chrome (WebGPU) and the
installed Firefox (WebGL2).

## Step 5. Skins blended as StepMania blends them

A skin's quads blend in linear light (decision 17), so faint pixels and
fades show far more than in StepMania, which blends in gamma space: a
pixel at 3 % comes out five times brighter on black (Delta's receptor
frames have such pixels at their edges, visible as specks that change
with the beat). Draw the skin quads through a non-sRGB view of the
target on WebGPU (the canvas's format is plain, its sRGB view is ours),
and through an offscreen pass on WebGL2 (its surface is sRGB only), with
the colors passed unconverted; decision 17 amended. Checked against
screenshots of fades, explosions and Delta's receptor before and after.

**Done on WebGPU 2026-10-12.** The renderer is given the canvas's plain
format when it draws through an sRGB view of it (WebGPU), builds the
textured pipelines a second time for it, makes skin textures with a plain
view too and samples that one there (so texels are filtered and blended
as encoded, as StepMania does, with no conversion in the shader), and
draws each run of a skin's quads in a pass of its own on the plain view
between the scene's passes (`passes`, `crates/render/src/renderer.rs`, tested);
the skin's colors are then given encoded (`NoteskinPlay::set_srgb(false)`). Everything else still blends
in linear light. `__DDI_DEBUG.skins_encoded` says which way a play draws.
Checked in headless Chrome: on WebGPU skins blend encoded, on WebGL2 as
before; Delta's receptor specks fainter (the brightest from about 46 to
27 of 255 on the dark field; re-encoding sampled texels instead, as a
first version did, gave 34: filtering in linear light lightens fringes),
Dito's colors unchanged and its additive
glow softer, as in StepMania; full autoplays all top tier on WebGPU and
in Firefox, no validation warnings. Reviewed independently: the
filtering (above) and an assertion that the plain view is given whenever
skins' colors are made for it. Open: WebGL2, whose surface is sRGB only (no other view of
it): skins still blend in linear light there.

**Done on WebGL2 2026-10-13.** The renderer takes how skins blend
(`SkinBlending`: the scene's way, through a plain view, or offscreen),
and on WebGL2 draws every frame offscreen (`crates/render/src/offscreen.rs`):
the scene's pipelines draw linear light into a half-float texture
(blending there is in linear light, as through an sRGB view), each run of
a skin's quads goes to a plain 8-bit texture the scene so far is encoded
into first and decoded back from after, and the frame is drawn to the
surface at the end, which encodes it. Skin textures are plain there (no
second view of a texture in WebGL2). Two ways that did not work, both
for Firefox: reading an sRGB texture while drawing to a plain one (Firefox
returns other values than Chrome), and copying between the sRGB and plain
textures (Chrome refuses it, Firefox converts the values); so no sRGB
texture is read on the way. Every 8-bit value survives the round trip
through half floats (tested natively). The app chooses offscreen only
when the adapter renders and blends half floats, and not when the player
chose the built-in arrows (the lighter choice draws no skin); else skins
blend in linear light as before. The conversions cover only the box of a
run's quads (the plain texture is read nowhere else; scissored, tested).
Reviewed independently: no fault found; the box and the built-in choice
came from its notes on the cost. Checked in headless Chrome (WebGL2 and WebGPU)
and the installed Firefox with a synthetic skin whose receptor is white
at 10 %: 30, 28, 37 over the field on WebGL2 and in Firefox, 30, 27, 37
on WebGPU, StepMania's 0.1 × 255 + 0.9 × the field (the old WebGL2 drew
90); song backgrounds within one level of before on every backend;
screenshots of CF_CHROME and Dito alike on both backends; full autoplays
all 329 Fantastic and 27 of 27 holds on WebGL2, in Firefox and on WebGPU,
no console warnings. Cost on WebGL2: two textures the size of the canvas
(12 bytes a pixel), a full-screen draw at the end of every frame, and
two draws over the field's quads for each run of them (two or three runs
a frame: receptors, holds, notes, explosions between the built-in
sprites). A skin chosen but not drawn (a layout other than dance, a
chart's own colors) is known only when its session starts: then
`Renderer::set_skins_drawn` makes the scene's pipelines draw to the
canvas again (2026-10-13; checked with a Dancing☆Onigiri work, local only,
and `__DDI_DEBUG.offscreen`).

## Step 6. Documentation

`docs/PLAN.md` (a decision for bundled skins and their licences, decision
36 revised, the script-safety decision from `docs/plans/lua.md` step 5),
`AGENTS.md`, the verify skill, the credits.

**Done 2026-10-12.** Decisions 36 (revised), 37 (bundled skins), 38 (SVG)
and 39 (scripts' sandbox), `AGENTS.md`, the verify skill and the credits
were kept current at each step; this one added decision 39 and closed
`docs/plans/lua.md`.

## Later

- Settled 2026-10-12: a file whose last statement is
  `return Def.Sprite { Texture = "x.png" }` (a tail call) resolves `x.png`
  from the folder of the Lua that loaded it, not its own. StepMania does
  the same: its `Def` climbs the stack past the `=(tail call)` frame to
  the next file (`Themes/_fallback/Scripts/02 ActorDef.lua`), so skins
  write such paths through `NOTESKIN:GetPath`, as the bundled ones do.

- Done 2026-10-13: Dito's explosions (`Down Explosion.lua`: a glow and
  the note's outline in its judgement's color, the onigiri's star and
  the centre's square for theirs).
- Done 2026-10-21: Dito's color schemes, songs' own colors, its
  explosions and the built-in drawing are its parameters
  (`docs/plans/skin-parameters.md`): `Colors` loads each scheme's skin as
  before, and the "built in, without a skin" choice is Dito's `Lighter`.

## Peters-DDR consolidation (2026-10-22)

One selectable skin, **Peters-DDR**, with **Note colors** (Vivid by
default, By subdivision) and **Explosions** (on by default). This is the
starting point for a later SVG design closer to Peters' arrows; Dito
remains the game's default.

The former `Peters-DDR-Note` assets now live in `Peters-DDR`. Its original
metrics draw the subdivision colors; its `Colors` parameter chooses
`Peters-DDR-Vivid` for Vivid, which falls back on the shared skin. The
original tap-arrow models and source textures are unchanged. The shared explosion script
returns an empty actor frame when `Explosions` is off, covering taps,
holds, rolls and mines without disabling receptor feedback.

`Peters-DDR-Note` remains a small fallback alias with Quantized as its
default; it and Vivid are marked `variant_of` so Options shows only the
combined skin. Saved bundled Note/Vivid selections migrate to Peters-DDR
with the matching color choice; imported names are untouched. The aliases
also preserve old replay drawing records and external fallback chains.
Provenance lists the moves and parameter additions; notices retain Peter
Lawrence's Unlicense and identify the MIT additions.

SkinCraft's parameter-controls update exposes Colors and Explosions in its
preview panel and MCP, saves preview values per project, and retains both
variants in its file inventory and exports. The Lua guard still keeps
effects enabled in older hosts without parameter support.

The approved SkinCraft raster extension is now bundled: the square center's
matching receptor and charge regions, center/diagonal holds with connected caps,
blue-purple rolls with flashing bars and charged heads, and layered metallic
mines. Electricity connects only across adjacent lanes; isolated mines retain
complete effects. Roll bars follow the active scaled roll timing window,
independently of BPM. Both color modes and compatibility aliases share the
extension. Original arrow tap models and cardinal hold artwork remain unchanged.
See [`tools/noteskin-peters/README.md`](../../tools/noteskin-peters/README.md)
for native sources and the superseded model/SVG study. Bundled checks load the
raster charge sheets and all mine/long-note directions, including their metrics.

Verified: bundled-skin tests build every element and layout; the Peters
parameter test checks both color modes, their original textures and
explosions on/off. Settings tests cover migration, persistence and imported
names. The desktop GPU field-preview test switches both modes, and the
actual play renderer captured Beatucada at 24.25 s in Vivid, subdivision
colors and Vivid with explosions off. Index and credits regenerated;
REUSE lint passes. This consolidation has not had a new browser run.

The approved raster bundle also passes the bundled Lua checks for every layout
and all ten mine/long-note button directions. The desktop GPU field-preview
check loads both color modes in dance-single and danoni-9B. Asset index and
credits are regenerated, and REUSE lint passes.
