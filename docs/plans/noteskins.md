# Implementation plan: noteskins (2026-10-10)

Draw imported StepMania noteskins for the `dance` layouts (single, solo,
double) as StepMania draws them, next to the procedural arrows, which stay
the default and the look of every other layout. Research and sources:
`docs/research/noteskins.md` (cited below as "research §n").

**Scope of this round: everything that does not run a skin's Lua.** Lua
(research §8; a hardened fork of piccolo behind a narrow host API,
`docs/plans/lua.md`) and MilkShape 3D models are later steps (7 and 8
below). Until then an element that a skin defines only
in Lua (`Down Tap Note.lua`) or as a model is drawn by the built-in skin, and
the import says which. In practice nearly every real skin defines its tap
note and receptor in Lua (all of StepMania 5.1's, ITGmania's and OutFox's
dance skins but `midi-vivid` and `midi-routine`), so until step 7 an imported
skin mostly contributes its holds, mines and explosions. Everything built
before then is what the Lua step plugs into: the Lua only decides which
actors exist and which commands they run.

## Step 0. Conventions

- New crate `noteskin/` (`ddi-noteskin`): pure, natively tested, no I/O, no
  wgpu. A skin is given to it as a tree of file names with the text of its
  `metrics.ini` and `.redir` files; images are the shell's business.
- `render` draws what `noteskin` resolves; the app imports, stores and loads
  images; `engine` reports the per-lane events skins react to.
- The built-in fallback replaces StepMania's `dance/default` and
  `common/common` (their art is not under an explicit licence, research §10):
  their **behaviour** (the `Load` conventions, the explosion and receptor
  structure, the metric defaults) is reimplemented natively from
  StepMania's MIT code and cited; anything a skin does not provide as an
  image is drawn procedurally, as today.
- Imported skins stay local like imported songs. Test skins committed to the
  repository are synthetic: images drawn by a script in `crates/noteskin/tests/`,
  MIT like the rest.
- Fidelity: follow StepMania 5.1 (`5_1-new` @ `825467b`), ITGmania where it
  differs, cite file and line in comments where a rule is not obvious.

## Step 1. The skin: lookup, metrics, file names (commit 1)

- `metrics.ini` with StepMania's `IniFile` rules (`src/IniFile.cpp`): `[Section]`,
  `key=value` with only the key trimmed, `;`, `#`, `//`, `--` comment lines,
  a trailing `\` continues the line, BOM removed, later keys win. Merged
  under the fallbacks (`MergeIniUnder`); `GetMetric(button, name)` looks in
  `[<Button>]`, then `[NoteDisplay]`; numbers read as `StringToFloat` does
  (leading number, the rest ignored: `-32\t// top of tail`).
- The fallback chain (research §2): `FallbackNoteSkin`, else `default` then
  `common`; each name looked up in the game's skins, then `common`; an
  imported `default` or `common` is used when present, else the built-in;
  a missing fallback fails the skin; depth limit 20.
- `GetPath`: case-insensitive prefix match of `"<Button> <Element>"` over
  the chain, then `"Fallback <Element>"`, first match by lowercase name,
  `.redir` hops (bare name, up to 100), `_`-prefixed names only by explicit
  name.
- File name hints: `" WxH"` grid, `(res WxH)`, `(doubleres)`; display frame
  size; power-of-two padding factors for texture-coordinate metrics.
- `Load` without Lua: the `dance/default` + `common/common` conventions
  natively (buttons redirected to `Down`, `Head` elements and `Tap Fake` to
  `Tap Note`, the `Blank` list, `PartsToRotate` and `Rotate`; research §7);
  a skin's own `NoteSkin.lua` is ignored and reported. The result per (button,
  element) is an `Element`: an image (path, grid, size, rotation), blank,
  needs Lua (`.lua`), needs a model (`.txt` model), or missing (built-in).
- The `[NoteDisplay]` metrics as a typed struct per button and part, with
  `common/common`'s defaults.
- Tests: synthetic skins for every rule above (prefix, case, `Fallback`,
  redirect chains and loops, duplicate matches, fallback skins, per-button
  metrics, hints). Adversarial inputs (huge ini, redirect cycles, deep
  chains, odd names) under the memory watchdog.
  `DDI_NOTESKINS=<dir> cargo test -p ddi-noteskin --test local_skins -- --nocapture`
  resolves every element of every skin under a local folder and tabulates
  images, blanks, Lua, models and missing ones.
- Review before handover (parsers of untrusted files).

