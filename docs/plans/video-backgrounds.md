# Implementation plan: video backgrounds (2026-10-09)

The research and every decision are in
[`../research/video-backgrounds.md`](../research/video-backgrounds.md)
(read it first; sections 5 and 6 are the design and the toolchain trial).
This document turns them into steps, in order, each ending in a commit the
maintainer makes. `docs/PLAN.md` decisions 18–19 (still backgrounds, shared
folders) and 33 (linked folders) are the ground this builds on; the timing
model in `AGENTS.md` is untouched: nothing here feeds the judge.

What gets built, in one sentence: `#BGCHANGES` movies play behind the field
in the browser, decoded either by WebCodecs (H.264, hardware) or by a
cut-down FFmpeg compiled to a separate wasm module that is fetched only
when a song needs it, scheduled from the audio clock like the still images,
with a Video setting whose default turns a movie off when it costs frames.

## Step 0. Repository layout and conventions

- `video/` (new, top level): the FFmpeg module's sources and build.
  - `video/ddivideo.c`: the C shim (appendix A).
  - `video/build.sh`: downloads the pinned wasi-sdk and FFmpeg tarballs into
    `target/video/` (never into the repository), verifies their SHA-256,
    runs the configure line (appendix B), builds the four libraries, links
    the shim, strips, writes `target/video/ddivideo.wasm` and
    `target/video/ddivideo.json` (version, configure line, size). Idempotent:
    skips what is already built. Runs on macOS (arm64) and Linux (x86_64,
    CI); no Docker.
  - `video/FFMPEG.toml`: version, git tag, source URL, licence, configure
    line, SHA-256 of the tarballs; read by `xtask gen-credits` for the
    Credits screen and `CREDITS.md`.
- `justfile`: `just video` runs the build; `just build`/`just check` do not
  need it (the app works without the module: section "Without the module"
  below).
- `index.html`: `<link data-trunk rel="copy-file" href="target/video/ddivideo.wasm" data-target-path="video" />`
  only when the file exists: Trunk fails on a missing file, so the copy
  line is written by the `regen-seo`-style pre-build hook (`xtask video-link`)
  that emits an include fragment, or the module is produced by a Trunk
  pre-build hook that runs `video/build.sh` when its inputs exist. Choose
  the simpler of the two after reading `Trunk.toml`; CI builds the module
  before `trunk build`.
- `LICENSES/LGPL-2.1-or-later.txt`; `REUSE.toml` annotation for
  `video/ddivideo.c` (MIT, ours) and a note that the built module is
  LGPL-2.1-or-later (built artifacts are not committed).
- CI (`.github/workflows`): a `video` job that caches `target/video/` keyed
  by `video/build.sh` + `video/FFMPEG.toml`, runs `video/build.sh`, and the
  `build` job that consumes it before `trunk build`.
- Test videos: never in the repository. Local third-party files from
  `~/Music/Simfiles` are copied into a scratch directory for one-off runs
  only. For fixtures that may be committed, generate tiny clips with
  `ffmpeg -f lavfi -i testsrc=size=64x48:rate=30 -t 2 -c:v mpeg4 x.avi`
  (and `-c:v libx264`/`-c:v mpeg2video -f mpegvideo`): synthetic, ours.

## Step 1. The module and its build (commit 1)

1. Add `video/ddivideo.c` from appendix A, changed from the trial so that
   `ddi_next` hands over YUV 4:2:0 planes instead of RGBA (research 5.6):
   one contiguous buffer `Y (w×h), U, V ((w+1)/2 × (h+1)/2)`, plus
   `ddi_colorspace()` (`f->colorspace`: BT.601 = 5/6, BT.709 = 1, else
   unspecified → 601 for height < 720, 709 otherwise, as players do) and
   `ddi_full_range()` (`f->color_range == AVCOL_RANGE_JPEG`). Drop
   `swscale` from the configure line and the link. Keep `ddi_seek(t)`:
   `av_seek_frame` to the keyframe before `t` then decode-and-drop until
   `pts ≥ t` (used when a change is entered late or on loop); on failure
   reopen.
2. `video/build.sh` and `video/FFMPEG.toml` as above. Pin wasi-sdk 34.0 and
   FFmpeg n9.0.2 (the versions the trial used).
3. A native test harness is not possible (the module is wasm); instead
   `video/check.mjs` (Node ≥ 22, `node:wasi`) opens a given file, decodes N
   frames, prints ms/frame and writes one frame as PNG (appendix C). `just
   video-check <file>` runs it. Used by hand and in the CI `video` job on a
   generated 2-second clip of each codec.
