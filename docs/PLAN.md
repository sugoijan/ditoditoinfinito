# DitoDitoInfinito — research summary and plan

A DDR-style rhythm game (genre term: 4K VSRG, four-key vertical scrolling
rhythm game) written entirely in Rust. Browser first (wasm, Trunk,
GitHub Pages at `https://sugoijan.dev/ditoditoinfinito/`), desktop later, with
the gameplay core shared unchanged between both.

Status (2026-10-21): phases 0–3 done and deployed (scaffold, chart
importers, engine, web platform, renderer, song select, options, credits,
four bundled OutFox Serenity songs), phase 4 (format breadth), phase 5
(import and storage), phase 6 (layouts as data: StepMania `dance-solo`,
`dance-double` and pump's single, half-double and double with ticked
holds; Dancing☆Onigiri works in 5, 7, 9A and 9B keys with their own
rules, colors, speed and lyrics as options) and phase 7 (gamepads,
bindings, touch lanes; WebHID left out) done, phase 8 partial (play
options, note color schemes, song backgrounds, image background changes
on both layers and background videos done; imported StepMania noteskins
with their Lua (on `lunita`, our hardened fork of piccolo), 3D models and
StepMania 3.9 and OpenITG skins, drawing every built-in layout, with a
preview; bundled ones: Dito, the built-in arrows as a skin and the
default, Delta, Alpha-Arrows, Dake, Peter Lawrence's CF_CHROME,
CF_VIBRANT, CF_ALLOY, Peters-DDR-Note and Peters-DDR-Vivid (3D models)
and `DanOni`; Dancing☆Onigiri's back and mask layers; StepMania's
named background effects and transitions, and scripted backgrounds'
first slice (`default.lua` folders, foregrounds too) since
2026-10-21), phase 9 in progress (the
UI core, our theme with a start menu, and the desktop shell at parity
with the web; left: hardware checks, Windows, packaging), phase 10
partial (replays and the ghost, importing plays, FFR levels, more
rulesets, the editor planned); per-phase status in the roadmap
(section 5). What both shells do lives in shared crates (`session`,
`shell`, `ui`), so a shell keeps only its platform's code.
DanOni URL imports (2026-10-19): the `danoni-importer` feature, on by
default for desktop since 2026-10-21 (never built for the web or
Windows), exposes desktop URL import through a packaged native helper and
framed IPC, with the ordinary SSC/Opus library handoff. The helper and SDK
share the root workspace/lock; the pinned generated Ruffle worker remains
separate. Browser URL import remains separate. See decision 51,
[the build contract](plans/danoni-importer.md) and
[the integration notes](research/danoni-desktop-integration.md).
Background continuity (2026-10-20): song select, pre-start and play share
an encoded Gaussian base. The prompt introduces softly blurred framed
content; Start finishes sharpening it without delaying audio. The play's
base follows current images/videos and layer crossfades. Both shells share
visual geometry/progress, with static filters cached and reduced motion
settled immediately ([theme transition](plans/theme.md#into-the-song-2026-10-20)).
Players import their own simfile packs (folder, zip or drag and drop; `.sm`,
`.ssc`, `.dwi`) into IndexedDB, edit a per-song offset on the results, and
Ogg Vorbis decodes in wasm where the browser cannot. Calibration runs on the real gameplay path (generated 120 BPM chart
with a click per note) and converges: each batch of 8 hits whose mean error
is significant (|mean| > 2σ/√n) is applied as a temporary correction, and the
test ends once 16 residual hits are within noise (`crates/engine/src/calibration.rs`,
`crates/web/src/calibration.rs`). Guided order: arrows only (muted, saves to the
visual offset), sound only (arrows hidden, any key, saves to the audio
offset), then a combined check. The two offsets are separable because the
audio offset moves judging and rendering together. Immediate fail
blanks the field and fades the music. A debug overlay (options) shows backend,
FPS, frame time, output latency, clock drift and timing-error statistics.
Offsets are stored per device profile (`ddi_platform::DeviceFingerprint`:
audio path from sample rate and latencies, display from screen size and DPR;
the measured refresh rate is shown but not part of the key because
variable-rate panels change it); an unknown device prompts to calibrate at
song start, and device changes mid-play are flagged on the results. Receptor
snap draws a note exactly on the receptor in its closest frame.

Options follow-up (2026-10-18): note colors and songs' own colors now
sit under Arrows, shown for Dito and Built-in only. A future noteskin
parameter mechanism should let skins declare controls and defaults,
persist values per skin and share them between preview and play. Dito's
color variants would use it first; retiring the visible Built-in choice
can follow once Dito covers its needs, keeping procedural arrows for
fallbacks. Done 2026-10-21 (decision 56, `docs/plans/skin-parameters.md`):
skins declare parameters in their metrics, kept per skin, read by their
Lua and shown under Arrows for the chosen skin; Dito's colors, songs' own
colors, explosions and a lighter drawing are its parameters, and the
Built-in choice is Dito's lighter drawing.

Song-list follow-up (2026-10-20): pack covers appear on headers, browsing
expands one pack at a time, and both shells persist the last open pack,
song and chart across launches. Search shows all matching packs and stays
per session without replacing the saved browsing position. Desktop
restores imported songs when its asynchronous library scan supplies them.

Detailed research with sources lives in [`docs/research/`](research/):

- [`formats-sm-ssc-dwi.md`](research/formats-sm-ssc-dwi.md) — StepMania `.sm`/`.ssc` and DWI formats, timing semantics, existing parsers, test fixtures.
- [`formats-ffr-danoni.md`](research/formats-ffr-danoni.md) — FlashFlashRevolution (R^3) and Dancing☆Onigiri (danoniplus) chart data, key modes, judgement, gauges.
- [`rules-ddr-itg-dwi.md`](research/rules-ddr-itg-dwi.md) — DDR / StepMania / ITG / DWI timing windows, scoring, gauges, options, note colors.
- [`tech-stack.md`](research/tech-stack.md) — Web Audio clock, input latency, rendering, storage, native parity, build/deploy.
- [`cc-songs.md`](research/cc-songs.md) — freely redistributable simfiles (OutFox Serenity per-song licences, Sockpuppet pack), traps, attribution requirements.
- [`video-backgrounds.md`](research/video-backgrounds.md) — what `#BGCHANGES` movies packs ship (AVI with H.264, Xvid, MPEG-2), what browsers decode, WebCodecs and a cut-down FFmpeg wasm module loaded on demand, StepMania's movie timing, the desktop path.
- [`noteskins.md`](research/noteskins.md) — StepMania's noteskin format (lookup, sprite sheets, beat-locked animation, holds, receptor and explosion commands), the Lua real skins depend on and how to run it in wasm, what players use and what may be bundled.
- [`ui-toolkits.md`](research/ui-toolkits.md) — drawing the menus ourselves: Rust GUI toolkits measured on wasm (egui, iced, fyrox-ui, yakui and others), Masonry checked in depth (builds for wasm on main, paints a renderer-neutral list, no tweens or menu navigation), game UI and other rhythm games (each built its own scene and tween layer), what to reuse.
- [`stepmania-themes.md`](research/stepmania-themes.md) — StepMania and ITGmania themes: the archive by engine, screens and their layers, actors and fallback, Lua per generation, screen ratios, noteskins, licences, support tiers.
- [`lua-sandbox.md`](research/lua-sandbox.md) — running untrusted Lua (noteskins, later backgrounds) with the same guarantees natively and in the browser: where rilua and piccolo fall short, what an in-process sandbox needs in Rust (no catchable panics or allocation failures on the web), why a hardened fork of piccolo, how much skin Lua runs during play, what scripted backgrounds and modfiles use.

Implementation plans for larger steps live in [`docs/plans/`](plans/), indexed with their status in [`docs/plans/README.md`](plans/README.md).

Reference implementations to consult while building: StepMania 5.1 (`5_1-new` branch) for parsing and timing semantics, [Project OutFox](https://projectoutfox.com/) (the actively developed StepMania fork; its [wiki](https://outfox.wiki/) documents formats and mode support and its Serenity packs are CC-BY), ITGmania / Simply Love for ITG rules, danoniplus for Dancing☆Onigiri, rCubed for FFR.

---

## 1. Goals and non-goals

Goals, in priority order:

1. Accurate, low-latency gameplay in the browser. Audio is the master clock; every input is timestamped and judged against it. This is the one thing a rhythm game cannot get wrong, so it is designed first.
2. A platform-free core. `chart`, `engine` and `render` crates have no web or OS code and are unit-tested natively. The web app and the future desktop app are thin shells.
3. Pluggable everything the brief names: input devices, song sources and formats, note skins and color schemes, background layers, and rulesets (judgement windows, scoring, combo, gauge, fail policy).
4. Eventually the union of DDR, StepMania, DWI, FFR and Dancing☆Onigiri features: holds, rolls, mines, lifts, fakes, shock arrows, speed/scroll changes, warps, arbitrary lane layouts (4-panel, 6-panel solo, 5-key onigiri, 7-key, …), lyrics and background events.

Non-goals for now: multiplayer, online scores, mobile as a sync-accurate target, keysounds, editor.

## 2. What the research settled

### 2.1 Chart formats (import sources)

| Source | Time base | Lanes | Note kinds | Timing features | Verdict |
|---|---|---|---|---|---|
| StepMania `.sm` | beats; 4-beat measures, rows at N ∈ {4,8,12,16,24,32,48,64,192} | per steps type (dance-single L,D,U,R; dance-solo L,UL,D,U,UR,R; double 2×4; …) | tap, hold, roll, mine, lift, fake, auto-keysound, attack | `#OFFSET` (time(beat 0) = −OFFSET), `#BPMS`, `#STOPS`, `#DELAYS`; negative BPM/stop = warp | **MVP** |
| StepMania `.ssc` | same | same | same | + `#WARPS`, `#SPEEDS` (beat=ratio=delay=unit), `#SCROLLS`, `#FAKES`, `#LABELS`, `#TIMESIGNATURES`, `#TICKCOUNTS`, `#COMBOS`; per-chart split timing (v ≥ 0.70) | phase 2 |
| DWI `.dwi` | beats; one char = 1/8 note, brackets `()`=1/16 `[]`=1/24 `{}`=1/64 `` `' ``=1/192 | SINGLE/DOUBLE/COUPLE/SOLO; numpad digits, `A`=U+D, `B`=L+R, `C`–`M` solo combos, `<…>` jumps | tap, hold (`X!Y`, released at next note in column) | `#GAP` ms (OFFSET = −GAP/1000), `#BPM`, `#CHANGEBPM`/`#FREEZE` in **quarter-beat** units, freeze in ms | phase 2 (small) |
| Dancing☆Onigiri (dos) | integer frames @ 60 fps; `blankFrame`/`startFrame`/`adjustment`/`playbackRate` | arbitrary, defined as data (5…23+ keys; onigiri/giko/iyo lanes; per-lane color group, shuffle group, scroll sign, position, default keys) | tap, hold (`frz*_data` pairs), dummy notes | no BPM; `speed_data` (global), `boost_data` (per-note), `color_data`/`acolor_data`/`ncolor_data`, `word_data`, `back_data`/`mask_data`, `scrollch_data`, `keych_data`; per-chart gauge params in `difData` | phase 3 |
| FFR (R^3 `beatBox`) | integer frames @ 30 fps (+ optional ms, + per-song `sync`) | 4 fixed (L,D,U,R) | tap only; color stored per note | none | late; content is not redistributable, importer only for user-owned data |
| KSF, BMS, `.osu`, `.sma` | various | | | | not planned; notes in research doc |

Semantics an importer must get right (all verified against StepMania source):

- Stop pauses **after** the row's notes are judged; delay pauses **before**. Delay is applied first when both sit on a row.
- Warps skip time; notes inside are unjudgeable unless a stop/delay sits on that exact row. `.sm` negative BPMs/stops convert to warps with StepMania's algorithm.
- `#SPEEDS` scale scroll speed with linear interpolation over beats or seconds; `#SCROLLS` rescale displayed beat distance. Neither changes judgement time.
- Hold tails (`3`) are derived from the head; rolls are heads of kind `4`.
- SM's internal grid is 48 rows per beat; it covers every quantization above and every DWI bracket. We adopt it.

Intentional deviations from StepMania 5.1 in our importer (more forgiving, documented in `crates/chart/src/formats/sm.rs`): per-note `{mods:len}` attack suffixes are skipped rather than read as a column; `.ssc` files containing negative BPMs/stops are converted to warps like `.sm` instead of being rejected. `A` note characters are ignored like SM 5.1.

The `.dwi` importer (`crates/chart/src/formats/dwi.rs`, whose module doc lists every deviation) follows the loader StepMania shipped up to 5.1.0. The current `5_1-new` branch carries an ITGmania change (`829f49f622`) that misreads jumps (`<24>` places only Down); we do not reproduce it. Other deviations: steps-type tags are case-insensitive, panels missing from the layout are dropped instead of landing in column 0, `#DISPLAYBPM` is read as decimal, non-positive freezes are dropped, and a chart StepMania would assert on is skipped.

### 2.2 Rules (judgement, scoring, combo, gauge)

The five games disagree on nearly every axis, which is the argument for a data-driven `Ruleset`:

| Axis | DDR (A→WORLD) | StepMania 5 default | ITG / Simply Love | FFR | Dancing☆Onigiri |
|---|---|---|---|---|---|
| Windows | Marvelous ±16.7 ms, Perfect ±33, Great ±92, Good ±142 (community table; A+ is ms-resolution and undocumented; one measured EXTREME table is asymmetric) | 22.5 / 45 / 90 / 135 / 180 ms (`scale*x + add`) | 21.5 / 43 / 102 / 135 / 180 + 1.5 ms add; FA+ 13.5 | asymmetric 30 fps table: −118…+117 ms, Amazing −18…+17, no late Average | symmetric 60 fps buckets ±2/4/6/8 frames; Excessive −9…−16 frames |
| Empty press | ignored | ignored | ignored | **Boo**: −5 score, −5 life | ignored unless Excessive on |
| Combo | Good keeps combo (2013+); 1 per row; OK doesn't add; NG breaks | Great keeps; 1 per note | same as SM | Miss breaks only | Shobon/Uwan break; Matari hides; separate freeze combo |
| Score | 1,000,000 × weighted% (5/5/3/1/OK 5) − 10 per non-Marvelous, rounded to 10; EX 3/2/1/OK 3 | DP percent 3/2/1/0/0, Held 3, mine −2; grade weights 2/2/1/0/−4/−8 | DP 5/4/2/0/−6/−12, Held 5, mine −6; EX 3.5/3/2/1 | raw ±50/50/25/5/−5/−10 + end formula with combo×1000 | 1,000,000 × (8/4/2/Kita 8 + combo terms)/(n×10); ranks SS≥97%… |
| Gauge | starts 50%; fixed % per judgement (community-measured, ~+0.4…1.2% Perfect, −4.8…−10% Miss, consecutive-miss and DANGER modifiers); LIFE4; RISKY; FLARE; GRADE | +0.008/+0.008/+0.004/0/−0.040/−0.080, Held +0.008, LetGo −0.080, mine −0.160; LifeDifficulty multiply/divide; regen lockout after miss; battery 4 lives | W5 −0.050, Miss −0.100, mine −0.050 | 0–100, start 50, ±5 | `{init, border, recovery, damage}` per gauge/chart; fixed or scaled by note count; max 1000 |
| Fail | immediate at 0 (Premium: continue, score frozen) | Immediate / ImmediateContinue / EndOfSong / Off | same | immediate at 0 | border gauges evaluate at song end; life gauges fail at 0 |
| Holds | no release judgement; brief release tolerated; OK = full step, no combo; NG breaks | hold life decays over 0.25 s released, reset on press; roll decays over 0.5 s, reset on tap | hold 0.32 s, roll 0.35 s | none | start within ±4 frames, release tolerance `frzAttempt` 5 frames |

Grade tables verified: DDR A (AAA ≥ 990k … D < 550k), Simply Love (★★★★ 100% … D < 55%), StepMania (AAA all-W2, AA 93%, A 80%, B 65%, C 45%).

### 2.3 Technology

Decisions, with the reasoning in `tech-stack.md`:

- **Clock.** `AudioContext.currentTime` is authoritative. `getOutputTimestamp()` pairs it with `performance.now()`, which is also the timeline of `KeyboardEvent.timeStamp` and rAF timestamps. `getOutputTimestamp().contextTime` is already the sample leaving the device (it accounts for output latency), so the web backend adds `outputLatency` back to report a `currentTime`-equivalent, and the engine subtracts it once: heard time = context time − `outputLatency` − user audio offset. Render at predicted presentation time + user visual offset. Sanity-check `getOutputTimestamp` (Safari had a broken implementation) and fall back to back-to-back sampling.
- **Playback.** Decode whole song with `decodeAudioData` into one `AudioBuffer` kept in browser memory (never copied into wasm), start with ~100 ms lookahead via `AudioBufferSourceNode.start(when, offset)`. Song-select previews use Web Audio too (a context created in the click, the song decoded, a slice played with fades; before the player's first input the selected song decodes on an `OfflineAudioContext`, which needs no activation, so that input only starts the sound: 50 ms from the first key in Chrome and Firefox, against 300 and 680–760 ms when it decoded after the key, measured 2026-10-16 on Beatucada with stock autoplay rules): `<audio>` cannot play Ogg in Safari, and Safari only starts audio synchronously inside the gesture. Prefer OGG/Opus; MP3 without a LAME/Xing header has decoder-dependent ~25 ms gaps that StepMania never trimmed, so per-song offset editing is a first-class feature.
- **Input.** Keyboard via `event.code` + `event.timeStamp`, ignore repeats, track pressed set. Gamepads must be polled; use a `MessageChannel` 1 ms wake loop rather than rAF (DDR is the W3C's own motivating example). Dance pads enumerate as HID gamepads; WebHID is a Chromium-only extra for later. As built (phase 7): `ddi_platform::gamepad::PadTracker` turns polled snapshots into edges (buttons, axis halves with hysteresis, hat switches reported as one axis) and stamps each edge with `Gamepad.timestamp` when it moved forward and is plausible for that poll, else with the poll time; the web poller (`crates/web/src/web/gamepad.rs`) reads every pad in one JS call and runs the 1 ms loop only during play with a pad connected (option to poll per frame instead). The loop alternates a `MessageChannel` message and a `setTimeout` for the remaining time (a timeout set from a message task is not nested, so not clamped to 4 ms): ~900 polls/s at ~8–11% of the main thread in headless Chrome, against ~90% for a plain ping-pong spin, with no frame drops. Measured through a scripted fake pad (`auto=pad`): edges stamped with the pad's timestamp are judged exactly; poll-time stamping adds about +0.5 ms. All sources are merged, sorted by time and resolved by `LaneInput` (every press steps; a lane releases when its last control does). Touch lanes use pointer events on the play canvas.
- **Rendering.** wgpu 30 with `webgpu` + `webgl` features (WebGPU is Baseline since Jan 2026 but Firefox/Linux still needs the fallback), atlas-instanced sprites, glyphon for dynamic text, pre-rendered sprites for judgement words. Every screen drawn by the game's own UI core (decision 45); the web shell's pages are components of our own (`crates/web/src/component.rs`; Yew until 2026-10-16), plain DOM only for `#/video-check`, the crash page and the no-GPU page. No winit on web; rAF + web-sys. Same `render` crate on desktop under winit. Bevy and macroquad rejected: their audio layers hide the hardware clock and scheduled starts.
- **Storage/import.** Bundled demo songs via Trunk `copy-dir`; user imports via `<input webkitdirectory>`, drag-and-drop (`webkitGetAsEntry` for folders) or zip, stored as `Blob`s in IndexedDB, `navigator.storage.persist()` after the first import. Zips are read sans-IO by slicing the `File` (`ddi-library::zip`: central directory, then one entry at a time; stored entries go to storage as slices without entering wasm), so a large pack never sits in wasm memory. IndexedDB instead of OPFS: see decision 8.
- **Fallback decode.** When `decodeAudioData` rejects a file, or returns silence (Safari ≥ 18.4 accepts Ogg Vorbis and decodes it to silence; detected by sampling windows across the buffer), Symphonia 0.6 (features `ogg` + `vorbis` only, ≈ 95 KB gzip, MPL-2.0) decodes it in wasm (`ddi-platform` feature `decode`). It covers Safari's Ogg Vorbis gap. Ogg Opus goes to the Opus worker (`rusty-opus`, fetched only then; decision 9), whose float WAV `decodeAudioData` resamples to the context's rate.
- **Desktop parity** (`docs/plans/desktop.md`, 2026-10-16). Audio is our own small mixer on cpal, which gives each callback the host time its buffer is heard at: the web's `getOutputTimestamp()` pair, so the song clock is unchanged. This replaces the earlier choice of `web-audio-api` 1.7, whose `current_time()` moves in render blocks (measured: 0.9–1.7 ms jitter against cpal's 2–14 µs). Keys through winit, stamped by the OS on macOS (an `NSEvent` monitor) and on receipt elsewhere; controllers through gilrs on its own thread; Opus through `rusty-opus` (pure Rust, matches libopus); movies through the OS's decoders where a stream is checked, else FFmpeg in a sandboxed helper process (decision 54; the FFmpeg module under wasmtime until 2026-10-20).
- **Constraints.** GitHub Pages cannot set COOP/COEP: no SharedArrayBuffer, no wasm threads, no cpal AudioWorklet backend, 100 µs timers in Chrome. The design never needs them. Firefox rounds event timestamps to 1–2 ms; `resistFingerprinting` makes it 100 ms, so detect and warn.
- **Size budget.** ≤ 2 MB compressed first load (wgpu both backends ≈ 2–3 MB raw, glyphon ≈ 1–2 MB raw; Yew, gone since 2026-10-16, was 97 KB raw) with `opt-level="z"`, fat LTO, `wasm-opt -Oz`.
- **Crate versions (2026-10):** wgpu 30.0.1, glyphon 0.12, winit 0.30.13, cpal 0.18.2, gilrs 0.11.2, zip 8.x, symphonia 0.6.1, trunk 0.21.14. No usable `.sm` parser crate exists; we write our own and differential-test against `rgchart` 0.0.15 / `danceparser` 0.2.1.

---

## 3. Architecture

### 3.1 Workspace layout

Mirrors `../heddobureika` / `../noynoynoy` (Cargo workspace, edition 2024, Trunk, `justfile`, `xtask regen-seo`, boot shell in `site/index.html`, Pages workflow), with the game split into more crates because the platform-independence is the point.

```
DitoDitoInfinito/
  Cargo.toml                 workspace; default-members = ["crates/app"]
  Trunk.toml  justfile
  site/         index.html  styles.css  boot-init.js  seo/ (metadata.toml, favicon.svg): the web page
  .github/workflows/deploy.yml
  crates/
    chart/        ddi-chart     chart model + TimingMap + importers (sm, ssc, dwi, danoni, ffr)
    engine/       ddi-engine    SongClock, judge, combo, score, gauge, scroll, options → Frame + events
    render/       ddi-render    wgpu scene (atlas sprites, text), NoteSkin, background layers
    platform/     ddi-platform  traits: AudioBackend, InputSource, Surface; MP3 as StepMania reads it; optional decoders
    library/      ddi-library   pack scanning, sans-IO zip reader, manifest entries (shared by app and xtask)
    actor/        ddi-actor     StepMania's actor model: tweens, effects, sprite states, commands, model data (shared by noteskins and screens)
    ui/           ddi-ui        UI core: events → requests, hit-testing, gestures, navigation and focus, screens, dialogs, text field
    theme/        ddi-theme     our theme's look for the UI core's screens
    noteskin/     ddi-noteskin  StepMania noteskins (metrics, redirects, models, Lua at load)
    lua/          ddi-lua       StepMania's Lua for skins, sandboxed
    session/      ddi-session   one play session for both shells (play, calibration, backgrounds' rules)   [lib, both]
    shell/        ddi-shell     what both shells do between settings and renderer (field options, a work's layers' textures)   [lib, both]
    app/          ditoditoinfinito  the game: starts `web` on wasm, `desktop` natively [bin]
    web/          ddi-web       web shell, web-sys platform impls, canvas host     [lib, wasm32]
    desktop/      ddi-desktop   winit + wgpu + cpal + gilrs, movies in a sandboxed helper   [lib, native]
    opus-worker/  ddi-opus-worker  the web's Ogg Opus fallback decoder   [bin, wasm32 worker]
    importer-protocol/  ddi-importer-protocol  the app/importer helper's IPC contract
  video/                      the software video decoder (FFmpeg shims, sandboxed helper, its protocol)
  tools/                      the DanOni URL importer helper and SDK
  xtask/                      regen-seo, song manifest, noteskins' index, credits
  assets/       songs/ (CC-licensed .sm fixtures), noteskins/, fonts/
  docs/
```

Dependency direction: `app` → `web` (wasm) or `desktop` (native); `web` and `desktop` → {`shell`, `session`, their platform impls, `render`, `engine`, `library`, `chart`}; `shell` → `session`, `render`, `ui`; `session` → `engine`, `chart`, `library`, `platform`, `ui`; `library` → `chart`, `engine` (a work's charts' keys), `platform`; `render` → `engine` (reads `Frame`), `actor`; `noteskin` → `actor`; `ui` → `actor`, `engine`; `engine` → `chart`; `platform`, `actor` → nothing. `chart`, `engine`, `actor`, `ui`, `platform`, `session`, `shell` compile for native and wasm and are tested with plain `cargo test`.

### 3.2 `chart`: the internal model

```rust
/// 48 ticks per beat (StepMania's ROWS_PER_BEAT): exact for 4th…192nd and all DWI brackets.
pub struct Tick(pub i64);

pub struct TimingMap {
    offset_seconds: f64,              // time(beat 0) = -offset (SM sign convention)
    bpms:   Vec<(Tick, f64)>,
    stops:  Vec<(Tick, f64)>,         // pause after row is judged
    delays: Vec<(Tick, f64)>,         // pause before row is judged
    warps:  Vec<(Tick, Tick)>,        // (start, length) skipped
    speeds: Vec<SpeedSegment>,        // ratio, delay, unit ∈ {Beats, Seconds}
    scrolls: Vec<(Tick, f64)>,
    fakes:  Vec<(Tick, Tick)>,
    time_signatures, tick_counts, combos, labels …
}
impl TimingMap {
    fn seconds_at(&self, t: Tick) -> f64;         // StepMania's FindEvent ordering
    fn tick_at(&self, seconds: f64) -> BeatPos;   // with in_stop / in_delay flags
    fn displayed_beat(&self, t: Tick) -> f64;     // scroll segments
    fn judgeable(&self, t: Tick) -> bool;         // !warp && !fake (stop/delay exception)
}

pub enum NoteKind { Tap, HoldHead { end: Tick }, RollHead { end: Tick }, Mine, Lift, Fake,
                    Shock /* DDR whole-row */, AutoKeysound, Dummy /* visual only */ }
pub struct Note { pub tick: Tick, pub lane: u8, pub kind: NoteKind,
                  pub keysound: Option<u16>, pub color: Option<ColorRef>, pub speed_mul: Option<f32> }

pub struct Layout {               // "steps type" / key mode as data, not an enum
    pub id: String,               // "dance-single", "dance-solo", "pump-single", "danoni-5", …
    pub family: LayoutFamily,     // StepMania | Danoni: picks per-family defaults
    pub lanes: Vec<Lane>,         // name, label, glyph (Arrow(rotation_deg) | Onigiri | Giko | Iyo | …),
                                  // color_group, shuffle_group, scroll_sign, column, row,
                                  // pad (which physical pad: double = 0,0,0,0,1,1,1,1), default_keys
    pub turn_left: Option<Vec<u8>>, // StepMania GetTrackMapping "Left"; Right is its inverse
    pub players: u8,
}
// Charts name their layout by id; `Song::layout_of(chart)` looks in the
// song's own `layouts` (file-defined, e.g. DanOni custom keys) first, then
// in `Layout::builtin`. A chart whose layout resolves nowhere is not played.

pub struct Chart { layout: LayoutId, difficulty: Difficulty, meter: u32, name, credit,
                   notes: Vec<Note>, timing: Option<TimingMap> /* SSC split timing */,
                   ruleset_hints: RulesetHints /* DanOni difData gauge params, FFR judge window */ }

pub struct Song { title, subtitle, artist, (translits), genre, credit, music: AssetRef,
                  preview: (start, len), banner/background/jacket: Option<AssetRef>,
                  timing: TimingMap, display_bpm: DisplayBpm, charts: Vec<Chart>,
                  effects: Vec<EffectEvent> /* BGCHANGES, FGCHANGES, DanOni word/back/mask/color */,
                  keysounds: Vec<AssetRef>, source: SourceInfo /* format, version, unknown tags */ }
```

Frame-based sources (DanOni 60 fps, FFR 30 fps) import losslessly onto this grid with a synthetic constant tempo: at 75 BPM one beat is 0.8 s = 48 frames at 60 fps, so **1 frame = 1 tick** (FFR: 1,500 BPM, 40 ticks a frame, so millisecond-timed notes fit too; `docs/plans/phase-10.md`). `blankFrame`/`startFrame`/`adjustment`/`sync` fold into `offset_seconds`; `playbackRate` becomes a music-rate hint. Quantization-based coloring is meaningless for those charts, which matches the source games (DanOni colors per lane group, FFR stores colors per note), so `Note.color` carries the explicit color and skins honour it.

Importers implement `trait ChartImporter { fn sniff(bytes, name) -> bool; fn import(files: &dyn FileSet) -> Result<Song> }`. `FileSet` abstracts a directory, a zip, or a single text file so the same code runs on web and desktop. Exporting is out of scope but the model keeps enough (`source.unknown_tags`, SSC version) not to preclude it.

### 3.3 `engine`: deterministic gameplay core

Inputs: a `Song` + chosen `Chart`, a `Ruleset`, `PlayOptions`, a stream of `InputEvent { lane, pressed: bool, host_time }`, and `ClockSample`s. Outputs: a `Frame` (what to draw: note positions, receptor state, judgement to flash, combo, gauge, score) and `JudgeEvent`s (for sounds, stats, replays). No I/O, no floats where a tick will do, no allocation in the hot path. Tests feed recorded input logs and assert judgement sequences, so every ruleset is regression-tested natively.

```rust
pub struct SongClock { /* anchor (context_time, host_time), output_latency, audio_offset,
                          visual_offset, rate; low-pass filtered, drift-corrected */ }
impl SongClock { fn song_time_at_host(&self, host: f64) -> f64; fn heard_now(&self) -> f64; fn render_time(&self, predicted_present: f64) -> f64; }

pub struct Ruleset {
    pub judge: JudgeTable,     // tiers with independent early/late bounds (s), scale + add,
                               // hold/roll/mine windows, optional W0 sub-window, rescore-early flag,
                               // empty-press policy (Ignore | Boo | ExcessiveEarly { lo, hi, factor })
    pub combo: ComboRules,     // lowest tier that continues, per-row vs per-note, OK/NG behaviour
    pub score: Box<dyn ScoreRules>,   // DdrMoney(SN2 | A), DancePoints{weights}, FfrRaw, DanOni; + EX secondary
    pub gauge: Box<dyn GaugeRules>,   // Bar{deltas, start, modifiers}, Battery{lives, costs}, Danoni{init,border,rec,dmg,scaled}, Flare…
    pub fail:  FailPolicy,     // Immediate | ImmediateContinue | EndOfSong { threshold } | Off
    pub grades: GradeTable,    // thresholds over money / percent / EX + FC lamps
    pub names: JudgeNames,     // Marvelous/Perfect/… vs Fantastic/Excellent/… vs イイ/シャキン/…
}
```

Most of a ruleset is data (serde, shippable as presets: `ddr-a`, `ddr-extreme`, `sm5`, `itg`, `itg-fa+`, `ffr`, `danoni-normal`, …). `ScoreRules` and `GaugeRules` are traits because they carry state and formulas (DDR's step ordinal and running combo, StepMania's regen lockout, DanOni's note-count scaling). The hold model is StepMania's decaying hold-life with a configurable window, which reproduces DDR's "brief release tolerated" and ITG rolls.

`PlayOptions` (as built): `scroll` (x-mod or c-mod speed, reverse, scroll action Normal/Boost/Brake/Wave), `transform` (turn Off/Mirror/Left/Right/Shuffle; cut: timing Off/Quarters/Eighths, no jumps, no holds), `appearance` (Visible/Hidden/Sudden/HiddenSudden/Stealth), `seed` (fixes Shuffle), `clock` (audio and visual offsets, rate), `visible_range`, `receptor_snap`. It is `#[serde(default)]`, and the option enums deserialize leniently (`ddi_engine::lenient`: an unknown value falls back to that field's default instead of failing the whole settings object). `Player::new` applies the transform (`crates/engine/src/transform.rs`) before building the judge, so judging, the score's note counts and the results all describe the transformed chart; ticks never change, so judged times cannot. Scroll actions (`ScrollAction::adjust`) and appearance (`crates/engine/src/appearance.rs`) only change a note's drawn height and its alpha/glow in the `Frame`. `Results` carry the options played, the lane map and an `assist` flag (any cut). The autoplayer's press script (`crates/engine/src/autoplay.rs`) is built from the transformed notes. The maths follows StepMania; sources in `rules-ddr-itg-dwi.md` §4.1. Not built yet: lane covers (Hidden+/Sudden+), music rate, a separate judge offset in the UI.

### 3.4 `render` and skins

`render` consumes `Frame` and draws with wgpu: one instanced quad pipeline over a texture atlas, glyphon for score/combo text, pre-baked sprites for judgement words. It never touches web-sys or winit; the shell gives it `Device`, `Queue`, a `TextureView` and the DPI-scaled size, and the game lives in a fixed virtual coordinate space, letterboxed.

```rust
pub trait NoteSkin {
    fn receptor(&self, lane: &Lane, beat_phase: f32) -> Sprite;
    fn note(&self, lane: &Lane, kind: NoteKind, q: Quantization, beat_frac: f32, song_beat: f64,
            explicit: Option<ColorRef>) -> Sprite;   // static, quantized (NOTE), progress (RAINBOW), cycling (VIVID/FLAT)
    fn hold_body(&self, …) -> Sprite;  fn explosion(&self, judgement) -> Sprite;
}
pub trait BackgroundLayer { fn update(&mut self, song_time: f64, events: &[EffectEvent], frame: &Frame); fn draw(&self, …); }
```

Skins are data (atlas image + TOML describing per-lane rotation or per-lane sprites, color tables, animation) plus optional Rust implementations for procedural ones. Backgrounds are optional layers: none, static image, SM `BGCHANGES` playback, DanOni `back_data`, and built-in generative visualizers driven by beat/judgement events.

### 3.5 `platform` traits and the web shell

```rust
pub trait AudioBackend { type Buffer; type Error;
    fn now(&self) -> ClockSample;                 // { context_time, host_time, output_latency }
    fn play(&mut self, b: &Self::Buffer, start_at_context_time: f64, offset: f64, volume: f32) -> Result<SoundId, Self::Error>;
    fn duration(&self, b: &Self::Buffer) -> f64; fn sample_rate(&self) -> f64;
    fn master_ramp(&mut self, from: Option<f32>, to: f32, at: Option<f64>, seconds: f64);
    fn stop_all(&mut self); fn running(&self) -> bool; fn fingerprint(&self) -> Option<DeviceFingerprint>;
    fn buffer_from_pcm(&self, channels: Vec<Vec<f32>>, rate: f64) -> Result<Self::Buffer, Self::Error>; }
pub trait InputSource { fn poll(&mut self, out: &mut Vec<RawInput>); }   // RawInput { device, control, pressed, host_time }
```

Decoding stays with each shell (the browser's `decodeAudioData`, desktop's own), and so does the song library: there is no `SongStore`. What both share is the import's core, `ddi_library::scan` (a song's entry, its files found as StepMania finds them, the report's warnings), which the web runs on `Blob`s and desktop on its folders, and the play itself, `ddi_session::play::PlaySession`, generic over the `AudioBackend` and the shell's inputs (`docs/plans/desktop.md`, step 7.1).

`Bindings` maps `(device, slot, control)` → lane and lives in the engine so a dance pad, a keyboard and a touch overlay are interchangeable; `LaneInput` adds the per-lane hold state on top. The saved bindings (`ddi_engine::bindings::ControlBindings`, flattened into the app's settings) keep one keyboard table per layout and, per controller model, a `dance-single` table plus one per other layout; `ControlBindings::resolve(layout, connected pads)` builds a play's `Bindings` (decision 21). Web implementations: `WebAudioBackend` (noynoynoy's `audio.rs` is the starting point), `WebKeyboard`, `Gamepads` (MessageChannel loop, `crates/web/src/web/gamepad.rs`), `TouchLanes` (`crates/web/src/web/touch.rs`), `BundledStore` (fetch under `assets/songs/`), the IndexedDB song library (`crates/web/src/songs.rs`, `crates/web/src/web/idb.rs`) and the folder/zip/drop import (`crates/web/src/import.rs`, `crates/web/src/web/files.rs`). The web app owns routing (hash router) and the pages that host the UI core's screens and the game loop (rAF) on the canvas (Yew until 2026-10-16). Desktop later reuses everything except these implementations.

---

## 4. MVP / proof of concept

One screenful of scope, deliberately small, but built through the abstractions so that widening it later is additive.

**In:**

- `.sm` import for `dance-single` (taps, holds, mines parsed; mines not yet judged), BPM changes and stops, negative-BPM→warp conversion. Three pure CC-BY songs from OutFox Serenity bundled under the scheme in section 8 (REUSE.toml, LICENSES/, generated credits, in-game Credits screen).
- Keyboard input (arrows and a second binding set like D/F/J/K), rebindable in a simple settings panel.
- Engine with one `Layout` (4-panel) and two presets to prove the plug point: `itg` and `ddr-a`-style money score. Taps + holds (OK/NG with decaying hold life), combo, gauge bar, immediate fail, grade.
- wgpu renderer with one note skin (quantization-colored arrows, DDR "NOTE" style), receptors, hold bodies, judgement sprites, combo/score text. x-mod speed 1–8, reverse scroll.
- Audio clock design in full (anchor filtering, output latency, calibration): the calibration screen measures audio offset (tap to a click) and visual offset (tap to a moving marker) and stores them.
- Yew screens: song select with preview, options, gameplay, results. Hash routes so a song can be deep-linked.
- Trunk build, `justfile`, `xtask regen-seo`, GitHub Pages workflow, entry added to `../landing-page/links.toml`.

**Out (next phases):** `.ssc`, `.dwi`, DanOni, FFR; rolls, lifts, fakes, shock arrows; speeds/scrolls/warps rendering; gamepads; user import and storage; skin selection; backgrounds; desktop.

Definition of done: a chart plays in Chrome, Firefox and Safari with judgements that stay within a few ms of a known-good StepMania setup after calibration, verified with a recorded input log that is also a unit test.

## 5. Roadmap

Status as of 2026-10-21: one line per phase, the full notes below the table; known gaps before phase 9 after them. The plans and their status are indexed in `docs/plans/README.md`.

| Phase | Status | Deliverable | Notes |
|---|---|---|---|
| 0 Scaffold | done | workspace, crates with empty APIs, Trunk boot shell, SEO xtask, justfile, `REUSE.toml` + `LICENSES/` + `reuse lint` in CI, CI deploy of a "hello" page | copy conventions from noynoynoy/heddobureika and detonito |
| 1 Core | done | `chart` (`.sm` parser, `TimingMap` with seconds↔tick, warp conversion) and `engine` (clock, judge, holds, combo, score, gauge, scroll) with native tests and replay fixtures; differential tests vs `rgchart`/`danceparser` | no browser needed; this is where correctness lives. Differential tests against two oracle parsers and corpus autoplay (notes below) |
| 2 Web platform | done | `WebAudioBackend`, keyboard, canvas + wgpu renderer, game loop; play a bundled chart | first playable |
| 3 MVP | done | song select, options, calibration, results, bundled songs with `PROVENANCE.toml`, `xtask gen-credits` + Credits screen, deploy | deployed with four OutFox Serenity songs |
| 4 Format breadth | done | `.ssc` (split timing, warps, speeds, scrolls, fakes), `.dwi`, rolls/lifts/fakes/mines judged, shock arrows | `.ssc` (speeds/scrolls drawn) and `.dwi` (hand-written fixtures, cross-checked against an equivalent `.sm`); rolls/lifts/fakes/mines verified in play with autoplay on a hand-made chart; shock arrows are full mine rows under the DDR A rules (decision 42); `#COMBOS` applied (decision 43) |
| 5 Import & storage | done | folder/zip/drag-drop import, OPFS/IDB store, per-song offset editor, Symphonia fallback decode | IndexedDB only (decision 8); folder handles re-read in place on Chromium (decision 32); notes below |
| 6 Layouts & DanOni | done | `Layout` as data in the UI, 6-panel solo, DanOni 5/7/9A/11 key modes (7i until 2026-10-13), onigiri lane, `speed_data`/`boost_data`, color events, lyrics (`word_data`) | solo, double and pump layouts (decisions 20–22, 40); Dancing☆Onigiri key modes and works (decisions 25–29); notes below |
| 7 Input devices | done | gamepad poller, dance-pad profiles, touch lanes, WebHID experiment | 1 ms poll loop with pad timestamps, bindings per controller model, touch lanes; WebHID out of scope; notes below |
| 8 Presentation | partial | skin packs (static/quantized/rainbow/vivid), background layers (BGCHANGES, DanOni back/mask, generative), DDR option set (Boost/Brake/Wave, Hidden/Sudden, Turn, Cut) | done: options, color schemes, `#BGCHANGES`/`#BGCHANGES2`, background videos (`docs/plans/video-backgrounds.md`), noteskins with Lua, models and legacy formats (`docs/plans/noteskins.md` and the skin plans), DanOni overlays and StepMania's named background effects and transitions (`docs/plans/overlays.md`). Scripted backgrounds and `#FGCHANGES`: `default.lua` folders done (`docs/plans/scripted-backgrounds.md`); not done: their movies, StepMania 3.9's and ITG's formats; notes below |
| 9 UI core & desktop | in progress | the screens drawn by our own UI core (`actor`, `ui`) on web and desktop (`docs/plans/ui-core.md`, decision 45); then the `desktop` crate: winit + wgpu + cpal + gilrs (`docs/plans/desktop.md`); same `engine`/`render`/`chart` | UI core and theme done (`docs/plans/ui-core.md`, `docs/plans/theme.md`); desktop parity steps 7.0–7.9 done (`docs/plans/desktop.md`); movies through OS decoders and a sandboxed FFmpeg helper (`docs/plans/native-video.md`); packaging and draft releases from tags (`docs/plans/packaging.md`); start menu since 2026-10-21 (`docs/plans/start-menu.md`); next: hardware checks, frame pacing measurements, Windows video; notes below |
| 10 Later | partial | FFR `beatBox` import (user-owned data), replays (from the play history, phase 9), more rulesets (DDR legacy scoring, FLARE, LIFE4), editor | replays and the ghost done (decision 49, `docs/plans/play-history.md`); importing plays done, FFR levels imported on both shells (`docs/plans/phase-10.md`; no `ffr` preset, `docs/plans/rulesets.md`); more rulesets done (decision 57, `docs/plans/rulesets.md`: `ddr-extreme`, DDR A's LIFE4, RISKY, FLARE and FLOATING FLARE gauges with FLARE SKILL, LIFE4's lamps); editor planned (`docs/plans/editor.md`); not done: the `ffr` preset, the editor itself |

### Phase notes

What each phase delivered and how it was checked, in full.

**1 Core.** No browser needed; this is where correctness lives. Differential tests (`crates/chart/tests/differential.rs`): 300 seeded random `.sm` files per oracle, structure against `danceparser`, note times against `rgchart` (±2 ms); both agree. `DDI_DIFF_CORPUS=<dir>` runs the same comparison over any local song folder, plus a `.dwi`-against-`.sm` check where a folder holds both; it is opt-in and not part of CI. On 2026-10-14 every local chart agrees (1,147 `.sm` files, 458 DWI charts against their conversions), after teaching the test three oracle quirks (danceparser keeps every BPM change on one beat and misreads a row across its 4 KiB read buffer; rgchart times nothing without an `#OFFSET`); `DDI_AUTOPLAY_CORPUS=<dir>` autoplays every chart of a folder (decision 44): 30,960 plays of the local corpus all perfect, 4,198 shock rows among them under DDR A. In headless Chrome (WebGPU) and Firefox (WebGL2), a scratch pack of local songs imported and autoplayed to the results all top tier: pump charts with ticked holds and H.264 MP4 movies, a `#COMBOS` chart, still-image background changes, an AVI with a DWI/SM pair (the `.sm` taken), a chart with mines just before notes (none hit) and DDR shock rows on single and double (a shock stepped on by hand: one N.G., all lanes go off, the arrows hidden for a moment). Headless Firefox's software WebGL2 drops frames with a movie, so the automatic setting turns movies off there; with them on, they play

**5 Import & storage.** IndexedDB only (decision 8); per-song offset on the results screen; Ogg Vorbis fallback only. Banners/backgrounds without tags are classified by image size like StepMania. Imports verified by hand in Firefox and Safari, including private windows; Firefox asks once per site to confirm sharing a folder's files. Chromium keeps a picked folder's handle (`showDirectoryPicker`) and re-reads it in place, removing songs whose folders are gone (decision 32); Shift-JIS simfiles and zip names decode (decision 31); imported packs show their banners (decision 30); the browser is asked to keep the library from a click or drop, and the song list says whether it does

**6 Layouts & DanOni.** Layouts resolved through the song (decision 20), StepMania `dance-solo` and `dance-double` playable end to end (pump's `pump-single`, `pump-halfdouble` and `pump-double` since 2026-10-13, decision 40): style selector on the song list (decision 22), keyboard and controller bindings per layout with the guided flow, double on two controllers (decision 21), touch columns per layout, Turn/Shuffle per layout, shuffle order on the results with each lane's arrow; imports keep any pack with a playable chart and list left-out layouts in the details for pack authors. Native replay fixtures autoplay solo and double all top tier under every turn and cut on the ITG, SM5 and DDR A presets; headless Chrome autoplays both on WebGPU and WebGL2, two identical fake pads play one half of double each, `auto=pad` on double stays all top tier. Every built-in Dancing☆Onigiri key mode (5 … 23 keys, pattern 0) is `Layout` data (decision 25), of which 5, 7, 9A and 9B keys are played (decision 27; 7i until 2026-10-13); two-row modes (kept for later) draw each row's receptors at its own end with its own scroll direction, touch splits such fields into halves, the bindings page and the bind flow offer the key modes the library has charts for; native fixtures autoplay 5, 7, 7i, 9A, 11, 12, 17 and 23 keys all top tier under every turn, headless Chrome autoplays 5 to 17 keys (charts written as `.sm` files with a `danoni-<mode>` steps type) on both backends. Dancing☆Onigiri works import (decision 26), in the key modes the game plays (decision 27): pages with inline or external dos (Shift_JIS and other declared charsets decoded by the browser, music found where danoniplus loads it or by name, encoded music scripts decoded) and dumps of the research tool's format; every chart of a work with holds, dummy charts, `speed_data`/`speed_change` as scroll segments and `boost_data` per note; one song per music file, the work's fields stored once for all of them; chart buttons show the chart's name; the x-mod plays a work at its declared tempo, or the chart's own speed as an option (decision 23); the work's note colors (`setColor`, `frzColor`, `color_data`, `acolor_data`, `ncolor_data`) as an option; `startFrame` starts the play part-way, `endFrame` stops and `fadeFrame` fades the music; lyrics (`word_data`) over the field (decision 29). Rules: "the ruleset below for every chart" by default, "each chart's own game's rules" and "Dancing☆Onigiri rules for every chart" as options, with danoniplus's frame windows, hold rules, gauges (including a work's custom ones), score and ranks (decision 28). `DDI_DANONI_CORPUS=<dir> cargo test -p ddi-chart --test danoni_corpus` compares every captured chart of a local dump corpus with danoniplus's own parse (lane names, tap and hold frames, speed and boost events, adjustment): the 9 references of the 2026-10-09 corpus match exactly; headless Chrome imports danoniplus's sample pages and the corpus and autoplays 5, 7 and 7i charts all top tier on both backends (AP and 1,000,000 under a work's own rules), with chart colors on and off; the onigiri, giko and iyo symbols are checked under the vivid, rainbow and subdivision schemes. Left out: key-group switching (`keych_data`; a work like Defeat,I Doll draws all its lanes at once), `playbackRate` (no work at hand uses a rate other than 1; it needs judging windows in real time and a rate on the music), divided external dos, `wordRev_data` (`back_data`/`mask_data` since 2026-10-14, phase 8)

**7 Input devices.** 1 ms `MessageChannel` poll loop with pad-timestamp/poll-time stamping (`PadTracker`, natively tested); bindings per controller model (`Gamepad.id`) for buttons, axis directions and hat directions, with a guided "press each arrow" flow for the keyboard and any pad and standard-mapping defaults; controller Start/Back on the play screen; touch lanes aligned to the receptors. Verified in headless Chrome with a scripted fake `getGamepads` (`auto=pad` plays a chart through it). The song list moves with a controller, a dance pad's arrows and Start/Back, or the arrow keys (decision 30). Not done: WebHID (out of scope), a Safari-specific pad start (Safari does not treat pad presses as a gesture, so the prompt asks for a tap)

**8 Presentation.** Done: Turn (Mirror/Left/Right/seeded Shuffle), Cut (quarters or eighths only, no jumps, no holds; marked as assist on the results), Boost/Brake/Wave, Hidden/Sudden/Hidden+Sudden/Stealth, all following StepMania's maths; options persisted, summarised on song select and the results, and ignored by calibration. Native replay fixtures show identical judgements under every scroll action and appearance, lane-permuted judgements under every turn, and an all-top-judgement autoplay on the ITG, SM5 and DDR A presets under every turn × cut × scroll action, with each appearance paired in; headless Chrome autoplays (WebGPU and WebGL2) with several options on at once also end with only top judgements. The song's static background drawn behind the field (decoded by the browser, copied into a texture; brightness, default 40 %, and a DDR-style field filter behind the lanes, default 40 %). Note color schemes on the procedural skin: by subdivision, vivid and flat cycling rainbows (StepMania's `midi-vivid` hues and 4-beat cycle, with the tail-to-tip gradient along each arrow measured on DDR footage) and rainbow by position (DDR's bands, each a two-shade gradient flowing once per beat), plus a single color; vivid is the default (`crates/render/src/note_colors.rs`). Both backends draw in linear light through an sRGB view (decision 17). `#BGCHANGES` playback for still images (`crates/library/src/backgrounds.rs`, decision 18), verified with a synthetic pack on both backends; the second layer, `#BGCHANGES2`, since 2026-10-16 (decision 48): drawn over the first with its images (their transparency kept) and movies, with what StepMania gives that layer alone, checked natively, offscreen on desktop and with a synthetic pack in headless Chrome (WebGPU, WebGL2) and Firefox. Background videos (decision 35; `docs/plans/video-backgrounds.md`, researched in `docs/research/video-backgrounds.md`): every format StepMania plays, through a cut-down FFmpeg compiled to wasm (`just video`, `just video-check`) that runs in a worker and is fetched only when a song needs it; H.264 AVIs, nearly all pack movies, through the browser's decoder (WebCodecs) with the module's frame times and exact seeks, so a song whose movies are all H.264 never downloads the module; played behind the field on the audio clock as StepMania times them (rates, loops, rewinds, resuming a movie where it left off); imports sniff each movie and store or link it; the Background videos setting (automatic turns a movie off for the rest of a song when it costs frames; off never fetches the decoder) and the background frame (decision 34); the module's licence notices are served next to it and its credits generated from `video/FFMPEG.toml`. Checked: the module on generated clips under Node and in the browser (`#/video-check`), the H.264 order and times against `ffprobe` on 93 local H.264 AVIs, the browser's decoder against the module frame by frame in Chrome (pixels) and Firefox (times; its own colors), full autoplays on WebGPU and WebGL2 all top tier with no frame dropped or late, and a build without the module (the song background, one line on the results). Noteskins (decision 36; `docs/plans/noteskins.md`, researched in `docs/research/noteskins.md`): StepMania noteskins import with songs or on their own (folders, zips, `NoteSkins` trees), stored in the browser, chosen under Options as the arrows of the dance layouts (and since 2026-10-13 of Dancing☆Onigiri's and pump's, `docs/plans/danoni-skins.md`, `docs/plans/pump.md`); drawn as StepMania draws them (fallback chains to stand-ins for its own `default` and `common`, lookups and redirects, sheets and file name hints, beat-locked frames and vivid, color by texture translation, holds with tiled bodies and caps under normal and reverse scroll, receptors and explosions driven by the engine's events with the metrics' commands), at first without a skin's Lua or 3D models (what needed them was drawn by the built-in skin), both since drawn too (below); checked with a synthetic skin in headless Chrome on WebGPU and WebGL2 (all top tier) and on the 54 dance skins of StepMania, ITGmania and OutFox natively. The Lua runtime skins will run on (`lua/`, `docs/plans/lua.md`): a hardened fork of piccolo with per-skin memory and string limits, fuel and a deadline per call, StepMania's `cmd` keyword and Lua 5.1's globals, checked on the local skin corpus and on adversarial cases natively and in wasm; a skin's Lua runs at load and in play (`crates/noteskin/src/lua/`, with StepMania's own Lua for its `default` and `common`): its elements built through it, the engine's events, queued commands and update functions played through it each frame within a fuel budget, the load in slices so the page keeps drawing; a skin whose Lua stops is drawn without it (natively at load, built in for the rest of a play) and turned off after a second stop; all 345 local skins load and play natively, and synthetic skins in headless Chrome on WebGPU and WebGL2. Noteskins' 3D models (`docs/plans/models.md`): MilkShape meshes, materials, animated textures and rigid bones, drawn as StepMania draws them in the field's perspective (back faces culled, a model's own faces sorted, color by texture translation masked per vertex, textures scrolled on the beat, bones posed from their keys); a preview page draws a skin's notes alone (`#/skin-preview`, `docs/plans/skin-preview.md`), checked against Peter Lawrence's published renders, and plays a short autoplayed chart (taps hit in every judgement's window and a miss, holds and rolls kept and dropped, a mine; `crates/engine/src/demo.rs`) on any built-in layout, which Options' field preview now plays with the options chosen; all 338 model files of the local corpora parse, and StepMania's, ITGmania's and Peter Lawrence's model skins draw in headless Chrome (WebGPU, WebGL2) and Firefox. Skins in StepMania 3.9's and OpenITG's format (`docs/plans/legacy-skins.md`): their `.sprite`, `.actor`, `.model` and XML actor files built natively, a file per note type, OpenITG's receptor and explosion rows on the engine's events, and a stand-in for OpenITG's own `dance/default` by its layout; Peter Lawrence's 16 Open-ITG and Not-ITG skins build every element as their own natively and play in headless Chrome (WebGPU, WebGL2) and Firefox. Dancing☆Onigiri's back and mask layers since 2026-10-14 (decision 41, `docs/plans/overlays.md`). StepMania's named background effects and transitions since 2026-10-21 (`docs/plans/overlays.md`, "Named effects and transitions"): every effect StepMania 5.1 ships (placements at the image's own size, in quarters and stretched, `Color1`/`Color2` tints, `OverlayAdd`'s and the visualizations' additive blend, the flashes, two files) and every transition (crossfades, the soft-edged `Fade*` wipes, the `Slide*` moves) on both layers, as data rather than theme scripts; checked natively, offscreen on desktop and with a synthetic pack in headless Chrome (WebGPU, WebGL2) and Firefox. Scripted backgrounds and `#FGCHANGES` since 2026-10-21 (`docs/plans/scripted-backgrounds.md`): folders with a `default.lua` on either layer and as foregrounds, their Lua on the noteskins' sandboxed host in a VM of the song's own (the folder seen at `/Songs/<pack>/<song>/`), `InitState`, `On`, `GainFocus` and `LoseFocus` at the changes, foregrounds for their tweens' time, one fuel budget a frame and a stop for them all (a note on the results), drawn on StepMania's screen in the background frame on both shells; checked natively, offscreen on desktop and with a synthetic pack in headless Chrome (WebGPU, WebGL2) and Firefox. Their text (`BitmapText`) since, in the platform's sans-serif face. Their sounds since, on the play's sound. Not done: their movies (and a movie as an effect's second file: the movie slots decode one file a segment) and render targets, StepMania 3.9's and ITG's formats (`BGAnimation.ini`, `default.xml`, 3.0-style layer folders), which StepMania 5.1 itself plays only in quirks mode

**9 UI core & desktop.** UI core steps 0–6 done 2026-10-16 (`actor`, `ui`, the 2D pass, our theme `theme/`, `docs/plans/theme.md`: every web page is the theme's screens on the canvas, song select, options, results, the start prompt, calibration's menu and outcomes, the credits, the device notices; Dancing☆Onigiri's lyrics in the scene; no Yew; the play history, `docs/plans/play-history.md`); desktop parity steps 7.0–7.9 done 2026-10-16 (`docs/plans/desktop.md`: one session for both shells in `session/` and the import's core in `library/`; cpal with our own mixer and CoreAudio's latency, every music format, MP3 as StepMania 5.1 decodes it; the library read in place from folders and zips, simfiles and works; keys with the OS's stamps, controllers through gilrs; play with noteskins and their Lua, backgrounds, movies (since 2026-10-20 through the OS decoders and a sandboxed FFmpeg helper process; wasmtime dropped), a work's layers and lyrics, the theme's results and the history; calibration and device profiles; CI on Linux, macOS and Windows); next: the maintainer's checks on hardware (listed at the top of `docs/plans/desktop.md`), frame pacing: late with two frames in flight defaults on when a macOS display link is available, block with one elsewhere (`--pacing-report` compares all four); the follow-up held 120 Hz with no acquire-inferred misses in late·2 and 4.20 ms mean frame start to estimated shown; target gaps are now distinguished from observed repeats, fullscreen and direct presentation measurements remain; macOS hardware video for checked H.264/HEVC/VP9 subsets implemented with exact seeks and software fallback, MP4/Matroska/WebM packets demuxed by the sandboxed helper (`video/helper`); Windows Media Foundation experimental and opt-in, wider stream classes next (decision 50); packaging done 2026-10-21 (`docs/plans/packaging.md`: an ad hoc signed `.app` in a zip and a `.dmg`, a Linux tarball, a Windows zip, built by `release.yml` for draft releases from tags); SDL3 if winit timestamps are a problem

**10 Later.** Replays done (2026-10-16, decision 49, `docs/plans/play-history.md` "Replays"): a kept play watched again on both shells from its results ("Watch replay"), from song select's plays list for a chart (P, or a long press on a phone's chart: newest first, the best marked, its results, removal) and its best (W), `#/play?replay=<id>` and desktop's `--replay`; its results checked against the record's, refusals said; the ghost of the chart's best play beside a play (off by default). Not done: importing plays

### Beyond the original plan

Start menu (implemented 2026-10-21; [plan](plans/start-menu.md)): a
Singleplayer-first entry before song select, disabled Multiplayer hint,
Options returning to its caller, and desktop Quit. Back from ordinary
song browsing returns to the silent menu in one press, preserving its filter
and place; Clear resets search explicitly. Web Start is `#/`, Songs is
`#/select`; app-owned history preserves callers across reload. The shared
`TitleMenu` actor boundary prepares future theme work.

Done beyond the original plan: calibration that converges on the real
gameplay path, per-device audio/display offset profiles with app-wide banners,
receptor snap, fail fade, cancel gesture, debug overlay, calibration drawn
with the chosen noteskin (2026-10-14; in the audio test, which hides the
notes, with the beat held still so that a skin pulsing on it gives no
cue, and any key judged for the one lane but shown on its own receptor),
a search over the
song list (2026-10-14: every word typed found in a song's title, artist,
their transliterations, pack, credits, chart names or step artists,
ignoring case, accents, width and katakana/hiragana, and in romaji both
ways: fields with kana and the transliterations romanised (kana in
Hepburn) and reduced to one romaji spelling, so `sakura` finds さくら and
かくめい finds 革命 transliterated `KAKUMEI`, while Latin fields are
matched only as written (no `Lemon` for `moon`); from three characters,
two with any beyond ASCII (Japanese), or on a space or Enter;
the packs it finds songs in open, other styles with matches offered; kept
across a play; `/` to search, Enter or Down into the list; songs imported
before it are found by their transliterations once imported again), and
autoplay in play pressing on the judged timeline, exact under any
calibrated offset (2026-10-15; calibration's autoplayer still follows the
heard or drawn cue).

Song select navigation (2026-10-20): all four arrows browse songs with
provisional continuous motion, isolated difficulty pairs, opposite-pair
chaining and capped accelerating holds. Timings come from the maintainer's
local calibration tool; both shells share the recognizer, physical release
routing and list motion (`docs/plans/song-navigation.md`, decision 53).

Additional onigiri-free charts (2026-10-18): Options can expose 5-key →
Single, 7-key → Solo, 9A → Double and 9B → Techno 8-panel versions alongside
their originals. Only the generated versions are assisted by this feature;
indexes, original digests and replays remain stable. See
[`chart-simplification.md`](plans/chart-simplification.md).

Menu backgrounds (2026-10-18): cached Gaussian-filtered images replace the
offset copies. Filtering runs once at 256 pixels on upload; frames draw
one sprite per layer. Sharp pictures keep separate cache entries, including
the start prompt's central picture (`docs/plans/theme.md`).


### Known gaps before phase 9 (2026-10-15, reviewed 2026-10-21)

Everything known to be missing, approximate or unchecked before the
desktop build, gathered in one place; what has been built since is
removed. Items marked *new* were found in
code comments or test reports and were not written down before; the
others point to where they are described.

Charts and import:

- *New:* `.sm` routine charts keep only the first player's notes
  (`crates/chart/src/formats/sm.rs`, module doc); routine and couple are not
  played anyway (`docs/plans/pump.md`).
- *New:* `#ATTACKS` (song and chart) is parsed, but no modifier is ever
  applied; per-note `{mods}` are skipped (section 2.1).
- `.lrc` lyrics, jackets, CD titles, `.ssc` per-chart `#MUSIC` and
  `#PREVIEW` are used since 2026-10-21 (`docs/plans/format-gaps.md`);
  `#PREVIEWVID` is read but not shown.
- *New:* zips with encrypted entries, compression other than stored or
  deflate, or several disks are refused (`crates/library/src/zip.rs`;
  kept, `docs/plans/format-gaps.md` decision 11).
- Layouts not played (couple, routine, techno, kb7, para, ez2, …) are
  left out with a note (decision 27); KSF, BMS and `.osu` are not
  read (section 2.1); FFR levels play under the rules chosen until
  theirs come (phase 10); keysounds are a non-goal (section 1).
- Korean and Chinese legacy encodings are told apart by a heuristic
  (`ddi_library::legacy`, `docs/plans/format-gaps.md`): a short title
  may still be misread.
- Songs imported before a change need importing again to gain it:
  transliterations for search, pump charts, legacy skins' files,
  Shift-JIS, Korean and Chinese text, background files named with `,`
  or `=`, lyrics, jackets, CD titles, chart and preview music (this
  section; `docs/plans/format-gaps.md`, `docs/plans/pump.md`,
  `docs/plans/legacy-skins.md`, decision 31).

Rules and options:

- *New:* the `itg`, `itg-fa+`, `sm5`, `ddr-a`, `ddr-extreme` and `danoni`
  presets exist; `ffr` (section 3.3) is not built, nor DDR SuperNOVA2/X's
  rules (`docs/plans/rulesets.md`).
- Not built: lane covers (Hidden+/Sudden+), music rate, a separate judge
  offset (section 3.3); the editor (phase 10); LIFE4's standings and the
  FLARE SKILL total (across plays and DDR's versions;
  `docs/plans/rulesets.md`). The battery is an option of the StepMania
  presets, not a preset (`docs/plans/rules-options.md`).
- DDR A windows and gauge are approximations (section 7), as are its
  FLARE drains (community estimates) and DDR EXTREME's life bar
  (StepMania 3.9's; `docs/plans/rulesets.md`);
  the shock window and hide time are not measured (decision 42);
  `#COMBOS` has no miss counter and comes from `.ssc` only (decision 43);
  ticked holds' known deviations (`docs/plans/pump.md`).

Play screen:

- O.K. / N.G. for holds and DDR shock rows show over their lanes, a
  little past the receptors, animated as StepMania 5.1's `_fallback`
  `HoldJudgment` (2026-10-16; `render::scene::hold_judgements`, the
  ruleset's names: "Held" and "Let Go" under ITG), centered there and
  sized to fit the lane; none in the pump game (StepMania's
  `HoldJudgmentY` −99999 there), and with ticked holds only the shocks';
  under Dancing☆Onigiri's rules its freeze judgement, still, for 60
  frames, beside the tap judgement as danoniplus places it (two arrows
  right, 1.4 down). Drawn as text in our HUD face, not as the default
  theme's graphic (StepMania's `_fallback` has none; no noteskin carries
  one); StepMania's squash at the end is a fade here. The play ends once
  the last O.K. or N.G. has shown (`Player::end_time`; StepMania keeps
  drawing judgements through its out transition).
- *New:* a skin's `Def.Sound` is kept silent
  (`crates/noteskin/src/lua/convert.rs`); the game's own mine sound and
  assist clap, made in code, are the event sounds
  (`docs/plans/rules-options.md`).
- *New:* shock rows are drawn as the skin's mine; StepMania skins have no
  shock graphic (decision 42).
- *New:* in the calibration audio test the beat is held still, but a
  skin effect timed in seconds still runs and could give a visual cue.
- Hold checkpoints show no judgement word (`docs/plans/pump.md`);
  Dancing☆Onigiri's rank X is not shown (decision 28).

Menus:

- *New:* search: Down on a controller from the field lands on the first
  pack header; the list has no way back up into the field, and only the
  `/` key reaches it (no controller); a short query or Enter on a large
  library opens most packs (not yet tried on the maintainer's full
  library); kanji titles without a transliteration are not found by
  romaji (no reading dictionary); input methods were only simulated
  (to try in Firefox, and in Safari after a push).
- *New:* the guided controller binding records one control per lane; a
  second set (face buttons beside the d-pad) comes only from the standard
  defaults, while the keyboard has "add key".
- The skin preview shows the chosen skin only; lifts and fakes are not in
  its demo (`docs/plans/skin-preview.md`).

Audio and video:

- *New:* Firefox: the maintainer saw lag and a slow "preparing audio…"
  on a song with a software-decoded movie, not reproduced headless and
  not seen again after the refresh-rate fix
  (`docs/plans/video-backgrounds.md`). An autoplay that failed before
  the judged-timeline change needed a calibrated offset near 100 ms,
  which may mean Firefox reports no output latency there; unchecked.
- *New:* Safari did not start an msmpeg4v3 movie (FFmpeg module) on a
  local origin; WebKit plays it. To check on the published URL.
- wasm memory grows after a fallback decode of Ogg Vorbis or MP3 (Opus
  decodes in a worker that is ended after); previews decode the whole
  song (sections 2.3, 7).
- Videos: AV1 does not play, the module is 2.4 MB, Firefox's colors
  for SD movies are its own, MP4 through WebCodecs and movie audio are
  later, `-random-` picks images only (section 7,
  `docs/research/video-backgrounds.md`, `docs/plans/video-backgrounds.md`).
- Phase 8 left out: the generative backgrounds and visualizers of the
  phase's deliverable (*new*: never started). The named background
  effects are done (2026-10-21) but for a movie as an effect's second
  file; scripted backgrounds and `#FGCHANGES` have their first slice
  (2026-10-21): what is left is listed in
  `docs/plans/scripted-backgrounds.md` (movies, render targets, the
  older formats).

Noteskins, Lua and models:

- Center notes for the other bundled skins (`docs/plans/pump.md`; pump
  skins, `NoteSkins/pump/*`, since 2026-10-16); OutFox's `global/` skins
  (`docs/plans/noteskins.md`); StepMania 5.1's "newskin" format
  (`docs/research/noteskins.md`).
- *New:* legacy actor files' `Quad`, `BitmapText`, `ActorScroller` and
  `BGAnimation` are not drawn (`crates/noteskin/src/legacy.rs`); the cabinet
  light effect clocks are reported and ignored (`crates/noteskin/src/actor.rs`).
- *New:* quads beyond one device buffer in a frame are not drawn
  (`crates/render/src/textured.rs`).
- *New:* a bundled skin's Lua stopping is untested (none stops).
- Known drawing differences and Lua gaps (`docs/plans/noteskins.md`,
  `docs/plans/lua.md`), model features left out
  (`docs/plans/models.md`), Lua actors drawn by the built-in skin
  (`docs/plans/lua.md`), the VM's known costs (`docs/plans/piccolo-fork.md`),
  NotITG's mods (`docs/plans/legacy-skins.md`); a skin's Lua state over a
  retry, skin images' storage (section 7).
- Not checked by the maintainer: a phone, WebGPU in Firefox
  (`docs/plans/bundled-skins.md`).

Dancing☆Onigiri:

- *New:* the danoniplus sample "Combative InstinctⅡ", chart 2 (5 keys),
  imports with no notes and ends at once; unexplained (a quirk of the
  sample or of the importer), left as is.
- Key-group switching, `playbackRate`, divided external dos,
  `wordRev_data`; two-row and other key modes; key patterns past the
  first; layers' color boxes, transforms and script-computed positions
  (phase 6, decisions 25–27, `docs/plans/overlays.md`,
  `docs/plans/danoni-skins.md`).

Input and browsers:

- *New:* Firefox's coarse timestamps (`resistFingerprinting`) are named
  as a risk with "detect and warn" (section 7), but nothing detects them.
- *New:* checks waiting for Safari on the published URL: pump charts with
  ticked holds, the search field's look and input method, the
  msmpeg4v3 movie above.
- WebHID left out; Safari takes no pad press as a gesture; small touch
  targets on phones; storage eviction without `persist()`; a browser
  refusing the sRGB view (phase 7, section 7).

Tooling:

- The local-corpus tests (differential, autoplay, skins, Lua) are opt-in,
  not in CI (phase 1, `AGENTS.md`).

## 6. Decisions (1–7 confirmed 2026-10-06; 8–10 made with phase 5, 2026-10-07; 11–12 with phase 7, 2026-10-07; 13–19 with phase 8, 2026-10-08; 20–29 with phase 6, 2026-10-08/09; 30–33 with the loose ends, 2026-10-09; 34–35 with the background videos, 2026-10-10; 36 with noteskins, 2026-10-10; 37–39 with bundled noteskins and skins' scripts, 2026-10-12; 40 with pump layouts, 2026-10-13; 41 with the overlays, 2026-10-14; 42–44 with the local corpus sweep, 2026-10-14; 45 with the UI core, 2026-10-15; 46–47 with desktop parity, 2026-10-16; 48–49 with the second layer and replays, 2026-10-16; 50 with native video, 2026-10-17; 51–52 with DanOni URL imports and simplified charts, 2026-10-18; 53–54 with song navigation and the video helper, 2026-10-20; 55 with the rules options, 56 with skin parameters, 57 with more rulesets, 2026-10-21)

1. **Licence: MIT.** Bundled songs keep their own licences in `assets/songs/*/LICENSE`.
2. **Rendering: wgpu from day one** (WebGPU + WebGL2 fallback), not Canvas2D, because desktop parity is a stated goal and heddobureika already proves the wgpu+Trunk path. Costs ~1 MB on the wire.
3. **Menus in Yew, gameplay in wgpu.** Same split as heddobureika; the HUD stays in the renderer so it exists on desktop. Superseded by decision 45 (2026-10-15).
4. **Frame-based sources map onto the 48-ticks-per-beat grid via a synthetic tempo** (75 BPM: 1 frame = 1 tick at 60 fps, 2 ticks at 30 fps) instead of adding a seconds-based position variant. For Dancing☆Onigiri at playback rate 1, danoniplus plays note frame `F` at `(F − blankFrame + floor(adjustment)) / 60` seconds into the music file (the fraction of `adjustment` cancels out in v51; `formats-ffr-danoni.md` B8), so a chart's `offset_seconds` is `(blankFrame − floor(adjustment)) / 60`, per chart since `blankFrame` and `adjustment` are.
5. **Deploy path: lowercase `--public-url /ditoditoinfinito/`** derived in the workflow from the repo name, with hash routing. Correction (2026-10-07): on a custom domain GitHub Pages serves the project path **case-sensitively** (`/DitoDitoInfinito/` and `/ditoditoinfinito/` are different URLs), so the repository was renamed to `sugoijan/ditoditoinfinito`; the local folder keeps its mixed-case name.
6. **Branch: `main`** (matches landing-page and heddobureika).
7. **MVP ruleset presets: `itg` and `ddr-a`.** DDR A+ exact windows are undocumented, so `ddr-a` uses the community frame table (±16.7/33/92/142 ms) and is labelled approximate in the UI.
8. **Imported songs live in IndexedDB as `Blob`s, not OPFS** (2026-10-07). The access pattern is write once, read whole files, which IndexedDB handles everywhere; OPFS is absent in Firefox private windows and Safari gained main-thread writes only in version 26. One code path instead of two.
9. **Fallback decode: Ogg Vorbis and MP3 in the app, Ogg Opus in a worker fetched when needed** (2026-10-07, Opus 2026-10-16): browsers decode MP3/WAV/FLAC natively, and each extra Symphonia codec costs wasm size. Since 2026-10-16 the fallback decodes MP3 too, as StepMania does (decision 46). Opus first had no pure-Rust decoder; now `rusty-opus` 1.0.1 (BSD-3-Clause) matches libopus within float rounding (desktop's decoder, `docs/plans/desktop.md` 7.0), and the bundled songs are all Opus, so a browser that cannot decode Opus (Safari before 18.4, WebKit builds without it) decodes it in wasm: `ddi_platform::decode::Decoder` reads Ogg Opus with the feature `opus` (the `OpusHead` pre-skip dropped, the stream ended at its last granule, the output gain applied; `crates/platform/src/opus.rs`), and desktop's `decode_all` now goes through the same code. **Size** (release, `wasm-opt -Oz`, measured 2026-10-16 on a 6.60 MB raw, 2.71 MB gzip, 2.07 MB brotli bundle): the decoder in the app would add 182 KB raw, 82 KB gzip, 65 KB brotli, and resampling to the context's rate with rubato (desktop's) 175, 82 and 54 more: 119 KB brotli (6 %) to every first load, already past the 2 MB budget, for a few browsers. So it is a **worker of its own** (`opus-worker/`, built by Trunk beside the app, `data-type="worker"`, no preload), started only when `decodeAudioData` rejects an Ogg Opus file or returns silence for it: 401 KB raw, 178 KB gzip, 151 KB brotli (plus 3 KB of glue), fetched then and compiled once a session, its memory gone when the page ends it (one worker a decode, handed the compiled module, ended at once when the decode is no longer wanted: a preview the player moved past, a play left; a worker that has not started in 25 s fails with the reason), its files named by their hash (`xtask stage-opus`) so a deploy never mixes two builds'; the app grows by 4 KB raw, 1 KB brotli. **Resampling** is the browser's: the worker answers with a WAV of 32-bit floats at 48 kHz, which `decodeAudioData` reads at the context's rate, resampled as the browser resamples its own decodes. rubato was tried first: 51–52 dB from Chrome's own decode at 44.1 kHz (the filters differ), against 136 dB for the WAV where no sample passes ±1, and it would have cost the worker 54 KB brotli. **Checked** (a forced fallback, `__DDI_FORCE_WASM_DECODE`, on the four bundled songs; the browser's own decode of the same file as the reference, lag by normalised cross-correlation on eight windows a channel): in headless Chrome, Playwright's Firefox and WebKit the lag is 0 samples everywhere (correlation 1.000000), and at 48 kHz the samples are the browser's within float rounding (137–138 dB, at most 4.2e-7) once the browser's peaks above ±1 are clamped, as `rusty-opus` clamps (80, 26, 21, 10 and 1 samples of Beatucada, Some Things Must and Into My Dream reach 1.01–1.08; desktop.md 7.0); at a 44.1 kHz context 134–136 dB where nothing clips, 77–107 dB where the clamped peaks pass through the resampler. Our stream ends at the last granule, the browsers' 99–312 samples earlier (RFC 7845's end, as desktop's). The worker's decode and the WAV's took 0.7–1.4 s a song in Chrome and WebKit, 9–26 s in Playwright's Firefox (its slow wasm; Firefox decodes Opus itself). Playwright's WebKit decodes Ogg Opus natively, so there too the fallback was tested forced. Autoplays with the forced fallback, every bundled song's Beginner chart in Chrome (its context at 44.1 kHz), Firefox and WebKit (48 kHz): all top tier, nothing let go. Previews decode there too; a preview the player moved past ends its worker.
10. **Per-song offset moves the notes, not the audio**: `Song::shift_notes` lowers `#OFFSET` (and every split timing), like StepMania's song offset edit, stored per song id in the settings. Positive = notes later.
11. **Controller bindings are keyed by `Gamepad.id` and never assume a layout** (2026-10-07). The id describes the model and stays the same across sessions (the index does not); identical pads share bindings, fine for one player. Browsers format the id differently, so each browser keeps its own. Only pads with the `standard` mapping get defaults; everything else (dance pads, adapters) is bound with the guided flow, which records whatever control moves: a button, an axis direction or a hat direction.
12. **Gamepad edges are stamped with `Gamepad.timestamp` only when it is plausible** (2026-10-07): it must have moved since the previous poll and lie between shortly before that poll and now; otherwise the poll time is used. Browsers differ in whether and how they update it, so the per-pad split is shown in the debug overlay rather than assumed. The 1 ms loop only runs during play with a pad connected and a visible tab, and can be turned off.

13. **Play options follow StepMania where it defines the maths** (2026-10-08), or DDR where StepMania has no equivalent (the eighth-note cut), cited in `rules-ddr-itg-dwi.md` §4.1. Turn and Cut are chart transforms inside `Player::new`; scroll actions and appearance only move or hide what is drawn. Cut makes a play an assist; Turn does not (as in DDR).
14. **Shuffle uses a built-in SplitMix64**, seeded per play by the app and kept in the results (2026-10-08), so a dependency update can never change a seeded shuffle and a play stays reproducible.
15. **Hidden/Sudden lines are scaled to the screen, Boost/Brake are not** (2026-10-08): StepMania's lines are screen pixels on a 7.5-arrow-tall screen, ours is 10 arrows tall, so they are scaled by 10/7.5 (approximately the same place on screen: our receptors sit 1.5 arrows from the top, 15 % of the screen against StepMania's 20 %, so the Hidden line lands at about 44 % of the screen height instead of 49 %; a portrait screen is taller than 10 arrows); Boost and Brake act on beats before the speed multiplier, so StepMania's units carry over unchanged.
16. **Background images are decoded by the browser** (`createImageBitmap`, scaled down to at most 2048 px a side, the size every WebGL2 device supports; an image still over the device's limit is skipped) and copied with `copy_external_image_to_texture` (2026-10-08): no image decoder in the wasm, and the same path works on WebGPU and WebGL2. A desktop shell would write decoded RGBA into the same texture. Brightness and the field filter are perceptual on either kind of target (see decision 17).

17. **Draw in linear light through an sRGB view on both backends** (2026-10-08). WebGL2 offers an sRGB surface; a WebGPU canvas has none but accepts an sRGB view of its format (`viewFormats`), so both blend in linear light and look the same. Colors stay written as they should look (sRGB-encoded, `crates/render/src/color.rs`) and the renderer converts them to linear for an sRGB target, including the sprite shader's own constants; solid colors look exactly as on the earlier WebGPU path, only anti-aliased edges and translucent overlays blend more smoothly. Text is converted the same way, which keeps glyphon's darker look from the old WebGPU path. A browser refusing the sRGB view keeps the plain path, and the renderer passes colors through unchanged there. Amended 2026-10-12: a noteskin's quads blend as StepMania's do, in encoded values, through a plain view of the same canvas texture on WebGPU (faint pixels and fades stay as faint as in StepMania; `docs/plans/bundled-skins.md`, step 5). WebGL2's surface has no other view, so there (since 2026-10-13) every frame is drawn offscreen: the scene in a half-float texture of linear light, each run of a skin's quads in a plain texture the frame is encoded into and decoded back from by shaders, the frame drawn to the surface at the end; both backends then show the same pixels, within one level. A WebGL2 device that cannot render and blend half floats keeps linear light for skins, and the built-in arrows (the lighter choice) skip the offscreen frame. Amended 2026-10-16: text is no longer darkened. Glyphon already turns a run's color into linear light where the target holds it (its accurate mode), and the renderer converted it once more, so the HUD's words came out about as their color's 2.2th power ("Decent", written (0.75, 0.5, 1.0), drew as (133, 54, 255)) on WebGPU, WebGL2, desktop and the field preview alike. Now text colors go to glyphon as written, in its accurate mode on targets holding linear light (sRGB views, the half-float scene) and as they are on plain ones (`render::text::color_mode`), so every judgement word, the combo, the score and the timing error draw as written (Decent as (191, 128, 255)); the judgement words look brighter than they have since 2026-10-08. The 2D pass's colors (the theme's accent `#ff6ea0`) already drew as written, and the arrows' and skins' colors are unchanged (`crates/desktop/tests/hud_colors_offscreen.rs` reads them back).

18. **Background changes play still images only, following StepMania** (2026-10-08): the first layer of `#BGCHANGES` becomes a schedule on the song's timing (a change shows when the song reaches its beat, before a delay there), the song background shows before the first change and, unless the file ends with the `-nosongbg-` marker, again from the last beat, `CrossFade` transitions fade (1, 0.75 or 0.5 s; StepMania's wipes and slides become 1 s fades, unknown names cut), and videos, scripted animations and missing files show the song background. The images a song refers to are listed in its manifest entry (`bg_images`, earliest first) and stored with an imported song under `bg/<path>` (at most 32, scaled to 1280 px); they are decoded during the start prompt and copied to the GPU before the music starts, or during play only when no change is due within a second. A scan of 735 local DDR/DWI charts found 57 with `#BGCHANGES`, all of them videos, so the feature was first tested with a synthetic pack, then with a real one: D2R from Suko's ZiV DDR Legacy Accurate collection cuts between two stills (`MAX-EXTREME/robot1.png`, `robot2.png`) at beats 243.75 and 244.75, verified locally with the images placed in the song folder. StepMania finds such names in its shared folders, so imports now recognise them too (decision 19). The second layer, `#BGCHANGES2`, followed on 2026-10-16 (decision 48).

19. **Shared background folders are part of the library, resolved at play time** (2026-10-08). Any imported folder named `RandomMovies` or `SongMovies`, or one of those followed by a separator (ignoring case; a picked `RandomMovies (low rez).zip` imports as such a folder; the innermost one counts when a zip wraps its own folder; never inside a song folder), is a shared folder: its images are stored once for the whole library under `shared/<RandomMovies|SongMovies>/<path>`, in batches; videos are skipped. The import records which background-change images a song's folder lacks (`bg_shared`, as StepMania lets a song-folder file win); when the song is played those are looked up in StepMania's order (`SongMovies/<pack>/`, `SongMovies/`, `RandomMovies/`; `BackgroundUtil::GetGlobalRandomMoviePaths`), so songs and shared folders can be imported in either order. The import report lists songs that were not imported (including folders with music but no simfile) and warnings for songs imported with something missing (`#BANNER`/`#BACKGROUND` naming a missing file with no other image to stand in, since StepMania quietly uses a stand-in and packs often rename files without updating the simfile; background-change images found nowhere (videos are not reported: they are never shown), more than 32 change images); none of these stops the import. A "details for pack authors" box in the import panel (saved in the settings) also lists inconsistencies that make no difference in play: renamed files a stand-in replaced, and background videos or animations, which are not played.

20. **Layouts are data resolved through the song** (2026-10-08). A chart names its layout by id; the song's own layouts (a file may define them) come first, then the built-in ones. Turns are per layout: Left and Right only where the layout has StepMania's table, Mirror and Shuffle within each shuffle group as danoniplus does (`applyMirror`/`applyRandom`, `js/lib/dataLoader.js`), which for StepMania's one-group layouts is the full reversal and the full shuffle; a seed gives the same shuffle as before groups existed.

21. **Bindings are kept per layout and per device; two-pad layouts take two controllers** (2026-10-08). The keyboard has a table per layout (none saved: the layout's default keys). A controller keeps its `dance-single` table (the saved shape stays as it was) plus one per other layout; standard-mapping controllers get defaults for the built-in layouts (solo: shoulder buttons for the diagonals; double: d-pad left pad, face buttons right pad). On `dance-double`, controllers without a double table of their own each play one pad with their single bindings, by `Gamepad.index` (the lower one on the left), as StepMania gives each side its own controller; edges carry the pad's index (`RawInput::slot`) so two identical pads, which share `Gamepad.id`, stay apart. A lone standard controller plays the whole double field on its defaults; further controllers beyond the layout's pads are unused; the sides follow `Gamepad.index`, so an idle controller with a lower index takes the left pad (pads are revealed by a press, and a pad first pressed during play re-resolves the sides). The options page labels each controller from the same plan (`ControlBindings::plan`) the play uses. A saved but empty table means "cleared" and hides the defaults; "use defaults" drops a layout's table again. Start and Back are shared by every layout: the guided flow keeps them when skipped and refuses a control that plays a lane in another layout's table, and a control bound to a lane later clears them. Older settings load unchanged: `keys_single` moves into the per-layout table on load and is still written as a mirror of the single table, so a tab of an older version left open across an update keeps the player's keys.

22. **A style selector picks the layout on the song list** (2026-10-08), remembered in the settings, offering only layouts the library has charts for; songs without a chart in that style are hidden. The options page edits bindings for one style at a time.

23. **One consistent game first, an archive second** (2026-10-08). Default ruleset mode: the chosen StepMania preset for every chart, Dancing☆Onigiri charts included; "each chart's original rules" and "Dancing☆Onigiri rules for every chart" are options (`presets::for_chart`; the importer keeps a work's gauge, hold and Excessive headers so switching needs no re-import). Speed: one setting meaning the same everywhere (on frame-based charts the x-mod acts as a constant speed at the work's declared BPM, else at 150 BPM), with the chart's own DanOni speed as an option (`speed_for_chart`: danoniplus moves notes 2 px a frame per unit of speed on 50 px arrows, 2.4 arrow heights a second, over a field about as many arrows tall as ours, so the conversion keeps both the spacing and the reading time). Note colors: the player's scheme everywhere, a work's own colors as an option. Lyrics show by default (they are the work's text, not a gameplay change) and can be turned off.

24. **Dancing☆Onigiri behaviour follows danoniplus v51.2.2 (`80c3c47`)** (2026-10-08) where it defines it; the points the earlier notes got wrong or left open are in `formats-ffr-danoni.md` B8 (judging buckets are `[−k, k+1)` frames, cumulative hold release budget, force-removal of a late note, Light/Easy recovery doubling, color events keyed by arrival frame).

25. **Dancing☆Onigiri key modes are built-in layout data generated from danoniplus** (2026-10-08): pattern 0 of every mode in `g_keyObj` (`crates/chart/src/layout/danoni_table.rs`, ids `danoni-<mode>`, named "<mode> keys"), with key names converted to `KeyboardEvent.code` as danoniplus does; other key patterns are left to the bindings. Lanes are one lane width apart (danoniplus spaces them 45–57.5 px for 50 px arrows). A lane in the lower row (`pos > div − 1`) has its receptor at the bottom and scrolls down (Reverse swaps both rows), and the field is centered where danoniplus centres it, so the upper row of 11 keys sits right of centre and uneven fields (9i, 14i) keep their offset. On touch screens each half of a two-row field plays its row, and a finger keeps the row it went down on while it slides. 12i keeps danoniplus's F1–F12 and also gets its second pattern's letter row, since laptops send F-keys as media keys. Lanes are labelled by number and direction ("6 · Down-left"); giko, iyo and the other ASCII-art lanes draw as plain circles until there is art for them. `.sm` files may use a `danoni-<mode>` steps type, which is how charts on these layouts are tested before the Dancing☆Onigiri importer exists.

26. **Dancing☆Onigiri works are read the way danoniplus reads them, without running them** (2026-10-08). The importer (`crates/chart/src/formats/danoni/`) follows `dosConvert`, `headerConvert`, `keysConvert` and `scoreConvert` (v51.2.2): fields split on `|` and `&`, later fields win, note arrays are `parseFloat` lists, effect values are evaluated as plain arithmetic (danoniplus evaluates them as JavaScript; anything beyond arithmetic falls back to the field's default), frames land on ticks at 75 BPM with the offset `(blankFrame − floor(adjustment)) / 60`. External dos files are scripts in danoniplus; only a literal `g_externalDos = …` assignment is read, and a work that builds its dos with code is reported, never run. An imported song stores its music number and a reference to the work's merged dos fields, which are stored once per work under a key named by their content (`works/<hash>`, dropped when no song refers to them; songs imported before keep the fields inline), so loading re-imports it and a better importer improves stored songs. A work's own key modes get ids `danoni-<mode>~<hash of the definition>`, so identical definitions share bindings across works. `playbackRate` is not applied: charts play at the music's own speed, the timing their authors wrote against. `startFrame` starts the play part-way, with the music joining `blankFrame` frames later and fading in as danoniplus does; `endFrame` stops the music (notes after it are left out, as danoniplus never reaches them) and `fadeFrame` fades it; each per chart with danoniplus's fallbacks, and the fades scheduled on the audio clock so they follow the music exactly. Frame values are truncated like `parseInt`. A tap on the frame of a hold start in the same lane is dropped (danoniplus judges neither). The dump format of the research tool (`docs/research/danoni-corpus.md`) imports too, which is how the local corpus is played; third-party works stay out of the repository. Imported files are untrusted, so the importer bounds what they can cost: shorthand lists expand to at most 1024 entries (an adversarial review found `x@:99999999999` exhausting memory), key modes to 255 lanes, field references to 8 MB, expressions to 64 levels of nesting, works to 512 charts, color changes to 100,000 per chart with 1024 targets an entry, lyrics to 1 KB each and 4 MB a work, rule headers to 64 KB a work, and frames beyond 2⁴⁰ are dropped; a second review's stress inputs (lyrics re-copied per cue, a quadratic comma merge, color lookups scanning every change) run in milliseconds and megabytes now. Header-defined key mode ids hash an explicit, versioned description, so a later importer keeps them stable.

27. **Four lanes first, with near neighbours** (2026-10-09). The game is a 4K VSRG with room for adjacent formats: StepMania Single, Solo and Double (and pump's single, half-double and double since 2026-10-13, decision 40), and Dancing☆Onigiri 5, 7, 9A and 9B keys (`PLAYED_DANONI_MODES`; 7i until 2026-10-13, when skins came to these layouts: its giko and iyo lanes have nothing in a skin to stand for them). The rest of danoniplus's key modes stay as data (custom keys refer to them) but are not offered, and works in them, or in key modes a work defines or redefines itself (Twilight's 15-lane `7wi`, Defeat,I Doll's 43-lane `Tr`), are left out with a note in the import report. Non-arrow lanes draw as symbols in the arrows' style (a thick stroke with a darker rim; receptors as thin outlines): a star for the onigiri, a square for giko, a circle for iyo; a triangle would read as an up arrow. The color schemes' gradients apply to them radially: the flowing schemes as rings moving outward with the arrows' phase, the tail-to-tip ramp as centre to rim. Since 2026-10-13 the chosen noteskin draws these layouts too, from its own folders and the ones it falls back on only (`docs/plans/danoni-skins.md`): an arrow lane takes the dance button its angle names (or the skin's own `DownLeft`/`DownRight`, pump's, decision 40), else the Down arrow turned; the onigiri the skin's `Onigiri` elements (our name; Dito has a star), else StepMania's `Center` ones (pump's and techno's middle panel), else its Down note unturned.

28. **Dancing☆Onigiri rules as danoniplus plays them, inside the engine's rule model** (2026-10-09; `crates/engine/src/rules/danoni.rs`). Windows are frame buckets, `k` frames early and `k + 1` late, for the four judge ranges; Shobon is the fourth tier and the fifth is off. Holds keep a cumulative release budget of `frzAttempt` frames (`HoldRules::ReleaseBudget`), start only within the `sfsf` window (`HoldStart::accept`) and are judged as taps only with `frzStartjdgUse` (otherwise they count by their outcome alone, and the chart's steps exclude them). A late note gives way to the next one in its lane (`Supersede`): a tap to the next tap once that is within Shakin and the tap more than Ii late, a hold's start to the next tap or hold by `sfsf` and Kita; a hold never takes over from an earlier tap. Matari neither adds to nor breaks the combo (`ComboRules::neutral`); hold outcomes keep their own combo inside the score. Gauges are resolved per chart the way `resolveGaugeValues` does: the survival or border list by the difData border, `customGauge` (inline or `survival`/`border`), difData on the first overridable family with Light and Easy taking twice its recovery, then `gauge<Name>` headers (per chart via `$` lists and numbered headers), with the variable gauges scaled by the note count; border gauges are checked at the end, survival gauges stop at zero. The player chooses a gauge by name (a chart that does not offer it plays its first), the judge range, and Excessive (the chart's setting by default). A border is checked at the end in life units, as danoniplus compares; a chart of holds alone whose starts are not judged earns the full-combo lamp when all are held. Score `(8·Ii + 4·Shakin + 2·Matari + 8·Kita + 2·maxCombo + 2·freezeMaxCombo) / (10·fullArrows) × 10⁶` with ranks SS…D, AP, PF and F. Fast and slow count only beyond a dead zone, one frame under these rules (`justFrames`) and 1 ms elsewhere, so on-time presses (autoplay) count as neither. Rank X (no rank) is not shown; rolls and mines from other formats keep StepMania's windows, and a mine costs a miss's damage.

29. **Lyrics are an HTML overlay** (2026-10-09; drawn by the 2D pass over the play since 2026-10-16, below). The HUD font has Latin glyphs only and lyrics are often Japanese, so `word_data` is drawn by the browser over the canvas (`crates/web/src/lyrics.rs`), from an engine timeline (`crates/engine/src/lyrics.rs`) that follows `updateWord`: even depths at the top of the field, odd at the bottom, swapped under Reverse on one-row modes (`wordAutoReverse` overrides; lyrics with line breaks stay), `[fadein]`/`[fadeout]`, alignment and `[fontSize=n]` relative to danoniplus's 500 px field; markup is reduced to text with line breaks. The DOM changes only when a line does. Since 2026-10-16 the lines are drawn by the UI core's 2D pass over the play (`crates/render/src/lyrics.rs`; the browser's text on the web, cosmic-text with the system's fonts on desktop), placed as danoniplus places its `lblword` labels on its stage (a box 100 px in from either side, 10 px from the top or 60 px up from the bottom, the text wrapped and aligned in it, `#cccccc` with a soft shadow), the stage mapped onto the screen as the work's layers are; the 2D pass draws through the play's sRGB surface on WebGL2 (its colors decoded first).

30. **The song list is an accordion of packs that keeps its place** (2026-10-09). A controller button still held from the page before (the Start that picked a song, the Back that left one) is not a press on the next page: a pad's first reading reports held buttons only when its timestamp says they went down after the page opened, which keeps the press that reveals a pad to the browser (`PadTracker::held_before`). The bundled songs come first as a pack like the others; one pack is open at a time, each headed by its banner (StepMania's group banner: the first image at the pack folder's root, `png` before `jpg`, `jpeg`, `gif`, `bmp`, else an image named like the folder beside it, `SongManager::AddGroup`), or its name where it has none; a search (2026-10-14) opens every pack it finds songs in, and closing it returns to the pack that was open. The open pack and the chart last started are kept per tab (`sessionStorage`), so coming back from a play shows the same pack with that chart focused. The list moves with the arrow keys and Escape, a standard controller's d-pad or left stick with A/Start and B/Back, or a dance pad's arrows (its `dance-single` table) with its Start and Back (`ControlBindings::menu_input`); up and down move between rows keeping the column, back goes from a song to its pack and closes an open pack, and a held direction repeats.

31. **Shift-JIS where text reads as Japanese** (2026-10-09). A simfile that is not UTF-8 is decoded as Shift-JIS (the browser's `TextDecoder`, strict) when the result reads as Japanese, else as Windows-1252. Windows-1252's accents and typographic quotes are Shift-JIS lead bytes and half-width katakana, so Western text can decode cleanly into lone kanji inside Latin words ("Don’t" → "Don稚"); such a character rules Shift-JIS out, and Japanese is recognised by full-width kana or forms, runs of kanji or half-width katakana, or a kanji standing alone, with NEC's and IBM's extensions accepted (`pack::reads_as_japanese`). A zip's legacy names are decided for the whole archive (UTF-8 if all are valid, Shift-JIS if all decode and read as Japanese, else CP437), so a folder never splits between two readings. StepMania only tries UTF-8 and the system code page, so on a Western system it shows such titles as mojibake; Japanese packs are common enough to do better. Songs imported before keep their stored text until imported again.

32. **Keeping the library** (2026-10-09). Browsers may clear a site's storage under pressure unless it is marked persistent; that covers IndexedDB and OPFS alike, so moving the library to OPFS would not help. The game asks (`navigator.storage.persist()`) from the click or drop that starts an import, since Firefox shows a prompt, and the song list says whether the browser keeps the songs, with a "keep them" button when it does not. Where `showDirectoryPicker` exists (Chromium), a picked folder's handle is kept in IndexedDB under a key of its own (the same folder picked again, by `isSameEntry`, keeps its key; two folders may share a name), every song imported from it records that key, and an open pack from it offers "re-read folder": the folder is imported again after the browser's permission prompt, changed songs update, new ones are added, and songs from that folder whose folders are gone are removed (not those inside a zip that failed to read). Removing songs is disabled while an import runs, since it clears shared records the import may be about to use. Firefox and Safari keep the folder input; there a pack is updated by picking its folder again.

33. **Folder imports are linked where the browser can keep the folder** (2026-10-09). Copying a whole library into the browser costs its full size (2.4 GB for a local test library, almost all music; OPFS would cost the same). Where `showDirectoryPicker` exists, a chosen folder's songs keep their chart text and banner in the browser (kilobytes, so the list needs no disk access) and record the rest (music, background, background-change images) as paths inside the kept folder, a zip's entry by name (`songs::Link`); a song reads them through the folder handle when it plays or previews, a zip's directory read again. A linked song is about 5 % of a copied one. The browser asks again for a kept folder after a restart (Chrome offers "allow on every visit"): the chart and preview clicks ask in advance, and the play page shows an "allow reading the folder" button when the permission is still missing. A folder moved or deleted leaves its songs unplayable, with a message pointing to re-reading or importing again. Re-reading keeps how a pack was imported (linked or copied); files whose names hold `\` are copied (Chromium lists them but will not open them by name); a zip's directory is read once per play for all its linked files; images of shared background folders (`RandomMovies`) are still copied, since they serve many songs and other imports. "Store a chosen folder's songs in the browser" copies as before; zips and files picked or dropped are always copied, and Firefox and Safari copy everything. Copies stay in IndexedDB: OPFS would give copies and links the same shape (a folder handle and a path) but needs a worker to write in Safari and a migration, while the same uniformity is kept in code (`songs::song_file` reads a copy or a link).

34. **Backgrounds are fitted into the screen their art was made for** (2026-10-10). Pack stills and movies were drawn for a cabinet's 4:3 or 16:9 screen, and on a cabinet a song never jumps between them; StepMania cover-fits each to the player's screen (`CoverPreserve`), so a 4:3 still and a 16:9 movie, or a movie with its own bars, zoom at every change. Backgrounds and movies are cover-fitted into a centered frame instead, and the rest of the screen shows a blurred, dimmed extension of what the frame shows. `Background frame`: automatic (default; the shape of the song's first movie less its own black bars, found by sampling it during the start prompt, else the background image's, snapped to 4:3 or 16:9), 4:3, 16:9, or the whole screen (StepMania's look). The frame never changes once a movie shows. Movies have their own setting, `Background videos` (automatic, on, off): automatic turns them off for the rest of a play when the display falls behind because of them (three seconds under 80 % of the refresh rate pause them for a trial; off only if the display then keeps up, since 2026-10-16) or the decoder does (late frames in two seconds running), off never fetches the decoder (`docs/plans/video-backgrounds.md`, steps 5 and 8). When a song's first background change is a movie at or before its first note, that movie's frame where the change starts, kept as a still at import, is the song's picture in song select, behind the start prompt and in the field before the change (the maintainer's decision, 2026-10-16; step 8).

35. **Background videos play every format StepMania plays: H.264 AVIs through the browser, the rest through FFmpeg compiled to wasm** (2026-10-10). Pack movies are AVIs with H.264, Xvid/DivX or MPEG-1/2, now and then MP4, MPEG program streams or Matroska; browsers play none of them from an AVI. The software decoder is StepMania 5.1's FFmpeg build cut down (every built-in video decoder, demuxer and parser, no audio, single-threaded), compiled with wasi-sdk to a 5.9 MB module (2.4 MB gzipped) by `video/build.sh` from the release pinned in `video/FFMPEG.toml`; it is LGPL-2.1-or-later, so it is never committed, is served as a separate file with its licence notices and FFmpeg's source tarball next to it, and credited on the Credits screen, and the app (MIT) only talks to it through a worker. It is fetched when a song needs it and hands out YUV planes the shader converts with each movie's matrix and range. Movies are timed as StepMania times them (each frame at its packet's decode time, `#BGCHANGES` rates, loops and rewinds, the shared movie texture that resumes a file where it stopped), on the audio clock, three frames ahead. H.264 AVIs, nearly all pack movies, go through WebCodecs instead: the AVI is demuxed in Rust and the decoder's output order replayed from the slice headers (FFmpeg's `h264_select_output_frame`), so every frame has the module's time and seeks land on the same frame in every browser; a song whose movies are all H.264 never downloads the module. Where WebCodecs gives RGB frames (Firefox on macOS) they go to the GPU as they are (an `ImageBitmap` copied into a texture) in the browser's own color conversion: Firefox converts SD movies with BT.709 where StepMania uses BT.601, a mean difference of 0.5–10 of 255 that is not visible side by side, so it is left uncorrected. Any WebCodecs failure, or a decoder silent for 4 s, moves that movie to the module at the same position. Movies that cannot play (AV1, a missing file, a build without the module) show the song background, and the results say why in one line. Imports sniff each movie's format and store or link it with the song (`import_videos`).

36. **Imported noteskins are drawn by StepMania's rules, their Lua not run yet** (2026-10-10; their Lua runs since 2026-10-12, decision 39, and their 3D models are drawn, `docs/plans/models.md`). Players' noteskins are StepMania 5 skins (ITGmania and OutFox read the same format); almost none carries an explicit licence on its art and many copy Konami's, so they are imported like songs, not bundled. A skin is a chain of folders ending in StepMania's `default` and `common`, whose art is not freely licensed: stand-ins list their files by name, so lookups and redirects land where StepMania's do, and what lands on them is drawn by the built-in skin. Everything that is not a skin's own Lua is StepMania's behaviour: lookup, sheets and hints, `[NoteDisplay]` metrics, frames on the beat and vivid, color by texture translation, holds, receptor and explosion commands from the metrics on the engine's events (a row flashes when a press completes it). Nearly every real skin defines its taps and receptor in Lua, so until a Lua runtime (a hardened fork of piccolo, `docs/plans/lua.md`) and a MilkShape model loader follow, an imported skin mostly contributes its holds, mines and explosions; the import report says which elements are drawn built in and why. The procedural arrows stayed the look of every non-dance layout until 2026-10-13, when skins came to Dancing☆Onigiri's layouts (decision 27); since 2026-10-12 the default for the layouts skins draw is Dito, the same arrows as a bundled skin whose Lua runs in every play (decision 37), with the color schemes as its variants; the procedural arrows remain what a stopped or missing element falls back on, and a choice ("built in, without a skin"; since 2026-10-21 Dito's `Lighter` parameter, decision 56). Since 2026-10-11 a skin's Lua runs, at load and in play (`docs/plans/lua.md`, step 4); only 3D models were still drawn built in, and since 2026-10-12 they are drawn too (`docs/plans/models.md`). Since 2026-10-12 the few skins whose art carries a free licence are bundled (decision 37).
37. **Freely licensed noteskins are bundled** (2026-10-12; `docs/plans/bundled-skins.md`). Skins whose upstream files declare a licence on their art (CC BY, CC BY-SA, Apache-2.0, the Unlicense, CC0) ship under `assets/noteskins/<game>/<name>/`, byte-identical with a `PROVENANCE.toml` like songs, credited on the Credits screen; Lua we may not take (derived from unlicensed skins) is replaced by our own. They are served with the game and read without storage (an index of their files and texts, generated before each build), listed under Options before imported skins, and cannot be removed; a stop of their Lua counts for the session only. An imported skin's fallbacks are imported skins first, so an import named like a bundled skin stands in for it there (as StepMania's own folder would); a bundled skin's are bundled ones only, so no import changes it; choosing a bundled skin always plays it. First: StepMania 5.1's Delta (CC BY 3.0), then Alpha-Arrows (Apache-2.0 images, our own Lua and metrics) and Dake (Apache-2.0, its dance files; our Lua host stands in for the few OutFox and theme names it reads), and once models drew, Peter Lawrence's CF_CHROME, CF_VIBRANT and CF_ALLOY (the Unlicense; CF_CHROME's mine explosion CC0), then (2026-10-13) his Peters-DDR-Vivid with Peters-DDR-Note, which it falls back on (the Unlicense). The default for the layouts skins draw (dance, and since 2026-10-13 Dancing☆Onigiri's) is Dito, the built-in arrows as a bundled skin of our own (art generated from the shader's shapes, a variant per color scheme, explosions of its own), so every play runs a skin's Lua.
38. **Skins may use SVG** (2026-10-12; `docs/plans/bundled-skins.md`, step 2). Our extension: an `.svg` is an image wherever a bitmap is, read only in a static subset that browsers and `resvg` draw alike (shapes, paths, gradients, masks, `use`; no text, images, filters, styles, scripts or outside references), sized by its root's `width` and `height`, and drawn by the browser (an image element, which runs no script and loads nothing) at four times that size so vector art stays sharp on large lanes. Dito's art is SVG; StepMania itself would not read it.
39. **A skin's scripts are untrusted and run in a sandbox of our own** (2026-10-12; `docs/plans/lua.md`, `docs/plans/piccolo-fork.md`). A player never has to trust a noteskin: its Lua runs in a hardened fork of piccolo (`lunita` on crates.io since 2026-10-13; pure Rust, stackless, the same natively and in wasm), one VM per skin sharing nothing. Every call has a fuel budget and a wall-clock deadline (a play's frame 200,000 fuel and 50 ms for all of a skin's Lua; a load 20 million fuel and 5 s of Lua time, in slices that let the page draw), fuel is charged before the work it pays for, memory is capped (32 MiB), and so are source size, strings, actors (16,384) and textures; there is no bytecode, `debug`, `os`, `io` or loader but the skin's own files, and the host API reads game state only (the player's key bindings, for one, are not shown). A stop is never a crash: the skin is drawn without its Lua (natively at load, built in for the rest of a play), the next play tries again, and a second stop turns its Lua off until the skin is imported again (bundled skins: for the session). Fuel, not time, decides in normal runs, so a skin behaves the same on every machine; the deadline is the backstop. Song backgrounds' scripts, if they come, get VMs of their own under the same rules.

40. **Pump layouts are played, drawn with the chosen skin** (2026-10-13; `docs/plans/pump.md`). StepMania's second pad game, `pump` (five panels: down-left, up-left, centre, up-right, down-right), has charts in ordinary simfiles, three bundled songs among them. Its single, half-double and double styles are layouts like dance's, named "Pump single", "Pump half-double" and "Pump double" (StepMania's own name for the game), in StepMania's track order with its turns, and judged as dance except for holds: since 2026-10-14 a rule option, Holds, ticks them on pump charts by default as StepMania 5.1's pump game does (checkpoints at the chart's `#TICKCOUNTS`, 4 a beat without one, each row one judgement in score and life and the holds kept added to the combo; a hold never dropped before its end, its head always a top judgement), or dance-style or ticked for every chart. The chosen dance skin draws them from its own folders and its fallbacks only: a skin's own `DownLeft`, `DownRight` and `Center` elements when it has them, else its Down arrow turned for the diagonals and as it is for the centre; its Lua is told the dance game, as StepMania never loads a dance skin for pump. Dito has a centre, a square as the built-in arrows draw it. Pump skins (`NoteSkins/pump/*`) are left for later.

41. **Dancing☆Onigiri's back and mask layers are drawn in the scene, as danoniplus places and animates them** (2026-10-14; `docs/plans/overlays.md`). Half the captured works use them, where StepMania's second background layer and foreground have no use at hand, so they come first. They lie under and over the arrows, which the canvas draws, so they are textured quads in the wgpu scene rather than HTML: danoniplus's stage (its frame, widened for the key modes, `windowWidth`/`windowHeight`) mapped onto the screen as the lyrics' field is (decision 29), images from the work's files and text drawn by the browser, placed as danoniplus's `<img>` and `<span>` are, with its own 16 layer animations reproduced (CSS `ease`, transforms about the centre), clipped to the stage; masks lie over the field and under our HUD, where danoniplus's cover its judgements too. A work's own animations, defined in a stylesheet neither a capture nor a page import reads, play as none, and color boxes, layer transforms and script-computed positions are left out; the import report says so. StepMania's `#FGCHANGES` entries are actors shown until their tweens end, so a still or movie never shows: the foreground joins scripted backgrounds.
42. **A row of mines across every lane is a DDR shock arrow under the DDR A rules, and mines under the others** (2026-10-14). StepMania-format DDR packs write shock arrows as full mine rows (1,113 such rows in one local pack); DDR has "only full lines of four or eight Shock Arrows" ([RemyWiki DDR glossary](https://remywiki.com/DDR_Glossary)). Under DDR A such a row is one shock: avoided it is an O.K. that adds one to the combo ("jumps and avoided shock arrows both increment the combo by one", [ddrguide](https://ddrguide.com/glossary/)), stepped on in any lane, or a panel held as it passes ("triggered by a player standing on any of the arrows", [Rhythmic Keystrokes](https://rhythmickeystrokes.wordpress.com/2018/10/08/shock-arrows-the-floor-is-lava/)), it is an N.G. that breaks the combo and costs life as a miss does, and "subsequent arrows disappear for a short time" (RemyWiki), drawn as 0.2 s without the notes still to come (no measured figure is published). It scores as a freeze arrow: one unit of the million (`SC = 1,000,000 / (steps + freezes + shock arrows)`, [RemyWiki SN2 scoring](https://remywiki.com/DanceDanceRevolution_SuperNOVA2_Scoring_System)), 3 EX for an O.K. ([RemyWiki EX Score](https://remywiki.com/EX_Score)), counted with the freezes' O.K.s and N.G.s on the results as DDR counts them. Its window stays the mines' 0.07 s; DDR's is not published. ITG and SM5 keep the row as mines, as StepMania plays it.
43. **`#COMBOS` applies as StepMania 5.1 applies it** (2026-10-14). A combo segment's values multiply what a row adds to the combo and how many times its judgement is counted (`ScoreKeeperNormal::HandleTapNoteScoreInternal`, `HandleComboInternal`, `HandleRowComboInternal`): a row or checkpoint row that continues the combo counts the segment's combo, one that breaks it its miss combo; dance points, the holds' own outcomes, mines and shocks are unchanged, and there is no miss-combo counter to multiply. Before the first segment the first applies (`GetSegmentIndexAtRow`, as for tick counts since). As StepMania 5.1, from `.ssc` files only (its `.sm` loader has no `#COMBOS`), under every StepMania preset and not under Dancing☆Onigiri's rules; one local chart uses them (×4 and ×3 for seven beats).
44. **A mine is crossed while held only if its lane was down at the mine's time** (2026-10-14). The judge counts a mine crossed while its lane is down only when the lane was down at the mine's time, from the presses' and releases' own timestamps (a second key on a lane already down keeps it down since the first; a lane let go and pressed again within a frame was down over a mine it passed before), where StepMania looks at the panel's state on the frame that crosses the mine; otherwise a mine a fraction of a beat before a note was stepped on by the press for the note, by an amount that depends on the frame rate. The autoplayer lets each press go before its lane's next press or mine and keeps roll re-taps 0.1 s from mines, as StepMania's autoplayer never steps on one; with that, `DDI_AUTOPLAY_CORPUS=<dir> cargo test --release -p ddi-engine --test local_autoplay` plays every chart of a local folder under ITG, SM5 and DDR A and expects only top judgements, 100 %, the full EX score and 1,000,000.

45. **The game draws its own screens, on web and desktop** (2026-10-15; `docs/plans/ui-core.md`, researched in `docs/research/ui-toolkits.md` and `docs/research/stepmania-themes.md`). Every player-facing screen moves from Yew over the DOM to a UI core of our own drawn on the GPU, so that the game ships a look of its own, StepMania themes can later be hosted in the same scene (StepMania 5 gameplay and evaluation decorations first), and the same screens run on desktop and later on constrained targets without a WebView. Both web and desktop are kept in view from the start. No existing toolkit fits: the general ones lack StepMania's actor model (tween queues, sprite sheets, blend modes, draw order, a virtual screen) and menu navigation by pad, and Masonry, the closest, would add a second renderer and system and is unreleased since 2025-10 with the web a low priority for its maintainers. Japanese input must work (a hidden text field on the web is acceptable); on the web the browser draws the text (system fonts, Japanese included, no font download; cosmic-text, as on desktop, stays possible behind the same interface); fonts under OFL-1.1 may be bundled; portrait layouts wait until after the desktop build and the known gaps; accessibility is deprioritized. How the rest of the core is built (P1–P13 in the plan) is proposed there and settled step by step.

46. **MP3s are heard where StepMania 5.1 has them, on desktop and in every browser** (2026-10-16; `docs/plans/desktop.md`, D9 and 7.2). StepMania 5.1's MP3 reader (libmad, `RageSoundReader_MP3.cpp`) trims nothing (no encoder delay, no decoder delay, no padding), skips a first frame holding a `Xing` tag and plays one holding an `Info` tag as a frame of silence, "so we match DWI sync"; charts were synced against that. Browsers decode the same file earlier, each its own way, measured on 2026-10-16 with `decodeAudioData`: on generated LAME files Chrome and Firefox (151 and 158, stock and Playwright's) trim LAME's delay and the decoder's (`Info` +2257 samples against StepMania, `Xing` +1105, no tag 0; an earlier Playwright Firefox trimmed only the decoder's 529, and Firefox still does for a tag without LAME's figures), Playwright's WebKit 529 always and the whole delay on most stereo files, plus 1152 more at 48 kHz (+1681 to +3409, +529 to +2257, +529); on 16 local tagged MP3s all three gave +2257 (`Info`) and +1105 (`Xing`) on 15, and 1152, 1681 and 2257 on the 16th. A table cannot cover that (nor Safari, untestable here), so the web measures it per song: the start of the MP3 is decoded in wasm as StepMania does (`ddi_platform::decode`, which now decodes MP3, +113 KB of wasm, +34 KB brotli, with room reserved for that start only), and the music decides which of the lags the first frame allows is the browser's (`ddi_platform::mp3::align`: the tag frame, a frame more or less, LAME's delay and the decoder's 529 samples each trimmed or not, every lag measured above among them; each scored at its exact offset on up to eight stretches spread over the start, taken when its correlation is at least 0.9 and 0.05 above every other's). A free search was tried first and rejected: periodic intros (a tone, a saw, a sub-bass, hi-hat loops) matched wrong lags as well as the right one, and two stretches agreed on the same wrong answer. When the music cannot tell (a pure tone, silence for 40 s, a decode that differs) the lag measured for this browser and kind of file stands in (`FirstFrame::expected`), logged. The buffer keeps its samples; its lead is kept on it and sounds made from it start that much later (or further in), so no second copy of the song is made. The song's audio start moves, nothing else (the audio clock, user offsets and calibration are untouched). It costs 17–60 ms of main-thread time a song in Chrome, about 100 ms when the music cannot tell (40 s decoded), and the wasm memory of that start's left channel only, room reserved for its first 8 s and grown when more is needed (a six-minute song's whole decode reserved 120 MB before). The built-in fallback and desktop decode MP3 that way directly, and find an MP3 where StepMania's reader finds one: past junk and ID3v2 tags anywhere within 32 KB (75 of 219 local MP3s start with 417 zero bytes), inside a WAV of format 0x55; a later tag frame (files joined end to end) and a tag frame in a file with CRCs are frames of silence, as StepMania decodes them. 203 of 219 local MP3s have no tag frame and were not moved; on the 16 others plays move 25 or 51 ms later, where StepMania has them. **Dancing☆Onigiri works are the exception**: danoniplus schedules the browser's own decode (`docs/research/formats-ffr-danoni.md`, "Frame to music time"), and works are made and synced in Chrome, so a work's music goes where Chrome's decode has it: its lead is the lag found less Chrome's expected lag for the file (`FirstFrame::target_lag`, `mp3::Target::Chromium`), so Chrome's decode stays as it is and another browser's moves by its difference from Chrome's; the built-in decoder and desktop cut Chrome's lag from the start of StepMania's decode (`decode_all::Range::target`; `ddi_session::music_target` says which a song wants). Previews the same.
47. **The desktop shell is the same game, not a port** (2026-10-16; `docs/plans/desktop.md`, D1–D11 as settled there). What a play does is shared code both shells run: the session, calibration and the backgrounds' rules (`session/`), the import's core for simfiles and Dancing☆Onigiri works (`crates/library/src/scan.rs`, `works.rs`), song select's data and the screens (`ui/`); a shell keeps only its platform. Desktop's sound is cpal with our own mixer (not web-audio-api: its clock is a hundred times steadier, and only our own backend can correct CoreAudio's latency), every music format decoded natively (Opus through `rusty-opus`); keys are stamped with the OS's time where it gives one (macOS's `NSEvent`), controllers read by gilrs on their own thread; frames use late pacing with two in flight when a macOS display link is available, blocking with one elsewhere (Mailbox, else Fifo; revised 2026-10-17); the library is the player's folders, StepMania roots and zips read in place, nothing copied, with a cache; movies through the web's FFmpeg module under wasmtime (feature `video`, on by default; since 2026-10-20 the OS decoders and a sandboxed FFmpeg helper, decision 54); data in the platform's folders (`DDI_HOME` moves them, for agents and tests); a `desktop` build profile beside wasm's `release`. Device profiles and pad bindings are each shell's own (fingerprints differ); ids, records and bests are the same for the same songs. Packaging and signing come in their own plan.
48. **The second background layer plays as StepMania 5.1 plays it** (2026-10-16). `#BGCHANGES2` (and `#BGCHANGES1`, the first layer, as `SMLoader::ProcessBGChanges` numbers them with `sscanf("BGCHANGES%d")` on the tag's name as written, so `#bgchanges2` is the first layer; a third layer is dropped, as StepMania has two, `NUM_BackgroundLayer`) becomes a second schedule (`backgrounds::second_layer`), drawn over the first and under the field on both shells, with what `Background.cpp` does for that layer and not the first: it shows nothing until its first change, which cuts in (the song background is put at beat −10000 on the first layer only, and with no background before it no transition runs); `-nosongbg-`, `-random-`, files found nowhere and BGAnimations found nowhere are never loaded there, so the layer keeps what it shows ("Tried to switch to a background that was never loaded"), cutting short a fade that is running (the faded-out one is dropped), and nothing returns at the last beat (`SMLoader::TidyUpData` looks at the first layer only); a change naming no file is not there at all; `songbackground` (compared exactly) and the song's `#BACKGROUND` show the song's picture, and `songbackground` in a song without one the plain backdrop, opaque, where StepMania shows its theme's fallback graphic. Its changes are otherwise timed, matched and played as the first layer's (decision 18: transitions, rates, movie effects, the last of a beat winning), and on both layers a change to what is fading in cuts the fade (the fading background becomes the current one and is dropped). Every layer draws its incoming background and then the one leaving over it, fading out (`DrawPrimitives`, `BackgroundTransitions/CrossFade.xml`; the first layer drew the other way round before), the first layer over black (StepMania's screen under its backgrounds) and the second over the first; images keep their own transparency (StepMania draws them as blended sprites), so black shows through a transparent first-layer image and the first layer through the second's. Blending is in linear light (decision 17), so a half-transparent image mixes as (148, 148, 30) where StepMania, blending encoded values, shows (115, 115, 30): kept, as everywhere else. What StepMania would play there but the game cannot show (movies that cannot play, BGAnimations that exist, which the import records as `bg_scripted`) shows nothing on the second layer, the first showing through, as the first layer shows its song background for them. Movies play on the second layer through the same paths (WebCodecs or the module's worker on the web, on desktop the module under wasmtime, since 2026-10-20 the OS decoders or the sandboxed helper): each layer's current and next movie are decoded (`backgrounds::movie_segments_at`), a file current on both layers once, into its one texture; the one automatic video rule covers both (it watches one drawn movie, so a change on the other layer does not start its warm-up again; off or a trial stops both); and a file on both layers shares one clock, as StepMania's layers share the movie's texture and each advances it (`RageTextureManager`, `Sprite::Update`): both layers' current segments add to its position, a restart on either starts it over for both (a decoder whose position goes back within a lap seeks, `SEEK_BACK_SECONDS`), a no-loop change on either stops it looping from then on, mid-segment too. Both layers' files are listed by the import (images stored, movies sniffed, shared folders looked up), in order of first use across both; songs imported on the web before 2026-10-16 lack the second layer's files and the BGAnimations' names, so they show the first layer only until imported again; desktop's library cache (version 4) is read again. The opening still (step 8 of `docs/plans/video-backgrounds.md`) is the first layer's alone: the second never makes the song's picture, and in play it draws over the still as over anything on the first layer. Also from `SMLoader::LoadFromBGChangesVector`: a change naming a `.ini` or `.xml` file (field 2 or 8) is dropped at load, on either layer. On the web every image is decoded with its colors unmultiplied (`premultiplyAlpha: "none"`): WebGL2 takes a bitmap as it is, so premultiplied ones darkened half-transparent edges there (backgrounds, noteskins, a work's layers, the 2D pass's pictures). StepMania's named effects (placements, tints, the additive blend, the flashes, two files) and transitions since 2026-10-21, on both layers (`docs/plans/overlays.md`). None of the 5,094 local simfiles (local packs, StepMania and OutFox installs, converted works; 449 with a `#BGCHANGES`) has a `#BGCHANGES2`, so it was checked natively against cases derived from StepMania's source, offscreen on desktop (`crates/desktop/tests/play_layers.rs`), and with synthetic packs in headless Chrome (WebGPU, WebGL2) and Firefox.
49. **A replay is the record's play judged again as it is watched, on this device's clock** (2026-10-16; `docs/plans/play-history.md`, "Replays"). The log is in judged song time, so a replay feeds each edge to a new play (`Player::input_at`) when this device's judged time reaches it, before the frame's update: no edge is ever behind what was judged, at any frame rate, so the watched results are the record's exactly, and they are checked against the same log judged at once, the timing kept and the summary (all of it but `late_edges`, which only a live play has); a difference is shown, never hidden. It plays the record's chart (by digest, in any song that has it), rules (`RulesetKey`), options as played (seed and worked-out speed included), song offset and drawing settings (the snapshot kept), with this device's volume, bindings and graphics; the offsets are the record's on the devices it was played on and this device's elsewhere (the record's corrected other hardware; judging never reads them). It never records, counts toward a best or changes a setting. What it cannot reproduce is refused with the reason: a newer record format or judge, a song or chart gone or changed, a skin no longer installed. Records now keep the judge that judged them (`judge_version`, 1 for older ones) and summaries the options (for the plays list). The ghost of the best play is the same mechanism: the best's log played beside a play, judged to the same point every frame, its score's lead shown under the score (off by default).

50. **OS video decoders come before a native FFmpeg helper** (2026-10-17; `docs/plans/native-video.md`). The sandboxed wasm module remains the baseline decoder and fallback (on desktop the sandboxed FFmpeg helper since 2026-10-20, decision 54; the web keeps the module); the verified macOS paths (indexed 8-bit H.264 AVI, checked single-slice H.264/HEVC MP4/Matroska and visible-picture VP9 WebM/MP4, copied planes) now select hardware VideoToolbox automatically, with exact-position module recovery and an explicit module override. Windows Media Foundation/D3D11 now has an experimental opt-in implementation and a deferred-host fidelity runner; Windows Auto stays on the software decoder until real hardware validation. Extend and validate OS decoders, selecting only supported streams that preserve the module's frame times and exact seeks; unsupported or failed movies return to the module. Keep packet demuxing separate from decode, as WebCodecs does: our checked AVI reader first, a bounded packet interface from the sandboxed module for other containers (ABI v1, checked Rust reader and streaming H.264/HEVC/VP9 consumers implemented). The M1 Max spike hardware-decodes H.264, HEVC and VP9; it does not establish support for every profile or OS. CPU plane copies remain the default. An opt-in `video-metal` build with `DDI_VIDEO_PLANES=metal` now shares macOS NV12 planes on wgpu's actual device, following rendered measurements and 4,800 exact frame comparisons; command-buffer leases, bounded retention and copied fallback protect GPU lifetimes. Other Macs and physical GPU loss still need validation before changing defaults. D3D11/DX12 sharing awaits Windows evidence. Linux VA-API needs a separate driver/capability spike. Later, if the remaining codecs need more speed, use the same FFmpeg shim in a sandboxed child with bounded pipe messages and shared-memory frames, never native FFmpeg in the game's process. The child must have a working, tested OS sandbox before it is offered to players; the module remains available on every platform. Done since: decision 54.

51. **DanOni URL imports start on desktop with an SSC/Opus handoff**
(2026-10-18). Retain the embeddable SDK and its pinned, confined worker inputs
under `tools/danoni-dump/`, with dedicated locked CI checks. The SDK and helper
share the root workspace/lock and use the native `importer` build profile; the
generated upstream Ruffle worker keeps its independent pinned workspace. Hand
prepared SSCs, Opus music and title artwork to the existing desktop loader and shared play
session; do not introduce a second gameplay path or change music offsets.
The managed store's verified catalog publishes entries with the SDK's stable
IDs; recursively scanning its transactions would expose cancelled or obsolete
songs. The optional `danoni-importer` feature (`danoni` alias) now builds a
separate Rust 1.99 SDK helper and confined worker automatically on macOS and
x86_64 Linux; the Rust 1.98 game links only the pure protocol, URL validator and
hashing. Add songs' URL tab uses anonymous-pipe IPC, owned cancellable jobs and
complete verified snapshots. Since 2026-10-20 it asks only for the URL, reuses
cached files automatically, and shows task counts with continuous activity
between updates (stationary under reduced motion). Explicit desktop downloads
skip site-approval prompts and robots.txt; SDK research/batch defaults still
respect robots. Public-address checks, limits and pacing remain in both modes.
The executable-relative content bundle includes
exact helper/SDK/worker lock graphs, inventory, copied-code/font notices and
source retrieval information. No runtime operator configuration or host-executable
binding is required. The package command audits native dependencies and stages
assets, notices and any video compliance payload for relocation. See
`docs/plans/danoni-importer.md` and `docs/plans/danoni-import.md`. CI retains
synthetic handoff/IPC tests and adds an actual native package/relocation check;
Windows checks the pure SDK and keeps URL import unavailable. Frozen corpus
measurements remain historical evidence, not a current live-import claim.
Windows confinement, managed-store durability and browser URL acquisition
remain separate work.

52. **Simplifications can expose additional chart versions** (2026-10-18; `docs/plans/chart-simplification.md`). The Without onigiri option exposes converted DanOni charts in Single, Solo, Double and Techno 8-panel. Shared library loading appends them for native works and simfiles (including SSC exports) with explicit provenance and stable indexes; the option controls their visibility. Originals stay unchanged and unassisted by this feature. Converted charts have their own content digests, assisted results and bests, and keep their source chart's lyrics, layers and replay identity. Existing cut options keep their behavior.

53. **Song browsing takes priority over difficulty gestures** (2026-10-20; `docs/plans/song-navigation.md`). Physical direction edges feed a shared recognizer. A first tap previews 20% over 260 ms, then completes the same curve over 160 ms; logical commit never restarts easing. Difficulty pairs require 180 ms of quiet afterward; opposite pairs remain provisional to avoid a transient song jump. Browsing is immediate until 400 ms of quiet. Holds start at 275 ms and ramp 8→24 rows/s over 1.2 s, stopping on release. Row geometry and emphasis follow the wheel continuously. Other menus retain standard repeat, and pointer scrolling retains its existing physics. These values were selected in the maintainer's local timing tool.

54. **Desktop decodes movies in a sandboxed FFmpeg process, and wasmtime leaves the game** (2026-10-20; `docs/plans/native-video.md`, section 10). Decision 50's "later" helper is now the desktop's software decoder: `ddi-video-helper` (`video/helper`) links the same pinned FFmpeg release natively and statically, with `video/ddivideo.c` and `video/packets.c` unchanged in what they decode, built by `video/build-native.sh` and staged beside the game by `crates/desktop/build.rs`. The game and the helper speak a bounded, hand-encoded pipe protocol (`video/protocol`, `ddi-video-protocol`: a sequence number per request, lengths and handles checked on both sides) and move every bulk byte through one anonymous shared-memory arena: the helper never opens a file (the game serves 64 KiB reads of the one movie each handle names, as the web worker serves blobs), frames and packets come back in slots whose dimensions the game checks before copying. The helper confines itself before it reads anything (Seatbelt through `sandbox_init` on macOS; Landlock and a seccomp-bpf allowlist on Linux; a restricted token, untrusted integrity and a job object on Windows, unverified on hardware) and then tries to open a file, list a directory, reach localhost and start a process: any success and it refuses to serve, which the game reports as a decoder that is not available, so an open sandbox never decodes a movie. A dead, stuck (10 s) or lying helper is a crash as a trap was: every open movie fails, the helper is killed and the next open starts a fresh one. FFmpeg runs with assembly and a few frame threads (half the cores, at most four; `DDI_VIDEO_THREADS`); frame times are StepMania's as before (a frame lacking a DTS in a stream whose other packets have one is the one case threads can shift, by design accepted). wasmtime and the module's compile cache are gone from the desktop build; the web keeps the wasm module untouched. The OS decoders' demux goes through a helper of its own per movie. Not done: Windows on hardware (its FFmpeg build needs MSYS2 and the MSVC tools; `crates/desktop/build.rs` skips the helper there unless `DDI_VIDEO_NATIVE=1`), Linux beyond the container checks recorded in section 10.

55. **StepMania's player options are options of the StepMania presets, kept with each play** (2026-10-21; `docs/plans/rules-options.md`). The fail type (Immediate, Delayed, Fail at end, Off), the life and judge difficulty as StepMania 5.1's levels (life 1–7, judge 1–8 and Justice, 4 the preset as it is) and the life meter (the preset's bar or a battery of 1–10 lives) apply to `itg`, `itg-fa+`, `sm5` and `ddr-a`, never to Dancing☆Onigiri's rules (its gauges and judge range are its own) or calibration. A play's `RulesetKey` keeps them, so its replay and re-judging build the same rules; bests are kept apart by judge level only (the rest changes whether a play fails, not how it is judged). ITG FA+ is ITG with Simply Love's 15 ms window shown (blue and white Fantastics, Simply Love's EX percentage), not Simply Love's FA+ mode. The event sounds (the mine sound, the assist clap) are made in code and played through the session's audio backend, the clap scheduled on the audio clock where the music plays its row.
56. **Skins declare their parameters; Dito's looks are its parameters** (2026-10-21; `docs/plans/skin-parameters.md`). Our extension, as SVG images are: a skin's `metrics.ini` lists parameters (`[Parameters] List`, a `[Parameter <name>]` each: a choice, a toggle, a number in a range or a color, with a label, a description and a default; bounded, malformed ones left out with a warning), merged under its fallbacks like any metric. Options shows the chosen skin's as rows under Arrows; the values are kept per skin key in the settings (a default not stored), go into a play's settings snapshot and its drawn record so replays draw the same, and reach the skin's Lua read-only (`NOTESKIN:GetParameter`), with the same values for Options' preview and a play (`ddi_shell::play_skin`, `field_preview_skin`). A choice may name the skin that draws it (one falling back on this one), loaded in its place, so Dito's color schemes stay the skins they were and are its `Colors` choice. The note colors and songs' own colors are Dito's parameters (older settings migrate on load), with `Explosions` (read by its Lua) and `Lighter`, which draws the built-in arrows instead: the visible Built-in choice is retired into it, the procedural arrows staying what missing elements, a stopped Lua and a song's own colors fall back on. The game reads Dito's colors, songs' own colors and lighter by name; other skins' parameters reach only their own Lua. A skin without declarations behaves as before.

57. **DDR's legacy score is a preset of its own, DDR's gauges and LIFE4's lamps go with DDR A** (2026-10-21; `docs/plans/rulesets.md`). `ddr-extreme` is DDRMAX2 and EXTREME's normal play: EXTREME's windows as measured from its `GAME.DAT` (Marvelous only in courses), MAX2's money score by the song's feet, MAX's dance points for the grade (AAA at 100%, E failed), a jump adding two to the combo, and StepMania 3.9's life bar for DDR's unpublished one (approximate). SuperNOVA2/X and an `ffr` preset are not built. DDR A takes DDR's other gauges as a player option (`StepManiaOptions::gauge`: LIFE4, RISKY, FLARE I–EX with BEMANIWiki's WORLD drains, FLOATING FLARE), replaced by StepMania's battery when that is chosen; a DDR A play keeps its gauge, the FLARE rank its gauge kept and its FLARE SKILL points (`Results::ddr`), and earns LIFE4's clear types as lamps (LIFE4 clear, single digit perfects). Bests are kept with every gauge's plays. LIFE4's standings and the FLARE SKILL total are across plays and DDR's own songs, not a play's. `JUDGE_VERSION` 2 judges kept plays again for them.

## 7. Risks

- Safari: codec gaps (Ogg only from 18.4 and shaky), `getOutputTimestamp` regressions, context interruptions, 44.1 kHz contexts. Mitigation: sanity checks, the wasm fallback (Symphonia for Ogg Vorbis, `rusty-opus` in a worker for Ogg Opus, decision 9).
- MP3 decoder delay mismatch vs StepMania-synced packs (25–51 ms). Settled by decision 46: MP3s are lined up with StepMania 5.1's decode on every browser; a song whose first 40 s are silent keeps the browser's decode.
- Gamepad polling competes with rendering on the main thread. Mitigation: one JS call per poll, a loop that sleeps between polls and runs only during play with a pad connected, an option to poll per frame, FPS and polls per second in the debug overlay. Touch targets are small on phones (the field's arrow size is capped in device pixels, so middle lanes are about 50 CSS px wide in portrait).
- Safari does not treat gamepad presses as a user gesture (WebKit bug 217675), so a song started with a controller's Start button cannot start audio there; the prompt then asks for a tap or key.
- Firefox timestamp precision and `resistFingerprinting`. Mitigation: detect quantised timestamps and warn.
- Ecosystem churn (wgpu majors every few months, winit 0.31 beta, Trunk 0.22 rc). Mitigation: pin, upgrade deliberately, keep wgpu behind `render`.
- DDR's exact modern windows and gauge numbers are not public; presets are approximations and should say so in the UI.
- Safari: `decodeAudioData` accepting Ogg Vorbis and returning silence; covered by the silence check, but a decode that returns noise instead would not be caught. Previews decode the whole song (~0.5 s before sound).
- Imported packs: text in Korean and Chinese legacy encodings is read by a heuristic (`docs/plans/format-gaps.md`; a short title may be misread, the charts parse either way); wasm memory grows by a song's raw PCM size after a fallback decode and never shrinks; browsers may evict the library unless `persist()` is granted (Safari's 7-day rule for origins without interaction).

- Background videos: the module is 2.4 MB gzipped, downloaded on the first song with a movie that is not an H.264 AVI (a slow connection shows its progress on the start prompt, and the movie starts late rather than the song). Movies in a codec FFmpeg only decodes through hardware or external libraries (AV1) cannot play. WebCodecs differs per browser: Firefox and Safari give frames out in timestamp order and Chrome in bitstream order (frames are identified by their chunk, so both work), Firefox on macOS gives RGB frames in its own colors, and a decoder that stalls or errors moves the movie to the module. Firefox and Safari are checked by hand (`#/video-check?decoder=both`).

- Noteskins: a skin's Lua state carries over to a retry (its actors start over). Skin images are kept in IndexedDB like copied songs (a skin of thousands of large images costs that space); the library's database went to version 2 for them.

- A browser that refuses an sRGB view of the canvas falls back to drawing without one (colors stay right; edges blend as before). The headless runs read `srgb_view` from the debug state; Firefox and Safari are only checked by hand.

## 8. Bundled songs: licensing and attribution

The demo songs are committed to the public repo and served with the MVP. The repo stays MIT for code, every asset carries exact SPDX metadata, and the in-game credits are generated from that metadata so they can never drift from the files. Conventions follow `../detonito`.

**Selection rule.** Only songs where music, charts *and* graphics are each covered by an explicit per-song licence file upstream, and all three are CC-BY (any version). CC-BY-SA is acceptable as a second tier (a bundle is a collection, so the MIT code is unaffected), but the first three should be pure CC-BY. Nothing NC, ND, undeclared, or granted only on a web page rather than in the distributed files (this excludes Sockpuppet for now; its CC BY-SA grant exists only on itch.io).

Candidates meeting the rule, from `docs/research/cc-songs.md` (all OutFox Serenity, `.ssc`, Opus audio):

| Song | Music | Charts | Graphics | dance-single |
|---|---|---|---|---|
| Some Things Must (OutFox Edit) — Sevish | CC-BY 4.0 | CC-BY (per `#CREDIT`) | CC-BY 3.0 | 2/9/10/13/16 |
| Into My Dream — Lagoona (Andreas Viklund) | CC-BY 4.0 | CC-BY | CC-BY 4.0 | 1/2/3/8/12/15 |
| Sweeteners — Jack5 | CC-BY 3.0 | CC-BY | CC-BY 4.0 | 3/10/13 |
| Beatucada — Kurio Prokos (second tier) | CC-BY-SA 4.0 | CC-BY | CC-BY 4.0 | 1/3/7/11/11 |

Re-verify each song's upstream `credits.txt`/`license.txt` at the pinned commit before committing it; the research scan is a snapshot.

**Repository layout.**

```
LICENSES/                 MIT.txt, CC-BY-3.0.txt, CC-BY-4.0.txt (+ CC-BY-SA-4.0.txt if Beatucada)
REUSE.toml                "**" → MIT, すごいジャン; then one block per song path with the real authors
assets/songs/<slug>/
  <song>.ssc              unmodified upstream chart
  <song>.opus             unmodified upstream audio (see transcoding note)
  <song>-bn.png, -bg.png  graphics, only when their licence is CC-BY
  credits.txt             upstream credits/licence file, verbatim
  PROVENANCE.toml         upstream repo URL + commit hash, retrieval date, files excluded (e.g. .mp4 BGA),
                          modifications (none, or exact transcode command), SPDX ids per file
credits/CREDITS.md        generated; checked in; CI fails if stale (beside THIRD_PARTY.toml, THIRD_PARTY_RUST.md)
```

`REUSE.toml` per-song blocks list `SPDX-FileCopyrightText` for the musician, every chart author from the `#CREDIT` tags, and the graphic authors, with `SPDX-License-Identifier` matching the file (separate blocks for audio, charts and graphics when the versions differ, e.g. `CC-BY-3.0` audio next to `CC-BY-4.0` art).

**Generated credits.** `xtask gen-credits` reads `REUSE.toml` and every `PROVENANCE.toml` and writes `credits/CREDITS.md` plus a JSON fragment compiled into the app. The app shows a Credits screen (title, creator, source URL, licence with link, chart authors, modifications) and links it from the song-select footer, satisfying CC-BY §3(a) in the published game, not only in the repo. `xtask gen-credits --check` runs in CI like the landing page's build check.

**CI.** A `reuse lint` job (`fsfe/reuse-action` in CI; locally `just reuse-lint`, which runs it through `uvx` with its encoding extra) and the credits check gate every push. Rust sources carry SPDX headers only if `reuse lint` requires them for the chosen layout; the `**` annotation covers them otherwise.

**Modifications.** Ship upstream bytes unmodified when the browser can decode them. Serenity audio is Opus-in-Ogg; Chrome, Firefox and Edge decode it, Safari only from 18.4 and unreliably. If Safari testing fails, add a transcoded copy (CC-BY permits it) and record the exact command in `PROVENANCE.toml` and the credits line, rather than replacing the original. Dropping large `.mp4` BGAs is an exclusion, not a modification, and is noted the same way.

**README and LICENSE.** The repo `LICENSE` is MIT and states that `assets/**` are under their own licences as recorded in `REUSE.toml` and `CREDITS.md`.

**Code from other projects.** Code or data copied into the game keeps its upstream form where possible (StepMania 5.1's two noteskin `metrics.ini` files byte for byte under `crates/noteskin/src/builtin/stepmania/`, StepMania 3.9's two under `crates/noteskin/src/builtin/stepmania39/`; danoniplus's key tables converted by a script), is annotated in `REUSE.toml` (StepMania's MIT-style licence as `LicenseRef-StepMania`, whose terms ask for the notice in all copies and in supporting documentation) and listed in `THIRD_PARTY.toml`, from which `CREDITS.md` and the Credits screen show each project's notice. Nothing is copied from ITGmania, GPLv3 since 2024-09-20; StepMania 5.1 holds the same older files under its own licence.
