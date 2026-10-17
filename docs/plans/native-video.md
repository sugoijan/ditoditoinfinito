# Plan: OS and native video decoding on desktop (2026-10-17)

The maintainer asked whether desktop could decode background movies
natively instead of running the web's FFmpeg module under wasmtime
(`docs/plans/desktop.md` D8, 7.0, 7.7). They named `ffmpreg`, `rsmpeg`
"and other projects", and floated one design: a Rust shim crate in place
of `video/ddivideo.c`, built as a `cdylib` for wasm (the web unchanged)
and used natively through a safe Rust API. This document surveys the
candidates, tries that design, measures native FFmpeg against the
module on this machine, and proposes what to do. The original spike left the
production decoder unchanged: the native spike is in
`/home/user/.claude/jobs/2f6f2abd/tmp/spike-native-video/`. The 2026-10-17
VideoToolbox continuation is in the `next` worktree
(`/home/user/sugoijan/ditoditoinfinito-next/target/scratch/vt/`); its
source and measurement logs are also handed over in the main checkout
as `target/scratch/video-spike-2026-10-17.tar.gz`, without movie files.

Read first: `docs/plans/video-backgrounds.md` (why the module exists,
its exact seeks, StepMania's frame times), `docs/plans/desktop.md` (D8,
7.0's and 7.7's numbers and hardening), `video/FFMPEG.toml`,
`video/ddivideo.c`, `desktop/src/video.rs`.

## Direction

- **Native FFmpeg is about 3× faster than the module under wasmtime,
  frame for frame, on one thread.** With four decoder threads it is
  about 10× faster. Exact seeks into long H.264 GOPs are 3–5× faster.
  1080p H.264 drops from 17.6 to 5.5 ms a frame (CPU time). Every
  pack movie fits the module's budget today; native buys headroom,
  not playability.
- **The output is the same to the eye.** Frame times are identical on
  every movie, and so is every seek. Pixels are identical for every
  codec except those that use FFmpeg's IDCT (MPEG-1/2/4, DivX 3,
  FLV1, MJPEG). There, native's NEON IDCT rounds differently from the
  module's C one: at worst 65 dB PSNR on real movies, invisible. No
  public FFmpeg option makes them byte-identical on every stream (see
  "Agreement" in section 4), so native checks need a PSNR bound for
  those codecs.
- **The maintainer's design works.** A Rust port of the shim over
  `ffmpeg-sys-the-third` builds both natively and as a wasm32-wasip1
  `cdylib`. The `cdylib` has the module's exact 20 WASI imports and 16
  exports, and the same frames on all 29 test clips and the pack
  movies. It is 0.5 % larger after wasm-opt, and it runs under today's
  wasmtime host unchanged. It needs bindgen (libclang) and three link
  tricks. It does not make the native side any safer: the unsafe code
  is FFmpeg, not the shim.
- **`rsmpeg` is out.** 0.18.0 is for FFmpeg 8.0, does not compile
  against 9.0, and links all seven FFmpeg libraries. **`ffmpreg` is
  unrelated:** a WAV/PCM toy with no video codecs. **No pure-Rust or
  OS decoder covers the codec list.**
- **The real cost of native is the sandbox.** The module confines a
  memory-safety bug in any of 264 decoders to its linear memory, under
  a deadline. Native FFmpeg in the game's process would give such a
  bug the user's account. 2025 alone brought heap overflows in two
  decoders we enable (SANM, JPEG 2000).
- **Decision 50: OS decoders first.** Keep the module as the shipped
  baseline and fallback. The first production VideoToolbox path covers
  checked H.264 AVI (below); extend it, then add Media Foundation, for
  streams whose pixels, timestamps and seeks pass the same checks. The
  M1 Max spike hardware-decodes H.264, HEVC and VP9 at much less CPU
  cost than the module; codec names alone do not establish support for
  every stream. A sandboxed native FFmpeg child comes later for the
  remaining formats. Keep `ddivideo.c` as the shared FFmpeg shim.

## 1. What the decoder must do (unchanged)

- **Formats:** every format StepMania 5.1 plays. That is its FFmpeg's
  every built-in video decoder, demuxer and parser
  (`video/FFMPEG.toml`, 264 decoders), and no audio. Packs ship
  Xvid/DivX 4–5, DivX 3, MPEG-1/2 and H.264 in AVI, and now and then
  MP4, MPEG-PS, Matroska, WMV or FLV. The local library here has 312
  movies: 179 H.264, 100 MPEG-4 Part 2, 24 MPEG-2 and 9 DivX 3.
- **Timing and output:** all of it lives in `video/ddivideo.c`; the
  hosts only move bytes.
  - StepMania's frame times: each frame at the dts of the packet that
    released it.
  - Exact seeks: a seek lands on the frame that decoding straight
    through gives, byte for byte.
  - The end, and `ddi_last_time`.
  - The black-bar probe (`ddi_seek_key`).
  - 8-bit 4:2:0 planes, with the matrix and range.
- **The same pixels in both shells** where the codec allows it, so the
  checks (`video/check.mjs`, `--video-check`,
  `desktop/tests/video.rs`) compare one decoder with itself.
- **Licence:** FFmpeg stays LGPL-2.1-or-later (no `--enable-gpl`, no
  `--enable-nonfree`), replaceable by the user, with its notices and
  source shipped (decision 35).
- **Untrusted input:** movies come from third-party packs downloaded
  from anywhere, so the decoder parses untrusted input.

## 2. Candidates (crates.io and upstream, checked 2026-10-16)

### FFmpeg bindings

| Candidate | Latest | Licence | FFmpeg | Linking | Verdict |
|---|---|---|---|---|---|
| `rsmpeg` (larksuite) | 0.18.0+ffmpeg.8.0 (2025-08-24) | MIT | 6.x–8.0. PRs for 8.1 and 9.0 are open, unmerged | through `rusty_ffmpeg`: pkg-config, vcpkg, `FFMPEG_LIBS_DIR` (static), `FFMPEG_DLL_PATH` (dynamic) | **No.** Tried: against 9.0.2's headers it fails to compile (`AVCodec`'s removed `supported_framerates`/`pix_fmts`/`sample_fmts`/`ch_layouts`, `av_parser_init`'s argument), and it needs avfilter, avdevice and swresample (not feature-gated). No release in 14 months |
| `rusty_ffmpeg` (CCExtractor) | 0.17.0+ffmpeg.8.1 (2026-04-10) | MIT | 4.x–8.1. 9.0 PR open | as above; bindgen, or a pre-generated `FFMPEG_BINDING_PATH` | Raw bindings only. Its build script links all seven libraries (`LIBS`), so a cut-down build needs stub libraries or a fork |
| `ffmpeg-sys-the-third` / `ffmpeg-the-third` | 6.0.0+ffmpeg-9.0 (2026-08-09) | WTFPL | 5.1–9.0 | `FFMPEG_DIR`, pkg-config, vcpkg (MSVC), `static`, `build` (configure from git) | **The binding to use.** Per-library features (`avcodec`, `avformat`, `swscale` only). Tried: builds natively and for wasm32-wasip1 (section 3). Needs libclang (bindgen and `clang` crates) at build time |
| `ffmpeg-next` / `ffmpeg-sys-next` | 9.0.0 / 9.0.1 (2026-10-09) | WTFPL | 3.4–9.0 | as above | Works too. Officially in maintenance mode, 95 open issues and PRs; its safe API has known soundness gaps |
| `video-rs` | 0.12.0 (2026-09-09) | MIT OR Apache-2.0 | 9 (via ffmpeg-next) | inherited | Too high-level: no packet dts, no exact-seek control |
| `ac-ffmpeg` | 0.19.0 (2025-05-16) | MIT | 4.x–7.x | own C shim, no bindgen | No 8.x or 9.x |
| `ffmpeg-sidecar` | 2.6.0 (2026-09-30) | MIT | any `ffmpeg` binary | runs the CLI in a child process | Its downloads are GPL builds; raw video over a pipe loses our frame times and seeks. Only the child-process idea is useful (section 5) |

### Not FFmpeg

