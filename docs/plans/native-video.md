# Plan: native video decoding on desktop (2026-10-16)

The maintainer asked whether desktop could decode background movies
natively instead of running the web's FFmpeg module under wasmtime
(`docs/plans/desktop.md` D8, 7.0, 7.7). They named `ffmpreg`, `rsmpeg`
"and other projects", and floated one design: a Rust shim crate in place
of `video/ddivideo.c`, built as a `cdylib` for wasm (the web unchanged)
and used natively through a safe Rust API. This document surveys the
candidates, tries that design, measures native FFmpeg against the
module on this machine, and proposes what to do. Nothing in the
repository changed: the spike lives outside it, in
`/Users/jan/.claude/jobs/2f6f2abd/tmp/spike-native-video/`.

Read first: `docs/plans/video-backgrounds.md` (why the module exists,
its exact seeks, StepMania's frame times), `docs/plans/desktop.md` (D8,
7.0's and 7.7's numbers and hardening), `video/FFMPEG.toml`,
`video/ddivideo.c`, `desktop/src/video.rs`.

## In short

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
- **Proposed:** keep the module under wasmtime as desktop's decoder
  (D8 stands) and keep `ddivideo.c` as the one shim. Offer native
  decoding later, opt-in and isolated: the same shim and FFmpeg built
  natively into one shared library with the module's `ddi_*` ABI,
  loaded at run time in a decoder process with a sandbox, frames over
  shared memory. Do macOS first. Section 7 has the steps.

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
| Windows Media Foundation (`windows` 0.62.2) | 2025-10-06 | MIT OR Apache-2.0 | H.264, MPEG-4 Part 2, DivX 3 (Vista+), WMV, AVI source; MPEG-2 sometimes a Store extension; N editions have none of it | Covers most packs on Windows only; different pixels per driver |
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
| E. VideoToolbox / Media Foundation for H.264 only, the rest on the module | H.264 is decoded by the OS (on macOS in `VTDecoderXPCService`, out of process) | a second decode path per OS, like WebCodecs on the web: our AVI demuxer (`library/src/avi.rs`) and output-order replay; pixels per GPU driver; macOS lacks DivX 3 and AVI |

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
  `libddivideo.so`) holds FFmpeg and the shim. The shim is MIT, and so
  is the combined library's source. It is distributed under the LGPL,
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
    replaced dylib with another signature is refused. LGPL-2.1 (which
    FFmpeg's "or later" lets us choose) has no installation-information
    clause, unlike LGPL-3.0, so that is compliant. A user who rebuilds
    can re-sign ad hoc and use the unsigned path.
  - **Windows:** Authenticode-sign the DLL beside the executable.
  - **Linux:** the `.so` beside the binary, found through `$ORIGIN`.
- **The wasm module needs none of this:** it is data, read and checked
  by its hash. Replacing it needs no re-signing, which is one more
  thing in A's favour.

## 7. Proposal

### Decisions (proposed)

- **N1. The module under wasmtime stays desktop's decoder** (D8 is
  confirmed, not changed). It plays every pack movie in budget, it is
  the web's decoder byte for byte, and it is sandboxed by
  construction. Native is an addition, never a replacement.
- **N2. `ddivideo.c` stays the one shim** for every target. The Rust
  port works (section 3), but it adds libclang and a second libc to
  the web's build and makes nothing safer. Revisit it if the shim
  grows.
- **N3. Native decoding, when built, is "the module, natively":**
  - one shared library per platform, built by `video/build.sh
    --native` from the same tarball, decoder list and shim, exporting
    the same 16 `ddi_*` functions;
  - loaded at run time, so the host's code is the same against both
    (the wasmtime calls become function pointers);
  - FFmpeg's assembly on and the shim unchanged (`FF_IDCT_AUTO`).
    Frame times and seeks compare exactly against the module. Pixels
    compare exactly for the bit-exact codecs (H.264, HEVC, VP8/9,
    lossless) and within 60 dB PSNR for the IDCT codecs. Native x86-64
    and arm64 also differ from each other there, so CI needs the bound
    anyway;
  - threads: `thread_count` 2–4 natively (frame threading, same
    frames).
