# Working on Dito Dito Infinito

Orientation for coding agents (and humans) on this repository. Keep it
accurate: when a convention here changes, change this file in the same commit.

## What this is

A dance rhythm game that plays StepMania simfiles; in genre terms a 4K VSRG
(four-key vertical scrolling rhythm game), which is the vocabulary to use when
researching comparable games or writing public copy. Written in Rust and shipped
as a static web app (wasm via Trunk, deployed to GitHub Pages) and as a
desktop app (`cargo run`), the same game: what a play does is shared code
both shells run. `docs/PLAN.md` holds the
architecture, decisions and roadmap; `docs/plans/` the plans for each piece
of work, indexed with their status in `docs/plans/README.md`; `docs/research/` holds the sourced notes
on formats, rules and the tech stack. Read those before changing the engine or
the importers.

Public copy (page title, descriptions, README lead, landing-page entry) says
"dance rhythm game". Other games may be named to explain supported file
formats, layouts, rulesets or accurate gameplay comparisons (StepMania
simfiles, Pump It Up-style five-panel charts, the DDR A ruleset); do not
imply official affiliation, bundled commercial song catalogs or exact
emulation. Keep the title focused on this game's own name and genre.
Rust and the browser are implementation details, not advertised features.

## Layout

- `crates/chart/` chart model and importers (`.sm`/`.ssc`; FFR's `beatBox`,
  `formats/ffr.rs`), 48 ticks per beat, exact
  StepMania timing semantics, and the `.ssc`/`.sm` writer the chart
  editor saves with (`formats/sm_write.rs`: what is read is written back
  equal). Pure, natively tested.
- `crates/engine/` deterministic gameplay: clock, judge, rulesets, scoring, gauges,
  scroll, calibration maths. No I/O. Replay fixtures in `crates/engine/tests/`.
- `crates/render/` wgpu scene (procedural SDF arrows, glyphon text) and the 2D pass
  the menus move onto (`docs/plans/ui-core.md`): a draw list, its painter,
  the texture cache and the text atlas (runs measured and drawn by the
  platform: the browser on the web, cosmic-text natively; drawn over a
  play too, through an sRGB surface where there is no plain view, for
  Dancing☆Onigiri's lyrics, `crates/render/src/lyrics.rs`),
  cached menu Gaussian images (`crates/render/src/gaussian.rs`, recorded once
  on upload before mipmaps, keyed separately from sharp images), Options'
  field preview (`crates/render/src/field_preview.rs`: the demo chart drawn as
  play draws it into a texture the 2D pass shows) and song select's
  preview movie's picture (`crates/render/src/movie_picture.rs`, the same
  way) and the chart editor's field in the player's skin
  (`crates/render/src/editor_field.rs`: a play's frame paused at the
  cursor, the skin's clock at rest; `ddi_ui::screens::editor_field`
  places its notes and the overlay over it; its audio view's picture,
  `crates/render/src/editor_audio.rs`); StepMania's named
  background effects drawn from `ddi_library::backgrounds::BgDraw`
  (`crates/render/src/background.rs`) and a song's scripts' actors as quads
  in the background frame (`crates/render/src/scripted.rs`); skins'
  materials (`crates/render/src/material.rs`: a WGSL `paint` checked for
  WebGL2 with naga, drawn by the textured pipeline,
  `docs/plans/noteskin-scene.md`). No platform code.
- `crates/platform/` traits and plain data the shells implement (audio clock, input,
  device fingerprints), plus the Ogg Vorbis and MP3 fallback decoder behind
  the `decode` feature (Symphonia, MPL-2.0; MP3 as StepMania decodes it,
  `crates/platform/src/mp3.rs`), Ogg Opus through `rusty-opus` (BSD-3-Clause)
  behind `opus` (`crates/platform/src/opus.rs`: the web's Opus worker and
  desktop's decoder) and, behind `decode-all` (the
  desktop's), every format a simfile's music comes in: MP3 as StepMania
  5.1 decodes it (`crates/platform/src/decode_all.rs`), resampling with rubato.
- `crates/opus-worker/` the web's Ogg Opus fallback (`ddi-opus-worker`, PLAN.md
  decision 9): a worker Trunk builds beside the app (`site/index.html`,
  `data-type="worker"`; `xtask stage-opus`, a post-build hook, names its
  files by their hash and tells the page in index.html's
  `ddi-opus-worker` meta), started by `crates/web/src/web/opus.rs` only when the
  browser cannot decode an Ogg Opus file (the module compiled once a
  session and handed to a worker a decode; one that has not started in
  25 s fails); it sends back a float WAV at 48 kHz that
  `decodeAudioData` resamples. Empty natively.
- `crates/svg-worker/` skins' SVG images on the web (`ddi-svg-worker`,
  `docs/plans/noteskin-svg.md`): a worker Trunk builds beside the app
  (`xtask stage-svg` names its files by their hash, index.html's
  `ddi-svg-worker` meta) drawing them with desktop's code
  (`ddi_noteskin::raster`, resvg, the `raster` feature; the app's own wasm
  goes without it), its rasterizer's crates built at `opt-level = 3` in
  the release profile; started by `crates/web/src/web/svg.rs`, a few at
  once, ended once idle; the browser draws an SVG only where none starts.
  Empty natively.
- `crates/library/` song library logic shared by the app and xtask: pack scanning
  (which file is the chart, music, banner), a sans-IO zip reader, the manifest
  entry type, and the import's core (`scan.rs`: a song's entry, its files
  and the report's warnings from its folder; `works.rs`: a
  Dancing☆Onigiri work's songs, ids, music and layers; the web's import
  runs them on `Blob`s, desktop's library on its folders), FFR levels
  (`ffr.rs`: a song a level file, its folder its pack; the chart and
  music of a SWF, `swf.rs`), a stored song's entry after the chart
  editor saved it or made it new (`edited.rs`), a music file's title
  and artist from its tags (`tags.rs`). Pure, natively tested.