| Candidate | Latest | Licence | Coverage | Verdict |
|---|---|---|---|---|
| `ffmpreg` | 0.1.2 (2025-12-11); git v0.2.0 | Apache-2.0 | WAV, PCM, ADPCM, Y4M. **No video codec** | Unrelated despite the name |
| NihAV (nihav.org) | git only | AGPL-3.0 | H.264, partial MPEG-4 ASP, VP3–8, FLV. No DivX 3, MPEG-1/2, WMV, VP9, ASF, MPEG-PS | AGPL, and incomplete |
| oxideav (`oxideav-h264` 0.1.9, `-mpeg4video` 0.1.8, `-mpeg12video` 0.0.13, `-msmpeg4` 0.0.10, `-avi` 0.0.10) | 2026-04 to 2026-10 | MIT | claims nearly all of ours | Six months old, self-reported conformance, almost no users. Worth a conformance run in a year, not a dependency now |
| `openh264` | 0.9.9 (2026-10-15) | BSD-2-Clause (Cisco's binary has its own patent licence) | H.264 only; Main/High not conformant | No |
| macOS VideoToolbox (`objc2-video-toolbox` 0.3.2) | 2025-10-04 | Zlib OR Apache-2.0 OR MIT | H.264 and HEVC in hardware; MPEG-4 Part 2 and MPEG-2 unverified on Apple Silicon; no DivX 3, WMV, VP6; AVFoundation does not open AVI | Possible for H.264 only, fed by `library/src/avi.rs` as WebCodecs is on the web |
| Windows Media Foundation (`windows` 0.62.2) | 2025-10-06 | MIT OR Apache-2.0 | H.264, MPEG-4 Part 2 and WMV candidates; other codecs depend on installed extensions, edition and hardware | Runtime probing and fidelity checks required; section 7 |
| Linux | | | hardware APIs only (VA-API, V4L2) | No software fallback |
| GStreamer (`gstreamer` 0.25.4) | 2026-09-21 | bindings MIT/Apache; GStreamer LGPL | old codecs come from gst-libav, which is FFmpeg | A large runtime to ship for no codec gain |
| libmpv (`libmpv2` 6.0.0), libVLC | | LGPL / GPL | players, not frame sources | No |

**Conclusion:** decoding natively means FFmpeg natively. OS decoders
can at most take H.264 off it, as WebCodecs does on the web.

## 3. The maintainer's design, tried

**The idea.** A Rust crate replaces `video/ddivideo.c`. It is built as
a `cdylib` for wasm32-wasip1, which becomes the web's module with the
worker unchanged, and used natively as an rlib with a safe API.

**Crate types.** A Rust `dylib` has Rust's unstable ABI and is only for
other Rust crates, so it does not fit here. A `cdylib` exports a C ABI:
on wasm it is the module, natively it would be a `.dylib`, `.dll` or
`.so`. An `rlib` is linked into the game. The crate can be both
(`crate-type = ["rlib", "cdylib"]`).

**What was built** (`rs-shim/` in the spike):

- `ddivideo.c` ported line for line to Rust (670 lines against the C file's 485) over
  `ffmpeg-sys-the-third` 6.0.0 with `avcodec`, `avformat` and
  `swscale` only.
- A safe API: `Video::open(&str) -> Option<Box<Video>>`, `next()`,
  `seek()`, `seek_key()`, `planes() -> &[u8]`, `Drop`.
- Under `cfg(target_family = "wasm")`, the 16 `ddi_*` exports as
  `#[unsafe(no_mangle)] extern "C"` functions.

**Natively.** It builds against the native FFmpeg 9.0.2 shared
libraries with `FFMPEG_DIR`. On all 29 clips and the pack movies it
gives the same frames as the C shim compiled natively: identical frame
times and CRC-32s, every seek exact, the same speed within noise
(tables in section 4).

**For wasm32-wasip1.** It links the static libraries `video/build.sh`
already makes with wasi-sdk 34. The result:

- **The module's ABI:** exactly the module's 20 `wasi_snapshot_preview1`
  imports and its 16 `ddi_*` exports, plus `_initialize` and a stray
  `clock`. The 1 GiB memory cap and stack-first layout are kept.
- **Size:** after Binaryen `-Oz`, 5,918,564 bytes against the C
  module's 5,889,206 (+0.5 %); 2,457,760 gzipped against 2,446,597.
- **Output:** run by today's desktop host (`DDI_VIDEO_MODULE` pointed
  at it, `--video-check --frames`), every clip and pack movie decodes
  to the same frame times and CRC-32s as the C module, and all of its
  104–126 seeks a file land exactly.

It took three things the C build does not need:

- `BINDGEN_EXTRA_CLANG_ARGS="--sysroot=<wasi-sdk>/share/wasi-sysroot
  -fvisibility=default"`. Clang's wasm target hides declarations by
  default, and bindgen then emits no functions at all.
- Rust's `crt1-reactor.o` passed to the link (`rustc-cdylib-link-arg`).
  Without it wasm-ld treats the module as a command and wraps every
  export in a call to the constructors and destructors (seen as
  `ddi_open.command_export`). `-mexec-model=reactor` does this for
  clang.
- `--stack-first`, `-zstack-size=1048576` and `--max-memory=1073741824`
  as `cdylib` link arguments, and zlib linked from the build's prefix.

**What it costs:**

- **Two C libraries.** FFmpeg's objects are compiled against wasi-sdk
  34's libc headers but linked with Rust's bundled wasi-libc (the
  `wasm32-wasip1` target's `self-contained/libc.a`). It worked, but the
  module's notices would have to name that wasi-libc. Linking
  wasi-sdk's instead (`-C link-self-contained=no` and its sysroot) was
  not tried.
- **New build requirements.** libclang on every build host for both
  targets, the `wasm32-wasip1` rustup target (added on this Mac for the
  spike), and `ffmpeg-sys-the-third`'s build script in the web's
  module build.
- **Speed is unchanged.** It is the same FFmpeg in the same runtime.

**Verdict.** The design is viable, and it is the right shape if the
shim is ever rewritten. It does not decide the native question,
though. The safe API wraps the same unsafe FFmpeg calls, so a crafted
movie is exactly as dangerous natively whether the few hundred lines around
FFmpeg are C or Rust. What native needs is isolation (section 5), and
the C shim compiled natively gives the same frames (section 4).
Proposed: keep `ddivideo.c` as the one shim for both targets for now
(N2). The Rust port is the documented alternative if the maintainer
prefers Rust; this section lists what it needs.

## 4. Measured (M1 Max, macOS, Rust 1.99, FFmpeg 9.0.2)

**Builds.**

- **Native FFmpeg:** built from the pinned tarball (SHA-256 checked)
  with `FFMPEG.toml`'s decoder list and flags, changed only for native:
  no cross-compile, the CPU's assembly on, pthreads on (decoding still
  single-threaded unless `thread_count` is set),
  `--disable-runtime-cpudetect` dropped.
  - **small:** with `--enable-small`, as the module.
  - **fast:** without it.
  - **noasm:** `--disable-asm`, to confirm the IDCT finding.
  - **module:** static libraries plus `ddivideo.c` linked into one
    `libddivideo.dylib` that exports only the 16 `ddi_*` functions
    (`-dead_strip`, `-exported_symbols_list`).
- **wasmfast:** the module rebuilt for speed: no `--enable-small`, `-O3`
  and wasm-opt `-O2` (Binaryen 133's `-O3` hit an assertion).
- **Agreement:** the existing module was run by a build of the
  maintainer's checkout at `78f0b3c` with ddi-desktop's `video`
  feature (`--profile desktop`), its per-frame time patched in the
  scratch copy to the decoder thread's CPU time. Native ran through a
  harness doing `check()`'s straight decode and seeks, with per-frame
  CRC-32s.

**Timing method.** The machine was shared with other agents' builds
(load average 25–140), so the tables give each frame's **CPU time on
the decoding thread** (`CLOCK_THREAD_CPUTIME_ID`), which load barely
moves. Wall times on a quieter stretch agree with 7.0's: the module
took 0.51, 0.44, 0.47, 1.11, 1.51, 1.94, 4.4 and 12.3 ms on the first
eight rows below. Seeks are wall time, the slowest of 124–126.

### Speed: ms a frame, mean (p99)

| Movie | module (wasmtime) | wasmfast | native fast | native, C IDCT | native small | native fast, 4 threads (wall) |
|---|---|---|---|---|---|---|
| Xvid 640×360 (clip) | 0.57 (1.94) | 0.45 | 0.18 (0.40) | 0.24 (0.72) | 0.20 | 0.06 |
| MPEG-2 640×360 (clip) | 0.45 (1.06) | 0.41 | 0.19 (0.37) | 0.26 (0.73) | 0.20 | 0.24 |
| DivX 3 640×480 (Bad Apple!!, 6,571 frames) | 0.49 (1.37) | 0.49 | 0.14 (0.33) | 0.23 (0.75) | 0.16 | 0.14 |
| H.264 640×360 (clip) | 1.12 (1.53) | 0.97 | 0.43 (0.59) | 0.43 | 0.55 | 0.14 |
| H.264 640×360 AVI (pack, 3,524 frames) | 1.49 (3.63) | 1.38 | 0.51 (1.48) | 0.51 | 0.62 | 0.18 |
| H.264 854×480 (clip) | 2.14 (4.65) | 1.61 | 0.74 (1.12) | 0.77 | 0.91 | 0.24 |
| H.264 1280×720 (clip) | 4.49 (5.64) | 3.71 | 1.69 (2.25) | 1.67 | 2.35 | 1.63 (under load) |
| H.264 1280×720 MP4 (pack, 3,623 frames) | 7.55 (26.3) | 7.81 | 2.87 (11.2) | 3.03 | 3.33 | 1.06 |
| H.264 1920×1080 (clip) | 9.6 (19.6) | 7.74 | 3.41 (4.11) | 3.35 | 5.07 | 0.81 |
| H.264 1920×1080 MP4 (pack, 25 fps, 2,790 frames) | 17.6 (48.4); 15.4 in the wasmfast run | 13.4 | 5.54 (15.3) | 5.50 | 6.82 | 1.64 |

- **Single thread:** native is 2.4–3.6× faster than the module, and
  2.6–3.2× on H.264. `--enable-small` costs native 10–50 %.
- **Four threads:** wall time falls another 3–4× on H.264. Frame
  threading changes neither frame times nor pixels (identical CRCs).
  It adds a few frames of decoder delay, which the queue of three
  absorbs.
- **The C IDCT** (`FF_IDCT_SIMPLE`, tried for byte-identical frames)
  costs 0.06–0.1 ms a frame on SD MPEG-4/MPEG-2/DivX 3 and nothing on
  H.264. It fails as a fix, though (see "Agreement" below).
- **The one-library build** (`libddivideo.dylib`) runs at the fast
  build's speed: 0.53 against 0.51 ms on the pack AVI, 5.50 against
  5.54 on the 1080p pack MP4. Its frames agree the same way.
- **wasmfast** (the module built for speed) gains 10–20 % on H.264 and
  about nothing on the SD codecs, for a 10.2 MB module instead of
  5.9 MB. It is not worth a second module.

### Worst frames and seeks

| Movie | module: worst frame / slowest seek (ms) | native fast: worst frame / slowest seek (ms) |
|---|---|---|
| Xvid 640×360 (clip) | 3.7 / 7 | 0.7 / 4 |
| DivX 3 640×480 (Bad Apple!!) | 4.6 / 11 | 0.4 / 3 |
| H.264 640×360 AVI (pack) | 5.7 / 455 | 2.7 / 157 |
| H.264 854×480 (clip) | 7.2 / 482 | 3.0 / 189 |
| H.264 1280×720 (clip) | 12.9 / 1,067–2,911 | 6.1 / 421 |
| H.264 1280×720 MP4 (pack) | 85 / 258 | 22 / 108 |
| H.264 1920×1080 (clip) | 26 / 2,339–4,735 | 11 / 857 |
| H.264 1920×1080 MP4 (pack) | 213 / 619 | 30 / 232 |

Exact seeks in long-GOP H.264 are what native helps most: 3–5× faster.
The module's worst single frames on the 1080p pack MP4 (48 ms p99, one
of 213 ms CPU) are what `MovieTrack`'s queue and late-frame rule
absorb today (7.7 saw the same).

### Agreement with the module

| Set | Frame times | Pixels |
|---|---|---|
| 29 clips of `video/clips.sh` (every codec family and pixel path), native fast | all identical | identical for H.264 (8- and 10-bit, 4:4:4), HEVC, VP8, VP9, WMV2, FFV1, HuffYUV, Cinepak, MS Video 1, ZMBV, PNG, QuickTime RLE. **Different** for MPEG-1, MPEG-2 (4:2:0 and 4:2:2), MPEG-4 (AVI and MOV), Xvid with dropped frames, DivX 3, FLV1 and MJPEG |
| the same, `idct_algo = FF_IDCT_SIMPLE` | all identical | all identical |
| the same, `--disable-asm` | all identical | all identical (checked on Xvid and H.264) |
| the pack movies (Bad Apple!!, the 640×360 AVI, BANG BANG, BAD) | all identical | H.264 identical; DivX 3 differs in 5,559 of 6,571 frames with NEON, identical with `FF_IDCT_SIMPLE` |
| 14 local movies (`~/Music/Simfiles`, read in place: 4 H.264 from 304×176 to 1280×720, 4 MPEG-2, 4 MPEG-4 Part 2, 2 DivX 3; 2,547–4,294 frames each) | all identical | H.264 identical. MPEG-2 and DivX 3 differ with NEON and match with `FF_IDCT_SIMPLE`. **MPEG-4: three Xvid-encoded files match with NEON and differ with `FF_IDCT_SIMPLE`**; the fourth (not Xvid) the other way round |
| the Rust port (native and wasm) | all identical to the C shim built the same way | likewise |

- **The difference is FFmpeg's NEON IDCT.** It and the C "simple" IDCT
  round differently, as IEEE 1180 allows. Errors build up over a GOP.
  The worst PSNR against the module:
  - Y 83.8 dB, U 78.0 dB, V 81.4 dB on the Xvid clip;
  - 65.2–72.2 dB on the local MPEG-2, DivX 3 and MPEG-4 movies and
    Bad Apple!! (an RMS error of a fifth of a level at most).
- **Why no option fixes it:**
  - The MPEG-4 decoder switches to the Xvid IDCT (C everywhere) when it
    finds an Xvid signature, but only while `idct_algo` is
    `FF_IDCT_AUTO`. Forcing `FF_IDCT_SIMPLE` fixes MPEG-1/2, DivX 3,
    MJPEG and non-Xvid MPEG-4, and breaks Xvid files, the most common
    MPEG-4 in packs.
  - On x86-64, `FF_IDCT_SIMPLE` still selects the SSE2/AVX IDCT
    (`libavcodec/x86/idctdsp_init.c`).
  - Byte identity everywhere would need a small FFmpeg patch (the
    assembly IDCTs skipped under `AV_CODEC_FLAG_BITEXACT`), shipped and
    maintained as a modification. Not worth it for 65 dB.
- **H.264 and HEVC are bit-exact by specification**, so their pixels
  match anyway.
- **Seeks:** every native seek (124–126 a file) landed on the frame
  decoding straight through gives, byte for byte, just as the
  module's do.

### Sizes and build times

| Item | Size | Gzipped |
|---|---|---|
| the module (`ddivideo.wasm`, today) | 5,889,206 | 2,446,597 |
| wasmtime in the game binary (7.7, stripped) | +6.8 MB | |
| native small: libavcodec + libavformat + libswscale + libavutil (already stripped) | 7,321,232 | 3,535,988 |
| native fast: the same four | 11,638,224 | 5,179,678 |
| native fast as one `libddivideo.dylib` (shim inside, only `ddi_*` exported, dead code stripped) | 10,529,424 | 4,690,204 |
| the Rust-built wasm module | 5,918,564 | 2,457,760 |
| wasmfast | 10,183,621 | |

- **Distribution size:** native does not ship less. One native library
  per platform (10.5 MB fast, about 7 MB small) replaces the module
  plus wasmtime (12.7 MB). On macOS a universal arm64 + x86_64 library
  doubles it.
- **Native build time:** configure takes 77 s (this machine's
  `configure` is slow under load); the shared build takes 60 s wall /
  170 s CPU for small and 188 s / 262 s for fast; the static
  single-dylib build takes 150 s with `-j4`. Each platform needs its
  own build, plus x86_64 macOS.
- **Today's costs, for comparison:** a clean wasmtime build takes 84 s
  wall / 460 s CPU (7.0), and the module is built once for both shells.
- **Opening a movie:** 1–15 ms natively, against 3–21 ms for the module
  once compiled. The module's first compilation (0.85 s) is cached.

## 4a. VideoToolbox spike (2026-10-17)

**Scope and packet source.** A small Rust driver feeds compressed samples
into `VTDecompressionSession`, retaining each callback's `CVPixelBuffer`.
H.264 AVI uses the game's own `library/src/avi.rs` index and packet reader,
and `library/src/h264.rs` for picture order, DTS-based output times and
`ExactSeek`, as the web's WebCodecs path does. The MP4/WebM/MPEG-PS
experiments use the earlier spike's native libavformat **only as a test
packet source**. This is not production demuxing: those containers need
a bounded packet interface from the sandboxed module before integration.
There is no new in-process FFmpeg path in the game.

Test machine: M1 Max, macOS 27.0.1 (build 26A434). Generated clips from `video/clips.sh`
and the earlier spike, no third-party movies published. Query hardware
support, try the actual format description and session, then check
`UsingHardwareAcceleratedVideoDecoder`; the query alone is insufficient.

| Codec / fixture | Observed VT result | Agreement and disposition |
|---|---|---|
| H.264, progressive 8-bit 4:2:0, 360p AVI and 720p/1080p MP4, 600 frames each | hardware, NV12 | all 1,800 frame timestamps and I420 CRCs equal the wasm module; first 60 frames of each byte-identical to native FFmpeg; exact-seek trials below |
| HEVC, 320×240 MP4, 60 frames | hardware, NV12 | all timestamps/CRCs equal the module; raw-plane PSNR against the module exact on Y/U/V |
| VP9, 320×240 WebM, 60 frames | hardware after supplemental decoder registration, NV12 | all timestamps/CRCs equal the module; raw-plane PSNR against the module exact on Y/U/V |
| AV1, generated 320×180 MP4 with av1C | hardware query false; session refused −12906 | unavailable on this machine; capability-test other devices, no general support claim |
| MPEG-2, 320×240 MPEG-PS, 60 frames | session accepted; hardware query false, hardware property absent; service uses CPU | worst PSNR against the module: Y 58.2, U 33.2, V 30.9 dB (36.7 dB combined against native FFmpeg), below the 60 dB acceptance bound; rejected for integration |
| MPEG-4 Part 2, MOV with VOL in esds (AVI also tried) | hardware query false; session refused −8969 | fallback; a rejected fixture does not prove every profile unavailable |

H.264's 124 seek targets per clip include exact frame times, mid-frame
times, EOF and rewind. Seeking uses our streaming picture-order and
retry logic; all 124 targets per clip passed (360p median/max 23.8/56.9 ms,
720p 35.5/123.8, 1080p 53.3/130.7). Every returned timestamp/CRC must equal the sequential
module frame showing at that time. HEVC and VP9 each passed 122 targets
(0 wrong), with median/max 21.3/25.2 ms and 26.0/57.9 ms respectively.
Those two are **finite indexed fixture trials**: container PTS sorts
outputs, codec metadata supplies delay, and the full packet index maps
frame times. They prove the tested seeks, not a bounded streaming
implementation for arbitrary HEVC/VP9. Long GOPs, variable frame rate,
missing DTS, profile changes, 10-bit and interlacing remain acceptance
work; unsupported timing must fall back, never silently change the clock.

**CPU measurements.** The H.264 runs decode 600 frames. Default scheduling
placed 89–99% of the measured VT caller CPU on performance cores
(the module process 92–100%, sampled with `RUSAGE_INFO_V6`);
`taskpolicy -c background` placed effectively all on efficiency cores
(0–0.2% P-core CPU on the measured VT caller; under 0.1% on
the module process in the three H.264 runs). It is a scheduling proxy,
not an affinity promise for the XPC service. VT CPU is caller `getrusage`
plus the newly created service's `proc_pid_rusage` delta, excluding open
and seeks. It includes callback handling, I420 allocation/copy and CRCs.
The module figure is `CLOCK_THREAD_CPUTIME_ID` around each decoder call
in the earlier spike's driver; it excludes host upload and CRC work.
No GPU rendering is timed. These are CPU costs, not playback throughput
or battery measurements; background QoS also greatly increases wall time.

| H.264 fixture | Scheduling | VT copied I420, caller + service CPU ms/frame | VT Metal import, caller + service CPU ms/frame | wasm decoder CPU ms/frame (p99) |
|---|---|---:|---:|---:|
| 640×360 AVI | default, mainly P cores | 0.385 | 0.348 | 1.15 (1.45) |
| 1280×720 MP4 | default, mainly P cores | 0.520 | 0.358 | 4.35 (5.24) |
| 1920×1080 MP4 | default, mainly P cores | 0.886 | 0.452 | 9.78 (13.23) |
| 640×360 AVI | background, E cores | 1.024 | 0.859 | 3.68 (6.22) |
| 1280×720 MP4 | background, E cores | 1.440 | 0.884 | 13.39 (21.04) |
| 1920×1080 MP4 | background, E cores | 2.207 | 1.089 | 28.02 (38.27) |

Every frame time/CRC also matches the module under background scheduling.
The 1080p default run had more E-core time than the lower-resolution runs;
these are measured scheduling policies, not pinned-core microbenchmarks.
The earlier default run (`measure/`, before the final CPU-only pass) was
99.9% P-core in the VT caller and gave 0.659 ms/frame with copies,
0.312 with Metal import; this spread is why the report keeps the raw logs.
The final pass disables repeated seeks for the CPU comparison, through a
scratch-only `DDI_SPIKE_NO_SEEKS` switch; full seek evidence is separate.

The 60-frame HEVC/VP9 probes also ran under both policies. These
measure synchronous VT decode and its service CPU before copying planes,
so they are not directly comparable to the H.264 pipeline totals:

| Fixture, 320×240 | VT default CPU ms/frame | VT background CPU ms/frame | wasm default / background CPU ms/frame |
|---|---:|---:|---:|
| HEVC MP4 | 0.41 (0.10 caller + 0.31 service) | 1.64 (0.39 + 1.25) | 0.60 / 1.84 |
| VP9 WebM | 0.42 (0.10 + 0.32) | 1.29 (0.29 + 1.00) | 0.50 / 1.41 |

At these small sizes the VT submission/service overhead consumes much
of the gain. Hardware decoding is not automatically faster for every
movie; benchmark larger clips before expanding the production gates.

**Plane copies and Metal.** NV12 is what VT returned. The copy path locks
the pixel buffer, copies Y and splits interleaved UV into the renderer's
three I420 planes. At default QoS, that costs 0.024 / 0.077 / 0.229 ms
per frame at 360p / 720p / 1080p. The alternative requests IOSurface-backed
buffers and wraps Y as R8 and UV as RG8 with
[`CVMetalTextureCacheCreateTextureFromImage`](https://developer.apple.com/documentation/corevideo/cvmetaltexturecachecreatetexturefromimage(_:_:_:_:_:_:_:_:_:));
import/release costs 0.010 / 0.009 / 0.015 ms, with no CPU plane copy.
Total caller + service CPU is in the table above. Under background
QoS, plane copying costs 0.078 / 0.346 / 0.694 ms per frame; Metal
import/release costs 0.049 / 0.027 / 0.039 ms. CRC work also disappears from
this path, so the total difference is not solely the memcpy saved.
The spike imports and releases views; it does not submit a GPU draw.

Integration is feasible but backend-specific: create the texture cache
on wgpu's actual Metal device, wrap the retained `MTLTexture`s using
wgpu-hal 30's `Device::texture_from_raw`, then
`wgpu::Device::create_texture_from_hal`. The renderer needs an NV12
plane/shader variant beside I420, preserving range, matrix and colour
conversion. Retain the CVPixelBuffer and CVMetalTexture views until GPU
completion, handle device loss and seek generations, and never recycle a
surface still sampled. First ship the simpler copied-plane path; measure
upload costs before adding this interop. Apple's
[VT/Metal session](https://developer.apple.com/videos/play/wwdc2020/10090/)
describes both IOSurface sharing and keeping the views alive during GPU work.

**Process isolation observed.** Session creation spawned a new
`VTDecoderXPCService`, measured separately from services already present;
H.264/HEVC/VP9 sessions reported hardware acceleration and the service
accounted for measurable CPU. This agrees with Apple's description of
sandboxed decoding and IOSurface transfer in that session. It establishes
the observed path, not a guarantee that every optional decoder or every
future OS uses the same service. The caller still parses packet headers
and owns the FFI boundary; cap dimensions, packet sizes and queued outputs.

**Reproduce locally.** `target/scratch/vt/measure.sh` interleaves copied
VT planes, Metal import and the wasm module at default/background QoS;
`cpu-final/*.txt` holds every timestamp/CRC and the final CPU summaries;
`measure/*.txt` keeps the earlier runs (the redundant background 720p
seek run was stopped, not used as a completed result).
`h264-360.txt`, `hd-seeks.txt`, `generic-seeks.txt`, `codecs-final.txt`,
`av1.txt` and `core-observations.txt` retain the other evidence. Build the
standalone scratch crate with `FFMPEG_DIR` pointing to the earlier spike's
`ffmpeg-native/fast`; the only Objective-C helper is texture wrapping and
core-accounting. Run binaries under the scratch watchdog, never in a
window. The earlier FFmpeg spike and its builds are retained.

## 4b. First production slice: H.264 AVI (2026-10-17)

`desktop/src/video/avi.rs`, `vt.rs` and `native.rs` now integrate the first
copied-plane path behind `video`. The default host tries VideoToolbox on
macOS and the module on other platforms. Selection requires an actual
hardware session for an indexed AVI with a single SPS/PPS configuration,
progressive 8-bit 4:2:0 H.264,
POC 0/2, a fixed reorder delay at most 16, and visible dimensions at most
4096×2160. Every packet's header and range are checked; changed parameter
sets, unsupported output layouts/ranges, dropped or duplicate callbacks,
and missing expected outputs cause fallback. No native FFmpeg is linked.

The file reader is relative to a `MovieFile` range, so a stored zip entry
is read in place. Index bytes are capped at 32 MiB, entries at one million,
packets/samples at 8 MiB. Compressed packets stay on disk. The callback
retains its pixel buffer only during the validated NV12 → I420 copy,
then releases it. Its queue is capped at 32 frames / 128 MiB, as is the
reorder queue (before another packet is fed). Each session's callback state
is separate; generation tags and the host's existing seek generations
keep superseded frames out. Seeks recreate the session and use the shared
`PictureOrder`, `OutputOrder`, `seek_start`, `seek_back` and `ExactSeek`.
EOF timestamps use the module's incremental additions, including their
rounding when the AVI stream starts at a nonzero frame.

Each movie's worker owns all OS calls, including destruction. A bounded
command/reply channel and a four-second deadline keep a stalled session
from holding the game thread; the controller detaches rather than joining
native teardown. One timeout disables new native sessions for the process;
a global four-worker cap also bounds abandoned/closing workers. Refusal,
bad output or a timeout moves that movie to the module. Recovery seeks to
the last emitted frame (or the requested seek time before any was emitted),
skips frames already delivered and continues the outstanding wants, without
posting a second Opened event. Probe/black-bar sampling still uses the module.
A movie can open through VT without the module; fallback then reports the
existing missing-decoder error if needed.

`DesktopVideo::decoder_status()` exposes the chosen backend and fallback
reason; decoder selections are logged. `--video-check FILE --decoder
module|videotoolbox|auto|both` selects a path; `videotoolbox` is strict, so a
refusal is visible, and `both` compares every sequential timestamp and CRC
while checking each backend's exact seeks. `with_module` remains an explicit
module-only host for existing tests. The frame and track interfaces and the
automatic video rule stay shared with the web.

Validation on the M1 Max: `desktop/tests/video_native.rs` generates eight
AVIs (B frames, no B frames, long GOP, dropped-frame gap, full-range BT.709,
fractional frame rate, nonzero stream start, and 720p): 560 sequential frames
and 906 seeks per backend. Every returned I420 CRC and timestamp and the
stream metadata match the module, and each path passes the checker’s
frame-time/mid-frame/EOF/rewind seeks. The same test covers stored-entry-like
ranges, rapid superseding seeks, 10-bit and non-AVI fallback, a truncated
index, and a later changed SPS: the module resumes with the same complete
frame sequence. `DDI_REQUIRE_VT=1` makes absent hardware a failure for local
verification; CI can skip only a genuine hardware-session refusal. Unit
tests cover the deadline/disable behaviour, range bounds, alternate
configurations and malformed avcC.
`play_movie` also renders MPEG-4 and H.264 through the real offscreen play.

This completes the AVI part of steps 1–2. Sections 4c–4e add bounded packet
exports and checked streaming H.264/HEVC/VP9 consumers. Windows and GPU
interop remain. The finite spike alone does not broaden a production gate.

## 4c. Bounded sandboxed packets (2026-10-17)

`video/packets.c` now supplies additive ABI v1 exports for container metadata,
raw codec configuration, compressed video packets, side data and paged seek
hints. The old decode interface stays compatible. The checked Rust reader
(`desktop/src/video/packets.rs`) owns a private wasmtime instance, uses the
existing file/range sandbox, memory cap, cache and call deadlines, and checks
all exported lengths, ranges, flags and generations before copying. Errors
poison the reader; no later close enters a trapped instance. No native FFmpeg
is loaded. The full contract and inspection command are in
[`video/packets.md`](../../video/packets.md).

Packets retain exact signed PTS/DTS ticks, including negative/unknown values;
seeks increment an epoch. Configuration side data is preserved and changes
are signaled explicitly. Initial extradata is not synthesized from in-band
headers. Backward key-packet seeks and FFmpeg's potentially incomplete index
are primitives, not exact presentation seeks. Consumers must still validate
in-band changes, reconstruct codec output order and preserve the baseline's
DTS/EOF rules. Seeking restores header configuration; changing streams need
reopen/preroll or fallback.

Generated H.264 MP4/AVI/Matroska, nonzero-start MP4, variable-rate Matroska,
HEVC MP4, VP9 WebM and AV1 Matroska are checked against ffprobe for packet
bytes, exact ticks, positions, flags and available configuration. The reader
also checks key seeks after EOF, range-relative files and interleaved
instances; synthetic hostile exports check limits, malformed records,
configuration generations and trap recovery. AV1 demux works even though the
module has no AV1 decoder. The old and new modules gave identical planes and
frame times for 1,730 frames and 174 seeks across 29 generated clips. The
existing codec/reference suite passed (Theora skipped: local encoder absent),
and fresh headless Chrome and Firefox each passed 150 frames and 188 exact
seeks through the module. This does not widen the playback capability gate.

Sections 4d–4e connect these packets to checked H.264/HEVC/VP9 output
models and exact seeks. Packet agreement alone is insufficient to select
an OS decoder.

## 4d. Streaming H.264 MP4/Matroska (2026-10-17)

The native worker now uses `media.rs` for both the checked AVI reader and a
private `PacketReader` for MP4/Matroska. `h264_format.rs` holds the shared
SPS/PPS validation and sample framing. The new container class requires avcC,
a reliable container duration, the same bounded progressive 8-bit 4:2:0
POC 0/2 stream with a fixed reorder delay at most 16, and one supported slice
per packet. Dimensions, range, matrix, field order and delay from the container
must agree with the checked SPS. Corrupt/discard flags, packet side data,
changed parameter sets, unsupported NALs or framing, and more than 256 NALs
in a packet cause module fallback. Known other codecs/containers skip the
native probe. Automatic selection is limited by these checks, not file suffix.

The demuxer is capped at 256 MiB and two seconds per call, inside the existing
four-second worker deadline and four-worker limit. Reopening destroys the
old wasm heap before creating another. Compressed bytes stay on disk; no
whole-movie packet or frame index is built for this path. Identity, duration
and output queues are bounded at 32 frames; decoded planes retain the existing
128 MiB bound. One million sequential packets per generation is a refusal
limit. The controller never joins an OS teardown or timed-out worker.

POC selects the output identity; the packet that releases it supplies its
normalized DTS. Missing DTS and EOF use the baseline's incremental additions.
The original packet duration is kept for a held frame's final display span.
Seeks reproduce the module's timestamp backoff, then back up from the first
landed DTS with bounded exponential retries; failed/empty seeks reopen from
the start. A non-IDR seek is refused and recovered in the module at the same
requested time. Generation tagging and exact-position fallback are unchanged.

Generated MP4/Matroska tests compare every plane CRC, exact timestamp, stream
metadata and seek against the module, on hardware VideoToolbox and Auto.
They cover B/no-B streams, fractional rates, nonzero starts, VFR gaps, long
GOPs, 720p, full-range BT.709, interleaved audio, range-relative members,
simultaneous readers and rapid seeks. Edge cases cover multi-slice/10-bit
refusal, later SPS changes, open-GOP seeks, malformed files and bad ranges.
The existing AVI suite remains a regression check. No native FFmpeg is linked.

Section 4e adds independently checked HEVC and VP9 stream classes. The
H.264 checks alone do not select other codecs.

## 4e. Streaming HEVC and VP9 (2026-10-17)

`codec.rs` selects separate output models below the existing worker and
private demuxer. HEVC in MP4/Matroska requires hvcC Main, progressive 8-bit
4:2:0, one layer and temporal layer, one VPS/SPS/PPS and an independent,
visible slice per packet. Short-term RPS syntax (including prediction) and
POC wrap are parsed with bounded Rust header reads. The DPB retains pending
output and reference identities, removes references no longer in the RPS,
and emits the lowest POC when either the reorder count or DPB capacity is
exceeded. An IDR drains the old sequence before starting the new one.
Every emitted identity takes the current packet's normalized DTS; EOF uses
the module's incremental additions. Container PTS is never an output index.

The HEVC gate refuses long-term references, extra/dependent slices, changed
parameter sets, custom scaling lists, PCM, fields and unsupported NALs/SEI.
Only user-data-unregistered SEI is accepted; recovery/timing/film-grain SEI
could alter output or pixels. A seek must land on an IDR; an open-GOP CRA
seek returns to the module at the same requested time. The SPS DPB is at
most 16 pictures, with at most 64 short-term RPS definitions and 16
references per set. NAL units and samples retain the existing 256/8 MiB
limits; each header reader examines at most 64 KiB. Full compressed payloads
stay opaque to Rust and are submitted only after those checks.

VP9 in WebM/Matroska or MP4 requires profile 0 and one visible coded picture
per packet. Every header checks profile, geometry and rendering size; key
headers also check range/matrix against the container. References all retain
the checked geometry. A fresh decoder/seek must begin with a key picture.
Visible pictures emit immediately on their packet DTS. Hidden alternate
references, superframes and show-existing pictures refuse before submission;
their output multiplicity is outside this first class. Higher bit depth,
HDR, changed geometry/colour, packet side data and corruption also use the
module. The existing 4096×2160 geometry, two-second/256 MiB demux heap,
four-second worker deadline and four-worker/32-output bounds remain.

CoreMedia builds HEVC's format from the checked parameter sets and VP9's
from a vpcC atom describing the checked profile and colours. VP9 registers
Apple's supplemental decoder before attempting the session. Both require
hardware and verify the actual hardware property, retaining the copied NV12
to I420 path and its range/geometry checks. No native FFmpeg is linked.

HEVC can release several pictures at the same DTS. The decoder-check harness
now allows equal successive times and seeks to the last picture at that
time, matching the unchanged module and shared `ExactSeek`. Recovery counts
the delivered part of a coincident group, seeks just before it, and skips
only those delivered pictures. An exact seek is restored to its original target so the module coalesces
the same initial pictures before applying the count. This avoids losing a still-undelivered picture
when a native failure happens part-way through a group.

Nineteen generated clips (1,540 frames and 2,188 exact seeks per backend)
match the module through hardware VideoToolbox and Auto. They cover B/no-B
HEVC, closed GOPs, POC wrap, fractional rates, nonzero starts, VFR gaps, long GOPs, 720p,
full-range BT.709 and lossless VP9. Stored members, concurrent codecs and
superseded seeks exercise the shared worker. Refusal/recovery cases include
later HEVC SPS changes in both containers, non-IDR open-GOP seeks, multiple
slices, 10-bit streams and two-pass VP9 hidden alternate references. Real
movie play remains covered offscreen on a fake clock; H.264 remains a
regression suite.

Sources for syntax and baseline behaviour:
[H.265](https://www.itu.int/rec/T-REC-H.265), sections 7.3.2/7.3.6/7.3.7 and
8.3.1; [VP9 headers](https://downloads.webmproject.org/docs/vp9/vp9-bitstream_superframe-and-uncompressed-header_v1.0.pdf)
and [ISO binding](https://www.webmproject.org/vp9/mp4/). The pinned FFmpeg
9.0.2 source (`target/video/ffmpeg-9.0.2/libavcodec/hevc/{ps,hevcdec,refs}.c`,
`vp9.c` and `bsf/vp9_superframe_split.c`) defines the module's release and
DTS behaviour. These are independent Rust header/output models checked
against that baseline, not native FFmpeg decoding.

Windows now has an experimental opt-in implementation (section 7), awaiting
Windows runtime evidence. Wider macOS stream classes require their own output
models and fidelity evidence. The opt-in macOS plane-sharing path below keeps
these same codec gates.

## 4f. Rendered IOSurface/Metal planes (opt-in, 2026-10-17)

The import-only experiment in 4a is now followed by an offscreen benchmark
and an opt-in integration in real play. Defaults still copy I420 planes.
Build `ddi-desktop` with `video-metal` and set `DDI_VIDEO_PLANES=metal` to
request sharing for the same checked VideoToolbox streams; neither changes
codec availability. Other platforms and builds retain their current routes.
There is no menu option yet. Manual play on other Macs, physical GPU loss
and presentation-latency measurements remain deferred.

The completed, immutable `CVPixelBuffer` moves from the decoder worker to
the drawing thread in the existing external-frame envelope. Only that
restricted owned wrapper is `Send`, not `Sync`; no native handle enters the
renderer or the platform trait. CVMetalTexture/cache objects stay on the
drawing thread. The cache uses **wgpu's actual Metal device**, checks NV12
plane geometry, upper-left origin and a full clean aperture, then wraps
initialized R8/RG8 views through wgpu-hal. The renderer's NV12 fragment
entry samples those planes and calls the same YUV conversion as copied
I420: BT.601/709, full/limited range, dimming, alpha and plain/sRGB targets
keep their existing arithmetic. Unsupported imports copy that very image;
the hardware decoder can continue. CPU diagnostics materialize planes and
charge that copy to decode time.

Ownership has two independent retirement conditions: both HAL textures
must be retired, and every command buffer that could read them must have
completed. The shell attaches leases to the **actual drawing encoder**
before rendering, including paused/repeated pictures. Discarding an encoder
before submission drops its leases. The retained image and both Core Video
views survive until both conditions hold, not merely until the next frame
or a callback on earlier queue work. Old-device caches survive while their
textures remain referenced. A process-wide budget permits at most four
imported images and 128 MiB of accounted pixel storage (plane pitches and
allocation size); ordinary pressure uses copies. Existing decoder/output
queues keep their separate byte bounds, and external frames do not enter
the movie-loop cache.

Unconfirmed teardown is deliberately conservative: after a bounded wait,
any surviving owners are quarantined and sharing is disabled for the
process. The global budget bounds this retained storage even across device
replacement. It is not evidence that a physically hung GPU completed. New
frames use copies; new decoder sessions request copied output. Healthy
retirement releases all owners and periodically flushes unused Core Video
cache entries. No new native FFmpeg code is introduced.

`desktop/src/video/vt/benchmark.rs` exercises the production importer,
background pipeline and GPU-completion leases. Generated H.264 MP4/MKV,
HEVC MP4 and VP9 WebM clips cover BT.601/709 and limited/full range, plain
and sRGB output, one/three frames in flight, decoder teardown before GPU
retirement, and seeks within/across a GOP. All **4,800 rendered frame
identities and RGBA CRCs** match the copied path exactly. Separate tests
cover private-device destruction/replacement, paused images, an abandoned
encoder, budget fallback and deterministic unconfirmed completion. The
fault test retains one bounded image and disables sharing intentionally.
The real offscreen play test traversed the normal host, session, movie
track and renderer for MPEG-4 fallback plus H.264 AVI/MP4/MKV, HEVC and VP9:
**1,073 shared frames, no copy fallbacks, zero retained owners at the end**.
Strict native decoder suites also materialize external frames to compare
all timestamps, plane CRCs and exact seeks against the module.

For performance, the optimized `desktop` profile keeps optimization level 3
but disables LTO (`CARGO_PROFILE_DESKTOP_LTO=false`): the LTO compiler
exceeded the local 2 GiB memory limit and was stopped. Each H.264 fixture
has 600 frames at 60 Hz, tested in copied/shared/shared/copied order at
360p, 720p and 1080p, with one and three frames in flight. Session opening,
pipeline warm-up and CRC readback are outside timed runs. CPU is this
process's user+system time, including its workers, **excluding the separate
VideoToolbox service**. Wall time includes demux, decode, transfer, drawing
and GPU completion; this is unpaced offscreen throughput, not display
latency or a game-frame saving. The loop uses the production parsers,
callback and importer but bypasses normal host message dispatch. Background
QoS is a weak-CPU proxy, not pinned efficiency cores or measured battery
use. Measurements and raw runs are saved with the pending snapshot.

Means of the two runs per path on this M1 Max, with three frames in
flight (one-frame runs and all samples are in the saved CSV):

| Scheduling | Height | Copy/shared CPU ms/frame | Copy/shared wall ms/frame |
|---|---|---|---|
| default | 360p | 0.55 / 0.35 | 0.90 / 0.71 |
| default | 720p | 1.11 / 0.37 | 1.71 / 0.95 |
| default | 1080p | 2.00 / 0.48 | 2.88 / 1.38 |
| background | 360p | 2.60 / 1.64 | 4.95 / 3.31 |
| background | 720p | 4.30 / 1.71 | 8.36 / 4.01 |
| background | 1080p | 7.33 / 2.08 | 16.31 / 4.72 |

Run these separately, under the repository watchdog and a scratch
`DDI_HOME` (the device-loss tests can disable sharing within their process):

```sh
cargo test -p ddi-desktop --features video-bench --lib rendered_metal_planes_match_copies -- --ignored --nocapture
cargo test -p ddi-desktop --features video-bench --lib metal_device_loss_uses_fresh_resources -- --ignored --nocapture
DDI_VIDEO_PLANES=metal cargo test -p ddi-desktop --features video-metal --lib retained_play_frames_obey_gpu_leases_and_fallback -- --ignored --nocapture
CARGO_PROFILE_DESKTOP_LTO=false cargo test -p ddi-desktop --profile desktop --features video-bench --lib measure_video_gpu -- --ignored --nocapture --test-threads=1
```

Implementation: `desktop/src/video/metal.rs`, `vt.rs`, `native.rs`,
`play/movies.rs` and `play/view.rs`; renderer plane bindings in
`render/src/background/nv12.rs`. The optional objc2-metal/Core Video
bindings are 0.3.2 (MIT OR Apache-2.0 OR Zlib); default builds do not gain
a new rendering path. Core Video's SDK declarations define flipped origin
and clean coordinates; wgpu's command-encoder completion callback and HAL
texture drop guard provide the two retirement signals.

## 5. Safety

**The risk.** The module decodes with 264 decoders and every demuxer:
decades of C written for speed, fuzzed constantly and still producing
memory-safety bugs. In 2025 Google's Big Sleep reported heap overflows
in two decoders on our list: the SANM decoder (CVE-2025-59730) and
JPEG 2000 (CVE-2025-9951, rated high). Movies come from packs anyone
can publish, and players download and open them without a second
thought.

**Today, under wasmtime:**

- **Memory:** an out-of-bounds write stays inside the module's linear
  memory (capped at 1 GiB), so it can only corrupt the decoder's own
  state.
- **System access:** the module reaches nothing but our 20 WASI calls,
  over the one movie file.
- **Hangs:** a spin traps at the 10 s deadline.
- **Recovery:** a trap fails the open movies and the next open starts
  a fresh instance (7.7's hardening, tested with a hostile stand-in
  module).
- **The worst case** is a wrong picture or a movie that stops.

**Natively, in the game's process,** the same bug can be code
execution as the user: their files, their network, their saved
credentials. Treat that as the realistic worst case, not a theoretical
one. Fewer decoders (the four codecs packs actually use) would shrink
the attack surface, but it breaks decision 35's "every format
StepMania plays", and H.264 alone has had such bugs.

**The options:**

| Option | What it isolates | Cost |
|---|---|---|
| A. Keep the module under wasmtime (today) | everything; proven in 7.7 | the speed above; +6.8 MB of wasmtime |
| B. Native, in process | nothing | none; **not acceptable** for third-party movies |
| C. Native in a decoder process, no OS sandbox | the game's memory; a crash or hang kills only the child | a helper executable to build, sign and ship; IPC; frames over shared memory (1080p 4:2:0 is 3.1 MB, a memcpy of about 0.3 ms); one process for all movies, as one decoder thread today. A compromised child still has the user's rights |
| D. C plus an OS sandbox | as A, if the sandbox holds | per-OS sandbox code, reviewed and tested on each OS (below) |
| E. OS decoders for verified streams, module for the rest (next) | VT used its decoder service in this spike; Media Foundation/VA-API isolation must be assessed separately | packet demux and output-order replay, per-OS decode API, optional GPU interop; the fallback preserves format coverage |

**The sandbox for D, per OS:**

- **macOS:** the child is a signed helper in the bundle with the App
  Sandbox entitlement and no file access. Alternatively it calls
  `sandbox_init` with a profile that denies files and network (the API
  is deprecated but is what Chromium uses).
- **Linux:** seccomp-bpf (`seccompiler` 0.5.0, Apache-2.0 OR
  BSD-3-Clause) to allow only reads, writes, `mmap`, `futex` and exit
  on inherited descriptors, and landlock (`landlock` 0.4.7, MIT OR
  Apache-2.0) to drop file access.
- **Windows:** a restricted token, low integrity and a job object, or
  an AppContainer (`rappct` 0.13.3, MIT). This is the hardest to get
  right.
- **Every OS:** the child never opens files. The game reads the movie
  and answers the child's reads, the same 64 KiB requests the WASI
  host already answers, so the sandbox can deny the file system
  outright.

**Rough costs:**

- **C:** about 1,000 lines of code: a helper binary, a message protocol
  equal to `VideoHost`'s, a shared-memory frame ring (`memmap2` 0.9.11,
  MIT OR Apache-2.0) and crash handling equal to today's trap
  handling.
- **D adds,** per OS, a few hundred lines and a test that the sandbox
  actually denies: an `open("/etc/passwd")` from the child must fail.

## 6. LGPL on the native side

- **Stay a separate replaceable file, as today.** FFmpeg's checklist
  asks for dynamic linking (or object files for relinking), the source
  of the exact FFmpeg used, the licence and a notice, an about-box
  line, and no ban on reverse engineering. Static linking into the
  game would oblige us to ship object files for relinking, so we do
  not do it.
- **One shared library** (`libddivideo.dylib`, `ddivideo.dll`,
  `libddivideo.so`) holds FFmpeg and the shim. The shim is MIT; FFmpeg retains its LGPL licence. The combined
  library is distributed under the LGPL,
  with:
  - FFmpeg's tarball, or an offer valid three years;
  - the shim and the build script, so the user can rebuild and
    replace it;
  - `ddivideo.LICENSE.txt`, as the module has today.
- **Loading at run time** (`libloading` 0.8 or `dlopen`, not a
  link-time dependency) keeps the game starting without it. A build
  with no library plays songs with the background and the results'
  one line, as a build without the module does.
- **Code signing:**
  - **macOS:** with the hardened runtime and library validation, a
    replaced dylib with another signature can be refused. Packaging must
    document and test a working replacement/rebuild route, including
    ad-hoc re-signing where needed; do not infer compliance merely from
    keeping the library in a separate file.
  - **Windows:** Authenticode-sign the DLL beside the executable.
  - **Linux:** the `.so` beside the binary, found through `$ORIGIN`.
- **The wasm module needs none of this:** it is data, read and checked
  by its hash. Replacing it needs no re-signing, which is one more
  thing in A's favour.

## 7. Implementation order (decision 50)

The module remains the baseline decoder and fallback throughout this work.
The verified macOS H.264, HEVC and VP9 subsets select VideoToolbox automatically;
`DDI_VIDEO_DECODER=module` forces the module for play.
An OS path is selected for a movie only after its supported stream class
passes the checks; absence, refusal, bad output or a stalled session moves
that movie to the module at the same requested time. If the module also
cannot decode it (AV1 on today's build, for example), keep the existing
still-background fallback and notice. Report the selected
backend and fallback reason in `--video-check` and diagnostics. Preserve
`VideoBackend`/`VideoDecoder`, `MovieTrack`'s queues and automatic video rule.

1. **Packet and fidelity harness.** Separate demux from decode as on the
   web. Start with our checked H.264 AVI reader and shared output-order /
   exact-seek logic. Bounded packet/configuration/index exports and a
   checked host reader now feed native H.264 MP4/Matroska while retaining
   StepMania's DTS rules; wider codecs still need their own output models.
   Extend `--video-check` to choose and compare backends: every timestamp
   and seek must match, bit-exact codecs' planes must match, IDCT codecs
   must meet at least 60 dB per plane. Include long GOPs, nonzero starts,
   dropped frames, EOF, rapid seeks, variable frame rate and malformed
   input. Fixture success alone never broadens a capability gate.
2. **macOS copied-plane decoder.** VideoToolbox behind the existing video
   feature, initially the proven H.264 subset. Bound the callback queue;
   retain/release pixel buffers explicitly; drain/flush on seek; tag
   callbacks with generations so old results cannot enter a new movie.
   Probe actual hardware sessions, preserving software/module fallback.
   Streaming HEVC/VP9 subsets are implemented (4e); wider profiles, AV1 and
   other codecs need their own capability and fidelity checks. A watchdog
   abandons a stuck session and its outputs without blocking the game.
3. **Windows copied-plane decoder.** Experimental Media Foundation/D3D11
   implementation and a strict deferred-host test suite are in place, below.
   Auto stays on the module until real Windows fidelity, device-loss,
   performance and isolation evidence exists. Cross compilation cannot
   verify codec availability or GPU output.
4. **GPU planes where they pay.** IOSurface/Metal now has a rendered
   benchmark and an opt-in play path (4f), with copied fallback and bounded
   lifetime protection. Validate on other Macs before changing defaults.
   D3D11/DX12 sharing still needs compatible adapters, fences and Windows
   runtime evidence; preserve the copied route.
5. **Linux capability spike.** VA-API, below, if it benefits the remaining
   hardware. The module continues to cover Linux without it.
6. **Sandboxed native FFmpeg child, later.** Build the same pinned tarball,
   decoder list and `ddivideo.c` as a replaceable shared library, assembly
   on, `FF_IDCT_AUTO`, 2–4 decoder threads. Load it only in the helper,
   never the game. Reuse the fidelity harness and require a working OS
   sandbox before offering this backend to players; an unsandboxed
   experimental toggle is not part of this plan. Mac first, then Windows
   and Linux with their own tested restrictions. Ship the module too.

### Windows: Media Foundation with D3D11 (experimental, 2026-10-17)

`desktop/src/video/mf.rs`, `mf/surface.rs` and the portable `mf_core.rs`
implement the copied-plane path behind `video`, using `windows` 0.62.2
(MIT OR Apache-2.0). This was written and cross-compiled on the Mac. **No
Windows decoder, device or driver has been exercised here.** The maintainer's
Windows verification is deferred; this is not a completed Windows rollout.

`mf/api.rs` loads MF entry points only on opt-in, from System32, and keeps
that DLL loaded through COM teardown. Missing MF libraries/exports return
a refusal instead of adding an unconditional executable import (important
for Windows N). Startup on a Windows N installation still needs validation.

Windows **Auto remains the module**. Strict diagnostics use
`--video-check FILE --decoder mediafoundation`; `--decoder both` compares
the platform's strict native decoder with the module (MF on Windows, VT on
macOS). For deliberate play experiments,
`DDI_VIDEO_DECODER=mediafoundation-auto` opts into MF with module recovery;
`mediafoundation` reports refusals instead. These settings are explicit and
unsaved. Missing/failed transforms, unsupported streams, identity errors and
worker deadlines recover through the existing exact-position module path
only in the opt-in auto mode. No codec extension is installed automatically.

The first class reuses the checked H.264/HEVC/VP9 header and output models
from macOS, but requires MP4/Matroska/WebM packets with usable, distinct
container PTS. AVI is declined. Actual PTS, rounded to MF's 100 ns units,
correlates an output sample to its submitted picture; it never supplies the
game's output time. The codec model still assigns each picture the normalized
DTS of its releasing packet. Unknown, duplicated, missing or excessively
reordered sample times refuse the path. The identity map has at most 32
pending entries, 64 recent entries and an eviction watermark. Seek creates
a fresh session and namespace. Input H.264/HEVC uses checked Annex B with
parameter sets in the type and repeated before the first sample.

COM/MF startup, transform negotiation, submission, drain and teardown live on
the bounded movie worker. Enumeration tries at most 16 synchronous,
asynchronous or hardware registrations that accept the codec and NV12. A
transform must advertise D3D11 awareness, accept the hardware D3D11 device
manager, and provide its own DXGI output samples. Async transforms are
unlocked and polled through NeedInput/HaveOutput/DrainComplete credits; no
callback borrows Rust state. Polling has a two-second deadline and bounded
event bursts inside the existing four-second worker deadline. A timed-out
worker is abandoned and further native opens disabled for the process, as
on macOS; this cannot kill a driver call already stuck inside Windows.

Output is progressive NV12 at the checked geometry, matrix and range. Only
zero-origin display apertures are accepted. Decoder samples retain their
surface while a D3D11 staging copy is mapped; checked pitch and full allocated
luma height locate the UV plane, then visible pixels are copied to owned
I420. Device removal, a different owning device, invalid texture layout,
unsupported crop, format changes or incomplete drain fail the session.
Output queues retain the shared 32-frame/128 MiB bounds. No renderer or DX12
interop is introduced.

The diagnostic label is `Media Foundation (D3D11; experimental)`. A hardware
D3D11 device and DXGI output **do not prove hardware codec execution**, and
Media Foundation is not inherently a sandbox. Some OS transforms wrap DXVA
inside a synchronous MFT. Neither hardware use nor process isolation is
claimed without Windows measurements. Further codecs (AV1, MPEG-2/MPEG-4,
WMV), software/system-memory output and D3D11/DX12 sharing remain later.

Deferred-host recipe (from a Windows checkout with Rust and FFmpeg's
libx264/libx265/libvpx-vp9 encoders available):

```powershell
.\video\check-windows.ps1 -Module C:\path\to\ddivideo.wasm
```

The module may be the unchanged build from another host. The runner sets a
scratch home/cache, restores environment afterwards, records OS/GPU driver,
Rust/FFmpeg versions and module SHA-256, then runs the three ignored tests
in `desktop/tests/video_media_foundation.rs`. It opens no window or sound
device. Missing encoders/module, unavailable codecs and missing DXGI output
**fail**, rather than turn the hardware suite into skipped successes. Each
codec has a separate test so failures identify the unsupported class.
Generated cases cover B/no-B pictures, both container families, fractional
rates, nonzero starts, VFR gaps, long GOPs, 720p, full-range BT.709, VP9
lossless, every harness exact seek/EOF/loop, stored members, simultaneous
sessions and superseded seeks. Strict and recovery selections must match
module metadata, frame timestamps and I420 CRCs; refusal cases include
10-bit input, AVI without packet PTS, HEVC multiple slices and open-GOP seeks.
Default Auto must select the module. Retain the timestamped report folder
and generated clips on failure. This runner itself also awaits Windows use.

Before broadening Auto: run that suite on real Windows hardware, test
missing-codec/software-only systems and Windows N editions, exercise device
removal/adapter switching, measure decode/readback and in-play frame pacing,
and identify each selected transform's hardware/process boundary. Record
those results here; cross-compilation and the portable state/layout tests
cannot substitute for them. GPU sharing then needs compatible adapters,
DX12 import, fences and sample lifetime through GPU completion.

Primary API contracts:
[`MFTEnumEx`](https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mftenumex),
[asynchronous MFTs](https://learn.microsoft.com/en-us/windows/win32/medfound/asynchronous-mfts),
[D3D11 video decoding](https://learn.microsoft.com/en-us/windows/win32/medfound/supporting-direct3d-11-video-decoding-in-media-foundation),
[H.264 decoder](https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-decoder),
[System32-only loading](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-loadlibraryexw),
[Windows N media features](https://support.microsoft.com/en-us/windows/experience/platform-variants/media-feature-pack-for-windows-n).
Codec availability depends on installed extensions, edition, profile and
GPU; the [codec inventory](https://learn.microsoft.com/en-us/windows/win32/medfound/codecobjects)
is a list of candidates, not a capability promise.

### Linux: VA-API

Use [libva's capability queries](https://intel.github.io/libva/group__api__core.html)
(`vaQueryConfigProfiles`, entrypoints and surface attributes) on the
selected render node. H.264/HEVC/VP9/AV1/MPEG-2 support depends on GPU and
driver; MPEG-4 Part 2 is not a reliable baseline. VA-API exposes codec
picture/slice buffers, not an MF-style generic compressed-sample decoder:
our parser must construct those, or a later sandboxed FFmpeg helper must
own that path. This is substantially more work than the VT binding.
Start with mapped planes; `vaExportSurfaceHandle` can expose DMA-BUFs for
a later Vulkan import, subject to modifiers, plane formats, adapter and
fence compatibility. Driver code executes in the client process; VA-API
alone does not provide the module's memory isolation. Capability failures
and unsupported drivers use the module; no dependency is added now.

### Later helper: IPC and ownership

One helper process serves movies on a worker, matching today's host.
Use a versioned, length-prefixed binary pipe protocol with bounded payloads;
requests carry request id, movie id and seek generation. Messages are
`Open`, `Seek`, `Want`, `Close`, `Probe`, `Recycle`, with replies for metadata,
frame readiness, EOF and errors. Never send Rust structs as ABI bytes.
The helper requests `ReadAt(movie, offset, length)`; the parent validates
checked offset arithmetic and length (at most the existing 64 KiB read
window), then serves only that movie. No paths, arbitrary descriptors or
network access cross the boundary. Cap outstanding requests and log bytes.

The parent allocates a fixed-capacity shared frame ring (start with three
slots per movie, a global memory ceiling). Negotiate dimensions/strides
within today's image limits, validate all lengths/offsets with checked
arithmetic, and keep authoritative allocation metadata privately in the
parent. Treat shared headers as untrusted; a compromised helper can mutate
them after a check. Copy validated metadata once and derive plane slices
from parent-owned bounds. The helper publishes a slot only after writing
its planes; the parent acknowledges `Recycle` only when upload/sampling
no longer needs them. Seek increments the generation, drops stale frames,
and drains outstanding slot ownership before reuse. A malicious child
may corrupt pixels, but never controls a host pointer or allocation size.

A child crash, malformed reply or missed 10 s deadline kills that helper,
invalidates all its outstanding movies/generations and reopens needed
movies in the module at their requested positions; a later open may start
a fresh helper. Test process death, blocked pipes, a spinning decoder,
message floods, oversized pictures, stale generations and ring exhaustion.
OS sandbox tests must demonstrate denied file and socket access, alongside
working inherited pipes/shared memory, before packaging enables the child.

Costs: a 1080p I420 frame is 3,110,400 bytes, three slots 9.3 MB per
movie; four simultaneous movies need about 37 MB just for rings. At
60 fps a full plane copy is 187 MB/s per movie, before GPU upload. Shared
memory avoids sending those bytes through pipes but may still need a
copy from FFmpeg's strided buffers; benchmark it. Control messages and
64 KiB file reads add IPC/wakeups; the helper also adds startup, decoder
state, another build and per-OS signing/sandbox maintenance. Section 4's
native speedups exclude all those costs. The earlier unsandboxed helper
line-count estimate is not an implementation budget.

## 8. Dependencies and packaging

No production video dependency is added by this spike. For macOS, use
objc2 framework bindings for VideoToolbox/CoreMedia/CoreVideo/Metal alongside
the existing objc2 family; the experiment's raw FFI is not the production
wrapper. On Windows use the existing `windows` crate with explicit features.
Verify current releases and licences when adding either. Linux needs a
libva binding only when that step is built.

The later helper can use `libloading` (ISC), `memmap2` (MIT OR Apache-2.0)
and per-OS sandbox APIs. Section 5's sandbox crates are candidates from the
first survey, not frozen versions. Keep the C shim: the successful Rust
port added bindgen/libclang without isolating FFmpeg. The native build
uses the same pinned FFmpeg and notices as the wasm module; one library
per platform, with macOS arm64/x86_64 as needed, tested through the shared
harness. Package helper and library together, with the exact source and
rebuild instructions; follow [FFmpeg's distribution checklist](https://www.ffmpeg.org/legal.html).
Signing, library replacement and sandbox entitlements need an explicit
packaging test. Keep the earlier builds until the maintainer chooses to
remove them.

## 9. Remaining validation

- Sections 4b–4e implement the checked H.264/HEVC/VP9 subsets and the
  bounded packet interface. Wider profiles and output classes (including
  hidden/superframe VP9, HEVC long-term references and higher bit depths)
  remain with the module until their own fidelity checks pass.
- Section 4f supersedes the import-only spike with rendered fidelity,
  throughput and lifetime tests plus opt-in play. Other Macs, physical GPU
  loss, real presentation latency and manual play remain unverified; the
  copied path remains the default.
- Default vs background QoS is a useful weak-CPU proxy, not a general
  prediction for older Intel machines, Windows GPUs or laptop battery life.
- Windows has a cross-compiled experimental implementation; Linux remains
  researched only. Neither has run on a host here. Each requires hardware
  capability/fidelity tests and fallback tests on its own platform.
