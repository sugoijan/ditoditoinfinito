# Noteskins: StepMania's format, the Lua in it, and what can be bundled (2026-10-10)

Scope: let players import StepMania noteskins for the `dance` layouts
(single, solo, double) and draw them as StepMania does, next to the
procedural arrows we have. Same conventions as the other notes: sources
cited, third-party skins read locally and never copied into the repository.

Sources, pinned:

| Tag | What | Revision |
|---|---|---|
| SM | StepMania 5.1, https://github.com/stepmania/stepmania branch `5_1-new` | `825467bcd81c812b33ad684dc04dd151b2d5dec3` |
| ITG | ITGmania, https://github.com/itgmania/itgmania branch `release` | `8a3bf8884acdcb69363f7f0ea592138e398c71e4` |
| SM39 | StepMania 3.9, tag `v390release` | `5d417c1f1ab6a1bf090b4ba21a3ca37e8d4af892` |
| OF | Project OutFox, local install (Mac Public Alpha 0.4.15.1); closed source, read from its shipped skins, metrics and docs | — |

`SM src/NoteDisplay.cpp:667-695` means that file and lines at the SM
revision. Where the code and a document disagree, the code wins.

## 1. The model

A noteskin is a folder of images, Lua actor files, `.redir` files and a
`metrics.ini`. For every element the engine needs ("Down Tap Note") it
calls the skin's Lua `Load()`, which normally maps the button and element
to a file with `NOTESKIN:GetPath` and `LoadActor`s it: a PNG becomes a
Sprite, a `.lua` file returns an actor tree, a `.txt` file is a MilkShape 3D
model. The engine keeps **one actor per (button, element)** and draws it
once per note, setting its sprite frame from the song beat, shifting its
texture coordinates for the quantization colour, and placing it from the
scroll mods. Holds are not actors: their caps and body are textured strips
sampled from three Sprites. Receptors and explosions are long-lived actors
the engine drives with named commands ("Press", "W1", "HoldingOn"), whose
effect is defined in Lua and metrics.

## 2. Lookup

- **Folders and fallbacks** (`SM src/NoteSkinManager.cpp:75-201`): skins live
  in `NoteSkins/<game>/<name>/`, matched case-insensitively. Each step of the
  chain looks in `NoteSkins/<game>/X`, then `NoteSkins/common/X`; the next
  step is `[Global] FallbackNoteSkin` in `metrics.ini`, and without that key
  `<game>/default` and then `common/common`. A missing fallback drops the
  skin. Metrics are merged so the skin wins over its fallbacks
  (`MergeIniUnder`, `:164`). The `NoteSkin.lua` files run from the last
  fallback back to the skin, each receiving the previous table as `...`
  (`:173-199`), hence every script's `local ret = ... or {}`. ITGmania
  requires a `metrics.ini` in every skin of the chain
  (`ITG src/NoteSkinManager.cpp:208-236`) and adds `variants/<name>/`
  subfolders exposed as `<skin>_<variant>` (`:118-146`).
- **The engine requires only `Load`** (`SM src/NoteSkinManager.cpp:461-487`),
  called with the thread variables `Player`, `Controller`, `Button`,
  `Element`, `SpriteOnly` (ITGmania adds `Color`). `Redir`, `Rotate`,
  `PartsToRotate` and `Blank` are conventions of `common/common` and
  `dance/default`'s Lua (`SM NoteSkins/common/common/NoteSkin.lua:3-61`,
  `NoteSkins/dance/default/NoteSkin.lua:3-77`); OutFox's skins use other
  table names (`ButtonRedir`, `ElementRedir`, `KBXRedir`; OF
  `global/broadhead/NoteSkin.lua`). An importer can rely on `Load` only.
- **`GetPath(button, element)`** (`SM src/NoteSkinManager.cpp:336-459`): over
  the whole chain, a **case-insensitive prefix match** of `"<Button>
  <Element>"` (so frame hints and extensions may follow), then the same for
  `"Fallback <Element>"`; a `.redir` file holds a bare name that is searched
  again as is (up to 100 hops). Several matches warn and take the first by
  lowercase name (`GetPathFromDirAndFile`, `:527-544`;
  `src/RageUtil_FileDB.cpp:10-60`). Names starting with `_` are private
  assets the skin's Lua loads by name. SM5 has no extension priority (SM 3.9
  had one, `SM39 src/NoteSkinManager.cpp:291-304`).
