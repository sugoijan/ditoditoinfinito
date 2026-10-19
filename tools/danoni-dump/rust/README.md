# Rust DanOni import SDK

An embeddable native URL-to-library pipeline, with a CLI over the same Rust APIs.
The [installed runtime](../../../docs/research/danoni-rust-runtime.md) supplies the
confined Ruffle worker, packaged Latin/Japanese fonts and dependency notices.
Supported native imports need no Python, Node, browser, compiler or FFmpeg command.
The default Ogg Opus encoder and decoder are pure Rust. Optional Vorbis encoding
uses bundled C libraries; TLS also has native dependencies. HTML5 browser capture remains separate.

The default `native` feature supplies HTTP/cache, filesystem and worker adapters.
`--no-default-features` leaves the in-memory APIs and host-supplied assets. The
default `opus` feature supplies Opus; `vorbis` separately enables compatibility
exports (JSON codec `"ogg"`). The SDK and native importer helper share the
game's root workspace and lockfile, while remaining outside the game binary's
dependency tree, including WASM. Code is MIT under REUSE. Third-party sources,
music, artwork and generated packs stay outside Git.

## Entry points

- `runtime::import_url` / `import-url HOST.json URL`: installed runtime and shared
  host defaults. The host config supplies cache, staging, store and fetch policy.
- `import_job::execute`: progress/cancellation-aware URL import and atomic managed
  store admission. `bind-import` / `resume-import` install a previously verified
  Ready candidate without refetching or executing the worker.
- `collection`: static publisher/event-list discovery and bounded per-work imports;
  results retain exclusions, failures and partial success.
- `preparation::prepare` / `prepare-fresh`: fresh audio, tempo, preview, title
  artwork and SSC preparation from normalized source evidence.
- `work::prepare_work` / `prepare-work`: assemble songs using supplied, verified
  media and review receipts. `rebuild-accepted` is the old-corpus migration bridge.
- `worker_build`: prepare pinned Ruffle sources and package an explicitly built
  worker. Compilation is a maintainer step, never part of importing a URL.

Supported playable modes are 5, 7, 9A and 9B. MacOS and Linux x86-64 have confined
worker adapters; Linux needs Landlock ABI >= 6, seccomp and Vulkan/Mesa for rendered
captures. Windows and other Linux architectures cannot execute movies yet. Exact
source frames and derived SSC timing remain separate; musical fits are provisional,
and arbitrary author engine customizations are outside the playable export scope.

The [desktop probe](../../../docs/research/danoni-desktop-integration.md) links the
SDK with the real loader and play session. The optional desktop importer uses
a separate helper process; see [the build contract](../../../docs/plans/danoni-importer.md).
See the [consolidation review](../../../docs/research/danoni-consolidation.md)
for cleanup decisions and follow-ups, and the
[Sarasa comparison](../../../docs/research/danoni-font-comparison.md) for the optional
font study (production fonts are unchanged).

## Commands

Run from the repository with Rust 1.99 or later. `just danoni-check` checks
the workspace SDK and two separate research packages, including synthetic SSC/Opus
handoff (Python 3 and ffmpeg are maintainer-only requirements). It clears
optional corpus/runtime environment switches. `just danoni-check --minimal`
checks the SDK without native/codec features. Neither runs public URL imports
or builds a worker. More focused commands:

```sh
cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust
cargo clippy --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust --all-targets -- -D warnings
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust -- compile-chart < /tmp/request.json > /tmp/result.json
DDI_DANONI_EXPORTS="$HOME/Music/DanOni/exports" cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust --test local_corpus -- --nocapture
# Full offline rebuild with explicit accepted codec-stage inputs, into a fresh tree:
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust -- rebuild-accepted "$HOME/Music/DanOni/export-registry.json" "$HOME/Music/DanOni/exports" /tmp/danoni-rust-new
# One portable job, with all paths interpreted through its explicit roots:
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust -- prepare-work /tmp/job.json /tmp/danoni-song-new
```

The local differential test reads accepted `reports/*/manifest.json`, original
source charts/merged metadata preserved in derived references, reviewed policy
inputs and accepted SSC files. It never downloads or runs work scripts, writes
corpus files or changes exports. No corpus means the test reports a skip explicitly.

## Chart API and request

