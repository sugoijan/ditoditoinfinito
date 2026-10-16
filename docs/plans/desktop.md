# Implementation plan: desktop parity (2026-10-16)

Step 7 of `docs/plans/ui-core.md`, joined with the rest of phase 9
(`docs/PLAN.md`, section 5): the desktop shell grows from a window running
the theme's screens on sample data into the game. Audio with a clock the
timing model can trust, keyboards and controllers with timestamps, play
with the same engine, renderer, noteskins and results as the web, and the
library read from folders on disk. Research: `docs/research/tech-stack.md`
sections 1, 2, 4 and 5 (clocks, input, native audio and input),
`docs/research/video-backgrounds.md` (the decoder), and the crates checked
on crates.io on 2026-10-16 (versions below).

Status (2026-10-16): proposed. Nothing here is built yet; every choice
below is a proposal with its reason, settled step by step as in the other
plans. What the lead should decide is gathered at the end.

## Goals

- **`cargo run` plays songs from local folders** with the same screens
  (song select, options, calibration, play, results, credits), the same
  timing model and the same results as the web, and keeps the play
  history.
- **One game, two shells.** What a play does (the session, calibration,
  song select's data, the import's song building) is shared code, not a
  desktop copy of the web's. The shells keep only what is the platform's:
  audio output, input, windows, files, storage.
