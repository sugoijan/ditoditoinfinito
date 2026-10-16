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
Changing the game workspace, main-shell storage adapters or `docs/PLAN.md` is a
handover to the implementation session. The SDK supplies a separate native managed
store and import-job coordinator without modifying those main-owned components. The eventual crate location can change
without changing the API's responsibilities.

The goal is Rust-owned application code and an installable desktop pipeline.
It does not require rewriting Ruffle, codecs or a browser engine in Rust.
Users should need no Python/Node installation, compiler, system FFmpeg command
or manually prepared per-song recipe. Ship reviewed workers/codec dependencies
with the desktop distribution and their licence notices. During migration,
external tools may still be used by the reference/verification commands; do not
claim the new pipeline is standalone before all runtime dependencies are addressed.

Supported built-in modes are 5, 7, 9A and 9B, with 5-key first. Preserve
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
is a URL. The supported native single-work subset now implements this boundary
as [`import_job::execute`](danoni-rust-import-job.md), connecting URL preparation,
admission and library installation. [`url_prepare::execute`](danoni-rust-url-preparation.md)
and I/O-free `preparation::prepare` remain separately callable beneath it. The
[reviewed collection coordinator](danoni-rust-collections.md) adds bounded static
publisher/event lists with sequential per-work transactions and partial outcomes.
Broader index layouts and browser routes remain planned; success is never promised for arbitrary URLs. The eventual desktop
adapter should use resumable jobs with typed per-work outcomes rather than
process-global configuration or a UI-blocking synchronous call.

A prepared import contains song identities, chart/media/banner artifacts,
exact reference and provenance records, selected/excluded selectors, policy and
implementation versions, content hashes and validation results. Internal artifact
paths are relative and validated. A cache blob's hash identifies bytes; source
identity includes logical URL and semantic query/FlashVars, and song identity
also includes the original encoded audio. Selectors describe chart coverage,
not song identity, so selecting another difficulty does not duplicate a song.
The same engine SWF can represent different songs.

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
URL and capture time per resource, including explicitly recorded mixed snapshots.
Never silently fall back from an archive snapshot to the live site.
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


### Automatic shared-engine menu planning, 2026-10-14

The first [native menu profile](danoni-rust-profiles.md) recognizes shared control
bytecode, observes the work's runtime menu, drives every supported 5/7-key row,
discovers exact audio starts and derives motion candidates from source timing.
The unchanged schedule/movement/PCM normalizer remains the acceptance gate.
Unknown engines and incomplete captures reject; other observed key labels remain
explicit exclusions. It writes a portable explicit replay plan as well as the
normalized work, with whole-operation bounds and no network or installation.

The profile takes source-only immutable context rather than per-song click/frame
recipes. Existing exact optional-colour 404 reviews can travel with that context.
A required missing GET retains structured evidence and stops. Next bridge the
existing dependency resolver into these pre-menu/initialization probes, preserving
fresh retries, source identities and parent readiness. Then extend reviewed
control profiles to other native families and implement milestone 5's host URL,
policy/cache and bounded index-discovery services. Musical estimate acceptance,
HTML5 browser-reference capture and desktop library installation remain separate.


The first profile's local gate admits seven works / eight charts, comparing
5,435 endpoints and 102 authored-scroll/player-multiplier checks against the
frozen corpus and real game parser. One matched work reaches game over on early
holds before a tap witness; its three menu rows remain unadmitted. Generic
hold-motion or derived play-input planning is a separate proof gap, recorded in
the profile handover. Accepted exports remain unchanged.


### Automatic profile dependency recovery, 2026-10-14

The [profile recovery API](danoni-rust-profile-recovery.md) now accepts source-only
inputs, resolves observed required GETs through the host callback and restarts
menu discovery in fresh owned attempts. Explicit plans and automatic profiles
share response verification, retry/body/URL/time/output budgets and final parent
readiness. Profiles count captures at runtime boundaries across retries. Exact
already-reviewed optional-colour 404s are classified without being refetched or
weakening the unchanged full normalization audit.

Cached March and No Name trials cover missing DOS, DOS plus colour resources,
7-key holds and two 5-key menu charts. All source acquisition provenance and failed
probes remain local. This closes the pre-menu resource bridge for the first
profile; no built-in HTTP service or library installation is claimed. Next prove
generic hold-motion or derived play-input witnesses for the existing early-hold
game-over gap, then expand reviewed native profiles and milestone 5 acquisition.