**Done 2026-10-10.** What differs, and what later steps need:

- The chain follows StepMania literally, including its quirk: `common`
  names no fallback, so `default` (not loaded yet) is added after it, and
  `common` again. The `NoteSkin.lua` scripts run from the last fallback back
  and common's replaces `Redir`, `Rotate`, `PartsToRotate` and `Blank`
  outright, so the base nearest the skin decides: a skin on
  `FallbackNoteSkin=default` gets dance/default's redirects, one on
  `common` gets none. Most real skins on `common` (70 of the local
  corpus's) repeat dance/default's `Redir` in their own `NoteSkin.lua`; until
  that runs they mostly show fallback art.
- The stand-ins list StepMania's file names with what each is
  (`builtin::Virtual`: art, Lua reimplemented or drawn built in, a redirect
  with its target), so lookups land where StepMania's do. Their metrics are
  StepMania's (`builtin.rs`).
- Lookups use a per-folder index built once (sorted, a prefix run found by
  binary search), are cached, and take `&self`; paths are normalised
  (`\`, `.`, `..`) and masks keep StepMania's up to two `*`. Hints are read
  from the whole path (`/NoteSkins/<game>/<skin>/<file>`), and a texture
  under 8 px is raised to 8 and stretched, as StepMania does.
- `Resolved::NotASprite` for a hold cap or body that is not a sprite;
  folders holding `default.lua` are Lua; movies are told apart.
- The corpus test (`local_skins`) loads all 54 dance skins of StepMania 5.1,
  ITGmania and OutFox; with their own `default` and `common` 8,665 elements
  resolve to images and 1,307 to Lua; with the stand-ins 1,310 images, 7,926
  built in, 736 Lua. Every skin's explosion row and mine build, and every
  metrics command parses but 70 that call Lua functions.
- Review: the nearest-base rule, normalisation, the small-texture rule and
  linear-time parsing came from it; the hostile tests (random text and
  folders, 80k files, 1,000 redirects, a megabyte of ini keys, 64 KB names)
  run in 0.3 s under 65 MB.

## Step 2. Actors: commands, tweens, effects, animation (commit 2)

- Command strings as StepMania writes them in metrics
  (`LuaHelpers::ParseCommandList`, `src/LuaManager.cpp:898-984`):
  `name,arg,arg;name,…` with numbers, quoted strings, booleans,
  `color("r,g,b,a")` and `color("#RRGGBB")`, enum names
  (`BlendMode_Add`, `bottom`); anything else is a Lua expression and is
  reported (the Lua step evaluates it). A leading `%` is raw Lua.
- An actor model with the properties skins use (research §8): diffuse and
  alpha, glow, zoom and base zoom, x/y, rotation and base rotation, visible,
  blend (normal, add), vertical alignment; the tween queue (`linear`,
  `accelerate`, `decelerate`, `smooth`, `spring`, `sleep`, `stoptweening`,
  `finishtweening`, `hurrytweening`) and the effect clock (`diffuseramp`,
  `diffuseshift`, `diffuseblink`, `glowshift`, `glowramp`, `glowblink`,
  `pulse`, `spin`, `wag`, `bounce`, `bob`, `effectclock`, `effectperiod`,
  `effecttiming`, `effectoffset`, `effectmagnitude`, `effectcolor1/2`,
  `stopeffect`), `playcommand`/`queuecommand`, with StepMania's maths
  (`src/Actor.cpp`, `src/Tween.cpp`; cited per formula).
- Sprite states (frame grid, `FrameNNNN`/`DelayNNNN`, 0.1 s default),
  `animate`, `setstate`, `SetAllStateDelays`; the engine's beat-locked frame
  selection with the vivid phase and the quantization texture translation
  (`Denominator`, `Progress`, `ProgressAlternate`; research §4).
- Pure and natively tested: each tween and effect against values computed
  from StepMania's formulas, command parsing, frame selection.

**Done 2026-10-10.** `actor.rs` (the tween queue, effects and their
clocks, sprite states, commands, the draw list), `commands.rs`,
`notes.rs` (frame selection with vivid, color translation), `build.rs`
(elements to actors). Command arguments evaluate natively when they are
numbers, strings, booleans, `nil`, `color("…")` or enum and alignment
globals; anything else is reported as needing Lua. Duplicate commands
chain as `MergeTables` does. Methods StepMania defines in Lua (`smooth`,
`bouncebegin`, `bounceend`, `drop`, `thump`, `heartbeat`) are native.
x and y rotations are drawn edge-on (they scale the other axis).

## Step 3. The events skins react to (commit 3)

- `engine::Frame` gains each lane's events with their song times: press and
  lift edges, the judgement of every press (`None` for an empty one), the
  explosion of a row once the whole row is judged (with `Bright` above a
  combo of 100), `Held` on a completed hold, `HitMine`, holding and rolling
  on and off, and each hold's life for the let-go grey (research §5–6).
  Deterministic, tested with the replay fixtures; the receptor flash the
  procedural skin uses stays as it is.
- Judgement tiers map to `W1`…`W5` (the five hit tiers of the ruleset, a
  Dancing☆Onigiri ruleset's included).

**Done 2026-10-10.** `Player::skin_events_since(seq)`: a log of the last
256 events, numbered. Explosions follow StepMania's dance rule whatever the
ruleset counts: a row flashes, every lane with the row's last judgement,
when a press completes it; a row ending on a miss does not flash.
`ReceptorState::holding` and `NoteSprite::{beat, hold_life}` complete the
frame. Bright: combo above 100, after the row is scored.

## Step 4. Drawing (commit 4)

- `render`: a textured sprite pipeline (per-instance texture rectangle and
  translation, diffuse, glow, normal and additive blend), batched by
  texture; skin images uploaded once per play.
- Per lane: receptor actor, notes (taps, heads, tails, mines, lifts, fakes)
  with frame selection and color, holds as StepMania draws them (top cap,
  body tiled from the tail end, bottom cap cropped when short, tail, head
  pinned while held; active and inactive sprites; grey after a let-go;
  `Flip*WhenReverse`; research §5), explosions; field order as StepMania's
  (research §7). 64 StepMania pixels are one lane width; Hidden/Sudden keep
  working (alpha and glow).
- Elements resolved as built-in, Lua or model are drawn by the procedural
  skin in the same place, so a partial skin plays.
- Checks: a synthetic test skin (script-drawn sheets, a note-color sheet, a
  vivid sheet, holds with caps) autoplayed in headless Chrome on WebGPU and
  WebGL2, all top tier, frame time within budget; screenshots at known song
  times (frame selection, colors, holds under normal and reverse).
- Review before handover.

**Done 2026-10-10.** `crates/render/src/textured.rs` (StepMania's diffuse then
glow pass, normal and additive blending, clamp or repeat), `noteskin.rs`
(the playing skin: lanes' actors, commands from the events and the
holding edges, `DrawHold` geometry), and scene segments interleaving the
procedural sprites and the skin's quads (receptors, holds, notes,
explosions). Elements made only of the stand-ins' art are the built-in
skin's shapes; explosion parts that fall back use generated white art
(`art.rs`). The synthetic skin is `crates/noteskin/tests/make_test_skin.py`.
Headless Chrome, a bundled 10-level chart: all 329 taps top tier and 27 of
27 holds kept with the test skin on WebGPU, as without it; WebGL2 and
reverse checked by screenshot (held bodies green, head pinned, flipped caps
under reverse, explosions, vivid colors), 60 fps. StepMania's Delta
(local copy) plays as a partial skin: its taps and receptor (Lua) built in,
its holds, caps and explosions its own.

## Step 5. Import, storage, the setting (commit 5)

- Import a noteskin from a folder or a zip: a skin folder (`metrics.ini` or
  `NoteSkin.lua` with images), a `NoteSkins/<game>/<name>/` tree (several
  skins, `common` and the game's `default` included), or skins inside a
  song import. Only `dance` skins and `common` are kept; others are listed
  as skipped. Stored in IndexedDB like copied songs (linking a kept folder,
  decision 33, can follow).
- The import report lists each skin's elements drawn by the built-in skin
  and why (Lua, a model, missing), and the files it ignored (`NoteSkin.lua`).
- Options: "Arrows" (built-in, or an imported skin), applied to dance
  layouts; the color scheme applies to the built-in arrows only. Delete an
  imported skin.
- Images decoded by the browser and limited in size (texture limits, a
  memory budget per skin).
- Firefox by the maintainer; review before handover.

**Done 2026-10-10.** Skins are found in any import (folder, zip, a
`NoteSkins` tree; `ddi_noteskin::import::find_skins`), dance and common
ones stored in IndexedDB (a `noteskins` store, database version 2, and the
files under `noteskin/<game>/<name>/` in `files`; images, Lua, models and
texts are kept, at most 2,000 files a skin), other games' reported as
skipped. The import report has a line per skin (how many of a column's
elements are its own images, which are Lua, models or StepMania's own).
Options: "Arrows" (built in, or an imported skin) and removing the chosen
skin. A play readies the skin while the song loads (images decoded at
their own size, up to 4096 px), and the debug object reports it
(`__DDI_DEBUG.noteskin`). A chosen skin that is no longer there (removed,
or on desktop chosen from a folder another machine has) is drawn as
Options shows it, as if it were not chosen: Dito with its parameters,
before Options has been opened too (`ddi_shell::play_skins`, 2026-10-21). On the web, Start waits for the skin (the
prompt says "loading the noteskin…"); a bundled skin's files are fetched
once a session, together, and tried again when a download fails, and the
chosen skin is fetched in the background soon after the app starts
(`noteskins::warm`), so a play on a slow network neither starts without
it nor loses it on the next song.

Reviews of steps 2–5 found, and these were fixed: a call budget for
commands that play each other (a skin's `Init` could freeze the page), hold
heads drawn separately from bodies, taps in row order (later ones on top),
the receptor's judgement before `Press` and never `Miss`, StepMania's 2D
projection of x/y rotations, bright taken before the row's score, the
Active parts only while the button is down (and `HoldActiveIsAddLayer`),
holds cut to what Hidden/Sudden leave visible, reverse per column,
`finishtweening`/`hurrytweening` passed to children, texture coordinates in
padded units, subtract and modulate blends, a fresh copy of the actors on a
retry; the library reset now removes skins, a skin's images are capped at
64 megapixels together, a storage error no longer resets the choice, a tab
older than the library says to reload, a skin readied mid-play waits for the
next play, and imports cap files (2,000 kept), texts (1 MB) and skins (200).

Known differences left: holds and taps are drawn field-wide (all holds,
then all taps) rather than column by column; a short bottom cap's texture
shift is relative to its frame (StepMania's is in whole-texture units, which
differs only for sheets stacked vertically); on an sRGB target blending is
in linear light (decision 17), so fades and additive explosions look
brighter than in StepMania (since 2026-10-12 skins blend as StepMania's on
WebGPU and since 2026-10-13 on WebGL2, `docs/plans/bundled-skins.md` step
5); re-reading a kept folder imports again a skin
the player removed and keeps one whose folder is gone.

## Step 6. Documentation

`docs/PLAN.md` (phase 8 row, a decision for noteskins), `AGENTS.md`
(`noteskin/` under Layout, the corpus test under Commands), the verify skill
(the test skin, the import, screenshots).

## Later

Done since this plan (2026-10-12 to 2026-10-14), each in a plan of its
own: a skin's Lua (step 7, `docs/plans/lua.md`, on `lunita`,
`docs/plans/piccolo-fork.md`), 3D models with bones (step 8,
`docs/plans/models.md`), bundled skins (Dito, Delta, Alpha-Arrows, Dake,
Peter Lawrence's CF and DDR skins, `docs/plans/bundled-skins.md`),
StepMania 3.9 and OpenITG skins (`docs/plans/legacy-skins.md`), a preview
(`docs/plans/skin-preview.md`), and skins on Dancing☆Onigiri and pump
layouts (`docs/plans/danoni-skins.md`, `docs/plans/pump.md`). Left:

- OutFox's `global/` skins and per-game extras.
- Pump skins: done 2026-10-16 (`docs/plans/pump.md`, "Pump skins").
- Fades and crops: done 2026-10-16. `cropleft`, `cropright`, `croptop`,
  `cropbottom`, `fadeleft`, `faderight`, `fadetop`, `fadebottom` (and
  `GetCrop*`) tween with the rest of the actor's state
  (`ddi_actor::TweenState::crop`, `fade`) and are drawn as StepMania
  5.1's `Sprite::DrawPrimitives` draws them: a crop cuts the quad and its
  image, never past the sprite; a faded edge is a piece of its own around
  the inside, transparent on its outer side, its inner side as opaque as
  a crop leaves room for, the corners where two fades meet left out; the
  glow is not faded. Both draw paths (the 2D pass, a skin in play) take
  the pieces as quads. Our shapes (`cornerradius` and the rest) are of
  a whole quad: a cut piece draws StepMania's plain one. In the corpus
  pump/delta (StepMania 5.1's, ITGmania's, OutFox's) and the community
  Bright14 use them; they load and play through their Lua with no
  warning about them (`crates/desktop/tests/actor_cuts_offscreen.rs` draws each
  direction and a tween on the GPU; the 84 skin previews of the bundled
  skins are identical).
