# Rust DanOni export core

Linux x86-64 worker packages and capture use Landlock ABI >= 6 plus seccomp;
see [the Linux handover](../../../docs/research/danoni-rust-linux.md) for runtime
requirements, software-renderer tests and remaining corpus/distribution gates.
Linux rendering currently requires native scale; enlarged captures reject before
launch to preserve the kernel file-size limit. Windows and other Linux
architectures cannot execute movies.

An embeddable [Rust migration](../../../docs/research/danoni-rust-plan.md) of the
DanOni export pipeline. The pure API now prepares complete SSC songs from
normalized works, source-bound reviews and supplied PCM/playback media. An
optional `native` feature (enabled by default) supplies confined filesystem reads,
fresh output directories, collection packaging and journaled publication; `--no-default-features` leaves host-supplied
asset access and in-memory artifacts. The game's workspace/WASM build is unchanged.

Fetching remains host-owned. Native work execution uses the installed confined
Ruffle worker, with source-bound normalization, explicit selector orchestration
and bounded dependency recovery. The [automatic menu profile](../../../docs/research/danoni-rust-profiles.md)
now plans reviewed 5/7-key engine controls and motion observations from runtime
state, without a per-song capture recipe. The
[variable-travel profile](../../../docs/research/danoni-rust-scheduled-profiles.md)
adds a verified five-key gen3 subset with source-derived flight windows and
observed built-in Practice settings. Its
[dependency-recovery entry point](../../../docs/research/danoni-rust-profile-recovery.md)
asks a host resolver for missing inputs before menu discovery and retries under
the same operation-wide bounds. The [hold-head/intro-input extension](../../../docs/research/danoni-rust-hold-motion.md)
adds generic hold witnesses and bounded source-derived physical inputs, recapturing
all roles and preserving full source/audio proofs. Optional zero-based
`ProfileRequest.selectors` records unselected rows explicitly. The
[early menu profile](../../../docs/research/danoni-rust-early-profiles.md) extends
automatic selection to a shared gen1 five-key engine, including its renderer and
bounded control-role identity inventory. The
[native audio stage](../../../docs/research/danoni-rust-audio.md) decodes supplied
normalized sources and produces WAV/Ogg neutral receipts in process. Accepted
media and reviewed banners can still be reused byte-identically;
the existing Python/Node workflow remains the extraction/codec/analysis reference.
Code is MIT under REUSE; all third-party inputs/results stay outside Git.

The [URL-to-prepared-song bridge](../../../docs/research/danoni-rust-url-preparation.md)
now joins URL extraction, fresh audio/tempo analysis, pickup previews, source-star
classification and optional host-font title banners. `url_prepare::execute` and
`prepare-url-native BUNDLE JOB.json NEW_OUTPUT` write new local candidate song
folders; the reusable `preparation::prepare` API accepts normalized inputs.
BPM fits remain provisional, and installation in the desktop library is separate.

The [prepared-import handover](../../../docs/research/danoni-rust-import.md)
validates outer Ready outputs against the real library APIs, assigns stable
source/audio song identities and provides duplicate planning and host transaction
traits. `inspect-import READY_DIRECTORY` is read-only. `import_host::install`
coordinates the host's atomic commit, rollback and uncertain-commit recovery.
The native [managed-store adapter](../../../docs/research/danoni-rust-managed-store.md)
provides an immutable filesystem store with atomic catalog publication and crash
recovery. `create-store`, `install-import`, `inspect-store`, `verify-store` and
`recover-store` wrap the same SDK API; main-app wiring remains separate.
The [native import-job coordinator](../../../docs/research/danoni-rust-import-job.md)
now connects these stages through `import_job::execute`, with structured progress,
cancellation and a serializable terminal result. `import-url-native` calls the
same API; `bind-import`/`resume-import` use an exact Ready-hash token and need no
worker/network services for resumed installation.
The [reviewed collection coordinator](../../../docs/research/danoni-rust-collections.md)
adds static publisher/event-list discovery and bounded sequential imports through
those same jobs. `discover-index`, `import-index-native` and
`import-discovered-native` report missing/browser-required/unsupported entries
and partial results; earlier child commits survive later failures/cancellation.

## Commands

Run from the research checkout with Rust 1.99 or later:

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

The [complete-song API and offline jobs](../../../docs/research/danoni-rust-offline.md)
add source loading, review binding, grouping, headers, supplied artwork/media,
previews, provenance, native title/panel artwork, collection ZIPs and validated
archive/promotion/recovery. Native musical analysis now has its own versioned
API and provisional evidence; public fetching remains pending. Restricted native
capture is available as described below. The offline handover specifies all job/result schemas.


Additional CLI commands (all paths external to Git):