Call `compile_chart(&Chart, &Metadata, &PreparationOptions)` directly. All source
arguments are borrowed. The caller supplies already merged work/reference/chart
metadata and validated, hash-bound review decisions. The core does not certify
unverified caller-supplied motion/grid/rating evidence. The complete-song `prepare_work` API
performs those checks before calling it. Unknown source chart/lane fields are
preserved in flattened maps and not evaluated.

CLI stdin is at most 16 MiB and contains exactly one request:

```json
{
  "schema": "ddi-danoni-compile-request/1",
  "chart": {
    "id": "normal",
    "mode": "5",
    "lanes": [
      {"tapsSeconds": [1], "holdsSeconds": []},
      {"tapsSeconds": [], "holdsSeconds": [[2, 3]]},
      {"tapsSeconds": [], "holdsSeconds": []},
      {"tapsSeconds": [], "holdsSeconds": []},
      {"tapsSeconds": [4], "holdsSeconds": []}
    ],
    "difficultyStars": 4,
    "speed": [{"seconds": 1, "multiplier": 1.5}]
  },
  "metadata": {},
  "options": {
    "grid": {"bpm": 120, "beatZeroSeconds": 0, "subdivisionsPerBeat": 2, "maximumErrorMs": 9},
    "nativeMotion": {"baseArrowHeightsPerSecond": 2.8},
    "scrollFactor": 1,
    "tierPolicy": "stars",
    "tierBounds": [1, 3, 5, 7],
    "rating": {"kind": "source"},
    "preserveLabel": false
  }
}
```

Only `schema` and `chart` are required; metadata defaults empty, and each option
has the shown default except `grid`/`nativeMotion`, which default null. With no
grid, exact source frames become ticks at 75 BPM. With a grid, finite
60–360 BPM, phase within ±600 s and subdivisions 1/2/3/4/6/8 are accepted; the
budget must be positive and below 10 ms (default 9 ms). Times are audio-relative
seconds; custom note times can be quantized with a grid, while speed event times
retain the existing exact-source-frame requirement. Lane order is the normalized
built-in order documented in the preparation contract, not guessed from lane names.

`rating:{"kind":"reviewed","stars":N}` supplies an already validated source
rating; null explicitly selects unknown. Source rating extraction honors numeric
chart-over-metadata precedence and pure ★ strings, never arbitrary difficulty
names. Unknown uses Edit/0. Bounds are inclusive and strictly increasing.
`tierPolicy:"edit"` selects Edit without discarding a known numeric meter.
Chart `metadata.difficultyLabel` becomes CHARTNAME for unrated charts or when
`preserveLabel` is true. Unknown modes (including 9i/9t) reject deserialization.

## Chart result and units

Success stdout is `ddi-danoni-prepared-chart/1`:

- `id`, `mode`, `difficultyStars` (integer or null), `difficultyClassification`.
- `timingBpm`, `offsetSeconds` (SSC sign convention), `paddingTicks` and
  `sscBaseRatio` (source arrow heights/s × factor × 60 / timing BPM).
- `lanes[]`: `taps` and paired `holds`, signed **unpadded** ticks, 48 per beat,
  plus `collisionSafeguard` when the whole lane uses fine-tick fallback.
- `endpoints[]`: lane/index, `kind` (`tap`, `hold-head`, `hold-tail`),
  `sourceSeconds`, `exportSeconds`, `shiftMs`, signed unpadded `tick`, and
  `method` (`exact-source-frame`, `rhythmic-subdivision`, `fine-tick-fallback`,
  `lane-collision-fine-fallback`). Both hold endpoints are included.
- `scrollEvents[]`: source/export seconds, shift ms, unpadded tick and original
  multiplier; effective SSC ratios also include `sscBaseRatio`.
- `maximumNoteErrorMs`, `unrepresentedEffects` (nonempty colors/boost/stops/effects
  counts) and `ssc` (one complete NOTEDATA block; no music/banner/preview headers).

Native motion adds a baseline SCROLLS segment and preserves the user's multiplier.
Musical pre-roll is padded by whole beats; positive phase with native motion may
add whole measures to make the baseline active before audio zero. Source seconds
are not rewritten. Source/policy evidence remains the caller's responsibility.

Failures exit nonzero, emit `{code,message}` JSON on stderr and produce no result.
Request/options reject unknown fields to catch configuration typos. Source maps
allow unknown factual fields. Finite-number and relationship checks apply equally
to direct library calls and JSON calls.