- **N4. Native only in a decoder process** (option C), with the OS
  sandbox (D) before it is on by default anywhere. Until a platform has
  its sandbox, native is an opt-in setting ("Fast video decoder
  (experimental)") with the module as the default and the fallback.
  Never in the game's process (B).
- **N5. Not now:** `rsmpeg` (stale), `ffmpreg` (no video), pure-Rust
  decoders (none complete; oxideav to re-check in 2027), GStreamer,
  libmpv. VideoToolbox/Media Foundation for H.264 (E) is a separate,
  later question, worth asking only if 1080p packs become common.

### Steps (each a commit the maintainer makes)

Nothing here blocks anything: every movie plays today. In order of
value:

**Step 1. A native driver for the checks.** `video/check` grows a native
twin of `check.mjs`. It takes the same options and runs the same
checks: frame times and seeks exact, pixels exact or within a PSNR
bound per codec (`clips.sh` already carries these bounds). With it, the
native library is checked by the same clips as the module. It is
small, and the other steps need it.

**Step 2. The native library's build.** `video/build.sh --native [--small]`:
- `FFMPEG.toml`'s configure with the native changes from section 4
  (`--enable-static --enable-pic`, assembly on, pthreads, no
  cross-compile);
- then the four static libraries and `ddivideo.c` linked into one
  library exporting `ddi_*` only:
  - macOS: `-dead_strip -exported_symbols_list`, `install_name
    @rpath/…`;
  - Linux: `-Wl,--gc-sections`, a version script;
  - Windows: a `.def` file;
- the notice file and `ddivideo.json` as for the module.
- Checked by `video/clips.sh` through step 1's driver.
- **CI, a `video-native` job on three OSes:**
  - macOS 14 arm64, plus x86_64 cross (`--arch=x86_64 --cc="clang
    -arch x86_64"`), merged with `lipo`;
  - Ubuntu 22.04 for an old enough glibc;
  - Windows through MSYS2 with clang, or cross-compiled on Linux with
    llvm-mingw. Plain C ABI, no CRT objects crossing it (`ddi_free`
    frees what the library allocates), so a mingw DLL is fine beside
    an MSVC game.
  - Outputs cached and keyed like the module's.
  - Cost: about 3–5 minutes a platform on a cold cache.

**Step 3. The decoder process.** `ddi-video-helper`, a small binary in the
workspace:
- it loads the library and speaks `VideoHost`'s messages (open, seek,
  want, close, probe, recycle) over a pipe, with frames in a
  shared-memory ring;
- the game serves its file reads, so the child opens nothing;
- `desktop/src/video.rs` gets a second backend behind the same
  `VideoBackend`/`VideoDecoder` traits;
- a child that dies or misses the 10 s deadline counts as a trap does
  today: open movies fail, the next open starts a new child, the module
  takes over.
- Tests: `desktop/tests/video.rs` through both backends; the hostile
  cases of 7.7 (a spinning decoder, 64 MiB of messages, a huge
  picture) against the child.

**Step 4. The sandbox, macOS first.**
- The helper signed with the App Sandbox entitlement (no files, no
  network) or `sandbox_init`, plus a test that the child cannot open a
  file or a socket. Then Linux (seccomp + landlock), then Windows
  (AppContainer or a restricted token).
- Native becomes the default on a platform once its sandbox test is
  green in CI. Until then it is the setting under Options, Graphics.

**Step 5. Packaging,** with desktop's packaging plan:
- the library and the helper in the bundle (`Contents/Frameworks`,
  `Contents/MacOS`), both signed and notarised;
- on Windows, the DLL and the helper beside the executable,
  Authenticode-signed;
- FFmpeg's tarball or the offer, `ddivideo.LICENSE.txt`, and the
  credits line (`xtask gen-credits` already reads `FFMPEG.toml`);
- the module stays shipped too, as the fallback.

**Kept from today:**

- **One shim, one decoder list, one tarball:** `video/ddivideo.c`,
  `FFMPEG.toml` and the pinned FFmpeg release.
- **The host:** `VideoHost`'s thread, its message protocol, frame
  recycling, the probe, the deadline and the crash rules.
- **The fallbacks:** the module for the web and as desktop's default
  and fallback; the no-decoder fallback (the song background and the
  results' line).
- **The checks:** `clips.sh`, `--video-check` and
  `desktop/tests/video.rs`, which keep comparing exact frames.

## 8. Dependencies (only if steps 3–4 are built)

| Crate | Version | Licence | Where | Note |
|---|---|---|---|---|
| libloading | 0.8.x | ISC | desktop | loads the native library in the helper |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 | desktop, helper | the frame ring |
| seccompiler | 0.5.0 | Apache-2.0 OR BSD-3-Clause | helper (Linux) | |
| landlock | 0.4.7 | MIT OR Apache-2.0 | helper (Linux) | |
| rappct | 0.13.3 | MIT | helper (Windows) | or the `windows` crate directly |
| FFmpeg | 9.0.2 | LGPL-2.1-or-later | the native library | as the module; a separate file |
| zlib | system (macOS, Linux) or 1.3.2 | Zlib | the native library | |

Not needed by the proposal, only by the Rust-shim variant:
`ffmpeg-sys-the-third` 6.0.0+ffmpeg-9.0 (WTFPL; bindgen 0.72 and
`clang` 2.0, BSD-3-Clause and Apache-2.0, at build time).

## 9. Risks and open questions

- **Windows' native build** was not tried. It is the riskiest platform
  for FFmpeg's toolchain, so mingw cross-builds from Linux are the
  plan's default.
- **Measurements under load.** CPU time per thread is robust to load,
  but the M1's efficiency cores run slower per instruction. All
  decoders ran interleaved under the same load and default QoS. Repeat
  on a quiet machine before quoting absolute wall times.
- **The opt-in (N4) needs a reason to exist.** If nobody's machine
  struggles with the module, step 3 onwards can wait indefinitely.
  The trigger: a player's report of late movie frames, or 1080p60
  packs.
- **Battery:** native at about a third of the CPU is real on laptops.
  Not measured.
