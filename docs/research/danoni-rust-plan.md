# Rust migration of the DanOni export pipeline

Agreed 2026-10-13. The target is an embeddable native Rust pipeline that accepts
one public URL and prepares playable songs for Dito Dito Infinito's local library.
A CLI calls the same library API. Desktop integration comes later; the web app
will not include the native pipeline initially. This supersedes the next-corpus
batch as the immediate tooling priority, not the remaining acquisition research.

## Baseline and boundaries

The accepted local generation is 93 songs / 144 charts from 89 required entries:
96,419 taps, 4,520 holds and all 93 banners. Its source evidence, normalized
references, reviews and exports remain outside Git under `~/Music/DanOni/`.
The [progress review](danoni-progress.md) and [runtime verification](danoni-runtime.md)
record the coverage and limits. Existing Python/Node commands remain usable and
serve as migration oracles until each replacement passes its acceptance gate.

New code initially lives in `tools/danoni-dump/rust/`, package
`ddi-danoni-export`, MIT under the existing REUSE annotation. It is a standalone
Cargo workspace, like `pack-verify`, and does not change the game's workspace or
WASM dependency tree. Only research documents and tooling change on this branch.
Adding desktop dependencies, storage adapters or changing `docs/PLAN.md` is a
handover to the implementation session. The eventual crate location can change
without changing the API's responsibilities.

The goal is Rust-owned application code and an installable desktop pipeline.
It does not require rewriting Ruffle, codecs or a browser engine in Rust.
Users should need no Python/Node installation, compiler, system FFmpeg command
or manually prepared per-song recipe. Ship reviewed workers/codec dependencies
with the desktop distribution and their licence notices. During migration,
external tools may still be used by the reference/verification commands; do not
claim the new pipeline is standalone before all runtime dependencies are addressed.

Supported built-in modes are 5, 7, 7i, 9A and 9B, with 5-key first. Preserve
unsupported charts and custom keys as evidence and report exclusions. Do not
infer a built-in mode from lane count alone. Preserve original note frames,
credits, difficulty notation and original files separately from derived SSC
metadata. Perfect reproduction of author engine customizations is not a goal.

## Responsibilities and eventual API

```text
URL
  → bounded discovery
  → policy-aware acquisition and content cache
  → native family extraction or HTML5 capture
  → normalized work and exact references
  → song preparation and validation
  → prepared import
  → host's local-library transaction
```

The pure core owns typed normalized charts, validated policy decisions,
quantization, preview/tempo decisions from supplied audio and prepared artifacts.
It performs no network access, work-code execution or library writes. Native
orchestration owns HTTP, disk cache, staged artifacts, platform process launch,
resource limits and cancellation. A small CLI translates arguments/events and
calls those APIs; it must not hold extraction logic unavailable to the app.

The eventual native API is conceptually `prepare_url(url, services, options)`:
`services` supplies a persistent cache, staging store, allowed fetcher, restricted
worker/browser providers, progress sink and cancellation token. The app supplies
these services and reasonable defaults, so the only user input normally needed
is a URL. This is a proposed interface, not an implemented function or a promise
that every URL succeeds. Use a resumable job with typed per-work outcomes rather
than a process-global configuration or a UI-blocking synchronous call.

A prepared import contains song identities, chart/media/banner artifacts,
exact reference and provenance records, selected/excluded selectors, policy and
implementation versions, content hashes and validation results. Internal artifact
paths are relative and validated. A cache blob's hash identifies bytes; a work's
identity also includes logical URL, semantic query/FlashVars and selector. The
same engine SWF can represent different songs.

The host consumes prepared files directly through the existing song-library
import path; ZIP generation is optional. Only the host commits the local-library
update after validation. Cancellation/failure leaves existing songs intact.
Collection results retain individual successes and unavailable/unsupported works;
partial success must not be presented as complete. A product-level choice of
which completed songs to include is separate from preparation.

Progress events should name discovery, acquisition, extraction, audio,
preparation, validation and completion, with per-work identity and bounded counts.
Warnings and errors need stable codes, source URLs and actionable scope. Unknown
engine, unavailable resource, unsupported mode, prohibited acquisition, worker
halt and insufficient tempo evidence are distinct outcomes. Sparse tempo evidence
can retain exact source timing; failed extraction cannot become a guessed chart.