- **`LoadActor(path)`** resolves relative to the calling script's folder
  (`SM Themes/_fallback/Scripts/02 ActorDef.lua:95-162`): `.lua` runs the
  chunk, images become `Def.Sprite`, `.txt` becomes `Def.Model` (MilkShape
  ASCII), a folder a `BGAnimation`.
- **StepMania 3.9 skins** use other element names (`tap note 4th`,
  `receptor go`, `tap explosion dim Perfect`), other metric names and a
  different vivid formula (`SM39 src/NoteDisplay.cpp:240-373`,
  `src/GhostArrowRow.cpp:50-52`). SM5 has no compatibility path for them;
  players' 3.9/OpenITG/NotITG skins are a separate format ("Noteskins for
  3.9 wont work with SM5 and vice versa",
  https://zenius-i-vanisher.com/v5.2/thread?threadid=5967).

## 3. Elements

What the C++ asks for, per column (`SM src/NoteDisplay.cpp:432-454`,
`src/ReceptorArrow.cpp:35-37`, `src/GhostArrowRow.cpp:29-39`):

| Element | Kind |
|---|---|
| `Tap Note`, `Tap Mine`, `Tap Lift`, `Tap Fake` | actor |
| `Hold Head`, `Roll Head` (`Active`/`Inactive`) | actor |
| `Hold Topcap`, `Hold Body`, `Hold Bottomcap` and the Roll ones (`Active`/`Inactive`) | **Sprite** (`SpriteOnly`) |
| `Hold Tail`, `Roll Tail` (`Active`/`Inactive`) | actor (`common` redirects it to `_blank`) |
| `Receptor`, `Explosion` | actor, one per column |

