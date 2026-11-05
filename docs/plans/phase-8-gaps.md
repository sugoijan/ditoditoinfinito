# Phase 8's last gaps: movies wherever StepMania plays them, and backgrounds of our own

Status: in progress, steps 1–3 done 2026-11-03, step 4's study ready the same day, its choices exported 2026-11-05 and step 5 done the same day; planned 2026-11-03, the
maintainer's choices taken then ("The maintainer's choices", at the end).
Steps 1–3 follow StepMania; step 4 is a round of experiments the
maintainer picks from.

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
6. **Generative backgrounds follow the music alone** (the maintainer,
   2026-11-03).
   - **What drives them:** the music analyzed when the play loads (its
     loudness and bands, and whatever else step 4 finds worth it) and
     the song's beats and measures, which its timing writes down. Never
     the play's events (judgments, combo, life), and never a live
     analyser.
   - **So it draws the same everywhere:** a replay, an autoplay, the
     README's capture and both shells draw the same picture for the
     same song. Each browser's decode may differ by a sample, which does
     not show.
   - **Never in:** calibration (nothing may cue the player), the chart
     editor's field, or Options' preview.
   - **On by default** for every song without a background (decision
     4). An option also adds one over a song's own picture, as a
     visualizer; that one is off by default.
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

**Done 2026-11-03.**
- **Where it lives:**
  - **The library:** the rules are in `ddi_library::backgrounds::random`.
  - **The schedule:** `backgrounds::layers` takes the pool. The first
    layer plays it in StepMania's order: missing files first, then each
    `-random-` change; for a song without changes, its whole span.
  - **The setting:** Options → Display → Random movies
    (`Settings::random_movies`, on).
  - **The web:** the import keeps a pack's `BackgroundMapping.ini`, and
    a song's load finds its pool (`songs::random_found`).
  - **Desktop:** reads the pool from its StepMania root in place
    (`library::random_movies`).
- **Choices made on the way:**
  - **Imported and library songs only.** Bundled songs keep their own
    backgrounds on both shells, and a song shared by link is what was
    shared.
  - **A random movie's name is its shared path**
    (`RandomMovies/RM/red.avi`). No song file can be confused with it,
    and both shells open it by that name.
  - **What takes no random movie,** as StepMania finds these:
    - a file that exists but cannot play here (the song background, as
      before);
    - the four stock `BGAnimations` flashes;
    - a song whose one movie cannot play.
  - **When there are none:** with Background videos off there are no
    random movies, so a missing file shows the song background rather
    than a movie that never loads.
  - **The last beat now follows StepMania's `GetLastBeat`**
    (`random::span`) for the song background's return too. It leaves
    out generated charts and an edit beside other charts, and reads
    each chart on its own timing.
  - **Desktop now plays shared-folder files a change names**
    (`library::shared_found`), as the web always did. Otherwise such a
    change would get a random movie on desktop and its own movie on the
    web.
  - **The import's warning** about files found nowhere now says a random
    movie may take their place.
  - **The web's `__DDI_DEBUG.backdrop`** names a movie by its file, not
    its texture id.
- **Left as it is:** images past a song's 32-image budget count as found
  nowhere, so with a pool they get a random movie. StepMania has no
  budget.

Checked:
- **The library's tests:** the grid across time signatures and BPM
  changes, the lead-in, the last beat, both quirks, rate, transition and
  effect kept, missing files, the pool's folders, extensions, order and
  genres, a work, the span.
- **Desktop:** a library test on a generated StepMania root (the pool,
  the files found, a change's shared movie, a missing file's random one,
  off), and an offscreen play decoding a random movie's frames from the
  start (`play_movie`). The whole desktop suite passes.
- **Headless Chrome** importing a pack and a `RandomMovies` folder of
  colored clips at 240 BPM:
  - a song without changes alternates blue and red at 0, 5, 9 and 13 s,
    then shows the ground at 17 s;
  - a change's `fire.avi` plays, then a random movie for its missing
    file;
  - a genre mapped to one movie keeps it alone;
  - with the setting off, nothing.