## Milestones and acceptance gates

### 1. Contracts and a shared Rust entry point — complete

Define typed supported modes, normalized lanes in audio-relative seconds,
policy inputs, immutable source/evidence fields, prepared artifacts and errors.
Keep the source schemas in [the preparation contract](danoni-postprocess.md)
compatible; new request/result formats have their own version identifiers.
Introduce an I/O-free library and a thin CLI. Do not create a dummy URL command
that merely invokes Python or claims URL support before it exists.

The first implementation includes ratings/tier classification, bounded musical
quantization, exact 60 Hz timing, negative pre-roll, holds, authored scroll
compensation, SSC chart blocks and per-endpoint diagnostics. A reviewed numeric
rating can override source metadata explicitly without parsing difficulty names.
The core accepts already validated review decisions; source hash/provenance
validation belongs to the forthcoming offline orchestration boundary.

Acceptance: original synthetic tests plus a local-only differential comparison
against every accepted chart through the game's real SSC parser. Compare layout,
lanes, holds, ticks/colour subdivisions, times, meter/tier/label, scroll segments
and x1/x2/x3 behavior. Preserve exact references and report any intentional
validation differences. Rust's round-to-even behavior must match Python.
Numerical text spelling and JSON key order may differ; semantic equality and
original media byte identity matter more than ZIP byte identity.

### 2. Complete offline preparation — complete

Port normalized-work loading, metadata merge and shared-audio grouping; bind
asset/review hashes and constrain paths. Port source-bound published rating
reviews, authored/display BPM metadata, preview selection from supplied PCM,
artwork crop/title-card generation, song headers and asset placement. Carry
unsupported effects and source credits unchanged into reference artifacts.
Port staged rebuild inventories, optional ZIP assembly, validation, atomic
promotion, archival and interrupted-swap recovery. Add a host-consumable prepared
song/artifact contract rather than exposing a CLI output directory as the API.

Acceptance: reconstruct the 93-song generation with supplied reviewed grids and
PCM/audio assets. Compare all 144 charts, preview starts, titles/credits,
ratings, banners, deduplication and omissions. Preserve source audio; test fresh
staging, cancellation, interrupted publication and path containment. This is
still offline preparation, not fresh URL acquisition or automatic audio fitting.

### 3. Audio processing and musical analysis

Replace Python scientific imports and subprocess-only FFmpeg/audio helpers with
packaged codec/DSP implementations accessible from Rust. Select latest stable
libraries when adding them; audit licences and distribution needs before fixing
the desktop dependency set. Do not assume a Rust wrapper makes a native codec
library or external executable disappear.

Port decoding/resampling with explicit sample-frame units, MP3 stream selection,
decoder-origin correlation, PCM preservation and Ogg encoding. Port onset/pulse
analysis, chart lattice fitting, aliases, robust phase fitting, shared-audio
consensus and cache decisions. Version algorithm/config/dependency identities
explicitly rather than hashing Python source filenames. Preserve old cache
provenance; reuse it only through an explicit compatible format.

Acceptance: known synthetic tempo/phase/audio-origin cases and all local tracks.
Codec changes require independently measured sample alignment, rate/channel/frame
counts and declared tolerances. BPM/phase results need comparison with the existing
analysis, including ambiguous/sparse/variable-tempo songs. Keep the existing 9 ms
note/hold budget; matching a guessed BPM never permits moving notes further.
Preserve native base scroll independently of musical colouring and user speed.
A numerical port may need an explicitly reviewed algorithm version rather than
pretending a new beat tracker produces identical librosa results.

### 4. Native extraction and supervision

Move family projections, stored-property graph decoding, SWF/audio inventory,
recipes, source/prepared bank checks, timebase and audio-origin proofs into Rust.
Make capture/selection/recipe generation reproducible using tracked commands and
supported-family recognition, not local study scripts. Keep selectors and all
ordinary runtime-built resource requests visible; no invented missing responses.
Package a reproducibly built pinned Ruffle worker and the stored-state hooks.

The supervisor is embeddable; Ruffle executes acquired movies in a restricted
child process with bounded input, CPU, wall time, memory and outputs, not inside
the application's trusted process. Retain the tested macOS restrictions first.
Linux and Windows require separate tested confinement/process termination
implementations before acquired movies run there. No unrestricted fallback.
Use versioned worker request/result messages and preserve successful-run receipts.