"Topcap"/"Bottomcap" with a lowercase c: file lookup ignores case, Lua
`Blank`/`PartsToRotate` keys do not. `Tap Addition` is never requested
(`:438`). Everything else (`Tap Explosion Bright/Dim`, `Hold Explosion`,
`Roll Explosion`, `HitMine Explosion`, `Ready`/`Go Receptor`) is requested by
`common/common`'s Lua (`Fallback Explosion.lua:1-41`, `Fallback
Receptor.lua`), and real skins invent their own (ITG `metal` uses `Tap
Explosion Dim W1`…, `NoteSkins/dance/metal/Fallback Explosion.lua:24-115`).
OutFox adds hold-lift tails, mine holds and an `Ovceptor` overlay (OF
`common/common/*.redir`). Buttons for dance: `Left, Down, Up, Right, UpLeft,
UpRight` (`SM src/GameManager.cpp:513-518`); double reuses the four names
for the second pad, distinguished only by `Var "Controller"`
(`src/Style.cpp:130-139`).

## 4. Sprite sheets, animation and colour

- **File name hints**: `" WxH"` frame grid, row-major (`SM
  src/RageTexture.cpp:29-77`); `(res WxH)` overrides the displayed source size
  (`src/RageBitmapTexture.cpp:20-42,319-320`); `(doubleres)` halves the
  display size (`:322-330`). Display size is the source frame size, with no
  rescaling to the 64 px arrow: the convention is a 64×64 frame, often a
  128 px `(doubleres)` one. Textures are padded to powers of two and texture
  coordinate metrics are in padded units (`:164-166`,
  `RageTexture.cpp:36-39`).
- **Animation is beat-locked** (`SM src/NoteDisplay.cpp:667-695`, the same in
  ITGmania): all of a sprite's states cycle over `<Part>AnimationLength`
  beats (seconds with `AnimationIsBeatBased=0`). **Vivid** adds the note's
  beat fraction floored to steps of `1/L` to the cycle position, so it does
  nothing when `L=1` (`midi-vivid` uses `L=4` with a `4x4` sheet). On a Lua
  ActorFrame the engine's frame selection does nothing and the Lua animates
  itself (`src/Actor.h:620-621`).
- **Quantization colour is a texture translation** for taps, heads and tails,
  not bodies (`SM src/NoteDisplay.cpp:1346-1381`): `NoteColorType`
  `Denominator` (`BeatToNoteType`: 4th … 192nd, clamped to
  `NoteColorCount − 1`, default 8; `src/NoteTypes.cpp:128-144`), `Progress`
  or `ProgressAlternate` (rainbow), times
  `<Part>NoteColorTextureCoordSpacingX/Y`. "Note" skins put eight colours
  along one axis (`dance/default`: spacing Y `.125` on `_arrow 1x8`); flat
  skins have no spacing. The changelog's "ProgressOnBeatAtEnd" does not exist
  in the code (`src/NoteDisplay.h:38-45`).
- **Metrics**: every `[NoteDisplay]` value is read per button (`[Down]` first,
  then `[NoteDisplay]`; `src/NoteSkinManager.cpp:289-313`). The parts are
  `TapNote, TapMine, TapLift, TapFake, HoldHead, HoldTail, HoldTopCap,
  HoldBody, HoldBottomCap`; there are no `Roll*` parts (rolls use the Hold
  metrics). The other metrics and their `common/common` defaults:
  `DrawHoldHeadForTapsOnSameRow` 1, `StartDrawingHoldBodyOffsetFromHead` 0,
  `StopDrawingHoldBodyOffsetFromTail` −32 (0 in `dance/default`),
  `HoldLetGoGrayPercent` 0.25, `HoldHeadIsAboveWavyParts` 1,
  `FlipHeadAndTailWhenReverse` 0 (1 in `dance/default`),
  `FlipHoldBodyWhenReverse`, `TopHoldAnchorWhenReverse`,
  `HoldActiveIsAddLayer` (`SM NoteSkins/common/common/metrics.ini:5-83`,
  `src/NoteDisplay.cpp:107-137,575-590,1102-1147`). `ReverseDrawOrder` and
  `UseLighting` are read by nothing in 5.1.

## 5. Holds

`SM src/NoteDisplay.cpp:760-1264`: per hold, the top cap, the body and the
bottom cap (a diffuse pass, then a glow pass), then the tail actor, then the
head actor on top; all holds of a column before its taps
(`:1598-1601`). The parts are triangle strips of three vertices per row,
every 16 px, following the column's x position at each y, so holds bend
with x-moving mods. The **body tiles** every frame height with texture
wrapping, anchored at the tail end so it does not slide as the hold shrinks
into the receptor (`:802-820`; at the head with `TopHoldAnchorWhenReverse` in
reverse); caps use the whole frame, and the bottom cap is cropped from the
top when the rest of the hold is shorter. While a hold is active the head
stays on the receptor; the **Active** sprites show while held (always while
inside a roll, `src/Player.cpp:1414-1424`); colour scales from 1 down to
`HoldLetGoGrayPercent` as the hold's life drains after a let-go (`:1226`); a
completed hold disappears (`:1577-1581`). Under reverse the head and tail
offsets swap, so `FlipHeadAndTailWhenReverse=1` is needed for the head to
land on the hold's head (`:1219-1228`).

## 6. Receptors and explosions

- **Receptor** (`SM src/ReceptorArrow.cpp`, `ReceptorArrowRow.cpp`): `Init`;
  `ReverseOn`/`ReverseOff`; `On` on the first field update
  (`src/NoteField.cpp:334-336`); `Press`/`Lift` on button edges; the
  judgement name (`W1`…`W5`, `Miss`, `HitMine`, `None` for an empty press) on
  **every press** (`ReceptorArrow::Step`, `:69-75`; `src/Player.cpp:2611-2615`);
  `ShowNoteUpcoming`/`HideNoteUpcoming` every frame. The beat pulse is not a
  command but the receptor's InitCommand, `effectclock,'beat';diffuseramp`
  (`common/common/metrics.ini:113`), so the actor effect system is needed.
- **Explosion** (`SM src/GhostArrowRow.cpp:60-143`): a `ColumnJudgment`
  message, then `Judgment`, `Bright` or `Dim`, then `W1`…`W5`, `HitMine` or
  `Held`; `HoldingOn`/`HoldingOff` and `RollOn`/`RollOff` follow
  `active && life > 0`. In dance a tap flashes only when its **whole row** is
  judged, every column with the row's last judgement
  (`src/Player.cpp:2571,2834-2856`). Bright means combo above
  `BrightGhostComboThreshold` (100) or the Blind mod (`:2562,2838`). SM 5.1
  never sends `LetGo`, `MissedHold` or a tap `Miss` to explosions (only
  `DidHoldNote(Held)` and `DidTapNote` call sites in `src/Player.cpp`).
  OutFox adds `ProW1`…`ProW5` (OF `common/common/Fallback Explosion.lua:25-54`).

## 7. Rotation, sizes, draw order

- One Down image becomes the other directions through `BaseRotationZ`, set by
  `Load` for the elements in `PartsToRotate` (`SM
  NoteSkins/common/common/NoteSkin.lua:50-52`); `dance/default`'s `Rotate`:
  Up 180, Down 0, Left 90, Right −90, UpLeft 135, UpRight 225
  (`NoteSkins/dance/default/NoteSkin.lua:55-63`). Caps, body and tail never
  rotate. `Redir` maps the button before lookup but `Rotate` is indexed by
  the original button, so a skin can ship only `Down …` files.
- `ARROW_SIZE` and the column spacing are 64 (`SM src/ScreenDimensions.h:36`,
  `src/GameManager.cpp:136`); one beat at 1× is 64 px (`[ArrowEffects]
  ArrowSpacing`, `Themes/_fallback/metrics.ini:196`). There is no noteskin
  zoom metric; zoom comes from the field and Mini/Tiny
  (`src/ArrowEffects.cpp:1338-1361`).
- Field order: board, receptors, columns (holds, then taps from far to near),
  explosions (`src/NoteField.cpp:787,815,1071-1075`; `NoteDisplay.cpp:631-640`).

## 8. The Lua in real skins

A survey on 2026-10-10 of 324 dance noteskins in 37 families, read without
running any of their code (files syntax-checked with `luac -p` only):
OutFox's 28 (`dance/`, `global/`), StepMania 5.1's 16, ITGmania's 13, and
267 from 22 community repositories (found with `gh search repos noteskin`,
by stars; among them Pete-Lawrence/Peters-Noteskins `6faea8e`,
SchneiderAFX/Schneiders-DDR-HD-Noteskins `a7409b7`, MadkaT182/DDRNoteskins
`6a57803`, HURG-IIDX's repositories, MidflightDigital/STARLiGHT-NEXXT
`0525c1e`, PoRa-dayo's, 1033Forest/Pulsen-NoteSkins `3a65dcb`,
DarkBahamut162/dance `d9e1664`). NotITG/OpenITG builds (49), the abandoned
5.1 "newskin" format (3) and overlay or option folders (26) were left out.
Fallback chains were followed. The scripts were one-off; the plan's corpus
test (`docs/plans/noteskins.md`) makes the measure repeatable.

| Class | Skins |
|---|---|
| (a) data only: images, metrics, a boilerplate `NoteSkin.lua` | 1 |
| (b) simple Lua actors: `Def.Sprite`/`ActorFrame`/`LoadActor`, `Frames`, tween commands | 80 |
| (c) heavy: 3D models only | 74 |
| (c) heavy: logic (engine state, update functions, `math.random`, `loadfile`), with or without models | 169 (33 without models) |

- **Every skin depends on Lua code.** `NoteSkin.lua` is always a program
  (53 distinct code shapes once data tables are stripped), and even
  `metrics.ini` commands are Lua: StepMania compiles `a,b;c,d` into
  `function(self) self:a(b) self:c(d) end`, with arguments such as
  `color("1,1,1,0")` or `PREFSMAN:GetPreference("GlobalOffsetSeconds")`
  (`SM src/LuaManager.cpp:898-984`).
- **StepMania's Lua is patched**: SM 5.1 and ITGmania vendor Lua 5.1.5 with
  `cmd` as a keyword (`SM extern/lua-5.1/src/llex.c:39`,
  `lparser.c:596-670`): `cmd(diffusealpha,0;zoom,1)` is
  `function(self, ...) self:diffusealpha(0) self:zoom(1) end`. 62 % of the
  skins use it; stock Lua rejects 42 of the 521 distinct files and misreads
  18 more, and a token rewrite to plain closures makes all 521 parse. OutFox
  links Lua 5.3.6; the corpus uses only the 5.1 ∩ 5.3 subset.
- **API surface** (distinct files): `NOTESKIN:GetPath` 407,
  `NOTESKIN:GetMetricA` 133, `NOTESKIN:LoadActor` 36, `Var` 130 (`Element`,
  `Button`, `SpriteOnly`, `Player`, `Color`), `LoadActor` 46, `color` 46;
  `Def.Sprite` 292, `ActorFrame` 201, `Model` 105, `Actor` 35, `Quad` 8,
  `BitmapText` 1 (no `ActorMultiVertex`, shaders or render targets); about
  eleven `GAMESTATE` methods (game, style, player options, steps and their
  timing, song position, lead-in); `string.find`/`gsub`/`lower`,
  `math.random`, `setmetatable`, `loadfile`. 64 actor methods, 58 of them
  registered by the engine and six defined in StepMania's own Lua
  (`smooth`, `thump`, `heartbeat`, `MaskSource`, `MaskDest`;
  `Themes/_fallback/Scripts/02 Actor.lua`), the most used being
  `diffusealpha`, `blend`, `effectclock`, `linear`, `zoom`, `diffuse`,
  `effectcolor1/2`, `visible`, `rotationz`, `stoptweening`, `finishtweening`.
  About 40 command names (`Init`, `None`, `W1`–`W5`, `Press`, `Lift`,
  `HitMine`, `Held`, `HoldingOn/Off`, `RollOn/Off`, `Judgment`, `Bright`,
  `Dim`, …).
- **Beyond Lua**: 65 % of skins load **MilkShape 3D ASCII** models (tap notes,
  lifts and mines mostly; every classic ITG skin) with `ani tex.ini` texture
  animation (ITG `NoteSkins/dance/cyber/_mine ani tex.ini`); 28 % read live
  song timing in per-frame update functions
  (`GAMESTATE:GetSongPosition():GetMusicSeconds()`); 41 % have commands that
  branch or read state, so the Lua must stay available during play.

## 9. Running that Lua in the app

Measured with `wasm-opt -Oz` standalone builds (the app's wasm was 4.73 MB,
1.90 MB gzipped):

| Option | Size (opt / gzip) | Outcome | Skins loadable at the Lua level |
|---|---|---|---|
| rilua 0.1.27 (pure-Rust Lua 5.1, MIT OR Apache-2.0, https://crates.io/crates/rilua) | 433 KB / 174 KB | builds for `wasm32-unknown-unknown` with no imports; string patterns, `pcall`/`error` and closures behave as PUC Lua; count hooks (`debug.sethook`) supported; young (first release 2026-02-15, 0.1.27 on 2026-10-03) | all |
| piccolo 0.3.3 | 719 KB / 243 KB | no `string.find`/`gsub`/`format`, `table.concat`, `load` | 20 % |
| mlua 0.12.2 (vendored) | — | `lua-src` refuses `wasm32-unknown-unknown` (emscripten and WASI only) | — |
| PUC Lua 5.1.5 (StepMania's patched copy) built with wasi-sdk as a separate module, like the video decoder | 137 KB / 66 KB | runs `cmd` natively; errors need Wasm exception handling (Chrome 137, Firefox 131, Safari 18.4) and JS glue | all |
| no runtime: recognise common shapes and interpret command strings | — | breaks on every new template | 37 % with a model loader, 20 % without |

(Superseded for the choice of VM by `docs/research/lua-sandbox.md`: with
untrusted scripts and a native build to come, none of these enforces time
and memory limits in-process as it is; the plan is a hardened fork of
piccolo.)

rilua keeps everything in one wasm with typed Rust callbacks and runs
natively for tests; the host API skins need is narrow (section 8), so
replacing the VM later is cheap. The patched C build is the fallback and a
test oracle.

## 10. What players use, and licences

- **ITG players** use ITGmania's set: the ITG2 classics (`cel`, `metal`,
  `flat`, `vivid`, `robot`: 3D models), `cyber`, `enchantment` and Peter
  Lawrence's DDR recreations (ITGmania commits `e424ff5c8`, `82e4d201d`,
  `e13de6a11`; README credits). Simply Love can hide StepMania's stock skins
  (`Scripts/SL-PlayerOptions.lua:154-180` at Simply-Love-SM5 `dd06138b`).
  **DDR-style players** use Peter's and Schneider's DDR recreations, ports of
  official skins (MadkaT182, Zenius-I-vanisher forum threads) and
  StepMania's `midi-*`. **OutFox** ships its own set plus a `global/` folder
  of skins for any game. Distribution is GitHub, engine bundles, forums and
  file hosts; no usage statistics exist.
- **Packs do not carry noteskins**: none of the 22 local packs has a
  `NoteSkins` folder, `metrics.ini` or Lua. Only NotITG loads skins from a
  song folder (`/Songs/{group}/{song}/NoteSkins/{name}`, NotITG 4.9 changelog,
  https://josevarela.net/SMArchive/Builds/BuildChangeLogs.php?Version=NITG49).
  So a noteskin is imported on its own (a folder or a zip, or a
  `NoteSkins/<game>/<name>/` tree).
- **Licences**: nearly every skin players use is undeclared or copies
  Konami's art (DDR's, and ITG's since Konami acquired In the Groove in
  2006). A repository licence tag cannot license that art. Skins with an
  explicit free licence on original art:
  - **Delta** by shakesoda, **CC BY 3.0** (`NoteSkins/dance/Delta/readme.txt`
    in StepMania 5.1): 2D sheets with 64 px frames. Fits the bundling rule
    as is.
  - **Alpha-Arrows** by Aozora Tori, **Apache-2.0**
    (`Noteskins/alpha-arrows/[Readme].txt` and `LICENSE` in
    Tiny-Foxes/Aozora-Tori-Assets `499ee273`): 2D, 512 px frames; its Lua
    derives from unlicensed OutFox skins, the images do not.
  - **Dake** by hanubeki, **Apache-2.0** (`README.md`, `LICENSE` in
    hanubeki/dake `e26aa8d0`): 2D sheets for every game; needs OutFox 0.5.
  - **Peter Lawrence's CF_CHROME/CF_VIBRANT/CF_ALLOY**, **Unlicense**
    (Pete-Lawrence/Peters-Noteskins `6faea8e1`): 3D models.
  - **Aozora-Note shapes**, **CC BY 4.0** (aozora-tori/Aozora-ProjectOutfox-Noteskin-Graphics
    `145e94eb`): SVG outlines only.

  StepMania 5's other stock skins sit under a notice that covers "the
  Software" and "source code" (`Docs/Licenses.txt`, `README.md`), not the art;
  OutFox's skins carry no licence at all.
- **Names and looks**: Konami holds the DANCE DANCE REVOLUTION and
  DANCEDANCEREVOLUTION marks and filed DDR (third-party trademark mirrors,
  not checked against USPTO); trade dress claims were part of Konami v. Roxor
  (E.D. Tex. 2:05-cv-00173). The dance-game patent US 6,410,835 has expired.
  Simple shapes and colourings are not protectable
  (37 CFR 202.1(a)), so our procedural arrows and colour schemes are fine;
  a close copy of a specific official rendering is not, and bundled skins
  never carry other games' names. Importing skins a player owns is the same
  model as importing songs.
- **Dancing☆Onigiri** skins are CSS themes plus single-colour silhouette
  image sets used as masks and coloured per lane (danoniplus `1a29eb62`,
  `js/danoni_main.js:1835`); works can name their own sets with `imgType`
  (https://github.com/cwtickle/danoniplus/wiki/dos-h0082-imgType). Out of
  scope here; the procedural symbols already play that role.