### Hold-head motion and derived intro inputs, 2026-10-14

The [hold-head extension](danoni-rust-hold-motion.md) uses explicitly paired native
hold clips with the unchanged geometry, speed, arrival and clock proofs.
Hold-only constant charts retain full source/spawn/duration agreement. For early
holds whose otherwise complete normalizer reports insufficient audio, bounded
physical key inputs come from source frames and captured key bindings. Fresh
projection/motion/audio roles share those inputs; the complete chart and audio
origin must remain identical and final PCM correlation still gates readiness.

Two requested Maspa selectors now normalize without the old manual input recipe.
The third rewrites four source heads and remains excluded through the new optional
selector filter; a Ready subset never implies full-work coverage. All failed
probes and source-bound comparisons remain local, and accepted exports are
unchanged. Next extend reviewed engine profiles and implement milestone 5's host
URL/policy/cache and bounded index discovery. Source transformations need their
own evidenced contracts; musical estimate acceptance, HTML5 capture, other-OS
confinement and desktop library installation remain separate work.


### Early shared-engine menu profile, 2026-10-14

The [early-menu profile](danoni-rust-early-profiles.md) recognizes six exact
shared gen1 control/placement roles, observes its distinct options-menu frame,
derives five-key collaborator selectors and supplies the offscreen renderer.
Initialization/audio discovery is bounded to 600 iterations to fit the unchanged
rendered-worker CPU budget; later/insufficient starts fail explicitly. All
source/schedule, motion, clock, role-control and PCM checks remain mandatory.

Targeted identity inventory scans every bounded SWF tag for AVM2 and retains
required role duplicates, without spending its emitted-record allowance on
unrelated artwork placements. The original full-inventory API/limits remain
unchanged; receipts declare the narrower fingerprint scope. No acquired code or
third-party assets enter the repository, and accepted exports remain unchanged.
Fresh automatic runs cover all 17 matching works / 23 charts, compared with
frozen source/audio references and 14,818 real-parser endpoints. Both four-row
collaborations are discovered at runtime; operation-budget retries retain their
original failures. No note timing or fit parameter is selected by song identity.
Next implement milestone 5's host URL/policy/cache and bounded discovery services
for this proven profile subset; add other engine variants incrementally.


### Passive URL discovery and cache evidence, 2026-10-14

The first [milestone 5 slice](danoni-rust-acquisition.md) adds Rust URL/archive
identities, bounded passive HTML discovery and a versioned cached-page reader.
Object/embed/noscript, FlashVars, anchors, frames and document base URLs are data;
no work script runs. Original queries, final redirects, request context and
retrieval/policy records remain source-bound. Older archived bodies are explicitly
weaker evidence, with no invented HTTP status, completeness or robots permission.

The local gate covers 86 pages: 81 match their independently recorded work SWF
by default, and five require explicit evidenced Shift_JIS overrides. Its 91 cases
retain the original encoding failures as well as successful overrides. Both
Wayback bodies preserve their own page capture and separate logical URL. The
replay performs no fetch and accepted exports remain unchanged.

This is discovery evidence, not Ready extraction or live cache reuse. Next port
the actual host fetch/cache policy service (robots/terms, request-context/Vary,
redirect/DNS checks, pacing, bounded bodies/decompression, persistence and
cancellation), connect observed dependencies and introduce bounded reviewed index
routes with partial outcomes. Native context construction must account for embed
parameters before execution; arbitrary page scripts still need the optional
browser fallback. No dependency is added in this slice.


### Historical cache replay and request-policy core, 2026-10-14

The [Rust network core](danoni-rust-network.md) now reads the old immutable cache
index, preserves recorded refusals and exact reviewed aliases, verifies each body
once and returns individual redirect responses. Context fingerprints and Vary
rules prevent incompatible reuse without making it a fresh fetch. Bound robots
responses, including complete redirect chains, use the latest cached robotstxt
release; source receipts retain original policy URLs, actual retrieval URLs and
each response. DNS answer pin validation and per-origin monotonic pacing are pure
host contracts; cancellation before a request starts consumes no pacing slot.