```sh
# Hashed font/title or hashed native snapshot/run/pixels; fresh output only:
target/danoni-rust/debug/ddi-danoni-export artwork /tmp/art-job.json /tmp/art-new
# Ready offline generation + explicit collection names and font binding:
target/danoni-rust/debug/ddi-danoni-export package-generation /tmp/package-job.json /tmp/pack-new
target/danoni-rust/debug/ddi-danoni-export verify-generation /tmp/pack-new
# Destination may be absent or a validated Rust generation; history must exist:
target/danoni-rust/debug/ddi-danoni-export promote /tmp/pack-new /tmp/library-pack /tmp/history
target/danoni-rust/debug/ddi-danoni-export recover /tmp/history/transaction-NONCE.json
```

Artwork is pure byte-in/byte-out with explicitly supplied fonts and captured
pixels. `package::write_zip` accepts a host-owned sink. The native publisher
archives old generations and recovers stale journals; it does not silently
replace the legacy Python exports or implement the desktop library UI.


Audio preparation uses the same source-bound neutral receipt already consumed
by complete-song preparation:

```sh
target/danoni-rust/debug/ddi-danoni-export prepare-audio /tmp/audio-job.json /tmp/audio-new
```

The default `vorbis` feature builds bundled C codec libraries through `vorbis_rs`;
no FFmpeg/oggenc executable is needed at runtime. Disable it with
`--no-default-features --features native` for WAV-only native output, or disable
all defaults for pure host-supplied decoding, resampling/origin and WAV APIs.
The game/WASM workspace is unchanged. See the audio handover for exact bounds,
MP3 metadata/physical-frame distinctions, origin proof scope and corpus gates.


Musical analysis uses the same normalized sources and proved PCM:

```sh
target/danoni-rust/debug/ddi-danoni-export estimate-tempo /tmp/tempo-job.json /tmp/tempo-new
```

The [tempo handover](../../../docs/research/danoni-rust-tempo.md) specifies the
source-bound API, whole-work fallback, cached-result validation and output.
`grid.json` feeds `WorkRequest.reviews.grid`. The explicit legacy-analysis bridge
reproduces accepted decisions; fresh Rust spectral analysis is experimental and
has recorded BPM/phase aliases. Native policy now accounts for coarse pulse
resolution and reports measured, ambiguous or insufficient quarter-phase support.
Mechanical 9 ms validation does not prove musical
correctness. Accepted exports are unchanged. Neither stage needs Python/Node or
an external audio command; neither implements URL discovery/runtime extraction.


Passive native extraction now reads saved, hash-bound AVM1 captures:

```sh
target/danoni-rust/debug/ddi-danoni-export project-native /tmp/projection-job.json /tmp/projection-new
```

The [extraction handover](../../../docs/research/danoni-rust-extraction.md) specifies
movie/run/config bindings, seven structural families, movement and clock APIs,
units and two historical proof gaps. This stage does not execute work code
or prepare a newly normalized song. The Rust supervisor now creates fresh captures as described below;
acquisition remains pending; the [pinned Higan/Kouhaku paths](../../../docs/research/danoni-rust-pinned.md)
now use the same projection/normalization APIs.


Saved SWF music is now inventoried/demuxed in Rust as unchanged MP3 frames:

```sh
target/danoni-rust/debug/ddi-danoni-export demux-native /tmp/demux-job.json /tmp/demux-new
```

