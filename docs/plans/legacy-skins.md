# Implementation plan: StepMania 3.9 and OpenITG noteskins (2026-10-13)

Draw the noteskins made for StepMania 3.9, 3.95 and OpenITG (and NotITG,
which reads the same format): the older ITG community skins, and Peter
Lawrence's `Open-ITG` and `Not-ITG` folders (16 skins, his StepMania 5
`Peters-Shadow-*` skins fall back on them). Until now an import recognises
them and says their `.sprite`, `.model` and `.actor` files are not read
(`docs/plans/noteskins.md`, Later).

Source: OpenITG @ `f2c129fe` (StepMania 3.95's code under StepMania's
MIT-style licence, `Licenses.txt`; read to learn its behaviour, our code
our own): `src/NoteSkinManager.cpp`, `src/NoteDisplay.cpp`,
`src/GhostArrow*.cpp`, `src/HoldGhostArrow.cpp`, `src/ReceptorArrow*.cpp`,
`src/ActorUtil.cpp`, `src/Model.cpp`, and its own skins
(`assets/d4/NoteSkins/`: `dance/default`, `flat`, `cel`, `metal`, `robot`,
`vivid`, `common/default`; ITG's art, so stand-ins by name only, as for
StepMania 5's `default` and `common`).

## How the format differs (to confirm in step 1)

- Elements by file, no Lua: `Down Tap Note 4th.model`, one file per note
  type when `TapNoteAnimationIsNoteColor` (`NoteDisplay.cpp:212`), the
  same as our per-colour copies for `Var "Color"`.
- `.actor` (an INI: `File=`, `BaseRotationZDegrees=`, commands),
  `.sprite` (an INI: `Texture=`, frames, delays, base rotation), `.model`
  (an INI naming `Meshes=`, `Materials=`, `Bones=` MilkShape files, which
  `noteskin/src/model.rs` reads), `.xml` (actor trees whose commands are
  metrics-style strings, which `noteskin/src/commands.rs` reads).
- Explosions by judgement and brightness: `Tap Explosion Bright Great`,
  `Dim Marvelous`, `Hold Explosion`, `Roll Explosion`, `ok`/`ng` for
  holds (`GhostArrowRow.cpp:41`); receptors `Receptor Go` and
  `Receptor Waiting`.
- Metrics of 3.9: `[NoteDisplay]` `…AnimationLengthInBeats`,
  `…AnimationIsNoteColor`, `ReverseDrawOrder`, `HoldHeadIsAboveWavyParts`;
  `[GhostArrowDim]`/`[GhostArrowBright]`; `FallbackNoteSkin` to
  OpenITG's skins (`flat`, `default`).

## Status (2026-10-13)

Steps 1 to 3 are done, and step 4 for the 16 skins: all of Peter
Lawrence's `Open-ITG` and `Not-ITG` skins build every element the engine
asks for as their own (`local_skins`), import as "25 of 25 elements are
its own", and play all top tier in headless Chrome (WebGPU, WebGL2) and
Firefox; the preview draws them. His `Peters-Shadow-*` skins fall back on
a StepMania 5 `Peters-DDRLike` his repository does not have, so they are
not this format's to fix.

How it is built (`noteskin/src/legacy.rs`):

- A skin is in the format when it, or a skin its chain falls back on
  among the imported ones, has `.sprite`, `.actor` or `.model` files and
  no `NoteSkin.lua`; its chain then ends in stand-ins for OpenITG's
  `dance/default` (its layout as code: Down's elements turned for the
  other buttons, the redirects; its art drawn built in), StepMania 3.9's
  `flat` and OpenITG's `common/default` (an imported `default`, `flat`
  or `common` with a `NoteSkin.lua` is StepMania 5's and is passed over;
  a StepMania 5 skin falling back on an old one stays StepMania 5's).
  Their metrics are StepMania 3.9's `dance/default` and `flat` (byte for
  byte, free; without 3.9's `[ReceptorArrow]`, which OpenITG's
  `dance/default` does not have) and a few of ours (the receptor's pulse,
  a held hold's flash).
- Elements are looked up by button, without StepMania 5's
  `dance/default` redirects; a part whose `<Part>AnimationIsNoteColor` is
  set loads a file per note type (`build_colored`), the play keeping one
  set per type as for skins whose Lua reads `Var "Color"`.
- Actor files are read into nodes (`Node`; INI sections, and a strict
  XML reader with depth and size limits) and built as OpenITG's
  `ActorUtil` does, each file read once per skin. Lua is not run: only
  `@NOTESKIN:GetPath(…)` is read. An element's files make at most 4,096
  actors, 8 files deep; a node has at most 520 attributes; a skin's files
  make at most 32,768 actors and 4 million units of work (an actor and
  each of its attributes, 64 bytes read), so hostile files fail an
  element (drawn built in) instead of stalling a load. A bones file that
  is not there leaves a model still (OpenITG stops on it; the skins point
  at OpenITG's own).
- Commands are parsed as OpenITG writes them (`commands::parse_legacy`);
  `effectdelay` is supported natively but not offered to StepMania 5 Lua.
- The receptor is OpenITG's: the key-press block under `receptor go`,
  the `[ReceptorArrow]` score command then `Press` on a step. `receptor
  waiting` is left out: a play starts at once.
- The explosion is `GhostArrowRow`: a dim and a bright row of one actor
  per score, the play sending `Dim<score>`/`Bright<score>` as well as
  StepMania 5's commands; the hold explosion shows while a hold or roll is
  active. There is no roll explosion (NotITG's `Roll Explosion` files are
  not read) and no lifts (drawn built in).
- Vivid animation is OpenITG's (rounded a quarter step early), metrics
  read by their 3.9 names, a hold part's under the next part's name as
  OpenITG reads them. No `On` or `Reverse` command is played on a
  receptor as the play starts.
- Skins imported before 2026-10-13 have no actor files stored: they are
  drawn built in until imported again.

## Steps

1. **Research note**: the format as OpenITG loads and draws it, element by
   element, with the differences from StepMania 5 that change what is
   drawn (`docs/research/noteskins.md`, a section of its own); a survey of
   the local skins in the format (which files, which features).
2. **Loading**: the 3.9 element names onto the engine's elements, the
   per-colour files, the file types into actors (reusing the model,
   sprite and command code), stand-ins for OpenITG's own skins; natively
   tested on synthetic skins, then on the local ones (`local_skins`).
3. **Playing**: the explosions by judgement and receptor states driven by
   the engine's events, the 3.9 metrics, and the import report; checked
   by screenshot against the skins' look and with full autoplays.
4. **Peter's skins**: his 16 old-format skins and the three
   `Peters-Shadow-*` that fall back on them, checked in the preview
   (`docs/plans/skin-preview.md`) and, for the Shadow ones, against his
   renders.

## Later

- NotITG's own additions (its Lua mods), if a skin needs them.