The independent existing-fetcher gate covers 1,629 requests and all 1,629 robots
outcomes from both caches: responses, refusals, unsupported variants, redirects,
allow/deny and unavailable policies agree. It reads 941 bound files exactly once
and performs no request. Synthetic negative checks cover policy and integrity
failures; no third-party data is committed and accepted exports remain unchanged.

This is a reusable offline cache/policy component, not a live URL importer.
Next implement the actual anonymous transport, terms/freshness/deadlines,
bounded decoding and single-writer cache persistence, join observed native
requests, and add reviewed index routes. Shell crates.io DNS was unavailable in this slice; the subsequent transport
slice resolved dependencies through operation-scoped `100.100.100.100`.


### Live HTTP and durable cache coordination, 2026-10-14

The [HTTP slice](danoni-rust-http.md) adds an embeddable async `network::Fetcher`
and `fetch-url` CLI, explicit resolver selection, pinned public DNS answers,
verified TLS, bounded decoding, cancellation, deadlines and per-origin pacing.
New requests need host-reviewed site access and fresh bound robots evidence.
A durable attempt record prevents duplicate GETs after cancellation/crash; immutable
response capsules keep provenance stable as the mutable cache index advances.
Historical variants, refusals and corrupt records never trigger automatic refetch.
Persistent writing is Unix-only pending equivalent platform durability tests.

Next join acquired responses to static work discovery and the native dependency
loop, then prove single-work URL preparation against frozen corpus references.
Index traversal, browser control and desktop installation remain later. This
change does not regenerate or promote accepted exports or close native musical
quality review.

Validation: 202 native and 85 pure SDK tests, both strict Clippy configurations,
format checks and `UV_OFFLINE=1 just check` pass (828/828 REUSE). The existing
1,629 cache/robots and 91 discovery-case gates pass. Four CLI success trials prove
real HTTPS acquisition, identical zero-request offline replay, and historical
anonymous/browser-context replay. The host-selected explicit CA bundle keeps
TLS verification enabled where this agent's platform-verifier IPC fails. Original
cache indexes, registry and accepted exports retain their previous hashes.


### URL-native discovery and dependency bridge, 2026-10-14

The [URL-native bridge](danoni-rust-url-native.md) connects host HTTP/cache services
to static work-page discovery and automatic native dependency recovery. Its public
job-thread API starts with `UrlRequest::new(url)`; the CLI is `extract-url-native`.
It stages self-contained immutable acquisition evidence, preserves logical versus
archive retrieval identities and FlashVars, and checks archive-to-live redirects
before cached or live hops. Host cancellation is polled while awaiting HTTP and
drained into durable attempt records. The worker remains offline and confined.

A generic declared-viewport hint handles a broad game beside equal-height narrow
Flash panels. Unknown sizes, ties, multiple broad games and conflicting FlashVars
still refuse automatic selection; known engine/runtime proofs remain mandatory.
No per-song filenames, selector clicks or interpreter patches are introduced.

Higan and No Name now run from page URLs and local cache evidence without recipes
or new downloads, yielding three observed 5-key charts. No Name passes its frozen
parser/PCM/scroll gate; Higan's notes/holds/scroll fields and WAV match its earlier
extraction. Higan's colour triples remain explicitly unmapped raw presentation in
the automatic family, a gap retained in the handover. A real cancellation after
child success proves the outer URL job cannot publish readiness.

Next join normalized output to the existing audio/tempo/artwork/song APIs for
URL → prepared songs. Reviewed optional-colour handling, generic colour mapping,
index/frame routes, broader native families, HTML5 control and desktop installation
remain separate work. Accepted exports and registry remain unchanged.

Validation passes 211 native and 85 pure SDK tests, both strict Clippy variants,
format checks and `just check` (833/833 REUSE), including existing cache/page gates.
Final targeted bridge tests and real-worker cancellation pass after wrapper
publication changes. A direct SWF trial reproduces the corresponding page-route
chart exactly. The complete parent manifest is staged before its final readiness
checkpoint and only renamed on success; cancellation removes the pending marker.
All real-work trials use zero GETs, original caches/exports are unchanged, and
validation receipts remain local.


### URL-to-prepared-song slice, 2026-10-15