- Firefox and Safari by the maintainer.

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

**Done 2026-11-03.**
- **The schedule:** a change's second file resolves among the song's
  movies too (`backgrounds::look_of`). The change's rate is kept when
  either file is a movie, since StepMania sets it on the whole
  background (`SetUpdateRate`), and each effect's script passes it to
  both files' textures.
- **The clock** (`backgrounds::movie_position_of`,
  `movie_loops_of`): StepMania's effects play a second file as
  `StretchNormal` plays a first one (`File2Normal.lua`,
  `Visualization2File.lua` and the rest: the background's rate, played
  while current, paused after, never put back to its start). So a
  file's clock counts every change showing it as either file. Only a
  change showing it as its own file restarts it or stops its looping.
- **The slots** (`backgrounds::movie_segments_at`,
  `ddi_session::movies`):
  - Decoders are keyed by `(layer, segment, part)`
    (`backgrounds::movie_of`).
  - A file current twice (both files of a change, or both layers) is
    decoded once.
  - A next change's movie that runs on from a current one gets no
    decoder ahead: the current decoder moves to it at the change. A
    visualization over changing pictures would otherwise open a second
    decoder at every change.
  - The automatic video rule watches a second file's movie as it does a
    first file's.
- **Drawing:** unchanged. The effects already found a second file's
  texture by name, and a movie's texture is one more.

Checked:
- **Tests:** the library's (the second file resolved and its rate kept,
  its clock across a pause and a restart, looping, the decoders wanted,
  the drawn movie) and the slots' (a second file's decoder opened ahead,
  its frames shown, carried into the next change).
- **Desktop offscreen** (`play_layers`): red under `Visualization2File`
  with a yellow clip added reads (255, 193, 30), as expected in linear
  light.
- **Headless Chrome on WebGPU and WebGL2,** an MPEG-4 clip through the
  decoder module and an H.264 one through WebCodecs: (255, 194, 30) at 3
  and 5 s, blue before and after. Before this change the red alone
  showed, (200, 30, 30).
- **Playwright's Firefox** (WebGL2): the same with the module, (255, 183,
  31) with H.264 (Firefox's own colors for SD movies, a known gap).
- Firefox and Safari by the maintainer.

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

**Done 2026-11-03.**
- **The actors** (`ddi_actor`): a movie's sprite carries
  `movie_texture(index)`, above the render targets' ids. While it
  animates and decodes, it hands each update's effect time to its
  movie (`Sprite::take_movie_seconds`), except the first update of a
  new sprite and the first after `play` (StepMania's
  `m_bSkipNextUpdate`).
- **The Lua host:**
  - A song's script loading a movie gets that sprite, at the size the
    file's header says (`ddi_library::video::dimensions`: AVI, MP4 with
    its index first, Matroska/WebM, Ogg Theora, MPEG), else 640 × 480
    with a note. At most 8 movies a song.
  - `GetTexture()` gives the movie's one texture, with `rate`, `loop`,
    `position` (0 only, as StepMania), the sizes and `GetPath`.
  - Also `SetTexture` of a movie's texture, `SetDecodeMovie` and
    `GetDecodeMovie`.
- **The clock** (`ddi_session::scripted`): each frame, every sprite of
  a file adds its time to the file's clock at the texture's rate
  (`UpdateMovie`). A retry rewinds them.
- **Decoding:**
  - The movies the frame's quads drew are decoded on their clocks
    (`MovieSlots::update_clips`, a deck of their own on each shell).
  - Their new frames are drawn into textures the quads sample
    (`ddi_render::script_movies`). It uses a converter of its own, since
    the backdrop's drawing keeps per-frame state.
  - A movie's texture is made with its first frame, so it draws from the
    next frame on. Background videos Off decodes none.
- **Danger:** `ShowDangerAll` and `HideDangerAll` reach the scripts when
  the play's danger changes (`PlaySession::danger_shown`).

