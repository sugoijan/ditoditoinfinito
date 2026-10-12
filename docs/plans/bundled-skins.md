# Implementation plan: bundled noteskins and the built-in arrows in Lua (2026-10-12)

Ship a few freely licensed StepMania noteskins with the game, and re-create
the built-in arrows as a StepMania skin of our own whose Lua runs in every
dance play, so the Lua path (`docs/plans/lua.md`) is exercised all the
time rather than only with imported skins. Research: `docs/research/noteskins.md`
§10 (which skins carry an explicit licence on their art); the candidates
were fetched and run through `local_skins` on 2026-10-12 (below).

## What the candidates are

| Skin | Licence | Upstream | Loads today |
|---|---|---|---|
| Delta (shakesoda) | CC BY 3.0 (`readme.txt`) | stepmania/stepmania `5_1-new` @ `825467bc`, `NoteSkins/dance/Delta` | 156 of 156 elements through its Lua, no warnings, at most 21.8k fuel a frame; 64 px art |
| Alpha-Arrows (Aozora Tori) | Apache-2.0 (`[Readme].txt`, `LICENSE`) | Tiny-Foxes/Aozora-Tori-Assets @ `499ee273` | 156 of 156, no warnings, at most 27.8k fuel; 512 px art. Its readme says its Lua is "based from" Lirodon's OutFox skins (no licence) and its explosion Lua is Jose Valera's: the images and metrics are the author's, the Lua is not cleanly licensed |
| Dake (hanubeki) | Apache-2.0 (`README.md`, `LICENSE`) | hanubeki/dake @ `e26aa8d0` | every element fails: it calls OutFox's `INPUTFILTER`, `GAMESTATE:GetIsFieldReversed`, `THEME:GetMetric("NoteField", …)` and `_fallback`'s `JudgmentLineToColor`; with those four stubbed, 156 of 156 and at most 20.5k fuel |
| CF_CHROME, CF_VIBRANT, CF_ALLOY (Peter Lawrence) | Unlicense (CF_CHROME's mine explosion CC0, JangaFX) | Pete-Lawrence/Peters-Noteskins @ `6faea8e1` | taps, heads and mines are MilkShape models, drawn built in until models are |

The built-in arrows (`render/src/scene.rs`, `shader.wgsl`,
`note_colors.rs`) are procedural: an SDF arrow with a darker rim, an
outline receptor that flashes the judgement's colour for 0.25 s and
shrinks while pressed, hold and roll bodies in fixed green and grey,
a mine whose spikes turn with the song, five colour schemes (Vivid by
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
  default for dance layouts. The procedural arrows stay: they are what a
  skin's missing elements fall back on, what is drawn when a skin's Lua
  stops, every non-dance layout's look, and a choice under Options
  ("built in, no Lua") for slow devices.
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

**Done 2026-10-12.** `noteskin/src/bundled.rs` (the index type),
`xtask gen-noteskins` (`xtask/src/noteskins.rs`: the provenance record
with unknown keys refused, `default`/`common` names refused, every skin
loaded against the others; tested), a Noteskins section in the credits,
Trunk's hook and `index.html`'s copy rules, the CI freshness step. In the
app (`app/src/noteskins.rs`) the catalogue is imported records then the
bundled index (fetched once; a bundled skin plays when storage does not
open, and Options still lists it),
the chosen skin's folder put first so a shadowing import never plays in
its place, bundled images fetched by URL, Options listing "Delta (with
the game)" first and without a remove button. `noteskin/tests/bundled.rs`
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
  is an image wherever a bitmap is (`noteskin/src/svg.rs`). Only a
  static subset is read, the one browsers and `resvg` draw alike: shapes,
  paths, groups, `use`, gradients, masks and clip paths; no text, images,
  filters, styles, scripts, animation, entities or references outside the
  file; the root's `width` and `height` are the size the skin sees. The
  app checks it and has the browser draw it (an image element, which runs
  no script and loads nothing) on a canvas at four times that size, so a
  64-pixel lane stays sharp at the 160-pixel arrow cap.
- **Art from the shaders' shapes.** `xtask gen-dito` writes the SVGs from
  `sd_arrow`'s skeleton (two round-capped strokes: the darker rim, a
  quarter of the colour, and the core), the receptor's ring (a mask), the
  mine's spiked outline and core, the bodies and their rounded ends, and
  takes every colour from `ddi_render::note_colors`. The files are
  committed (text, MIT) and CI checks they are fresh (`gen-dito --check`).
- **A skin per colour scheme, sharing a base** by `FallbackNoteSkin`, the
  way StepMania's own skins share one: `Dito` (receptor, holds, rolls,
  mine, lift, fake, explosions; Quantized taps, `Denominator` over nine
  rows), `Dito Vivid` (32 frames over four beats, each with its
  tail-to-tip ramp, vivid), `Dito Flat` (Vivid's frames, not vivid),
  `Dito Rainbow` (`ProgressAlternate` over the four bands, 16 frames of
  the flow a beat), `Dito Single`. Each scheme has a tap, a lift (the
  tap with its dot) and a fake sheet (the scheme's colour, flat, at
  60 %): the engine sets a note's colour and animation frame on the
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
  colour bursts from the receptor as a glow and the arrow's outline,
  larger past 100 combo (`ColumnJudgment`'s `Bright`); a completed hold
  bursts green; a held hold or roll glows green, pulsing on the beat (a
  roll twice as fast); a mine flashes red.
- **The default**: an unset `noteskin` setting is Dito, in the variant of
  the note colours; `builtin` draws the procedural arrows without a skin
  (Options: "built in, without a skin (lighter)"); a chart drawn in its
  own colours keeps the procedural arrows instead of the default (a
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
way; an imported `default` or `common` changing Dito; chart colours
dropping a chosen skin; Dito's Lua stops counted per variant. Checked: `noteskin/tests/bundled.rs`
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

Its images and `metrics.ini` byte-identical; `NoteSkin.lua` and the
explosion Lua written anew (what to load per button and element, the
rotations, the explosion's commands), checked against the original's
output in `local_skins` (same elements, same textures and frames)
without copying it. The 512 px sheets are decoded at their size (the
4096 px cap holds).

## Step 4. Dake (commit 4)

OutFox's `INPUTFILTER:GameButtonToKeyMapped`, `GAMESTATE:GetIsFieldReversed`,
`THEME:GetMetric("NoteField", "NoteFieldType" | "ShowBoard")` and the
`_fallback` function `JudgmentLineToColor` in the host (with tests), then
Dake's dance files byte-identical; the other games' files and the sounds
listed as excluded.

## Step 5. Documentation

`docs/PLAN.md` (a decision for bundled skins and their licences, decision
36 revised, the script-safety decision from `docs/plans/lua.md` step 5),
`AGENTS.md`, the verify skill, the credits.

## Later

- Peter Lawrence's CF skins once MilkShape models draw.
- Explosions for Dito, if wanted (the procedural arrows have none).