The [prepared-song handover](danoni-rust-url-preparation.md) documents the new
`preparation::prepare` pure coordinator and `url_prepare::execute` job-thread
adapter/`prepare-url-native` CLI. The supported native URL path now reaches fresh
Ogg/WAV media, full native BPM fitting, pickup previews, source-star classification,
measured native scroll correction, optional explicit-font title banners and
self-contained SSC song folders. No Python/Node/FFmpeg/browser or per-song grid is
required at runtime for these supported native profiles.

Full source-bound motion coverage is mandatory. BPM fits remain provisional;
insufficient evidence preserves exact source-clock timing and marks otherwise
unknown display BPM as `*`. Missing ratings stay unknown. Passive unique completed
HTML titles supply bounded, source-cited fallback names without inventing artist
credits, and normalized provenance retains the page URL.

The outer prepared-URL receipt binds both source and prepared child receipts. Its
complete body is staged before the final Ready callback; cancellation after child
success removes the pending marker and leaves null completion/failure evidence.
Existing exports, registry and library are never replaced. This completes the
single-work native preparation seam in milestone 5, not reviewed index traversal,
broader engines, musical-quality acceptance or desktop installation.

Local Higan and No Name/灼冬赫夜 URL jobs produce two songs / three charts,
covering all 2,092 real-parser endpoints within 8.51 ms and native x1/x2/x3
velocities. Both final-cancellation trials and artifact-only rebuilds pass; the
CLI independently completes Higan. All requests are cached/offline, original
indexes/exports are unchanged, and the new outputs remain local. Validation
passes 217 native and 88 pure SDK tests, strict Clippy for both configurations,
formatting, the existing cache/page oracles and `just check` (838/838 REUSE).

Next define the prepared-import handover to the host library: stable source and
song identities, file inventory, validation outcome, duplicate policy and a single
cancellable installation transaction. The main-owned shell/library adapters must
queue work off the UI thread, consume the outer Ready marker, keep native worker
and cache dependencies out of WASM, and supply platform-specific rollback/recovery.
Broader index traversal and HTML5 execution can then share the same preparation
boundary without changing source timing, rating or provenance policy.


### Prepared-import and host transaction slice, 2026-10-15

The [host handover](danoni-rust-import.md) implements pure `prepared_import`
admission and `import_host` duplicate planning/installation coordination, with a
read-only `inspect-import` CLI. Bound outer/source/runtime/profile success,
normalized origin, exact artifact inventories, archived work replay and the real
library scanner/resolver/parser/summary now yield an opaque import. Per-chart
endpoint, motion and omission diagnostics remain visible; musical estimates stay
provisional.

Stable IDs use canonical logical SWF URL/FlashVars and original encoded audio,
independent of transient paths, titles, codecs, fitted BPMs and retrieval dates.
Payload revisions distinguish changed metadata/artifacts/selector coverage.
Partial selections keep identity but cannot silently remove installed charts.
Object-order feature unification in an embedding host cannot change identity
hashing; generated reference JSON replays semantically while playback/source/input
bytes remain exact and all saved inventory hashes are verified.

The host transaction trait has a single atomic publication point, compare-and-swap
catalog preconditions, explicit duplicate policies and distinct committed,
rejected and needs-recovery outcomes. Known uncommitted failures roll back;
uncertain commits cannot trigger automatic rollback/retry. Cancellation has no
fallible callback after successful publication. No concrete main library storage
adapter, app import UI, accepted generation or acquisition cache is changed.

Next implement the host's durable managed-store adapter and its crash/recovery
acceptance tests, then wire the desktop job to the preparation/admission/plan/
install APIs. The interface is usable with a host-defined filesystem or database
layout; choosing that layout and integrating the main-owned shell remain a
separate implementation boundary. Broader URL routes and HTML5 can reuse it.

Validation passes 224 native / 94 pure tests, strict Clippy and `just check`
(843/843 REUSE). A local-only in-memory host admits Higan and No Name as two
songs / three charts, recognizes a separately captured Higan as unchanged, and
refuses both cancelled parents. Default and host insertion-order JSON features
produce identical identities/revisions/catalog summaries. CLI/API summaries
agree, and all original export/cache indexes and the registry remain unchanged.
No durable desktop store or real library write is claimed by this test host.


### Durable managed-store slice, 2026-10-15