Acceptance: reproduce accepted native charts/audio/motion/artwork from original
cached sources using only tracked Rust tooling and packaged components. Exercise
halts, truncated captures, missing dependencies, wrong selectors, clock mismatch,
worker crashes, resource limits and cancellation. Existing recipes remain importable
for research but must not be required user input in supported URL flows.

### 5. Acquisition and URL discovery

Port allowed public fetching, robots/terms handling, per-host pacing, redirect/DNS
checks, request-context-sensitive cache identity, byte/decompression limits,
provenance and single-writer cache behavior. Reuse existing cache records through a
versioned reader, never quietly drop protections or redownload the whole corpus.
Acquisition runs in the supervisor; the SWF worker has no live network access.
Iterate bounded dependency requests through the fetcher and exact cached bindings.

Support individual work URLs and explicit SWF URLs, then reviewed index/event/list
routes. Parse anchors, frames, preserved embeds/noscript, FlashVars and known
router data first. Keep genuine navigation/referrer context where necessary.
Support explicit Wayback inputs with original logical URL, requested/final replay
URL and capture time per resource, including mixed snapshots and live fallback.
No login/paywall/protection bypass or unbounded site/archive crawl.

Acceptance: cached/offline replay plus controlled original synthetic HTTP cases,
then bounded real URL runs selected from the existing survey. A supported URL
must reach validated prepared songs without local study scripts or operator
per-song recipes. An index exposes partial outcomes and does not recursively
follow arbitrary links.

### 6. HTML5 controller and desktop integration

Replace Node/Playwright runtime orchestration with Rust-owned browser control;
choose a supported browser/WebView/CDP approach based on a measured prototype.
Work JavaScript still executes only within a sandboxed compatible browser runtime.
A general JavaScript interpreter does not supply DOM, media or engine semantics.
Keep exact engine-hash review gates and browser-observed reference capture.
Browser-generated capture expressions are allowed; they are not a requirement
for Node at runtime. Static/simple works can use verified native import paths,
with their confidence/scope reported separately from an engine reference.

Port screenshot/title rendering needed in the production pipeline; browser game
verification can remain a development-only oracle until its own Rust replacement.
Offline native SWF preparation must not depend on Chrome. Prove HTML5 runtime
packaging and browser availability before claiming a no-install desktop experience.

Handover a conditional native dependency and host storage/progress/cancellation
adapter to the desktop session. Initially exclude these dependencies from WASM.
Acceptance: URL → prepared import → local library, play through the existing game
path, rollback/cancellation and partial collection outcomes, on each declared
supported desktop platform. The absence of a desktop shell today is not a reason
to couple the pipeline to the current web app.

## Initial chart slice

`tools/danoni-dump/rust/` implements milestone 1's chart preparation and starts
milestone 2. It has no HTTP client, audio codec, Ruffle or browser dependency and
no library storage writes. Its CLI accepts one `ddi-danoni-compile-request/1`
JSON document and returns `ddi-danoni-prepared-chart/1`, including an SSC chart
block and endpoint/scroll diagnostics. These are internal migration APIs, not
the complete prepared-import schema.

See [the Rust tool README](../../tools/danoni-dump/rust/README.md) for exact request
fields, commands, validation differences and the local differential gate. The subsequent complete-song slice below extends this API before fresh
acquisitions.


### First-slice acceptance, 2026-10-13

The Rust chart projection passes the read-only game-parser differential gate on
all **89 accepted work entries / 93 songs / 144 charts**: **96,419 taps / 4,520
holds / 105,459 endpoints**. Lane/kind/tick/hold pairs, musical tick colours,
meters/tiers/labels, timing maps and scroll maps agree with the Python-produced
accepted SSCs. Every recorded quantization endpoint/method matches; maximum
source-to-export note shift remains **8.9983948666 ms**. x1/x2/x3 positions use
the game's real scroll code. Source arguments stay unchanged.

The gate supplies the accepted grids, native motion and reviewed ratings; it
therefore proves chart preparation parity, **not** Rust tempo estimation, review
validation, audio preparation or URL extraction. Media/preview/banner generation
and complete song rebuilding still use the existing pipeline. No new corpus or
export generation was produced.

