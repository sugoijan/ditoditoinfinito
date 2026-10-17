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

## Where it stands (2026-10-17)

`cargo run` is the game on desktop: song select on the bundled songs and
the song folders on disk (simfiles and Dancing☆Onigiri works, folders
and zips read in place), options with the bindings, the UI core's start
prompt, plays through the session the web runs (keyboard and
controllers, noteskins with their Lua, backgrounds, movies, a work's
layers and lyrics), the theme's results, the play history, calibration
with per-device offsets, and the device notices. Each step's results are
under it in section 7:

| Step | State |
|---|---|
| 7.0 Measure | the clock, Opus and the video module measured; the loopback, key timing and the controller wait for the maintainer |
| 7.1 One session | done: `session/`, the import's core in `library/` |
| 7.2 Audio | done: cpal with our mixer, CoreAudio's latency, every format, MP3 as StepMania |
| 7.3 Library | done: folders, StepMania roots and zips in place, the cache |
| 7.4 Input | done: keys with the OS's stamps on macOS, gilrs, the bind flow |
| 7.5 Play | done, and its review's fixes |
| 7.5b Works | done: Dancing☆Onigiri works, layers, lyrics |
| 7.6 Calibration | done, and its review's fixes |
| 7.7 Video | done: the module under wasmtime, wired in 7.5, on by default |
| 7.8 Windows and Linux | CI on all three; Windows checked from the Mac; the gilrs change waits |
| 7.9 Docs | done |

**Left to do:**

- Frame pacing: late with two frames in flight is the default when a
  macOS display link is available, block with one elsewhere. The link
  held 120 Hz in the maintainer's run; margin learning and the report's
  queue estimate are now corrected and simulated. Check 9 still needs
  the next windowed run: simulation does not prove compositor behaviour.
- OS video decoders are the next video work, macOS then Windows; the
  module remains the shipped decoder and fallback. The VT spike and
  later sandboxed native helper are in `native-video.md` (decision 50).
- Packaging and signing (an `.app`, an installer, the video module and
  the licences beside the executable), in their own plan.
- Windows: the gilrs change (8 ms polling, stamps from the reading) needs
  the maintainer's account and a Windows machine; a run on Windows
  hardware; the video module built on Windows and Linux hosts.

**The maintainer's checks** (what no agent or CI can do here):

1. **The microphone loopback** (7.0, item 2): a click played and
   recorded on the built-in speakers, wired headphones and Bluetooth;
   the true output latency against what the clock reports.
2. **Key timing** (7.0, item 3): the monitor's stamps against receipt at
   60 and 120 Hz (a `--seconds` run prints the pairing at its end).
3. **The controller** (7.0, item 4; 7.4): song select, options and play
   by a real controller, binding it.
4. **Windowed play** (7.5): whole plays by keyboard and controller in
   the window, against Firefox on the same Mac (the calibrated offsets
   should agree within a few milliseconds once the latencies are
   subtracted); a song folder of one's own and its bests.
5. **Calibration by hand** (7.6): on speakers and on Bluetooth
   headphones, kept per device; the new-device notices.
6. **Video on desktop** (7.7): a pack's movies in play, by eye.
7. **IME** on song select's search (macOS).
8. **Linux and Windows hardware:** the window, sound, IME and a
   controller on a real Linux desktop (a UTM VM will do) and on Windows.
9. **Frame pacing** (section 3, "Frame pacing"): `DDI_HOME=<scratch>
   cargo run --profile desktop -- --play beatucada --chart 0 --auto
   --start --pacing-report --volume 0.005`, the window left alone in
   front until the results, windowed at 120 Hz (ProMotion), then in
   fullscreen; the printed report's four columns (block·1, block·2,
   late·1, late·2, four seconds each in turn): "display link:
   CADisplayLink" in the title, refresh times 100 % link, the link's
   period 8.333 ms with a small spread, then which columns repeat no
   refreshes and, of those, which has the lowest frame start → estimated shown. These are
   model estimates, not measured presentation latency.
   Expected from the simulation: block·2 smooth at a refresh more than
   late·2, late·2 with few repeats and lowest; block·1 and late·1 repeating as
   before if the compositor keeps drawables long.

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

## Where it started

(Before step 7; kept for the record.)

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

## Decisions (proposed, then settled step by step in section 7)

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

### The frame loop (`desktop/src/play/`)

- **Menus** stay on demand (`ControlFlow::Wait`, P13). **Play** runs
  continuously: each frame reads the clock, drains input, updates the
  session, builds the `Frame` at the predicted presentation time and
  draws.
