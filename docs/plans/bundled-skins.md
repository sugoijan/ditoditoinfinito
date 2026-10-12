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
- **Bundled skins are read-only and come first in no chain**: imported
  skins are listed before bundled ones, so an import named like a bundled
  skin shadows it in fallback chains (StepMania's own folder wins too);
  the setting keys bundled skins as `bundled:<game>/<name>`, imported ones
  as today.

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

- **Art from the shaders' shapes.** `xtask gen-dito` evaluates the same
  SDFs as `shader.wgsl` on the CPU (a small Rust port, tested against
  the shader's formulas) and writes PNGs: 256 px frames marked
  `(res 64x64)`, so they stay sharp at the 160 px lane cap. Generated at
  build time into `target/noteskins/`, like the index, so no binary is
  committed; `metrics.ini` and the Lua are committed and MIT.
  `cargo run -p xtask -- gen-dito --out <dir>` writes a complete folder
  for StepMania, to compare the two.
- **A skin per colour scheme, sharing a base** by `FallbackNoteSkin`, the
  way StepMania's own skins share one: `Dito` (receptor, holds, rolls,
  mine, lift and fake; Quantized taps, `NoteColorType=Denominator` over
  nine colours), `Dito Vivid` and `Dito Flat` (32 frames over four beats,
  each with the tail-to-tip ramp baked in; vivid on and off), `Dito
  Rainbow` (`ProgressAlternate` over the four bands, a texture per
  direction so the gradient runs down the screen), `Dito Single`. The
  "Note colours" option picks the variant while Dito is the skin; Vivid
  stays the default.
- **Behaviour in Lua**: the receptor's `Press`/`Lift` and its flash on
  `W1`…`W5` (not `Miss`) with a linear 0.25 s fade; the mine's spikes
  turned by an update function from the song's seconds; `Tap Fake` at
  60 % alpha; a lift's dot. Hold bodies use Active and Inactive art and
  `HoldLetGoGrayPercent`. No explosions, as now.
- **The default**: `noteskin` in the settings becomes `Default` (Dito
  with the chosen colours), `BuiltIn` (procedural, no Lua) or a skin key;
  a stored `None` reads as `Default`. The Arrows section is always shown.
  Decision 36 is revised.
- Checked: frame fuel and time natively (the `local_skins` play at 60
  and 144 Hz) and in the browser (`__DDI_DEBUG.noteskin.lua`); every
  scheme by screenshot next to the procedural arrows, with reverse,
  Hidden, Sudden and a jump; a full autoplay all top tier; Firefox by the
  verify skill's helper and by hand; a phone by the maintainer.

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