- `crates/actor/` StepMania's actor model, shared by noteskins and later our
  screens (`docs/plans/ui-core.md`): actors, the tween queue, effects,
  sprite states, eases, commands as metrics write them, a model's meshes,
  bones and animated textures, and what is drawn (with our own
  extensions for the theme: sprites' `cornerradius`, `softedge`,
  `outline`, `saturation`; text's `ellipsis` and letter spacing). Pure,
  natively tested;
  the files actors are built from (`metrics.ini`, MilkShape, legacy actor
  files, Lua) are read in `noteskin/`.
- `crates/ui/` the UI core our screens are built on (`docs/plans/ui-core.md`,
  step 3): input events in, requests to the shell out (handled
  synchronously, gesture-bound ones flagged), hit-testing through actors,
  touch and mouse gestures, menu navigation with key repeat and focus,
  screens with transitions, dialogs, messages to actors, the one-line text
  field (mirroring the web's hidden textarea, or editing itself on
  desktop), and a headless harness; the theme's screens
  (`crates/ui/src/screens/`: start menu, song select, results, options, the pages for
  calibration's menu and the credits, the app's notices as the UI's
  chrome, the play's start prompt, `prompt.rs`: Start never waits for
  what loads behind it, a loading screen after Start until the play is
  ready, a countdown on song select's plays, and this play's options,
  `prompt_options.rs`, `docs/plans/start-prompt.md`) with their sample
  data, options' rows made from the settings and the shell every
  platform shares (`options_model.rs`, `options_shell.rs`, the bind
  flow in `crates/ui/src/state/bind.rs`); the state screens read (settings and
  their migrations, the song list's place, song select's data from a
  library (`select_view.rs`), the play
  history and its replays, `crates/ui/src/state/history/`, and when its
  stale plays are judged again, `reworker.rs`; stored through
  `ddi_platform::storage`, which each shell installs), the chart
  editor's model (`crates/ui/src/state/editor/`: edits with undo; the
  music's waveform and spectrogram, `audio.rs`, which its audio view,
  `screens/editor_audio.rs`, draws on the field's rows) and
  screen (`crates/ui/src/screens/editor.rs`, `docs/plans/editor.md`;
  its note record, `state/editor/record.rs`, takes a press's time stamp
  as a play judges it, `ddi_session::listen::Listener::heard_at`) and
  the developer screen both shells run (`crates/ui/src/dev.rs`). Pure,
  natively tested.
- `crates/theme/` our theme (`ddi-theme`, `docs/plans/theme.md`): `Infinito`,
  a look for the UI core whose components are the maintainer's mockups,
  its colors, faces, sizes, timings and places in
  `crates/theme/src/infinito.toml` (turned into tables at build time). The
  start title is shaped Dosis artwork (`tools/title-design/`, `assets/title/`),
  with Infinito Vivid paint on D/D/I through the shared 2D painter; reduced
  motion freezes both its palette and evolving texture. The
  screens it dresses are `crates/ui/src/screens/` (start menu, song select, results,
  options; theme-agnostic, fed plain data). Pure, natively tested.
- `crates/noteskin/` StepMania noteskins: fallback chains, file lookup and
  redirects, `metrics.ini`, file name hints, what each element is (stand-ins
  for StepMania's `default` and `common`), its actors built (`actor/`) from
  images, MilkShape models and commands, and
  (`crates/noteskin/src/lua/`) a skin's Lua run at load over `lua/`; StepMania
  3.9's and OpenITG's format built natively (`crates/noteskin/src/legacy.rs`);
  SVG images in a checked subset (`crates/noteskin/src/svg.rs`, our extension);
  the buttons a skin draws without files of their names declared in its
  metrics (`[Global] Buttons=Onigiri,Center`, our extension);
  elements drawn by components of a skin's SVG, `Def.Component` with
  `Bind.*` bindings on the engine's clocks (`crates/noteskin/src/component.rs`,
  our extension, `docs/plans/noteskin-scene.md`; Infinito uses components for
  taps/fakes, receptors, holds/rolls (`Def.LongNote`) and lifts (`Def.Lift`), with
  masked WGSL holographic/wave paint; explosions use judgment components and
  original effect art, with lane/time-stable material variation. Explosion
  `rim-left`/`rim-right`/`link` roles join visible matching judgments on
  adjacent receptors (`crates/render/src/noteskin/judgment_links.rs`). Mines retain
  actors for their adjacency-linked electricity);
  opt-in mine row links from actual drawn positions (`MineConnectRows`,
  a `Tap Mine` frame’s `MineLink` child, with optional `MineRimLeft`/`MineRimRight`
  children hidden on connected sides; `docs/plans/noteskin-authoring.md`);
  opt-in roll body/cap animation from the active scaled roll timing window
  (`RollAnimationTimingWindowRatio`, `docs/plans/noteskin-authoring.md`);
  parameters a skin declares in its metrics, kept per skin in the
  settings, shown under Options → Arrows and read by its Lua
  (`NOTESKIN:GetParameter`; `crates/noteskin/src/params.rs`, our extension,
  `docs/plans/skin-parameters.md`; Infinito's Vivid (default), Rainbow and Flat colors, onigiri, mine shape,
  receptor fill and hit effects are its parameters; Peters-DDR combines
  the former Note/Vivid choices with Colors and Explosions, retaining
  their names as hidden compatibility variants);
  the skin keys settings store (`crates/noteskin/src/bundled.rs`); the same
  Lua host runs a song's scripted backgrounds over its folder
  (`Skin::song`, `LuaSkin::actor_file`, `lua/song.lua`;
  `docs/plans/scripted-backgrounds.md`), and builds a song's StepMania 3.9
  and ITG animations (`BGAnimation.ini`, `default.xml`, 3.0 layer
  folders) natively (`crates/noteskin/src/bganimation.rs`). Pure,
  natively tested; `docs/plans/noteskins.md`. `crates/render/src/noteskin.rs`
  loads a skin through its Lua for both shells (`NoteskinPlay::load_lua`),
  draws it in play and runs its Lua each frame. Our opt-in
  `HoldUseDroppedState` loads separate per-note dropped actors; hit messages
  carry `Autoplay` and local `HitX`/`HitY` from the hit note's last drawn
  position (`tools/noteskin-danoni/README.md`). These are presentation data,
  never changes to judging or replay inputs. `LiftStencilMaterial` opts into
  leading, time-sized lift trails with a continuous hold material, white head
  overlay and independently colored charge stencil. `Frame.note_motion` supplies
  local scroll speed and beat duration for the preparation distance.
  `TapJudgmentsPerNote` opts into
  immediate per-note flashes rather than StepMania's completed-row effects.
  Per-part `TapNoteAnimationOnChildren` (and `TapLift`, `TapFake`,
  `HoldHead`, etc.) opts layered notes into one beat/Vivid animation phase;
  descendants otherwise keep their own clocks, including mine electricity.
  `HoldStencilMaterial`/`RollStencilMaterial` opts holds into shared material over alpha/envelope/shadow
  masks, with protected head holes and continuous head/body/cap color; see
  `docs/plans/noteskin-authoring.md`. Legacy holds and rolls keep their own path.

- `crates/lua/` StepMania's Lua for skins, sandboxed (`docs/plans/lua.md`):
  `lunita` (crates.io), our hardened fork of piccolo, with per-VM memory
  and string limits, fuel and a deadline per call, StepMania's `cmd`
  keyword, Lua 5.1's globals and nothing that reaches outside.
  Platform-free, natively tested.
- `crates/session/` one play session for both shells (`ddi-session`,
  `docs/plans/desktop.md` 7.1): the play (`play.rs`: the engine's
  `Player`, bindings, the quit gesture, autoplay and `auto=pad`, the
  fail's fade, a work's start and end fades, the facts a play is kept
  with, a kept play replayed from its log), the event sounds (`sounds.rs`:
  the mine sound and the assist clap, made in code, the clap scheduled on
  the audio clock), calibration's modes, chart and click track, backgrounds' frame,
  the sides their images are kept at
  and the automatic video setting's step, a work's layers' timeline, the
  play's background movies (`movies.rs`: `MovieSlots`, which segments
  have a decoder, their tracks and what shows, each shell's decoders
  behind `MovieShell`; `BackdropTextures`, both shells' `movies.rs` run
  on it; song select's `#PREVIEWVID` movie, when it plays, `movies/preview.rs`), a song's scripted backgrounds and foregrounds (`scripted.rs`:
  their VM, the commands at each change, one budget a frame); generic over `ddi_platform::AudioBackend` and a shell's `Inputs`
  (`crates/web/src/play.rs` is the web's). Platform-free, natively tested on a
  fake clock.
- `crates/shell/` what both shells do between the settings (`ddi-ui`) and
  `render` (`ddi-shell`), so neither keeps a copy: a play's
  `RenderOptions` and Options' field preview's options and skin from the
  settings, the skin a play readies with its parameters' values (the
  preview's the same, `play_skin`), the note colors as the renderer has them, the ghost's lead as
  the HUD shows it, and a work's layers' textures
  (`crates/shell/src/stage.rs`, generic over each shell's decoded content:
  the web's `ImageBitmap`, desktop's RGBA rows) and a song's scripts'
  (`crates/shell/src/scripted.rs`). Platform-free (native
  and wasm), natively tested.
- `crates/app/` the game (`ddi`, the workspace's default member):
  a `main` that starts the web shell on wasm and the desktop shell
  everywhere else; Trunk builds it for the browser (`site/index.html`, with the page's
  styles, boot script and SEO data beside it in `site/`),
  `cargo run` for desktop; its `build.rs` writes the Windows (MSVC)
  executable's icon and version resource, with no resource compiler
  (`docs/plans/packaging.md`).
- `crates/web/` the web shell (`ddi-web`): the UI core's screens on the canvas
  (start menu at `#/`, `crates/web/src/components/page_routes.rs`; song
  select at `#/select`, `crates/web/src/components/select_page.rs`; the start
  prompt and the results on the play route,
  `crates/web/src/components/game_canvas.rs`; options at `#/options`,
  `crates/web/src/components/options_page.rs`; calibration's menu at
  `#/calibrate` and the credits at `#/credits`,
  `crates/web/src/components/page_routes.rs`; the device notices over all of
  them, `crates/web/src/notices.rs`; a calibration's outcome a card on the
  start prompt's screen; Dancing☆Onigiri's lyrics drawn over the play
  by the 2D pass, `ddi_render::lyrics`), pages as components of our own
  (`crates/web/src/component.rs`: messages handled in a microtask, each page's
  few DOM elements its own; no Yew; `#/video-check`, the crash page and
  the no-GPU page are plain DOM), one wgpu canvas and
  device for the whole app (`crates/web/src/web/screen.rs`: made at boot, moved
  into the page that draws, made again after a lost device), Web Audio
  clock, settings in `localStorage` (`crates/web/src/web/kv.rs`), imported songs
  and the play history in IndexedDB (`ddi-library` version 3; the plays in
  `plays` and `play_summaries`, `crates/web/src/history.rs`,
  `docs/plans/play-history.md`), replays on the play route
  (`#/play?replay=<play id>`, `crates/web/src/replay.rs`), the chart
  editor at `#/edit?song=<id>&chart=<n>` (`components/edit_page.rs`;
  its session, saving and playtests `#/play?…&edit=1`,
  `crates/web/src/editor.rs`; new songs from Add songs,
  `editor/new_song.rs`). Compiles for `wasm32-unknown-unknown` only (empty when
  built natively).
- `crates/desktop/` the desktop shell (`ddi-desktop`, run by `app` natively;
  `docs/plans/desktop.md`): a winit window drawing the UI core with
  `render`'s painter (cosmic-text, system fonts); sound through cpal with
  our own mixer, the clock from CoreAudio's host time and latencies
  (`audio/`, music decoded with `decode-all` on a thread); keys stamped
  with the OS's time where it gives one (`keyboard.rs`, macOS's
  `NSEvent`), controllers through gilrs on their own thread (`pads.rs`);
  the library read in place (`library/`: the song folders Options lists,
  StepMania roots, packs and zips, simfiles and Dancing☆Onigiri works,
  with a cache; a first start offers other games' song folders,
  `offer.rs`); plays through the shared session (`play/`: the UI core's
  start prompt, noteskins with their Lua read on a thread, backgrounds,
  a song's scripts read on a thread and their Lua run in slices
  (`play/scripts.rs`),
  movies (feature `video`, on by default: checked 8-bit H.264, HEVC and VP9 subsets through
  hardware VideoToolbox on macOS, experimental opt-in Media Foundation/D3D11
  on Windows (`mf.rs`, `mf_core.rs`; Windows Auto remains the software
  decoder), and for the rest and on failure the software decoder:
  FFmpeg in a sandboxed child process, `ddi-video-helper`
  (`video/helper`; `video.rs`, `video/helper.rs`, the arena in
  `video/arena.rs`; MP4/Matroska packets for the OS decoders demuxed by a
  helper of their own, `video/packets.rs`);
  movies in zips read in place or extracted once),
  a work's layers and lyrics, the theme's results, the history in the
  data folder (stale plays judged again on a thread, `history.rs`),
  replays, `play/replays.rs`); a play's frames paced (`pacing/`: blocking in the
  acquire, or each started as late before its refresh as its
  cost allows (late with two frames in flight by default when a display
  link is available, block with one elsewhere; late falls back to block
  for a window state where it keeps missing with its margin at its cap,
  as modeled for fullscreen); pure, with a simulated display in `pacing/sim.rs`, built
  for tests only: feature `sim`, which the crate's dev-dependency on
  itself turns on; the refreshes from a display link on macOS,
  `display_link.rs`: the view's `CADisplayLink` from macOS 14, else a
  `CVDisplayLink`);
  calibration's menu and tests (`calibrate.rs`); the chart editor
  (`editor.rs`: song select's E, saving into the song's folder, its
  playtests; new songs from a music file, `editor/new_song.rs`); the
  devices' fingerprints and notices (`devices.rs`). Settings in
  `storage.json` in the config folder; `DDI_HOME` moves the config, data
  and cache folders under it (`paths.rs`). Tested without a sound device
  on a fake clock and offscreen (`crates/desktop/tests/`; GPU tests skip without
  an adapter or with `DDI_NO_GPU=1`, as in CI's `desktop` job on Linux,
  macOS and Windows, with and without `video`). The video decoder
  helper is found at `DDI_VIDEO_HELPER`, else as `video/ddi-video-helper`
  beside the executable, in a macOS app's `Contents/Helpers/` or beside a
  folder above it (`crates/desktop/build.rs`
  stages it in the build's `target/<profile>/video/`, which a test in
  `target/<profile>/deps/` finds too); `cargo test -p ddi-desktop --test
  video` checks it on generated clips; `--decoder both` compares the
  platform's strict OS subset against it, `DDI_VIDEO_DECODER=software`
  forces it for play (`module`, its old name, still works); Windows
  experiments use `mediafoundation-auto` for software recovery,
  `mediafoundation` for strict refusal; `video/check-windows.ps1 -Helper
  PATH` records the deferred-host fidelity run, without a window or
  sound). `crates/desktop/build.rs` builds the helper on every `video` build:
  FFmpeg natively first (`video/build-native.sh`, a few minutes once,
  cached in `target/video/native/`; nasm on x86_64 for its assembly), then
  the helper crate in `target/video/helper-target/` with the `helper`
  profile; `DDI_NO_VIDEO_HELPER=1` skips it (movies then show their song
  background), cross builds skip it, and Windows skips it unless
  `DDI_VIDEO_NATIVE=1` (MSYS2 bash with the MSVC tools, untested here).
  `target/debug/video/ddi-video-helper --sandbox-check` runs the sandbox
  self-test alone. On Linux it builds
  against ALSA and udev (`libasound2-dev`, `libudev-dev`; LGPL, linked
  from the system); winit, rfd and arboard load their libraries at run
  time.