Choices made on the way:
- **The import is unchanged.** A script folder's movies were already
  kept with its files (`bga/<path>`, within the 64 files) and read by
  the decoder's own probe. They are not sniffed, and the "also store
  background videos" option does not apply to them, unlike step 3's
  first plan.
- **StepMania 3.9's formats' movie layers** (natively built animations)
  still draw nothing: only Lua scripts show movies.

Checked:
- **Tests:** the actor's (time given and skipped, decoding off, an
  image's sprite), the host and runtime's (one clock for two sprites of
  a file at rate 2, the file's size, a retry rewinding it), the slots'
  (a clip opened on its clock, a clock put back seeking, closed when not
  drawn), the quads' (a movie's texture drawn, marked drawn before it is
  made), the danger told once a change, header sizes against `ffmpeg`
  clips of each container.
- **Desktop offscreen** (`play_scripts::script_movies_play_in_their_sprites`):
  a clip turning from red to blue after 1 s over the screen, and the same
  clip held by `rate(0)` in a strip. Red, red at 0.6 s; blue, red at 2 s.
- **Headless Chrome** (WebGPU, WebGL2) and Playwright's Firefox, the same
  song imported as a zip: red until 1 s into the movie, blue after, the
  held strip red throughout. Firefox's strip was black in the change's
  first frame, before the decoder module's first frame of it.
- Firefox and Safari by the maintainer.

### Step 4. Generative backgrounds: the look (a design study)

A round of experiments in a browser study, `tools/background-design/`,
as the HUD's and the title's were (`tools/play-hud-design/`). The
maintainer compares them and chooses which visualizations the game
supports.

- **What drives the study:** a song's music, analyzed in the page
  (Web Audio's decode, the same analysis as step 5's), with its beats
  from a simfile's timing, playing along, and every parameter live.