The native [managed store](danoni-rust-managed-store.md) implements the existing
host transaction traits without changing the main library or shell. An explicit
fresh store holds a writer lock, stores immutable song/evidence transactions and
publishes a complete catalog with one atomic rename. Duplicate/no-op/replacement
planning retains stable identities and selector coverage. Readers can retain old
catalog rows after replacement; metadata listing does not load archived audio.

A durable pending record distinguishes the exact old catalog from the receipted
new one. Known uncommitted failures retire recovery markers; uncertain publication
never triggers rollback/retry. Recovery audits the visible catalog and its
referenced transaction inventories, retains interrupted payloads and fails closed
on a third state, missing content, corruption or symlinks. CLI commands are thin
SDK wrappers. The adapter currently requires Unix/local-filesystem semantics;
process-exit and injected-error tests do not certify actual sudden power loss.

Next wire the desktop host's background job, cancellation/progress UI and local
library reader to URL preparation → admission → duplicate plan → managed-store
installation, using explicit recovery after an uncertain outcome. Main-owned
shell/library changes remain outside this worktree's scope. A separate SDK slice
can add a single reusable job coordinator/status receipt for those calls, with
cached preparation reuse and safe resume before publication. Retention/quotas,
explicit abandoned-transaction garbage collection and broader URL/HTML5 engines
remain follow-up work; no transaction directory should be removed while an older
reader can reference it.

Validation passes 234 native / 94 feature-disabled SDK tests, strict Clippy for
both configurations, formatting and `just check` (847/847 REUSE). Eight injected
error boundaries and eight child-process exits preserve whole multi-song
replacements; additional tests exercise reentry and third-state recovery refusal.
Separate fresh API/CLI stores admit two local songs / three charts, agree on
identities/revisions/entries/files, recognize the duplicate and reject cancelled
parents. All referenced inventories and playback files verify after reopening.
The accepted exports/registry/cache indexes remain unchanged, and no new work is
fetched or executed. Local evidence is under
`~/Music/DanOni/reviews/rust-managed-store-20261015/`.


### Native import-job coordinator, 2026-10-15

The [job coordinator](danoni-rust-import-job.md) joins URL preparation → exact
Ready admission → duplicate planning → installation through one blocking SDK API.
The host owns its background queue, trusted services and target store. Structured
progress preserves nested source/preparation details and emits the full admitted
summary/plan before any installation transaction. Source-preparation readiness is
never reported as an installed job.

A saved `import-candidate.json` binds the absolute Ready root and exact manifest
hash. Resume requires no cache, worker or font: it revalidates the bound proof
closure and makes a fresh revision-checked plan. Cancellation at eight phases,
callback failure and cooperative deadlines leave a validated candidate reusable.
No fallible progress callback, filesystem write or cancellation check follows the
host's terminal installation outcome. An uncertain commit is never automatically
rolled back or retried; store recovery remains an explicit separate API.

`import-url-native`, `bind-import` and `resume-import` are thin CLI adapters.
Serializing the returned result is a host action; no separate job journal can
claim installation independently of the managed catalog. Desktop UI/job scheduling
and playback-library wiring are handed over to the main implementation session.

Next extend native acquisition from individual works to a bounded, reviewed
index/collection route, reusing these per-work jobs and the existing passive page
parser. Enumerate supported child URLs with provenance and explicit unavailable/
unsupported/partial outcomes; do not turn ordinary anchors into a general crawl.
Preserve Wayback logical/retrieval context and per-host pacing. Start with frozen
cached examples from the existing survey before fetching additional works. The
native single-work flow is now joined; broader native families, HTML5 browser
control, musical acceptance and other-platform worker confinement remain separate
milestones.

Validation passes 243 native / 94 feature-disabled SDK tests, strict Clippy in
both configurations, formatting and `just check` (850/850 REUSE). A real offline
Higan URL job cancels after planning, leaves its Ready/token with no installed
songs, and resumes without source services. The CLI imports No Name into that
fresh store, giving two songs / three charts. Both fresh content digests match
the frozen earlier candidates; repeats are no-ops and a changed token cannot
alter the catalog. Source transfers are zero, original export/registry/cache
indexes are unchanged, and local evidence is under
`~/Music/DanOni/reviews/rust-import-job-20261015/`.