- SkinCraft is maintained in [sugoijan/skincraft](https://github.com/sugoijan/skincraft),
  a separate native application consuming our loader and field renderer as Git
  dependencies (`docs/plans/noteskin-authoring.md`). Its own repository contains
  the editor hosts, MCP bridge, local development patches and authoring tests.
- `crates/importer-protocol/` the platform-free framed app/helper IPC contract (serde
  and plain data only; no SDK/networking or filesystem).
- `tools/danoni-importer/` the native helper, a root workspace member requiring
  Rust 1.99 (as does the SDK). Both share the root lock; the `importer` profile
  keeps native builds unwinding with bounded compiler memory.
  The desktop's default feature `danoni-importer` (`danoni` alias; the web
  never builds it) prepares the pinned worker/helper
  and complete notices through `crates/desktop/build.rs`, stages an immutable bundle
  beside the app, and compiles only its content ID/digest into desktop. The SDK
  is not linked into the app. `tools/package/package.py` copies a complete relocatable
  distribution; see `docs/plans/danoni-importer.md`. Python/Java are build-time
  prerequisites only. No helper preparation runs for wasm or Windows builds or
  with `--no-default-features`. `DDI_IMPORTER_TEST_ONLY=1` skips preparation
  only in debug unit/check builds (`just test`, `just check`, CI's
  `deploy.yml`, and agents' clippy and test runs set it), leaves import
  unavailable and cannot be packaged.
  Desktop's URL tab is a user-triggered single-work download: no site-approval
  prompt or robots.txt request, with the usual public-address checks, limits and
  pacing. The SDK's research/batch default still respects robots. Show real task
  counts where available and continuous activity otherwise; honor reduced motion.
- `tools/danoni-dump/` the Dancing☆Onigiri URL-import SDK
  (`rust/`, a root workspace member), its confined Ruffle worker build inputs
  (`ruffle/`), desktop handoff probe (`desktop-probe/`) and independent chart verifier
  (`pack-verify/`). The SDK, probe and verifier are root workspace members
  sharing `Cargo.lock` (the probe links the desktop, so a separate lock went
  stale with every game dependency change); only the generated upstream Ruffle
  worker keeps its own workspace and lock. Root Cargo checks cover them only
  when selected (the app remains the default member). CI's `danoni.yml` runs
  on changes to the crates, the DanOni tools, the manifests or the lock, not on
  documents, assets or the web shell's code alone. `just danoni-check`
  (Python 3, ffmpeg and native build prerequisites) runs their locked checks, synthetic tests and independent
  Opus decoder comparison; `--minimal` checks the SDK without native/codec
  features, including on Windows in CI. No live songs or runtime downloads
  are part of this check. The SDK runs inside the optional native helper rather
  than the game process; prepared SSC/Opus use its verified playback snapshot
  and the ordinary library. Never scan transaction directories. Sources and
  generated third-party packs stay outside Git. See
  `docs/plans/danoni-importer.md` for the current build/distribution contract;
  `docs/research/danoni-desktop-integration.md` records the research.
- `crates/icon/` the approved brand icon's build-time compositor: the vector
  artwork, frozen color texture and target profiles in `assets/icon/`. Cargo
  builds the Windows/Linux window SVG in `OUT_DIR`; Windows' executable
  resources use the same renderer. Optional `raster` uses resvg for xtask's
  target-specific output. No shader or editor runs during builds.
- `xtask/` generators: target-specific icons (`icons --target
  web|windows|linux|macos`, cached under `target/icons/`), SEO fragments, the song manifest, `credits/CREDITS.md`
  and `credits/THIRD_PARTY_RUST.md`.
- `tools/package/` the desktop's packaging (`package.py`: the relocatable
  folder and each system's archives; the app's `Info.plist`, the Linux
  desktop entry, the icon), run by `just package` and
  `.github/workflows/release.yml` (`docs/plans/packaging.md`). Icon sources
  live only in `assets/icon/`; Trunk stages the generated favicon, and
  packaging asks xtask only for its platform's ICNS or SVG. Generated icon
  assets stay out of Git.
