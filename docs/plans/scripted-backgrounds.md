# Implementation plan: scripted backgrounds and `#FGCHANGES` (2026-10-21)

Phase 8's last StepMania background feature: the BGAnimations a
`#BGCHANGES`, `#BGCHANGES2` or `#FGCHANGES` change names (a folder in the
song's folder), drawn behind the field (and over it, for a foreground) on
both shells, their Lua sandboxed as noteskins' is. The named effects and
transitions that need no script are done (`overlays.md`, "Named effects
and transitions"); modfiles (scripts that move the field or change the
game) stay out of scope (`docs/research/lua-sandbox.md`, section 6).

## What StepMania 5.1 does

Sources: StepMania 5.1 (`5_1-new` at `825467b`, MIT): `src/Background.cpp`
(`CreateBackground`, `LoadFromSong`, `UpdateCurBGChange`,
`DrawPrimitives`), `src/Foreground.cpp`, `src/BackgroundUtil.cpp`,
`src/ActorUtil.cpp` (`ResolvePath`, `MakeActor`), `src/BGAnimation.cpp`,
`src/BGAnimationLayer.cpp`, `BackgroundEffects/UpperLeft.lua`,
`Themes/_fallback/Scripts/`.

- **What a name finds.** A change's file is looked for in the song's
  folder as a BGAnimation (`GetSongBGAnimations`: folders only), a movie,
  an image, then the global `BGAnimations/` (folders and `.xml` files),
  then the shared movie folders. A `.lua` file in the song's folder is not
  a background; a folder is, whatever it holds.
- **What a folder plays** (`MakeActor`, `ResolvePath`), with the default
  preferences (`QuirksMode` off):
  - with a `default.lua`: that script's actor tree, placed by the
    `UpperLeft` effect (a frame at the screen's corner, tinted by
    `Color1`): the script positions everything itself, usually with
    `SCREEN_CENTER_X`/`SCREEN_CENTER_Y`;
  - with a `default.xml` (and no `default.lua`): nothing (XML actors are
    quirks-mode only; an empty `Actor`);
  - with a `BGAnimation.ini`: nothing (`BGAnimation::LoadFromAniDir`: "We
    don't officially support .ini files anymore");
  - otherwise a StepMania 3.0 folder of layers: each image or movie in it
    (`*.png`, `*.jpg`, `*.jpeg`, `*.gif`, `*.ogv`, `*.avi`, `*.mpg`,
    `*.mpeg`; sorted; names starting with `_` skipped) is a layer whose
    file name picks its kind (`BGAnimationLayer::LoadFromAniLayerFile`:
    `usesongbg`, `center`, `stretchstill`, `stretchscroll*`,
    `stretchspin`, `particles*`, `tile*`), the `Centered` effect placing
    the folder.
- **The actor lifecycle.** Every background a song names is built when
  the song loads (`InitCommand` played as each actor loads). At a change
  the layer's current one gets `LoseFocus` and the transition's commands,
  the new one `InitState`, `On`, `GainFocus`; each layer updates its
  current and fading background every frame with the music's elapsed
  time (not scaled by the music rate), and drops the fading one when its
  tweens end. The same background named again is the same actor.
- **Foregrounds** (`Foreground::LoadFromSong`, `Update`): `#FGCHANGES`
  names a folder of the song (`default.lua`, else in quirks mode
  `default.xml`, else `MakeActor` on the folder); each is built at load,
  hidden, and at its beat shown with `On`; it stays while its tweens run
  (`GetTweenTimeLeft`, frames included) and is hidden for good after. A
  plain image or movie has no tweens and never shows. Scripts that want
  to stay `sleep` for a long time. Drawn over the field (under the
  theme's HUD).
- **What the scripts can reach.** Everything a theme can: every actor
  class, the screens and players (`SCREENMAN:GetTopScreen()`,
  `GetChild("PlayerP1")`), player options, preferences, the message bus,
  sounds, render targets (`ActorFrameTexture`), fonts by theme name,
  files anywhere in StepMania's file system. StepMania bounds none of it
  (`lua-sandbox.md`, section 6).

## What packs use

- **Local corpus** (read-only survey, counts only): of 2,225 local
  simfiles (`~/Music/Simfiles`, 1,482; OutFox's installed songs, 743),
  432 carry a non-empty background tag. Scripted names: one song (OutFox's own
  credits) names a `default.lua` folder on its first layer and another in
  `#FGCHANGES` (its only `#FGCHANGES`); 9 names in other songs are
  BGAnimations found nowhere (theme or global animations of older
  StepMania versions, such as one bundled song's `white reverse flash`).
  No song folder holds a `BGAnimation.ini`, a `default.xml` or a 3.0-style
  layer folder. The two scripts use `Def.ActorFrame`, `Def.Sprite`,
  `Def.Quad`, `Def.BitmapText` and `Def.Sound`; `xy`, `zoom`, `linear`,
  `accelerate`, `decelerate`, `sleep`, `diffusealpha`, `queuecommand`,
  `setstate`, `SetAllStateDelays`, `animate`, `texcoordvelocity`,
  `SetTextureFiltering`; `GAMESTATE:GetCurrentSong():GetSongDir()` to
  load their images; the foreground also reads the player's options and
  the top screen (a modfile habit).
- **Public scripted songs** (`lua-sandbox.md`, section 6): of 25 scripted
  songs and templates, 23 are modfiles, one a 16-line visual background,
  one a 26-line background reading song state. Animations call 334
  distinct actor methods, 65 of them noteskins' too (59 % of call sites).
- So: what matters first is a plain visual background script (frames,
  sprites, quads, tweens, the song's images, its position), then text,
  then everything modfiles reach, which is out of scope.

## Decisions (the maintainer may change any)

1. **A sandboxed VM per song, on the noteskins' Lua host.** The skins'
   runtime (`crates/noteskin/src/lua/`: the prelude, `Def`, `LoadActor`,
   the converter from actor tables to `ddi-actor` trees, 65 native actor
   methods, commands, messages, update functions, fuel and deadlines)
   already is StepMania's actor model; a song's folder is given to it as a
   one-folder "skin" whose files Lua sees at StepMania's own path,
   `/Songs/<pack>/<song>/` (`Skin::song`). The background code lives
   next to the noteskin Lua (`crates/noteskin/src/lua/`, entry
   `LuaSkin::actor_file`, and `crates/noteskin/src/lua/song.lua` for what
   only songs read), not in a crate of its own: the host is one, and
   splitting it would copy it. A song's scripts get a VM of their own,
   never a skin's (a failing background never turns off the arrows).
2. **The per-play runtime is shared** (`crates/session/src/scripted.rs`):
   which scripts a play needs, their schedule (from the layers'
   schedules and `#FGCHANGES`), the commands at each change, the frame's
   updates within one budget, what each draws and at what opacity; both
   shells feed it the folder's files and draw its quads.
3. **Drawn in the scene**: a script's quads are textured quads of the
   field's renderer, in StepMania's screen (480 pixels tall, as wide as
   the background frame's shape) mapped onto the background frame;
   backgrounds before the field (with Dancing☆Onigiri's back layers),
   foregrounds after it, under the HUD (with the mask layers). The first
   layer shows black where its current background is a script (the
   script's own quads over it). Simplification: a script draws over both
   image layers (StepMania draws a first-layer script under the second
   layer), and the transitions fade a script in or out (StepMania applies
   wipes and slides to its leaves too).
4. **What plays.** Folders with `default.lua`, on either background
   layer and as foregrounds. Classes: `ActorFrame`, `Sprite` (images of
   the song's folder; movies are not decoded for scripts, so a movie
   sprite draws nothing), `Quad`, `Model`, `Actor`; `BitmapText`,
   `ActorMultiVertex`, `ActorFrameTexture`, `ActorProxy`, `ActorScroller`
   and `Sound` load as actors that draw nothing (noted once in the log).
   Folders without one keep what the game does now (the song background
   on the first layer, nothing on the second), although StepMania 5.1
   draws an empty actor for `BGAnimation.ini` and `default.xml` folders:
   they were made for StepMania 3.9 and ITG, whose authors meant them to
   play, and a later step can draw them natively (the legacy noteskin
   reader parses the same actor files). 3.0-style layer folders come with
   that step too.
5. **Budgets.** Load: 20 million fuel and 5 s for all of a song's scripts
   together, in slices between frames on the web (the skins' load
   budget); play: 100,000 fuel and 8 ms per frame for all of them (half a
   skin's: the field comes first); 4,096 actors, 64 images and 48 million
   decoded pixels a song (the work layers' cap), a script's memory under
   the VM's own limit (`ddi_lua::Settings`). A song's scripts that error
   at load draw nothing (that one background shows the song background);
   one that runs out of its budget or memory, or errors at load, at a
   change or in an update stops all of the song's scripts for the rest of
   the play (they draw nothing; the first layer shows the song
   background): one line in the results' notes and the log. A
   command that errors is reported and the play goes on, as in StepMania.
6. **What never runs.** No file is read but the song folder's own files
   the import kept (scripts and images, read-only, through the host); no
   network, `io`, `os`, `require`, `dofile` or `debug` beyond what the
   skins' sandbox keeps; no screen, player, option, preference or
   profile is reachable (`SCREENMAN`, `GetTopScreen`, `PlayerOptions` and
   the like are absent or inert, so modfiles do nothing); no sound plays;
   no movie is decoded for a script; nothing a script does changes
   judging, the clock, the score or a replay's inputs (the runtime reads
   the song position, never writes it).
7. **The time a script sees** is the music's: `GAMESTATE:GetSongPosition()`
   and the beat from the song's timing, each frame's delta the song time
   that passed (0 while paused), so a replay draws the same backgrounds.
8. **Import.** The web stores a scripted folder's scripts and images
   with the song (64 files and 16 MB a song at most, the rest left out
   with a note); desktop reads them in place. Songs imported before keep
   showing the song background until imported again.

## Steps

1. **Host for songs** (`noteskin`): `Skin::song`, a one-folder "skin" at
   `/Songs/<pack>/<song>/` with no fallbacks; `LuaSkin::actor_file`
   (`LoadActor` of a path, converted and initialized like an element);
   `song.lua` (the `SCREEN_*` globals and `_screen` from the frame's
   shape, `GAMESTATE:GetCurrentSong()` with `GetSongDir`, the song's
   timing); text and other unsupported classes drawn as nothing rather
   than refusing the tree. Natively tested on synthetic scripts.
2. **Schedule** (`library`): scripted folders on both layers and in
   `#FGCHANGES` (`BgImage::Scripted`, the first layer black under it, the
   second clear), the files the import keeps, natively tested.
3. **Runtime** (`session`): the commands at each change (`LoseFocus`,
   `On`, `GainFocus`), the foregrounds' show and hide, the updates, the
   budget and the stop; natively tested on a fake clock.
4. **Drawing** (`render`): the trees' quads in the frame, background
   before the field and foreground after; offscreen on desktop.
5. **Shells**: the web's import and play, desktop's play; a synthetic
   pack in headless Chrome (WebGPU, WebGL2) and Firefox, offscreen on
   desktop.
6. Later: `BitmapText` with the game's face for theme font names;
   `BGAnimation.ini`, `default.xml` and 3.0-style layer folders drawn
   natively (decision 4); the global `BGAnimations/` folder; `Sound`
   actors on the play's mixer; render targets within a pixel budget;
   movies in scripts.

## Status

Planned 2026-10-21; nothing built yet.