Intentional tightening over the old helpers: reject >999 raw stars, nonfinite or
>±7200 s source times, more than 1,000,000 note endpoints, >100,000 speed events,
>20,000 emitted measures, malformed effect arrays and scroll multiplication
that overflows. These do not change the accepted corpus. Python's ties-to-even
rounding is preserved. SSC uses shortest round-trip Rust decimal formatting;
whitespace/numeric spelling and archive bytes are not promised identical.

## Stage contracts

The CLI exposes individual stages for research and recovery. Each calls the same
library implementation as the URL pipeline; job/result schemas and evidence
requirements live in the linked contracts. Output directories must be fresh and
outside Git. Saved-corpus receipts are local comparison evidence, never fixtures
or bundled songs.

| Stage | Commands / API | Contract |
| --- | --- | --- |
| Song assembly, artwork and archives | `prepare-work`, `rebuild-accepted`, `artwork`, `package-generation`, `verify-generation`, `promote`, `recover` | [Offline preparation](../../../docs/research/danoni-rust-offline.md) |
| Decode, resample and origin alignment | `prepare-audio`, `audio` | [Audio](../../../docs/research/danoni-rust-audio.md) |
| Musical grid estimates and evidence | `estimate-tempo`, `tempo` | [Tempo](../../../docs/research/danoni-rust-tempo.md) |
| Passive runtime projection and SWF audio | `project-native`, `demux-native` | [Extraction](../../../docs/research/danoni-rust-extraction.md) |
| Source-bound normalized works | `normalize-native` | [Normalization](../../../docs/research/danoni-rust-normalization.md) |
| Confined capture | `capture-native`, `supervisor` | [Supervisor](../../../docs/research/danoni-rust-supervisor.md) |
| Worker build and package | `prepare-worker`, `package-worker`, `verify-worker` | [Worker](../../../docs/research/danoni-rust-worker.md) |
| Selector/capture orchestration | `extract-native`, `orchestration` | [Orchestration](../../../docs/research/danoni-rust-orchestration.md) |
| Static page and historical cache inspection | `inspect-cached-page`, `replay-cache` | [Acquisition](../../../docs/research/danoni-rust-acquisition.md), [cache](../../../docs/research/danoni-rust-network.md) |
| Policy-aware HTTP | `fetch-url`, `network::Fetcher` | [HTTP](../../../docs/research/danoni-rust-http.md) |
| URL extraction and preparation | `extract-url-native`, `prepare-url-native` | [URL extraction](../../../docs/research/danoni-rust-url-native.md), [preparation](../../../docs/research/danoni-rust-url-preparation.md) |
| Ready candidate admission and store | `inspect-import`, `install-import`, `create-store`, `verify-store`, `recover-store` | [Import](../../../docs/research/danoni-rust-import.md), [managed store](../../../docs/research/danoni-rust-managed-store.md) |
| Progress, cancellation and resume | `import-url-native`, `bind-import`, `resume-import` | [Import jobs](../../../docs/research/danoni-rust-import-job.md) |
| Static collections | `discover-index`, `import-index-native`, `import-discovered-native` | [Collections](../../../docs/research/danoni-rust-collections.md) |

Ogg Opus (`codec: "opus"`) is the fresh-import default. Quality 0.5 selects
96 kbit/s mono or 192 kbit/s stereo; the 0..1 scale spans 64..128 kbit/s per
channel. Playback is 48 kHz with pre-skip and exact final-granule trimming;
source-rate PCM remains the tempo/preview reference. See the [Opus contract](../../../docs/research/danoni-rust-opus.md).
Vorbis (`codec: "ogg"`, feature `vorbis`) and WAV (`codec: "wav"`) remain explicit
options. `--no-default-features --features native` gives WAV-only native output
and requires selecting `wav` in requests. Origin correction is measured against the source runtime;
BPM fitting does not alter that origin. Fits remain provisional even when every
endpoint stays within the 9 ms budget. Source colours, boosts and unsupported
presentation effects remain in provenance with explicit omissions.

Rendering uses portable wgpu readback. Software-only selection requires an
installed CPU adapter; see [rendering](../../../docs/research/danoni-rust-rendering.md)
and the [Linux handover](../../../docs/research/danoni-rust-linux.md).