- `video/` the software video decoder: C shims over FFmpeg's video
  decoders (every format StepMania plays), built two ways from the one
  pinned release (`video/FFMPEG.toml`): for the web to
  `target/video/ddivideo.wasm` with wasi-sdk (`video/build.sh`; LGPL
  module, never committed; `docs/plans/video-backgrounds.md`), and for
  desktop as static libraries (`video/build-native.sh`, into
  `target/video/native/<target>/`) linked into `ddi-video-helper`
  (`video/helper`, the sandboxed decoder process: Seatbelt on macOS,
  seccomp and Landlock on Linux, a restricted token on Windows, each
  self-tested before it serves; `video/protocol`, `ddi-video-protocol`,
  the bounded pipe messages and the shared-memory arena both sides use;
  `docs/plans/native-video.md`, section 10). `video/ddi_io.c` is the
  helper's file layer (the game serves the bytes). Additive bounded demux
  exports in `video/packets.c` (`video/packets.md`), read on desktop by
  `crates/desktop/src/video/packets.rs` for native MP4/Matroska/WebM (`media.rs`,
  codec validation and output models in `codec.rs`, `h264_format.rs`,
  `hevc.rs` and `vp9.rs`). FFmpeg is never linked into the game.
- `assets/songs/<id>/` bundled songs with a `PROVENANCE.toml` each;
  `assets/noteskins/<game>/<name>/` bundled noteskins, the same way
  (`docs/plans/bundled-skins.md`; Infinito is the default SVG skin;
  the original Dito variants are retired
  and procedural arrows are an internal fallback without a picker entry);
  `assets/fonts/` the embedded HUD font and the theme's text face
  (Iosevka Term, served as `fonts/`).

