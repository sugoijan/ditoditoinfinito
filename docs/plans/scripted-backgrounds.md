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
   sprite draws nothing), `Quad`, `Model`, `Actor`, `BitmapText` (since
   the second slice: whatever font it names, in the platform's
   sans-serif face, semibold, 20 pixels a line apart 24, StepMania's
   `_fallback` `Common Normal` being Open Sans Semibold at 24 pixels a
   line; drawn white by the shell, as a work's layers' text is, once per
   different text, 64 at most a song, and tinted by the actor's colors;
   `settext` and `maxwidth` reach it from Lua; a text not drawn yet is
   measured as a guess and shown once drawn, a frame or so later on the
   web); `ActorMultiVertex`, `ActorFrameTexture`, `ActorProxy`,
   `ActorScroller` and `Sound` load as actors that draw nothing (noted
   once in the log).
   Folders without one keep what the game does now (the song background
   on the first layer, nothing on the second), although StepMania 5.1
   draws an empty actor for `BGAnimation.ini` and `default.xml` folders:
   they were made for StepMania 3.9 and ITG, whose authors meant them to
   play, and a later step can draw them natively (the legacy noteskin
   reader parses the same actor files). 3.0-style layer folders come with
   that step too.
5. **Budgets.** Load: 20 million fuel and 5 s for all of a song's scripts
   together (the skins' load budget), while the start prompt shows, on
   the web in slices of 100,000 fuel with the page's turn between them
   (`Scripts::load_sliced`, as skins load); play:
   100,000 fuel and 8 ms per frame for all of them (half a skin's: the
   field comes first); the host's 16,384 actors, 64 kept files a song
   (images decoded at 1,280 pixels a side at most), the VM's own memory
   limit (`ddi_lua::Settings`). A script whose Lua errors while it loads
   draws nothing (StepMania's empty actor: black on the first layer). One
   that ends the VM (its fuel, its memory, the deadline, a host limit),
   at load or in play, stops all of the song's scripts for the rest of the
   play: they draw nothing and the first layer shows the song background
   where they were; in play one line in the results' notes, and the log.
   A command that errors is reported and the play goes on, as in
   StepMania.
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
6. Later: `BGAnimation.ini`, `default.xml` and 3.0-style layer folders drawn
   natively (decision 4); the global `BGAnimations/` folder; `Sound`
   actors on the play's mixer; render targets within a pixel budget;
   movies in scripts.

## Status (2026-10-21)

Steps 1 to 5 done, the first slice: folders with a `default.lua` on both
background layers and as foregrounds, on both shells.

- **Host** (`noteskin`): `Skin::song` (a song's folder at
  `/Songs/<pack>/<song>/`, no fallbacks, no metrics), `LuaSkin::actor_file`
  (the prelude's new `actor` entry: `LoadActor` of a path, a background in
  the `UpperLeft` effect's frame with its `diffuse(Color1)`), `song.lua`
  (`SCREEN_*`, `_screen`, `GetSongDir`, `Center`, `CenterX`, `CenterY`,
  `FullScreen`, `align`, `scale_or_crop_background`); natively in
  `ddi-actor`: `stretchto`, `scaletocover`, `scaletofit`, `zoomtowidth`,
  `zoomtoheight`, `InitState` (`Actor::init_state`) and
  `GetTweenTimeLeft` (`Actor::tween_time_left`). Text, render targets,
  proxies, scrollers and sounds load as actors drawing nothing, with a
  note; the rest of the tree draws.
- **Schedule** (`library`): `BgImage::Scripted` on both layers through
  `backgrounds::layers` (the change's transition and `Color1` kept),
  `BgImage::Black` under it on the first while scripts run,
  `backgrounds::foregrounds`, `script_folders` and `script_files` for the
  import, `ManifestEntry::bg_scripts` (the files kept); the import's note
  names only folders made for older versions as not played.
- **Runtime** (`session/src/scripted.rs`): one VM a song, a background
  built once per script and `Color1`; at a change `LoseFocus` on the
  script leaving, `InitState`, `On`, `GainFocus` on the one coming, then
  updated from the change's second; the leaving one updated while its
  transition runs; foregrounds shown from their beat for their tweens'
  time after `On`, hidden for good after; one budget a frame, the stop
  (`stop_note` on the results); a retry starts them over; the
  foregrounds retimed by the song offset.
- **Drawing** (`render/src/scripted.rs`, `shell/src/scripted.rs`): the
  trees on StepMania's screen mapped onto the background frame, quads
  not turned cut to it, backgrounds among the work layers' back quads,
  foregrounds before their mask quads; images registered as a skin's
  (again after a skin replaces them).
- **Shells**: the web's import stores the files as `bga/<path>`, the
  play reads them alongside the start prompt (the play never waits),
  decodes the images, loads the scripts and draws them
  (`crates/web/src/scripts.rs`; `__DDI_DEBUG.scripts`: `running`,
  `stopped`, `fuel`, `back`, `front`); desktop reads them in place when
  the song is set (`View::load_scripts`).

Checked: native tests (the host on synthetic scripts: a frame, a quad,
a sprite of the folder's, `FullScreen`, `Center`,
`scale_or_crop_background`, a tween run by `On`, a script that errors, one
that loops; the schedule, the folders and files kept, the foregrounds;
the runtime on a fake clock: a foreground's lifetime, a background's
`InitState` and `On`, a runaway stopping them all; the quads cut to the
frame), offscreen on desktop (`crates/desktop/tests/play_scripts.rs`: the
song background, then the script's green screen with the folder's red
star left of the field, the foreground's white square for its second,
and a runaway script stopping them all for the song background and a
results note), and a synthetic pack imported as a zip in headless Chrome
(WebGPU, WebGL2) and Firefox, sampled as for the named effects, the
autoplay all top tier.

Since (2026-10-21, second slice): the web loads the scripts in slices
(`Scripts::load_sliced`, the page's turn every `LOAD_SLICE` fuel, as a
skin's Lua loads), checked natively (the host's turns counted) and with a
script spending about a million and a half loop turns at load in headless
Chrome (no frame later than 17 ms while it loads) and Firefox (50 ms, its
Playwright build), the play then drawing it as before.

Not yet: the images' decoded pixels have no budget of their own beyond the 64 files;
a script's quads draw over the second image layer; the transitions fade
a script whatever their kind; scripts read only their folders' files
(not the song's other files); no sound, movies or render targets; the
3.9 and ITG formats and 3.0-style layer folders (step 6).

`BitmapText` since (2026-10-21, second slice, decision 4): the
converter makes a song's `BitmapText` a run of text (`Kind::Text`); the
shared view measures every tree's runs by the shell's drawings of them
(`ScriptView::measure`: a run not drawn yet is guessed and asked for,
`ScriptView::add_text` forgets the guesses), and draws each run as its
drawing over its line box (`ddi_render::scripted`). Desktop draws them
with cosmic-text (`play::stage::text_image`, the ones `Init` leaves when
the song is set, later ones in the frame), the web through a canvas
(`text_bitmap`, arriving a frame or so later). Checked: native tests (the
converter, a run's quad), offscreen on desktop (yellow text left of the
field from the change on) and in headless Chrome (WebGPU, WebGL2) and
Firefox (the same, with a screenshot looked at).
