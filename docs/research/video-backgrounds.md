# Video backgrounds: decoding in the browser and on the desktop (2026-10-09)

Scope: play the movies that `#BGCHANGES` name (StepMania packs) in the web
shell, with a path that the desktop shell shares. Same conventions as the
other notes: sources cited, local third-party packs used only for numbers.

## 1. What packs ship

StepMania accepts these movie extensions: `avi`, `f4v`, `flv`, `mkv`, `mp4`,
`mpeg`, `mpg`, `mov`, `ogv`, `webm`, `wmv` (`ActorUtil::InitFileTypeLists`,
`src/ActorUtil.cpp`, 5_1-new). What it can then play is whatever its FFmpeg
build decodes (`MovieTexture_FFMpeg.cpp` opens any codec `avcodec` knows and
reports "Unsupported codec" otherwise), so in practice packs carry whatever
the authors' encoders produced.

A local library of 24 DDR/ITG-era packs (6.9 GB) holds 188 movies, all named
`.avi`, 4.5 GB in all, measured with `ffprobe` on 2026-10-09:

| Container | Video codec | Files | Typical size | Frame rate |
|---|---|---|---|---|
| AVI (RIFF) | H.264 (fourcc `H264`, Main profile; 6 High, 1 `avc1`) | 94 | 640×360, 640×480, 852×480; 4 at 1280×720 | 30, a few 29.97/25 |
| AVI | MPEG-4 Part 2 (fourcc `XVID`, `DX50`; Advanced Simple and Simple profiles) | 70 | 320×240 to 640×480 | 30 |
| raw MPEG-2 video stream (not AVI at all: starts with the sequence header `00 00 01 B3`) | MPEG-2 | 24 | 640×360 | 30 |

So "video backgrounds" means, for the packs at hand: demux AVI or a raw MPEG
stream, decode H.264, MPEG-4 Part 2 or MPEG-2, ignore the audio track (as
StepMania does), at SD resolution and 30 fps.

## 2. What browsers can do

**`<video>`:** no browser plays the AVI container or MPEG-4 Part 2 or
MPEG-2, so the straightforward route is closed for every file above.