## Commands

- `cargo build` includes the native URL-import helper/runtime on macOS and
  x86_64 Linux (the default `danoni-importer` feature; a JDK, CMake and
  Python 3.11+ at build time). The first build fetches pinned public
  build inputs; no runtime tools or maintainer configuration are needed by the
  result. `python3 tools/package/package.py folder --app
  target/debug/ddi --output /tmp/ddi-package` prepares a fresh
  relocatable folder (`build.py package` forwards to it). Keep its assets, notices, optional full video payload and
  `danoni-importer/` beside the executable. `--danoni-bundle-info` and
  `--danoni-importer-check` inspect it without a window or sound device. The app
  defaults to video and the importer; `--no-default-features --features
  danoni-importer` makes a deliberate no-video package, `--no-default-features
  --features video` a build without the importer.
- `just package` builds with `--profile desktop` and packages for this
  system into `target/dist/` (`package.py dist`: an ad hoc signed `.app`
  in a zip and a `.dmg` on macOS, its files in `Contents/Resources` and
  the helper in `Contents/Helpers/video`, found through
  `ddi_desktop::paths::package_dirs`; a tarball with a desktop entry on
  Linux; a zip on Windows, `--without-video-helper`), after the packaged
  game's headless checks. A pushed tag `v<Cargo's version>` runs the same
  in `.github/workflows/release.yml` on the three systems and attaches the
  archives to a draft release (`docs/plans/packaging.md`).
- `cargo run` opens the desktop app on our theme's silent start menu
  (`--profile desktop` for smooth frames: `release` is the wasm bundle's,
  tuned for size and aborting on panics). Flags (`ddi_desktop::run`):
  `--songs DIR` (a folder for this run), `--sample`, `--screen
  start|select|results|options|calibration|credits`, `--dev`, `--widgets`,
  `--play SONG [--chart N] [--auto | --auto=pad] [--start]`,
  `--edit SONG [--chart N]` (the chart editor on it),
  `--replay PLAY_ID [--results] [--start]` (a kept play's replay, or its
  results),
  `--calibrate visual|audio|combined [--auto] [--bias MS] [--start]`,
  `--fake-pad "1.0:13+,1.1:13-"`, `--silent` (no sound device: plays run
  on the host clock), `--volume V`, `--seconds N` (quits, printing the
  library, the clock's latency and jitter, the devices, the plays),
  `--lose-device-after S`, `--video-check FILE [--reference REF.yuv]
  [--frames] [--decoder auto|software|videotoolbox|mediafoundation|mediafoundation-auto|both]`, `--pacing block|late` (a play's frame pacing),
  `--in-flight 1|2` (defaults: late with two when a display link is
  available, block with one elsewhere), `--pacing-report` (block and late with one and two in
  flight, 4 s each in turn on Fifo, a table of each printed when the
  play ends; `docs/plans/desktop.md`, "Frame pacing"). **Agents' and tests' runs set
  `DDI_HOME=$PWD/target/scratch/<name>/home`** (never the maintainer's
  folders; the URL importer refuses a home inside a Git checkout, so
  `--danoni-importer-check` takes one outside it, as under `/tmp`), use
  `--volume 0.005` or `--silent`, and run under
  `target/scratch/wd.sh`; prefer offscreen tests: a window takes the
  Mac's focus. The same crate built for wasm is the web app, which Trunk
  builds (`just dev`, `just build`).
- The web build is compiled with wasm SIMD (`.cargo/config.toml`'s
  `+simd128` for `wasm32-unknown-unknown`, `--enable-simd` for wasm-opt in
  index.html; the page refuses a browser without it, `no-wasm-simd`).
- `just dev` dev server (auto-reloads on every rebuild; unsuitable for long
  runtime tests). `just dev 0.0.0.0` serves the local network, but a LAN
  address over HTTP is not a secure context (no gamepads, no WebGPU); from
  another machine, tunnel instead (`ssh -N -L 8080:127.0.0.1:8080 <host>`,
  then open `http://localhost:8080`).
- `just build` release build with the production public URL.
- `just screenshot [scenario.json] [output.png]` regenerates the README's
  gameplay image from `docs/screenshots/readme.json` (Beatucada, with its
  background). The desktop example `screenshot` runs the actual play and
  renderer offscreen on a fixed clock, with no window or sound device;
  its home is isolated under `target/scratch/screenshot/home`. Requires
  a GPU. `just animation [scenario.json] [output.gif]` uses the same
  example with `docs/screenshots/animation.json` to generate a small,
  silent looping GIF, requiring FFmpeg on `PATH`. Media provenance and
  reproduction are documented in `docs/screenshots/README.md`; the generated
  PNG and GIF are committed and their third-party visual components are
  annotated in `REUSE.toml`.