- **Present modes** (D6): Mailbox if offered, else Fifo (`AutoVsync`);
  `desired_maximum_frame_latency` 1 in play, 2 in menus. Frames are paced
  with `WaitUntil(next_vblank − margin)`, the refresh rate from winit's
  `MonitorHandle::refresh_rate_millihertz` checked against frames
  measured (the web's `FpsMeter`). An option "lowest latency (may tear)"
  uses Immediate. As built: "Frame pacing" below.
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

### Frame pacing (2026-10-17)

`desktop/src/pacing/` (pure, tested on a fake clock), driven by the
shell's loop; `--pacing block|late`, `--pacing-report`.

**Blocking frames (the default without a display link).** The loop polls; each
frame waits in `get_current_texture` until the frame before is shown
(Fifo with one frame in flight: on Metal `maximumDrawableCount` 2, from
`desired_maximum_frame_latency` 1, so `nextDrawable` returns when the
refresh releases the older drawable), then reads the clock and input,
draws and presents, and is shown at the next refresh. Between the play
reading its input and the picture showing it lies a whole refresh
(16.7 ms at 60 Hz, 8.3 at 120), and the main thread spends most of each
frame blocked. Judging does not suffer (keys carry the OS's stamps on
macOS, pads gilrs's, D4), but on Windows and Linux keys are stamped on
receipt, which a blocked loop delays (D6), and what the player sees of
their press (the receptor, the judgement) is a refresh older than it
could be.

**Late pacing (`--pacing late`).** Each frame starts as late before the
refresh it is for as its cost allows, the loop asleep in
`ControlFlow::WaitUntil` until then (events are still read as they
come), so it never waits in the acquire:

- **Present mode:** Fifo, two frames in flight with a display link
  by default, one otherwise (`--in-flight 1|2` overrides), whatever the surface
  offers. The waits are how the refreshes are found (below); Mailbox
  never waits, so it says nothing of them, and drawing as fast as it
  takes frames spends a core (Metal has no Mailbox anyway). Immediate
  tears; not considered. Where blocking frames use Mailbox (DX12 and
  Vulkan surfaces that offer it), a frame is shown up to a refresh
  sooner than under Fifo, so the session's prediction, and the notes,
  sit about half a period apart between the two modes there: a visual
  offset calibrated in one is off by that much in the other. On Metal
  both are Fifo and agree.
- **The refreshes (`Grid`).** wgpu 30 gives no present timestamps (the
  Metal hal keeps the `CAMetalDrawable` private, so `presentedTime` and
  `addPresentedHandler` are out of reach without a fork). What the
  surface does tell: an acquire that waited (over 1 ms) returned when
  the frame before was shown. The period starts as the OS's rate
  (`refresh_rate_millihertz`), is seeded from the median of consecutive
  returns when that is more than 10 % off (a wrong rate, none), and
  refined by the slope of the returns against their refresh count; the
  phase is the median residual, returns more than 2 ms (or a quarter
  period) off left out. Returns that do not fit a grid (under 70 % of
  them, or a spread over 1 ms: a compositor that releases at no fixed
  time, a variable rate with no cadence) are no grid, and with no grid,
  or none seen for 2 s, frames block as before. Robust to the odd acquire
  held up for another reason (tested with 10 % strays).
- **Locking and probes.** A play's first frames block, as before, until
  six returns fit. Then every half second one frame (a probe) starts
  right after the one before: its acquire waits for the refresh (one
  more return, the grid kept locked and its period refined), and it is
  still shown at the next refresh, so no refresh is lost; only that
  frame has the old latency (2 frames a second). Every 4 s a burst of
  four such frames looks for a faster refresh: returns closer together
  than three quarters of the period (a display gone from 60 to 120 Hz
  while the OS's rate was not read again) start the grid again on their
  spacing (simulated: found within one burst, 120 frames a second after,
  no repeats).
- **The schedule.** The next frame starts at the last frame's refresh
  plus a period, minus the budget: the 95th percentile of the last 120
  frames' costs plus a margin. A frame's cost runs from its planned
  start to its present's return with the waits left out, so it holds
  the timer's lateness, the loop's turn, the UI's update, the play's
  tick and drawing and the present call. On macOS winit's `WaitUntil`
  is a `CFRunLoopTimer`; when it fires, `about_to_wait` asks for the
  redraw, `request_redraw` wakes the run loop and `RedrawRequested`
  comes in the next turn (microseconds), all counted in the cost.
- **The margin** covers GPU work and the compositor's deadline. It
  starts at 2 ms. A wait ending over half a refresh after the inferred
  drawable release suggests a missed refresh. Repeated misses within
  10 s raise the floor by 0.5 ms and the margin by 1.5 ms; an isolated
  miss with more spare time than recent successful frames changes neither.
  The margin decays to the floor over 5 s. The cap is half a refresh per
  frame in flight: with two, the spare drawable allows up to 8.33 ms at
  120 Hz, instead of the old 4.17 ms ceiling. The floor survives songs
  and drops 0.5 ms after a minute without misses (including time in menus).
  A miss within 30 s of lowering it doubles that hold, up to eight
  minutes. Wake lateness remains part of the measured cost's 95th
  percentile; the margin covers recurring tails. This inference is
  imperfect, especially when compositor releases drift across its
  half-period threshold; direct presentation timestamps would improve it.
- **The predicted presentation** for late pacing is the estimated
  target refresh on the grid, handed to the play (`Host::present_at`
  → `View::present_at`). Blocking keeps the session's prediction,
  "now plus the frame interval". They agree in the simple one-in-flight
  model where a refresh immediately releases its drawable; delayed
  compositor releases and a queued second frame break that assumption.
  Compare visual calibration after changing modes or queue depth. Neither
  prediction is a measured presentation time, and neither changes judging.
- **A variable refresh rate** at its top rate (the rate the OS gives):
  a frame presented before the shortest period has passed is shown when
  it does, so the grid holds as on a fixed display; slower frames are
  shown on arrival, early by at most the margin.
- **Menus** are unchanged: on demand (`Wait`), AutoVsync, two frames in
  flight; nothing paced, nothing polled.
- **Judging** is unchanged: the audio clock is the play's, input keeps
  its stamps, and only when the frame reads them and what it draws
  move.

**What the first reports showed (2026-10-17).** Two `--pacing-report`
runs on the maintainer's M1 Max (ProMotion, the OS says 120 Hz),
windowed. The grid from the acquires' returns locked for 0–13 % of the
frames, so late pacing ran as blocking frames; the returns came at no
fixed time. Block itself repeated refreshes (300 and 743 in about
44 s), its present-to-present 9–10 ms ± 2.7–3.4 ms with builds of
1.3–2.3 ms, waiting 7–7.5 ms a frame in the acquire; the rate measured
111–117 Hz. Nobody felt a difference.

Reading it: with one frame in flight the layer has two drawables
(`maximumDrawableCount` = `desired_maximum_frame_latency` + 1 in
wgpu's Metal backend; Apple allows 2 or 3, 3 by default). The frame
after a present waits until the compositor lets go of the drawable it
replaced, which in a window comes some time after the refresh, not at
it; the frame then has what is left of the period for its build and
the compositor's deadline, and when that is too little it misses and
the refresh repeats. Developers report the same of two drawables on
Apple's forums (drawables held longer than expected, `nextDrawable`
blocking; [forum 711033](https://developer.apple.com/forums/thread/711033)),
and Apple's guidance for variable refresh displays is to draw for the
display link's `targetTimestamp` at the link's own rate, which can be
below the display's top rate ([WWDC21 10147](https://developer.apple.com/videos/play/wwdc2021/10147/)).
Adaptive-Sync scheduling is for full-screen apps; a window's rate is
the compositor's, which ProMotion lowers when content does not keep up
or does not ask for more. `CADisableMinimumFrameDurationOnPhone` is an
iPhone key; a Mac asks through the display link's
`preferredFrameRateRange`. `presentsWithTransaction` stays off (wgpu's
default) and `displaySyncEnabled` on for Fifo. The simulation
reproduces the report with a compositor that keeps a replaced drawable
6 ms and wants frames 1 ms before the refresh: blocking frames with one
in flight repeat a third of the refreshes, with two in flight none
(`a_compositor_that_keeps_drawables_needs_two_in_flight`).

**What changed for it.**

- **A display link** (`desktop/src/display_link.rs`): on macOS 14 and
  later the window's view's `CADisplayLink` (`NSView.displayLink(target:
  selector:)`, objc2's bindings), on the main run loop, paused outside
  play, its `preferredFrameRateRange` pinned at the OS's rate (120 Hz on
  ProMotion) so the window is not held back; before macOS 14 a
  `CVDisplayLink` (the active displays', on its own thread). Each
  refresh's `timestamp` and `targetTimestamp`, read on the host clock
  (`CACurrentMediaTime` counts `mach_absolute_time`, as `Instant` and the
  keys' stamps do; tested), are the pacer's grid while they keep coming
  (`Grid::tick`; the link's own interval is the period): no locking
  phase, no probes or bursts. Elsewhere, and if the link stops, the
  grid is inferred from the acquires' returns as before. The shell
  feeds the latest refresh before asking for a frame and before starting
  one; the link's callbacks wake the loop each refresh in play.
- **Frames in flight** (`--in-flight 1|2`, 2 with a link, 1 otherwise): two give the
  layer three drawables, so a frame never waits for the compositor to
  let go of one. Blocking frames then queue a frame ahead (a refresh
  more latency, smooth); late frames start before each refresh as
  before and keep the queue empty. The pacer knows which refresh frees a
  drawable (the last frame's with one in flight, the one before's with
  two) and learns how long after it the compositor lets go (the waits
  that end within half a refresh of it), so a late frame does not start
  before a drawable is free. A miss is now a wait that ends more than
  half a refresh after that refresh, whatever the frames in flight.
  With two in flight a missed refresh leaves a frame queued behind;
  the next acquire then waits and says so only when the compositor keeps
  drawables longer than the frame's budget leaves (the ProMotion case);
  with one, every miss shows.
- **The report** runs four columns in turn, four seconds each: block·1,
  block·2, late·1 and late·2, all on Fifo, and says
  whether a display link runs (and which), where each column's refresh
  times came from (link or returns), and the link's period and its
  spread (whether ProMotion held 120 Hz).

On Vulkan and DX12, Fifo waits in the acquire, the present or the GPU
depending on the driver; the report says which.

**The linked run and the next default (2026-10-17).** The maintainer's
M1 Max run held 120.00 Hz in all four columns (link period 8.333 ms,
sd 0–0.005 ms, grid locked 100%). Block·1 repeated 40 refreshes in
25 s, block·2 none; late·1 repeated 60 in 21 s, late·2 23 in 21 s
(46 and 18 inferred misses). Builds averaged 0.9–1.3 ms and wake
lateness p95 was about 1.5 ms; late's margin had reached its old
4.17 ms ceiling. No difference was felt by hand without calibration.
Late·2 is now the default with a link; block·1 stays the fallback
without one. The larger margin and persistent floor need check 9.

The old block latency figures, 29.8/51.4 ms mean/p95 for one frame in
flight and 76.4/125.5 for two, **do not establish those latencies**.
The estimate carried a phantom queue forward whenever its inferred
refresh fell behind. `target_after` now bounds it by the configured
queue capacity. Block assumes that capacity is full; late carries only
frames still inferred due. Actual compositor scheduling is still unknown.

The deterministic ProMotion-like model (120 Hz, build 1.1±0.3 ms,
exponential timer lateness averaging 0.5 ms, drawable release
4.5±0.7 ms after refresh, latch deadline normally 1 ms and 6 ms on
0.4% of frames) now gives, in a 100 s alternating report: repeats
46 / 0 / 86 / 4 for block·1 / block·2 / late·1 / late·2; frame start
to estimated presentation means 11.03 / 19.27 / 4.26 / 8.18 ms.
A separate ten-minute late·2 run repeats 18 of 71,923 frames, nine in
the first minute, then 0/3/0/2/1/2/0/1/0 each minute; mean 8.61 ms,
p95 9.51, final margin 6.50 ms. This is near-zero in the model after
learning, not a claim about the next hardware run.

To measure directly on Metal, retain each `CAMetalDrawable` and attach
[`addPresentedHandler`](https://developer.apple.com/documentation/metal/mtldrawable/addpresentedhandler(_:)),
then associate its [`presentedTime`](https://developer.apple.com/documentation/metal/mtldrawable/presentedtime)
with the frame's input sample and start. wgpu-hal 30 keeps the drawable
private; `Texture::as_hal` exposes the texture, not that drawable. This
needs a small Metal surface/present hook in wgpu-hal (or an upstream
API), with callbacks delivering records without touching the renderer.
It measures presentation, not photons or end-to-end device latency.

**Measuring (`--pacing-report`).** A play runs its columns in turn,
four seconds each (block·1 first; counted from the first successful
present, with half a second after startup and each switch left out), all on Fifo (so the columns differ in pacing and frames in flight
only; the surface is configured again when the frames in flight change;
where block alone would use Mailbox the title says so), and prints,
when it ends (or the window closes), a column for each: frames, the
refresh and where it came from (link, measured, the OS's or assumed),
the share of refresh times from a display link and from the returns,
the link's period (mean and sd), the share on a locked grid, present-to-present
intervals, time in the acquire, the present call, the build (acquire's
return to present), how late the timer woke, input read → estimated shown and
frame start → estimated shown (to the refresh the grid gives the frame),
estimated repeated refreshes (gaps between inferred refreshes), frames that
overran or came early (shown at another refresh than drawn for) and
missed refreshes, probes and the margin. Keys and pads reach the play
when the loop polls them, just before a frame starts; a blocking frame
then waits in the acquire, so "input read → estimated shown" (from the clock
read after the acquire) understates what a press waits for under block,
and "frame start → estimated shown" is the closer figure: late's gain for keys
and pads is the larger of the two. The probes and bursts make late's
present-to-present intervals uneven (they start right after the frame
before); what is shown stays one frame a refresh, which the estimate approximates. Startup `Occluded` acquires
are retried quietly and never enter the report.

**The harness.** `pacing::sim::Surface` is a Fifo swapchain on a fake
clock (drawables, a compositor's deadline, the release after a refresh)
over a fixed or variable display; `sim::run` and `sim::Loop` drive a
`Pacer` as the shell's loop does and say when each frame was really
shown. It is built for tests only (`cfg(test)`, and the `sim` feature
the integration tests turn on through ddi-desktop's dev-dependency on
itself), not into the shipped binary. Tests
(`desktop/src/pacing/tests.rs`): the grid (a 119.88 Hz display called
120, a wrong rate, strays, no grid), block never sleeps, late at 60, 120
and 144 Hz (latency, the same drawn-for time as block, no repeats, one
frame a refresh), one overrun costs one repeat and no more, a 3 ms
deadline it is not told is learned, hiccups every 20 s for ten minutes
leave the floor where it was, the floor comes down without misses and
the learned floor survives a new play, a faster refresh found with a stale OS rate, a
timer up to 2 ms late, a variable refresh rate, a new play locks again,
the report over one alternating run, a display link as the grid from
the first frame, a compositor that keeps drawables (one in flight
repeats, two do not); `display_link.rs`: a `CVDisplayLink` run with no
window ticks at the display's rate on the host clock, and
`CACurrentMediaTime` is the host clock. A review's simulation (phase
shifts, 60 ↔ 120 Hz, a 3 s stall, late releases) ran on it too. `desktop/tests/pacing.rs`: every bundled chart autoplayed with its
frames at the times late pacing gives them (60 Hz, 144 Hz, a 48–144 Hz
variable display, a missed refresh, a late timer) judged as at a steady
60: the same tally, the same log (so the same replay), every hit at the
same offset.

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
- **By the maintainer:** the list under "Where it stands" at the top.
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
  Settled 2026-10-16: the web's fallback decodes Opus with the same code
  (`platform/src/opus.rs`, which `decode_all` now goes through), in a
  worker fetched only when needed (PLAN.md decision 9).

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

#### Done, 7.1 (2026-10-16)

- **`session/`** (`ddi-session`): `play.rs`, the play the web ran in
  `web/src/play.rs` (the engine's `Player`, bindings, the quit gesture,
  autoplay and `auto=pad`, the fail's fade, a work's fades, the facts a
  play is kept with), generic over `ddi_platform::AudioBackend` and an
  `Inputs` trait (begin and poll, connected pads, interruptions, device
  changes, the test pad); `calibration.rs` (modes, chart, click track,
  generic); `backgrounds.rs` (the frame's shape, the refresh meter, the
  automatic video setting's step); `stage.rs` (a work's layers'
  timeline). The web keeps a thin wrapper (`web/src/play.rs`: its
  keyboard, gamepads, touch lanes, listeners, and what the field draws).
  Nine native tests on a fake clock: autoplay all top tier at 60, 144
  and 37 fps, through a test pad, calibration converging on a late
  player (visual +30 ms, audio −20 ms), the quit gestures, an
  interruption, the click track and the chart's gaps, the frame and
  the meter.
- **`AudioBackend`** gained what the session needs (sample rate, master
  volume and its ramps, stop all, running, a buffer from PCM, the
  device's fingerprint); `WebAudio` and desktop's `Audio` implement it.
  `SongStore` is gone from the docs (PLAN.md 3.5).
- **Song select's view** of the library is `ddi_ui::state::select_view`
  (the web's `select_data.rs` keeps what only it knows: banners from
  storage, the import's panel).
- **The import's core:** `library/src/scan.rs` now has its steps apart
  (`read_song`, `Reading::images_to_size` and `sized`,
  `Reading::movies`, `Reading::finish` with the files `Naming::InPlace`
  or `Naming::Stored`), so the web's `import_song` runs it on `Blob`s
  (its reads asynchronous) and desktop's `scan_song` on its folders; the
  web's own copy is gone, and so are its `identity` and `LOOSE_PACK`
  (`SongCandidate::identity`, `pack::LOOSE_PACK`) and its duplicate
  check (`scan::Duplicates`). Encrypted zip entries are left out and
  reported ("encrypted; not read"), as desktop does, instead of failing
  the song when read.
- **Checked** in Chrome (`--disable-audio-output`) against a bundle built
  before: autoplay all top tier on WebGPU and WebGL2 and through
  `auto=pad` (Beatucada, 38 taps), calibration's three automatic runs
  converging on +30 ms in one adjustment, the plays kept with the same
  records; the stills checks (opening stills, prompts, the play's
  pixels and the movie guard on generated packs) the same up to timing
  noise; an import of a folder (12 songs: four songs of two local packs, with
  background changes and movies, seven generated MP3 songs and a loose
  one) and of a zip (one song, an encrypted simfile beside it), every stored song record (ids, entries, digests, links) and file
  key and size identical, the zip's report now listing its encrypted
  entries; desktop's `every_layout_has_the_webs_ids`. Firefox and WebKit
  runs were not made (the Mac's sound was unavailable; see the report).
- **Wasm:** 6,578,792 → 6,627,052 bytes after `wasm-opt` (+48 KB),
  2,060,263 → 2,072,889 brotli (+12.6 KB), with other work landed in
  the same tree meanwhile and the MP3 fixes included.

### 7.2 Audio on desktop

- `desktop/src/audio/` (stream, mixer, clock, decoder thread),
  `desktop/src/clock.rs`, `decode-all` in `platform`, `[profile.desktop]`
  (D11).
- Song select on sample data plays previews; the developer screen's
  sound; the options' volume sample.
- Checked: the mixer and clock tests; the decoders against references;
  by the maintainer: previews start promptly, no clicks at the fades,
  the device changed while playing (headphones unplugged) keeps sound.

#### Done, 7.2 (2026-10-16)

Built: `desktop/src/audio/` (`mod.rs` the backend and the cpal stream,
`mixer.rs`, `clock.rs`, `hal.rs`, `decoder.rs`), `desktop/src/clock.rs`
(the process's epoch), `desktop/src/preview.rs` (song select's
previews, the developer screen's tone), `decode-all` in `platform`
(`platform/src/decode_all.rs`), `[profile.desktop]` in the workspace's
`Cargo.toml`, and the shell's wiring (`StartAudio`, `PlayPreview`,
`StopPreview`, song select's `Selected`, the pulse's `BeatFn`).

- **The clock as built.** On CoreAudio a frame is heard at cpal's
  `callback` (`mHostTime`) plus the device's and its output stream's
  latency read from the HAL (`hal.rs`, read on the main thread once a
  second and when cpal reports the system moved the stream). Stream
  instants become host time through an offset measured once from the
  tightest of 16 back-to-back readings of cpal's `Stream::now()` and
  `Instant` (the same clock on macOS, so the offset is constant). The
  latency reported is the lead (how far ahead of its call a buffer
  reaches the device, the largest over the last 0.5–1 s) plus the HAL's
  latencies, so `context_time` is, at a callback's start, the first
  frame of its buffer (`currentTime`) and the engine's `heard =
  context_time − output_latency` is `mHostTime` + HAL exactly. Other
  hosts keep the plan's anchor (smallest `entry − callback` over 0.5–1
  s) with cpal's `playback` and `playback − callback` as the latency;
  WASAPI's and ALSA's meaning of `callback` is still to check. Without
  the HAL, cpal's `playback` is used. Published through a sequence lock;
  `Audio::now` returns the web's `ClockSample`.
- **Measured** in the running shell (`cargo run -- --seconds 6 --volume
  0.005`, the clock sampled every millisecond on another thread, the
  first half second dropped), M1 Max, the default output (the headphone
  jack, 44.1 kHz), a 256-frame buffer: latency **9.45 ms** (9.42–9.49)
  = a 7.41 ms lead + 90 frames from the HAL (66 device + 24 stream,
  2.04 ms), where cpal alone would say 8.89 ms from `callback` and hear
  everything 6.9 ms late; jitter of `heard − host` about a fitted line
  **sd 18–20 µs, max 31–52 µs** over four runs, dev and `desktop` profile alike (7.0's 14 µs / 47–63 µs
  on the jack); drift +1.0 to +3.3 ppm; output peak −46 dBFS at master
  0.005. The bundled preview (Some Things Must, from its start to the
  preview's end) decoded and resampled in 0.15–0.19 s.
- **The mixer** (`mixer.rs`): voices of `Arc<Pcm>` (interleaved, mono or
  stereo; 5.1 folded as Web Audio does) started on an output frame (a
  start already past plays at once from its offset, Web Audio's rule),
  each with a gain that follows `Set`, `Hold`, linear `Ramp` and
  `Target` events (`AudioParam`'s, sample-accurate), a master with the
  same events (`set_master_volume` is the web's 15 ms target,
  `master_ramp(to, at, seconds)` and `master_set_at` the fades), stops
  on a frame or at once with a 5 ms fade (the web stops dead; a click
  is not worth it), linear interpolation only if a stream reopened at
  another rate. 64 voices, 16 events a gain, two `rtrb` rings of 256
  (commands in, finished voices out to be dropped on the main thread);
  the callback neither locks nor allocates. Deviation from Web Audio,
  on purpose: a ramp with nothing pending before it starts when it is
  scheduled, not at the last event's time.
- **`Audio`** implements `AudioBackend` (`now`, `play`, `stop`,
  `set_volume`, `duration`) and has, as inherent methods under the
  names section 1 gives them, `sample_rate`, `set_master_volume`,
  `master_ramp`, `stop_all`, `running`, `buffer_from_pcm` (resampled to
  the device's rate) and `fingerprint` (`audio:<host>:<device>:<rate>:<5 ms
  bucket>`, labelled with the device's name), so 7.1's grown trait is a
  move of signatures. Also `status_line` (with the output's peak, the
  web's meter) and `ClockProbe` for measurements. Without a device the
  clock is the host's and nothing sounds (CI).
- **Device changes.** macOS: cpal's default-output stream follows the
  system's default itself (`DeviceChanged`); the HAL's latencies and
  the name are read again. Elsewhere, and when the stream fails, it is
  opened again on the new default within a second, its timeline going
  on from where the old one was; sounds playing then stop (a preview
  starts again on its next loop). Not exercised unattended (no
  unplugging).
- **Decoding** (`decode-all`): Symphonia's readers (MP3, Ogg, FLAC, WAV,
  AIFF; ID3 and APE tags skipped) and decoders, Ogg Opus through
  `rusty-opus` (pre-skip dropped, ended at the last granule, the
  header's gain applied; mapping family 0 only, which every simfile
  uses), a `Range` to stop early, and `resample` (rubato's FFT
  resampler over the whole clip, its delay trimmed). The decoder thread
  reads the file, decodes to the end of what is asked, drops what is
  before its start, resamples once and hands back a `Sound`; a newer
  job replaces any waiting, a panic fails its job only. Checked: the
  four bundled songs against libopus's float decode (`ffmpeg 9.0.2 -c:a
  libopus -request_sample_fmt flt`, libopus 1.6.1) above 120 dB SNR
  and within 1e-4 once libopus is clamped to ±1 (`rusty-opus` clamps
  inside its decode and has no unclamped output, so loud masters'
  peaks above 1 stay clipped: the 7.0 note stands); our length ends at
  the granule, ffmpeg's 312 frames earlier. FLAC and WAV lossless,
  Vorbis identical to the web's fallback, resampling keeping pitch,
  phase and length.
- **MP3 as StepMania 5.1 decodes it (D9).** Read in `RageSoundReader_MP3.cpp`
  (libmad): no encoder delay or padding trimmed, a first frame holding a
  `Xing` tag skipped, one holding an `Info` tag decoded as audio (a
  frame of silence) "so we match DWI sync" as BASS does, frames whose
  bit reservoir is missing played as silence. Built as Symphonia
  without gapless trimming, plus 1152 (MPEG-1; 576 for MPEG-2/2.5)
  frames of silence when the first frame holds `Info` (or `VBRI`, which
  StepMania's `xing_parse` does not know), plus silence of a packet's
  length for packets that fail. Checked sample for sample against
  FFmpeg's untrimmed decode (`-flags2 +skip_manual`, which drops the tag
  frame as Symphonia does) on generated fixtures with an `Info` tag, a
  `Xing` tag and none, and the shift against a gapless decode: +2257
  frames with LAME's `Info` (1152 + 576 + 529), +1105 with `Xing`, 0
  without. Local MP3s (`~/Music/Simfiles`, 219 files, scanned locally
  only): 203 have no tag frame (decoded as browsers do), 13 a LAME
  `Info` tag (2257 frames, 51 ms at 44.1 kHz later than a browser's
  decode; checked on one by decoding), 2 a LAME `Xing` tag (1105
  frames, 25 ms), 1 no frame in its first 64 KB (an `Info` file behind
  a large ID3 tag, found by the stricter scan since). That was the data
  for the web's separate question, settled the same day: the web now
  lines MP3s up with StepMania's decode too (PLAN.md decision 46), and
  the MP3 decoder is `ddi_platform::decode`'s, shared by both shells.
- **Previews** as the web's (300 ms settle, `#SAMPLESTART` for
  `#SAMPLELENGTH`, 12 s when none and 30 s at most, 0.5 s fades, again
  0.6 s after it ends), with no gesture needed, decoded from the file's
  start to the preview's end and cut (exact, and 0.2 s for a bundled
  song; seeking is for later if long songs make it slow). The pulse
  reads the heard position from the clock (`Player::beat`); the shell
  draws a frame when the first sample is heard. Sample data's made-up
  songs have no music and pulse by their BPM. The developer screen's
  sound is the web's tone (880 Hz, 0.2 s, −14 dBFS) through the master.
  The options' volume sample: `Player::sample(song, play, volume)` and
  `Player::set_volume` are ready; desktop's options run on sample data
  with no platform yet, so it waits for the options screen's desktop
  shell.
- **`[profile.desktop]`** inherits `release` with `opt-level = 3` and
  `panic = "unwind"`; `cargo run --profile desktop` (AGENTS.md). A clean
  build takes about 5 minutes (fat LTO, one codegen unit), 1.5 minutes
  after a change.
- **Linux:** CI's test job installs `libasound2-dev` (and `ffmpeg` for the
  references); AGENTS.md says so. Checked in a Debian 13 container (`rust:1`,
  Rust 1.99, through `container`): `ddi-desktop` builds against ALSA and
  its tests pass (no sound device there: the backend runs silent), and
  `decode-all`'s pass; libopus 1.5.2 there decodes Sweeteners 86 dB from
  1.4 (Ubuntu 24.04, CI's) and 1.6.1, which agree with `rusty-opus`
  above 135 dB, so the reference test's bar is 80 dB and 2e-4. Not run:
  sound on a Linux machine.
- **Not checked here:** Windows (WASAPI's `callback`, only CI will build
  it), Linux's timing on real hardware, the loopback (7.0 item 2), and
  by ear: previews starting promptly, no clicks at the fades or stops,
  headphones unplugged while one plays.

**After review (2026-10-16).** An independent review's findings, fixed:

- **Underruns no longer reopen the stream.** cpal reports `Xrun` on
  CoreAudio overloads and on WASAPI, ALSA and PipeWire; only
  `DeviceNotAvailable`, `StreamInvalidated`, `HostUnavailable` and
  `BackendError` reopen it now. Underruns are counted (`Audio::xruns`,
  across reopens, in the status line) for 7.5 to flag a play;
  `RealtimeDenied` and the rest are ignored.
- **One timeline across reopens and without a device.** A new stream's
  clock is seeded from the old timeline and its first reading's frames
  renumbered so heard time goes on from it; with no stream the last
  timeline is extrapolated (a silent backend's starts at 0). Measured
  with the reviewer's harness: no step back in a `SongClock`'s song
  time across a forced reopen, `context_time` never 0 again (reopen
  takes 13–57 ms on the Mac).
- **The mixer:** a voice stopped (or `stop_all`) before it sounds is
  handed back at once (it played 5 ms at full gain); a `Target` snaps
  to its value and ends when within 1e-6 or when a step no longer moves
  an `f32` (0.8 → 0.5 stalled at 0.500021, 0 went subnormal), and the
  callback sets FTZ/DAZ on x86-64 (checked by compiling for it in a
  container); gains are cleaned at the API and in every event (finite,
  0 to 8).
- **Retry without a device:** the player wakes the loop once a second
  whatever the stream's state, so a gone device is opened again.
- **The fingerprint** is `None` until sound runs and buckets the
  stream's nominal latency (on CoreAudio the stream's buffer, the
  safety offset and the HAL's latencies; elsewhere `playback −
  callback`), so a device near a bucket's edge cannot flip profiles.
- **No voice freed on the audio thread:** the return ring holds every
  voice that can exist (commands, mixer) and is emptied before each
  command; a voice it had no room for would be leaked and counted, not
  freed.
- **Decoding:** an Ogg stream (Vorbis, Opus) ends at its last whole page
  (garbage after it no longer fails the probe); container errors are
  read past (32 in a row end the stream there); audio the reader drops
  (a corrupt FLAC frame, an Ogg page with a bad CRC) comes back as
  silence of its length from the packets' times, and FLAC and PCM are
  padded to their header's length, so later audio stays in time (the
  reviewer's corruption harness: every damaged FLAC, Opus and Vorbis
  case keeps its length; a 200-byte-flipped Ogg still fails its
  headers). MP3 is found as StepMania does, through up to 32 KB of junk
  (its 25,000-byte limit, checked as its 16 KB buffer runs dry) and
  past every ID3v2 tag met on the way, and fails past that ("Can't find
  data"); the probe no longer has to recognise the start, and a WAV of
  format 0x55 is read as the MP3 it holds.
- **Memory:** a decode keeps only its range (`Range::from`: frames
  before it dropped as they come) and resamples straight into the
  interleaved buffer the mixer plays: the decoded part and the result
  at most, not four copies. The reviewer's 500 preview switches peaked
  at 220 MB RSS (it saw 0.5–1.3 GB), 92 KB live after closing.
- **Tests added** for each: errors' kinds, the timeline across a forced
  reopen (with the Mac's device, and without), voices stopped before
  sounding, cleaned gains and arriving targets, the Opus output gain
  (−6 dB set in the header, −6.000 dB measured), ranges, garbage around
  Ogg and MP3, a corrupt FLAC frame.
- **Left as it is, with its alternative written down:** WASAPI's and
  ALSA's instants are on `Instant`'s clock too, so `Timebase::Host` with
  a measured offset may be exact there as well; the anchor stays until
  a check on such a machine logs `Stream::now()` against `Instant` and
  what `callback` means there (`audio/clock.rs`).

### 7.3 The library on disk

- Song folders in the settings, the scan thread and cache, zips in place,
  bundled songs from `assets/`, noteskins from `NoteSkins/` and the
  bundled index, the history in the data folder (`dirs`).
- Song select on the real library (`Feed`), with bests; previews from
  disk; Options lists the folders.
- Checked: the scan tests; the local library scanned and cached (times
  reported); by the maintainer: their folder added, a pack zip, packs
  removed and read again.

Done, 7.3 (2026-10-16):

- **Shared building** (`library/src/scan.rs`): `scan_song` is the
  platform-free part of the web's `import_song` (parse, notes and
  warnings, `resolve_assets`, `classify_images` from image headers,
  movies sniffed, missing files, `summarize`, the chart digests) over a
  `Reader` (read, read a range); `scan_root` builds a listing's songs.
  It follows the web's import where they differed: songs at an import's
  root are named by their simfile (`SongCandidate::identity`, with
  `pack::LOOSE_PACK`, both now in `ddi_library::pack` for the web to
  call in 7.1), a second song folder with the same pack and folder
  names is reported and left out (`scan::Duplicates`), scripted
  background animations get the web's notes; zip names are sorted as
  the web sorts its paths; a movie read in place counts as kept
  (`VideoRef::stored`), as in the web's linked import. Encrypted zip
  entries are reported and skipped on desktop (the web lists them,
  then fails to read them). The web's import is not moved onto it yet
  (7.1), so the two copies must agree until then; the ids and digests
  below say they do.
- **Logical paths** (2026-10-17): `ddi_library::path::{RelPath,
  RelPathBuf}` carry manifest filenames, scan candidates and reader paths.
  They preserve the original text, ordering, hashes and plain-string JSON;
  only `to_path(base)` converts to an OS path, component by component,
  rejecting lexical escape above the base. The local wrapper keeps those
  exact string semantics and a fallible conversion without another
  dependency; it has no `AsRef<Path>` or implicit string dereference.
  This is lexical validation, not a symlink sandbox. The separator test
  runs on every OS and Windows clippy checks the consumers. The existing
  `every_layout_has_the_webs_ids` passes; a before/after scan of 26 local
  songs in seven roots produced identical cache bytes and source listings.
  A fresh Chrome import of the same scratch corpus produced identical
  IndexedDB JSON strings (only import timestamps removed), ids, digests,
  keys and 119 stored-file hashes. Firefox also imported the two synthetic
  ZIP layouts. No cache or IndexedDB migration is needed.
- **Folders** (`desktop/src/library/`): `ddi.desktop.folders.v1` in
  desktop's storage (the web's settings untouched); each is a StepMania
  root (`Songs/`, with `RandomMovies/` and `SongMovies/` as the shared
  folders and `NoteSkins/` for skins), a songs folder, one pack, or a
  zip; zips lying in a songs folder are packs of their own. Each root's
  files are named as the web names the same folder or zip picked for an
  import (the folder's name first; a zip's entries under its name, a
  zip in a songs folder under the folder's), so packs, ids and titles
  come out the same in every layout: a pack whose name has a dot, a
  StepMania root's loose songs, a folder that is itself a song, a zip
  with one song at its root (`every_layout_has_the_webs_ids`). File
  names are compared as bytes: Chrome and Firefox give a folder's and a
  zip's NFD names as they are on disk, as desktop reads them (measured),
  so no normalisation is applied (it would change existing web ids).
  `files.rs` walks a folder as `pack` lists files (six levels, ignored
  names skipped, symbolic links never walked twice) and reads zips in
  place through `ddi_library::zip` (the central directory, then each
  entry by a seek, a range of one by a seek or by inflating only as far
  as the range; legacy names decoded as the web does, Shift-JIS by
  `encoding_rs`). Links are followed only inside the root, a folder
  reached both as itself and through a link is walked as itself, names
  that are not Unicode are reported. The first folder holding a pack
  wins; the others are reported. `--songs <dir>` adds a folder for a run; `--sample` shows
  the sample data instead.
- **Scan and cache:** a thread of its own; `library-v1.json` in the
  cache folder (`dirs::cache_dir()/DitoDitoInfinito`) keeps, per root,
  its songs with a hash of each song folder's listing (names, sizes,
  times, hashed over the song's own sorted range), a zip's size and
  time, and the folders that failed (tried again when their listing
  changes); each song is read with its pack's files only. At start the
  cache's list (of the folders listed now) shows at once; the scan then
  parses only what changed and song select's `Feed` follows. A newer
  scan stops an older one, which never saves. A folder that is not
  there (an unplugged drive) keeps its cached songs for its return.
  "Read again" reads that pack whole (song select's Manage) or every
  folder (Options).
- **Song select on the library** (`desktop/src/songs.rs`): the bundled
  songs first, then the library by pack, through
  `ddi_ui::state::select_view` (the web's `select_data` logic in
  `ui::state`, D1; the web adopts it in 7.1), with bests from the
  history; previews from the music's path (a zip's music extracted
  once into `previews/` in the cache folder); banners and backgrounds
  by path, a zip's as `zip:<zip>!<entry>` through `crate::images`;
  "Add songs" opens the system's folder or zip picker (rfd; files
  other than zips are reported); removing a pack or a song hides it
  (`ddi.desktop.hidden.v1`), files untouched, until the folder is added
  or the pack read again. A library song's preview is made ready on a
  thread (its simfile read, a zip's music extracted once into
  `previews/`, written whole then renamed, named by the zip's size and
  time, the folder kept to 64 files).
- **Options:** a "Songs" section lists the folders with Remove, Add…
  and Read again, and what the last scan found
  (`OptionsPlatform::add_folder`, `remove_folder`, `rescan`, default
  no-ops on the web); its reset forgets the folders, what was hidden and
  the cache (in desktop's words: nothing in the folders is deleted); the
  noteskins of the folders' `NoteSkins/` (walked on a thread) and the
  bundled index are listed with the web's keys
  (`desktop/src/library/noteskins.rs`; drawn natively from phase 9:
  the field preview keeps the built-in arrows until then); the play
  history exports as JSON lines to a file the player picks.
- **History** in the data folder (`desktop/src/history.rs`, D10):
  `~/Library/Application Support/DitoDitoInfinito/plays` on macOS,
  `%APPDATA%\DitoDitoInfinito\plays` on Windows,
  `~/.local/share/ditoditoinfinito/plays` on Linux, where plays kept in
  the config folder before are moved before anything records: the
  folder renamed, or file by file (each removed once copied, never over
  a newer file), what is left said on the console.
- **Measured** on the local library (`~/Music/Simfiles`, local only,
  `--profile desktop`): 1,305 songs from 1,332 simfiles (27 without
  their music, reported); a full scan 1.6 s (4.9–5.7 s before the
  per-song ranges); the cache 2.1 MB; a start with the cache lists the
  songs in 7–11 ms and its scan takes 110–230 ms, nothing parsed.
  Synthetic: 8,000 songs in 80 packs scan in 1.6–3.0 s (61 s before),
  640 ms from the cache.
- **Ids:** two local packs imported on the web (folder input, a fresh
  Chrome profile) and the same pack zipped: the same 12 ids and chart
  digests on both shells for each; bundled songs keep their folder
  names.
- **Not yet:** drawing a folder's noteskins (phase 9). The first
  start's offer came with 7.8.

### 7.4 Input on desktop

- `DesktopKeyboard` with receipt stamps and the macOS `NSEvent` monitor;
  `desktop/src/pads.rs` (gilrs thread, `PadTracker`, ids and standard
  numbering); menu input by pad; the bind flow and bindings on 6.3's
  options screen.
- Checked: the tracker tests with recorded streams; a fake pad source
  (`--auto=pad`); by the maintainer: song select and options by
  controller, binding the controller's buttons.

Done, 7.4 (2026-10-16):

- **Keys** (`desktop/src/keyboard.rs`): `Stamps` pairs winit's
  `KeyboardInput` with the OS's stamp of the same key code (winit's
  `to_scancode`, macOS's virtual key code; a key winit cannot name by
  the OS's own code, macOS's ISO § key, 0x0a, as winit's `Backquote`),
  direction and repeat: of the stamps of that key and direction before
  its next stamp the other way, the newest (the older ones were never
  delivered: a key an input method or a system panel took, a doubled
  call), the older dropped; a key without one (another platform, a
  stamp gone stale after a second, one more than 2 ms in the future)
  is stamped on receipt. On macOS
  the `NSEvent` local monitor (objc2-app-kit, key down and up; modifier
  keys' `FlagsChanged` are not monitored and stamp on receipt) queues
  each event's `timestamp`, converted to host time by reading
  `CLOCK_UPTIME_RAW` (`clock_gettime_nsec_np`) and the host clock back
  to back, and hands the event on unchanged. `DesktopKeyboard` is the
  play's `InputSource` (physical codes, repeats and doubles dropped,
  everything released on `Focused(false)`), kept from 7.5 on; menus get
  the stamped time already. The shell's UI times are the host clock's
  now (`crate::clock`), so keys, pads and audio share one.
- **Controllers** (`desktop/src/pads.rs`): gilrs 0.11.2 with its filters
  off on a thread of its own, blocking in `next_event_blocking`; each
  report becomes `PadEvent`s, read as one snapshot by the pad's
  `PadTracker` (the d-pad's two buttons of one hat change are one
  reading, so their stamp is trusted), the stamp from gilrs's
  `SystemTime` through `SystemTime::now()` and the host clock read
  together. Ids `<name> (Vendor: vvvv Product: pppp)`; pads SDL's
  mappings know, or (Linux, Windows) mapped by their driver with a
  gamepad's buttons (Linux: `BTN_GAMEPAD` in sysfs' capabilities), by
  the W3C numbers, buttons the standard does not name from 17 on (by
  their raw index); gilrs 0.11 calls every unmapped pad `Driver`, so on
  macOS any other HID joystick, as a dance pad, reads raw. Pads there
  before gilrs started get their `Connected` from `gamepads()`, a pad
  first seen through an event is made then, and a pad connected again
  in its slot lets go of what it held. Mapped pads by the W3C numbers, the d-pad's axes
  (macOS's hat) as buttons 12–15, the sticks' y turned down; unmapped
  ones by their raw codes, in ascending order and the same each time
  (macOS: the button page's usages from 1, the generic desktop's axes
  from X, so the hat is axes 9 and 10 as browsers number it; Linux:
  `BTN_*` from `BTN_MISC`, `ABS_*` codes; Windows: the raw indices),
  hats as two axes. Edges go to the main thread over a channel and wake
  the loop through an `EventLoopProxy`; a pad unplugged releases what
  it held.
- **Menus by pad on every screen** (`desktop/src/input.rs`): edges
  through `ControlBindings::menu_input` (the options' bindings as
  edited, else the stored ones) as `UiEvent::Menu`, a gesture always;
  while a screen binds (`Ui::wants_raw`), as `UiEvent::Raw`, decided
  for each edge once the one before was handled (the press that opens
  the bind flow sends the next ones raw).
- **Options on desktop** (`desktop/src/options.rs`): `--screen options`
  is the theme's options on the stored settings through
  `options_shell` (the API unchanged), `OptionsPlatform` on desktop: the
  volume sample on the preview player, the controllers connected and
  what each holds (the bind flow's), the settings as edited for the
  field preview and the menus, Back to the songs. Export, removal,
  calibration and the demo wait for 7.3, 7.5 and 7.6.
- **Fake pad:** `--fake-pad "1.0:13+,1.1:13-"` (seconds, W3C button,
  pressed) plays a standard pad through the same thread, hub and
  routing; `Pads::fake` does in tests.
- **Linux:** gilrs links libudev: CI installs `libudev-dev` beside
  ALSA's headers; AGENTS.md says so. Checked in a container (`rust:1`,
  Debian, with `libasound2-dev` and `libudev-dev`): the desktop crate
  and its tests build, the keyboard and pad tests pass.
- **Checked:** keyboard pairing on recorded macOS streams (a roll, a
  repeat, stale, missing and future stamps), the conversion of a stamp
  onto the host clock; the hub on recorded standard and unmapped
  streams (W3C numbers, the d-pad's hat, stick hysteresis, a stamp from
  the future falling back to receipt, an unplugged pad); gilrs's own
  events through `button_event` and `axis_events`; the bind flow on
  desktop's options driven by the fake pad (`desktop/tests/bind_flow.rs`:
  the pad moves down the rows as a menu, opens its own row's bind flow
  with A, binds its d-pad, Start and Back, and the stored bindings have
  them); 4-second windowed runs with gilrs (no controller connected:
  none seen, the monitor installed) and with the fake pad driving
  options (6 edges to menus). By the maintainer: a real controller in
  song select and options, binding it, and keys' stamps (the monitor's
  pairing ratio is printed at the end of a `--seconds` run).

### 7.5 Play

- `desktop/src/play.rs`: the session on the desktop backend and sources,
  the play field in the painter's `External` slot, the present modes and
  pacing (D6), noteskins with Lua and SVG, backgrounds, layers, results,
  the history recorded, `--play`/`--auto`.
- Checked: offscreen frames against the web's; autoplay of every bundled
  song to its results all top tier in CI (fake audio) and on the Mac
  (real audio); `--auto=pad`; the maintainer plays by keyboard and
  controller, and compares with Firefox on the same Mac.

#### Done, 7.5 (2026-10-16)

Built in `desktop/src/play/`:

- **`mod.rs`:** `Play`, the shared session (`ddi_session::play`) on the
  desktop's `Audio` and `DesktopInputs`: the shell's keyboard and pad
  edges come through a `Feed` (the keyboard's only while a play reads
  them; a hidden window interrupts the play, as a hidden page does),
  `--auto=pad`'s test pad answers there too. The play is kept in the
  history at its fail or its end through `ddi_session::record`, the
  record the web now builds the same way (moved out of
  `game_canvas.rs`). Sound dropouts during the play (`Audio::xruns`) are
  a line on the results.
- **`load.rs`:** the song read on a thread from where the library has
  it (a bundled folder, a song folder, a zip in place), its music
  decoded and resampled once (`audio::decoder::sound`).
- **`view.rs`:** the play field's `Renderer` over the window's surface,
  configured plain with an sRGB view format (D6): the scene through the
  sRGB view, skins through the plain one (`SkinBlending::PlainView`); the
  backgrounds and their changes (images decoded on a thread, uploaded as
  the web uploads them), the frame's shape and the automatic video
  setting's guard (`ddi_session::backgrounds`). A surface without the
  sRGB view (Vulkan without mutable swapchain formats, wgpu's
  `SURFACE_VIEW_FORMATS`) draws the play into the plain surface itself,
  its pipelines encoding (skins blend as the scene does; `Offscreen` is
  not open to it: it needs an sRGB target): the same picture
  (`a_plain_surface_draws_the_same_play`, mean difference under 3 of
  255).
- **`skins.rs`:** the chosen noteskin readied natively, as the web's
  `noteskins.rs` does in two passes: bundled skins from the index,
  the song folders' `NoteSkins/` (`library::noteskins`), the Lua run by
  `ddi-noteskin`/`lunita` (the skin's Lua steps every frame in
  `NoteskinPlay::update`), PNG and the rest through `image`, SVG (Dito)
  checked and rasterised through `resvg` 0.48.1 at four times its size.
  Read on a thread (`skins::Reading`: the files, the first pass, the
  images decoded; Dito took 113 ms on the UI thread), made on the UI
  thread (`skins::build`: the Lua's VM stays where it is made); the
  prompt says it loads until then, and Options' preview lets a pick's
  read go when another comes first. A skin's Lua that stops twice is
  off for the run (`library::noteskins::lua_stopped`, the web's rule for
  bundled skins; a skin on disk can change, so it is not kept), named so
  in Options' list; a stall on the clock is not held against it.
- **`movies.rs`** (feature `video`): the web's `MovieDeck` without its
  browser path: every movie on the FFmpeg module (7.7's host), the same
  schedule rules, lead time and queue depth.
- **The start prompt** is the UI core's (`ddi_ui::screens::prompt`,
  6.4's), as the web's play route shows it: the song, its chart and
  mods over its background in the play's frame with the field's filter
  (drawn by the 2D pass); "loading…" until the song is read and its
  skin made, "preparing audio…" until the play's sound runs (opened
  while the prompt is up, so a play starts on a running output: its
  clock, latency and device known; Start offered anyway after 3 s), on
  new devices their offsets offered first (`new_device_step`: another
  device's, calibrate now, play with defaults; not for a test or
  autoplay), an interrupted play's Retry, a test's outcome on its card.
  An offscreen picture of desktop's prompt against the web's play route
  for the same chart differs in 0.8 % of its pixels (the focus ring,
  text edges).
- **`host.rs`:** the shell's side: song select's call (`SelectCall` and
  its `PlayFn`; the autoplay switch, `SelectCall::Autoplay`, kept by
  `Songs`), the prompt while the song is read, the play, the theme's
  results with retry (the prompt again) and the song list.
- **The shell:** a play's keys and pads go to it, not to the menus;
  play mode (the keys read, the present mode) follows the play after
  each poll and each frame (a play ends inside a frame), the keyboard's
  queue dropped at each change (`play_mode.rs`); frames run
  continuously while the play is up, and the prompt asks for one when
  it changes; in play
  the surface presents Mailbox where it has it, else Fifo, with one
  frame in flight, and back to AutoVsync after. `--play SONG [--chart N]
  [--auto | --auto=pad]` opens a chart's prompt at once; `--silent`
  opens no sound device (`audio::set_headless`: every `Audio::new` is
  `Audio::headless`, which runs on the host clock, silent, with no
  latency).
- **A fix in `render`:** the HUD's text (`TextPipeline`) now uses only
  the embedded font, as on the web: on desktop cosmic-text matched a
  system face ("System Font" on macOS) even for the embedded family's
  name, so the HUD differed from the web's (the field preview's too).

Checked, with no sound device (CoreAudio is off limits for now) and no
window (the display is asleep):

- `desktop/tests/play.rs`: every playable chart of the four bundled
  songs autoplayed on a fake clock at 60 and 144 frames a second, all
  top tier, no hold let go, kept once in the history; Beatucada through
  the test pad, the same tally.
- **Against the web:** five charts (Beatucada's Challenge and double,
  Into My Dream's Challenge, Some Things Must's pump double, Sweeteners'
  Hard) autoplayed in Chrome (`--disable-audio-output`) and here: the
  same presses and releases in the same order, judged times at most
  47–56 µs apart (the web's estimate of its audio clock), and each log
  judged again under the web's record (its ruleset and options): the
  same judgement for every note, the same tally, money and EX score
  (`the_same_judgements_as_the_web`, `DDI_WEB_PLAYS`).
- `play_offscreen.rs` on the Mac's GPU: the prompt's picture, then a
  frame 12 s into Beatucada's Challenge with Dito (SVG), Delta (Lua) and
  the built-in arrows: receptors, notes, the background, the skin
  loaded. Against Chrome's screenshot of the same chart at the same
  second at 1280×720: the background, the field's filter, the receptors
  and the HUD's corner (the score) are identical (mean 0.29 levels over
  the frame, 99% of pixels within 1); what differs is the judgement's
  animation phase (the frames are 0.1 s apart) and the web's DOM "back"
  link.
- `play_flow.rs`: song select's call, the prompt, Enter, the play drawn
  every frame to its end, the results, kept once, R retries (the
  prompt), Escape goes back to song select.
- `play_movie.rs` (feature `video`): a song folder made with FFmpeg's
  test pattern as its `#BGCHANGES` movie, listed by the scan and played:
  the module compiled, 180 frames decoded, 217 shown (it loops), none
  late.

With sound (once CoreAudio was back): Beatucada's Beginner autoplayed
on the Mac's output in real time at master volume 0.005 (peak −47.8
dBFS), all top tier, no late edge, no dropout; the clock over the play
read 9.43 ms of latency (9.41–9.46), jitter 13.0 µs (σ; 25.8 µs at
most), drift +5.2 ppm (`a_play_on_the_sound_device`,
`DDI_AUDIO_DEVICE_TESTS=1`). Windowed runs (`--play SONG --auto
--start`, `DDI_HOME` in a scratch folder) draw about 115 frames a
second, but with the display asleep macOS reports the window covered
and the play stops as a hidden page's does, so whole windowed plays
wait for the maintainer.

After the maintainer's first look:

- **`DDI_HOME`** (`desktop/src/paths.rs`): the configuration, data and
  cache folders move under it; every agent's and test's run sets it
  (AGENTS.md, the verify skill), so no run writes the player's
  `storage.json` again.
- **Options' field preview** readies the noteskin play would draw
  (`Settings::noteskin_for`, the pump skin for pump layouts) natively,
  shown from the next frame after a pick
  (`desktop/tests/field_skin_offscreen.rs`).
- **Movies on by default:** feature `video` is ddi-desktop's default
  (`--no-default-features` builds without wasmtime; CI tests both). A
  `cargo run` finds the module in `target/video/` under any profile
  (the desktop one included); the desktop
  profile's binary is 24.5 MB with it (7.7 measured +8.4 MB for it, +6.8
  stripped). Bad Apple!! (a local pack, `--songs`, read in place)
  plays its movie: 20 s offscreen, 603 frames decoded, 595 shown, 6
  dropped, none late (`a_local_songs_movie_plays`,
  `DDI_MOVIE_SONGS=<dir>`, ignored by default).
- **`/` opens search without typing itself:** the key that starts a
  field's typing does not commit its text (`types_text` in the shell;
  `/` is the only key that opens a field).
- **The mouse pointer** hides on a key's or a pad's press and comes
  back when the mouse moves.

Not done here: the pacing by `WaitUntil` ahead of the display (frames
wait in `get_current_texture` under Fifo; not measured yet; since built
behind `--pacing late`, section 3's "Frame pacing"), WebKit
comparisons. Dancing☆Onigiri works: 7.5b.

Then, after its review (2026-10-16): play mode now ends with the play
(it stayed on, presses piling up for the next play); the play's sound
opens with the prompt (records had no devices and no latency: the
stream had not called back yet; latency facts now come from a running
output only, `ddi_session::play`); the refresh rate the OS gives stands
in until frames measure it (a first play recorded 60 Hz on a 120 Hz
display); a record's noteskin is the layout's (`noteskin_for`, both
shells) and its time whole milliseconds; a device error is logged and
a lost device made again (the window's surface and the UI's textures;
a play stops as interrupted, its prompt readies it again; results
stay), tried with `--lose-device-after S`; `--songs` folders' noteskins
are found; movies inside zips play: a stored entry read in place
(`video::MovieFile`'s range), a compressed one extracted once into the
cache folder (`movies/`, 2 GB kept, `zip::extract_to` streaming).

### 7.5b Dancing☆Onigiri works

- Desktop's library reads works where they are (pages with their
  external dos, music and back and mask images; dumps), found as the
  web's import finds them, with the same ids and entries; plays them
  with their layers, lyrics and music lined up with Chrome's decode.
- Checked: local works read in place against the web's import of the
  same folder; every chart autoplayed all top tier headless; a work
  with back and mask layers offscreen against Chrome.

#### Done, 7.5b (2026-10-16)

- **One core for both shells** (`library/src/works.rs`): what the web's
  import did with a page or a dump once read, moved as 7.1 moved the
  simfiles' (`scan.rs`): a page's dos (`read_page`, then
  `Page::finish` with the external dos), a dump's (`dump_work`), the
  songs, their ids, music, layer images and the report's lines
  (`plan`), a song's entry with its charts' keys (`entry`,
  `fill_chart_keys`). The web's import (`web/src/import/danoni.rs`)
  reads the files and stores what it found, as before.
- **The library on disk** (`desktop/src/library/`): each root's works
  scanned after its simfiles (read every scan: a few files each; a
  zip's kept with the zip's stamp), the page decoded in its declared
  charset (`encoding_rs`); a song's `WorkRef` says where its work is
  (page or dump, external or effective dos, music, layer images),
  kept in the cache (version 3). Works that cannot play (a key mode
  the game does not play, a dump without its dos) are reported as the
  web reports them.
- **A play** reads the work again (`play::load::read_work`, as the web
  reads its stored work), its music (a script's decoded, a dump's blob
  sniffed) lined up with Chrome's decode, its layers' images where
  they are. Previews of works too: from the start, a script's or a
  zip's music written once into the previews folder, lined up the same
  way.
- **Layers** (`desktop/src/play/stage.rs`, the web's `stage.rs`): the
  contents read on a thread (images at most 2048 px, their own size in
  the stage's px; texts drawn with cosmic-text on the system's fonts as
  the web's canvas draws them, three times their size, in their colour),
  their textures registered as a skin's (again after a skin replaces
  them), placed by `ddi_render::stage` on the play's timeline
  (`ddi_session::stage`).
- **Lyrics** (the renderer's `ddi_render::lyrics`): drawn by a 2D pass
  over the play (a painter of the play's view, cosmic-text with the
  bundled faces and the system's, Japanese included, read on a thread
  when the chart has lyrics), through the plain view where the surface
  has one, as the web's game loop draws them.
- **Checked:** the local corpus (`~/Music/DanOni/corpus`, read in place,
  13 works): the same 4 songs as Chrome's import of the folder (ids,
  pack, title, artist, BPM, length, layer images, every chart's layout,
  difficulty, meter, notes, digest and excessive setting); the 9
  others not read (key modes the game does not play, dumps without
  their dos); 6 charts autoplayed headless, all top tier
  (`desktop/tests/works.rs`, `local_works`, with `DDI_WORKS` and
  `DDI_WEB_WORKS`). A generated work with back and mask layers (two
  images, a text, Japanese lyrics; a folder and the same as a zip): the
  same id as Chrome's import, and offscreen at 2 s against Chrome's
  frame at 2 s, 0.6 % of the pixels differing (the text's face, the
  web's back link) (`a_works_layers_show_in_play`, always run where
  there is a GPU). `ddi_library::works` has its own test.
- **A known limit, kept as the web's:** a work's id hashes its pack and
  the last part of its page's path with its music number
  (`pack::song_id`), so two works at `site/a/index.html` and
  `site/b/index.html` of one pack get the same ids and the second is
  skipped as a duplicate. Changing the id needs a migration: the play
  history, bests and song offsets are kept by id. A way: a new id from
  the whole path inside the pack, and on both shells a one-time map
  from each old id to the new one, applied to the stored records and
  settings (the web's at its next import or load, desktop's at its
  next scan), the old id kept as an alias until then.
- **After its review (2026-10-16):** a fresh checkout's tests write the
  bundled songs' and noteskins' indexes they read when missing
  (`desktop/tests/common/mod.rs`; checked on a fresh copy with its own
  target folder), and CI writes them before its tests in both jobs; a
  charset label no decoder knows (`UTF-8N`) or a "replacement" one
  reads the page by what its bytes are, as the web does; text layers'
  colours are every form a canvas reads (`csscolorparser`); a page over
  8 MB is not read as a work; works are cached by their folder's
  listing, as simfile folders are (a page's music or images outside
  that folder changing does not read it again: "read again" does);
  the prompt waits for the song's pictures (5 s at most), after a lost
  device too; a bare `--calibrate` and a `--bias` that is not a number
  are errors.

### 7.6 Calibration and device profiles

- Calibration's three tests on desktop (6.4's menu), audio and display
  fingerprints from device names (section 1, section 3), the prompt for
  an uncalibrated profile, device changes flagged on the results.
- Checked: the automatic calibration runs (`--calibrate … --auto`)
  converge to the simulated and the real offsets; by the maintainer: a
  calibration on speakers and on Bluetooth headphones, kept per device.

#### Done, 7.6 (2026-10-16)

- **The tests run as plays** (`desktop/src/play/`): a calibration is a
  `Request` like a song's (`Request::calibration`), its chart and click
  track generated (`load::calibration`, the session's
  `calibration::song` and `click_samples` at the output's rate), played
  through the shared session with the web's configuration
  (`SessionConfig::calibrate`, now shared: the plain chart, the
  calibration ruleset, the sound-only test's notes hidden, the
  arrows-only one muted; the field with no background and each hit's
  error shown). The autoplayer follows the physical cue as on the web
  (the session's). A test is never kept in the history.
- **Its outcome** is the start prompt's card, as on the web
  (`ddi_session::calibration::card`, the web's `calibration_card`
  shared): what was measured, the offsets in effect, the devices, a
  device change during the test; save (to the device's profile, else
  the default), the guided flow's next, again, done. Done (and Escape,
  and leaving a test's prompt or quitting it) opens the menu again; the
  last step's done goes back to the songs, as the web's.
- **Calibration's menu** (`desktop/src/calibrate.rs`): 6.4's page, from
  song select's Calibrate link and Options' Calibrate button, its tests
  started through the library's play call (the menu closes, the test's
  prompt comes), Options in its place, the reset asked first.
- **Devices** (`desktop/src/devices.rs`): the audio path's fingerprint
  (the output's host, device, rate and nominal latency in 5 ms buckets,
  7.2) and the display's (`display:<monitor>:<w>x<h>@<scale>`, the
  refresh rate left out), looked at every second; the web's notices in
  the scene as the UI core's chrome (`NoticeBar`: "First time here?",
  "New audio path: …", the devices' line for five seconds), with their
  actions (calibrate opens the menu, use defaults, another device's
  offset, later, which holds for those devices); none over a play, its
  prompt or results, or calibration's pages: the prompt offers a new
  device's offsets itself before a play (its `NewDevice` step, as the
  web's). A play starts on the devices seen, touched in the settings
  (new ones at the default offsets when the prompt did not ask: a test,
  an autoplay), and a change during it is flagged on its results. `--silent` has no audio path: plays take the defaults.
- **CLI:** `--calibrate visual|audio|combined [--auto] [--bias MS]
  [--start]`; a timed run's last lines say the devices and the notice.
- **Checked:** headless on a fake clock (`desktop/tests/calibration.rs`):
  every test reads a 30 ms late autoplayer as +30 ms, and saving the
  sound-only and arrows-only tests' results brings offsets set 40 and
  25 ms off back to 0 ± 4 ms, the check then reading nothing to change;
  the window's flow offscreen (`calibration_flow.rs`: the menu, the
  arrows-only test's prompt and play, its card's Save as the visual
  offset, next, Escape back to the menu); the watcher's notices
  (`devices::tests`). On this Mac's headphones at volume 0.003 the
  sound-only test autoplayed converged at +0.0 ms in 20 hits (the
  latency the clock reports is the one heard); a fresh profile showed
  "New audio path: External Headphones · 44.1 kHz · ~9 ms output
  latency. New display: …".

Not done here: a calibration by hand on speakers and Bluetooth (the
maintainer's).

Then, after its review (2026-10-16): a skin in the settings' song
folders is found when read on its thread (the folders are taken on the
caller's: the settings are a thread's; `desktop/tests/skins.rs`); a
compressed movie inflates whatever its ratio (never miniz's `Finish`,
which wanted the rest of the output at once: 11 of 313 local movies;
a stream cut short is an error); two loads of a zipped movie extract it
side by side (parts of their own, the first one's kept), a load let go
gives its extraction up, a movie larger than the cache is not
extracted, parts left by an earlier run go at the start
(`zip_movies.rs`); a lost device reads the song's pictures again and the
prompt waits for them, and Retry after an interruption brings the
prompt's Start back (and its new-device step) rather than playing at
once (`play_recover.rs`); a record keeps the autoplayer's bias; the
web's `__DDI_MP3` says when the built-in decoder decoded the MP3; a
work's preview keeps Chrome's decode when its chart does not load.
Audio profiles match desktop's ids within 10 ms too (the web's rule, on
host, device and rate), and the watcher says nothing when only the
bucket moves; it reads the play's output while one runs, and the play's
output is kept between frames (voices given back; opened again, the
play stops as interrupted). The prompt's "preparing audio…" is looked at
every 15 ms. A misspelt `--calibrate` mode is an error. `just test`
writes the bundled songs' and noteskins' indexes first.

### 7.7 Video

- `desktop/src/video.rs` on wasmtime (feature `video`), the module found
  as section 3 says, the YUV path, the automatic setting's guard.
- Checked: the module's frames against the web's on generated clips
  (`video-check`'s references), a local pack with movies played all top
  tier with no frame late; a build without the module.

#### Done, 7.7 groundwork (2026-10-16)

What play (7.5) plugs in, behind ddi-desktop's `video` feature (off by
default until then): `desktop/src/video.rs`, its unit tests in
`desktop/src/video/tests.rs`, `desktop/tests/video.rs`, and
`--video-check FILE [--reference REF.yuv] [--frames]` (run with
`--features ddi-desktop/video`).

- **The host.** `VideoHost` (`VideoBackend`, its source a path) runs the
  FFmpeg module on one decoder thread, shared by every movie
  (`VideoHost::shared`, as the web shares its worker), and `DesktopVideo`
  is `VideoDecoder`. It mirrors `web/src/web/video_worker.js` and
  `web/src/web/video.rs` call for call: the module's 20 WASI calls are
  ours (no `wasmtime-wasi`), over the movie files with the worker's
  64 KiB read-ahead and names (`/v/<id>.<ext>`), everything else refused;
  messages in order (open, seek, want, close, probe, recycle, load);
  frames only on `want`, so `MovieTrack` sets the queue's depth; a seek's
  generation cancelling older wants and frames; the end once until the
  next seek, with `ddi_last_time`; 50 broken frames fail a movie; a trap
  fails every open movie and the next open starts a fresh instance.
  Frame buffers are recycled as on the web (`VideoHost::recycle`, eight
  kept; frames a seek or close passes over go back by themselves). The
  black-bar probe is there too (`probe`, `probe_result`, `Picture`).
- **Frames as the renderer takes them:** `VideoFrame`'s planes are what
  `Renderer::set_video_frame` reads (Y, then U and V, 8-bit 4:2:0, rows
  packed), so `video::show(renderer, device, queue, id, &frame)` passes
  them with the matrix and range; 7.5 calls `Renderer::add_video` once a
  movie is opened and `show` with `MovieTrack::update`'s frame.
- **The module and its compilation.** Found at `DDI_VIDEO_MODULE`, beside
  the executable (`video/ddivideo.wasm`), then, for an executable built
  in the workspace (any profile), the workspace's `target/video/`,
  found by walking up from the executable to its `target` folder.
  Missing, every open fails with the web's "the video decoder is not
  included in this build". Compiled with Cranelift (`OptLevel::Speed`,
  parallel) on first use or `preload()`: 0.85 s (desktop profile), 1.9
  s (dev); serialised into the cache folder (`dirs::cache_dir()`'s
  app folder, as the library's, `/video/`, or `DDI_VIDEO_CACHE`), named by the
  module's hash and size and the engine's compatibility hash, written
  aside and renamed; read back in 5 ms (wasmtime refuses a file from
  another version or config, and it is compiled again). Memory is capped
  at 1 GiB per instance (`StoreLimits`), as the module's own build caps
  it for the web.
- **Packaging, later:** ship `ddivideo.wasm` with `ddivideo.json` and
  `ddivideo.LICENSE.txt` in a `video/` folder beside the executable (or
  the bundle's resources, with `module_path` taught where), and FFmpeg's
  source tarball or an offer for it, as the site serves them (LGPL-2.1,
  the module a separate replaceable file, decision 35); the cache folder
  needs nothing.
- **Checked against the web module under Node** on all 29 clips of
  `video/clips.sh` (generated in a scratch folder; Theora skipped, this
  ffmpeg has no encoder): every frame's time and planes' CRC-32 the same
  as `video/check.mjs`'s run under Node, every clip against `ffmpeg`'s
  own decode within `clips.sh`'s bounds, and 104–124 seeks a clip (each
  frame time, mid-periods, the gap of dropped frames, the end, past it,
  and a loop back to the start) landing on the straight-through frames
  byte for byte. `desktop/tests/video.rs` does the same on four clips it
  generates (Xvid, Xvid with dropped frames, MPEG-2, H.264), plus two
  movies decoded at once, a truncated file (plays what it has and ends),
  a corrupt one (fails to open, the decoder carries on), and the cache
  read back by a new host; skipped without the module or `ffmpeg`.
- **Speed** (desktop profile, M1 Max, the machine busy with other
  builds): 0.40 ms a frame for Xvid and DivX 3 640×480 (Bad Apple!!,
  6,571 frames), 1.19 ms for H.264 640×360, 1.71 ms at 854×480, 3.78 ms
  at 1280×720 (clip), 6.66 ms for a pack's 720p MP4 and 13.6 ms for a
  1080p one: 7.0's figures (0.43–0.45, 1.04–1.41, 1.97, 4.04, 6.67,
  14.5). Single frames took up to 0.5 s on the 720p and 1080p MP4s
  under that load (7.0 saw 36 and 55 ms), which a decoder thread a few
  frames ahead and `MovieTrack`'s late-frame rule absorb; to measure
  again on a quiet machine. Exact seeks within a GOP cost up to 0.7–1.4 s
  on H.264 with long GOPs, as on the web.
- **Binary size** (`--profile desktop`): 21.9 MB with `video` (18.5 MB
  stripped) against 13.6 MB without (11.7 MB): +8.4 MB, +6.8 MB stripped
  (7.0 measured +13 MB with `wasmtime-wasi` and default features).
- **Hardened after review (2026-10-16).** Every call into the module
  has a deadline (`CALL_DEADLINE`, 10 s: wasmtime's epoch interruption,
  the epoch moved every 50 ms by a ticker thread): past it the call
  traps and counts as a crash (every open movie fails, the next open
  starts a fresh instance), so a module spinning on a broken file
  cannot hold the thread. A panic while handling a message (none known)
  counts as a crash too instead of leaving the thread dead while its
  status reads `Ready`; the probe's and frames' reads from the module's
  memory are checked and the WASI calls' sums saturate. The cached
  compilation is sealed with its length and two checksums (CRC-32,
  SipHash) and checked before wasmtime runs it, so a file cut short or
  damaged recompiles instead of crashing every launch (it did, with
  SIGILL); a new compilation removes the cache's older modules and
  temporary files an hour old, and its temporary name is the process's
  own. The module may hold 32 files open (more are refused, `EMFILE`),
  a decoder message is cut at 4 KB, a later success clears a `Failed`
  status, and no build path is kept in the binary (the workspace's
  `target/video/` is found from the executable, any profile).
  `desktop/src/video/tests.rs` runs a stand-in module written as text
  (`wat`, a dev-dependency) that traps, spins, opens its movie 1,000
  times, writes 64 MiB of messages with no line end, asks for files
  outside `/v` and claims a 100,000×100,000 picture; and damages and
  truncates the cache. The deadline's checks cost decode time in tight
  loops: on the clips, interleaved with the earlier build, Cinepak,
  MS Video 1 and ZMBV at 320×240 went from 0.7–1.3 to 1.3–2.9 ms a
  frame, PNG 2.2–2.9 to 2.9–4.1, H.264 at 720p 1.86–1.92 to 2.04–2.12
  (on the decoder thread, well within a frame); all 29 clips still
  decode to the same frames as under Node.
- **Not done here:** the play's wiring (7.5: `MovieTrack` per movie, the
  textures, `VideoGuard`), the probe's use in the menus, Windows (CI
  only), and the module's build on Linux and Windows hosts
  (`video/build.sh` is macOS- and Linux-tested for the web already).
- **Since:** 7.5 wired it into play (movies play, the probe frames the
  backgrounds, the feature is on by default), and its review added
  movies inside zips (a stored entry read in place, a compressed one
  extracted once into the cache).

### 7.8 Windows and Linux

- CI jobs on `macos-latest` and `windows-latest`; Linux's build-time
  libraries; the gilrs merge request (Windows polling); platform paths
  checked (`%APPDATA%`, XDG).
- Checked: CI green on all three; Linux by `act`; by hand where hardware
  allows (section 6).

Done, 7.8 (2026-10-16):

- **CI:** a `desktop` job (`.github/workflows/deploy.yml`) on
  `ubuntu-latest`, `macos-latest` and `windows-latest`, `fail-fast` off:
  `cargo test -p ddi-desktop`, then with `--features video` (wasmtime),
  rust-cache keyed `desktop`, ALSA and udev headers on Linux. Not among
  the deploy's needs: the site does not use the desktop shell, and the
  `test` job still builds and tests it on Linux before every deploy.
  `DDI_NO_GPU=1` makes `gpu::adapter` find none, so the GPU tests skip
  as on a machine without one (runners have software or virtual
  adapters the pictures were not checked against); sound tests open a
  device only with `DDI_AUDIO_DEVICE_TESTS`.
- **Windows, checked from the Mac:** `cargo check` and `cargo clippy`
  of `ddi-desktop` and its tests for `x86_64-pc-windows-msvc` (with and
  without `video`) and `aarch64-pc-windows-msvc`: clean, nothing to fix
  in cfgs, paths or the keyboard's other-platform path (keys stamped on
  receipt). Folders: settings and plays in `%APPDATA%\DitoDitoInfinito`
  (config and data are both Roaming there), the library's cache and
  previews and the compiled video module in
  `%LOCALAPPDATA%\DitoDitoInfinito` (the video cache was the only one
  named in lower case on macOS and Windows: it follows the app folder
  now); file dialogs are rfd's native ones. Controllers: a pad mapped by
  its driver counts as standard only when Windows.Gaming.Input sees a
  `Gamepad` (XInput's and WGI's own; gilrs-core gives those a nil uuid),
  any other raw game controller (a dance pad) reads raw.
- **Linux:** the job's commands in `catthehacker/ubuntu:act-latest` (the
  image `act` uses) through `container run`: both test runs pass (58
  and 64 tests in the library, the integration tests skipping without a
  GPU). `act` itself with `socktainer` 1.5.3 stops at the first `run`
  step: its scripts never reach the container (the copy into
  `/var/run/act/workflow` leaves it empty), so the image is run
  directly.
- **The first start's offer** (D7, `desktop/src/library/offer.rs`): with
  no song folder (settings and `--songs`) and no answer kept
  (`ddi.desktop.offer.v1`), the places StepMania 5, ITGmania and OutFox
  keep songs are looked at by folder names only (a folder counts with a
  `Songs` folder holding a folder): `StepMania 5`, `StepMania 5.1`,
  `StepMania 5.2`, `ITGmania` and `Project OutFox` in `~/Library/
  Application Support` (macOS) or `%APPDATA%` (Windows), Linux's
  `~/.stepmania-5.0`, `~/.stepmania-5.1`, `~/.stepmania-5.2`,
  `~/.itgmania` and `~/.project-outfox` (from StepMania 5.1's
  `MountUserFilesystems`, MIT, and the games' documentation; nothing
  from ITGmania's code), and portable installs: folders named
  StepMania…, ITGmania…, OutFox… in `/Applications`, `~/Applications`,
  `C:\Games` and Steam's `steamapps/common`. Found folders are offered
  in our theme's dialog over song select (`ddi_ui::screens::
  folders_offer`, opened through song select's `Feed::overlay`): "Add
  them" adds them as song folders (StepMania roots: their noteskins and
  movie folders come too), "No thanks" (or backing out) declines; either
  answer is kept, and Options adds and removes folders later. A `--seconds`
  run's last lines name the UI's layers (`select › folders-offer`).
  Checked: the places on made-up trees for each system, the dialog in
  the harness (it opens through the feed, "Add it" answers yes) and on
  the GPU (`desktop/tests/offer_offscreen.rs`), first-start runs on a
  scratch `HOME` (offered; on this Mac also OutFox's real folders in
  `/Applications/OutFox` and `~/Library/Application Support/Project
  OutFox`, which confirm macOS's OutFox place).
- **Not done:** the gilrs merge request (Windows' 8 ms polling, stamps
  from the reading's `Timestamp`) needs the maintainer's account and a
  Windows machine to test; CI on macOS and Windows runs once this is
  pushed (only Linux could be run here).

### 7.9 Docs

- `PLAN.md` (phase 9's row, decisions for D1–D11 as settled, section 3.1
  with `session/`, the crate versions line), `AGENTS.md` (layout:
  `session/`, desktop's modules; commands: `cargo run -- --play …`, the
  desktop profile, the CI libraries), this plan's results under each step.

Done, 7.9 (2026-10-16): this plan's status at the top with what is left
and the maintainer's checks in one place; PLAN.md's phase 9 row and
decisions; AGENTS.md's desktop and web paragraphs rewritten shorter (the
details live here and in the code's docs).

## 8. Dependencies

Added or changed, all compatible with MIT distribution:

| Crate | Version | Licence | Where | Note |
|---|---|---|---|---|
| cpal | 0.18.2 | Apache-2.0 | desktop | links ALSA on Linux (below) |
| rtrb | 0.4.0 | MIT OR Apache-2.0 | desktop | the mixer's command rings (7.2) |
| symphonia | 0.6.1 | MPL-2.0 | platform (`decode-all`) | file-level copyleft, unmodified: already accepted |
| rusty-opus | 1.0.1 | BSD-3-Clause | platform (`decode-all`) | chosen by 7.0 (5); else `opus-rs` 0.1.37 (BSD-3-Clause), or `opus` 0.4.0 over libopus |
| rubato | 5.0.1 | MIT OR Apache-2.0 | platform (`decode-all`) | resampling at decode (7.2) |
| objc2-core-audio | 0.3.2 | Zlib OR Apache-2.0 OR MIT | desktop (macOS) | the HAL's latencies (7.2); already under cpal |
| gilrs | 0.11.2 | Apache-2.0/MIT | desktop | links libudev on Linux (below) |
| objc2, objc2-app-kit | 0.6.5, 0.3.2 | MIT; Zlib OR Apache-2.0 OR MIT | desktop (macOS) | the `NSEvent` monitor |
| resvg | 0.48.1 | Apache-2.0 OR MIT | desktop | SVG skins; tiny-skia under it is BSD-3-Clause |
| image | 0.25.10 | MIT OR Apache-2.0 | desktop | + `gif`, `bmp`, `webp` features |
| dirs | 7.0.0 | MIT OR Apache-2.0 | desktop | config, data, cache folders |
| arboard | 3.6.1 → 3.7.0 | MIT OR Apache-2.0 | desktop | |
| wasmtime | 49.0.3 | Apache-2.0 WITH LLVM-exception | desktop (`video`) | 7.7: `cranelift`, `runtime`, `std`, `anyhow`, `parallel-compilation` only; the WASI calls are ours, no `wasmtime-wasi` |
| wat | 1.262.0 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | desktop (dev-dependency) | the video host's tests write their stand-in module as text |
| winit | 0.30.13 | Apache-2.0 | desktop | 0.31 is still beta (0.31.0-beta.3) |
| glyphon | 0.12 (workspace) | MIT OR Apache-2.0 OR Zlib | desktop | 7.5b: a work's text layers drawn with its cosmic-text |
| csscolorparser | 0.9.0 | MIT OR Apache-2.0 | desktop | 7.5b: the text layers' colours, every form a canvas's `fillStyle` reads |

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
  a note in the results of that check. *Settled 2026-10-16:* the web
  matches it in every browser (PLAN.md decision 46; what each browser's
  `decodeAudioData` does is measured there).
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
