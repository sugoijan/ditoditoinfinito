# Working on Dito Dito Infinito

Orientation for coding agents (and humans) on this repository. Keep it
accurate: when a convention here changes, change this file in the same commit.

## What this is

A dance rhythm game that plays StepMania simfiles; in genre terms a 4K VSRG
(four-key vertical scrolling rhythm game), which is the vocabulary to use when
researching comparable games or writing public copy. Written in Rust and shipped
as a static web app (wasm via Trunk, deployed to GitHub Pages). The gameplay
core is platform-free so a desktop build can follow. `docs/PLAN.md` holds the
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
  platform: the browser on the web, cosmic-text natively). No platform
  code.
- `platform/` traits and plain data the shells implement (audio clock, input,
  device fingerprints), plus the Ogg Vorbis fallback decoder behind the
  `decode` feature (Symphonia, MPL-2.0).
- `library/` song library logic shared by the app and xtask: pack scanning
  (which file is the chart, music, banner), a sans-IO zip reader, the manifest
  entry type. Pure, natively tested.
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
  (`ui/src/screens/`: song select, results, options) with their sample
  data; the state screens read (settings and
  their migrations, the song list's place, library views; stored through
  `ddi_platform::storage`, which each shell installs) and the developer
  screen both shells run (`ui/src/dev.rs`). Pure, natively tested.
- `theme/` our theme (`ddi-theme`, `docs/plans/theme.md`): `Infinito`,
  a look for the UI core whose components are the maintainer's mockups,
  its colours, faces, sizes, timings and places in
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
- `app/` the game (`ditoditoinfinito`, the workspace's default member):
  a `main` that starts the web shell on wasm and the desktop shell
  everywhere else; Trunk builds it for the browser (`index.html`),
  `cargo run` for desktop.
- `web/` the web shell (`ddi-web`): Yew for menus, one wgpu canvas and
  device for the whole app (`web/src/web/screen.rs`: made at boot, moved
  into the page that draws, made again after a lost device), Web Audio
  clock, settings in `localStorage` (`web/src/web/kv.rs`), imported songs
  and the play history in IndexedDB (`ddi-library` version 3; the plays in
  `plays` and `play_summaries`, `web/src/history.rs`,
  `docs/plans/play-history.md`). Compiles for `wasm32-unknown-unknown` only (empty when
  built natively).
- `desktop/` the desktop shell (`ddi-desktop`, run by `app` natively:
  `cargo run -- [--screen select|results|options | --dev | --widgets]
  [--seconds N]`): a winit window drawing the UI
  core through `render`'s painter with cosmic-text and system fonts,
  settings in `storage.json` in the user's config folder (the play
  history beside it, two files a play in `plays/`), file dialogs
  (rfd), clipboard (arboard), banners and backgrounds decoded from
  `assets/` (`image`). So far it runs the theme's screens on sample data
  (song select by default, `--screen` for another) and the developer
  screens (`--dev`, `--widgets`);
  audio and play come with the rest of phase 9. Built and tested by `just
  test`, `just core-check` and CI (it builds on Debian with no system
  libraries: winit, rfd and arboard load theirs at run time).
  `desktop/tests/offscreen.rs` draws the developer screen on the GPU and
  compares pixels with the web's (`DDI_OFFSCREEN_PPM=<file>` keeps the
  picture; skipped without an adapter); `theme_offscreen.rs` draws the
  theme's song select the same way.
- `xtask/` generators: SEO fragments, the song manifest, `CREDITS.md`.
- `video/` the software video decoder: a C shim over FFmpeg's video
  decoders (every format StepMania plays), built to
  `target/video/ddivideo.wasm` with wasi-sdk (LGPL module, never
  committed); `docs/plans/video-backgrounds.md`.
- `assets/songs/<id>/` bundled songs with a `PROVENANCE.toml` each;
  `assets/noteskins/<game>/<name>/` bundled noteskins, the same way
  (`docs/plans/bundled-skins.md`); `assets/fonts/` the embedded HUD font
  and the theme's text face (Iosevka Term, served as `fonts/`).

## Commands

- `cargo run` opens the desktop window on our theme's song select
  (sample data; `--release` for smooth frames; flags on
  `ddi_desktop::run`). The same crate built for wasm is the web app,
  which Trunk builds (`just dev`, `just build`).
- `just dev` dev server (auto-reloads on every rebuild; unsuitable for long
  runtime tests). `just dev 0.0.0.0` serves the local network, but a LAN
  address over HTTP is not a secure context (no gamepads, no WebGPU); from
  another machine, tunnel instead (`ssh -N -L 8080:127.0.0.1:8080 <host>`,
  then open `http://localhost:8080`).
- `just build` release build with the production public URL.
- `just check` native checks, wasm check, SEO regen, REUSE lint.
- `just test` native tests (chart, engine, render, platform with `decode`,
  library with `nfkd`, actor, ui, theme, noteskin, lua, desktop, xtask). The app crate has no native tests; verify it in a browser.
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
  credits (Trunk runs them before every build; CI checks `CREDITS.md` is
  fresh). `gen-dito` regenerates the SVG art of Dito, the default skin
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

Useful URL parameters: `#/play?song=<id>&chart=<n>` (chart index as in the
manifest; the play ends on the theme's results drawn on the canvas, what
they show in `window.__DDI_DEBUG.results`), `&auto=1` autoplay, `&bias=<ms>` make the autoplayer late/early,
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
`#/ui-dev?screen=select|results|options` runs the theme's screens on
sample data (the bundled songs and made-up packs; `--screen select` and
so on in the desktop binary), with their state in
`__DDI_DEBUG.ui.theme` (screen, song, style, chart, greyed, search,
popup, hints, pulse, compact, …); a window under 480 CSS pixels high
gets the compact layout, and `prefers-reduced-motion` (or the
`reduced_motion` setting) stops the beat pulse.
`window.__DDI_LOSE_DEVICE()` loses the device (a WebGL2 context too), to test recovery;
`__DDI_DEBUG` carries the canvas's `screen`, `textures`, `text` and
`pass2d` counters, and on every page `history` (the play history:
`count`, `loaded`, `blocked`, `stale`, `unreadable`, `last` (the latest
play's summary and `id`), and on the play route, once the chart is
loaded, `song` and its charts' `bests`). Songs
played to the end or failed are kept (autoplay flagged, never counted
toward bests); the Options page's "export play history" downloads them as
JSON lines. Imported songs have ids
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
assets and copied code are tracked in `REUSE.toml`.

## Deployment

- GitHub Pages from the `main` branch via the workflow in `.github/`. The
  public URL is lowercase; on a custom domain Pages paths are case-sensitive,
  which is why the repository name is lowercase.
- `CREDITS.md` is generated and committed; `Trunk.toml` ignores it in the
  watcher so regenerating it does not loop the dev server.

## Conventions

- Commit messages: imperative summary line, body explaining why.
- Keep `docs/PLAN.md` status and decisions current when scope changes.
- UI text avoids jargon from specific games except where naming a ruleset
  or a layout by StepMania's own name for its game ("Pump single", from
  StepMania's `pump` game, `docs/plans/pump.md`).