### Reviewed static collection slice, 2026-10-15

The [collection coordinator](danoni-rust-collections.md) adds four reviewed static
list structures: AutumnLeaves Flash, CW author catalogues, the 2006 7-key festival
and iso's numbered Flash list. It selects work cells rather than following all
anchors, retains query/capture/scheme distinctions and missing destinations, and
never executes list scripts. Iso work links remain browser-navigation-required
because of the documented referrer guard. Engine/layout admission is still a
per-child responsibility; a discovered link is not native compatibility proof.

An opaque discovery replays its ordered inventory from exact hash-bound HTML and
copied HTTP/cache evidence. The SDK and thin CLI can discover first or run a full
URL-to-collection import. Sequential children share network/time/output bounds,
retain independent Ready/resume tokens and publish individually. Unavailable or
failed children produce partial results; cancellation preserves earlier commits.
Uncertain publication, unknown transfer accounting or a failed parent report
stops later work. The managed catalog remains authoritative after a crash; a
collection status checkpoint does not establish a library transaction.

Next broaden native engine-family coverage using the existing normalized source
corpus, keeping each new adapter behind its own source-bound reference gate.
Static discovery does not make the many HTML5 catalogue destinations executable
natively. A packaged, controlled HTML5 capture provider remains a separate
migration slice, alongside desktop UI/background-job and playback-library wiring
owned by the main implementation session. Browser-required guarded routes,
collection-wide crash reconciliation, retained-output quotas/garbage collection
and Windows/Linux confinement also remain separate work.


Validation passes 263 native / 94 feature-disabled SDK tests, strict Clippy,
formatting and `just check` (859/859 REUSE). Four frozen indexes match an
independent HTML oracle: 116 AutumnLeaves destinations, 272 CW destinations,
31 festival links, and 34 browser-required iso links plus three unavailable rows.
A real offline index import preserves Higan when another exact destination is
uncached; saved-discovery selection recognizes its duplicate without changing the
catalog. All transfers are zero, frozen Higan playback/revision data agree and
original export/registry/cache indexes are unchanged. Evidence remains local at
`~/Music/DanOni/reviews/rust-collections-20261015/`.


### Automatic variable-travel five-key profile, 2026-10-15

The [scheduled-menu extension](danoni-rust-scheduled-profiles.md) connects the
existing `scheduled-counter` projector to two exact shared gen3 control
combinations, observed Practice/Motion OFF settings and per-head travel derived
from the same zero-counter graph used for projection. The normalizer and exact
reference schema remain unchanged. Default old-profile controls/deadlines and
the worker's process/sandbox limits remain unchanged; this compiler uses the
existing 10-second per-activation maximum demonstrated by the frozen captures.

Crow, Moon Drop, Sacrifice and River complete automatic source-only extraction
and match the independent frozen chart/audio references: four charts, 1,710
taps, 44 holds and 1,798 endpoints, canonical PCM within one signed unit and
63 x1/x2/x3 scroll checks. Four original-page URL imports then prepare/install
those songs in a fresh verified managed store with zero transfers. Metadata
review inputs are supplied only to the differential comparison to match its
frozen policy; production URL jobs have no per-song capture/fitting overrides.

Maid's bitmap drawing remains an explicit failing requirement. An original blank
movie reproduces the Metal compiler-service startup error outside the sandbox
as well as within it; the production sandbox is unchanged. Cynthia's candidate
initialization is excluded from automatic recognition pending a working rendered
5/7-key gate. Broader gen3/classic/gen4 controls and renderer startup remain next
native coverage work. The existing explicit 5/7-key projector and earlier
constant-travel profile keep their verified scope. Local evidence is under
`~/Music/DanOni/reviews/rust-scheduled-profiles-20261015/`; accepted exports and
registry/cache indexes remain unchanged.


Validation passes 269 native / 94 feature-disabled SDK tests, both strict Clippy
configurations, formatting and `just check` (861/861 REUSE). The final CLI reports
Maid's bitmap requirement explicitly, rejects the unverified Cynthia build before
execution, and treats a repeated source-only Crow URL import as a no-op. The
original accepted corpus and all main-owned implementation files are unchanged.

