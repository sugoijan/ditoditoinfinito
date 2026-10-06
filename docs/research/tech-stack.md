# Tech-stack research: DDR-style rhythm game in Rust, browser-first (2026-10-06)

Scope: wasm32-unknown-unknown via Trunk on GitHub Pages first, native Windows/macOS later. Hard requirements: sub-10 ms audio/visual/input sync, low input latency, keyboard + dance-pad input, and a platform-agnostic game core. For scale: In The Groove's "Fantastic" window is ±21.5 ms and DDR's "Marvelous" is ±16.7 ms (one 60 Hz frame) ([Timing Window notes](https://scrapbox.io/0b5vr/Timing_Window)), so systematic error above a few ms is player-visible.

## 0. Version snapshot (crates.io, checked 2026-10-06)

| Crate | Latest | Date | Notes |
|---|---|---|---|
| wgpu | 30.0.1 | 2026-08-22 | 29.0.4 on 2026-07-02 |
| winit | 0.30.13 stable / 0.31.0-beta.3 | 2026-03-02 / 2026-09-04 | 0.31 still beta |
| glyphon | 0.12.0 | 2026-07-09 | depends on wgpu ^30, cosmic-text ^0.19 |
| cosmic-text | 0.19.0 | 2026-04-22 | |
| bevy | 0.19.1 (0.20.0-rc.2) | 2026-08-13 (rc 2026-09-28) | bevy_seedling 0.8 = Firewheel for Bevy 0.19 |
| firewheel | 0.14.0 | 2026-09-05 | MIT/Apache |
| kira | 0.12.5 | 2026-09-26 | MIT/Apache |
| cpal | 0.18.2 | 2026-08-16 | Apache-2.0 |
| rodio | 0.22.2 | 2026-03-05 | |
| web-audio-api | 1.7.0 | 2026-08-08 | MIT, orottier/web-audio-api-rs |
| oddio | 0.7.4 | 2023-10-15 | stale |
| tinyaudio | 2.0.0 | 2025-11-15 | |
| gilrs | 0.11.2 | 2026-05-30 | Apache/MIT |
| sdl3 | 0.20.0 | 2026-09-07 | MIT |
| rdev | 0.5.3 | 2023-06-26 | stale |
| symphonia | 0.6.1 | 2026-08-13 | MPL-2.0 |
| zip | 9.0.0-pre3 (8.x stable line) | 2026-08-11 | MIT |
| async_zip | 0.0.19 | 2026-08-22 | MIT |
| macroquad | 0.4.16 | 2026-07-30 | |
| ggez | 0.10.0 | 2026-06-03 | |
| trunk | 0.21.14 stable / 0.22.0-rc.1 | 2025-05-08 / 2026-10-01 | |
| wasm-bindgen | 0.2.129 | 2026-09-25 | |
| rgchart | 0.0.15 | 2026-06-19 | MIT, .sm parser |
| danceparser | 0.2.1 | 2026-06-07 | MIT, .sm parser |

---

## 1. Audio clock and playback in the browser

### The master clock
`AudioContext.currentTime` is the hardware-driven audio clock, in seconds, and is what every serious web rhythm/sequencer design uses as the authority ("A Tale of Two Clocks", [web.dev/articles/audio-scheduling](https://web.dev/articles/audio-scheduling)). It advances in render quanta (128 frames; ≈2.67 ms at 48 kHz, ≈2.9 ms at 44.1 kHz), so reading it from the main thread gives a step function, not a smooth value; interpolation is needed for visuals.

`AudioContext.getOutputTimestamp()` returns `{contextTime, performanceTime}`: the context time of the sample currently being output, paired with the `performance.now()` time at which it was (estimated to be) rendered by the device ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/AudioContext/getOutputTimestamp)). It is Baseline since April 2021 in Chrome, Firefox and Safari. This is the bridge between the audio timeline and DOM event timestamps (`KeyboardEvent.timeStamp`, rAF timestamps), both of which live on the `performance.now()` timeline.

Caveat: Safari/iOS 15.1 shipped a bug where `contextTime` was ~10,000x too small ([Apple forums thread 696356](https://developer.apple.com/forums/thread/696356)); nobody reported a fix in-thread. Sanity-check the pair (`|contextTime − currentTime|` must be < ~0.5 s) and fall back to `(currentTime, performance.now())` sampled back-to-back when it fails.

### Latency properties
- `baseLatency`: processing latency inside the graph (Chrome typically one render quantum or the chosen buffer size).
- `outputLatency`: estimated delay from the graph handing a buffer to the OS until it reaches the DAC. Support: Chrome/Edge 102, Firefox 70, Safari 18.4 (Baseline since March 2025) ([MDN outputLatency](https://developer.mozilla.org/docs/Web/API/AudioContext/outputLatency); [web.dev audio-output-latency](https://web.dev/articles/audio-output-latency)). Treat it as an estimate: Bluetooth output adds 100–200 ms the browser may not report. Keep a user calibration offset anyway (as Rhythm Quest does, with separate audio and visual offsets: [devlog 10](https://ddrkirby.com/rhythm-quest/devlog/10.html)).
- `latencyHint: "interactive"` is the default and the browser may ignore it; Safari in particular does not expose much control. Don't build on assumptions about buffer size; measure.

### Autoplay
An `AudioContext` created without a user gesture starts `suspended` in all browsers; call `resume()` inside a click/keydown handler. Yew menus make this easy (start the context on "Play"). Also handle `statechange`: Safari suspends/interrupts contexts on tab hide and route changes.

### Recommended sync model (what osu!, Quaver, Rhythm Quest, Godot docs converge on)
1. Decode the whole song (`decodeAudioData`) to an `AudioBuffer`.
2. Start with lookahead: `t0 = ctx.currentTime + 0.1; src.start(t0, offsetSec)`; remember `t0` and `offsetSec`.
3. Song position on the audio clock: `song = (ctx.currentTime − t0) + offsetSec`. Smooth it for rendering by maintaining an anchor `(contextTime, performanceTime)` from `getOutputTimestamp()` (refreshed every frame or every N ms and low-pass filtered against drift), then `song(tPerf) = contextTime + (tPerf − performanceTime)/1000 − t0 + offsetSec`. This gives a continuous clock in `performance.now()` units that any event timestamp can be converted into. The itch.io article on interpolating audio time ("sample the playback clock every frame, never accumulate frame deltas", drift-correct slowly) describes the same approach for Unity ([itch.io](https://itch.io/blog/744936/the-right-way-to-interpolate-the-audio-playback-time-unity)); Godot's docs cover the same with output-latency compensation ([Godot sync_with_audio](https://docs.godotengine.org/en/latest/tutorials/audio/sync_with_audio.html)).
4. What the player hears is `song − outputLatency` when `song` is derived from `currentTime`. Note that `getOutputTimestamp().contextTime` already *is* the output position (`currentTime − outputLatency` in Chrome), so do not subtract the latency a second time when anchoring through it. Judge inputs against heard time plus user calibration. Render against predicted presentation time (`rAF` timestamp + one frame interval), plus visual-offset calibration.

### MP3 encoder delay/padding
MP3 has no in-band field for encoder delay; the LAME/Xing "Info" tag carries it. Browsers used to disagree: Firefox did not trim delay/padding until Firefox 83 ([Bugzilla 1566389](https://bugzilla.mozilla.org/show_bug.cgi?id=1566389)), and Safari emitted 2,496 extra frames (0.5 s file decoded as 0.552 s) until macOS Monterey ([WebKit 228215](https://bugs.webkit.org/show_bug.cgi?id=228215)). Current browsers agree on files that carry a Xing/Info header, but MP3s without it (old StepMania packs are full of them) still decode with a ~1,105-sample (~25 ms at 44.1 kHz) leading gap whose handling is decoder-dependent. StepMania itself does not trim it, so packs were synced with that gap included; a browser decoder that does trim shifts the chart by up to 25 ms. Mitigations: (a) prefer OGG Vorbis / Opus, which carry exact pre-skip in the stream; (b) for MP3 without an Info tag, decode in wasm with Symphonia to match StepMania's behaviour, or detect and apply a fixed per-format offset; (c) make per-song offset editing a first-class feature. Opus in WebM decodes in Safari 15.4+ ([WebKit 226922](https://bugs.webkit.org/show_bug.cgi?id=226922)); Ogg Vorbis/Opus support landed in Safari 18.4 but is reported as buggy ([frequal.com](https://frequal.com/java/OggOpusStillNotWorkingInSafari18_4.html)).

### Sample rate
`AudioContext.sampleRate` is the device rate (44.1 or 48 kHz; Safari is often 44.1, Windows/Android 48). `decodeAudioData` resamples to the context rate, so time in seconds is preserved; never convert sample counts from the file to time using an assumed 44.1 kHz. Also the step granularity of `currentTime` (one quantum) differs between rates, which is another reason to interpolate.

---

## 2. Input latency in the browser

### Keyboard
`KeyboardEvent.timeStamp` is a `DOMHighResTimeStamp` on the `performance.now()` timeline ([MDN Event.timeStamp](https://developer.mozilla.org/en-US/docs/Web/API/Event/timeStamp)), stamped by the browser when the OS event was received, which is earlier and more stable than reading the clock in the handler. Use it, mapped through the audio anchor above. Precision caveats:
- Chrome ≥91: all DOMHighResTimeStamps are coarsened to 100 µs in non-cross-origin-isolated contexts, 5 µs when isolated ([Chrome blog](https://developer.chrome.com/blog/cross-origin-isolated-hr-timers)). GitHub Pages cannot be isolated (section 9), so 100 µs: irrelevant for gameplay.
- Firefox: `privacy.reduceTimerPrecision` is on by default, rounding to multiples of 2 ms per MDN (older builds documented 1 ms); with `privacy.resistFingerprinting` it becomes 100 ms, which makes the game unplayable; detect by observing quantised timestamps and warn.
- Use `event.code` (physical key, layout-independent, e.g. `ArrowLeft`, `KeyD`) for binding, `event.key` only for display. Ignore `event.repeat === true` and also de-duplicate held keys yourself (a `pressed[code]` set), because macOS can deliver repeats before `repeat` is set in some edge cases.
- Prevent default on arrow keys/space to stop scrolling, and consider `navigator.keyboard.lock()` (Chromium only) in fullscreen.

### Gamepad API
No events exist for buttons; the current W3C draft still lists button/axis events as "more discussion needed", exposes `Gamepad` only on `Window` (not workers), and returns an empty list until a "gamepad user gesture" ([W3C Gamepad](https://www.w3.org/TR/gamepad/)). You must poll `navigator.getGamepads()`. Polling once per rAF costs half a frame of average latency plus up to a frame of jitter, and loses inputs shorter than a frame; the spec issue "input loop decoupled from the game loop" explicitly cites DDR as motivation ([W3C thread, 2025](https://lists.w3.org/Archives/Public/public-webapps-github/2025Mar/0436.html)). Browsers sample the device at their own rate (Chrome ~ every 4 ms / 250 Hz historically; wired pads report at up to 1 kHz), so polling faster than rAF does help. Better-than-rAF options on the main thread:
- A tight `MessageChannel` ping-pong loop or a Worker that `postMessage`s every ~1 ms to wake the main thread, which polls `getGamepads()` and timestamps with `performance.now()`. This is the "worker ping-pong" trick; reports show sub-ms wake latency at the cost of CPU ([W3C thread 2024](https://lists.w3.org/Archives/Public/public-webapps-github/2024Sep/0224.html)). `setInterval` is clamped to ≥4 ms when nested and throttled in background tabs; `MessageChannel` is not clamped.
- `Gamepad.timestamp` is supposed to be the last hardware update time in the `performance.now()` timeline ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Gamepad/timestamp)); Chromium updates it per device report, Firefox historically did not (MDN carries a stale "not supported anywhere" note). Use it when it changes between polls, else fall back to the poll time.
Edge-detect buttons per lane, and expose both "pressed" and "released" with timestamps so holds/rolls work.

### Dance pads and WebHID
Most USB dance pads (L-TEK, StepManiaX in gamepad mode, Polish/"FSR" DIY pads using Arduino/Teensy joystick firmware, PS2→USB adapters) enumerate as HID gamepads/joysticks and appear in the Gamepad API with no extra work, with each panel as a button. WebHID (`navigator.hid`) is Chromium-only (Chrome/Edge 89+; Firefox only via an add-on, Safari none) ([Chrome WebHID](https://developer.chrome.com/docs/capabilities/hid)); use it only as an optional path for pads that expose raw sensor data or need LED output. Keyboard-emulating pads (common with StepMania setups) are covered by the keyboard path.

### Touch/pointer
`pointerdown`/`pointerup` with `pointer.timeStamp`; set `touch-action: none` on the canvas. Mobile is not a target for sync accuracy (Safari audio latency and 60 Hz rAF), but a 4-zone touch layout costs little.

---

## 3. Rendering the gameplay scene from wasm

Options:

(a) **wgpu 30 with `webgpu` + `webgl` features.** WebGPU reached Baseline in January 2026: Chrome/Edge, Firefox 141+ (Windows) / 145+ (macOS Tahoe), Safari 26+; Firefox on Linux is still behind a flag ([utsubo 2026 overview](https://www.utsubo.com/blog/frontier-web-apis-2026-production-ready)), so the WebGL2 fallback stays mandatory. Cost: both backends plus naga add roughly 2–3 MB of uncompressed wasm (Vello's `vello_hybrid` reports removing ~3 MB by dropping wgpu for a hand-written WebGL path: [vello#1011](https://skia.googlesource.com/external/github.com/linebender/vello/+/refs/heads/ajakubowicz-fix-wgsl-source%5E)); with `opt-level="z"`, `lto="fat"`, `wasm-opt -Oz`, and Brotli/gzip from GitHub Pages, a wgpu+glyphon app lands around 1–1.5 MB on the wire. Startup: WebGL adapter/device creation is ~tens of ms; WebGPU `requestAdapter` + `requestDevice` is async and typically 50–200 ms. Pros: same renderer on desktop (winit 0.30 + wgpu), instancing, shaders for effects, and you already have the setup. Cons: size, async init, and `wgpu` objects are not `Send`/`Sync` on web.

(b) **Canvas2D via web-sys.** Zero size cost, trivial, and `drawImage` of a few hundred sprites per frame is fine at 60–144 Hz on any GPU-accelerated browser. Cons: no desktop parity, limited effects, text via `fillText` is easy but inconsistent across browsers. A good fallback or a prototyping path, not the long-term renderer.

(c) **Raw WebGL2 via web-sys/glow.** glow would allow sharing GL code with desktop, but you lose Metal/DX12 on native and the wgpu ecosystem (glyphon). Smaller than wgpu by ~2–3 MB. Only worth it if size becomes the binding constraint.

(d) **Full engines.**
- Bevy 0.19/0.20: wasm builds are 15–55 MB uncompressed before `wasm-release`/`wasm-opt`, several MB after ([Bevy setup docs](https://bevy.org/learn/quick-start/getting-started/setup/)). Default `bevy_audio` is rodio-based; `AudioSinkPlayback::position()` exists ([docs](https://docs.rs/bevy_audio/latest/bevy_audio/trait.AudioSinkPlayback.html)) but is the mixer's consumed-sample counter with no output-latency or hardware-clock pairing. Firewheel (via `bevy_seedling` 0.8, slated to become Bevy's default) adds a seconds clock and `EventDelay` scheduling ([DESIGN_DOC](https://docs.rs/crate/firewheel/latest/source/DESIGN_DOC.md)). Bevy's web audio runs through cpal's Web Audio backend, so you cannot reach `AudioBufferSourceNode.start(when)` or `getOutputTimestamp` directly; you would end up bypassing the engine's audio anyway. Not recommended for a web-first rhythm game.
- macroquad 0.4.16 / miniquad: audio via quad-snd exposes `play_sound`/`stop_sound`/`set_sound_volume` only, no position or scheduled start ([docs](https://docs.rs/macroquad/latest/macroquad/audio/index.html)); its web backend is JS glue, not wasm-bindgen, so it cannot coexist with Yew/web-sys cleanly.
- ggez 0.10 (desktop-only in practice), notan (sparse maintenance), comfy (last release May 2024). None fit.

**wgpu+winit vs Bevy for the abstraction goal.** With wgpu+winit you own the loop: on web the "window" is your canvas (winit can wrap an existing canvas, or you skip winit on web entirely and use rAF + web-sys events, which is what Yew integration pushes you toward) and on desktop winit 0.30's `ApplicationHandler`. The `render` crate takes a `wgpu::Device/Queue/TextureView` and a scene description, so it is platform-free. Bevy would force its scheduler, asset system and audio onto the web build. Recommendation: wgpu 30 + a thin per-platform shell; no winit on web.

**Frame pacing.** rAF follows the display rate in Chrome and Firefox (144/240 Hz work); Safari historically throttled rAF to 60 Hz on 120 Hz devices and now clamps to device rate on ProMotion ([WebKit 173434](https://bugs.webkit.org/show_bug.cgi?id=173434)). Never assume 60 Hz: compute from rAF timestamps. Render receptors/arrows at `song(predicted_present_time)`. On high-refresh monitors the visual error is already small; input and audio dominate.

**OffscreenCanvas.** wgpu supports `WebOffscreenCanvasWindowHandle` ([wgpu docs](https://wgpu.rs/doc/wgpu/rwh/struct.WebOffscreenCanvasWindowHandle.html)), but rendering in a worker needs wasm threads or a separate module instance, and input/audio stay on the main thread. Without SAB on GitHub Pages it buys little. Skip for MVP.

**DPI/resize.** Size the canvas backing store to `clientWidth × devicePixelRatio` (use `ResizeObserver` with `devicePixelContentBoxSize` where available; it also fires on zoom/DPR change), call `surface.configure` on change, and keep the game in a fixed virtual coordinate space (e.g. 1920×1080 letterboxed). On desktop winit's `ScaleFactorChanged`/`Resized` map to the same call.

---

## 4. Native desktop audio with an identical abstraction

- **web-audio-api 1.7.0** ([repo](https://github.com/orottier/web-audio-api-rs)): pure-Rust Web Audio API on cpal (optional cubeb/JACK/ASIO), decoding through Symphonia (MP3, Vorbis, FLAC, AAC, WAV …). Exposes `current_time()`, `base_latency()`, `output_latency()`, `sample_rate()`, `decode_audio_data()`, `AudioBufferSourceNode::start_at_with_offset()` ([docs](https://docs.rs/web-audio-api/latest/web_audio_api/context/struct.AudioContext.html)). Actively released (1.4–1.7 in 2026), MIT. It mirrors the browser API almost 1:1, which makes the desktop backend a near-transliteration of the web one. It has no wasm target (its README calls cpal/wasm experimental; on web you use the real Web Audio API).
- **kira 0.12.5**: clocks (`ClockHandle`, `StartTime::ClockTime`), `StaticSoundHandle::position()`, `seek_to`, tweening ([docs](https://docs.rs/kira/latest/kira/)). Runs on wasm via cpal but "static sounds cannot be loaded from files" and streaming is unsupported there ([README](https://docs.rs/crate/kira/latest)). No output-latency query; its position is the mixer's position, not the DAC's. Good library, but a second abstraction over the same cpal stream.
- **cpal 0.18.2**: on wasm the default backend is Web Audio (`ScriptProcessorNode`-style callback); the lower-latency `audioworklet` backend needs nightly, `-Zbuild-std`, atomics and COOP/COEP headers ([README](https://docs.rs/crate/cpal/latest)), so unusable on Pages. On desktop cpal exposes `StreamInstant`/callback timestamps including playback time, which is what web-audio-api-rs uses for `output_latency`.
- **rodio 0.22.2**: convenience layer, `Sink::get_pos()`; no hardware-clock pairing (and a thread reports `get_pos` running past the file's end: [users.rust-lang](https://users.rust-lang.org/t/rodio-sink-get-pos-gives-a-pos-beyond-the-duration-of-the-audio-file/131594)).
- **firewheel 0.14.0**: audio graph with a seconds clock that accounts for underruns, musical clock, `EventDelay` scheduling; cpal backend including wasm with the "don't spawn threads" caveat; latency reporting not documented. Promising, API still moving monthly.
- **oddio** (2023, dormant) and **tinyaudio 2.0** (raw output callback only) are not candidates.

### Recommended `AudioBackend` trait shape
```rust
pub struct SongTime { pub seconds: f64 }        // position in the song, heard-time
pub struct HostTime { pub seconds: f64 }        // monotonic host clock (performance.now()/Instant)

pub trait AudioBackend {
    type Buffer;
    async fn decode(&self, bytes: &[u8]) -> Result<Self::Buffer>;     // browser decodeAudioData / symphonia
    fn duration(&self, b: &Self::Buffer) -> f64;
    fn now(&self) -> AudioClockSample;        // { context_time, host_time, output_latency }
    fn play(&mut self, b: &Self::Buffer, start_at_context_time: f64, offset: f64, volume: f32) -> SoundId;
    fn stop(&mut self, id: SoundId);
    fn set_volume(&mut self, id: SoundId, v: f32);
    fn preview(&mut self, b: &Self::Buffer, offset: f64, len: f64, fade: f64) -> SoundId; // start(when, offset, duration) + gain ramp
    fn resume(&mut self);                     // user-gesture unlock on web
}
```
The engine never sees `AudioContext`; it gets `AudioClockSample`s and converts an input's `HostTime` to `SongTime` with the anchor math from section 1. On web `host_time` is `performance.now()/1000` and `now()` wraps `getOutputTimestamp()`; on desktop `host_time` is `Instant` since app start and `now()` wraps `current_time()`/`output_latency()` from web-audio-api-rs. Hit sounds (claps/assist tick) are scheduled with `play(buf, t_hit, 0.0)`, which is sample-accurate on both.

---

## 5. Native input

- **winit 0.30**: `KeyEvent { physical_key, logical_key, text, location, state, repeat }` has no timestamp ([docs](https://docs.rs/winit/latest/winit/event/struct.KeyEvent.html)); `WindowEvent`/`DeviceEvent` carry none either. Stamp with `Instant::now()` on receipt. To keep that stamp honest, pump events continuously (`ControlFlow::Poll` or a dedicated event thread where the platform allows) rather than once per rendered frame; otherwise input is quantised to the frame. Winit's Windows backend recently moved to high-resolution waitable timers (~0.5 ms) ([winit PR 3950](https://github.com/rust-windowing/winit/pull/3950)), which helps wake-up jitter.
- **gilrs 0.11.2**: `Event { id, event, time: SystemTime }` — a wall-clock time stamped by gilrs when it reads the event ([docs](https://docs.rs/gilrs/latest/gilrs/ev/struct.Event.html)); not a hardware timestamp, but if you poll gilrs on its own thread at 1 kHz the stamp is within ~1 ms of the OS event. Supports SDL_GameControllerDB mappings; dance pads appear as generic gamepads with buttons.
- **SDL3 (sdl3 0.20.0)**: every event has `timestamp` in nanoseconds from `SDL_GetTicksNS()` ([SDL wiki](https://wiki.libsdl.org/SDL3/SDL_CommonEvent)); on most platforms SDL stamps on receipt as well, but SDL's event pump is cheap to run at high frequency on a dedicated thread. A complete alternative to winit+gilrs for the desktop shell, with the downside of a C dependency.
- **rdev 0.5.3** (2023) and **device_query** are global hooks/polling libraries; useful only for a last-resort polling thread for keyboards, and rdev is dormant.

Low-latency rendering on desktop: `PresentMode::Mailbox` (or `Immediate` when tearing is acceptable) with a frame-pacing sleep so the frame is submitted as late as possible before vblank; avoid deep swapchain queues (`desired_maximum_frame_latency = 1`). Input thread → lock-free queue → engine, so judging never waits on rendering.

---

## 6. Asset/song storage and import in the browser

**Getting files in.**
- `<input type="file" webkitdirectory>` works in Chrome, Firefox and Safari and yields `File`s with `webkitRelativePath`; simplest cross-browser folder import.
- Drag and drop: `DataTransferItem.webkitGetAsEntry()` gives `FileSystemDirectoryEntry` for recursion (all major browsers); Chromium also offers `getAsFileSystemHandle()`.
- `window.showDirectoryPicker()`: Chromium only, secure context, needs user activation ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Window/showDirectoryPicker)); Mozilla has declined the writable File System Access API. Use it as an enhancement (persisted handles allow re-scanning a local pack folder without copying).
- Zips: `zip` (pure Rust with default `deflate` via miniz_oxide; disable `bzip2`/`zstd`/`time` features for wasm) or `async_zip` 0.0.19 both compile to wasm32; parse from an in-memory `Vec<u8>` or via `Cursor`. `rc-zip` is another sans-IO option. Zip of a 20-song pack with MP3s is ~100–200 MB in memory while extracting; stream entry-by-entry into storage.

**Keeping them.** OPFS (`navigator.storage.getDirectory()`) is Baseline since March 2023 ([MDN OPFS](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system)); web-sys exposes `FileSystemDirectoryHandle`, `get_file_handle`, `get_directory_handle`, `entries`, `remove_entry` on stable features ([docs](https://docs.rs/web-sys/latest/web_sys/struct.FileSystemDirectoryHandle.html)). Writing: `createWritable()` on the main thread (Chrome/Firefox; Safari gained it only recently and historically required `createSyncAccessHandle` in a Worker), so keep an IndexedDB-blob fallback (stores `File`/`Blob` directly; works everywhere). Quotas: Chrome ~60% of disk, Firefox 10 GiB best-effort (more with `persist()`), Safari ~60% but with the 7-day script-storage eviction for origins without interaction when tracking prevention is on ([MDN quotas](https://developer.mozilla.org/en-US/docs/Web/API/Storage_API/Storage_quotas_and_eviction_criteria)). Call `navigator.storage.persist()` after the first import.

**Decoding.** Let the browser decode (`decodeAudioData`): it is native-speed, off-thread, and produces an `AudioBuffer` whose PCM lives in browser memory, not in the wasm linear memory. A 3-minute stereo 48 kHz f32 song is 180 × 48000 × 2 × 4 B ≈ 69 MB; keep exactly one gameplay buffer alive, drop it when leaving the song, and never copy it into wasm. Symphonia 0.6.1 in wasm is the fallback for formats a browser rejects (Ogg Vorbis on older Safari) and the tool for parity with StepMania's MP3 behaviour (section 1); note Symphonia's Opus support is through a libopus adapter, not pure Rust ([Symphonia](https://github.com/pdeljanov/Symphonia)), so Opus-in-wasm would need a different crate. `lewton`/`minimp3` are superseded by Symphonia.

**Streaming instead?** `MediaElementAudioSourceNode` keeps memory low, but `HTMLMediaElement.currentTime` is coarsened (Firefox 2 ms, resistFingerprinting 100 ms: [MDN currentTime](https://developer.mozilla.org/docs/Web/API/HTMLMediaElement/currentTime)), start is not schedulable on the audio clock, and buffering stalls are invisible. Use `<audio>` only for song-select previews, where a 50 ms error is harmless. Alternative for previews with exact looping: `AudioBufferSourceNode.start(when, previewStart, previewLen)` on an already-decoded buffer plus a `GainNode` fade.

Recommendation: import → store original compressed files (OGG/MP3) + parsed chart JSON in OPFS (IDB fallback) → decode on song start → single `AudioBuffer` → `AudioBufferSourceNode`.

---

## 7. Text/UI for the in-game HUD

- **glyphon 0.12** (wgpu 30, cosmic-text 0.19): proper shaping/fallback, atlas cache, fine for score/combo that change every frame. Cost: cosmic-text + swash + fontdb adds roughly 1–2 MB wasm plus the font files you ship, and a frame of `prepare` work. You already use it; it is the natural choice for titles, scores, settings text in the wgpu scene.
- **cosmic-text alone**: only if writing your own atlas uploader; no reason when glyphon exists.
- **Pre-rendered atlas sprites** for judgement words ("FANTASTIC", "EXCELLENT"…) and combo digits: smallest, stylable (outline/glow), deterministic, identical on desktop and web, and no shaping needed. This is what StepMania themes do.
- **DOM overlay (Yew)**: absolutely-positioned HTML over the canvas gets CSS animations and fonts for free, but is composited on the browser's own schedule (fine at 60 Hz, can visibly lag the canvas by a frame), does not exist on desktop, and complicates fullscreen/letterboxing. Keep DOM for menus, song select, results and the calibration wizard; keep the gameplay HUD in the renderer.

---

## 8. Existing Rust rhythm games and simfile crates

| Project / crate | What | Licence | Status |
|---|---|---|---|
| [RGates94/rustmania](https://github.com/RGates94/rustmania) | StepMania/Etterna-like VSRG, .sm support | MIT | 14 stars, last push 2021-04 — dead but readable timing/judge code |
| [JacobLinCool/rhythm-rs](https://github.com/JacobLinCool/rhythm-rs) (`taiko-core`) | Taiko engine, TJA parser also as wasm | MIT | active (2026-07) |
| [lifthrasiir/angolmois-rust](https://github.com/lifthrasiir/angolmois-rust) | BMS player port | (BMS/original: GPL) | last push 2020; classic reference for BPM/stop timing |
| [premiering/soundaim](https://github.com/premiering/soundaim) | Sound Space clone in Bevy | see repo | Bevy reference |
| [semyon422/rizu](https://github.com/semyon422/rizu) | multi-format VSRG (.sm among others) | GPL-3 | active, but Lua/LÖVE not Rust |
| `rgchart` 0.0.15 ([repo](https://github.com/R2O3/rgchart)) | parse/write charts; `parse::from_sm_generic`, wasm target | MIT | 2026-06-19, 3 stars |
| `danceparser` 0.2.1 ([codeberg](https://codeberg.org/nobbele/danceparser)) | StepMania chart parser | MIT | 2026-06-07 |
| `msd` 0.4.0 | MSD (the `#KEY:VALUE;` container of .sm/.ssc) reader/writer | — | 2023-05 |
| `rotterna-lib` 0.2.0, `rhythm-open-exchange` 0.6.2 | .sm→osu conversion / "ffmpeg of VSRG" | MIT | 2025-11 / 2025-12 |
| `minacalc-sys` 515.2.0 | Etterna difficulty calculator bindings | — | 2026-08 |

No `stepmania-sm`, `sm-parser`, `simfile` or `smparser` crates exist on crates.io. The .sm/.ssc format is small enough to own: MSD tokenising (`#TAG:value;`, backslash escapes, `//` comments), `#OFFSET`, `#BPMS`, `#STOPS` (plus `#DELAYS`, `#WARPS`, `#SPEEDS`, `#SCROLLS` for .ssc), and `#NOTES` as measures of rows with 4 (dance-single) columns. Reference material: barrysir's annotated [stepmania-parsing-code](https://github.com/barrysir/stepmania-parsing-code) and the Python `simfile` library docs ([simfile.readthedocs.io](https://simfile.readthedocs.io/en/main/about-simfiles.html)). Use `rgchart`/`danceparser` as differential-test oracles. Mind the "ITG 9 ms bias": many legacy packs embed +9 ms in `#OFFSET` ([StepManiaOnline FAQ](https://stepmaniaonline.net/faq)); expose per-song offset editing rather than guessing.

---

## 9. Build and deploy

**Trunk.** 0.21.14 is the stable line (0.22.0-rc.1 on 2026-10-01). Needed pieces ([trunkrs.dev/assets](https://trunkrs.dev/assets/)): `<link data-trunk rel="copy-dir" href="assets/songs" />` for bundled demo songs, `rel="copy-file"` for `coi-serviceworker.js`-style scripts if ever needed, `rel="rust" data-wasm-opt="z" data-bindgen-target="web" data-weak-refs data-reference-types` to shrink and speed up the wasm. Public URL: `Trunk.toml [build] public_url = "/DitoDitoInfinito/"` or `trunk build --release --public-url /DitoDitoInfinito/`; Trunk also accepts a relative `./` public URL, which makes the same build work at the repo subpath and at a custom-domain root (a custom domain serves a project site at `/`, with no repo segment). GitHub serves the `user.github.io/Repo/` segment case-insensitively but files within the site case-sensitively ([devactivity](https://devactivity.com/insights/solving-github-pages-404s-a-case-sensitive-developer-tool-insight)); with Yew routing prefer a hash router or a `404.html` copy of `index.html`.

**Size budget.** Target ≤ 2 MB compressed for the first load: wgpu (both backends) ≈ 2–3 MB raw, glyphon+cosmic-text ≈ 1–2 MB raw, Yew + web-sys ≈ 0.3–0.5 MB raw; after `opt-level="z"`, `lto`, `codegen-units=1`, `panic="abort"`, `wasm-opt -Oz` and Brotli this is realistic. Split fonts and song assets out of the wasm; load songs lazily.

**No COOP/COEP on GitHub Pages.** Custom headers cannot be set ([GitHub community #13309](https://github.com/orgs/community/discussions/13309)), so the page is never cross-origin isolated: no `SharedArrayBuffer`, no wasm threads, no cpal `audioworklet` backend, 100 µs timers in Chrome. Implications: (1) you lose nothing essential; `AudioBufferSourceNode.start(when)` already gives sample-accurate playback and hit-sound scheduling; (2) `AudioWorklet` itself does not require SAB — a worklet can load its own wasm instance and talk via `MessagePort` ([PaulBatchelor/rust-wasm-audioworklet](https://github.com/PaulBatchelor/rust-wasm-audioworklet)), but wasm-bindgen glue in `AudioWorkletGlobalScope` is awkward (no `TextDecoder`, separate module), so only do this later for custom DSP; (3) `coi-serviceworker` can fake isolation by reloading once through a service worker, but it is fragile and not needed.

**Firefox/Safari.** Firefox: 2 ms event-timestamp precision, no `showDirectoryPicker`, WebGPU still off on Linux (webgl2 fallback is mandatory), `resistFingerprinting` detection. Safari: autoplay unlock and context interruptions, 44.1 kHz contexts, Ogg support only from 18.4 and shaky (prefer WebM/Opus or MP3 with Info tag, or Symphonia fallback), `getOutputTimestamp` sanity check, rAF historically 60 Hz, 7-day storage eviction, WebGPU only from Safari 26.

---

## 10. Recommended architecture

### Crate layout (Cargo workspace, edition 2024, Rust 1.98)
- `chart` — .sm/.ssc (MSD) parser and the chart model: `TimingData` (offset, BPM segments, stops, delays, warps) with exact beat↔second conversion, `Note { beat, lane, kind }`, difficulty metadata. Pure, serde, fuzzed, differential-tested against `rgchart`/`danceparser`. No I/O.
- `engine` — gameplay core: `SongClock` (anchor filtering, latency compensation), `Judge` (ITG/DDR window sets, hold/roll/mine logic), `Scoring`, `Scroll` (speed mods, note positions as a pure function of song time). Input is `InputEvent { lane, pressed, host_time }`; output is a `Frame` snapshot for rendering plus `JudgementEvent`s. Deterministic, `no_std + alloc` friendly, replay-driven unit tests (feed a recorded input log and song-clock samples, assert judgements).
- `render` — wgpu 30 scene: texture-atlas instanced quads (arrows, receptors, explosions, judgement sprites, digits), optional glyphon text; takes `&wgpu::Device/Queue`, a target view, a `Frame`. No platform code.
- `platform` — traits only: `AudioBackend`, `InputSource`, `HostClock`, `SongStore` (list/import/load bytes), `WindowSurface` sizing.
- `platform-web` — web-sys implementations: `WebAudioBackend` (AudioContext + getOutputTimestamp anchor), keyboard/gamepad (`MessageChannel` poll loop), OPFS/IDB store, `webkitdirectory`/drag-drop/zip import, canvas sizing. `app-web` binary: Yew 0.23 for menus/song select/results/calibration, a canvas component hosting `render`.
- `platform-desktop` — winit 0.30 + wgpu + `web-audio-api` 1.7 (`current_time`/`output_latency`/`start_at_with_offset`) + gilrs on a 1 kHz thread; `app-desktop` binary. SDL3 is the alternative if winit's lack of timestamps proves to be a problem.

### Audio + clock design
Single master clock = audio context time. All inputs are stamped in host time at the earliest point available (`event.timeStamp`, poll time with `Gamepad.timestamp` when it moves, `Instant::now()` on desktop input thread), converted to song time via the anchor `(context_time, host_time)` refreshed every frame from `getOutputTimestamp()` (web) or `current_time()`+`Instant` (desktop), low-pass filtered and drift-corrected at a bounded rate. Heard time = context time − output latency − user audio offset; rendered time = song time at predicted presentation + user visual offset. The song starts with ~100 ms lookahead via `start(when, offset)`. A calibration screen measures audio offset (tap to metronome) and visual offset (tap to a moving marker with sound muted) independently and stores both per output device profile.

### MVP choices
- Rendering: wgpu 30 (`webgpu` + `webgl` features), atlas sprites, glyphon for dynamic text; Yew/DOM for everything outside gameplay.
- Audio: Web Audio via web-sys, browser `decodeAudioData`, `AudioBufferSourceNode`; Symphonia fallback deferred until a format actually fails.
- Input: keyboard with `event.code` + `timeStamp`; Gamepad via a `MessageChannel` 1 ms poll loop; WebHID and touch later.
- Storage: OPFS with IDB fallback; import via `webkitdirectory`, drag-drop, zip (`zip` crate, deflate only); `showDirectoryPicker` as a Chromium bonus.
- Charts: own `chart` crate; bundle a couple of CC-licensed .sm songs via `copy-dir`.
- Build: Trunk 0.21.14, `wasm-opt -Oz`, relative public URL, GitHub Actions deploy.
- Desktop (phase 2): winit + wgpu + web-audio-api + gilrs, reusing `engine`/`render`/`chart` unchanged.

### Main risks
1. Safari: codec gaps, `getOutputTimestamp` regressions, 44.1 kHz contexts and context suspension; mitigated by the sanity check and Symphonia fallback, but Safari will be the long tail of bug reports.
2. Gamepad polling on the main thread competes with rendering; a busy frame delays polls. Keep the game loop light and consider lowering render work before input work.
3. MP3 decoder-delay mismatch versus StepMania-synced packs (up to ~25 ms) — must ship per-song offset editing and a known-good decoder path or players will think the game is "off".
4. Memory: one ~70 MB `AudioBuffer` plus textures is fine on desktop browsers, tight on mobile Safari.
5. Firefox `resistFingerprinting`/2 ms timestamps and Linux WebGPU gaps — detect and message rather than silently degrade.
6. Ecosystem churn: wgpu majors every few months (29→30 in 2026), winit 0.31 beta, Trunk 0.22 pending; pin versions and upgrade deliberately.
7. GitHub Pages' missing COOP/COEP is a permanent ceiling (no threads); the design above never needs them, but keep it that way.