- **The candidates** (the maintainer's first ideas):
  1. **Spectrum bars**, an overlay on a base background (the song's
     picture, or another visualization).
  2. **A kaleidoscope** of a background image, turning and folding with
     the beat. The study tries it on the song's picture, and shows
     what a song without one could fold instead (its banner or jacket,
     else none).
  3. **An abstract field** like the HUD's turbulence, but quiet in
     color, with wavy lines whose waves follow the spectrum.
  4. **The start menu's outline rain** (`crates/theme/src/rain.rs`), some
     outlines' lines vibrating with the music.
- **Color:** sparing with Infinito's Vivid palette, which the screens
  already lean on. The study offers quiet palettes (a few hues, mostly
  dark and monochrome) and keeps Vivid as one choice among them, not
  the default.
- **What the music gives,** each shown as a live trace so its worth can
  be judged:
  - loudness;
  - bands, log-spaced;
  - onsets (spectral flux: where a sound starts, drums above all);
  - brightness (the spectral centroid);
  - stereo width and balance;
  - the waveform itself, for an oscilloscope;
  - sections (quiet stretches, builds, drops), from a slow loudness.

  The beat and measures come from the simfile.
- **What comes out:** the visualizations kept and their parameters
  (`approved.json`), whether the player chooses one or the game picks
  one a song, and which ones work over a picture. Step 5 ports what is
  chosen.

**Study ready 2026-11-03** (`tools/background-design/`, its README).
- **The page:** the four candidates, one at a time or side by side, on
  a bundled song or files picked. Each candidate's parameters are live,
  with drivers among the music's signals.
- **What the music gives,** traced under the stage: loudness, bands,
  onsets, brightness, stereo width, balance, sections and the waveform,
  with the beats and measures from the simfile.
- **Context:** quiet palettes, Vivid among them. Options to draw over
  the song's picture at the game's background brightness, the field's
  band and receptors with its filter, and reduced motion.
- **Choosing:** what to keep and what works over a picture, exported as
  `approved.json`.
- **Checked** in headless Chrome and Playwright's Firefox: the song
  analyzed (Beatucada: 447 and 445 onsets, each browser's decode),
  played, each candidate and the grid drawn, no console errors.

**Revised 2026-11-05** on the maintainer's first look:
- **The kaleidoscope** moved its whole picture with the music, and
  looked like a picture flipped. Its mirrors now stay still and the
  picture under them moves (turning with the beat, growing with the
  bass, wandering). Three mirrors tile the screen with triangles, the
  classic kaleidoscope's many reflections; a rosette of two remains a
  choice. Each shows a small piece of the picture, or pieces of glass
  in the palette for a song without one.
- **The bars** run across the whole screen, behind the field's band
  too, without rounded ends. Segmented bars with peaks held are the
  default; solid ones remain a choice.
- **The rain's moving outlines** do one of two things, chosen at random
  for each as it comes in: zoom with the bass, or wave round their
  outline with the treble. Both take what stands out of the song (a
  signal above its median over the song), since the bass alone sits
  high most of the time. The game's analysis has the whole song at
  load, so it can measure the same.

### Step 5. Generative backgrounds in play

- **The music's analysis** at load: what step 4 kept (bands, loudness,
  onsets…) about every 20 ms, from the editor's analysis
  (`ddi_ui::state::editor::audio`, made from the samples heard). The
  web steps it in slices, desktop runs it on a thread. Start never
  waits: until it is in, the pass runs on the beat alone.
- **The stand-in rule** wherever the song background shows with none
  there (decision 4), never for a work (decision 5).
- **The pass:** in `ddi-render`, drawn in the background frame. The
  field filter treats it as a background.
- **Settings and screens:** Options → Display. The generative
  background is on by default where a song has none; the visualizer
  over a song's picture is off by default; there is a choice of
  visualization if step 4 keeps several. Also reduced motion, and the
  song select → prompt → play backdrop
  (`crates/actor/src/background.rs`).
- **Checks:** natively, offscreen on desktop (pixels at set times of a
  replay), headless Chrome (WebGPU, WebGL2) and Firefox, the frame cost
  on both.

**Done 2026-11-05.** The maintainer's exported choices are
`tools/background-design/approved.json`: the player chooses the look,
from the four the study tuned (no candidate was ticked, so all four are
kept; the field and lines is the default, Ink the default palette).
- **The analysis** (`ddi_library::music`): the study's, less what its
  choices left unused: 32 bands from 30 Hz to 16 kHz of a 2048-point
  transform and the loudness, a frame every 20 ms, keeping only the
  samples the frames to come need. Its transform is the editor's, moved
  to `ddi_library::fft` for both. Desktop makes it on a thread from the
  decoded music, the web in 3 ms slices an animation frame from the
  prompt's decode (`crates/web/src/generated.rs`, at the buffer's MP3
  lead). Only where the look may show: a song without a picture, or
  over one.
- **The looks** (`ddi_library::generative`): the approved values as
  constants, lengths in pixels of a 540-pixel screen as the menu's rain
  measures. `Series` makes every look's signals once over the analysis
  (the bars' rise, fall and held peaks, the lines' bands, the rain's
  zoom and waves on what stands out of the song, the kaleidoscope's
  wandering clock), so a frame is a function of the song second and
  beat: a replay, both shells and any refresh rate draw the same.
- **The pass** (`ddi_render::generative`, `generative.wgsl`): the look
  over the whole texture (ground, field, kaleidoscope, the song's
  picture over which the bars draw), then strokes (the field's lines,
  the rain's outlines, sampled from the menu's art) in encoded values,
  as the study's canvas blended them. The texture is plain RGBA8 on any
  target and registered with the backdrop as a picture of its own,
  self-lit (`BackgroundPipeline::add_generated`), so it crossfades,
  enters, takes the field filter's band and the blurred base as any
  picture does. Its brightness: as designed at the default background
  brightness (40%), dimmer under it, none at 0.
- **The stand-in** (`ddi_session::movies::BackdropTextures::set_generated`):
  the generated texture is the song background's last resort, or its
  first over a picture. Never for a work, in calibration, the editor or
  Options' preview.