- `just check` native checks, wasm check, SEO regen, REUSE lint.
- macOS video planes remain copied by default. A build with
  `--features ddi-desktop/video-metal` and `DDI_VIDEO_PLANES=metal` opts
  into IOSurface/Metal planes for checked VideoToolbox streams; unsupported
  imports copy the current image, and unconfirmed GPU teardown disables
  sharing. `docs/plans/native-video.md` section 4f records ownership bounds,
  measurements and ignored GPU tests. `video-bench` adds the offscreen
  comparison/benchmark; run each ignored test separately under the watchdog
  with a scratch home. Optimized benchmarks set
  `CARGO_PROFILE_DESKTOP_LTO=false` to keep compilation under 2 GiB.
- Windows from the Mac: `rustup target add x86_64-pc-windows-msvc`
  (and `aarch64-pc-windows-msvc`), then `cargo clippy -p ddi-desktop
  --target x86_64-pc-windows-msvc --all-targets` with and without
  `--no-default-features` (no `video`; checking needs no linker; running needs Windows).
  Linux: the CI job's commands in `catthehacker/ubuntu:act-latest`
  through `container run` (`act` with `socktainer` gets as far as
  `actions/checkout`, then cannot copy its step scripts into the
  container).
- `just test` native tests (chart, engine, render, session, shell,
  platform with `decode` and, through desktop, `decode-all` (its references against
  the `ffmpeg` command where it is installed), library with `nfkd`,
  actor, ui, theme, noteskin, lua, desktop, xtask), after writing the
  bundled songs' and noteskins' indexes the desktop's tests read. The
  app crate has no native tests; verify the web in a browser. Desktop's
  local checks: `DDI_WEB_PLAYS=<file>` (`--test play -- --ignored`: the
  judgements against the web's plays of the same charts),
  `DDI_WORKS=<dir>` and `DDI_WEB_WORKS=<jsonl>` (`--test works --
  --ignored`: local works against the web's import, autoplayed),
  `DDI_WEB_EXPORT=<jsonl>` and `DDI_EXPORT_TO=<jsonl>` (`--test
  import_plays -- --ignored`: the web's play history export imported and
  judged again here; plays exported for the web to import),
  `DDI_FFR_FIXTURES=<dir>` (`--test ffr_levels -- --ignored`: our own
  FFR levels written for the web's import),
  `DDI_MOVIE_SONGS`, `DDI_AUDIO_DEVICE_TESTS=1`, `DDI_OFFSCREEN_PPM=<dir>`
  (keeps the offscreen pictures). The VideoToolbox comparisons
  (`video_native*`) run on a Mac only: CI sets `DDI_SKIP_VT=1`, its
  virtual machines' VideoToolbox having stalled in them;
  `DDI_REQUIRE_VT=1` makes a refused hardware session fail them.
- `DDI_WRITE_CORPUS=<dir>[:<dir>…] cargo test -p ddi-chart --test write local_corpus -- --nocapture`
  writes every `.sm`/`.ssc` under local folders back as `.ssc` (and
  `.sm` when nothing is lost) and expects it read back equal.
- `DDI_DIFF_CORPUS=<dir> cargo test -p ddi-chart --test differential local_corpus -- --nocapture`
  compares every `.sm` under a local folder against the two oracle parsers
  (`DDI_EXTRA_SIMFILES` does the same for plain parsing in `simfiles.rs`;
  `--test digest local_corpus_digests` checks the chart digests over it).
  Third-party songs used this way stay local.
- `DDI_AUTOPLAY_CORPUS=<dir>[:<dir>…] cargo test --release -p ddi-engine --test local_autoplay -- --nocapture`
  autoplays every chart under local folders on ITG, SM5 and DDR A and
  expects a perfect play, judged again from its input log to the same
  results (`DDI_AUTOPLAY_FILTER=<text>`,
  `DDI_AUTOPLAY_VERBOSE=1` to see what fell short).
- `DDI_NOTESKINS=<dir>[:<dir>…] cargo test -p ddi-noteskin --test local_skins -- --nocapture`
  resolves every element of every skin under local `NoteSkins` folders
  (`DDI_NOTESKINS_STANDINS=1`: without their `default` and `common`;
  `DDI_NOTESKINS_LUA=1`, in release under the watchdog: with each skin's
  Lua; `DDI_NOTESKINS_LUA_PLAY=1` also plays 20 s at 60 and 144 Hz and
  reports the Lua's fuel per frame).
- `DDI_LUA_CORPUS=<dir>[:<dir>…]` / `DDI_LUA_CASES=<cases.txt>` with
  `cargo test --release -p ddi-lua --test local -- --nocapture` compile
  local skins' Lua, or run lunita's exported limit cases with
  their time gates (under the memory watchdog).
- `just video` builds the web's decoder module (downloads the pinned
  toolchain and FFmpeg into `target/video/`); `just video-native` builds
  the desktop helper's FFmpeg the same way (`cargo build` of the desktop
  does it by itself); every Trunk build runs the former too when the
  module is missing (as after `cargo clean`, a few minutes) or older than
  its sources, and fails if it fails (`DDI_NO_VIDEO=1` builds the site
  without it); `just video-check [file]` checks it on a file
  or on generated clips against the `ffmpeg` command's decode.
- `cargo run -p xtask -- gen-songs` / `gen-noteskins` / `gen-credits`
  regenerate the song manifest, the bundled noteskins' index and the
  credits (Trunk runs them before every build; CI checks `credits/CREDITS.md` and
  `credits/THIRD_PARTY_RUST.md` are fresh). Infinito's SVG, WGSL and Lua
  sources live in `assets/noteskins/dance/Infinito/`; its experimental
  authoring playground is kept separately. The old `gen-dito` command is retired.
- `python3 tools/noteskin-danoni/generate.py [--check]` adapts the pinned
  danoniplus SVG alpha masks to the game's SVG subset for the `DanOni`
  noteskin proof. Its Lua behavior and host-interface limits are documented
  in `tools/noteskin-danoni/README.md`; upstream originals remain byte-identical.
- `python3 tools/noteskin-centers/generate.py [--check]` draws pump's
  center panel for Delta from its own art (`docs/plans/pump.md`, "Centers
  for the bundled skins").
- Peters-DDR's approved raster center, holds, rolls and layered mines are bundled
  from SkinCraft exports; editable sources are in `tools/noteskin-peters/design/`.
  `python3 tools/noteskin-peters/generate.py [--check]` reproduces only the earlier
  model/SVG study under `target/scratch/peters/model-svg-study`, not the bundled skin.

Prefer the latest released versions of dependencies; check crates.io before
adding one and refresh the lockfile with `cargo update`.

## Verifying changes at runtime

The maintainer uses Firefox and tests it by hand; automated checks run in
headless Chrome. See `.claude/skills/verify/SKILL.md` for the full recipe. In
short: build a static bundle (`trunk build --release --public-url / --dist
<dir>`) and serve it with the skill's `serve.mjs` (Python's `http.server`
drops connections under the page's parallel requests), drive it with `playwright-core`
against the installed Chrome (`channel: 'chrome'`, flags
`--enable-unsafe-webgpu --use-angle=metal --autoplay-policy=no-user-gesture-required`),
and read `window.__DDI_DEBUG` for engine state. Headless Firefox runs too
(Playwright's build or the installed one, WebGL2 only), but only through the
skill's `firefox.mjs`: on macOS a bare launch cannot open Firefox's data folder,
and the helper keeps Firefox away from the maintainer's profiles.
Playwright's WebKit (not Safari) runs headless with WebGPU; it ignores the
gesture rule below, so it does not replace the maintainer's Safari check.

The UI (song select, options, the pages, the start prompt and the results)
is our theme's screens on the canvas, not DOM: drive them by keys, a fake
pad (`window.__DDI_FAKE_PAD`) or taps at `window.__DDI_DEBUG.ui.rects`, and
read their state from `__DDI_DEBUG.ui.theme`. A fresh profile shows the
first-run device notice over every page: seed `audio_profiles` and
`display_profiles` in `ddi.settings.v1` (or tap `later`) when a test wants
the screen without it. The testing parameters `gfx`, `auto`, `bias` and
`slow` stay for the session once given on `#/?…` or `#/play?…`. The skill
lists the routes and their parameters (`#/play`, replays, calibration,
`#/video-check`, `#/skin-preview`, `#/ui-test`, `#/ui-dev`), what
`__DDI_DEBUG` carries, and a recipe for each screen and feature; song
select's arrow gestures are specified in `docs/plans/song-navigation.md`.

Import is tested by setting files on the import panel's inputs (folder or zip)
in a fresh browser profile; build test packs in a scratch directory, never in
the repo. Real third-party packs on the maintainer's machine may be used for
one-off local tests only: never copy them into the repository, fixtures or
published artifacts.

Safari is tested by the maintainer on the published URL after a push: local
origins are plain HTTP, which Safari treats as insecure, so local Safari
results are not representative. Firefox and Chrome must not break.

Start media from user gestures synchronously: create the `AudioContext` and
call `play()` inside the click or key handler, with no `await` (storage read,
fetch) in between. Safari refuses once the gesture is over, and the headless
runs (launched with an autoplay override) cannot catch it; for previews, test
with `--autoplay-policy=document-user-activation-required` and ask the
maintainer to confirm in Safari. A play opened from song select takes
song select's context, running from that press (or an earlier one), and
its start prompt decodes the music on it, so Start only starts the sound
(`select_preview::hand_over`, `web::audio::keep_for_play`); a play opened
any other way makes its context at Start, as before. That prompt counts
down to Start (Options → Display → Start countdown, 3 s by default): its
automatic Start is no gesture, so it runs only where song select handed
over a running context (never on a direct `#/play?…`, a replay, a
playtest or calibration), and where that context does not run any more
the prompt asks for a tap or key, as for a pad's Start
(`docs/plans/start-prompt.md`).

In calibration the autoplayer
follows the physical cue (heard audio, or drawn arrows in the muted test), not
the judged timeline; keep it that way or offset tests become meaningless. In
play (`&auto=1`) it presses on the judged timeline, so it stays exact under any
calibrated offset.

Menu Back is unmodified Escape or Backspace after text/binding/dialog handling.
Song browsing returns to Start in one press, keeping its filter and place;
Clear beside Search clears the filter explicitly. Options returns to its caller.
Web route ancestry and Options position live in history state; desktop uses
the UI stack. Preview audio belongs to visible song select (or an explicit
Options sample); Start is silent. See `docs/plans/start-menu.md`.

## Additional chart versions

- Options → Notes → Simplify → Without onigiri (`simplify_onigiri`)
  exposes additional versions of DanOni 5/7/9A/9B charts in Single,
  Solo, Double and Techno 8-panel (`techno-single8`); Without center
  (`simplify_center`: Off, Where missing, the default, listing one only
  where its style has no original of its difficulty (Edits always), or
  All) versions of pump single and half-double charts in Single and
  pump double in Double, the center removed and each pad turned an
  eighth clockwise, their holds still ticked as pump's
  (`Chart::pump_game`). Notes → Convert (`convert_pump_single`,
  `convert_five_keys`, on by default) converts pump single and 5 keys
  to each other, every lane kept (the center on the onigiri), not
  assisted (`Derivation::assisted`), each played by its original's
  rules. Song select marks every generated version
  (`ChartEntry::version`). See `docs/plans/chart-simplification.md`.
- Generated charts carry `Chart::derivation` and are appended after all
  originals (every onigiri-free version before every center-free one,
  then the conversions), independently of the display setting. Preserve
  original chart indexes, digests and files. Only a generated chart that
  leaves notes out gets the assisted mark from this feature; the
  existing transform options keep their behavior.
- Chart-scoped lyrics and layers resolve through `Song::effects_chart`.
  Keep chart provenance in manifests, bests and replay resolution. Old
  browser imports upgrade their metadata from stored chart data; desktop
  invalidates its library cache when the metadata changes.
- Simfiles used by the library, play and the bundled songs' index
  (`xtask gen-songs`) load through
  `ddi_library::scan::parse_simfile`, which appends generated versions too;
  the format importers preserve source charts, including SSC exports.

Song select → pre-start → play backgrounds share `crates/actor/src/background.rs`:
a cover-fitted Gaussian base at 0.2 over #05020d, with a framed content copy
entering over 450 ms and finishing its unblur over 300 ms after Start (on
the prompt's loading screen when the play is not ready yet, the play then
continuing from where it left the backdrop, `Entrance::resume`).
Reduced motion settles immediately; early Start samples the current visual
position without delaying audio. The play base follows current image/video
layers even for a screen-sized frame. Static filters are reused; direct RGB
movie uploads must call `Renderer::background_texture_changed`, while YUV
and NV12 upload methods invalidate their own filtered copies. The field
filter darkens only a drawn background (`Renderer::render`'s
`filter_path`, `Backdrop::shows`): a song without one, a picture that
never loaded, the time before a first change, or brightness Off plays on
the one #05020d ground with no band, as the prompt shows it
(`docs/plans/overlays.md`, "Field filter").

## Timing model (do not break)

- The audio clock is authoritative. Inputs carry browser timestamps and are
  judged against heard time: context time minus total output latency minus
  the user's audio offset. The visual offset moves only what is drawn.
- `getOutputTimestamp` already includes the latency; the web backend adds it
  back so the engine subtracts it exactly once.
- Offsets are stored per device profile (audio path fingerprint from sample
  rate and latencies; display fingerprint from screen size and scale, with the
  refresh rate deliberately excluded because variable-rate panels change it).
- Calibration converges on the real gameplay path with a non-periodic chart;
  the measurable range is ±250 ms by design. Changing the chart pattern or the
  window must keep every gap wider than twice the window.
- MP3s are heard where StepMania 5.1's reader has them (no delay trimmed, an
  `Info` tag frame played as silence; PLAN.md decision 46): the web moves
  each browser's decode to one of the lags the first frame allows, the one
  a wasm decode of the song's start agrees on, else the one measured for
  that browser (`ddi_platform::mp3::align`; `window.__DDI_MP3` says which
  and by how much), kept as the buffer's lead; desktop decodes that way.
  The shift is the song's audio start, never an offset. A
  Dancing☆Onigiri work's music goes where Chrome's decode has it
  instead (danoniplus plays the browser's own decode, and works are
  synced in Chrome): the lead less Chrome's expected lag
  (`mp3::Target::Chromium`, `ddi_session::music_target`).

## Licensing and attribution

- Code is MIT. Every file is covered by `REUSE.toml`; `just check` runs
  `reuse lint`. Full licence texts live in `LICENSES/`.
- Bundled songs: only works whose music, charts and graphics each carry an
  explicit free licence in the upstream files (CC BY, or CC BY-SA as a second
  tier). Never NC, ND, undeclared, or licensed only on a web page.
- Bundled noteskins: art under CC BY, CC BY-SA, Apache-2.0, the Unlicense
  or CC0, declared in the upstream files; code we may not take (Lua derived
  from unlicensed skins) is replaced by our own and listed as a
  modification.
- Bundled fonts: OFL-1.1 (as the HUD's and the theme's Iosevka subsets,
  `assets/fonts/`; the theme's are served as files, not embedded) or
  the licences above, declared upstream; subsetting is a modification.
- Each song or skin directory keeps the upstream files byte-identical plus
  `PROVENANCE.toml` (source URL, pinned commit, retrieval date, licences per
  part, exclusions, modifications). In that file, `excluded` and
  `modifications` must sit above the `[source]`/`[[licenses]]` tables: TOML
  attaches keys after a table header to that table, and the generator rejects
  unknown keys there.
- Any modification of an asset (resizing, cropping, transcoding) is recorded
  as a modification with the exact command.
- Code or data copied from another project (StepMania's noteskin metrics,
  danoniplus's key tables) keeps its upstream form where possible, gets a
  `REUSE.toml` annotation and a `credits/THIRD_PARTY.toml` entry (the credits ship
  its notice). Copy only from permissive sources: never from ITGmania
  (GPLv3 since 2024-09-20; StepMania 5.1 has the same older files under its
  MIT-style licence). Reading GPL code to learn behaviour is fine;
  transcribing it is not.

Third-party Rust dependencies keep their own licences (e.g. Symphonia is
MPL-2.0, file-level copyleft, fine to link from MIT code). Only bundled
assets and copied code are tracked in `REUSE.toml`. Their notices ship
with the game: `gen-credits` lists every crate in the web build (the app
and its Opus and SVG workers) and in the desktop binary (`cargo tree`, normal
dependencies without proc macros, Linux, macOS and Windows together) with
its licence texts from the crate's files, deduplicated, into
`credits/THIRD_PARTY_RUST.md` and the Credits screen's folded "Rust libraries"
(`xtask/src/rust_credits.rs`); a licence outside its allowlist (MIT,
Apache-2.0, BSD, ISC, Zlib, 0BSD, Unicode, MPL-2.0, CC0, Unlicense,
BSL-1.0, the LLVM exception) fails the generator, so check a new
dependency's licence before adding it.

## Deployment

- SEO lives in `site/seo/metadata.toml`; `xtask regen-seo` generates the
  static head/body and image sitemap. Trunk publishes the existing
  `docs/screenshots/readme.png` unchanged as `images/readme.png` for share
  cards and structured data; keep its metadata dimensions in sync. The
  deployed audit and post-deploy checks are in `docs/research/seo.md`.
- GitHub Pages from the `main` branch via the workflow in `.github/`. The
  public URL is lowercase; on a custom domain Pages paths are case-sensitive,
  which is why the repository name is lowercase.
- The page draws its loader from its own first bytes: its styles
  (`site/styles.css` inlined) and boot script are in `index.html`, which
  fits the first round trip compressed (about 12.5 KB gzipped), and
  nothing the first paint waits for is fetched (no stylesheet links,
  classic scripts with a source, `@import` or `url()`). `xtask
  check-boot`, Trunk's last post-build hook, fails a build that breaks
  it. The page follows the game's theme alone, never the system's color
  scheme: its colors are the theme's (`xtask regen-seo` writes them from
  `crates/theme/src/infinito.toml` to `target/seo/theme.css`, inlined;
  the theme-color meta is its ground).
- `credits/CREDITS.md` and `credits/THIRD_PARTY_RUST.md` are generated and committed;
  `Trunk.toml` ignores them in the watcher so regenerating them does not
  loop the dev server.

## Conventions

- Use American English in first-party identifiers, comments, UI text and
  documentation (`color`, for example). Preserve copied upstream text and
  names required by external APIs or file formats.
- Logical file paths use `ddi_library::path::{RelPath, RelPathBuf}`; convert to OS paths only with `to_path(base)`, never by joining their text onto a `PathBuf`.
- Commit messages: imperative summary line, body explaining why.
- Keep `docs/PLAN.md` status and decisions, and the plan's line in
  `docs/plans/README.md`, current when scope changes.
- UI text avoids jargon from specific games except where naming a ruleset
  or a layout by StepMania's own name for its game ("Pump single", from
  StepMania's `pump` game, `docs/plans/pump.md`).
