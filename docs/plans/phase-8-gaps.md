# Phase 8's last gaps: movies wherever StepMania plays them, and backgrounds of our own

Status: planned 2026-11-03; steps 1–3 need no choices, steps 4–5 wait
for the maintainer's (below, "For the maintainer").

Phase 8 (Presentation, `docs/PLAN.md` section 5) is done except for
two gaps listed in "Known gaps before phase 9":

- **Movies StepMania plays and we don't.** These are a movie as a named
  effect's second file, and a movie sprite in a song's script; the
  design for both was written on 2026-10-22 in
  `scripted-backgrounds.md`, "What they need". Reading StepMania 5.1
  for this plan found two more (*new*). A song with no background
  changes gets random movies when the player has a `RandomMovies`
  folder, and a `-random-` change plays from the same pool. Today both
  show the song background. PLAN.md's gap list wrongly said
  `-random-` picks images.
- **The generative backgrounds and visualizers** of the phase's
  deliverable ("built-in generative visualizers driven by
  beat/judgement events", section 3.4), never started. StepMania 5.1
  has nothing to follow here: its visualizations are three named
  effects, which we already draw. So this part is ours to design.

## What StepMania 5.1 does

Sources: StepMania 5.1, `5_1-new` at `825467b` (MIT):
- `src/Background.cpp`: `BackgroundImpl::LoadFromSong`, `LoadFromRandom`,
  `Layer::CreateBackground`, `CreateRandomBGA`, `Update`.
- `src/BackgroundUtil.cpp`: `GetGlobalRandomMoviePaths`,
  `GetGlobalBGAnimations`.
- `src/PrefsManager.h`, `src/Song.cpp` (`TidyUpData`, `HasBGChanges`).
- `Themes/_fallback/metrics.ini`, `[Background]`.
- `BackgroundEffects/Visualization2File.lua`,
  `Visualization2FileFlash.lua`, `SongBgWithMovieViz.lua`.

StepMania 3.9 (`v390release`) for what visualizations once were. All of
this was read from the code, not seen running, and is described here,
not copied.

- **Random backgrounds.** A song gets them when it has no changes on
  either layer.
  - **When they don't apply:**
    - backgrounds off, or brightness 0;
    - the static-background option;
    - the folder's single movie, which `TidyUpData` makes a change at
      beat 0 (our `implicit_movie`).
  - **Modes:** `RandomBackgroundMode` is Off, Animations or
    RandomMovies; **RandomMovies is the default**.
  - **The pool:**
    - `RandomMovies/<pack>/` if it holds movies, else `RandomMovies/`.
    - Only `.ogv`, `.avi`, `.mpg` and `.mpeg` count.
    - `RandomMovies/<pack>/BackgroundMapping.ini`'s `[GenreToSection]`
      narrows it by the song's genre, unless nothing matches.
  - **The order:** shuffled by a generator seeded from the song's
    folder (the same order every play), cut to `NumBackgrounds` (10),
    and handed out round-robin, never reshuffled.
  - **When they change:** the span runs from the first to the last
    note of the song's charts. Within each time signature there is a
    change every `RandomBGChangeMeasures` (4) measures of it, counted
    from the later of the segment's start and the first note. A BPM
    change on a measure start adds one more.
  - **The first and the last:** the first change goes at beat −1000,
    so it already shows in the lead-in. At the last beat the song
    background comes back.
  - **How they show:** every switch is a cut, at rate 1, as
    `StretchNormal` (covering, looping; a movie that comes round again
    resumes).
  - **An empty pool** leaves the song background. Stock StepMania ships
    no `RandomMovies`, so a plain install shows the song background.
- **`-random-`** (layer 1 only).
  - **Its span:** each marker is replaced by the same expansion, from
    its beat to the next change's (or the last beat). It draws from
    the same pool, continuing its rotation.
  - **What each change keeps:** the marker's rate, transition (a
    crossfade marker fades every switch) and effect. The second file
    and colors are dropped (the BPM-change path keeps the colors).
  - **Quirks:**
    - In the first time signature, the first generated change takes
      beat −1000 and sorts before the earlier changes, so the old
      picture stays for the marker's first 4 measures.
    - A last `-random-` after another ends before it starts and
      produces nothing.
    - On layer 2 the marker is never loaded, and the layer keeps its
      picture (we already do this).
- **A layer-1 file found nowhere,** in a song with changes, is replaced
  by the pool's next item, with its effect dropped. With an empty pool
  the song background shows, as we do now.