- **The timing model of `AGENTS.md`, unchanged.** The audio clock is
  authoritative; inputs carry host timestamps taken as close to the
  hardware as the platform allows and are judged against heard time
  (stream time minus output latency minus the user's audio offset); the
  visual offset moves only what is drawn; offsets are kept per device
  profile, now from real device names.
- **macOS first, Windows and Linux built and tested in CI.** The
  maintainer has a Mac and a standard controller; Linux is checked in
  local containers; Windows only in CI.
- **Measured before built on.** The audio clock, keyboard stamps and the
  controller thread are measured on macOS before play depends on them
  (step 7.0).

Not goals for now: packaging and signing (an `.app`, an installer, a
Flatpak), automatic updates, ASIO or JACK, dance-pad lights, keysounds,
multiplayer, mobile.

## Where it starts

- **`desktop/`** (`ddi-desktop`, run by `app/` natively): a winit 0.30
  window, the painter over a plain-format surface (`desktop/src/gpu.rs`),
  cosmic-text with the bundled faces and system fonts, keys as the DOM
  names them (`keys.rs`), typing with winit's IME in own mode
  (`shell.rs`), rfd, arboard, links through the OS (`os.rs`), banners
  decoded off the draw path (`images.rs`, PNG and JPEG), settings in
  `storage.json` (`store.rs`), the play history's file store
  (`plays.rs`, nothing records yet). The screens run on sample data
  (`ddi_ui::screens::sample`). `UiRequest::StartAudio`, `PlayPreview`,
  `StopPreview`, `Navigate` and `FieldPreview` are ignored.
- **`platform/`:** `ClockSample`, `HostTime`, `RawInput`, `DeviceId`,
  `AudioBackend` (implemented only by `web/src/web/audio.rs`),
  `InputSource`, `HostClock`, `DeviceFingerprint`/`DeviceProfile`,
  `gamepad::PadTracker` (edges from snapshots, axis hysteresis, hats,
  timestamps), `storage`, `history::PlayStore`, `video` (`VideoDecoder`,
  `VideoBackend`, `MovieTrack`, `VideoGuard`), and `decode` (Ogg Vorbis
  through Symphonia, the web's fallback). The doc still names a
  `SongStore` trait that does not exist.
- **What the web does that desktop must match**, and where it lives:
  - the play session (`web/src/play.rs`: the `Player`, input sources,
    bindings, cancel gesture, autoplay, fail fade, `startFrame`/`endFrame`
    fades, the facts a play is recorded with), calibration
    (`web/src/calibration.rs`), the frame loop (`web/src/game_loop.rs`:
    refresh estimate, backgrounds, the video guard), movies
    (`web/src/movies.rs`), Dancing☆Onigiri layers (`web/src/stage.rs`),
    lyrics (still DOM);
  - the library (`web/src/songs.rs`, `web/src/import/mod.rs`,
    `web/src/noteskins.rs`), song select's data (`web/src/select_data.rs`),
    previews (`web/src/select_preview.rs`, `web/src/preview.rs`);
  - device profiles (`web/src/web/devices.rs`, `display.rs`, the device
    watcher), the history (`web/src/history.rs`).
  Most of the session and calibration code is platform-free already: it
  names `WebAudio`, `WebKeyboard`, `Gamepads`, `TouchLanes` and a few
  web-sys types, and little else.
- **The UI core's remaining web steps** matter here: options with the
  bindings and bind flow (6.3) and calibration's menu, credits and the
  play screen's DOM parts (6.4: the start prompt, the quit button,
  lyrics). Desktop play needs 6.4's prompt in the scene; until then
  desktop draws none and starts at once.

## Decisions proposed

- **D1. One play session for both shells.** The session, calibration,
  song select's data and the import's song building move out of `web/`
  into platform-free code (step 7.1), behind `ddi_platform` traits. A
  desktop copy of `play.rs` would drift within weeks; the move is checked
  by the web's existing headless runs, which must not change.
- **D2. Audio: our own small mixer on cpal, not web-audio-api.** The
  web's clock rests on `getOutputTimestamp()`: a pair of "this sample
  leaves the device at this host time". cpal hands exactly that to every
  callback (`OutputStreamTimestamp { callback, playback }`, on CoreAudio
  from `mHostTime`, on WASAPI from QPC), so a backend that counts the
  frames it wrote can publish the same pair. web-audio-api 1.7 hides it:
  its `current_time()` moves in render blocks on another thread and
  `output_latency()` is `playback − callback` with no host time beside it
  (`src/io/cpal.rs`), so the clock would be sampled back to back, with
  a buffer's worth of jitter. What a play needs of Web Audio is small:
  buffers started at a frame, gains with linear ramps, a master gain, a
  stop. Step 7.0 measures both before this is final.
- **D3. Decode everything natively, Opus included.** Symphonia (already
  a dependency) for MP3, Ogg Vorbis, FLAC and WAV; a pure-Rust Opus
  decoder for the bundled songs, all Opus; resampled once to the device's
  rate at decode, as `decodeAudioData` does. MP3s decode the way
  StepMania 5.1 does (see D9).
- **D4. Keys stamped on receipt, and with the OS's own time where there
  is one.** winit 0.30 and 0.31-beta give key events no time. On macOS
  an `NSEvent` local monitor reads each key's `timestamp` (seconds since
  boot, the clock Rust's `Instant` uses there), so a busy main thread
  costs nothing. Elsewhere the event is stamped when winit hands it over,
  and the main thread is kept from blocking on presentation (D6). SDL3
  stays the fallback named in `PLAN.md` if step 7.0 or players show
  frame-quantised stamps on Windows or Linux.
- **D5. Controllers through gilrs on a thread of their own, into the
  existing `PadTracker`.** The thread blocks in `next_event_blocking`,
  stamps each event, builds the pad's snapshot and runs the same tracker
  the web runs, so axes, hats, hysteresis and bindings behave the same.
  Standard pads (gilrs's SDL mappings) get the W3C standard button
  numbers, so the standard defaults apply unchanged; other pads (dance
  pads, adapters) number their raw controls and are bound with the bind
  flow, as decision 11 has it.
- **D6. Present without blocking input.** Mailbox where the surface has
  it (Vulkan, DX12: no tearing, no wait), Fifo elsewhere (Metal), with
  `desired_maximum_frame_latency = 1` during play and frames paced by
  `ControlFlow::WaitUntil`, so events are read while the loop waits
  rather than while it is stuck in `get_current_texture`. One surface,
  plain format with an sRGB view (the web's WebGPU arrangement): the
  painter draws in encoded values (P11) and the play field's `Renderer`
  in linear light with `SkinBlending::PlainView`, inside the painter's
  `External` slot.
- **D7. The library is folders on disk, read in place.** Songs folders
  chosen in Options (StepMania's `Songs/<pack>/<song>/` or a single
  pack), zips in them or picked read in place by the sans-IO zip reader,
  nothing copied; a manifest cache makes the start instant. "Import" on
  desktop adds a folder or a zip to the list.
- **D8. Video through the same module the web runs**, under wasmtime on
  a decoder thread, last (step 7.7). One decoder, the same frame times
  and seeks as the web, the LGPL part still a separate replaceable file,
  and no FFmpeg development libraries at build time. FFmpeg linked
  dynamically through `ffmpeg-next` stays a later option for speed.
- **D9. Fidelity details follow StepMania 5.1** (the maintainer's rule):
  MP3 decoder delay and the Xing/LAME frame as its `RageSoundReader_MP3`
  handles them, read before 7.2 and cited in the code; the songs folder
  layout, pack banners and shared movie folders as `SongManager` finds
  them (decisions 19 and 30 already do).
- **D10. Data folders by platform convention**, through `dirs` 7.0:
  settings in the config folder (as now), the play history and the song
  folders list in the data folder, the manifest cache in the cache
  folder. On macOS and Windows config and data are one folder; on Linux
  `plays/` moves from `~/.config/ditoditoinfinito/` to
  `~/.local/share/ditoditoinfinito/` before anything records there.
- **D11. A desktop build profile.** The workspace's `release` profile is
  tuned for wasm (`opt-level = "z"`, `panic = "abort"`). Desktop gets
  `[profile.desktop]` (inherits release, `opt-level = 3`,
  `panic = "unwind"`): decoders run on untrusted files, and the image
  loader's `catch_unwind` (`desktop/src/images.rs`) only works with
  unwinding.

## 1. Audio

### Candidates (crates.io, 2026-10-16)

| Crate | Version | Licence | Clock and latency | Platforms | Verdict |
|---|---|---|---|---|---|
| cpal | 0.18.2 (2026-08-16) | Apache-2.0 | per callback: `callback` and `playback` `StreamInstant`s (CoreAudio `mach_absolute_time`, WASAPI QPC, ALSA htstamp, PipeWire `pw_stream_get_time_n`, PulseAudio `Instant`); buffer size chosen with `BufferSize::Fixed` within the device's range | macOS, Windows (WASAPI; ASIO optional), Linux (ALSA always; PipeWire, PulseAudio optional) | **the base** (D2) |
| web-audio-api | 1.7.0 (2026-08-08) | MIT | `current_time()` per render block, `output_latency()` = playback − callback, no host time paired; `set_sink_id`, `onsinkchange`; decodes through Symphonia (no Opus) | on cpal (cubeb optional) | the fallback if 7.0 shows our mixer buys nothing |
| kira | 0.12.5 (2026-09-26) | MIT OR Apache-2.0 | clocks and sound positions are the mixer's, no output latency or host time | on cpal | no: hides the hardware clock |
| rodio | 0.23.0 (2026-10-11) | MIT OR Apache-2.0 | playback position only | on cpal | no |
| firewheel | 0.14.0 (2026-09-05) | MIT OR Apache-2.0 | a graph with its own clock; latency and timestamps undocumented (docs.rs: 0 % documented) | on cpal | no: an API still moving monthly |
| cubeb | 0.38.0 (2026-08-18) | ISC | Firefox's library: position and latency | C++ built with CMake | no: a C++ build for what cpal gives |
| sdl3 (audio) | 0.20.0 (2026-09-07) | MIT (SDL zlib) | no output latency | C build | no (kept for input, D4) |
| oddio, tinyaudio | 0.7.4 (2023), 2.0.0 | | stale / raw callback only | | no |

All are permissive; none is copyleft. On Linux, cpal always links ALSA
(`alsa` 0.11 → `alsa-sys`, found with pkg-config): `libasound2-dev` at
build time, `libasound.so.2` (LGPL-2.1-or-later, a system library,
linked dynamically) at run time. See the risks.

### The backend: `desktop/src/audio/`

- **Stream.** The default output device, `f32`, the device's own rate
  (no resampling in the callback), stereo (mono and more channels mixed
  down or up), `BufferSize::Fixed` near 5 ms (256 frames at 48 kHz)
  within `SupportedBufferSize`, else the default. A setting (automatic,
  small, medium, large) for devices that crackle.
- **Mixer** (`mixer.rs`, the callback): voices of decoded PCM
  (`Arc<[f32]>`, never copied), each started at a frame of the output
  timeline, with its own gain and a list of linear ramps scheduled on
  frames; a master gain with the same ramps and a 15 ms smoother for set
  volume (the web's `set_target_at_time(…, 0.015)`). Commands arrive
  through an `rtrb` 0.4 ring (MIT OR Apache-2.0, real-time safe); the
  callback never locks or allocates. Finished voices go back through a
  second ring to be dropped off the audio thread.
- **Clock.** Each callback publishes, through a sequence lock of atomics,
  the frame its buffer starts at and the host time that frame is heard:
  `playback` converted to host time. The conversion offset is the
  smallest `Instant::now() − callback` over the last second, so it holds
  whichever clock the host's `StreamInstant` uses. `now()` returns the
  web's `ClockSample` exactly: `context_time` the `currentTime`
  equivalent (heard position plus latency), `host_time` now,
  `output_latency` = `playback − callback`. The engine's `SongClock`
  then works unchanged (its drift filter absorbs the device clock's ppm
  error against the host's).
- **Host time.** One epoch for the process (`desktop/src/clock.rs`, an
  `Instant` taken at start): audio, winit, the pad thread and the history
  all stamp against it.
- **Device changes.** cpal's error callback (device gone) and a check of
  the default device every second in menus reopen the stream on the new
  default; the clock re-anchors (`SongClock`'s re-anchor after 8
  agreeing samples), and a play in progress is flagged on the results,
  as the web's device watcher does.
- **Fingerprint.** `audio:<host>:<device name>:<rate>:<latency bucket>`
  (latency in 5 ms buckets, as the web), labelled with the device's name,
  replacing the web's guess from latencies (`DeviceFingerprint`'s doc
  already asks native shells for the real id). Offsets are per profile as
  on the web; desktop profiles are their own.

### `ddi_platform::AudioBackend` grows

The trait gains what the session uses `WebAudio` for directly today, so
the session can be generic (D1): `sample_rate`, `set_master_volume`,
`master_ramp(to, at, seconds)` (the fail fade, `startFrame` fade-in,
`endFrame` stop), `stop_all`, `running`, `buffer_from_pcm` (calibration's
click track), `fingerprint`. Decoding stays outside the trait: it is async
on the web and on a worker thread on desktop, and the session is handed
buffers. `web/src/web/audio.rs` implements the additions with the calls
it already makes.

### Decoding: `platform/src/decode.rs`

- A new feature `decode-all` (desktop) beside `decode` (the web's Ogg
  Vorbis fallback, unchanged in size): Symphonia 0.6.1 (MPL-2.0,
  file-level copyleft, linked unmodified: fine for MIT, as now) with
  `mp3`, `ogg`, `vorbis`, `flac`, `wav`, `pcm`; and Opus.
- **Opus.** Pure-Rust decoders now exist: `opus-decoder` 0.1.1 (MIT OR
  Apache-2.0, "RFC 8251 conformant, no unsafe, no FFI"), `rusty-opus`
  1.0.1 and `opus-rs` 0.1.37 (both BSD-3-Clause). Chosen after 7.0's
  measurement: `rusty-opus`, fed packets by Symphonia's Ogg reader, with the
  `OpusHead` pre-skip and output gain applied; checked against libopus
  (the `ffmpeg` command's decode) on the four bundled songs within
  rounding (it clamps to ±1 where libopus's peaks reach 1.076: 7.2 takes
  its output before the clamp if the crate allows, or keeps the
  difference in mind for loud masters). Fallback: libopus through `opus` 0.4.0
  (MIT/Apache-2.0 bindings, libopus BSD-3-Clause, a C build). If it
  holds, the web could use it too (decision 9's "no pure-Rust Opus
  decoder" is no longer true); a separate question.
- **Resampling** to the device rate with `rubato` 5.0.1 (MIT OR
  Apache-2.0), once, on the decoder thread.
- **MP3** as StepMania 5.1 decodes it (D9): read `RageSoundReader_MP3.cpp`
  for the Xing/LAME frame and encoder delay, set Symphonia's gapless
  option to match, and measure the shift against Chrome's decode on local
  MP3s with and without an Info tag; the per-song offset stays the
  remedy for packs synced elsewhere.
- **Memory.** A whole song decoded is about 64 MB (3 minutes, stereo,
  f32); one song at a time plus the preview.

### Previews, volume, sounds

- **Previews** (`SelectCall::Selected`): the decoder thread seeks to
  `#SAMPLESTART` and decodes only `#SAMPLELENGTH` plus the fades (the web
  decodes the whole song, a known gap), then the mixer loops it with the
  web's 0.5 s fades; the beat pulse (`BeatFn`) reads the preview's heard
  position from the clock. No gesture rule on desktop: the preview starts
  as soon as it is decoded.
- **Volume:** `Settings::volume` on the master gain; the options' volume
  sample plays through the same path.
- **Developer screen's sound** (`UiRequest::StartAudio`): a short tone
  from `buffer_from_pcm`.

### Calibration

Unchanged in method (the generated 120 BPM chart with a click baked into
its audio, batches of 8, convergence at |mean| ≤ 2σ/√n, the ±250 ms
range, arrows only / sound only / combined): `web/src/calibration.rs`
moves into the shared session (step 7.1) and runs on the desktop backend.
The calibration autoplayer keeps following the physical cue (`AGENTS.md`).
A desktop device profile with no offsets prompts to calibrate at song
start, as on the web.

## 2. Input

### Keyboard

- **What winit gives:** `KeyEvent` without a time in 0.30.13 and in
  0.31.0-beta.3 (checked on docs.rs). The shell stamps with the process
  clock when the event reaches `window_event`; the delay is however long
  the main thread was busy.
- **macOS:** `NSEvent::addLocalMonitorForEventsMatchingMask:handler:`
  (`objc2` 0.6.5, MIT; `objc2-app-kit` 0.3.2, Zlib OR Apache-2.0 OR MIT)
  queues each key down and up with its `timestamp` and key code; the
  shell pairs winit's next `KeyboardInput` with it by physical key and
  uses that time. Seconds since boot are `mach_absolute_time`, the clock
  of Rust's `Instant` on macOS, so the conversion is exact.
- **Windows:** `GetMessageTime` has tick resolution (about 15.6 ms):
  not used. Stamped on receipt; the main thread does not block on
  presentation (D6). If that proves not enough, a Raw Input thread
  (`RIDEV_INPUTSINK` on a message-only window, `windows` 0.62, MIT OR
  Apache-2.0) receives keys whatever the main thread does.
- **Linux:** X11 and Wayland events carry millisecond server times that
  winit does not expose; stamped on receipt (D6). A playable 1 ms is
  what Firefox gives on the web.
- **Kept from the web:** physical codes for bindings (`keys::code`),
  repeats dropped, a pressed set against doubled reports, everything
  released on `Focused(false)`. The play's sources become `InputSource`s
  as on the web: `DesktopKeyboard` fed by the shell.

### Controllers

- **gilrs 0.11.2** (Apache-2.0/MIT), its own thread (`desktop/src/pads.rs`),
  `GilrsBuilder` with the default filters off (no deadzone or jitter
  filter: `PadTracker` does hysteresis), `next_event_blocking` with a
  short timeout. Each event becomes a `PadSnapshot` of that pad (slot,
  id, standard, timestamp, buttons, axes) for the pad's `PadTracker`;
  edges go to the main thread over a channel, which wakes the loop with
  an `EventLoopProxy` (P13).
- **Timestamps.** gilrs stamps a `SystemTime`: on Linux the kernel's
  evdev event time; on macOS the moment its IOKit callback runs on its
  own run-loop thread; on Windows the moment its Windows.Gaming.Input
  thread polls, every 8 ms (`EVENT_THREAD_SLEEP_TIME`; read in
  `gilrs-core`'s main branch on 2026-10-16, to confirm against 0.6.8). The thread converts a `SystemTime` to host time through
  a pair `(SystemTime::now(), Instant::now())` read with each event, so
  wall-clock steps never reach a stamp. `PadTracker`'s plausibility rule
  (decision 12) stays: a stamp not after the previous one, or in the
  future, falls back to the receive time; the split is counted for the
  debug overlay.
- **Windows' 8 ms polling** is the weak point (up to 8 ms late, 4 ms on
  average). Proposed: a merge request to gilrs making the interval
  configurable and stamping from the reading's own `Timestamp`; until
  then Windows pads play with it, shown in the debug overlay. SDL3's
  gamepad subsystem is the fallback.
- **Ids and controls** (D5): the binding id is
  `<name> (Vendor: vvvv Product: pppp)` from gilrs's vendor and product
  ids, desktop's own (decision 11: each platform keeps its own).
  Standard pads map gilrs's `Button`s to the W3C numbers (South 0, East
  1, West 2, North 3, LeftTrigger 4, RightTrigger 5, LeftTrigger2 6,
  RightTrigger2 7, Select 8, Start 9, LeftThumb 10, RightThumb 11, DPad
  12–15, Mode 16) and axes likewise, so `ControlBindings`' standard
  defaults and menu input work unchanged. Other pads number raw button
  codes in ascending order (stable per device) and raw axes the same;
  hats arrive as two axes and bind as `axis:N±`.
- **Dance pads:** as on the web, any HID joystick works through the bind
  flow; keyboard-emulating pads through the keyboard. On Linux the
  device node must be readable: udev grants it to joysticks
  (`ID_INPUT_JOYSTICK`), and a pad udev does not classify needs a rule;
  the options page says so when no pad is seen. Not testable here (no
  pad); built from documented HID behaviour and checked with the
  standard controller and a scripted fake pad.
- **Menus:** pads drive the screens through `ControlBindings::menu_input`,
  as on the web; a pad press is a gesture on desktop (no browser rule).

## 3. Play on desktop

### The frame loop (`desktop/src/play.rs`)

- **Menus** stay on demand (`ControlFlow::Wait`, P13). **Play** runs
  continuously: each frame reads the clock, drains input, updates the
  session, builds the `Frame` at the predicted presentation time and
  draws.
- **Present modes** (D6): Mailbox if offered, else Fifo (`AutoVsync`);
  `desired_maximum_frame_latency` 1 in play, 2 in menus. Frames are paced
  with `WaitUntil(next_vblank − margin)`, the refresh rate from winit's
  `MonitorHandle::refresh_rate_millihertz` checked against frames
  measured (the web's `FpsMeter`). An option "lowest latency (may tear)"
  uses Immediate.
- **Predicted presentation** = the frame's start plus the frame interval
  times the frames in flight; wgpu gives no present timestamps, and the
  visual offset (calibrated per display profile) absorbs the constant.
  Measured in 7.0: the time spent in `get_current_texture` and `present`,
  per present mode.
- **One surface, two views** (D6): configured plain (`Bgra8Unorm`) with
  `view_formats = [Bgra8UnormSrgb]`. The play field is the `External`
  slot of the 2D pass, drawn by `Renderer` into the sRGB view with skins
  into the plain view (`SkinBlending::PlainView`), as on WebGPU. A
  Vulkan driver without mutable swapchain formats falls back to
  `SkinBlending::Offscreen`, the WebGL2 path, already built.
- **The display profile:** `display:<monitor name>:<w>x<h>@<scale>`
  (winit's `MonitorHandle`), refresh rate excluded (`AGENTS.md`);
  `ScaleFactorChanged` and a move to another monitor are a device change.
- **Judging** does not depend on the frame rate (`docs/plans/play-history.md`,
  step 1), so frame pacing only moves stamps and drawing. Edges must
  reach the engine within `JUDGE_LAG` (30 ms) or they count as
  `late_edges`, published in the debug state.

### What a play draws and plays

- **Noteskins with their Lua:** `ddi-noteskin`, `lunita` and
  `ddi_render::noteskin` run natively today (the local skin tests play
  345 skins). Desktop adds the files: bundled skins from `assets/noteskins/`
  through the generated index, imported ones from `NoteSkins/<game>/<name>/`
  beside a songs folder (StepMania's layout), images decoded by the
  shell. **SVG** (Dito, the default skin, is SVG; decision 38): rasterised
  with `resvg` 0.48.1 (Apache-2.0 OR MIT) at four times its size, the
  subset `noteskin/src/svg.rs` checks being the one browsers and resvg
  draw alike. The image loader gains GIF, BMP and WebP (`image`
  features), the formats `pack::IMAGE_EXTENSIONS` lists.
- **Backgrounds:** the still images and `#BGCHANGES` schedule
  (`ddi_library::backgrounds`), decoded by the shell into RGBA as
  decision 16 foresaw; the background frame and the blur as on the web.
- **Dancing☆Onigiri layers** (`web/src/stage.rs`'s timeline moves to
  the session) and lyrics (drawn in the scene once 6.4 moves them there;
  cosmic-text draws Japanese with system fonts).
- **Results** are the theme's screen (`ddi_ui::screens::results`), from
  the engine's `Results`; the per-song offset editor writes the settings.
- **The play history:** recorded when a song ends (finished or failed),
  through `FilePlayStore`, with the facts the web records
  (`PlayFacts`: devices, clock figures, settings), re-worked when stale
  as on the web. The autoplay flag as on the web.
- **Start and quit:** without 6.4's prompt, a play starts when its music
  is decoded; Escape or Back held or pressed twice quits, as on the web.

### Video (D8)

- **Options weighed:**
  - *The web's module under wasmtime* (`wasmtime` 49.0.3,
    `wasmtime-wasi` 49.0.3, Apache-2.0 WITH LLVM-exception). The module
    is a WASI reactor importing 20 calls (`web/src/web/video_worker.js`
    implements them in JavaScript); wasmtime-wasi or our own 20 calls
    over a file give it the same. Same decoder, frame times and seeks as
    the web's software path; LGPL compliance unchanged (a separate file,
    replaceable, with its notices); no C toolchain or FFmpeg libraries to
    build the game. Costs: a large dependency (compile time, several MB
    of binary) and wasm speed: the research's native single-thread
    figures (1.35 ms a frame for H.264 852×480, 1.86 ms at 720p) at the
    planning factor of 1.5–2 for Cranelift stay well inside a 33 ms frame
    on a thread of its own.
  - *FFmpeg linked natively* (`ffmpeg-next` 9.0.0, WTFPL, over
    `ffmpeg-sys-next` 9.0.1): fastest, hardware decoders possible, but
    FFmpeg's development libraries at build time on every platform,
    shipped DLLs on Windows (dynamic linking for LGPL), and a second
    decoder path to keep in step with the shim's seeking rules.
  - *Platform decoders* (VideoToolbox, Media Foundation, VA-API): H.264
    only, not Xvid or MPEG-2; no.
- **Proposed:** wasmtime, behind a cargo feature `video` (on by default
  for `cargo run`), implementing `VideoBackend`/`VideoDecoder`
  (`desktop/src/video.rs`) on one decoder thread per movie, the module
  looked for beside the executable, then in `target/video/` (dev), then
  `DDI_VIDEO_MODULE`. Without it, movies show the song background with
  the results' one line, as a web build without the module does. Planes
  go to the renderer's YUV path, shared with the web.
- **It can come later:** every song plays without it. Step 7.7, after
  play.

## 4. Library

### Folders (D7)

- **Settings:** `song_folders` (desktop only, under its own storage key
  so the web's settings object is untouched), edited on the Options page
  ("add folder", "remove", "scan again"); `--songs <dir>` adds one for a
  run. First start with none: song select shows the bundled songs and
  offers "add your songs folder", with StepMania's, ITGmania's and
  OutFox's usual locations offered when they exist
  (`~/Library/Application Support/StepMania 5.1/`, `%APPDATA%\ITGmania\`,
  `~/.itgmania/` and the like), never scanned without the player's
  choice.
- **What a folder may be:** a StepMania root (it has `Songs/`; then its
  `NoteSkins/`, `RandomMovies/` and `SongMovies/` are used too), a
  `Songs` folder (children are packs), or one pack. The walk lists files
  relative to the folder, StepMania's depth (pack/song/files, plus pack
  banners at the pack's root), skipping what `pack::is_ignored` skips,
  and hands the listing to `pack::find_songs`.
- **Zips** found in a songs folder or added by "import" are read in place
  (`ddi_library::zip`, a file and seeks), each a pack, as the web links a
  zip's entries by name (decision 33).
- **Ids:** `pack::song_id(pack, dir)`, as the web: the same pack has the
  same ids on both, so per-song offsets and history entries match by id
  and digest. Two folders holding the same pack: the first in the list
  wins, the other is listed in the import report.
- **Shift-JIS:** `pack::decode_text_with` with `encoding_rs` (already a
  dependency) where the web uses `TextDecoder`.

### Scanning and the cache

- **Shared song building.** `web/src/import/mod.rs::import_song`'s
  platform-free part (parse, playable layouts, `resolve_assets`,
  `classify_images` from image headers, missing-file warnings and notes,
  `summarize`, the chart digests) moves to `library/src/scan.rs` behind a
  small reader trait (list, read a range, read whole), used by the web's
  import, `xtask gen-songs` and desktop. The report (`ImportReport`)
  moves with it.
- **Cache:** `<cache folder>/library-v1.json`: per song folder, the
  simfile's path, size and modification time, a hash of the folder's
  listing, the `ManifestEntry` with its resolved file paths and digests.
  At start the list shows from the cache at once; a scan thread checks
  every folder (a directory listing and a `stat` per simfile), parses
  only what changed and feeds song select through its `Feed`
  (`ddi_ui::screens::select::FEED`). Deleting the cache is always safe.
- **On demand, from disk:** the chart is parsed again when a song is
  played or previewed (milliseconds); music, banners, backgrounds and
  movies are read from their paths; nothing is copied. Banners go
  through the existing image loader (absolute paths already allowed,
  `desktop/src/images.rs`).
- **To measure** (7.3): a full scan and a cached start of the local
  library (`~/Music/Simfiles`, about 1,200 simfiles, one-off and local
  only).

### Song select's data, the history, removal

- `web/src/select_data.rs` (packs, styles, charts, bests from the
  history) moves into `ui::state` (D1), fed by either shell's library.
- The history stays two files a play (`desktop/src/plays.rs`), in the data
  folder (D10); bests by chart digest work across both shells' ids.
- "Remove a pack" on desktop removes it from the list (the folder is
  hidden), never deletes files. "Read again" rescans the folder.

## 5. Platform services

- **File dialogs:** rfd 0.17.2 (MIT), on Linux through the XDG desktop
  portal (no GTK). Where no portal answers (a bare window manager), the
  folder setting takes a typed path in the UI core's text field.
  Dropping folders or zips on the window (winit's `DroppedFile`; not on
  Wayland) adds them, as the web's drop imports.
- **Clipboard:** arboard 3.7.0 (MIT OR Apache-2.0; bump from 3.6.1).
- **Links:** `desktop/src/os.rs` as now (`open`, `rundll32`, `xdg-open`).
- **IME:** done in step 3 (own mode, preedit and commit, the caret area).
  Windows (`WM_IME_*`) and Linux (X11 XIM through ibus or fcitx,
  Wayland's text-input-v3) are checked by hand only (section 6).
- **Fonts:** cosmic-text 0.19 with the bundled faces and the system's
  fonts (fontdb scans the font folders; on Linux it reads fontconfig's
  files without the library). Japanese falls back to the system's CJK
  fonts (Hiragino, Yu Gothic or Meiryo, Noto CJK when installed); a Linux
  without a CJK font shows boxes, said once in the import report. The
  time fontdb takes to load a system's fonts is measured at start.
- **Folders** (D10): `dirs` 7.0.0 (MIT OR Apache-2.0) for config, data
  and cache, replacing the hand-written lookup in `store.rs`.
- **Assets:** bundled songs, noteskins, fonts and the video module are
  read from `DDI_ASSETS`, then beside the executable, then the
  repository's `assets/` (as `images.rs` does today). Where a packaged
  app keeps them is a packaging question (section 8).
- **Debug state:** the web's `window.__DDI_DEBUG` becomes
  `--debug-json <file>` (written every second and at exit) and the debug
  overlay, with the same fields where they mean the same (clock drift,
  latency, stamps by source, late edges, frames, the history counts).
- **Command line**, mirroring the URL parameters for tests:
  `--play <song id> [--chart n] [--auto[=pad]] [--bias ms]`,
  `--calibrate visual|audio|combined`, `--songs <dir>`,
  `--backend metal|vulkan|dx12|gl`, `--seconds N`, `--debug-json`.

## 6. Testing

- **Natively, in CI (Linux):**
  - the shared session (7.1) with a fake audio backend whose clock the
    test drives (drift, jitter, latency steps) and scripted keyboard and
    pad edges: autoplay and `auto=pad` plays of the bundled songs and the
    replay fixtures all top tier, judged again from their logs to the
    same results; calibration converging on a simulated device with a
    known latency;
  - the mixer's arithmetic (voices started on a frame, ramps, master
    smoothing, loop points) and the clock's anchor maths, with a
    simulated callback stream;
  - decoders against references: the bundled Opus songs, generated MP3,
    Vorbis, FLAC and WAV clips (the CI's `ffmpeg` job makes them and
    decodes them for comparison, as `video-check` does);
  - the library scan on a generated songs tree (packs, zips, Shift-JIS,
    missing files, shared movie folders) against the web import's
    reports for the same files; the cache's invalidation;
  - `PadTracker` fed with recorded gilrs event streams (standard and
    unmapped pads, hats as axes);
  - offscreen GPU tests (`desktop/tests/*_offscreen.rs`, skipped without
    an adapter): a play's frame with the built-in arrows, Dito and a Lua
    skin, matched against the web's pixels within 2 levels, as the
    developer screen is; results and options on real data.
- **CI on Linux** installs `libasound2-dev` and `libudev-dev` (cpal and
  gilrs need them at build time; see the risks); everything else keeps
  loading its libraries at run time. CI has no sound device: the backend
  opens none and the session tests use the fake one.
- **CI on macOS and Windows** (GitHub's `macos-latest`, `windows-latest`):
  build and `cargo test -p ddi-desktop` on each, the offscreen tests
  where an adapter exists (WARP on Windows). This is the only Windows
  check available; Windows-only code paths (the WGI pad thread, WASAPI,
  IME) are kept small and covered by unit tests of their pure parts.
- **Linux locally:** `act` runs the CI jobs, `container` reproduces the
  Ubuntu build. Neither has a display, audio or a pad: for the window,
  sound, IME and controller on Linux, a Linux VM on the Mac (UTM) is the
  way, if the maintainer wants it.
- **By the maintainer, on the Mac:**
  - 7.0's measurements (they need speakers, the built-in microphone for
    the loopback and the controller);
  - a play by keyboard and by the controller, against the web on the
    same Mac in Firefox: the calibrated offsets should agree within a few
    milliseconds once the latency figures are subtracted;
  - Bluetooth headphones (the latency reported, the calibration);
  - IME on macOS (done for step 3's window; again on song select's
    search);
  - a song folder of their own and the history's bests.
- **Not checkable here:** Windows (only CI builds and tests), dance pads
  (no pad), Linux audio latency on real hardware. Each is said in the
  step that ships it.

## 7. Steps

Each step is its own commit or a few, checked natively, in CI, and by
the maintainer on the Mac where it says. Steps 7.2–7.4 can go in parallel
with the UI core's 6.3 and 6.4; 7.5 needs 6.3's options, and the start
prompt from 6.4.

### 7.0 Measure (scratch, not committed)

Spikes in a scratch crate outside the repository (path dependencies on
the workspace, its own target folder), like ui-core's step 0:

1. **The audio clock on macOS:** cpal with our anchor against
   web-audio-api's `current_time()` sampled back to back, 60 s each at
   the default and at a 256-frame buffer: the jitter of
   `context_time − host_time` around a fitted line, the drift between
   the device and `Instant` (ppm), the reported latency.
2. **Latency by loopback** (with the maintainer, not unattended: it
   records the room and macOS asks for the microphone): a click played
   and recorded through the built-in microphone, its onset found; the
   true output plus input latency against what cpal reports (built-in
   speakers, wired headphones, Bluetooth). The answer decides whether
   calibration's default offset needs a platform constant; until then,
   calibration on the real gameplay path carries it, as on the web.
3. **Key stamps:** winit's receipt time against `NSEvent.timestamp`
   while drawing with Fifo at 60 and 120 Hz and with an artificial 10 ms
   frame: how late receipt is, and whether the monitor's pairing holds.
4. **The controller:** gilrs on macOS with the maintainer's controller:
   events a second, the stamp against receipt, buttons and axes as gilrs
   maps them.
5. **Opus:** `opus-decoder` against `ffmpeg`'s libopus decode of the four
   bundled songs (sample difference, pre-skip, speed).
6. **The module under wasmtime:** frames a second for H.264, Xvid and
   MPEG-2 local movies, against the browser's figures.

Result: a short report in this plan; D2, D4 and D8 confirmed or changed
before the steps that build on them.

#### Results (2026-10-16)

Unattended items 1, 5 and 6 on an M1 Max laptop (macOS, Rust 1.99),
scratch crate outside the repository, digital silence only. Items 2–4
wait for the maintainer (no controller was connected: gilrs saw none).

**1. The audio clock.** cpal 0.18.2 and web-audio-api 1.7.0, 60 s each,
the clock sampled about every 1.6 ms. The default device was the
**headphone jack** ("External Headphones", 44.1 kHz; nothing plugged in
is needed for the clock); the built-in speakers (48 kHz) were opened by
name for a second set. Jitter is the residual of `context_time −
host_time` around a fitted line; drift is the device against `Instant`.

| Path | Buffer | Jitter sd / max | Drift | Reported latency |
|---|---|---|---|---|
| cpal + anchor, jack | default (512) | 14 µs / 63 µs | +2.4 ppm | 14.69 ms |
| cpal + anchor, jack | 256 | 14 µs / 47 µs | +2.5 ppm | 8.89 ms |
| cpal + anchor, speakers | default (512) | 5 µs / 14 µs | +2.1 ppm | 13.67 ms |
| cpal + anchor, speakers | 256 | 2 µs / 7 µs | +1.8 ppm | 8.33 ms |
| web-audio-api `current_time()`, jack | default (`Interactive`: 128) | 0.86 ms / 1.5 ms | +2.3 ppm | 5.99 ms |
| web-audio-api `current_time()`, jack | 256 | 1.69 ms / 2.9 ms | +1.5 ppm | 8.89 ms |

- cpal's stamps (CoreAudio's `mHostTime`) are smooth to microseconds;
  `current_time()` moves in 128-frame steps (2.9 ms) and its sawtooth is
  a hundred times wider. Callbacks came at a steady 512 or 256 frames.
- **cpal's `playback` on CoreAudio over-counts.** Read in
  `host/coreaudio/macos/device.rs` and measured: `callback` is the render
  timestamp's `mHostTime`, which is not the call time but the time the
  buffer's first frame reaches the device, already a buffer plus the
  safety offset ahead of the call (measured 7.38 ms at 256 frames =
  256 + 69 frames; 13.14 ms at 512). `playback` then adds the buffer,
  the device latency and the safety offset again (`playback − callback`
  = 256 + 66 + 70 frames on the jack), and leaves out the stream latency
  (24 frames). By Apple's model the first frame is heard at `mHostTime`
  + device latency + stream latency (2.0 ms on the jack), so cpal's
  figure is 6.9 ms too late at 256 frames and 12.6 ms at 512.
  web-audio-api's `output_latency()` inherits the same figure.
- The plan's anchor (`Instant::now() − callback`, smallest over a second)
  happens to cancel that over-count on macOS, landing 0.5 ms early (the
  missing stream latency). Proposed instead for 7.2: on CoreAudio,
  `StreamInstant` is `mach_absolute_time` like `Instant`, so use
  `callback` as host time directly and add the device and stream
  latencies read from the HAL; keep the anchor for hosts whose
  `StreamInstant` has another epoch, and check WASAPI's and ALSA's
  meaning of `callback` the same way before trusting `playback` there.
  The loopback (item 2) settles what is left.

**5. Opus.** The four bundled songs (all stereo, pre-skip 312, gain 0),
against `ffmpeg` 9.0.2's libopus decode in float. `ffmpeg -c:a libopus`
decodes to 16-bit unless given `-request_sample_fmt flt`; the reference
job in section 6 must pass it.

| Decoder | Against libopus (float) | Speed (per song) |
|---|---|---|
| libopus 1.6.1 through `opus` 0.4.0 | identical (RMS 3e-8) | 450–500× real time (0.22–0.33 s) |
| `rusty-opus` 1.0.1 | within float rounding (≥ 135 dB SNR, max 0.0 LSB16) once libopus is clamped to ±1 | 280–335× (0.33–0.48 s) |
| `opus-rs` 0.1.37 | the same output as `rusty-opus`, sample for sample | 290–335× |
| `opus-decoder` 0.1.1 | 16-bit output (RMS 0.3 LSB16, 88 dB), clamped | 5–7× (16–30 s) |

- Pre-skip and length: every decoder aligns at lag 0 with 312 frames
  dropped; we end at last granule − pre-skip (RFC 7845), `ffmpeg` 312
  frames (6.5 ms) earlier.
- The Rust decoders clamp to ±1. libopus's float output overshoots on
  loud passages (peaks 1.076 in Beatucada, 1.069 in Some Things Must,
  1.006 in Into My Dream; 106, 31 and 1 samples), the only place they
  differ. At full volume the device clips the same; at lower volume the
  browsers' unclamped decode keeps those peaks. Small; noted.
- `opus-decoder` is rejected: it decodes through 16-bit and spends most
  of its time in `sincos` (twiddles recomputed on every FFT).
- **Proposed for D3:** `rusty-opus` 1.0.1 (BSD-3-Clause, no
  dependencies, bit-exact on the RFC vectors by its tests, `unsafe` only
  in SIMD kernels with scalar twins), `opus-rs` (BSD-3-Clause, identical
  output, older and more used) as the alternative. No C build:
  libopus through `opus` would need CMake and a C compiler
  (`opusic-sys`, bundled). BSD-3-Clause adds a notice to the credits.

**6. The module under wasmtime.** wasmtime 49.0.3 with `wasmtime-wasi`'s
preview 1 (the module needs nothing else), planes copied out of wasm
memory each frame. Node 26 on the same files and machine for comparison
(`video/check.mjs`; browsers measured about the same as Node before).
Generated clips are 20 s of `testsrc2`; the rest are local pack movies.

| Movie | wasmtime ms/frame (p99, max) | Node ms/frame |
|---|---|---|
| Xvid 640×360 (clip) | 0.43 (1.2, 1.6) | 0.38 |
| MPEG-2 640×360 (clip) | 0.50 (2.2, 5.4) | 0.38 |
| DivX 3 640×480 (Bad Apple, 6571 frames) | 0.45 (1.6, 6.1) | 0.36 |
| H.264 640×360 (clip) | 1.04 (1.5, 3.9) | 0.92 |
| H.264 640×360 AVI (pack, 3524 frames) | 1.41 (3.6, 20) | 1.24 |
| H.264 854×480 (clip) | 1.97 (4.7, 11) | 1.55 |
| H.264 1280×720 (clip) | 4.04 (7.0, 12) | 3.53 |
| H.264 1280×720 MP4 (pack) | 6.67 (22, 36) | 5.93 |
| H.264 1920×1080 MP4 (pack, 25 fps) | 14.5 (37, 55) | 12.6 |

- wasmtime runs at 1.1–1.3× Node's time, inside a 33 ms frame for
  everything packs ship below 1080p; 1080p H.264 averages 14.5 ms but
  peaks past a frame, which a decoder thread a few frames ahead absorbs.
- Compiling the module with Cranelift takes 0.8–1.1 s (once per run, on
  the decoder thread, or cached with `Module::serialize`); instantiating
  0.5 ms, opening a movie 3–21 ms, wasm memory 8–26 MB.
- Cost: a clean release build of wasmtime and wasmtime-wasi (default
  features) takes 84 s on 10 cores (460 s of CPU) and adds 13 MB to a
  stripped binary; trimming features is for 7.7.

**Claims checked.**
- gilrs-core 0.6.8 (gilrs 0.11.2): Windows.Gaming.Input is polled every
  8 ms (`EVENT_THREAD_SLEEP_TIME`, XInput 10 ms), and each event is
  stamped `SystemTime::now()` at the poll; the reading's `Timestamp` only
  skips unchanged readings. On macOS events are stamped in the IOKit
  callback. Confirmed.
- winit 0.30.13 and 0.31.0-beta.3 (`winit-core`'s `KeyEvent`: key, text,
  location, state, repeat, `text_with_all_modifiers`): no time. Confirmed.
- `NSEvent.timestamp`'s clock (`NSProcessInfo.systemUptime`) read
  against `CLOCK_UPTIME_RAW`, Rust's `Instant` on macOS: the same clock
  (`CLOCK_MONOTONIC_RAW`, which counts sleep, was 5 hours ahead).
  Confirmed.

**Decisions.**
- **D2 confirmed:** our mixer on cpal. Its stamps are about a hundred
  times steadier than web-audio-api's `current_time()`, and only our own
  backend can correct cpal's CoreAudio latency (7.2 does it as above);
  web-audio-api inherits the error and hides the stamp.
- **D4 confirmed:** winit still gives no key time, and the `NSEvent`
  timestamp is on `Instant`'s clock, so the monitor converts exactly.
  How late receipt is (item 3) waits for the maintainer.
- **D8 confirmed:** the module under wasmtime decodes at 1.1–1.3× the
  web's time per frame, well within budget below 1080p; build time and
  binary size are the cost, behind the `video` feature as planned.
- **D3 changed (crate only):** `rusty-opus` instead of `opus-decoder`,
  which is too slow and decodes through 16-bit. Decision 9's question
  for the web (a pure-Rust Opus decoder now matches libopus) stays open.

### 7.1 One session for both shells

- New crate `session/` (`ddi-session`, platform-free, natively tested):
  `PlaySession` and `SessionConfig` from `web/src/play.rs`, generic over
  `AudioBackend` and fed `RawInput`s; `Calibrator` and the click track
  from `web/src/calibration.rs`; the backgrounds schedule and video guard
  glue from `web/src/game_loop.rs`; the layers' timeline from
  `web/src/stage.rs`; the `PlayFacts` a play is recorded with.
  Dependencies: `engine`, `chart`, `library`, `platform`, `ui` (settings).
- `ddi_platform::AudioBackend` extended (section 1); `WebAudio`
  implements it; `SongStore` dropped from the docs.
- `web/src/select_data.rs` → `ui::state::select_data`;
  `import_song`'s core → `library/src/scan.rs` (section 4).
- Checked: every web run that exists, unchanged (autoplay all top tier on
  WebGPU and WebGL2, `auto=pad`, calibration's automatic runs, plays
  recorded with the same facts, imports with the same reports), plus the
  session's native tests (section 6). Wasm size reported.

### 7.2 Audio on desktop

- `desktop/src/audio/` (stream, mixer, clock, decoder thread),
  `desktop/src/clock.rs`, `decode-all` in `platform`, `[profile.desktop]`
  (D11).
- Song select on sample data plays previews; the developer screen's
  sound; the options' volume sample.
- Checked: the mixer and clock tests; the decoders against references;
  by the maintainer: previews start promptly, no clicks at the fades,
  the device changed while playing (headphones unplugged) keeps sound.

### 7.3 The library on disk

- Song folders in the settings, the scan thread and cache, zips in place,
  bundled songs from `assets/`, noteskins from `NoteSkins/` and the
  bundled index, the history in the data folder (`dirs`).
- Song select on the real library (`Feed`), with bests; previews from
  disk; Options lists the folders.
- Checked: the scan tests; the local library scanned and cached (times
  reported); by the maintainer: their folder added, a pack zip, packs
  removed and read again.

### 7.4 Input on desktop

- `DesktopKeyboard` with receipt stamps and the macOS `NSEvent` monitor;
  `desktop/src/pads.rs` (gilrs thread, `PadTracker`, ids and standard
  numbering); menu input by pad; the bind flow and bindings on 6.3's
  options screen.
- Checked: the tracker tests with recorded streams; a fake pad source
  (`--auto=pad`); by the maintainer: song select and options by
  controller, binding the controller's buttons.

### 7.5 Play

- `desktop/src/play.rs`: the session on the desktop backend and sources,
  the play field in the painter's `External` slot, the present modes and
  pacing (D6), noteskins with Lua and SVG, backgrounds, layers, results,
  the history recorded, `--play`/`--auto`.
- Checked: offscreen frames against the web's; autoplay of every bundled
  song to its results all top tier in CI (fake audio) and on the Mac
  (real audio); `--auto=pad`; the maintainer plays by keyboard and
  controller, and compares with Firefox on the same Mac.

### 7.6 Calibration and device profiles

- Calibration's three tests on desktop (6.4's menu), audio and display
  fingerprints from device names (section 1, section 3), the prompt for
  an uncalibrated profile, device changes flagged on the results.
- Checked: the automatic calibration runs (`--calibrate … --auto`)
  converge to the simulated and the real offsets; by the maintainer: a
  calibration on speakers and on Bluetooth headphones, kept per device.

### 7.7 Video

- `desktop/src/video.rs` on wasmtime (feature `video`), the module found
  as section 3 says, the YUV path, the automatic setting's guard.
- Checked: the module's frames against the web's on generated clips
  (`video-check`'s references), a local pack with movies played all top
  tier with no frame late; a build without the module.

### 7.8 Windows and Linux

- CI jobs on `macos-latest` and `windows-latest`; Linux's build-time
  libraries; the gilrs merge request (Windows polling); platform paths
  checked (`%APPDATA%`, XDG).
- Checked: CI green on all three; Linux by `act`; by hand where hardware
  allows (section 6).

### 7.9 Docs

- `PLAN.md` (phase 9's row, decisions for D1–D11 as settled, section 3.1
  with `session/`, the crate versions line), `AGENTS.md` (layout:
  `session/`, desktop's modules; commands: `cargo run -- --play …`, the
  desktop profile, the CI libraries), this plan's results under each step.

## 8. Dependencies

Added or changed, all compatible with MIT distribution:

| Crate | Version | Licence | Where | Note |
|---|---|---|---|---|
| cpal | 0.18.2 | Apache-2.0 | desktop | links ALSA on Linux (below) |
| rtrb | 0.4.0 | MIT OR Apache-2.0 | desktop | the mixer's command rings |
| symphonia | 0.6.1 | MPL-2.0 | platform (`decode-all`) | file-level copyleft, unmodified: already accepted |
| rusty-opus | 1.0.1 | BSD-3-Clause | platform (`decode-all`) | chosen by 7.0 (5); else `opus-rs` 0.1.37 (BSD-3-Clause), or `opus` 0.4.0 over libopus |
| rubato | 5.0.1 | MIT OR Apache-2.0 | desktop | resampling at decode |
| gilrs | 0.11.2 | Apache-2.0/MIT | desktop | links libudev on Linux (below) |
| objc2, objc2-app-kit | 0.6.5, 0.3.2 | MIT; Zlib OR Apache-2.0 OR MIT | desktop (macOS) | the `NSEvent` monitor |
| resvg | 0.48.1 | Apache-2.0 OR MIT | desktop | SVG skins; tiny-skia under it is BSD-3-Clause |
| image | 0.25.10 | MIT OR Apache-2.0 | desktop | + `gif`, `bmp`, `webp` features |
| dirs | 7.0.0 | MIT OR Apache-2.0 | desktop | config, data, cache folders |
| arboard | 3.6.1 → 3.7.0 | MIT OR Apache-2.0 | desktop | |
| wasmtime, wasmtime-wasi | 49.0.3 | Apache-2.0 WITH LLVM-exception | desktop (`video`) | 7.7 |
| winit | 0.30.13 | Apache-2.0 | desktop | 0.31 is still beta (0.31.0-beta.3) |

System libraries, linked dynamically, flagged as copyleft: ALSA's
`libasound` and `libudev` (systemd) are LGPL-2.1-or-later; dynamic
linking against the system's copy keeps the game MIT. The FFmpeg module
stays LGPL-2.1-or-later, a separate file (decision 35). Not added:
web-audio-api, kira, rodio, firewheel, cubeb, SDL3 (fallbacks named
above).

A distributed desktop build has to carry its Rust dependencies' notices
(Apache-2.0's NOTICE files, BSD's and MIT's texts); `cargo-about` or the
credits generator can list them. That belongs to packaging (section 9).

## 9. Risks and open questions

Each with a recommendation; those the lead should decide are marked.

- **web-audio-api was the plan; this proposes cpal directly** (D2).
  *Decided below ("Decided here").* Recommendation: our mixer on cpal, since it gives the
  web's output-timestamp pairing and the API surface we use is small;
  7.0 measures both, and web-audio-api remains a drop-in fallback behind
  the same trait.
- **What cpal's `playback` instant means differs by host.** On CoreAudio
  it is `mHostTime` plus buffer, device latency and safety offset (cpal's
  source); stream latency and Bluetooth may be missing. Recommendation:
  trust it as the web trusts `outputLatency`, measure the gap by
  loopback (7.0), and let per-device calibration carry the rest, which is
  what it is for.
- **Linux needs `libasound2-dev` and `libudev-dev` at build time**,
  breaking "builds with no system libraries" (cpal's `alsa` is not
  optional; gilrs-core's `libudev-sys` uses pkg-config). *Decided below ("Decided here").*
  Recommendation: accept both (two apt packages in CI; both libraries are
  on every desktop Linux), rather than a desktop without sound or pads
  in CI or a fork that loads them at run time.
- **Windows pads are polled every 8 ms by gilrs.** Recommendation: the
  upstream change (configurable interval, the reading's timestamp);
  SDL3's gamepads if it does not land; say so in the debug overlay
  meanwhile.
- **Key stamps on Windows and Linux are receipt times.** Recommendation:
  never block the main thread on presentation (D6) and measure; a Raw
  Input thread on Windows, or SDL3, only if players or measurements show
  frame-sized errors.
- **MP3 decoding may differ from the browsers'** by the encoder delay
  (about 25 ms) where an Info tag is present. *Decided below ("Decided here"); whether
  the web should follow is a separate question.* Recommendation: desktop decodes as StepMania 5.1
  does (D9, the maintainer's fidelity rule); whether the web should
  switch MP3s to the same decode is a separate question, answered by
  7.2's measurement of how many local MP3s differ.
- **Pure-Rust Opus decoders are young.** 7.0 settled it: `rusty-opus`
  matches libopus within float rounding at about 300× real time
  (`opus-decoder` was rejected: 16-bit output, 5–7× real time). This
  reopens decision 9 for the web, a separate question.
- **wasmtime is large** for one feature. *Decided below ("Decided here").* Recommendation:
  take it behind the `video` feature for the same decoder everywhere;
  revisit with `ffmpeg-next` if 7.0's speed figures or the binary size
  disappoint.
- **The release profile is wasm's** (`opt-level = "z"`,
  `panic = "abort"`), so `cargo run --release` builds a slow desktop that
  aborts on a decoder panic. Recommendation: `[profile.desktop]` (D11),
  and AGENTS.md saying `cargo run --profile desktop`.
- **Desktop and web profiles and bindings are separate.** Offsets are
  per device fingerprint and pad bindings per id, both platform-specific
  (decision 11 already keeps browsers apart). Recommendation: keep them
  apart; settings themselves (speed, rules, skin, colours) are the same
  JSON and could be exported and imported between the two later.
- **Where a packaged app keeps its assets, its licences and the video
  module**, and code signing (Gatekeeper, SmartScreen). *When: decided below ("Decided
  here").* Recommendation: after 7.8, in its own plan; until then the game
  runs from the repository with `cargo run`.
- **Testing without the hardware:** no Windows machine, no dance pad, no
  Linux desktop. Recommendation: CI on all three systems for builds and
  pure logic, the fake pad and fake audio for play, a UTM Linux VM if the
  maintainer wants hands-on Linux checks; ask testers with pads and
  Windows once a build is published.
- **Library size and memory:** a cache of a large library and banners
  for thousands of songs. Recommendation: the cache stores entries only
  (kilobytes a song); banners stay under the texture cache's budget (P9,
  `desktop/tests/banners.rs`); measured on the local library in 7.3.
- **Two folders with the same pack** give the same song ids.
  Recommendation: the first folder in the list wins, the rest are
  reported, as StepMania keeps the first song folder it loads.

## Decided here (the maintainer may change any)

The lead's calls on the questions above (2026-10-16; the maintainer left
step 7 to the lead and may change any of them):

- **D2, cpal with our own mixer**, as recommended, confirmed or changed
  by 7.0's clock measurements before 7.2 builds on it; web-audio-api
  stays the fallback behind the same trait.
- **Linux build libraries:** CI installs `libasound2-dev` and
  `libudev-dev` (dynamically linked LGPL system libraries; the game
  stays MIT). AGENTS.md's "builds on Debian with no system libraries"
  changes in the same commit that adds them.
- **Video under wasmtime** behind a `video` feature, as the last step
  (7.7); native FFmpeg stays a later option.
- **MP3 on desktop follows StepMania 5.1's reader** (charts were synced
  against it). Whether the web should match it is its own question,
  measured on local MP3s with encoder-delay headers and settled in a
  separate change, so the two shells are never knowingly apart without
  a note in the results of that check.
- **A `[profile.desktop]`** for native release builds (speed, unwinding),
  leaving the wasm release profile as it is.
- **Packaging, signing and the dependencies' licence notices** after 7.8,
  in their own plan.
- **The microphone loopback** (7.0, item 2) waits for the maintainer;
  everything else in 7.0 runs unattended.

- One shared play session; the web refactored first, unchanged in
  behaviour.
- The library is read in place from folders and zips; nothing is copied
  on desktop.
- Desktop's device profiles and pad bindings are its own.
- Video is the last step; songs with movies play with the background
  until it lands.