The [SWF contract](../../../docs/research/danoni-rust-extraction.md#swf-audio-inventory-and-demux-2026-10-13)
explains FWS/CWS bounds, runtime slice offsets, stream timing metadata and
source-bound saved-run jobs. All 85 selected corpus streams match existing bytes
and metadata. Sixteen audio runs require their separate proven decoration review, now joined
by the combined normalization job. The command writes `stream.mp3`
and a final manifest, with no decoding, work execution or network access.


Complete saved structural runs now normalize directly through the same host API:

```sh
target/danoni-rust/debug/ddi-danoni-export normalize-native /tmp/native-work-job.json /tmp/work-new
```

The [normalization contract](../../../docs/research/danoni-rust-normalization.md)
defines typed movie/source/run/PCM bindings, exact cross-run decoration proofs,
portable evidence and canonical WAV output. The local gate covers 78 works /
113 charts, independently measured shared-song origins, every downstream SSC
endpoint and x1/x2/x3 authored scroll segments. Three selectors remain explicitly
unproved; accepted exports are unchanged. This removes Python from proved saved
structural normalization. The pinned Higan/Kouhaku path adds seven charts / six
songs with original reference parity, numeric stars, explicit child clocks and
measured width-based scroll. Fresh Rust worker execution is now described below; automatic selector discovery, URL acquisition and installation remain pending.


Fresh native captures use the Rust-owned supervisor on macOS and supported
x86-64 Linux hosts (see the Linux requirements above):

```sh
target/danoni-rust/debug/ddi-danoni-export capture-native /tmp/capture-job.json /tmp/capture-new
```

The [supervisor contract](../../../docs/research/danoni-rust-supervisor.md)
keeps installed worker identity separate from work data, stages only bound
inputs, enforces the fixed sandbox and resource budgets, and returns capture
bindings for normalization. No Python launcher, browser, shell, ps or FFmpeg is
needed at runtime. Ready captures and diagnostic failures have separate markers;
unsupported platforms never fall back to unrestricted execution. Seven fresh
pinned charts pass the downstream references, PCM, SSC and scroll gate. Worker
build preparation and local packaging are described below. Explicit selector
orchestration now follows; automatic discovery and URL fetching remain separate work.


The [worker preparation/bundle contract](../../../docs/research/danoni-rust-worker.md)
adds `worker_build::{prepare_sources, package_worker, load_bundle}` and three CLI
commands: `prepare-worker UPSTREAM_GIT NEW_SOURCES`, `package-worker JOB.json
NEW_BUNDLE`, and `verify-worker BUNDLE`. Preparation reads cached pinned Git
objects into a separate tree, leaving the upstream checkout untouched. Building
uses Cargo explicitly; packaging binds its selected executable hash and keeps
licences/provenance. The app loads its trusted installed bundle and passes the
returned `WorkerSpec` directly to the supervisor. No Python is needed to prepare,
package or run this worker. Windows/other-architecture confinement and
distribution notice review remain pending.


The [selector/capture orchestrator](../../../docs/research/danoni-rust-orchestration.md)
adds `orchestration::{prepare_plan, execute, execute_job}` and
`extract-native BUNDLE JOB.json NEW_OUTPUT`. One bound control config per chart
creates every role, deduplicates identical captures, preserves inputs/provenance
and normalizes the selected work. `replay-job.json` permits recapture using only
the staged archive. Only parent readiness authorizes downstream work; a failed
or cancelled plan can retain successful child captures/normalization as diagnostics.
Fresh M. MOON and stable-segment Destined Marionette close two proof gaps, with
nine-chart / 4,127-endpoint / 141-scroll-check parity. RE:RAVE's speed-bank proof,
bounded dependency fetching and automatic URL/selector discovery remain open.


Passive [source discovery and cache evidence](../../../docs/research/danoni-rust-acquisition.md)
now expose `acquisition::{inspect_page, inspect_cached_page}` and
`inspect-cached-page JOB.json`. The CLI verifies an existing manifest/body and
prints static SWF/FlashVars/navigation evidence while preserving logical URLs,
archive snapshots and original response records. It performs no fetch or script
execution. The local replay covers 86 pages with 81 default matches and five
explicitly recorded encoding overrides. Live HTTP/cache support is described below; bounded index traversal remains future work.


The [historical cache and request-policy core](../../../docs/research/danoni-rust-network.md)
adds `network::CacheReplay`, bound robots-chain evaluation, DNS answer validation
and per-origin pacing. `replay-cache JOB.json` inspects one response/refusal/miss
without a request or output-body copy. The existing Node oracle agrees on 1,629
cache requests and 1,629 robots outcomes; all 941 bound files are read once.
The [native HTTP coordinator](../../../docs/research/danoni-rust-http.md) now adds
`network::Fetcher`, cancellation, bounded async transport and durable single-writer
cache publication. `fetch-url JOB.json` acquires one reviewed anonymous URL and
its explicit redirects, or replays it offline. DNS can be scoped to
`100.100.100.100` without system changes. The URL-native bridge below connects
those responses to native dependency recovery; prepared-song integration follows.


The [URL-native bridge](../../../docs/research/danoni-rust-url-native.md) adds
`url_native::UrlRequest::new(url)` and blocking-job `execute`, plus
`extract-url-native BUNDLE JOB.json NEW_OUTPUT`. It acquires a direct SWF or
statically selected page embed, preserves HTTP/source context, then runs automatic
profiles with policy-aware dependency recovery. Archive redirects cannot silently
fall back live. A successful result contains normalized source proofs; prepared
song export and library installation still need later integration.

Portable native rendering and the optional trusted `renderer` policy are described
in [the rendering handover](../../../docs/research/danoni-rust-rendering.md).
Offscreen frame bytes use the same API across backend choices; software-only
selection requires an installed CPU adapter. The subsequent
[Linux handover](../../../docs/research/danoni-rust-linux.md) covers ELF packages,
confinement and native-scale software rendering. Windows remains unsupported.