- **Settings:** Options → Display → Generated background (Off, Spectrum
  bars, Kaleidoscope, Field and lines, Outline rain), Over song
  backgrounds (off), Generated colors (the study's quiet palettes).
- **Size and cost:** the background frame's size, within the device's
  largest texture.
- **Checks:** `crates/desktop/tests/generated_background_offscreen.rs`
  (each look on a song without a background, the band, the same picture
  for the same second, Off and brightness 0, the bars over a picture
  leaving it as it was); the library's, render's (the shader validated
  and written for WebGL2) and the shell's unit tests. In headless Chrome
  (WebGPU and WebGL2) and Firefox (WebGL2): an imported song without a
  background autoplayed with each look and Vivid, its 20 s analyzed
  before the play began (1000 frames), and Beatucada with the bars and
  the kaleidoscope over its picture, no console errors of their own
  (`target/scratch/gen/gen.mjs`; the verify skill).

**Revisited 2026-11-06** with the maintainer, the choices made above
one by one:
- **Kept:**
  - all four looks, the player choosing;
  - the field and lines and Ink as the defaults;
  - every look over a picture with Over song backgrounds;
  - the kaleidoscope's note in the export, settled by the revision;
  - the brightness, full at 40% and dimmer under it;
  - the texture made for a song with a picture, should it fail (the
    look then on the beat alone);
  - the export's place.
- **Vivid left out** of Generated colors: the screens use enough of it.
  A saved Vivid reads as Ink.
- **The kaleidoscope folds the song's cover** where it has no
  background: its jacket, else its banner (`BgImage::Cover`, read with
  the song's pictures only for the kaleidoscope standing in), else
  glass.
- **The start prompt shows the look's first frame held still** in the
  picture's place: the frame at the play's first second (−0.8 s), self-lit
  as the play draws it, so the entrance carries on from it as from a
  picture. Each shell draws it into the 2D pass's textures when the
  pass asks for its source (`ddi_shell::generative::Stills`), at each
  size asked, and the pass blurs it as an image it read. It is drawn
  again when the analysis or the cover comes in.
- **The texture is the frame's full size**, within the device's
  largest texture (WebGL2 may allow less than a 4K screen), no longer
  Full HD at most.
- **Checked:**
  - natively: the shell's tests (the cover, the picture folded, the
    still's size);
  - offscreen on desktop: the kaleidoscope folding a jacket in play,
    the other looks never drawing it, and the prompt through the host
    (`generated_prompt_offscreen.rs`: the field's still lit, the
    kaleidoscope's glass blue gray and a jacket red, none when Off);
  - in headless Chrome (WebGPU and WebGL2) and Firefox (WebGL2): each
    look's prompt with its still (`prompt_backdrop` `generated`), the
    play's entrance carrying on from it, a song with a `jacket.png`
    folded, a saved Vivid played as Ink, none when Off, and Beatucada
    with its own picture on the prompt and a look over it in play; no
    console errors of their own.

### Step 6. Documentation

- `docs/PLAN.md`: phase 8's row and gaps, a decision for the
  generative backgrounds.
- `scripted-backgrounds.md`, `overlays.md`, `video-backgrounds.md`:
  their "left out" notes.
- AGENTS.md, the verify skill, `docs/plans/README.md`.

## The maintainer's choices (2026-11-03)

- **Where:** in place of a missing song background; never for a
  Dancing☆Onigiri work, whose back layers are its background
  (decisions 4 and 5). The point of the plain ground was that no
  background means no band, which stays so for the works.
- **On by default** for songs without a background; **over a picture**
  as an option.
- **The look:** a round of experiments first (step 4), with the four
  ideas above. Not much more Vivid: the screens already come close to
  using it too much.
- **What drives it:** the music alone (loudness and bands first;
  step 4 shows what else it gives), not the play's events.
- **FFR levels:** to be decided when the maintainer tries FFR imports.
  Until then they follow the general rule (they have no background, so
  they get the generative one).