**WebCodecs `VideoDecoder`:** hardware (or the OS's) decoding of compressed
frames the page hands it. Supported in Chrome and Edge 94+, Firefox 130+
(desktop; not Firefox for Android), Safari 16.4+ for video (full WebCodecs
from Safari 26) ([caniuse](https://caniuse.com/webcodecs)). Codecs are what
the platform offers: H.264 everywhere; MPEG-4 Part 2 and MPEG-2 nowhere.
H.264 needs `codec: "avc1.PPCCLL"`; with no `description` the stream is
Annex B, with SPS/PPS in band, which is what AVI carries
([WebCodecs AVC registration](https://www.w3.org/TR/webcodecs-avc-codec-registration/)).
`isConfigSupported()` tells before `configure()`. Works in dedicated
workers ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/VideoDecoder)).
A decoded `VideoFrame` is transferable between a worker and the page and
must be `close()`d, since frames come from a small pool
([Chrome, WebCodecs](https://developer.chrome.com/articles/webcodecs)).

**Getting a frame onto the GPU:** `copyExternalImageToTexture` takes a
`VideoFrame` (Chrome 116+, [WebCodecs integration](https://developer.chrome.com/blog/new-in-webgpu-116)),
and wgpu's `ExternalImageSource` has a `VideoFrame` variant on both its
WebGPU and WebGL2 backends (`wgpu_types::texture::external_image`;
`wgpu_hal::gles::queue` uploads it with `texSubImage2D`), with colour
conversion from YUV done by the browser. The app already uploads background
images this way (`ImageBitmap`, `app/src/game_loop.rs`), so a video frame is
the same call with a different source.

**Software decoding in wasm:** FFmpeg builds with Emscripten; ffmpeg.wasm's
build (`build/ffmpeg.sh`, `build/ffmpeg-wasm.sh`) uses
`--target-os=none --arch=x86_32 --enable-cross-compile --disable-asm
--disable-programs --disable-doc --disable-runtime-cpudetect
--disable-autodetect` and links with `-sMODULARIZE -sENVIRONMENT=worker
-sINITIAL_MEMORY=32MB -sALLOW_MEMORY_GROWTH` for the single-threaded core.
The published cores are 25–31 MB because they include encoders and GPL
libraries; a build with `--disable-everything` plus only the needed
decoders, parsers and demuxers is far smaller (one such browser build with
a handful of codecs was 1.8 MB gzipped,
[alfg, FFmpeg + WebAssembly](https://dev.to/alfg/ffmpeg-webassembly-2cbl));
the exact size is measured, not assumed (section 6). Threads need
`SharedArrayBuffer`, which needs the COOP/COEP headers GitHub Pages cannot
set ([community discussion 13309](https://github.com/orgs/community/discussions/13309));
a service worker can fake them, at the cost of a reload and of breaking
cross-origin loads. A single-threaded decoder in one Web Worker needs none
of that and keeps the game loop untouched. Emscripten's `WORKERFS` mounts a
`File`/`Blob` for synchronous reads without copying it into wasm memory
([Emscripten file system API](https://emscripten.org/docs/api_reference/Filesystem-API.html)),
which fits linked folders (decision 33) and zips alike. Wasm SIMD
(`-msimd128`) is in every current browser; FFmpeg has started adding
`simd128` kernels (HEVC IDCT, 2024), not yet for these codecs, so the gain
there comes from autovectorised C.

**Patents** (for the record; the game plays files the player already
owns): MPEG-2's last patents ran out in 2018 except in Malaysia
([Via LA MPEG-2 list](https://www.via-la.com/licensing-2/mpeg-2/mpeg-2-patent-list/));
MPEG-4 Visual's last patent expired in July 2026
([It's FOSS](https://itsfoss.com/news/mpeg-4-visual-patent-expiry/)); H.264's
US patents run to about the end of 2027 and the pool is now Via LA. FFmpeg's
own position is that it does not know whether it uses patented algorithms
([FFmpeg legal](https://ffmpeg.org/legal.html)).

## 3. How fast must it be

Single-threaded native decode of files from the local library, `ffmpeg
-threads 1 -an -f null`, Apple Silicon laptop, 2026-10-09:

| File | Frames | CPU time | Per frame |
|---|---|---|---|
| MPEG-4 Part 2 (Xvid) 320×240 | 3115 | 0.31 s | 0.10 ms |
| MPEG-4 Part 2 (Xvid) 640×360 | 2735 | 1.20 s | 0.44 ms |
| MPEG-2 640×360 | ~3000 | 1.04 s | 0.35 ms |
| H.264 852×480 | 3649 | 4.92 s | 1.35 ms |
| H.264 1280×720 | 3295 | 6.14 s | 1.86 ms |

Published wasm-versus-native ratios range from 1.45–1.55× on SPEC-style
code ([Jangda et al., USENIX ATC 2019](https://www.usenix.net/system/files/atc19-jangda.pdf))
to 5–10× on codec code without hand-written SIMD. Taking 5× as the planning
figure: 2 ms a frame for Xvid and MPEG-2 at 640×360, 7 ms for H.264 at
852×480 and 10 ms at 720p, against a 33 ms frame period at 30 fps, in a
worker that shares no time with the game loop. That is comfortable for the
MPEG codecs and acceptable for H.264 as a fallback; H.264 should go through
WebCodecs where it exists, which also skips the pixel copy. The one cost
left on the main thread is the texture upload (a 640×360 RGBA frame is
0.9 MB; at 30 fps, 28 MB/s), which the background path already pays for
image crossfades.

## 4. How StepMania times a movie

- A movie starts at its change: when a `#BGCHANGES` segment becomes
  current the actor is reset and advanced by `song time − segment start`,
  so seeking into a segment lands mid-movie (`Background.cpp`,
  "How much time of this BGA have we skipped?").
- The clock is `m_fClock += dt × rate`; a frame shows once decoded and its
  timestamp (packet DTS × time base) is reached; one frame per update; a
  frame that failed to decode is skipped; looping rewinds (clock set to
  0.5 s, a hack for preview audio); non-zero seeks are "unsupported;
  ignored" (`MovieTexture_Generic.cpp`, `MovieTexture_FFMpeg.cpp`).
- The audio stream is discarded; the first frame is always shown; decoding
  runs on its own thread ahead of display.
- A missing layer-1 movie falls back to a random movie from the shared
  folders, then to the song's static background (decision 19 already
  resolves shared folders).

## 5. Design

**One pipeline, two decoders, loaded only when needed.**

1. **Import sniffs the codec.** The first kilobytes of a movie say what it
   is: an AVI's `strh`/`strf` chunks carry the fourcc (`H264`/`avc1`,
   `XVID`/`DX50`/`xvid`, …); a raw MPEG stream starts with `00 00 01 B3`;
   MP4/MKV have their own markers. Each song's manifest entry records its
   movies with their codec (`bg_videos: [(name, codec)]`), the way
   `bg_images` records images. The list then knows, without opening any
   file, whether a song needs WebCodecs (H.264) or the software module
   (MPEG-4 Part 2, MPEG-2, and H.264 where WebCodecs is missing). Shared
   folders' movies are sniffed the same way at import.
2. **The software module is a second wasm, fetched on demand.** A
   cut-down FFmpeg (`avformat` with `avi` and `mpegvideo` demuxers and the
   `h264`, `mpeg4video`, `mpegvideo` parsers; `avcodec` with the `h264`,
   `mpeg4`, `mpeg2video` decoders; `swscale` only if frames are converted
   in the worker; nothing else) built with Emscripten in a Docker step of
   CI from a pinned FFmpeg release and a checked-in configure line, output
   copied into the site by Trunk (`rel="copy-file"`). It is loaded by a Web
   Worker the first time a song that needs it starts (the start prompt
   shows "loading the video decoder" with the download's progress), never
   before, and never at all for a library without such songs; the
   browser's HTTP cache keeps it afterwards. No shared memory, no COOP/COEP.
   The song list may prefetch it in the background once it sees a song
   that needs it; that is a refinement, not a requirement.
3. **WebCodecs for H.264 where it exists.** A small Rust AVI demuxer
   (`library`, natively tested, like the zip reader) splits the stream
   into Annex B access units with timestamps for `VideoDecoder`. Where
   `isConfigSupported` says no (Firefox for Android, old Safari), H.264
   goes to the software module like the others.
4. **One `VideoDecoder` trait in `platform`:** configure with a codec and
   a file, pull decoded frames with timestamps. Web: the worker (software)
   or WebCodecs; desktop: FFmpeg linked natively (`ffmpeg-next` 9.0.0
   covers FFmpeg 3.4–8.0, LGPL, dynamic linking per FFmpeg's checklist),
   which also demuxes, so the Rust demuxer is only the WebCodecs feeder.
5. **The game loop schedules frames as it schedules images.** The
   background schedule (`library::backgrounds`, decision 18) gains movie
   segments; the segment's start second and the `rate` field drive a
   movie clock exactly as StepMania's, from the audio clock through the
   same `render_time` the notes use (AGENTS.md's timing model: the audio
   clock is authoritative, nothing here touches judging). The loop keeps
   two or three decoded frames ahead, shows the latest frame whose
   timestamp has been reached, drops frames it is behind on, and loops
   when the movie ends before the next change (StepMania's default).
   Crossfades between a movie and an image reuse the existing mix.
6. **Frame hand-over.** WebCodecs: the `VideoFrame` itself is transferred
   to the page and uploaded with `copy_external_image_to_texture`
   (both backends; the browser converts), then closed. Software: the
   worker hands over the decoded YUV 4:2:0 planes in one transferred
   buffer and the renderer converts in the background shader (three
   single-channel textures, BT.601 or BT.709 and limited or full range
   from the frame's own tags). Measured 2026-10-09: `sws_scale` to RGBA
   costs 0.2 ms a frame at 640×360 and 0.5 ms at 852×480 (a fifth of the
   decode), but the planes are 1.5 bytes a pixel against 4, so the
   hand-over and the texture upload on the game loop's thread shrink
   2.7× (10 MB/s instead of 28 MB/s at 640×360, 30 fps); the desktop
   build gets the same planes from FFmpeg, so the shader is shared.
   Decided: YUV planes for the software path, RGBA copies for WebCodecs.
7. **Setting: Video = auto | on | off** (auto default). The loop already
   measures frames per second and the worst frame time of each second
   (`FpsMeter`). Under auto, a movie is stopped for the rest of the song,
   with a notice on the results ("video turned off for this song: frames
   were being dropped") and a link to the setting, when the display drops
   below a threshold for several seconds while a movie plays, or when the
   decoder reports it cannot keep up with the song clock (it falls more
   than a few frames behind twice). Judging is unaffected either way, so
   the rule errs towards keeping the video. Memory and storage: linked
   imports (decision 33) read movies from disk; copying browsers never
   copy movies unless an import option asks, since they are most of a
   pack's size.
8. **Licensing.** FFmpeg stays LGPL (no `--enable-gpl`, no
   `--enable-nonfree`); the module is a separate file the player's browser
   fetches, replaceable like a shared library; `LICENSES/LGPL-2.1-or-later`
   is added, `REUSE.toml` lists the module, the Credits screen names FFmpeg
   with the version and a link to the source and the configure line, as
   the legal page asks. The desktop build links ffmpeg dynamically and
   ships the same notice.

**Left out:** audio tracks of movies (StepMania ignores them); non-AVI
containers through WebCodecs (the software module handles them if their
demuxers are enabled; MP4 can come later); WebGL2-only browsers without
WebCodecs get the software path for everything; `#BGCHANGES2`/`#FGCHANGES`
overlays are a separate step that reuses the same frames.

## 6. Toolchain for the software module

Two ways to compile FFmpeg to wasm, both upstream Clang/LLVM underneath:

- **Emscripten** (`emcc`): clang plus a musl-based libc, system libraries
  and JavaScript glue. Its old forked backend ("fastcomp", LLVM 6) was
  replaced by the upstream LLVM wasm backend in 1.39.0 (October 2019) and
  removed in 2.0.0 (August 2020)
  ([Emscripten, Building to WebAssembly](https://emscripten.org/docs/compiling/WebAssembly.html));
  releases continue (6.0.x in 2026). Its libc has no-op pthread stubs for
  single-threaded builds, so current FFmpeg (which assumes a threading API
  even when built with `--disable-pthreads`) compiles without shared
  memory; `WORKERFS` reads a `File` in place. Output: a `.wasm` plus a glue
  `.js` (`-sMODULARIZE -sENVIRONMENT=worker`). Official Docker image
  (`emscripten/emsdk`) for a reproducible CI build.
- **wasi-sdk**: upstream clang targeting `wasm32-wasi` with wasi-libc, no
  glue; the module imports about a dozen WASI calls (`fd_read`, `fd_seek`,
  `clock_time_get`, …) that the worker implements itself, reading the
  movie through `FileReaderSync`. FFmpeg builds this way
  ([FFmpeg-WASI](https://github.com/SebastiaanYN/FFmpeg-WASI); the
  `ffmpeg-wasi` crate), but the builds found pin FFmpeg 5.1 "because newer
  versions depend on threads"
  ([go-ffmpreg](https://pkg.go.dev/codeberg.org/gruf/go-ffmpreg)); wasi-libc
  offers pthreads only through its threads target, which needs shared
  memory, which needs COOP/COEP. Some builds also import `setjmp`/`longjmp`
  (from the command-line program, probably not from the decoders).

Neither can be folded into the app's own wasm: FFmpeg needs a libc, and
`wasm32-unknown-unknown` has none; a separate module is what the on-demand
load wants anyway.

**Tried on 2026-10-09: wasi-sdk 34 (clang 23) builds FFmpeg 9.0.2
single-threaded with no source changes.** `configure --target-os=none
--arch=wasm32 --enable-cross-compile --disable-everything
--disable-programs --disable-doc --disable-network --disable-pthreads
--disable-w32threads --disable-os2threads --disable-runtime-cpudetect
--disable-autodetect --disable-avdevice --disable-avfilter
--disable-swresample --enable-decoder=h264,mpeg4,mpeg2video
--enable-parser=h264,mpeg4video,mpegvideo --enable-demuxer=avi,mpegvideo
--enable-protocol=file --extra-cflags=-msimd128` (FFmpeg 9 knows
`arch=wasm32` and its `simd128` kernels) configures and builds the four
libraries cleanly, LGPL 2.1+, with the expected warning that the build
must not be used from several threads. Linking a 120-line C shim (open
through libavformat, decode, `sws_scale` to RGBA, next frame with its
timestamp) needs one stub: wasi-libc declares `clock()` but does not
define it, and `libavutil`'s random seed calls it. The result is a plain
reactor module (`-mexec-model=reactor`, `--no-entry`) with 20 imports, all
`wasi_snapshot_preview1` (`fd_read`, `fd_seek`, `path_open`,
`clock_time_get`, …), and no JavaScript of its own:

| Build | `.wasm` | gzipped | Xvid 640×360 | MPEG-2 640×360 | H.264 852×480 | H.264 1280×720 |
|---|---|---|---|---|---|---|
| `-O3`, stripped | 3.22 MB | 877 KB | 0.87 ms/frame | 1.00 | 2.83 | 4.16 |
| `--enable-small -Oz`, stripped | 1.53 MB | 627 KB | 0.97 | | 3.15 | |

Decode time per frame includes the RGBA conversion, measured under Node's
WASI (V8, the engine Chrome uses), 900 frames of each local file, single
thread; 2–2.5× the native single-thread times of section 3, well under
the 33 ms frame period. Decoded frames were saved as PNG and look right.
Emscripten was not needed and not tried.

Decision: **wasi-sdk**, the `--enable-small -Oz` build (627 KB over the
wire, 10 % slower and still twenty times faster than needed). The module
exposes the thin interface above so the toolchain behind it could still
change. The build is a script that downloads the pinned wasi-sdk tarball
and FFmpeg source and runs the configure line above, the same on a Mac and
in CI; no Docker or container is involved. The worker implements the 20
WASI calls itself: files map to the song's `File`/`Blob` read with
`FileReaderSync`, the clock to `performance.now()`, the rest return
`ENOSYS`.

### The app's own target

The app stays on `wasm32-unknown-unknown`: it is the only target
wasm-bindgen supports, and everything the app does (DOM, WebGPU, Web
Audio, gamepads, IndexedDB, folder handles) goes through wasm-bindgen's
bindings. WASI targets (`wasm32-wasip1`/`wasip2`) replace the libc layer,
not the browser bindings; in a browser they need a shim and still give no
DOM. The two modules never link: the app talks to the decoder worker with
messages and transferred buffers, so the decoder's toolchain is free to
differ from the app's.

Target features are a separate lever. Rust 1.99 enables `bulk-memory`,
`multivalue`, `mutable-globals`, `nontrapping-fptoint`, `reference-types`
and `sign-ext` by default; `simd128` is not on. Measured 2026-10-09 with
`-C target-feature=+simd128` (and `--enable-simd` for wasm-opt): the
module shrank from 4,504,048 to 4,474,841 bytes, and the wasm Ogg Vorbis
fallback decode of a 2:40 song took 455–470 ms both with and without
(headless Chrome, three runs each, twice). No gain for the Rust code as it
is, against losing browsers before Safari 16.4 (2023), so it stays off;
two lines of configuration turn it on when some code can use it. The
FFmpeg module is C with vectorisable loops and does get `-msimd128`.

## 7. To measure before building on it

- Decode time per frame in Firefox and Safari (SpiderMonkey and
  JavaScriptCore) for the three codecs, against the V8 numbers in section
  6; and the module instantiating from a worker in all three.
- `VideoDecoder` output latency and queue depth for H.264 from AVI (Annex
  B, no `description`) in the three browsers.
- Texture upload cost of a 640×360 RGBA frame at 30 fps on WebGL2.