- **Visualizations.** 5.1 has no mode for them. `VISUALIZATIONS_DIR` is
  defined but never read. What is left is the three effects above,
  movies on the music's clock.
  - `Visualization2File`: the second file added over the first,
    stretched to the screen.
  - `Visualization2FileFlash`: the second file added and fading out
    over 1 s at each `On`.
  - `SongBgWithMovieViz`: the song background with the file added
    over it.

  3.9's `MOVIEVIS` mode added a random movie from `Visualizations/` over
  the song background every 4 measures. It is gone.
- **Game events.**
  - **Danger:** `BackgroundImpl::Update` broadcasts `ShowDangerAll` and
    `HideDangerAll` when every player is in danger (not with failing
    off). Nothing stock listens; a song's script may.
  - **Dancing characters:** none ship with a model.
  - **The fail:** the background does not change.

## Decided here (the maintainer may change any)

1. **Random movies follow 5.1, on by default.** This changes nothing
   for a player without a `RandomMovies` folder.
   - **The setting:** Options → Display → Random movies, On or Off,
     5.1's RandomMovies mode against Off. Its Animations mode is left
     out: stock `BGAnimations` holds four flashes.
   - **The other settings:** Background videos Off turns them off too,
     and brightness 0 shows none.
2. **5.1's quirks are kept** (the −1000 first change, the last
   `-random-` after another): packs were checked on StepMania. The
   shuffle is ours, not 5.1's generator and hash. It is seeded by the
   song's pack and folder names, so it gives one order a song on both
   shells (SplitMix64, as decision 14's shuffle).
3. **StepMania's movie clocks.**
   - **An effect's second file** runs on its segment's clock (both
     files load when the change's background is made).
   - **A script's movie** runs from its load on the scripts' own frame
     time, so a replay draws the same.
   - **A random movie** runs as any `StretchNormal` segment.
4. **A generative background stands in for a missing song
   background** (the maintainer, 2026-11-03).
   - **Where `BgImage::Song` would show and there is none,** it shows:
     a song with no `#BACKGROUND` and no changes; before its first
     change; after its return at the last beat; a file that is missing
     or fails to decode.
   - **Random movies come first,** where 5.1 plays them (step 1).
   - **The field filter's band** shows over it, as over any background.
     The plain ground, and no band, stays for what is explicitly
     without a background.
5. **A Dancing☆Onigiri work is explicitly without a background.**
   - **Its back layers are its background** (`back_data`, decision 41).
     danoniplus has no background picture of its own to import: a work
     page's own CSS backdrop is not read.
   - **So a work never gets a generative background, nor random
     movies.** It plays on the plain ground with no band, its layers
     over it when it has them, as now.
   - **How a song says so:** its source format (`SourceFormat::Danoni`),
     which every stored work already has, so there is no field to add
     or migrate. One function asks it (`backgrounds::stands_in`). Step 5
     checks that a work the chart editor saved stays so; a `.sm` with
     `danoni-<mode>` charts is a StepMania song and keeps its own
     `#BACKGROUND`.
6. **Generative backgrounds are presentation only and deterministic.**
   - **What drives them:** song time, the play's events, and the music
     analyzed when the play loads. Never a live analyser, so a replay,
     the README's capture and both shells draw the same.
   - **Never in:** calibration (nothing may cue the player), the chart
     editor's field, or Options' preview.
7. **Light.**
   - Random movies use the movie slots as they are: at most the current
     and the next movie, as now.
   - A generative background is one pass, its cost measured on both
     backends; reduced motion holds it still.

## Steps

### Step 1. Random movies and `-random-` (`ddi-library`, both shells)

- **The pool** (`backgrounds::random_pool`): from the library's shared
  movies. On the web, the shared folders' movies are already kept under
  `import_videos`; desktop reads its roots' `RandomMovies/` in place.
  The genre whitelist and the seeded order as above.
- **The schedule** (`first_layer`):
  - random changes for a song without changes, on 5.1's grid;
  - the `-random-` expansion;
  - a missing layer-1 file replaced from the pool.

  Never for a Dancing☆Onigiri work (decision 5).

  These are plain `BgImage::Movie` segments, so both shells' movie
  slots, textures, frame and automatic video rule play them unchanged.
- **The setting** `random_movies` (on). Song select and the prompt are
  unchanged: they show the song's picture.
- **Checks:**
  - native tests on hand-written songs: the grid across time
    signatures and BPM changes, the first change in the lead-in, the
    last beat, the quirks, the pool's order and genres;
  - desktop offscreen with generated numbered clips in a scratch
    `RandomMovies/`;
  - headless Chrome importing the same as a folder and a zip (WebGPU,
    WebGL2), and Firefox.

### Step 2. A movie as an effect's second file

- **The schedule:** `look_of` resolves a second file among movies too,
  with the movie effects' rules for the second part.
- **The movie slots:** `movie_segments_at` and `OpenMovie` are keyed by
  `(layer, segment, part)`. The second part shares its segment's clock
  (`movie_position_in`), and its texture is found by name as now.
- **The renderer:** the named effects already sample the backdrop's
  textures (RGBA, planar YUV, NV12, the web's WebCodecs copies), so
  `Visualization2File`, `Visualization2FileFlash` and the `File2*`
  effects take a movie as they take a picture.
- **Checks:** numbered-color clips as a `Visualization2File` second
  file, read back natively, offscreen on desktop and in headless
  Chrome (WebGPU, WebGL2) and Firefox.

### Step 3. A script's movie sprite

As designed on 2026-10-22 (`scripted-backgrounds.md`, "What they
need"):
- **The clock:** one per movie file, shared by every sprite showing it,
  on the scripts' frame time, with `play`, `pause`, `rate`, `loop` and
  `position` (to 0).
- **The movie slots:** a third kind of wanted entry, by file, opened
  while a sprite showing it is drawn, at most two a song.
- **The textures:** each new frame converted into an RGBA texture the
  textured pipeline samples, one pass for both shells (as render
  targets are drawn, `TexturedPipeline::draw_targets`).
- **The import:** a script folder's movies go through the movie sniff
  (stored or linked as `import_videos` says), and the converter keeps a
  movie sprite's file as it keeps a `Sound`'s.
- **While there:** `ShowDangerAll` and `HideDangerAll` broadcast to a
  song's scripts as 5.1 does (`…MessageCommand`).
- **Checks:** a script's sprite of a numbered clip, playing, paused and
  at a rate, read back as in step 2; a danger message seen by a test
  script.

### Step 4. Generative backgrounds: the look (a design study)

A browser study in `tools/background-design/`, as the HUD's and the
title's were (`tools/play-hud-design/`): candidates the maintainer
compares and picks. It is driven by a recorded play's events and a
song's analysis, with every parameter live. The accepted one is kept as
`approved.json` and ported in step 5. What it explores depends on the
maintainer's answers below.

### Step 5. Generative backgrounds in play

- **The music's analysis** at load: a few bands about every 20 ms, from
  the editor's analysis (`ddi_ui::state::editor::audio`, made from the
  samples heard). The web steps it in slices, desktop runs it on a
  thread. Start never waits: until it is in, the pass runs on the beat
  and the events alone.
- **The stand-in rule** wherever the song background shows with none
  there (decision 4), never for a work (decision 5).
- **The pass:** in `ddi-render`, drawn in the background frame. The
  field filter treats it as a background.
- **Settings and screens:** the setting, reduced motion, and the song
  select → prompt → play backdrop (`crates/actor/src/background.rs`).
- **Checks:** natively, offscreen on desktop (pixels at set times of a
  replay), headless Chrome (WebGPU, WebGL2) and Firefox, the frame cost
  on both.

### Step 6. Documentation

- `docs/PLAN.md`: phase 8's row and gaps, a decision for the
  generative backgrounds.
- `scripted-backgrounds.md`, `overlays.md`, `video-backgrounds.md`:
  their "left out" notes.
- AGENTS.md, the verify skill, `docs/plans/README.md`.

## For the maintainer

Where it shows was settled on 2026-11-03 (decisions 4 and 5). Steps 4–5
still need these answered:

1. **On by default?** It would show for every song without a
   background, under a Display setting.
2. **Also over pictures?** As a visualizer added over a song's own
   picture, chosen in Options; off by default.
3. **Its look.** Either Infinito's language (the Vivid palette, the
   outlines, the menu's rain), so the play matches the menus, or
   something apart, found in the study.
4. **What drives it:** the beat and measures, the play's events
   (judgments, combo hundreds, danger, the full combo), the music's
   loudness and bands, or a subset.
5. **FFR levels** have no background either, in FFR's own game: a
   generative one, or explicitly none like the works?