The combined frozen replay gate passes all 28 earlier/new automatic-profile
works / 35 charts (19,783 taps, 1,134 holds, 22,051 endpoints, 786 authored-scroll
checks), including the existing seven-key references. No source timing/event
field changes and canonical PCM stays within one signed unit. This replay gate
does not claim fresh GPU execution while Metal startup is unavailable.

### Portable offscreen backend selection (2026-10-15)

The [rendering handover](danoni-rust-rendering.md) replaces the worker's forced
Metal choice with compiled-backend discovery, adapter selection and bounded
initialization fallback. The optional trusted `renderer` policy reaches source-only
profiles, URL imports and their saved replay plans; explicit requests enable
offscreen rendering. Ruffle retains device negotiation and frame readback. A
software-only policy accepts CPU-class adapters and never silently uses a GPU.
This does not bundle a software driver or complete Linux/Windows confinement.
Unsupported bitmap calls still reject; their presence alone does not prove a
chart-data dependency on rendered pixels. Renderer startup and movie execution
are validated separately, and initialization diagnostics have their own receipt.

### Retire 7i as a playable target (2026-10-15)

The current target is 5, 7, 9A and 9B. Collection filters, library admission,
legacy pack preparation/rebuild selection and gameplay verification no longer
accept 7i. New registry initialization does not require the 7i-only Canaria work.
The Rust `Mode::SevenI` and low-level SSC compilation remain for decoding and
comparing frozen research references; they do not authorize library installation.
Do not reinterpret a 7i source as 7-key or delete historical source evidence.

The 93-song/144-chart generation and its earlier all-chart migration checks are
historical snapshots that include 7i. This policy change does not regenerate or
modify the local exports/registry. A future corpus generation must exclude 7i
selectors and report its new counts; mixed-mode HTML5 capture/preparation must
select supported charts explicitly before admission. Other nine-key modes and
larger/custom modes remain outside the playable target.

Validation passes 274 native and 95 feature-disabled Rust tests, both strict
Clippy configurations, six pack-policy and nine rebuild tests, verifier syntax,
formatting and `just check`. Active parser tests exercise the four current
targets; a separate test preserves 7i identity as historical source evidence.


### Linux worker and software-rendering continuation, 2026-10-15

The [Linux handover](danoni-rust-linux.md) adds an x86-64 Landlock/seccomp
execution backend, shared Unix supervision, ELF bundle checks and original
negative confinement/rendering tests. Java remains build-time only; Mesa is
the tested CPU Vulkan runtime. Linux requires all reviewed kernel restrictions;
Windows and other Linux architectures remain fail-closed. The transferred
original rendering fixtures pass independently on Mesa Lavapipe. Confined
native-scale frames and ActionScript bitmap reads match those original results;
enlarged Linux rendering rejects before launch to retain the resource limits.
SDK/worker tests, both SDK Clippy configurations and `just check` pass.

Real-work Linux parity remains pending the corpus transfer. No corpus files or
historical jobs were read or modified, no accepted exports were regenerated,
and no actual chart dependency on bitmap pixels is inferred. See the Linux
handover for the final restricted-capture and validation results.


### Real-work Linux validation, 2026-10-16

On top of `644171a`, the transferred corpus is checked read-only against its
recorded bindings and used for fresh confined Linux jobs. The
[Linux handover](danoni-rust-linux.md#transferred-corpus-validation-2026-10-16)
records 31 fresh native works / 44 charts, including all 28 earlier automatic
works, explicit software-rendered Maid, the reviewed hold-intro selectors and
six Kouhaku charts. A separate saved HTML5 conversion covers 9A/9B; no fresh
HTML5 execution is claimed. Note/hold/event/origin/motion parity and real-parser
SSC checks pass; canonical PCM stays within the existing one-unit tolerance.

Crow completes an offline public-URL import, library reopen, cancellation after
planning, persisted-token resume and unchanged repeat with zero new requests.
The runtime audits and existing Cynthia/rewritten-selector refusals remain
strict. Cross-build Ogg waveform/byte differences and real title raster/state
differences are preserved and investigated separately from the chart result.
Accepted exports, source caches and the export registry remain unchanged; the
historical 93-song / 144-chart baseline still includes retired 7i. SDK tests,
both strict Clippy configurations and `just check` pass. Production admission,
worker confinement and reviewed maximum budgets are unchanged.