Twelve synthetic library/CLI tests plus the all-chart local test pass, as do
Clippy with warnings denied, formatting and `just check` including REUSE. Local
receipts are under `~/Music/DanOni/reviews/rust-migration-20261013/`, binding the
Rust source/lockfile, accepted generation and reference reports to the comparison.
Dependencies added here are serde 1.0.229 and serde_json 1.0.154, latest stable
when checked on 2026-10-13 using [serde's registry metadata](https://crates.io/api/v1/crates/serde)
and [serde_json's registry metadata](https://crates.io/api/v1/crates/serde_json).


### Complete-song slice, 2026-10-13

The [offline handover](danoni-rust-offline.md) documents `prepare_work`, its
host-supplied asset/progress boundary, optional filesystem feature and explicit
job/media contracts. Source/review validation, grouping, song headers, source
ratings, pickup previews from PCM, byte-identical supplied media/artwork and
provenance now run in Rust. The accepted-codec bridge reconstructs all 93 songs /
144 charts into a new local generation without Python/Node/FFmpeg/browser/Ruffle
at runtime. Existing accepted exports are unchanged.

The source-bound full-song differential gate matches preview/rating diagnostics,
metadata, banners, audio bytes and game-parser/scroll behavior. This still supplies
accepted musical grids and prepared media: it does not prove new Rust decoding,
encoding, audio-origin correlation or BPM estimation. At that checkpoint
milestone 2 remained open
for new artwork generation, ZIP/collection assembly and generation promotion/
recovery; the subsequent slice below completes those pieces.


### Artwork/collection/publication slice, 2026-10-13

Milestone 2 now includes browser-free title typography and captured panel crops,
deterministic collection ZIPs, self-contained source reconstruction/game-parser
validation, exact inventories and journaled archival/publication/recovery.
The [offline handover](danoni-rust-offline.md#native-artwork) specifies the APIs,
new schemas, output trees and tested interruption scope. The isolated local trial
covers all 93 songs / 144 charts; accepted exports are unchanged. Five Kouhaku
lossless crops match their previous pixels exactly. Song media/artwork remain
byte-identical supplied assets; new pack art/rasterization has its own version.

Next is milestone 3: first native decoding/PCM units and audio-origin checks,
then Ogg encoding and reproducible robust musical analysis. Keep the offline
prepared-media receipt as the boundary, and retain the current preview/9 ms note
and authored-scroll gates. Neither the completed offline stages nor a standalone
Rust CLI imply URL-to-library support before later milestones pass.


### Native codecs and origin math, 2026-10-13

The [audio handover](danoni-rust-audio.md) documents Rust decoding, canonical PCM,
latency-compensated resampling, multi-window decoder-origin correlation, optional
in-process bundled Vorbis encoding and neutral media jobs/receipts. All 93
normalized tracks pass the PCM timeline gate; 79 raw MP3 streams independently
match captured Ruffle origins with exactly the prior correction. Fresh Rust codec
receipts feed complete-song preparation for all 93 tracks without changing chart
diagnostics, artwork or preview starts. Accepted exports remain unchanged.

The next slice below implements onset/pulse and robust musical/shared-audio
fitting/cache policies; musical-quality acceptance remains open. The raw-origin routines are available to the upcoming
extraction supervisor; SWF demux/runtime/provenance orchestration is still
milestone 4. Codec-side native C libraries are bundled build dependencies of the
optional encoder, not external runtime executables and not part of the game WASM.


### Native tempo and explicit analysis bridge, 2026-10-13

The [tempo handover](danoni-rust-tempo.md) specifies the Rust lattice/pulse,
independent-chart consensus, source-bound cache and CLI contracts. The explicit
legacy bridge matches 126 accepted chart decisions; 1,044 fixed fit statistics
agree. Fresh versioned spectral analysis processes all 89 works / 93 tracks /
144 charts and checks every applied endpoint/event, with maximum error below
9 ms. No pack-specific constants or work-name branches are used.

Milestone 3 remains open for native musical-quality review: 29 charts choose a
different BPM family from accepted grids, including octave/triplet aliases, and
quarter phase is provisional. New local estimates have not replaced the accepted
exports. The next analysis step is a generic alias/phase assessment using these
recorded differences, before claiming native musical equivalence. Source/worker
extraction and URL acquisition remain milestones 4–5.


### Generic native alias/phase refinement, 2026-10-13

Native onset v3 retains the complete beat path, policy v2 accounts for coarse
pulse resolution, and quarter-onset support v1 explicitly marks observed,
ambiguous and insufficient phase evidence. Six new synthetic cases accompany
the corpus trial. All 126 compatibility decisions remain unchanged. Complete
song preparation still passes every endpoint and original x1/x2/x3 scroll check.

The [tempo handover](danoni-rust-tempo.md#alias-and-phase-refinement-2026-10-13)
records 117 accepted-family matches (up from 115), 26 remaining family differences
and one exact-source fallback. Seventeen estimated charts have observed quarter
support; 126 retain unresolved/provisional phase. Accepted exports are unchanged.
Milestone 3's mathematical/codec implementation is present; its native musical
quality remains provisional. Do not use accepted-grid agreement as sole truth
or let this review imply the missing extraction/URL stages are implemented.


### Passive native graph/family slice, 2026-10-13

The [extraction handover](danoni-rust-extraction.md) documents stored UTF-16/object
reading, all seven structural projections, motion/audio-start/cross-run clocks,
source-bound saved-state jobs and the thin `project-native` CLI. 115 charts project;
114 also match movement/origin and normalized fields, with exact agreement to
current Python outputs. Two strengthened-contract failures also occur in current
Python: RE:RAVE Hard's missing speed bank and Destined Marionette's motion segment.
Accepted packs are unchanged; neither is silently repaired or omitted.

Milestone 4 is in progress for SWF inventory/demux, combined normalized artifacts,
pinned Higan/Kouhaku contracts and restricted worker supervision. The passive
slice proves no fresh work execution, decoded SWF audio or URL acquisition.


### Passive SWF audio slice, 2026-10-13

The [SWF handover](danoni-rust-extraction.md#swf-audio-inventory-and-demux-2026-10-13)
documents bounded FWS/CWS traversal, stream inventory, runtime slice selection,
whole-frame MP3 output and source-bound `demux-native` jobs. All 85 existing
selected streams match bytes and every timing field, including five Kouhaku
songs and Higan. Sixty-three structural audio jobs pass the complete audit;
16 audio captures need their separately proven decoration contracts joined.
No diagnostics are erased and accepted packs remain unchanged.

Milestone 4 now has passive graph/family and SWF demux implementations. Next
combine those bound inputs with decoder-origin/media production into normalized
artifacts, including cross-run decoration proof, then port pinned Higan/Kouhaku
chart contracts and restricted worker supervision. The two recorded chart/motion
proof gaps remain open. URL acquisition is still milestone 5.


### Combined saved-run normalization, 2026-10-13

The [normalization handover](danoni-rust-normalization.md) specifies the embeddable
`prepare_native_work` API, `normalize-native` jobs, portable input/evidence layout,
canonical WAV and native-scroll review. All 16 previously audio-only decoration
rejections now join their separate converter/receptor proofs with exact error
multiplicity and matching selector/resource/worker identities. Every requested
chart independently matches its bound native PCM, even with shared MP3 data.

The local trial covers 78 works / 113 charts, 75,262 real-parser endpoints and
2,679 x1/x2/x3 source-speed checks through an in-memory downstream pipeline.
Canonical PCM differs by at most one unit, with unchanged origin corrections.
The two existing chart/motion gaps remain; M. MOON's second chart adds an explicit
control-plan gap because its motion run omits a selector click. Selected subsets
are recorded; accepted exports remain unchanged. The source-grid smoke test does
not claim musical 75 BPM or close milestone 3's native quality review.

Milestone 4 now produces normalized structural artifacts from saved runs. Next
port the separately pinned Higan/Kouhaku contracts and recapture/prove those three
selectors, then implement restricted worker supervision/resource acquisition.
URL discovery/fetching remains milestone 5; desktop library installation remains
a verified host transaction.


### Pinned Higan and Kouhaku normalization, 2026-10-13

The [pinned contract handover](danoni-rust-pinned.md) adds exact-movie Higan and
Kouhaku selections to the existing Rust projection/normalization APIs. It retains
complete original references, actual filled-star counts, child-timeline clocks,
nominal judgment frames and measured source-width scroll. It does not relax the
structural arrival proof or add work-specific DSP parameters.

Fresh restricted development captures supply enough PCM for seven independent
origin proofs; the current Python launcher remains the development supervisor.
Rust then produces two normalized works / six songs / seven charts, with full
current reference and frozen note/effect parity. All 3,073 game-parser endpoints
and 57 x1/x2/x3 scroll checks pass; independent PCM differs by at most one unit,
with zero measured shifts. The 78-work/113-chart structural gate still passes.
Accepted exports and provisional native musical-quality status are unchanged.

Milestone 4 now includes all existing pinned and structural passive adapters.
Next package and port the restricted worker supervisor/versioned capture protocol,
then the bounded resource-request loop. The three structural selector proof gaps
remain explicit. Recipe-free URL discovery/acquisition remains milestone 5.


### Rust restricted native supervision, 2026-10-13

The [supervisor handover](danoni-rust-supervisor.md) specifies the host-installed
worker boundary, strict source-bound capture requests, staged immutable inputs,
fixed macOS policy, native memory monitoring, kernel budgets, cancellation,
child reaping and ready/failure receipts. `capture-native` replaces the Python
runtime launcher; the separately pinned worker/build hooks remain unchanged.
Unsupported platforms never execute a worker.

All seven pinned charts now capture under Rust supervision and feed Rust
normalization/media/SSC preparation: 3,073 real-parser endpoints and 57 authored
x1/x2/x3 checks pass with unchanged source references and zero decoder shifts.
Original renderer/PCM/key fixtures pass; truncated graphs/audits and halted
workers fail without readiness. Independent original OS probes cover outside
file, input-write, symlink, live-network and child-spawn denial, plus resource
limits, cancellation/crash handling and reaping. Accepted exports are unchanged.

Milestone 4 remains open for Rust worker build/package preparation, reproducible
selector/request orchestration and the three structural proof gaps. Runtime
supervision itself no longer depends on Python. Dependency iteration/public URL
acquisition and other-platform confinement remain subsequent work.


### Rust worker preparation and bundles, 2026-10-13

The [worker handover](danoni-rust-worker.md) replaces `prepare.py` with pinned
Git-object source preparation in a fresh owned tree. It retains the exact existing
hooks/lockfile/protocol identity, adds complete source/bundle inventories and
returns trusted `WorkerSpec` values for the Rust supervisor. Git/Cargo/Java are
explicit build-time tools; an installed desktop host needs none for native capture.

A fresh rebuild preserves parsed state, PCM and synthetic pixels/key behavior.
Seven pinned charts pass the downstream reference/origin/SSC/scroll gate again:
3,073 endpoints, 57 x1/x2/x3 checks and unchanged decoder shifts. Tests cover
source isolation, changed/incomplete inventories, policy/source substitutions,
symlinks, cancellation and no overwrite. Accepted exports remain unchanged.
These local bundles preserve upstream/DDI notices; shipping requires an actual
worker dependency/embedded-asset notice audit and installation authority.

Milestone 4 now includes Rust worker source preparation, local packaging and
installed-bundle verification. Next implement reproducible selector/capture
orchestration and recapture the three explicit structural proof gaps. Bounded
resource iteration, URL acquisition and other-platform confinement remain ahead;
provisional native musical-quality review still keeps milestone 3 open.


### Rebased Rust selector/capture orchestration, 2026-10-14

Rebased cleanly onto local `main` `18791f5`, retaining the dated backup branch.
The [orchestration contract](danoni-rust-orchestration.md) adds one bound selector
config per chart, derived role observations, exact capture deduplication, staged
input/provenance inventories, portable replay and capture-to-normalization APIs.
`extract-native BUNDLE JOB NEW_OUTPUT` uses only the installed restricted worker
and Rust code. Parent readiness follows all chart proofs and a final cancellation
checkpoint; child transport success never authorizes an install.

M. MOON's omitted-click gap is closed by recapturing every role with the same
frame-330 control plan. Destined Marionette's motion proof is closed using its
existing stable samples strictly before the frame-460 author speed change.
The old requests still fail unchanged gates. RE:RAVE Hard remains an explicit
source/native speed-bank proof gap.

Fresh execution and frozen reference/PCM/downstream comparison cover four works,
nine charts and eight songs: 4,127 endpoints and 141 x1/x2/x3 checks, zero decoder
origin corrections and at most one PCM unit of difference. M. MOON replays from
only its staged dump with equal state/PCM and unchanged normalized chart/data
apart from the new capture timestamp. Accepted exports remain unchanged.

Milestone 4 now includes explicit offline selector/capture orchestration. It is
not yet automatic selector discovery or recipe-free URL import. Next prove the
remaining speed-bank behavior and implement bounded request/dependency iteration,
then public URL discovery/acquisition. Native musical-quality review, HTML5,
desktop library installation and distribution notices remain open.


### RE:RAVE source-bank closure, 2026-10-14

The [exact reviewed selector contract](danoni-rust-extraction.md#rerave-selector-controlled-speed-proof-2026-10-14)
proves Hard's shared speed data is inactive; the ordinary missing-bank gate
remains strict for unreviewed movies. The native reference preserves the
nonexecuted values, and fresh installed-worker execution passes complete frozen
chart/audio/game-parser comparison. The fresh combined gate now covers five
works / ten charts / nine songs, 5,049 endpoints and 144 scroll checks, with
zero decoder-origin corrections and at most one PCM unit difference. All 116
structural-family charts project; 115 pass the preserved historical motion,
and Destined Marionette's recaptured stable segment closes the older motion
failure. The three known proof gaps are closed without changing accepted packs.

Milestone 4's next slice is a bounded dependency/request loop above immutable
captures. Discover exact missing GET requests from preserved audit data,
return typed requests to the host, append individually policy-approved cached
responses and recapture in fresh owned directories. Limit retries, new URLs,
bytes and total work across the whole operation; detect repeated requests and
no progress. Preserve logical URL/query/FlashVars/charset identity and original
failed captures, check cancellation between stages, and never mark the parent
ready until the unchanged projection/motion/PCM proofs pass. This should expose
an embeddable sans-network coordinator first, with host HTTP/cache acquisition
in milestone 5. Automatic selector/profile discovery remains separate from
resource closure; a successful dependency loop must not guess chart selection.


The current worker records only the raw URL in `fetch-denied`. Its fetch path
uses that record for both absent cached GETs and denied methods/bodies, so it
cannot safely authorize dependency acquisition. The next slice must first add
trusted structured request evidence (method, body presence, original and
resolved logical URL, iteration and rejection reason), review/package the new
hook identity, then expose only exact body-free GET cache misses to the host.
Navigation/socket/POST denials and unparseable URLs remain extraction failures;
trace text and arbitrary runtime-error messages are never fetch requests. The
coordinator still needs host robots/terms/rate-limit approval and cached response
provenance; an audit record is evidence of a request, not fetching authorization.


### Bounded dependency coordination, 2026-10-14

The [Rust dependency coordinator](danoni-rust-dependencies.md) now consumes
structured method/body/header/URL evidence from the revised installed worker,
groups exact missing GETs, asks a host policy/cache resolver for immutable
responses and retries explicit selector plans in fresh owned directories.
Original and resolved logical URLs remain separate from archive retrieval URLs.
All earlier source/motion/audio proofs still gate readiness. Retry, capture,
URL/body, elapsed-time and whole-operation output limits apply across attempts;
late cancellation blocks the outer marker even after a child normalizes.

Higan recovers omitted DOS/colour inputs; archived RE:RAVE recovers DOS. All
responses come from the existing local corpus, retaining original retrieval
provenance, with no network fetch. Original synthetic tests exercise grouping,
read-once caching, strict denials, response provenance, budgets and cancellation.
The CLI's response map is an offline host adapter; the embeddable callback can
later use the app's policy-aware fetch/cache service. No dependency is added.

Milestone 4 now includes explicit selector execution and bounded dependency
closure. Next implement reviewed selector/profile discovery and request/motion
planning, then milestone 5's host acquisition/cache/discovery services. Do not
claim URL-only importing or replace accepted musical estimates before their
separate quality review. HTML5 capture, desktop library installation, other-OS
confinement and audited worker redistribution remain pending.
