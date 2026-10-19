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
architecture, decisions and roadmap; `docs/research/` holds the sourced notes
on formats, rules and the tech stack. Read those before changing the engine or
the importers.

Public copy (page title, descriptions, README lead, landing-page entry) says
"dance rhythm game" and never names other games as a selling point. Rust and
the browser are implementation details, not advertised features.

## Layout

- `chart/` chart model and importers (`.sm`/`.ssc`), 48 ticks per beat, exact
  StepMania timing semantics. Pure, natively tested.
- `engine/` deterministic gameplay: clock, judge, rulesets, scoring, gauges,
  scroll, calibration maths. No I/O. Replay fixtures in `engine/tests/`.
- `render/` wgpu scene (procedural SDF arrows, glyphon text) and the 2D pass
  the menus move onto (`docs/plans/ui-core.md`): a draw list, its painter,
  the texture cache and the text atlas (runs measured and drawn by the
  platform: the browser on the web, cosmic-text natively; drawn over a
  play too, through an sRGB surface where there is no plain view, for
  Dancing☆Onigiri's lyrics, `render/src/lyrics.rs`), and Options'
  field preview (`render/src/field_preview.rs`: the demo chart drawn as
  play draws it into a texture the 2D pass shows). No platform
  code.
- `platform/` traits and plain data the shells implement (audio clock, input,
  device fingerprints), plus the Ogg Vorbis and MP3 fallback decoder behind
  the `decode` feature (Symphonia, MPL-2.0; MP3 as StepMania decodes it,
  `platform/src/mp3.rs`), Ogg Opus through `rusty-opus` (BSD-3-Clause)
  behind `opus` (`platform/src/opus.rs`: the web's Opus worker and
  desktop's decoder) and, behind `decode-all` (the
  desktop's), every format a simfile's music comes in: MP3 as StepMania
  5.1 decodes it (`platform/src/decode_all.rs`), resampling with rubato.
- `opus-worker/` the web's Ogg Opus fallback (`ddi-opus-worker`, PLAN.md
  decision 9): a worker Trunk builds beside the app (index.html,
  `data-type="worker"`; `xtask stage-opus`, a post-build hook, names its
  files by their hash and tells the page in index.html's
  `ddi-opus-worker` meta), started by `web/src/web/opus.rs` only when the
  browser cannot decode an Ogg Opus file (the module compiled once a
  session and handed to a worker a decode; one that has not started in
  25 s fails); it sends back a float WAV at 48 kHz that
  `decodeAudioData` resamples. Empty natively.
- `library/` song library logic shared by the app and xtask: pack scanning
  (which file is the chart, music, banner), a sans-IO zip reader, the manifest
  entry type, and the import's core (`scan.rs`: a song's entry, its files
  and the report's warnings from its folder; `works.rs`: a
  Dancing☆Onigiri work's songs, ids, music and layers; the web's import
  runs them on `Blob`s, desktop's library on its folders). Pure,
  natively tested.
- `actor/` StepMania's actor model, shared by noteskins and later our
  screens (`docs/plans/ui-core.md`): actors, the tween queue, effects,
  sprite states, eases, commands as metrics write them, a model's meshes,
  bones and animated textures, and what is drawn (with our own
  extensions for the theme: sprites' `cornerradius`, `softedge`,
  `outline`, `saturation`; text's `ellipsis` and letter spacing). Pure,
  natively tested;
  the files actors are built from (`metrics.ini`, MilkShape, legacy actor
  files, Lua) are read in `noteskin/`.
- `ui/` the UI core our screens are built on (`docs/plans/ui-core.md`,
  step 3): input events in, requests to the shell out (handled
  synchronously, gesture-bound ones flagged), hit-testing through actors,
  touch and mouse gestures, menu navigation with key repeat and focus,
  screens with transitions, dialogs, messages to actors, the one-line text
  field (mirroring the web's hidden textarea, or editing itself on
  desktop), and a headless harness; the theme's screens
  (`ui/src/screens/`: song select, results, options, the pages for
  calibration's menu and the credits, the app's notices as the UI's
  chrome, the play's start prompt, `prompt.rs`) with their sample
  data, options' rows made from the settings and the shell every
  platform shares (`options_model.rs`, `options_shell.rs`, the bind
  flow in `ui/src/state/bind.rs`); the state screens read (settings and
  their migrations, the song list's place, library views, the play
  history and its replays, `ui/src/state/history/`; stored through
  `ddi_platform::storage`, which each shell installs) and the developer
  screen both shells run (`ui/src/dev.rs`). Pure, natively tested.
- `theme/` our theme (`ddi-theme`, `docs/plans/theme.md`): `Infinito`,
  a look for the UI core whose components are the maintainer's mockups,
  its colors, faces, sizes, timings and places in
  `theme/src/infinito.toml` (turned into tables at build time). The
  screens it dresses are `ui/src/screens/` (song select, results,
  options; theme-agnostic, fed plain data). Pure, natively tested.
- `noteskin/` StepMania noteskins: fallback chains, file lookup and
  redirects, `metrics.ini`, file name hints, what each element is (stand-ins
  for StepMania's `default` and `common`), its actors built (`actor/`) from
  images, MilkShape models and commands, and
  (`noteskin/src/lua/`) a skin's Lua run at load over `lua/`; StepMania
  3.9's and OpenITG's format built natively (`noteskin/src/legacy.rs`);
  SVG images in a checked subset (`noteskin/src/svg.rs`, our extension). Pure,
  natively tested; `docs/plans/noteskins.md`. `render/src/noteskin.rs`
  draws a skin in play and runs its Lua each frame.
- `lua/` StepMania's Lua for skins, sandboxed (`docs/plans/lua.md`):
  `lunita` (crates.io), our hardened fork of piccolo, with per-VM memory
  and string limits, fuel and a deadline per call, StepMania's `cmd`
  keyword, Lua 5.1's globals and nothing that reaches outside.
  Platform-free, natively tested.
- `session/` one play session for both shells (`ddi-session`,
  `docs/plans/desktop.md` 7.1): the play (`play.rs`: the engine's
  `Player`, bindings, the quit gesture, autoplay and `auto=pad`, the
  fail's fade, a work's start and end fades, the facts a play is kept
  with, a kept play replayed from its log), calibration's modes, chart and click track, backgrounds' frame
  and the automatic video setting's step, a work's layers' timeline;
  generic over `ddi_platform::AudioBackend` and a shell's `Inputs`
  (`web/src/play.rs` is the web's). Platform-free, natively tested on a
  fake clock.
- `app/` the game (`ditoditoinfinito`, the workspace's default member):
  a `main` that starts the web shell on wasm and the desktop shell
  everywhere else; Trunk builds it for the browser (`index.html`),
  `cargo run` for desktop.
- `web/` the web shell (`ddi-web`): the UI core's screens on the canvas
  (song select at `#/`, `web/src/components/select_page.rs`; the start
  prompt and the results on the play route,
  `web/src/components/game_canvas.rs`; options at `#/options`,
  `web/src/components/options_page.rs`; calibration's menu at
  `#/calibrate` and the credits at `#/credits`,
  `web/src/components/page_routes.rs`; the device notices over all of
  them, `web/src/notices.rs`; a calibration's outcome a card on the
  start prompt's screen; Dancing☆Onigiri's lyrics drawn over the play
  by the 2D pass, `ddi_render::lyrics`), pages as components of our own
  (`web/src/component.rs`: messages handled in a microtask, each page's
  few DOM elements its own; no Yew; `#/video-check`, the crash page and
  the no-GPU page are plain DOM), one wgpu canvas and
  device for the whole app (`web/src/web/screen.rs`: made at boot, moved
  into the page that draws, made again after a lost device), Web Audio
  clock, settings in `localStorage` (`web/src/web/kv.rs`), imported songs
  and the play history in IndexedDB (`ddi-library` version 3; the plays in
  `plays` and `play_summaries`, `web/src/history.rs`,
  `docs/plans/play-history.md`), replays on the play route
  (`#/play?replay=<play id>`, `web/src/replay.rs`). Compiles for `wasm32-unknown-unknown` only (empty when
  built natively).
- `desktop/` the desktop shell (`ddi-desktop`, run by `app` natively;
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
  movies (feature `video`, on by default: checked 8-bit H.264, HEVC and VP9 subsets through
  hardware VideoToolbox on macOS, experimental opt-in Media Foundation/D3D11
  on Windows (`mf.rs`, `mf_core.rs`; Windows Auto remains the module), the web's sandboxed FFmpeg module
  under wasmtime for the rest and on failure, `video.rs`, `video/`;
  movies in zips read in place or extracted once),
  a work's layers and lyrics, the theme's results, the history in the
  data folder, replays, `play/replays.rs`); a play's frames paced (`pacing/`: blocking in the
  acquire, or each started as late before its refresh as its
  cost allows (late with two frames in flight by default when a display
  link is available, block with one elsewhere); pure, with a simulated display in `pacing/sim.rs`, built
  for tests only: feature `sim`, which the crate's dev-dependency on
  itself turns on; the refreshes from a display link on macOS,
  `display_link.rs`: the view's `CADisplayLink` from macOS 14, else a
  `CVDisplayLink`);
  calibration's menu and tests (`calibrate.rs`); the
  devices' fingerprints and notices (`devices.rs`). Settings in
  `storage.json` in the config folder; `DDI_HOME` moves the config, data
  and cache folders under it (`paths.rs`). Tested without a sound device
  on a fake clock and offscreen (`desktop/tests/`; GPU tests skip without
  an adapter or with `DDI_NO_GPU=1`, as in CI's `desktop` job on Linux,
  macOS and Windows, with and without `video`). The video module is
  found at `DDI_VIDEO_MODULE`, in `video/` beside the executable, or (an
  executable built in the workspace) the workspace's `target/video/`,
  and compiled once into the cache folder (`DDI_VIDEO_CACHE` moves it;
  `cargo test -p ddi-desktop --test video` checks it on generated
  clips; `--decoder both` compares the platform's strict OS subset against it,
  `DDI_VIDEO_DECODER=module` forces the module for play; Windows experiments
  use `mediafoundation-auto` for module recovery, `mediafoundation` for strict
  refusal; `video/check-windows.ps1 -Module PATH` records the deferred-host
  fidelity run, without a window or sound). On Linux it builds
  against ALSA and udev (`libasound2-dev`, `libudev-dev`; LGPL, linked
  from the system); winit, rfd and arboard load their libraries at run
  time.
- `tools/danoni-dump/` the standalone Dancing☆Onigiri URL-import SDK
  (`rust/`), its confined Ruffle worker build inputs (`ruffle/`), desktop
  handoff probe (`desktop-probe/`) and independent chart verifier
  (`pack-verify/`). Separate workspaces and lockfiles: root Cargo checks do
  not cover them. `just danoni-check` (Python 3, ffmpeg and native build
  prerequisites) runs their locked checks, synthetic tests and independent
  Opus decoder comparison; `--minimal` checks the SDK without native/codec
  features, including on Windows in CI. No live songs or runtime downloads
  are part of this check. The SDK is not linked into the game yet; desktop
  integration will hand off prepared SSC/Opus through the verified managed
  catalog, never scan its transaction directories. Sources and generated
  third-party packs stay outside Git. See
  `docs/research/danoni-desktop-integration.md` for the remaining adapter,
  runtime attestation and redistribution work.
- `xtask/` generators: SEO fragments, the song manifest, `CREDITS.md` and
  `THIRD_PARTY_RUST.md`.
- `video/` the software video decoder: C shims over FFmpeg's video
  decoders (every format StepMania plays), built to
  `target/video/ddivideo.wasm` with wasi-sdk (LGPL module, never
  committed); `docs/plans/video-backgrounds.md`; additive bounded demux exports in
  `video/packets.c` (`video/packets.md`), read on desktop by
  `desktop/src/video/packets.rs` for native MP4/Matroska/WebM (`media.rs`,
  codec validation and output models in `codec.rs`, `h264_format.rs`,
  `hevc.rs` and `vp9.rs`).
- `assets/songs/<id>/` bundled songs with a `PROVENANCE.toml` each;
  `assets/noteskins/<game>/<name>/` bundled noteskins, the same way
  (`docs/plans/bundled-skins.md`); `assets/fonts/` the embedded HUD font
  and the theme's text face (Iosevka Term, served as `fonts/`).

## Commands

- `cargo run` opens the desktop app on our theme's song select
  (`--profile desktop` for smooth frames: `release` is the wasm bundle's,
  tuned for size and aborting on panics). Flags (`ddi_desktop::run`):
  `--songs DIR` (a folder for this run), `--sample`, `--screen
  select|results|options|calibration|credits`, `--dev`, `--widgets`,
  `--play SONG [--chart N] [--auto | --auto=pad] [--start]`,
  `--replay PLAY_ID [--results] [--start]` (a kept play's replay, or its
  results),
  `--calibrate visual|audio|combined [--auto] [--bias MS] [--start]`,
  `--fake-pad "1.0:13+,1.1:13-"`, `--silent` (no sound device: plays run
  on the host clock), `--volume V`, `--seconds N` (quits, printing the
  library, the clock's latency and jitter, the devices, the plays),
  `--lose-device-after S`, `--video-check FILE [--reference REF.yuv]
  [--frames] [--decoder auto|module|videotoolbox|mediafoundation|mediafoundation-auto|both]`, `--pacing block|late` (a play's frame pacing),
  `--in-flight 1|2` (defaults: late with two when a display link is
  available, block with one elsewhere), `--pacing-report` (block and late with one and two in
  flight, 4 s each in turn on Fifo, a table of each printed when the
  play ends; `docs/plans/desktop.md`, "Frame pacing"). **Agents' and tests' runs set
  `DDI_HOME=$PWD/target/scratch/<name>/home`** (never the maintainer's
  folders), use `--volume 0.005` or `--silent`, and run under
  `target/scratch/wd.sh`; prefer offscreen tests: a window takes the
  Mac's focus. The same crate built for wasm is the web app, which Trunk
  builds (`just dev`, `just build`).
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
- `just test` native tests (chart, engine, render, session, platform with
  `decode` and, through desktop, `decode-all` (its references against
  the `ffmpeg` command where it is installed), library with `nfkd`,
  actor, ui, theme, noteskin, lua, desktop, xtask), after writing the
  bundled songs' and noteskins' indexes the desktop's tests read. The
  app crate has no native tests; verify the web in a browser. Desktop's
  local checks: `DDI_WEB_PLAYS=<file>` (`--test play -- --ignored`: the
  judgements against the web's plays of the same charts),
  `DDI_WORKS=<dir>` and `DDI_WEB_WORKS=<jsonl>` (`--test works --
  --ignored`: local works against the web's import, autoplayed),
  `DDI_MOVIE_SONGS`, `DDI_AUDIO_DEVICE_TESTS=1`, `DDI_OFFSCREEN_PPM=<dir>`
  (keeps the offscreen pictures).
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
- `just video` builds the decoder module (downloads the pinned toolchain and
  FFmpeg into `target/video/`); every Trunk build runs it too when the
  module is missing (as after `cargo clean`, a few minutes) or older than
  its sources, and fails if it fails (`DDI_NO_VIDEO=1` builds the site
  without it); `just video-check [file]` checks it on a file
  or on generated clips against the `ffmpeg` command's decode.
- `cargo run -p xtask -- gen-songs` / `gen-noteskins` / `gen-credits`
  regenerate the song manifest, the bundled noteskins' index and the
  credits (Trunk runs them before every build; CI checks `CREDITS.md` and
  `THIRD_PARTY_RUST.md` are fresh). `gen-dito` regenerates the SVG art of Dito, the default skin
  (committed; CI checks it with `--check`).

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

Song select (`#/`) uses all four arrows to move through songs (Up/Left
previous, Down/Right next), waiting 250 ms for a possible double tap.
Double-tapping the same arrow changes chart on the current song (Up/Left
easier, Down/Right harder); held arrows scroll songs. Enter or Tab settles
a queued move first. Search, Back, pointer presses and focus changes
cancel it. Typing and navigation in the header keep their usual controls.
Song select is our theme's screen on the canvas: drive it by keys,
a fake pad or taps at `window.__DDI_DEBUG.ui.rects`, read
`__DDI_DEBUG.ui.select` and `__DDI_DEBUG.songs` (the library, the import,
the preview), set files on the hidden `.import-panel input[type=file]`
(the verify skill's "Song select"); `#/?gfx=gl` (and `auto`, `bias`,
`slow`, given there or on `#/play?…`) stay for the session: every play
Start opens and the way back to song select carry them. Song select's
autoplay switch (the header's `Autoplay` pill, `AutoIcon` on a phone;
`ui.theme.autoplay`; off at first, never saved) sets `auto=1` the same
way: its plays are flagged in the history, never counted, and their
results say AUTOPLAY. On a phone the results are one scrolled column
(`ui.theme.compact`, `scroll`; `ui.rects.ResultsClip`, `Back`).
Options (`#/options`) is our theme's screen too, on the stored settings:
drive it the same way (Up/Down rows, Left/Right change, Enter presses a
row's button, Tab the next section, Escape back). The layout selector above
the preview is available on every tab (P focuses it; Left/Right change it,
Up/Down return to the options). Read
`__DDI_DEBUG.ui.theme` (`section`, `optionId`, `value`, `rows`,
`notice`, `preview`) and `__DDI_DEBUG.options`, and the settings in
`localStorage` (`ddi.settings.v1`); the bind dialog takes keys and a
fake pad's presses as they are (the verify skill's "Options").
Calibration's menu (`#/calibrate`) and the credits (`#/credits`) are
pages on the canvas (`ui.theme.pageItems`, `pageFocus`; taps at
`ui.rects.Item<k>`). The device notices ("First time here?", "New audio
path: …") are drawn by our theme over every canvas page, read from
`ui.theme.appNotice` / `appActions`, pressed at `ui.rects.AppNotice.<id>`
(`later`, `defaults`, `calibrate`), reached by keys with F2 or Up past
the screen's top (`ui.chromeFocus`); the screen lies below them while
they show. A fresh profile shows the first-run one: seed
`audio_profiles` and `display_profiles` in `ddi.settings.v1` (or tap
`later`) when a test wants the screen without it.
Useful URL parameters: `#/play?song=<id>&chart=<n>` (chart index as in the
manifest; the play ends on the theme's results drawn on the canvas, what
they show in `window.__DDI_DEBUG.results`), `#/play?replay=<play id>` a
kept play's replay (`&view=results` its results; `__DDI_DEBUG.replay`:
`state`, `refused`, `fed`/`edges`, and at its end `check.matches`; the
verify skill's "Replays"; with the `ghost` setting a play's
`__DDI_DEBUG.ghost` is its lead over the chart's best play), `&auto=1` autoplay, `&bias=<ms>` make the autoplayer late/early,
`&gfx=gl` force WebGL2 (read at boot, or switching the device once for
the session; the Graphics setting chooses it too), `&auto=pad` autoplay through a fake gamepad the test
installs as `window.__DDI_FAKE_PAD` (see the verify skill), `&slow=1`
frames 40 ms longer while a movie is drawn (forces the automatic video
rule);
`#/calibrate/run?mode=visual|audio|combined` runs a
calibration (same `auto`/`bias` parameters); `#/video-check` runs the
video decoder on picked movies (`?decoder=both` compares the browser's
WebCodecs decoder with it frame by frame);
`#/skin-preview?skin=<key>` draws a noteskin's Down tap note alone
(`size`, `bg`, `beat`, `bpm`, `notes=4`, `receptors=1`, `gfx=gl`;
`demo=1` plays a looping autoplayed chart (every judgement, a miss,
holds and rolls kept and dropped, a mine), `rows`, `speed`, `layout=<id>`; `skin=builtin` for the
built-in arrows; a test sets `window.__DDI_PREVIEW_BEAT`);
`#/ui-test` draws the 2D pass's test scene (`t`, `scroll`, `delay`,
`budget`, `speed`, `all=1`; module doc of `web/src/components/ui_test.rs`);
`#/ui-dev` runs the UI core's developer screen (every primitive: focus,
dialog, popup, the text field over a hidden textarea, a flickable strip,
long press, a sound, a link and a file picker behind gestures) and
publishes `window.__DDI_DEBUG.ui` (screens, focus, the field's text,
selection and preedit, last requests, frames, actor rectangles);
`#/ui-dev?screen=widgets` (or the Widgets button) shows the widgets: a
10,000-song list, search with suggestions, option rows, a toggle, a
slider and results-like panels (`--widgets` in the desktop binary);
`#/ui-dev?screen=select|results|options|prompt` runs the theme's screens on
sample data (the bundled songs and made-up packs; `--screen select --sample` and
so on in the desktop binary), with their state in
`__DDI_DEBUG.ui.theme` (screen, song, style, chart, greyed, search,
popup, hints, pulse, compact, …; Options' field preview: where it
shows in `__DDI_DEBUG.ui.fieldPreview`, what draws it in
`fieldPreviewDrawn`, from the stored settings); a window under 480 CSS pixels high
or 600 wide gets the compact layout, and `prefers-reduced-motion` (or the
`reduced_motion` setting) stops the beat pulse.
`window.__DDI_LOSE_DEVICE()` loses the device (a WebGL2 context too), to test recovery;
`__DDI_DEBUG` carries the canvas's `screen`, `textures`, `text` and
`pass2d` counters, and on every page `history` (the play history:
`count`, `loaded`, `blocked`, `stale`, `unreadable`, `last` (the latest
play's summary and `id`), and on the play route, once the chart is
loaded, `song` and its charts' `bests`); on the play route `still`
(the song's opening still is loaded), while the start prompt shows
`prompt` (`step`, `notes`, `picture`, `frame`) and `prompt_backdrop`
(`still`, `background` or `none`: the picture behind it), and `video` (the movies' counters, `update_ms`, `paused`,
`guard`; `docs/plans/video-backgrounds.md`, step 8). Songs
played to the end or failed are kept (autoplay flagged, never counted
toward bests); Options' Data → Play history → Export downloads them as
JSON lines; song select's P (or "Plays…") lists the chosen chart's
plays (`ui.theme.plays`), W watches its best play's replay. Imported songs have ids
`u-<hash>` and play through the same route.

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
maintainer to confirm in Safari.

In calibration the autoplayer
follows the physical cue (heard audio, or drawn arrows in the muted test), not
the judged timeline; keep it that way or offset tests become meaningless. In
play (`&auto=1`) it presses on the judged timeline, so it stays exact under any
calibrated offset.

## Additional chart versions

- Options → Notes → Simplify → Without onigiri (`simplify_onigiri`)
  exposes additional versions of DanOni 5/7/9A/9B charts in Single,
  Solo, Double and Techno 8-panel (`techno-single8`). See
  `docs/plans/chart-simplification.md`.
- Generated charts carry `Chart::derivation` and are appended after all
  originals, independently of the display setting. Preserve original
  chart indexes, digests and files. Only the generated chart gets the
  assisted mark from this feature; the existing transform options keep
  their behavior.
- Chart-scoped lyrics and layers resolve through `Song::effects_chart`.
  Keep chart provenance in manifests, bests and replay resolution. Old
  browser imports upgrade their metadata from stored chart data; desktop
  invalidates its library cache when the metadata changes.
- Simfiles used by the library and play load through
  `ddi_library::scan::parse_simfile`, which appends generated versions too;
  the format importers preserve source charts, including SSC exports.

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
  `REUSE.toml` annotation and a `THIRD_PARTY.toml` entry (the credits ship
  its notice). Copy only from permissive sources: never from ITGmania
  (GPLv3 since 2024-09-20; StepMania 5.1 has the same older files under its
  MIT-style licence). Reading GPL code to learn behaviour is fine;
  transcribing it is not.

Third-party Rust dependencies keep their own licences (e.g. Symphonia is
MPL-2.0, file-level copyleft, fine to link from MIT code). Only bundled
assets and copied code are tracked in `REUSE.toml`. Their notices ship
with the game: `gen-credits` lists every crate in the web build (the app
and the Opus worker) and in the desktop binary (`cargo tree`, normal
dependencies without proc macros, Linux, macOS and Windows together) with
its licence texts from the crate's files, deduplicated, into
`THIRD_PARTY_RUST.md` and the Credits screen's folded "Rust libraries"
(`xtask/src/rust_credits.rs`); a licence outside its allowlist (MIT,
Apache-2.0, BSD, ISC, Zlib, 0BSD, Unicode, MPL-2.0, CC0, Unlicense,
BSL-1.0, the LLVM exception) fails the generator, so check a new
dependency's licence before adding it.

## Deployment

- GitHub Pages from the `main` branch via the workflow in `.github/`. The
  public URL is lowercase; on a custom domain Pages paths are case-sensitive,
  which is why the repository name is lowercase.
- `CREDITS.md` and `THIRD_PARTY_RUST.md` are generated and committed;
  `Trunk.toml` ignores them in the watcher so regenerating them does not
  loop the dev server.

## Conventions

- Use American English in first-party identifiers, comments, UI text and
  documentation (`color`, for example). Preserve copied upstream text and
  names required by external APIs or file formats.
- Logical file paths use `ddi_library::path::{RelPath, RelPathBuf}`; convert to OS paths only with `to_path(base)`, never by joining their text onto a `PathBuf`.
- Commit messages: imperative summary line, body explaining why.
- Keep `docs/PLAN.md` status and decisions current when scope changes.
- UI text avoids jargon from specific games except where naming a ruleset
  or a layout by StepMania's own name for its game ("Pump single", from
  StepMania's `pump` game, `docs/plans/pump.md`).