4. Expected: `--enable-small -Oz` ≈ 1.5 MB raw / ≈ 630 KB gzipped (the
   trial's numbers with swscale; without it, less); Xvid 640×360 ≈ 0.8
   ms/frame under V8.

Verification: `just video` on macOS; CI green on Linux with the same
hashes; `check.mjs` decodes the three codecs.

**Done 2026-10-09.** What differs from the above, and what later steps need:

- Module: 1,123,345 bytes, 528,520 gzipped. Exports only the `ddi_*`
  functions (`--export=` per function, not `--export-dynamic`, which also
  kept FFmpeg's public API); 1 MiB stack placed first (an overflow traps);
  memory capped at 1 GiB; frames over 3840×2160 refused. Xvid 640×360
  0.74 ms/frame, H.264 852×480 2.7 ms, 1280×720 3.8 ms under Node.
- Imports: 20, all `wasi_snapshot_preview1`, but not quite the trial's
  set: `environ_get`, `environ_sizes_get`, `clock_time_get`, `fd_close`,
  `fd_fdstat_get`, `fd_fdstat_set_flags`, `fd_filestat_get`,
  `fd_prestat_get`, `fd_prestat_dir_name`, `fd_read`, `fd_readdir`,
  `fd_seek`, `fd_write`, `path_filestat_get`, `path_open`,
  `path_remove_directory`, `path_rename`, `path_unlink_file`,
  `poll_oneoff`, `proc_exit`. Step 2's worker implements these.
- Planes: any 8–16-bit planar YUV or grey becomes 8-bit 4:2:0 in the
  shim (4:4:4 and 4:2:2 chroma averaged, high bit depth rounded).
- Timestamps follow StepMania 5.1 (`MovieDecoder_FFMpeg::DecodePacketInBuffer`
  stamps a frame with `frame->pkt_dts`, the packet that released it): with
  B-frames the first frame is at one or two frame periods, not 0, as packs
  were timed against. Step 4 shows the first frame from the segment's start
  ("the first frame is always shown", research section 4); step 6 must stamp
  WebCodecs frames the same way. `ddi_duration` is −1 when the container
  only guesses it from the bitrate (raw MPEG-2): the end is known at EOF.
- Seeking lands on the same frame, byte for byte, as decoding straight
  through: it backs off by the reorder delay, goes back one keyframe more
  when the target was an open-GOP B-frame the decoder drops, and decodes
  from the start when a seek finds nothing (an AVI whose index is lost to
  truncation). A seek in 720p H.264 with long GOPs took up to 1.2 s, so
  step 4 opens and seeks early.
- `video/clips.sh` (behind `just video-check`) generates eight clips with
  FFmpeg's test pattern (the three pack codecs, 4:4:4, 4:2:2, 10-bit,
  tagged and untagged BT.709) and compares every frame with the `ffmpeg`
  command's decode; H.264 is bit-exact, MPEG-4/MPEG-2 ≥ 78 dB.
- FFmpeg comes from the signed ffmpeg.org release tarball, not GitHub's
  generated archive (stable bytes). `LICENSES/LGPL-2.1-or-later.txt` is not
  committed (`reuse lint` rejects a licence no tracked file uses); the
  build copies FFmpeg's `COPYING.LGPLv2.1` to
  `target/video/ddivideo.LICENSE.txt`, served next to the module.
- CI caches only the outputs, keyed on `build.sh`, `FFMPEG.toml` and the
  shim, and uploads them as the `ddivideo` artifact for step 2's build.
- Reproducible across hosts (fixed 2026-10-10): CI's Linux module first
  came out 22 % larger than the macOS one. The verbose link showed why:
  clang's wasm driver runs whichever `wasm-opt` is on `PATH` after
  linking (Homebrew's on the Mac, none on the runner), and Homebrew's
  `C_INCLUDE_PATH`/`CPLUS_INCLUDE_PATH`/`LIBRARY_PATH` reached the wasm
  compiler too (no header was used: the libraries matched). `build.sh` now
  clears those variables, links with `--no-wasm-opt` and runs a pinned
  Binaryen (`version_133`, per-host SHA-256 in `FFMPEG.toml`) itself; a
  clean macOS rebuild gives the same module byte for byte. CI keeps the
  verbose link log (`link.out`) in the `ddivideo-build` artifact.

## Step 2. The worker (commit 2)

`app/src/web/video_worker.js` (plain JS, served by Trunk `copy-file`; it
is small and it must run before any Rust) and `app/src/web/video.rs` (the
Rust side that owns the `Worker`).

The worker:
- Fetches `video/ddivideo.wasm` on first use (`WebAssembly.instantiateStreaming`),
  reports download progress to the page (`{type: "loading", loaded, total}`),
  caches nothing itself (the browser's HTTP cache does).
- Implements the 20 `wasi_snapshot_preview1` imports the module has
  (research section 6): a file table where fd 3 is the preopened directory
  `/v`; `path_open("/v/<name>")` returns a new fd bound to a `Blob` the page
  sent; `fd_read` reads with `FileReaderSync` in 64 KB slices from the fd's
  position; `fd_seek`, `fd_filestat_get` (size), `fd_fdstat_get`, `fd_close`
  as expected; `clock_time_get` from `performance.now()`; `fd_write` to 1/2
  logs to the console; `environ_*` empty; `proc_exit` throws; `fd_readdir`,
  `path_*` others, `poll_oneoff` return `ENOSYS` (52); `path_open` of
  `/dev/urandom` returns `ENOENT` (44). Errno values: WASI preview1.
- Protocol (page → worker): `open {id, blob, name}`; `seek {id, t}`;
  `want {id, n}` (decode up to `n` more frames); `close {id}`.
  Worker → page: `opened {id, width, height, duration, codec, colorspace,
  full_range}`; `frame {id, pts, planes: ArrayBuffer}` (transferred); `eof
  {id}`; `error {id, message}`. Frames are decoded only on `want`, so the
  page controls the queue depth (3 ahead).
- One worker per play session, created when the session has a movie to
  show, terminated with the session.

Rust side (`video.rs`): a `VideoDecoder` trait in `platform` (configure
with a codec id and an opaque source; `seek`; `want(n)`; `poll() ->
Vec<Frame>`; `close`) with this worker as its web implementation; `Frame
{pts, width, height, planes}`. The desktop implementation (later) wraps
`ffmpeg-next`.

Verification: a headless-Chrome page (verify skill) loads the module from a
worker and decodes a generated clip; the same page run in Firefox and
Safari by the maintainer, which settles research section 7's first item.

**Done 2026-10-09.** What differs, and what later steps need:

- Firefox (maintainer, by hand, five local pack movies): the module loads
  in the worker and every seek is exact; H.264 640×360 1.7–2.2 ms/frame,
  MPEG-2 640×360 0.8 ms, Xvid 320×240 0.5 ms, the slowest seek 0.5 s.
  SpiderMonkey is as fast as V8 here.
- Safari (maintainer, published site, ten local movies of every kind):
  the same, within about 20 %: H.264 720p 4.2–4.6 ms/frame, 480p 2.7–3.4,
  640×360 2.1, MPEG-2 1.0, Xvid and DivX 0.3–0.5; every seek exact; the
  MP4-in-`.avi` movie refused as expected. The module needs no
  per-browser rule: `auto` (step 5) judges by the session. A hidden tab
  throttles the check page's polling (Safari: about once a second), which
  inflated its "with hand-over" times and one seek to 9 s while the tab
  was in the background; the decode times measured in the worker were
  unaffected, and play happens in a visible tab. This settles research
  section 7's first item.

- The worker script is embedded in the app (`include_str!`) and started
  from a blob URL instead of served with `copy-file`, so a cached old
  script can never speak an older protocol than the app. It fetches
  `video/ddivideo.json` revalidated, then `ddivideo.wasm?v=<sha256>`, with
  progress against the size in the JSON (the body arrives decompressed).
  A failed download is retried by the next open.
- The module reaches the site through a Trunk `post_build` hook,
  `xtask stage-video`, which copies `target/video/ddivideo.{wasm,json,LICENSE.txt}`
  into `video/` when they exist; `index.html` is unchanged and a build
  without the module still works. CI's `build` job needs `video`, downloads
  its artifact after rust-cache (which may clean `target/`) and fails if
  the site lacks the module.
- Protocol: `want` carries the seek generation; a seek cancels the wants
  from before it and the end cancels the rest (`eof` once per generation).
  `VideoDecoder::pending()` says how many frames are still coming, so step
  4 asks for `ahead − queued − pending`. Frames also carry their colour
  tags and decode time.
- Crashes: anything thrown out of the module (trap, `abort()`, a stack
  overflow, which V8 reports as `RangeError`) fails every open movie and the
  one being opened, and the next open gets a fresh instance; the worker's
  `onerror` fails every movie too.
- Seeking now lands on the frame showing at the target when frames are
  missing (Xvid's dropped frames hold the one before): the last frame that
  starts by the target, held by reference. `video/clips.sh` has a clip
  with a gap; both checks seek into every gap.
- Step 4: loop when the decoder reports `End`, not at `duration` (an H.264
  AVI with B-frames reports 2.0 s with its last frame stamped 2.033 s);
  `End` carries the last frame's time.
- `#/video-check` (unlinked) decodes picked movies in the worker, reports
  ms/frame and seeks compared by fingerprint, and mirrors the results to
  `window.__DDI_VIDEO_CHECK`; `?src=a,b` checks files of the site.
- Headless Chrome on an Apple Silicon laptop: Xvid and MPEG-2 640×360
  0.8 ms/frame, H.264 852×480 3.0 ms, 1280×720 3.9 ms; every seek exact,
  the slowest 0.85 s (720p H.264). Random and empty files fail with a
  message; a truncated AVI plays what it has.
- One local movie is not AVI: `REVOLUTION.avi` (DDR 2013) is H.264 in an
  MP4 container under an `.avi` name (the research note's survey counted
  it as AVI), so step 3's sniffer must look at the bytes, not the
  extension. That led to the format decision below.

**Formats (2026-10-10, maintainer's decision): everything StepMania plays.**
StepMania 5.1 builds its FFmpeg (a June 2026 commit) with every built-in
decoder, demuxer and parser and zlib (`CMake/SetupFfmpeg.cmake`); OutFox's
engine source is not public, so StepMania is the reference. The module now
has all 355 demuxers, 67 parsers, the 264 video decoders (FFmpeg's whole
video set less the hardware and external-library ones; not the audio and
subtitle decoders, since a movie's sound is ignored), zlib 1.3.2 (compiled
by `build.sh`; Zlib licence, for step 7's credits) and swscale for the
pixel formats the shim does not copy itself (RGB and palette images from
older codecs, packed YUV). No AV1, as in StepMania: FFmpeg decodes it only
through hardware or libdav1d. Size: 5,889,297 bytes, 2,446,734 gzipped
(StepMania's exact set, with the audio decoders, would be 3.85 MB
gzipped), fetched only for songs that have a movie. Measured before the
decision: MP4/MOV alone +66 KB gzipped, WebM with VP8/VP9 +169 KB, WMV +95
KB, Ogg/Theora +28 KB. Decoding is as fast as with the AVI-only module.
`video/clips.sh` checks 30 synthetic formats (MP4, HEVC, MOV, MKV, WebM
VP8/VP9, Ogg Theora, WMV, FLV, DivX 3, MPEG-1/2 program and transport
streams, MJPEG, HuffYUV, FFV1, Cinepak, MS Video 1, ZMBV, PNG, QuickTime
RLE, and the AVI set); a clip whose encoder the local ffmpeg lacks is
skipped and named. MPEG program streams have no index, so a seek lands
between keyframes: the shim now steps back 0.5, 1, 2 and 4 s from where a
seek landed, then decodes from the start, until the target frame comes
out first. WebCodecs (step 6) still comes first where the browser has the
codec; the module is the fallback for everything.
- Step 7, from FFmpeg's checklist (https://ffmpeg.org/legal.html) and
  LGPL-2.1 §6: serve the FFmpeg source tarball from the same site (item 8),
  and how the module was built: `ddivideo.c`, `build.sh`, `FFMPEG.toml`
  (item 6; the shim is linked into the same file); the credit line with a
  link to that source (item 9); consider a name that says FFmpeg
  (item 16, no obfuscated names). wasi-libc is linked in too: its musl
  (MIT) and cloudlibc (BSD-2-Clause) notices must go with the module (the
  wasi-sdk tarball has none; take them from the wasi-libc repository).

## Step 3. Import: know which songs have playable movies (commit 3)

- `library/src/video.rs`: `sniff(head: &[u8]) -> Option<VideoCodec>` from
  the first 64 KB: RIFF `AVI ` → `strh`/`strf` fourcc (`H264`/`avc1`/`X264`
  → H264; `XVID`/`xvid`/`DX50`/`DIVX`/`MP4V`/`FMP4` → Mpeg4; `MPG2`/`mpg2`
  → Mpeg2; else `Other(fourcc)`); bytes `00 00 01 B3` → Mpeg2 (raw
  stream); `ftyp` at offset 4 → `Other("mp4")`; anything else `None`.
  Natively tested with synthetic headers.
- `ManifestEntry.bg_videos: Vec<VideoRef {name, path, codec}>` (serde
  default), filled by `import_song` from the movies `#BGCHANGES` names
  (reusing `backgrounds::unshown_files`, which already lists them) when the
  file exists in the import or in a shared folder; the import report's
  note "videos are not played" becomes "N videos will play / M in a format
  the game cannot play (codec)".
- Storage: linked imports (decision 33) link movies like other files
  (`bg/<path>` → `Link`); copying browsers copy a movie only when the new
  import option `import_videos` (default off) is on, else record it as
  `absent` with a note. Shared-folder movies: same rule, stored under
  `shared/` keys with the same option.
- `backgrounds::schedule` gains `BgImage::Movie(String)` segments for
  names that resolve to a playable video; the `-random-` marker still picks
  from images only (StepMania picks random movies; out of scope).

Verification: native tests for `sniff` and the schedule; headless Chrome
import of a scratch pack with the three codecs plus an unsupported one,
checking the report and the manifest.

**Done 2026-10-10.** What differs, and what later steps need:

- Formats: with every StepMania format playable (step 1's decision), the
  sniffer returns a `VideoFormat {container, codec}` (`platform::video`;
  serde as `{"container":"avi","codec":"h264"}`) for AVI, MP4/MOV,
  Matroska/WebM, Ogg, ASF, FLV, MPEG program, transport and raw streams;
  unrecognised bytes are `unknown` and still tried. Only AV1 is
  unplayable (and so never stored). A file is a movie by StepMania's
  extensions (`video::MOVIE_EXTENSIONS`), as `ActorUtil` decides, then by
  its bytes.
- MP4s usually keep their index after the media data: the top-level box
  sizes in the first 64 KB say where (`mp4_index_at`), and the import reads
  up to 4 MB from there for the codec. Of a deflated zip entry only the
  start is inflated (`zip::inflate_prefix`), so an MP4 inside a compressed
  zip keeps an unknown codec. All 188 local movies sniff as `ffprobe`
  reads them (`DDI_MOVIES=<dir> cargo test -p ddi-library local_movies`).
- Manifest: `bg_videos: [{name, format, stored}]`; `bg_shared` now lists
  absent movies as well as images (`absent_files`), and play time splits
  the shared ones (`LoadedSong::shared_videos`). The schedule takes the
  playable movie names and gives `BgImage::Movie` segments; until step 4
  draws them they show the song background (`shown` falls back), so play
  looks as before. `load_background` already returns a movie's blob,
  stored, linked or shared.
- Storage: linked imports link movies (`bg/<name>`); copying ones store
  them only with the new "also store background videos" box (setting
  `import_videos`, default off; added here rather than in step 5, since
  the import needs it), otherwise `stored: false` and a note. Shared-folder
  movies follow the same rule: copied as blobs under `shared/`, or, when
  linked, a `SharedLink {folder, path, entry}` JSON at the same key; a kept
  folder that such a link uses is not dropped as unused.
- Report: "N background videos will play", "kept N shared background
  videos", "left out N background videos (\"also store background videos\"
  is off)"; warnings for unplayable formats (named, e.g. "AV1 in
  Matroska") and for images or videos found nowhere; scripted animations
  are the only "not played" notes left.
- Checked in headless Chrome with a scratch pack (synthetic H.264 AVI, MP4
  with the index at the end, MPEG-2 program stream, AV1 MKV, a missing
  movie, a scripted animation, a shared movie and image): copied with the
  box off and on, linked through a kept OPFS folder (then re-read after
  removing the song folder: the folder and the shared link stay), and a
  deflated zip; the linked song autoplays past its movie changes on WebGPU
  and WebGL2 with every judgement top tier and no console errors.

## Step 4. Drawing movies (commit 4)

- `render/src/background.rs`: a second background texture kind, YUV: three
  `R8Unorm` textures (Y, U, V) and a fragment shader variant that converts
  with BT.601 or BT.709 and limited or full range from a uniform; the
  `Backdrop` mix (crossfade) works unchanged across kinds. `Renderer::
  add_video(width, height)` returns an id like `add_background`;
  `update_video(id, planes)` writes the three planes with `write_texture`
  (odd sizes: chroma is `(w+1)/2`). A native WGSL validation test like the
  one in `render/src/sprite.rs`.
- `app/src/game_loop.rs`: `MovieState` per scheduled movie segment:
  `open` 1 s before the segment starts (decoder `open` + `want 3`); on each
  frame compute `movie_time = (song_time − segment.seconds) × rate`
  (`rate` from the change's field, default 1), apply loop as `movie_time
  mod duration` when known (`seek` on wrap), take the newest polled frame
  with `pts ≤ movie_time`, drop older ones, `want` as many as were
  consumed, upload at most one frame per display frame. Show nothing new
  while the decoder is behind (the last frame stays, as StepMania does).
  A frame decoded more than 0.5 s late twice is "cannot keep up" (step 5).
  Close the decoder when the segment ends and nothing else uses the movie.
- `shown()` in `library::backgrounds` resolves `BgImage::Movie` to the
  video's texture id or, before its first frame arrives, to the song
  background (so a change to a movie never flashes black).
- Memory: at most two movies open at once (current and the next segment);
  the YUV textures are reused when sizes match.

Verification: a scratch pack with a synthetic 30 fps clip whose frames are
numbered (`drawtext`), autoplayed headless on WebGPU and WebGL2; sample the
background pixel colour at known song times to confirm the frame shown is
the one `movie_time` says (within one frame), including after a loop and
after starting mid-segment (`&song_offset` via the per-song offset or a
late `#BGCHANGES` beat); `__DDI_DEBUG` gains `video: {decoded, dropped,
late}`.

**Done 2026-10-10.** What differs, and what later steps need:

- The movie clock follows StepMania's source, not the sketch above
  (`Background.cpp`, `BackgroundEffects/Stretch*.lua`,
  `MovieTexture_Generic.cpp`, 5_1-new). Field 3 is the rate (`StringToFloat`:
  an empty field is 0, a frozen movie); field 5 = 0 is `StretchNoLoop` and
  field 6 ≠ 0 `StretchRewind` (field 7 names the effect directly): those
  start over at each change, `StretchNoLoop` holds its last frame. Every
  change naming a file shares one movie texture (`RageTextureManager`), so
  the default effect resumes where the file was when it lost focus, a
  restart resets that for the later changes, and once a change stops it
  looping it stays so (`backgrounds::movie_position`, `movie_loops`). A
  movie fading out is paused. A change to the movie already shown is kept
  when it restarts it or changes its rate. Looping restarts the clock at
  0.5 s, StepMania's "best effort" (`platform::video::LOOP_RESTART`).
- Deliberate differences: StepMania shows at most one frame per update and
  never drops one, so a slow display drifts behind the music; here the
  newest frame not after the clock shows, frames passed over are dropped,
  and the movie stays on the music. After a loop StepMania flashes frame 0
  and catches up; here it goes straight to the frame due. A movie whose
  change is earlier than StepMania's own start time is a little further
  in here. Chroma is sampled centre-sited (H.264 and MPEG-2 are
  left-sited: half a luma pixel, invisible on a dimmed background).
- `platform::video::MovieTrack` is the playback policy, natively tested
  with a fake decoder: frames kept three ahead; the first frame of a run as
  soon as it arrives; seeks when the clock jumps (not when the decoder is
  merely slow: its frames show as they come, counted late), never while a
  seek is unanswered; a run from the start keeps its frames (up to 24 MB)
  and, reaching the end within them, plays every later lap from memory
  without seeking (a seek per lap would be slower than a short movie's
  lap); the end learned by one track seeds the movie's next ones; laps
  saturate. The worker reports the file's last frame time at the end even
  when a seek past it decoded it without handing it over
  (`ddi_last_time`).
- `app/src/movies.rs` (`MovieDeck`): the current and next movie segments
  (opened 2 s ahead) at most; a decoder moves to its movie's next segment
  instead of reopening the file; one texture per movie, shown only once it
  holds a frame of the current run (a retry or a restarting change shows
  the song background until then); a movie that fails shows the song
  background and is tried again on a retry, with a new worker if the old
  one stopped; decoding stops when the song ends (the results show the
  song background). The renderer's YUV kind (three `R8Unorm` planes,
  BT.601/709, limited/full range, linearised on sRGB targets) draws behind
  the field with the images' brightness and crossfades.
- Checked headless on WebGPU and WebGL2 with numbered clips (each frame a
  colour coding its number) read back from the screen at chosen song
  times (`window.__DDI_SONG_TIME`): first run, image between, resume,
  rewinding loop (from memory), double rate, a long clip wrapping at its
  end, a change at a negative beat (joined 8 s in), two movies alternating
  every 0.5 s, one movie in consecutive segments with rewinds and a rate
  change, a broken and a missing movie, a retry; every sample on the frame
  due. A local pack song (H.264 640×360) plays at 60 fps with no late or
  dropped frame. An independent review (movie clock against StepMania,
  the app's lifecycle, the render path) found the resume-past-the-end
  freeze, the per-lap seeking of short loops, slow-seek live-lock, the
  per-file texture sharing and the rest fixed above.
- Step 5's "cannot keep up" rule can read `MovieStats::late` (and
  `dropped`) from `MovieDeck::stats`.
- Songs without background changes: StepMania's `Song::TidyUpData` plays
  the only movie among the song folder's own files from beat 0, without
  looping ("DWI style"), when neither layer has a change
  (`backgrounds::implicit_movie`; the import keeps it like a named one).
  Two local songs rely on it (Koi hadou Moro Hadou OK Houteishiki!!, DDR
  2014; REVOLUTION, DDR 2013) and play.
- Fit: backgrounds and movies are both covered (StepMania's default
  `BackgroundFitMode`, `CoverPreserve`). Many pack movies are 4:3 pictures
  pillarboxed in a 16:9 frame next to a 4:3 background still, so on a
  wide screen the change looks like a zoom; StepMania's other fit modes
  (`FitInside`, `FitInsideAvoidLetter`, `FitInsideAvoidPillar`,
  `CoverDistort`) align them at the cost of bars. Step 5 adds that
  preference to the options; cropping a movie's own black bars would align
  them without bars (not in StepMania; later, if wanted).

## Step 5. The setting and the auto rule (commit 5)

- The background frame (decided 2026-10-10, the maintainer's idea): pack
  assets were drawn for a cabinet's screen, not for the player's, so
  backgrounds and movies are fitted (StepMania's `CoverPreserve`) into a
  virtual frame, and outside it the screen shows a blurred, dimmed
  extension of what is in the frame (moving with the movie; a small blur
  pass at low resolution). `Settings.background_frame`: `Auto` (default),
  `4:3`, `16:9`, `Screen` (the whole display: today's behaviour,
  StepMania's on a wide display).
  - Auto takes the song's movie picture: its frame less its own black bars
    (JOMANDA's movie, DDR 2014, is a 4:3 picture pillarboxed in 852×480,
    so the frame is 4:3, its 4:3 still covers it exactly and the bars are
    cropped: no jump at the change, as on the cabinet). Without a movie,
    the song background's shape; with several movies, the first one's.
    (KUNG FU FIGHTING, the song that raised this, has a full 16:9 movie
    and a 4:3 still: its frame is 16:9 and the change still zooms; the
    4:3 setting lines the two up there.)
  - A local survey (188 songs with one movie, 2026-10-10): 70 have a 4:3
    still and a full 16:9 movie, 30 both 4:3, 22 a 16:9 still and a 4:3
    movie, 12 both 16:9, 24 a square jacket as background, about 25 a
    movie with bars inside its frame. Stills and movies come from
    different releases, so the movie, which plays most of the song, sets
    the frame; where the still differs in content it still zooms at the
    change, no worse than now.
  - Bars are found before the movie shows (a frame change mid-song would
    be a jump of its own): the worker samples a handful of frames spread
    through the movie while the start prompt is up, skipping all-black
    ones (fade-ins), and takes the largest non-black area that stays
    consistent; about a second per movie, overlapped with the prompt.
- `Settings.video: VideoMode {Auto, On, Off}` (lenient, default Auto);
  options page: "Background videos" with the three choices and one line of
  explanation. (The `import_videos` checkbox came with step 3.)
- Auto: while a movie is being drawn, if `FpsMeter.fps` stays below 0.8 ×
  the estimated refresh rate for 3 consecutive seconds, or the decoder
  reports "cannot keep up", stop the video for the rest of the song (the
  song background shows), remember `video_stopped: Some(reason)` in the
  session, and show it on the results ("video was turned off for this
  song: frames were being dropped" with a link to Options). Never during
  the first 2 s of a movie (textures warming up). `On` never stops; `Off`
  never opens a decoder or fetches the module.
- The start prompt shows "loading the video decoder… 40 %" while the
  module downloads, and the song can be started before it finishes (the
  movie joins when ready).

Verification: force the rule with a debug query parameter (`&slow=1`
making the loop sleep) in headless Chrome and check the results card;
autoplay stays all top tier throughout (video never touches judging).

**Done 2026-10-10.** What differs, and what later steps need:

- The frame snaps to a cabinet shape: Auto takes the nearest of 4:3 and
  16:9 (split at their geometric mean, 1.54), so a square jacket fills a
  4:3 frame and an ultrawide picture a 16:9 one. It settles once a movie
  frame shows, or when the song starts with no probe running, and never
  changes after that (`GameLoop::frame_aspect`). The renderer draws the
  layers cover-fitted into the frame through the viewport; the extension
  is what the frame shows, enlarged to cover the screen (so a movie's own
  bars stay out of it), drawn at 1/12 of the screen (192 px at most),
  blurred by a 13-tap Gaussian, one pass each way (σ = 3 texels, about 36 screen
  pixels) and composited at 60 % of the frame's brightness, premultiplied
  (`render/src/blur.wgsl`, `background::frame_rect`). Nothing is drawn
  outside a frame within a pixel of the screen.
- The probe runs in the worker (`probe` message): six frames at 15–90 % of
  the duration (or at 1–20 s when it is unknown), each the first after a
  new shim export, `ddi_seek_key` (keyframe at or before the time, no
  exact decode; under 2 ms a sample on the clips). Without a keyframe
  index (MPEG-TS) a seek can land past the last keyframe; that sample is
  redone with `ddi_seek`. The Y planes come back and
  `platform::video::picture_bounds` takes the union of the non-black
  areas (a row or column is black at mean luma ≤ 28 and no sample above
  80, limited range; scaled for full range), skipping all-black frames.
  Sample aspect ratios are ignored, as StepMania ignores them. The module
  check (`check.mjs`) now runs six keyframe seeks on every clip and
  checks each gives a frame decoded straight through.
- The automatic rule is `platform::video::VideoGuard`, natively tested: a
  movie (each new segment) warms up for 2 s, then three measured seconds
  in a row below 80 % of the refresh rate measured before the play, or
  two in a row with at least two late frames each (one stall, a
  collection pause, makes one burst, not two), turn the movies off for
  the rest of the play. Retry plays them again. The results card says
  "video was turned off for this song: frames were being dropped" (or
  "the decoder could not keep up") with a link to the options.
- The worker starts, and fetches the module (`load` message), as soon as
  the first movie file of the song is read, during the start prompt;
  the prompt shows "loading the video decoder… N %" and the song can
  start at any time. Off reads no movie file: no worker, no fetch.
- Verified on JOMANDA (local, 852×480 H.264 with bars): the probe found
  (106, 0, 640, 480), as `cropdetect` does; full plays on WebGPU and
  WebGL2 all top tier with 3714 frames shown, none dropped or late; with
  `&slow=1` the movie turned off about 5 s in, the card said so and the
  judgements stayed all top tier; with On and `&slow=1` it played on at
  24 fps; Off fetched nothing. Throttled to 1.5 MB/s the prompt counted
  from 3 % to 99 %.

## Step 6. WebCodecs for H.264 (commit 6)

- `library/src/avi.rs`: a RIFF/AVI demuxer that yields the video stream's
  chunks with frame index, keyframe flag (`idx1` `AVIIF_KEYFRAME`, or the
  `movi` scan when there is no index) and the stream's `scale`/`rate`;
  natively tested on a generated clip. H.264 in AVI is Annex B with
  in-band SPS/PPS, which is what `VideoDecoder` takes with no
  `description`.
- A second `VideoDecoder` implementation (`app/src/web/webcodecs.rs`):
  `isConfigSupported({codec: "avc1.<profile from SPS>", optimizeForLatency: true})`,
  chunks as `EncodedVideoChunk {type: key|delta, timestamp: index × scale /
  rate × 1e6}`; output `VideoFrame`s uploaded with
  `copy_external_image_to_texture` (`ExternalImageSource::VideoFrame`, both
  backends) into an RGBA background texture, then `close()`d. Runs in the
  worker too (it holds the file), with the `VideoFrame` transferred.
- Selection per movie: H.264 and `isConfigSupported` → WebCodecs; else the
  FFmpeg module. The sniffed codec in the manifest makes the choice
  without opening the file, so the module is fetched only when a song's
  movies need it (research 5.1–5.2).

Verification: headless Chrome plays an H.264 AVI through WebCodecs (debug
state says which decoder); Firefox and Safari by the maintainer.

## Step 7. Documentation and handover

- `docs/PLAN.md`: phase 8 row (video done, what is left: overlays,
  DanOni back/mask), decisions 34 (video pipeline) and 35 (the setting),
  risks (module size, codecs the module does not have, Firefox/Safari
  worker differences).
- `.claude/skills/verify/SKILL.md`: how to run the module check, the
  synthetic clips, the debug state, the `&slow=1` parameter.
- `AGENTS.md`: one line under Layout for `video/` and one under Commands
  for `just video`; keep it short.
- Credits: FFmpeg entry generated from `video/FFMPEG.toml` with the version
  and a link to the source tag and the configure line (LGPL notice).

## Without the module

The app must keep working when `target/video/ddivideo.wasm` is absent
(`just dev` without `just video`, a build host without the toolchain):
movies in a format WebCodecs handles still play; the others show the song
background, and the results card says "video decoder not included in this
build" once. The fetch of a missing module is a 404 handled like any
load failure, never a crash.

## Order of work and what to measure first

1. Steps 1–2 settle the only open questions (the module in a worker in all
   three browsers; Firefox/Safari decode speed). Stop and report after
   them if any browser cannot run it.
2. Steps 3–5 are the feature.
3. Step 6 is an optimisation for H.264 (the module already decodes it) and
   can slip.

Each step: native tests where the code is native, headless Chrome on
WebGPU and WebGL2 for the rest, an independent review before the handover
of steps 2, 4 and 6 (worker protocol, scheduling, demuxer), and a
suggested commit message. Adversarial inputs for the demuxer and the
sniffer are welcome (truncated AVIs, huge chunk sizes, zero-size streams),
run under the memory watchdog like every ad-hoc binary.

## Appendix A. The shim as validated on 2026-10-09 (RGBA variant)

Step 1 changes it to hand over planes; this is the version that was built
and measured.

```c
// Decoder shim: opens a file through libavformat, decodes the best video
// stream, converts frames to RGBA. Exported for the worker.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libswscale/swscale.h>
#include <libavutil/imgutils.h>

#define EXPORT __attribute__((visibility("default"), used))

// wasi-libc declares `clock()` but this sysroot does not define it;
// libavutil's random seed calls it.
#include <time.h>
clock_t clock(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (clock_t)(ts.tv_sec * 1000000 + ts.tv_nsec / 1000);
}

typedef struct {
    AVFormatContext *fmt;
    AVCodecContext *dec;
    AVPacket *pkt;
    AVFrame *frame;
    struct SwsContext *sws;
    int stream;
    uint8_t *rgba;
    int width, height;
    double time_base;
} Video;

EXPORT Video *ddi_open(const char *path) {
    Video *v = calloc(1, sizeof *v);
    if (!v) return NULL;
    if (avformat_open_input(&v->fmt, path, NULL, NULL) < 0) goto fail;
    if (avformat_find_stream_info(v->fmt, NULL) < 0) goto fail;
    const AVCodec *codec = NULL;
    v->stream = av_find_best_stream(v->fmt, AVMEDIA_TYPE_VIDEO, -1, -1, &codec, 0);
    if (v->stream < 0 || !codec) goto fail;
    AVStream *st = v->fmt->streams[v->stream];
    v->dec = avcodec_alloc_context3(codec);
    if (!v->dec || avcodec_parameters_to_context(v->dec, st->codecpar) < 0) goto fail;
    v->dec->workaround_bugs = 1;
    v->dec->error_concealment = 3;
    if (avcodec_open2(v->dec, codec, NULL) < 0) goto fail;
    v->pkt = av_packet_alloc();
    v->frame = av_frame_alloc();
    v->time_base = av_q2d(st->time_base);
    return v;
fail:
    avcodec_free_context(&v->dec);
    avformat_close_input(&v->fmt);
    free(v);
    return NULL;
}

EXPORT int ddi_width(Video *v) { return v->dec->width; }
EXPORT int ddi_height(Video *v) { return v->dec->height; }
EXPORT const char *ddi_codec(Video *v) { return avcodec_get_name(v->dec->codec_id); }
EXPORT double ddi_duration(Video *v) {
    return v->fmt->duration > 0 ? v->fmt->duration / (double)AV_TIME_BASE : -1.0;
}

// Decodes the next frame into the RGBA buffer; returns its presentation
// time in seconds, -1 at the end, -2 on error, -3 without a timestamp.
EXPORT double ddi_next(Video *v) {
    for (;;) {
        int r = avcodec_receive_frame(v->dec, v->frame);
        if (r == 0) break;
        if (r == AVERROR_EOF) return -1.0;
        if (r != AVERROR(EAGAIN)) return -2.0;
        r = av_read_frame(v->fmt, v->pkt);
        if (r < 0) {
            avcodec_send_packet(v->dec, NULL);
            continue;
        }
        if (v->pkt->stream_index == v->stream) avcodec_send_packet(v->dec, v->pkt);
        av_packet_unref(v->pkt);
    }
    AVFrame *f = v->frame;
    if (f->width != v->width || f->height != v->height || !v->rgba) {
        v->width = f->width;
        v->height = f->height;
        free(v->rgba);
        v->rgba = malloc((size_t)v->width * v->height * 4);
        sws_freeContext(v->sws);
        v->sws = NULL;
    }
    v->sws = sws_getCachedContext(v->sws, f->width, f->height, f->format, v->width, v->height,
                                  AV_PIX_FMT_RGBA, SWS_BILINEAR, NULL, NULL, NULL);
    uint8_t *dst[1] = { v->rgba };
    int stride[1] = { v->width * 4 };
    sws_scale(v->sws, (const uint8_t *const *)f->data, f->linesize, 0, f->height, dst, stride);
    int64_t pts = f->best_effort_timestamp;
    double t = pts == AV_NOPTS_VALUE ? -3.0 : pts * v->time_base;
    av_frame_unref(f);
    return t;
}

EXPORT uint8_t *ddi_rgba(Video *v) { return v->rgba; }

EXPORT void ddi_close(Video *v) {
    if (!v) return;
    free(v->rgba);
    sws_freeContext(v->sws);
    av_frame_free(&v->frame);
    av_packet_free(&v->pkt);
    avcodec_free_context(&v->dec);
    avformat_close_input(&v->fmt);
    free(v);
}

EXPORT void *ddi_malloc(size_t n) { return malloc(n); }
EXPORT void ddi_free(void *p) { free(p); }
```

## Appendix B. The build as validated

wasi-sdk 34.0 (`wasi-sdk-34.0-arm64-macos.tar.gz`, `…-x86_64-linux.tar.gz`
from `https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-34/`),
FFmpeg `n9.0.2` (`https://github.com/FFmpeg/FFmpeg/archive/refs/tags/n9.0.2.tar.gz`).

```sh
WASI=…/wasi-sdk-34.0-<arch>
./configure --target-os=none --arch=wasm32 --enable-cross-compile \
  --cc=$WASI/bin/clang --ar=$WASI/bin/ar --ranlib=$WASI/bin/ranlib \
  --nm=$WASI/bin/nm --strip=$WASI/bin/strip \
  --disable-everything --disable-programs --disable-doc --disable-network \
  --disable-pthreads --disable-w32threads --disable-os2threads \
  --disable-runtime-cpudetect --disable-autodetect --disable-debug \
  --disable-avdevice --disable-avfilter --disable-swresample \
  --enable-avcodec --enable-avformat --enable-swscale \
  --enable-decoder=h264,mpeg4,mpeg2video \
  --enable-parser=h264,mpeg4video,mpegvideo \
  --enable-demuxer=avi,mpegvideo --enable-protocol=file \
  --enable-small --extra-cflags="-msimd128 -Oz"
make -j8
$WASI/bin/clang -Oz -msimd128 -I. -o ddivideo.wasm ddivideo.c \
  libavformat/libavformat.a libavcodec/libavcodec.a \
  libswscale/libswscale.a libavutil/libavutil.a \
  -Wl,--no-entry -Wl,--export-dynamic -Wl,--export=malloc -Wl,--export=free \
  -Wl,--strip-all -mexec-model=reactor
```

(`configure` probes `stdbit.h` and fails that one test; harmless. Step 1
drops `--enable-swscale` and `libswscale.a`.) Result: 1,529,923 bytes,
627,065 gzipped; imports listed in the research note's section 6.

## Appendix C. The Node harness as validated

```js
import { WASI } from 'node:wasi';
import { readFile } from 'node:fs/promises';
const [wasmPath, dir, file, maxFrames] = process.argv.slice(2);
const wasi = new WASI({ version: 'preview1', preopens: { '/v': dir }, args: [], env: {} });
const { instance } = await WebAssembly.instantiate(await readFile(wasmPath),
  { wasi_snapshot_preview1: wasi.wasiImport });
wasi.initialize(instance); // reactor: `_initialize`, not `_start`
const e = instance.exports;
const mem = () => new Uint8Array(e.memory.buffer);
const cstr = (s) => { const b = Buffer.from(s + '\0'); const p = e.ddi_malloc(b.length); mem().set(b, p); return p; };
const v = e.ddi_open(cstr('/v/' + file));
if (!v) { console.log('open failed'); process.exit(1); }
const w = e.ddi_width(v), h = e.ddi_height(v);
let frames = 0; const t0 = performance.now();
for (;;) { const t = e.ddi_next(v); if (t === -1) break; if (t < -1) continue; frames++;
  if (maxFrames && frames >= Number(maxFrames)) break; }
const dt = performance.now() - t0;
e.ddi_close(v);
console.log(`${file}: ${w}x${h} ${frames} frames ${(dt / frames).toFixed(2)} ms/frame`);
```

Writing one frame as PNG (to look at it) is twenty more lines with
`node:zlib`'s `deflateSync`; the trial did that and the frames were right.
