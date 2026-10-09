# DitoDitoInfinito — research summary and plan

A DDR-style rhythm game (genre term: 4K VSRG, four-key vertical scrolling
rhythm game) written entirely in Rust. Browser first (wasm, Trunk,
GitHub Pages at `https://sugoijan.dev/ditoditoinfinito/`), desktop later, with
the gameplay core shared unchanged between both.

Status (2026-10-09): phases 0–3 done and deployed (scaffold, chart
importers, engine, web platform, renderer, song select, options, credits,
four bundled OutFox Serenity songs), phase 4 (format breadth), phase 5
(import and storage), phase 6 (layouts as data: StepMania `dance-solo` and
`dance-double`, Dancing☆Onigiri works in 5, 7, 7i, 9A and 9B keys with
their own rules, colours, speed and lyrics as options) and phase 7
(gamepads, bindings, touch lanes; WebHID left out) done, phase 8 partial (play options, note colour schemes, song
backgrounds and image background changes done; skin packs and video
backgrounds not); per-phase status in the roadmap (section 5).
Players import their own simfile packs (folder, zip or drag and drop; `.sm`,
`.ssc`, `.dwi`) into IndexedDB, edit a per-song offset on the results, and
Ogg Vorbis decodes in wasm where the browser cannot. Calibration runs on the real gameplay path (generated 120 BPM chart
with a click per note) and converges: each batch of 8 hits whose mean error
is significant (|mean| > 2σ/√n) is applied as a temporary correction, and the
test ends once 16 residual hits are within noise (`engine/src/calibration.rs`,
`app/src/calibration.rs`). Guided order: arrows only (muted, saves to the
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
Detailed research with sources lives in [`docs/research/`](research/):

- [`formats-sm-ssc-dwi.md`](research/formats-sm-ssc-dwi.md) — StepMania `.sm`/`.ssc` and DWI formats, timing semantics, existing parsers, test fixtures.
- [`formats-ffr-danoni.md`](research/formats-ffr-danoni.md) — FlashFlashRevolution (R^3) and Dancing☆Onigiri (danoniplus) chart data, key modes, judgement, gauges.
- [`rules-ddr-itg-dwi.md`](research/rules-ddr-itg-dwi.md) — DDR / StepMania / ITG / DWI timing windows, scoring, gauges, options, note colours.
- [`tech-stack.md`](research/tech-stack.md) — Web Audio clock, input latency, rendering, storage, native parity, build/deploy.
- [`cc-songs.md`](research/cc-songs.md) — freely redistributable simfiles (OutFox Serenity per-song licences, Sockpuppet pack), traps, attribution requirements.

Reference implementations to consult while building: StepMania 5.1 (`5_1-new` branch) for parsing and timing semantics, [Project OutFox](https://projectoutfox.com/) (the actively developed StepMania fork; its [wiki](https://outfox.wiki/) documents formats and mode support and its Serenity packs are CC-BY), ITGmania / Simply Love for ITG rules, danoniplus for Dancing☆Onigiri, rCubed for FFR.

---

## 1. Goals and non-goals

Goals, in priority order:

1. Accurate, low-latency gameplay in the browser. Audio is the master clock; every input is timestamped and judged against it. This is the one thing a rhythm game cannot get wrong, so it is designed first.
2. A platform-free core. `chart`, `engine` and `render` crates have no web or OS code and are unit-tested natively. The web app and the future desktop app are thin shells.
3. Pluggable everything the brief names: input devices, song sources and formats, note skins and colour schemes, background layers, and rulesets (judgement windows, scoring, combo, gauge, fail policy).
4. Eventually the union of DDR, StepMania, DWI, FFR and Dancing☆Onigiri features: holds, rolls, mines, lifts, fakes, shock arrows, speed/scroll changes, warps, arbitrary lane layouts (4-panel, 6-panel solo, 5-key onigiri, 7-key, …), lyrics and background events.

Non-goals for now: multiplayer, online scores, mobile as a sync-accurate target, keysounds, editor.

## 2. What the research settled

### 2.1 Chart formats (import sources)

| Source | Time base | Lanes | Note kinds | Timing features | Verdict |
|---|---|---|---|---|---|
| StepMania `.sm` | beats; 4-beat measures, rows at N ∈ {4,8,12,16,24,32,48,64,192} | per steps type (dance-single L,D,U,R; dance-solo L,UL,D,U,UR,R; double 2×4; …) | tap, hold, roll, mine, lift, fake, auto-keysound, attack | `#OFFSET` (time(beat 0) = −OFFSET), `#BPMS`, `#STOPS`, `#DELAYS`; negative BPM/stop = warp | **MVP** |
| StepMania `.ssc` | same | same | same | + `#WARPS`, `#SPEEDS` (beat=ratio=delay=unit), `#SCROLLS`, `#FAKES`, `#LABELS`, `#TIMESIGNATURES`, `#TICKCOUNTS`, `#COMBOS`; per-chart split timing (v ≥ 0.70) | phase 2 |
| DWI `.dwi` | beats; one char = 1/8 note, brackets `()`=1/16 `[]`=1/24 `{}`=1/64 `` `' ``=1/192 | SINGLE/DOUBLE/COUPLE/SOLO; numpad digits, `A`=U+D, `B`=L+R, `C`–`M` solo combos, `<…>` jumps | tap, hold (`X!Y`, released at next note in column) | `#GAP` ms (OFFSET = −GAP/1000), `#BPM`, `#CHANGEBPM`/`#FREEZE` in **quarter-beat** units, freeze in ms | phase 2 (small) |
| Dancing☆Onigiri (dos) | integer frames @ 60 fps; `blankFrame`/`startFrame`/`adjustment`/`playbackRate` | arbitrary, defined as data (5…23+ keys; onigiri/giko/iyo lanes; per-lane colour group, shuffle group, scroll sign, position, default keys) | tap, hold (`frz*_data` pairs), dummy notes | no BPM; `speed_data` (global), `boost_data` (per-note), `color_data`/`acolor_data`/`ncolor_data`, `word_data`, `back_data`/`mask_data`, `scrollch_data`, `keych_data`; per-chart gauge params in `difData` | phase 3 |
| FFR (R^3 `beatBox`) | integer frames @ 30 fps (+ optional ms, + per-song `sync`) | 4 fixed (L,D,U,R) | tap only; colour stored per note | none | late; content is not redistributable, importer only for user-owned data |
| KSF, BMS, `.osu`, `.sma` | various | | | | not planned; notes in research doc |

Semantics an importer must get right (all verified against StepMania source):

- Stop pauses **after** the row's notes are judged; delay pauses **before**. Delay is applied first when both sit on a row.
- Warps skip time; notes inside are unjudgeable unless a stop/delay sits on that exact row. `.sm` negative BPMs/stops convert to warps with StepMania's algorithm.
- `#SPEEDS` scale scroll speed with linear interpolation over beats or seconds; `#SCROLLS` rescale displayed beat distance. Neither changes judgement time.
- Hold tails (`3`) are derived from the head; rolls are heads of kind `4`.
- SM's internal grid is 48 rows per beat; it covers every quantization above and every DWI bracket. We adopt it.

Intentional deviations from StepMania 5.1 in our importer (more forgiving, documented in `chart/src/formats/sm.rs`): per-note `{mods:len}` attack suffixes are skipped rather than read as a column; `.ssc` files containing negative BPMs/stops are converted to warps like `.sm` instead of being rejected. `A` note characters are ignored like SM 5.1.

The `.dwi` importer (`chart/src/formats/dwi.rs`, whose module doc lists every deviation) follows the loader StepMania shipped up to 5.1.0. The current `5_1-new` branch carries an ITGmania change (`829f49f622`) that misreads jumps (`<24>` places only Down); we do not reproduce it. Other deviations: steps-type tags are case-insensitive, panels missing from the layout are dropped instead of landing in column 0, `#DISPLAYBPM` is read as decimal, non-positive freezes are dropped, and a chart StepMania would assert on is skipped.

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
- **Playback.** Decode whole song with `decodeAudioData` into one `AudioBuffer` kept in browser memory (never copied into wasm), start with ~100 ms lookahead via `AudioBufferSourceNode.start(when, offset)`. Song-select previews use Web Audio too (a context created in the click, the song decoded, a slice played with fades): `<audio>` cannot play Ogg in Safari, and Safari only starts audio synchronously inside the gesture. Prefer OGG/Opus; MP3 without a LAME/Xing header has decoder-dependent ~25 ms gaps that StepMania never trimmed, so per-song offset editing is a first-class feature.
- **Input.** Keyboard via `event.code` + `event.timeStamp`, ignore repeats, track pressed set. Gamepads must be polled; use a `MessageChannel` 1 ms wake loop rather than rAF (DDR is the W3C's own motivating example). Dance pads enumerate as HID gamepads; WebHID is a Chromium-only extra for later. As built (phase 7): `ddi_platform::gamepad::PadTracker` turns polled snapshots into edges (buttons, axis halves with hysteresis, hat switches reported as one axis) and stamps each edge with `Gamepad.timestamp` when it moved forward and is plausible for that poll, else with the poll time; the web poller (`app/src/web/gamepad.rs`) reads every pad in one JS call and runs the 1 ms loop only during play with a pad connected (option to poll per frame instead). The loop alternates a `MessageChannel` message and a `setTimeout` for the remaining time (a timeout set from a message task is not nested, so not clamped to 4 ms): ~900 polls/s at ~8–11% of the main thread in headless Chrome, against ~90% for a plain ping-pong spin, with no frame drops. Measured through a scripted fake pad (`auto=pad`): edges stamped with the pad's timestamp are judged exactly; poll-time stamping adds about +0.5 ms. All sources are merged, sorted by time and resolved by `LaneInput` (every press steps; a lane releases when its last control does). Touch lanes use pointer events on the play canvas.
- **Rendering.** wgpu 30 with `webgpu` + `webgl` features (WebGPU is Baseline since Jan 2026 but Firefox/Linux still needs the fallback), atlas-instanced sprites, glyphon for dynamic text, pre-rendered sprites for judgement words. Yew/DOM for everything outside gameplay. No winit on web; rAF + web-sys. Same `render` crate on desktop under winit. Bevy and macroquad rejected: their audio layers hide the hardware clock and scheduled starts.
- **Storage/import.** Bundled demo songs via Trunk `copy-dir`; user imports via `<input webkitdirectory>`, drag-and-drop (`webkitGetAsEntry` for folders) or zip, stored as `Blob`s in IndexedDB, `navigator.storage.persist()` after the first import. Zips are read sans-IO by slicing the `File` (`ddi-library::zip`: central directory, then one entry at a time; stored entries go to storage as slices without entering wasm), so a large pack never sits in wasm memory. IndexedDB instead of OPFS: see decision 8.
- **Fallback decode.** When `decodeAudioData` rejects a file, or returns silence (Safari ≥ 18.4 accepts Ogg Vorbis and decodes it to silence; detected by sampling windows across the buffer), Symphonia 0.6 (features `ogg` + `vorbis` only, ≈ 95 KB gzip, MPL-2.0) decodes it in wasm (`ddi-platform` feature `decode`). It covers Safari's Ogg Vorbis gap; Opus has no pure-Rust decoder there and stays browser-only.
- **Desktop parity.** `web-audio-api` 1.7 (Rust Web Audio API on cpal with `current_time`, `output_latency`, `start_at_with_offset`) makes the desktop audio backend a transliteration of the web one. winit 0.30 + gilrs on a 1 kHz input thread, SDL3 as fallback if winit's missing event timestamps hurt.
- **Constraints.** GitHub Pages cannot set COOP/COEP: no SharedArrayBuffer, no wasm threads, no cpal AudioWorklet backend, 100 µs timers in Chrome. The design never needs them. Firefox rounds event timestamps to 1–2 ms; `resistFingerprinting` makes it 100 ms, so detect and warn.
- **Size budget.** ≤ 2 MB compressed first load (wgpu both backends ≈ 2–3 MB raw, glyphon ≈ 1–2 MB raw, Yew small) with `opt-level="z"`, fat LTO, `wasm-opt -Oz`.
- **Crate versions (2026-10):** wgpu 30.0.1, glyphon 0.12, winit 0.30.13, web-audio-api 1.7.0, gilrs 0.11.2, zip 8.x, symphonia 0.6.1, trunk 0.21.14, yew 0.23. No usable `.sm` parser crate exists; we write our own and differential-test against `rgchart` 0.0.15 / `danceparser` 0.2.1.

---

## 3. Architecture

### 3.1 Workspace layout

Mirrors `../heddobureika` / `../noynoynoy` (Cargo workspace, edition 2024, Trunk, `justfile`, `xtask regen-seo`, boot shell in `index.html`, Pages workflow), with the game split into more crates because the platform-independence is the point.

```
DitoDitoInfinito/
  Cargo.toml                 workspace; default-members = ["app"]
  Trunk.toml  index.html  styles.css  boot-init.js  seo/metadata.toml  justfile
  .github/workflows/deploy.yml
  chart/        ddi-chart     chart model + TimingMap + importers (sm, ssc, dwi, danoni, ffr)
  engine/       ddi-engine    SongClock, judge, combo, score, gauge, scroll, options → Frame + events
  render/       ddi-render    wgpu scene (atlas sprites, text), NoteSkin, background layers
  platform/     ddi-platform  traits: AudioBackend, InputSource, HostClock, SongStore, Surface; optional Vorbis decode
  library/      ddi-library   pack scanning, sans-IO zip reader, manifest entries (shared by app and xtask)
  app/          ddi (web)     Yew shell, web-sys platform impls, canvas host     [bin, wasm32]
  xtask/                      regen-seo, fixture tools, (later) atlas packing
  assets/       songs/ (CC-licensed .sm fixtures), skins/, fonts/
  docs/
  (later) desktop/  ddi-desktop  winit + wgpu + web-audio-api + gilrs            [bin, native]
```

Dependency direction: `app` → {`platform-web impls`, `render`, `engine`, `library`, `chart`}; `library` → `chart`; `render` → `engine` (reads `Frame`); `engine` → `chart`; `platform` → nothing. `chart`, `engine`, `platform` compile for native and wasm and are tested with plain `cargo test`.

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
    pub id: String,               // "dance-single", "dance-solo", "danoni-5", "danoni-7", …
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

Frame-based sources (DanOni 60 fps, FFR 30 fps) import losslessly onto this grid with a synthetic constant tempo: at 75 BPM one beat is 0.8 s = 48 frames at 60 fps, so **1 frame = 1 tick** (FFR: 2 ticks per frame). `blankFrame`/`startFrame`/`adjustment`/`sync` fold into `offset_seconds`; `playbackRate` becomes a music-rate hint. Quantization-based colouring is meaningless for those charts, which matches the source games (DanOni colours per lane group, FFR stores colours per note), so `Note.color` carries the explicit colour and skins honour it.

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

`PlayOptions` (as built): `scroll` (x-mod or c-mod speed, reverse, scroll action Normal/Boost/Brake/Wave), `transform` (turn Off/Mirror/Left/Right/Shuffle; cut: timing Off/Quarters/Eighths, no jumps, no holds), `appearance` (Visible/Hidden/Sudden/HiddenSudden/Stealth), `seed` (fixes Shuffle), `clock` (audio and visual offsets, rate), `visible_range`, `receptor_snap`. It is `#[serde(default)]`, and the option enums deserialize leniently (`ddi_engine::lenient`: an unknown value falls back to that field's default instead of failing the whole settings object). `Player::new` applies the transform (`engine/src/transform.rs`) before building the judge, so judging, the score's note counts and the results all describe the transformed chart; ticks never change, so judged times cannot. Scroll actions (`ScrollAction::adjust`) and appearance (`engine/src/appearance.rs`) only change a note's drawn height and its alpha/glow in the `Frame`. `Results` carry the options played, the lane map and an `assist` flag (any cut). The autoplayer's press script (`engine/src/autoplay.rs`) is built from the transformed notes. The maths follows StepMania; sources in `rules-ddr-itg-dwi.md` §4.1. Not built yet: lane covers (Hidden+/Sudden+), music rate, a separate judge offset in the UI.

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

Skins are data (atlas image + TOML describing per-lane rotation or per-lane sprites, colour tables, animation) plus optional Rust implementations for procedural ones. Backgrounds are optional layers: none, static image, SM `BGCHANGES` playback, DanOni `back_data`, and built-in generative visualizers driven by beat/judgement events.

### 3.5 `platform` traits and the web shell

```rust
pub trait AudioBackend { type Buffer;
    async fn decode(&self, bytes: &[u8]) -> Result<Self::Buffer>;
    fn now(&self) -> ClockSample;                 // { context_time, host_time, output_latency }
    fn play(&mut self, b: &Self::Buffer, start_at_context_time: f64, offset: f64, volume: f32) -> SoundId;
    fn preview(&mut self, b: &Self::Buffer, offset: f64, len: f64, fade: f64) -> SoundId;
    fn stop(&mut self, id: SoundId); fn set_volume(&mut self, id: SoundId, v: f32); fn resume(&mut self); }
pub trait InputSource { fn poll(&mut self, out: &mut Vec<RawInput>); }   // RawInput { device, control, pressed, host_time }
pub trait SongStore  { async fn list(&self) -> Vec<SongSummary>; async fn load(&self, id) -> Result<FileSet>; async fn import(&mut self, files: FileSet) -> Result<SongId>; }
```

`Bindings` maps `(device, slot, control)` → lane and lives in the engine so a dance pad, a keyboard and a touch overlay are interchangeable; `LaneInput` adds the per-lane hold state on top. The saved bindings (`ddi_engine::bindings::ControlBindings`, flattened into the app's settings) keep one keyboard table per layout and, per controller model, a `dance-single` table plus one per other layout; `ControlBindings::resolve(layout, connected pads)` builds a play's `Bindings` (decision 21). Web implementations: `WebAudioBackend` (noynoynoy's `audio.rs` is the starting point), `WebKeyboard`, `Gamepads` (MessageChannel loop, `app/src/web/gamepad.rs`), `TouchLanes` (`app/src/web/touch.rs`), `BundledStore` (fetch under `assets/songs/`), the IndexedDB song library (`app/src/songs.rs`, `app/src/web/idb.rs`) and the folder/zip/drop import (`app/src/import.rs`, `app/src/web/files.rs`). The Yew app owns routing (hash router), song select, options, calibration, results, and a canvas component that hosts the wgpu renderer and the game loop (rAF). Desktop later reuses everything except these implementations.

---

## 4. MVP / proof of concept

One screenful of scope, deliberately small, but built through the abstractions so that widening it later is additive.

**In:**

- `.sm` import for `dance-single` (taps, holds, mines parsed; mines not yet judged), BPM changes and stops, negative-BPM→warp conversion. Three pure CC-BY songs from OutFox Serenity bundled under the scheme in section 8 (REUSE.toml, LICENSES/, generated credits, in-game Credits screen).
- Keyboard input (arrows and a second binding set like D/F/J/K), rebindable in a simple settings panel.
- Engine with one `Layout` (4-panel) and two presets to prove the plug point: `itg` and `ddr-a`-style money score. Taps + holds (OK/NG with decaying hold life), combo, gauge bar, immediate fail, grade.
- wgpu renderer with one note skin (quantization-coloured arrows, DDR "NOTE" style), receptors, hold bodies, judgement sprites, combo/score text. x-mod speed 1–8, reverse scroll.
- Audio clock design in full (anchor filtering, output latency, calibration): the calibration screen measures audio offset (tap to a click) and visual offset (tap to a moving marker) and stores them.
- Yew screens: song select with preview, options, gameplay, results. Hash routes so a song can be deep-linked.
- Trunk build, `justfile`, `xtask regen-seo`, GitHub Pages workflow, entry added to `../landing-page/links.toml`.

**Out (next phases):** `.ssc`, `.dwi`, DanOni, FFR; rolls, lifts, fakes, shock arrows; speeds/scrolls/warps rendering; gamepads; user import and storage; skin selection; backgrounds; desktop.

Definition of done: a chart plays in Chrome, Firefox and Safari with judgements that stay within a few ms of a known-good StepMania setup after calibration, verified with a recorded input log that is also a unit test.

## 5. Roadmap

Status as of 2026-10-09.

| Phase | Status | Deliverable | Notes |
|---|---|---|---|
| 0 Scaffold | done | workspace, crates with empty APIs, Trunk boot shell, SEO xtask, justfile, `REUSE.toml` + `LICENSES/` + `reuse lint` in CI, CI deploy of a "hello" page | copy conventions from noynoynoy/heddobureika and detonito |
| 1 Core | done | `chart` (`.sm` parser, `TimingMap` with seconds↔tick, warp conversion) and `engine` (clock, judge, holds, combo, score, gauge, scroll) with native tests and replay fixtures; differential tests vs `rgchart`/`danceparser` | no browser needed; this is where correctness lives. Differential tests (`chart/tests/differential.rs`): 300 seeded random `.sm` files per oracle, structure against `danceparser`, note times against `rgchart` (±2 ms); both agree. `DDI_DIFF_CORPUS=<dir>` runs the same comparison over any local song folder, plus a `.dwi`-against-`.sm` check where a folder holds both; it is opt-in and not part of CI |
| 2 Web platform | done | `WebAudioBackend`, keyboard, canvas + wgpu renderer, game loop; play a bundled chart | first playable |
| 3 MVP | done | song select, options, calibration, results, bundled songs with `PROVENANCE.toml`, `xtask gen-credits` + Credits screen, deploy | deployed with four OutFox Serenity songs |
| 4 Format breadth | done | `.ssc` (split timing, warps, speeds, scrolls, fakes), `.dwi`, rolls/lifts/fakes/mines judged, shock arrows | `.ssc` (speeds/scrolls drawn) and `.dwi` (hand-written fixtures, cross-checked against an equivalent `.sm`); rolls/lifts/fakes/mines verified in play with autoplay on a hand-made chart; shock arrows exist in the model and are judged as mines, but no supported format produces them |
| 5 Import & storage | done | folder/zip/drag-drop import, OPFS/IDB store, per-song offset editor, Symphonia fallback decode | IndexedDB only (decision 8); per-song offset on the results screen; Ogg Vorbis fallback only. Banners/backgrounds without tags are classified by image size like StepMania. Imports verified by hand in Firefox and Safari, including private windows; Firefox asks once per site to confirm sharing a folder's files. Chromium keeps a picked folder's handle (`showDirectoryPicker`) and re-reads it in place, removing songs whose folders are gone (decision 32); Shift-JIS simfiles and zip names decode (decision 31); imported packs show their banners (decision 30); the browser is asked to keep the library from a click or drop, and the song list says whether it does |
| 6 Layouts & DanOni | done | `Layout` as data in the UI, 6-panel solo, DanOni 5/7/7i/9A/11 key modes, onigiri lane, `speed_data`/`boost_data`, colour events, lyrics (`word_data`) | Layouts resolved through the song (decision 20), StepMania `dance-solo` and `dance-double` playable end to end: style selector on the song list (decision 22), keyboard and controller bindings per layout with the guided flow, double on two controllers (decision 21), touch columns per layout, Turn/Shuffle per layout, shuffle order on the results with each lane's arrow; imports keep any pack with a playable chart and list left-out layouts in the details for pack authors. Native replay fixtures autoplay solo and double all top tier under every turn and cut on the ITG, SM5 and DDR A presets; headless Chrome autoplays both on WebGPU and WebGL2, two identical fake pads play one half of double each, `auto=pad` on double stays all top tier. Every built-in Dancing☆Onigiri key mode (5 … 23 keys, pattern 0) is `Layout` data (decision 25), of which 5, 7, 7i, 9A and 9B keys are played (decision 27); two-row modes (kept for later) draw each row's receptors at its own end with its own scroll direction, touch splits such fields into halves, the bindings page and the bind flow offer the key modes the library has charts for; native fixtures autoplay 5, 7, 7i, 9A, 11, 12, 17 and 23 keys all top tier under every turn, headless Chrome autoplays 5 to 17 keys (charts written as `.sm` files with a `danoni-<mode>` steps type) on both backends. Dancing☆Onigiri works import (decision 26), in the key modes the game plays (decision 27): pages with inline or external dos (Shift_JIS and other declared charsets decoded by the browser, music found where danoniplus loads it or by name, encoded music scripts decoded) and dumps of the research tool's format; every chart of a work with holds, dummy charts, `speed_data`/`speed_change` as scroll segments and `boost_data` per note; one song per music file, the work's fields stored once for all of them; chart buttons show the chart's name; the x-mod plays a work at its declared tempo, or the chart's own speed as an option (decision 23); the work's note colours (`setColor`, `frzColor`, `color_data`, `acolor_data`, `ncolor_data`) as an option; `startFrame` starts the play part-way, `endFrame` stops and `fadeFrame` fades the music; lyrics (`word_data`) over the field (decision 29). Rules: "the ruleset below for every chart" by default, "each chart's own game's rules" and "Dancing☆Onigiri rules for every chart" as options, with danoniplus's frame windows, hold rules, gauges (including a work's custom ones), score and ranks (decision 28). `DDI_DANONI_CORPUS=<dir> cargo test -p ddi-chart --test danoni_corpus` compares every captured chart of a local dump corpus with danoniplus's own parse (lane names, tap and hold frames, speed and boost events, adjustment): the 9 references of the 2026-10-09 corpus match exactly; headless Chrome imports danoniplus's sample pages and the corpus and autoplays 5, 7 and 7i charts all top tier on both backends (AP and 1,000,000 under a work's own rules), with chart colours on and off; the onigiri, giko and iyo symbols are checked under the vivid, rainbow and subdivision schemes. Left out: key-group switching (`keych_data`; a work like Defeat,I Doll draws all its lanes at once), `playbackRate` (no work at hand uses a rate other than 1; it needs judging windows in real time and a rate on the music), divided external dos, `wordRev_data`, `back_data`/`mask_data` (phase 8) |
| 7 Input devices | done | gamepad poller, dance-pad profiles, touch lanes, WebHID experiment | 1 ms `MessageChannel` poll loop with pad-timestamp/poll-time stamping (`PadTracker`, natively tested); bindings per controller model (`Gamepad.id`) for buttons, axis directions and hat directions, with a guided "press each arrow" flow for the keyboard and any pad and standard-mapping defaults; controller Start/Back on the play screen; touch lanes aligned to the receptors. Verified in headless Chrome with a scripted fake `getGamepads` (`auto=pad` plays a chart through it). The song list moves with a controller, a dance pad's arrows and Start/Back, or the arrow keys (decision 30). Not done: WebHID (out of scope), a Safari-specific pad start (Safari does not treat pad presses as a gesture, so the prompt asks for a tap) |
| 8 Presentation | partial | skin packs (static/quantized/rainbow/vivid), background layers (BGCHANGES, DanOni back/mask, generative), DDR option set (Boost/Brake/Wave, Hidden/Sudden, Turn, Cut) | Done: Turn (Mirror/Left/Right/seeded Shuffle), Cut (quarters or eighths only, no jumps, no holds; marked as assist on the results), Boost/Brake/Wave, Hidden/Sudden/Hidden+Sudden/Stealth, all following StepMania's maths; options persisted, summarised on song select and the results, and ignored by calibration. Native replay fixtures show identical judgements under every scroll action and appearance, lane-permuted judgements under every turn, and an all-top-judgement autoplay on the ITG, SM5 and DDR A presets under every turn × cut × scroll action, with each appearance paired in; headless Chrome autoplays (WebGPU and WebGL2) with several options on at once also end with only top judgements. The song's static background drawn behind the field (decoded by the browser, copied into a texture; brightness, default 40 %, and a DDR-style field filter behind the lanes, default 40 %). Note colour schemes on the procedural skin: by subdivision, vivid and flat cycling rainbows (StepMania's `midi-vivid` hues and 4-beat cycle, with the tail-to-tip gradient along each arrow measured on DDR footage) and rainbow by position (DDR's bands, each a two-shade gradient flowing once per beat), plus a single colour; vivid is the default (`render/src/note_colors.rs`). Both backends draw in linear light through an sRGB view (decision 17). `#BGCHANGES` playback for still images (`library/src/backgrounds.rs`, decision 18), verified with a synthetic pack on both backends. Not done: video and scripted backgrounds, `#BGCHANGES2`/`#FGCHANGES` overlays, skin packs |
| 9 Desktop | not started | `desktop` crate: winit + wgpu + web-audio-api + gilrs; same `engine`/`render`/`chart` | SDL3 if winit timestamps are a problem |
| 10 Later | not started | FFR `beatBox` import (user-owned data), replays, more rulesets (DDR legacy scoring, FLARE, LIFE4), editor | |

Done beyond the original plan: calibration that converges on the real
gameplay path, per-device audio/display offset profiles with app-wide banners,
receptor snap, fail fade, cancel gesture, debug overlay.

## 6. Decisions (1–7 confirmed 2026-10-06; 8–10 made with phase 5, 2026-10-07; 11–12 with phase 7, 2026-10-07; 13–19 with phase 8, 2026-10-08; 20–29 with phase 6, 2026-10-08/09; 30–33 with the loose ends, 2026-10-09)

1. **Licence: MIT.** Bundled songs keep their own licences in `assets/songs/*/LICENSE`.
2. **Rendering: wgpu from day one** (WebGPU + WebGL2 fallback), not Canvas2D, because desktop parity is a stated goal and heddobureika already proves the wgpu+Trunk path. Costs ~1 MB on the wire.
3. **Menus in Yew, gameplay in wgpu.** Same split as heddobureika; the HUD stays in the renderer so it exists on desktop.
4. **Frame-based sources map onto the 48-ticks-per-beat grid via a synthetic tempo** (75 BPM: 1 frame = 1 tick at 60 fps, 2 ticks at 30 fps) instead of adding a seconds-based position variant. For Dancing☆Onigiri at playback rate 1, danoniplus plays note frame `F` at `(F − blankFrame + floor(adjustment)) / 60` seconds into the music file (the fraction of `adjustment` cancels out in v51; `formats-ffr-danoni.md` B8), so a chart's `offset_seconds` is `(blankFrame − floor(adjustment)) / 60`, per chart since `blankFrame` and `adjustment` are.
5. **Deploy path: lowercase `--public-url /ditoditoinfinito/`** derived in the workflow from the repo name, with hash routing. Correction (2026-10-07): on a custom domain GitHub Pages serves the project path **case-sensitively** (`/DitoDitoInfinito/` and `/ditoditoinfinito/` are different URLs), so the repository was renamed to `sugoijan/ditoditoinfinito`; the local folder keeps its mixed-case name.
6. **Branch: `main`** (matches landing-page and heddobureika).
7. **MVP ruleset presets: `itg` and `ddr-a`.** DDR A+ exact windows are undocumented, so `ddr-a` uses the community frame table (±16.7/33/92/142 ms) and is labelled approximate in the UI.
8. **Imported songs live in IndexedDB as `Blob`s, not OPFS** (2026-10-07). The access pattern is write once, read whole files, which IndexedDB handles everywhere; OPFS is absent in Firefox private windows and Safari gained main-thread writes only in version 26. One code path instead of two.
9. **Fallback decode is Ogg Vorbis only** (2026-10-07): browsers decode MP3/WAV/FLAC natively, Opus has no pure-Rust decoder, and each extra Symphonia codec costs wasm size.
10. **Per-song offset moves the notes, not the audio**: `Song::shift_notes` lowers `#OFFSET` (and every split timing), like StepMania's song offset edit, stored per song id in the settings. Positive = notes later.
11. **Controller bindings are keyed by `Gamepad.id` and never assume a layout** (2026-10-07). The id describes the model and stays the same across sessions (the index does not); identical pads share bindings, fine for one player. Browsers format the id differently, so each browser keeps its own. Only pads with the `standard` mapping get defaults; everything else (dance pads, adapters) is bound with the guided flow, which records whatever control moves: a button, an axis direction or a hat direction.
12. **Gamepad edges are stamped with `Gamepad.timestamp` only when it is plausible** (2026-10-07): it must have moved since the previous poll and lie between shortly before that poll and now; otherwise the poll time is used. Browsers differ in whether and how they update it, so the per-pad split is shown in the debug overlay rather than assumed. The 1 ms loop only runs during play with a pad connected and a visible tab, and can be turned off.

13. **Play options follow StepMania where it defines the maths** (2026-10-08), or DDR where StepMania has no equivalent (the eighth-note cut), cited in `rules-ddr-itg-dwi.md` §4.1. Turn and Cut are chart transforms inside `Player::new`; scroll actions and appearance only move or hide what is drawn. Cut makes a play an assist; Turn does not (as in DDR).
14. **Shuffle uses a built-in SplitMix64**, seeded per play by the app and kept in the results (2026-10-08), so a dependency update can never change a seeded shuffle and a play stays reproducible.
15. **Hidden/Sudden lines are scaled to the screen, Boost/Brake are not** (2026-10-08): StepMania's lines are screen pixels on a 7.5-arrow-tall screen, ours is 10 arrows tall, so they are scaled by 10/7.5 (approximately the same place on screen: our receptors sit 1.5 arrows from the top, 15 % of the screen against StepMania's 20 %, so the Hidden line lands at about 44 % of the screen height instead of 49 %; a portrait screen is taller than 10 arrows); Boost and Brake act on beats before the speed multiplier, so StepMania's units carry over unchanged.
16. **Background images are decoded by the browser** (`createImageBitmap`, scaled down to at most 2048 px a side, the size every WebGL2 device supports; an image still over the device's limit is skipped) and copied with `copy_external_image_to_texture` (2026-10-08): no image decoder in the wasm, and the same path works on WebGPU and WebGL2. A desktop shell would write decoded RGBA into the same texture. Brightness and the field filter are perceptual on either kind of target (see decision 17).

17. **Draw in linear light through an sRGB view on both backends** (2026-10-08). WebGL2 offers an sRGB surface; a WebGPU canvas has none but accepts an sRGB view of its format (`viewFormats`), so both blend in linear light and look the same. Colours stay written as they should look (sRGB-encoded, `render/src/color.rs`) and the renderer converts them to linear for an sRGB target, including the sprite shader's own constants; solid colours look exactly as on the earlier WebGPU path, only anti-aliased edges and translucent overlays blend more smoothly. Text is converted the same way, which keeps glyphon's darker look from the old WebGPU path. A browser refusing the sRGB view keeps the plain path, and the renderer passes colours through unchanged there.

18. **Background changes play still images only, following StepMania** (2026-10-08): the first layer of `#BGCHANGES` becomes a schedule on the song's timing (a change shows when the song reaches its beat, before a delay there), the song background shows before the first change and, unless the file ends with the `-nosongbg-` marker, again from the last beat, `CrossFade` transitions fade (1, 0.75 or 0.5 s; StepMania's wipes and slides become 1 s fades, unknown names cut), and videos, scripted animations and missing files show the song background. The images a song refers to are listed in its manifest entry (`bg_images`, earliest first) and stored with an imported song under `bg/<path>` (at most 32, scaled to 1280 px); they are decoded during the start prompt and copied to the GPU before the music starts, or during play only when no change is due within a second. A scan of 735 local DDR/DWI charts found 57 with `#BGCHANGES`, all of them videos, so the feature was first tested with a synthetic pack, then with a real one: D2R from Suko's ZiV DDR Legacy Accurate collection cuts between two stills (`MAX-EXTREME/robot1.png`, `robot2.png`) at beats 243.75 and 244.75, verified locally with the images placed in the song folder. StepMania finds such names in its shared folders, so imports now recognise them too (decision 19).

19. **Shared background folders are part of the library, resolved at play time** (2026-10-08). Any imported folder named `RandomMovies` or `SongMovies`, or one of those followed by a separator (ignoring case; a picked `RandomMovies (low rez).zip` imports as such a folder; the innermost one counts when a zip wraps its own folder; never inside a song folder), is a shared folder: its images are stored once for the whole library under `shared/<RandomMovies|SongMovies>/<path>`, in batches; videos are skipped. The import records which background-change images a song's folder lacks (`bg_shared`, as StepMania lets a song-folder file win); when the song is played those are looked up in StepMania's order (`SongMovies/<pack>/`, `SongMovies/`, `RandomMovies/`; `BackgroundUtil::GetGlobalRandomMoviePaths`), so songs and shared folders can be imported in either order. The import report lists songs that were not imported (including folders with music but no simfile) and warnings for songs imported with something missing (`#BANNER`/`#BACKGROUND` naming a missing file with no other image to stand in, since StepMania quietly uses a stand-in and packs often rename files without updating the simfile; background-change images found nowhere (videos are not reported: they are never shown), more than 32 change images); none of these stops the import. A "details for pack authors" box in the import panel (saved in the settings) also lists inconsistencies that make no difference in play: renamed files a stand-in replaced, and background videos or animations, which are not played.

20. **Layouts are data resolved through the song** (2026-10-08). A chart names its layout by id; the song's own layouts (a file may define them) come first, then the built-in ones. Turns are per layout: Left and Right only where the layout has StepMania's table, Mirror and Shuffle within each shuffle group as danoniplus does (`applyMirror`/`applyRandom`, `js/lib/dataLoader.js`), which for StepMania's one-group layouts is the full reversal and the full shuffle; a seed gives the same shuffle as before groups existed.

21. **Bindings are kept per layout and per device; two-pad layouts take two controllers** (2026-10-08). The keyboard has a table per layout (none saved: the layout's default keys). A controller keeps its `dance-single` table (the saved shape stays as it was) plus one per other layout; standard-mapping controllers get defaults for the built-in layouts (solo: shoulder buttons for the diagonals; double: d-pad left pad, face buttons right pad). On `dance-double`, controllers without a double table of their own each play one pad with their single bindings, by `Gamepad.index` (the lower one on the left), as StepMania gives each side its own controller; edges carry the pad's index (`RawInput::slot`) so two identical pads, which share `Gamepad.id`, stay apart. A lone standard controller plays the whole double field on its defaults; further controllers beyond the layout's pads are unused; the sides follow `Gamepad.index`, so an idle controller with a lower index takes the left pad (pads are revealed by a press, and a pad first pressed during play re-resolves the sides). The options page labels each controller from the same plan (`ControlBindings::plan`) the play uses. A saved but empty table means "cleared" and hides the defaults; "use defaults" drops a layout's table again. Start and Back are shared by every layout: the guided flow keeps them when skipped and refuses a control that plays a lane in another layout's table, and a control bound to a lane later clears them. Older settings load unchanged: `keys_single` moves into the per-layout table on load and is still written as a mirror of the single table, so a tab of an older version left open across an update keeps the player's keys.

22. **A style selector picks the layout on the song list** (2026-10-08), remembered in the settings, offering only layouts the library has charts for; songs without a chart in that style are hidden. The options page edits bindings for one style at a time.

23. **One consistent game first, an archive second** (2026-10-08). Default ruleset mode: the chosen StepMania preset for every chart, Dancing☆Onigiri charts included; "each chart's original rules" and "Dancing☆Onigiri rules for every chart" are options (`presets::for_chart`; the importer keeps a work's gauge, hold and Excessive headers so switching needs no re-import). Speed: one setting meaning the same everywhere (on frame-based charts the x-mod acts as a constant speed at the work's declared BPM, else at 150 BPM), with the chart's own DanOni speed as an option (`speed_for_chart`: danoniplus moves notes 2 px a frame per unit of speed on 50 px arrows, 2.4 arrow heights a second, over a field about as many arrows tall as ours, so the conversion keeps both the spacing and the reading time). Note colours: the player's scheme everywhere, a work's own colours as an option. Lyrics show by default (they are the work's text, not a gameplay change) and can be turned off.

24. **Dancing☆Onigiri behaviour follows danoniplus v51.2.2 (`80c3c47`)** (2026-10-08) where it defines it; the points the earlier notes got wrong or left open are in `formats-ffr-danoni.md` B8 (judging buckets are `[−k, k+1)` frames, cumulative hold release budget, force-removal of a late note, Light/Easy recovery doubling, colour events keyed by arrival frame).

25. **Dancing☆Onigiri key modes are built-in layout data generated from danoniplus** (2026-10-08): pattern 0 of every mode in `g_keyObj` (`chart/src/layout/danoni_table.rs`, ids `danoni-<mode>`, named "<mode> keys"), with key names converted to `KeyboardEvent.code` as danoniplus does; other key patterns are left to the bindings. Lanes are one lane width apart (danoniplus spaces them 45–57.5 px for 50 px arrows). A lane in the lower row (`pos > div − 1`) has its receptor at the bottom and scrolls down (Reverse swaps both rows), and the field is centred where danoniplus centres it, so the upper row of 11 keys sits right of centre and uneven fields (9i, 14i) keep their offset. On touch screens each half of a two-row field plays its row, and a finger keeps the row it went down on while it slides. 12i keeps danoniplus's F1–F12 and also gets its second pattern's letter row, since laptops send F-keys as media keys. Lanes are labelled by number and direction ("6 · Down-left"); giko, iyo and the other ASCII-art lanes draw as plain circles until there is art for them. `.sm` files may use a `danoni-<mode>` steps type, which is how charts on these layouts are tested before the Dancing☆Onigiri importer exists.

26. **Dancing☆Onigiri works are read the way danoniplus reads them, without running them** (2026-10-08). The importer (`chart/src/formats/danoni/`) follows `dosConvert`, `headerConvert`, `keysConvert` and `scoreConvert` (v51.2.2): fields split on `|` and `&`, later fields win, note arrays are `parseFloat` lists, effect values are evaluated as plain arithmetic (danoniplus evaluates them as JavaScript; anything beyond arithmetic falls back to the field's default), frames land on ticks at 75 BPM with the offset `(blankFrame − floor(adjustment)) / 60`. External dos files are scripts in danoniplus; only a literal `g_externalDos = …` assignment is read, and a work that builds its dos with code is reported, never run. An imported song stores its music number and a reference to the work's merged dos fields, which are stored once per work under a key named by their content (`works/<hash>`, dropped when no song refers to them; songs imported before keep the fields inline), so loading re-imports it and a better importer improves stored songs. A work's own key modes get ids `danoni-<mode>~<hash of the definition>`, so identical definitions share bindings across works. `playbackRate` is not applied: charts play at the music's own speed, the timing their authors wrote against. `startFrame` starts the play part-way, with the music joining `blankFrame` frames later and fading in as danoniplus does; `endFrame` stops the music (notes after it are left out, as danoniplus never reaches them) and `fadeFrame` fades it; each per chart with danoniplus's fallbacks, and the fades scheduled on the audio clock so they follow the music exactly. Frame values are truncated like `parseInt`. A tap on the frame of a hold start in the same lane is dropped (danoniplus judges neither). The dump format of the research tool (`docs/research/danoni-corpus.md`) imports too, which is how the local corpus is played; third-party works stay out of the repository. Imported files are untrusted, so the importer bounds what they can cost: shorthand lists expand to at most 1024 entries (an adversarial review found `x@:99999999999` exhausting memory), key modes to 255 lanes, field references to 8 MB, expressions to 64 levels of nesting, works to 512 charts, colour changes to 100,000 per chart with 1024 targets an entry, lyrics to 1 KB each and 4 MB a work, rule headers to 64 KB a work, and frames beyond 2⁴⁰ are dropped; a second review's stress inputs (lyrics re-copied per cue, a quadratic comma merge, colour lookups scanning every change) run in milliseconds and megabytes now. Header-defined key mode ids hash an explicit, versioned description, so a later importer keeps them stable.

27. **Four lanes first, with near neighbours** (2026-10-09). The game is a 4K VSRG with room for adjacent formats: StepMania Single, Solo and Double, and Dancing☆Onigiri 5, 7, 7i, 9A and 9B keys (`PLAYED_DANONI_MODES`). The rest of danoniplus's key modes stay as data (custom keys refer to them) but are not offered, and works in them, or in key modes a work defines or redefines itself (Twilight's 15-lane `7wi`, Defeat,I Doll's 43-lane `Tr`), are left out with a note in the import report. Non-arrow lanes draw as symbols in the arrows' style (a thick stroke with a darker rim; receptors as thin outlines): a star for the onigiri, a square for giko, a circle for iyo; a triangle would read as an up arrow. The colour schemes' gradients apply to them radially: the flowing schemes as rings moving outward with the arrows' phase, the tail-to-tip ramp as centre to rim.

28. **Dancing☆Onigiri rules as danoniplus plays them, inside the engine's rule model** (2026-10-09; `engine/src/rules/danoni.rs`). Windows are frame buckets, `k` frames early and `k + 1` late, for the four judge ranges; Shobon is the fourth tier and the fifth is off. Holds keep a cumulative release budget of `frzAttempt` frames (`HoldRules::ReleaseBudget`), start only within the `sfsf` window (`HoldStart::accept`) and are judged as taps only with `frzStartjdgUse` (otherwise they count by their outcome alone, and the chart's steps exclude them). A late note gives way to the next one in its lane (`Supersede`): a tap to the next tap once that is within Shakin and the tap more than Ii late, a hold's start to the next tap or hold by `sfsf` and Kita; a hold never takes over from an earlier tap. Matari neither adds to nor breaks the combo (`ComboRules::neutral`); hold outcomes keep their own combo inside the score. Gauges are resolved per chart the way `resolveGaugeValues` does: the survival or border list by the difData border, `customGauge` (inline or `survival`/`border`), difData on the first overridable family with Light and Easy taking twice its recovery, then `gauge<Name>` headers (per chart via `$` lists and numbered headers), with the variable gauges scaled by the note count; border gauges are checked at the end, survival gauges stop at zero. The player chooses a gauge by name (a chart that does not offer it plays its first), the judge range, and Excessive (the chart's setting by default). A border is checked at the end in life units, as danoniplus compares; a chart of holds alone whose starts are not judged earns the full-combo lamp when all are held. Score `(8·Ii + 4·Shakin + 2·Matari + 8·Kita + 2·maxCombo + 2·freezeMaxCombo) / (10·fullArrows) × 10⁶` with ranks SS…D, AP, PF and F. Fast and slow count only beyond a dead zone, one frame under these rules (`justFrames`) and 1 ms elsewhere, so on-time presses (autoplay) count as neither. Rank X (no rank) is not shown; rolls and mines from other formats keep StepMania's windows, and a mine costs a miss's damage.

29. **Lyrics are an HTML overlay** (2026-10-09). The HUD font has Latin glyphs only and lyrics are often Japanese, so `word_data` is drawn by the browser over the canvas (`app/src/lyrics.rs`), from an engine timeline (`engine/src/lyrics.rs`) that follows `updateWord`: even depths at the top of the field, odd at the bottom, swapped under Reverse on one-row modes (`wordAutoReverse` overrides; lyrics with line breaks stay), `[fadein]`/`[fadeout]`, alignment and `[fontSize=n]` relative to danoniplus's 500 px field; markup is reduced to text with line breaks. The DOM changes only when a line does.

30. **The song list is an accordion of packs that keeps its place** (2026-10-09). A controller button still held from the page before (the Start that picked a song, the Back that left one) is not a press on the next page: a pad's first reading reports held buttons only when its timestamp says they went down after the page opened, which keeps the press that reveals a pad to the browser (`PadTracker::held_before`). The bundled songs come first as a pack like the others; one pack is open at a time, each headed by its banner (StepMania's group banner: the first image at the pack folder's root, `png` before `jpg`, `jpeg`, `gif`, `bmp`, else an image named like the folder beside it, `SongManager::AddGroup`), or its name where it has none. The open pack and the chart last started are kept per tab (`sessionStorage`), so coming back from a play shows the same pack with that chart focused. The list moves with the arrow keys and Escape, a standard controller's d-pad or left stick with A/Start and B/Back, or a dance pad's arrows (its `dance-single` table) with its Start and Back (`ControlBindings::menu_input`); up and down move between rows keeping the column, back goes from a song to its pack and closes an open pack, and a held direction repeats.

31. **Shift-JIS where text reads as Japanese** (2026-10-09). A simfile that is not UTF-8 is decoded as Shift-JIS (the browser's `TextDecoder`, strict) when the result reads as Japanese, else as Windows-1252. Windows-1252's accents and typographic quotes are Shift-JIS lead bytes and half-width katakana, so Western text can decode cleanly into lone kanji inside Latin words ("Don’t" → "Don稚"); such a character rules Shift-JIS out, and Japanese is recognised by full-width kana or forms, runs of kanji or half-width katakana, or a kanji standing alone, with NEC's and IBM's extensions accepted (`pack::reads_as_japanese`). A zip's legacy names are decided for the whole archive (UTF-8 if all are valid, Shift-JIS if all decode and read as Japanese, else CP437), so a folder never splits between two readings. StepMania only tries UTF-8 and the system code page, so on a Western system it shows such titles as mojibake; Japanese packs are common enough to do better. Songs imported before keep their stored text until imported again.

32. **Keeping the library** (2026-10-09). Browsers may clear a site's storage under pressure unless it is marked persistent; that covers IndexedDB and OPFS alike, so moving the library to OPFS would not help. The game asks (`navigator.storage.persist()`) from the click or drop that starts an import, since Firefox shows a prompt, and the song list says whether the browser keeps the songs, with a "keep them" button when it does not. Where `showDirectoryPicker` exists (Chromium), a picked folder's handle is kept in IndexedDB under a key of its own (the same folder picked again, by `isSameEntry`, keeps its key; two folders may share a name), every song imported from it records that key, and an open pack from it offers "re-read folder": the folder is imported again after the browser's permission prompt, changed songs update, new ones are added, and songs from that folder whose folders are gone are removed (not those inside a zip that failed to read). Removing songs is disabled while an import runs, since it clears shared records the import may be about to use. Firefox and Safari keep the folder input; there a pack is updated by picking its folder again.

33. **Folder imports are linked where the browser can keep the folder** (2026-10-09). Copying a whole library into the browser costs its full size (2.4 GB for a local test library, almost all music; OPFS would cost the same). Where `showDirectoryPicker` exists, a chosen folder's songs keep their chart text and banner in the browser (kilobytes, so the list needs no disk access) and record the rest (music, background, background-change images) as paths inside the kept folder, a zip's entry by name (`songs::Link`); a song reads them through the folder handle when it plays or previews, a zip's directory read again. A linked song is about 5 % of a copied one. The browser asks again for a kept folder after a restart (Chrome offers "allow on every visit"): the chart and preview clicks ask in advance, and the play page shows an "allow reading the folder" button when the permission is still missing. A folder moved or deleted leaves its songs unplayable, with a message pointing to re-reading or importing again. Re-reading keeps how a pack was imported (linked or copied); files whose names hold `\` are copied (Chromium lists them but will not open them by name); a zip's directory is read once per play for all its linked files; images of shared background folders (`RandomMovies`) are still copied, since they serve many songs and other imports. "Store a chosen folder's songs in the browser" copies as before; zips and files picked or dropped are always copied, and Firefox and Safari copy everything. Copies stay in IndexedDB: OPFS would give copies and links the same shape (a folder handle and a path) but needs a worker to write in Safari and a migration, while the same uniformity is kept in code (`songs::song_file` reads a copy or a link).

## 7. Risks

- Safari: codec gaps (Ogg only from 18.4 and shaky), `getOutputTimestamp` regressions, context interruptions, 44.1 kHz contexts. Mitigation: sanity checks, MP3-with-Info-tag or Opus-in-WebM for bundled songs, Symphonia fallback later.
- MP3 decoder delay mismatch vs StepMania-synced packs (≈25 ms). Mitigation: per-song offset editing, calibration, known-good decode path.
- Gamepad polling competes with rendering on the main thread. Mitigation: one JS call per poll, a loop that sleeps between polls and runs only during play with a pad connected, an option to poll per frame, FPS and polls per second in the debug overlay. Touch targets are small on phones (the field's arrow size is capped in device pixels, so middle lanes are about 50 CSS px wide in portrait).
- Safari does not treat gamepad presses as a user gesture (WebKit bug 217675), so a song started with a controller's Start button cannot start audio there; the prompt then asks for a tap or key.
- Firefox timestamp precision and `resistFingerprinting`. Mitigation: detect quantised timestamps and warn.
- Ecosystem churn (wgpu majors every few months, winit 0.31 beta, Trunk 0.22 rc). Mitigation: pin, upgrade deliberately, keep wgpu behind `render`.
- DDR's exact modern windows and gauge numbers are not public; presets are approximations and should say so in the UI.
- Safari: `decodeAudioData` accepting Ogg Vorbis and returning silence; covered by the silence check, but a decode that returns noise instead would not be caught. Previews decode the whole song (~0.5 s before sound).
- Imported packs: text in legacy encodings other than Shift-JIS (Korean, Chinese) still comes out as Windows-1252/CP437 mojibake (the charts parse); wasm memory grows by a song's raw PCM size after a fallback decode and never shrinks; browsers may evict the library unless `persist()` is granted (Safari's 7-day rule for origins without interaction).

- A browser that refuses an sRGB view of the canvas falls back to drawing without one (colours stay right; edges blend as before). The headless runs read `srgb_view` from the debug state; Firefox and Safari are only checked by hand.

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
CREDITS.md                generated; checked in; CI fails if stale
```

`REUSE.toml` per-song blocks list `SPDX-FileCopyrightText` for the musician, every chart author from the `#CREDIT` tags, and the graphic authors, with `SPDX-License-Identifier` matching the file (separate blocks for audio, charts and graphics when the versions differ, e.g. `CC-BY-3.0` audio next to `CC-BY-4.0` art).

**Generated credits.** `xtask gen-credits` reads `REUSE.toml` and every `PROVENANCE.toml` and writes `CREDITS.md` plus a JSON fragment compiled into the app. The app shows a Credits screen (title, creator, source URL, licence with link, chart authors, modifications) and links it from the song-select footer, satisfying CC-BY §3(a) in the published game, not only in the repo. `xtask gen-credits --check` runs in CI like the landing page's build check.

**CI.** A `reuse lint` job (installed with `pipx`/`uv tool run reuse`) and the credits check gate every push. Rust sources carry SPDX headers only if `reuse lint` requires them for the chosen layout; the `**` annotation covers them otherwise.

**Modifications.** Ship upstream bytes unmodified when the browser can decode them. Serenity audio is Opus-in-Ogg; Chrome, Firefox and Edge decode it, Safari only from 18.4 and unreliably. If Safari testing fails, add a transcoded copy (CC-BY permits it) and record the exact command in `PROVENANCE.toml` and the credits line, rather than replacing the original. Dropping large `.mp4` BGAs is an exclusion, not a modification, and is noted the same way.

**README and LICENSE.** The repo `LICENSE` is MIT and states that `assets/**` are under their own licences as recorded in `REUSE.toml` and `CREDITS.md`.
