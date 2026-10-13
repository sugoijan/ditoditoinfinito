# Rust passive native extraction

Implemented 2026-10-13 as the first slice of
[milestone 4](danoni-rust-plan.md#4-native-extraction-and-supervision).
The standalone `ddi-danoni-export` library now decodes stored AVM1 graphs,
projects all seven existing structural families, measures saved arrow motion,
reads captured stream-start counters and checks cross-run clock agreement.
The production API/CLI has no Python, Node, browser or Ruffle dependency.
The subsequent SWF slice below inventories and demuxes saved movie bytes.
The [combined saved-run producer](danoni-rust-normalization.md) now emits complete
normalized structural works and bound native-scroll evidence. It **does not execute
a movie**. The [pinned chart contracts](danoni-rust-pinned.md) now use the same
APIs; the [Rust supervisor](danoni-rust-supervisor.md) now creates source-bound
fresh captures under the fixed macOS sandbox. All third-party data and parity results remain local; accepted exports are unchanged.

## API and provenance boundary

The I/O-free entry point is
`extraction::prepare_projection(&ProjectionRequest,&AssetSource,checkpoint)`.
Assets use the existing [namespace/hash binding](danoni-rust-offline.md):
`{namespace,path,sha256}` with contained relative paths and lowercase SHA256.

```text
ProjectionRequest:
  movie: Binding
  capture:
    snapshot: Binding
    run: Binding
    config: Binding
  options:
    family: "child-timeline-5" | "dac-7" | "dac-5" | "dac-fixed-5" |
            "scheduled-counter" | "scheduled-constant" | "parafla-counter" |
            "higan-pilot" | "kouhaku-2004"
    pinnedSelection: string | null      default null; Kouhaku only
    mode: "5" | "7"
    sourcePrefix: "" | "n"             default ""; child timelines only
    sourceSuffix: string | null         default null; child timelines only
    reviewMissingDecorativeSprites: bool default false; scheduled families only
  optionalColor: null | {review: Binding,missingResponses: Binding[]}
```

`mode` is explicit: five/seven built-in lane order is not inferred from array
length. Other modes remain unsupported by these native contracts; the separate
HTML5 path still covers 7i/9A/9B. Child timelines require mode 5 and seven-lane
DAC mode 7. A suffix has at most 32 letters/digits/underscores; it cannot select
another family's data. The pinned five-key contracts require exact reviewed
movie hashes and use separately documented movement/clock scope. Undefined
optional fields are distinct from explicit native `null`, which is not an array. Unknown request/options fields reject.

The job hashes **actual preserved movie bytes**, snapshot, parent report and
configuration. Snapshot schema is `ddi-ruffle-probe/experimental-1`, parent
schema `ddi-ruffle-run/experimental-1`; Ruffle revision is exactly
`89a7049965f3090e706265670df309a5128233ac`. The parent must explicitly report
exit 0, null termination and unchanged inputs. Its config hash must equal the
supplied config binding, parsed config must equal the snapshot's entire config,
and snapshot movie hash must equal the supplied movie bytes. This verifies
saved receipts; it does not independently relaunch the worker or prove original
Flash behavior. The combined producer now binds cached resource bytes and
matching saved worker/sandbox identities. Actual worker/profile verification belongs to the fresh supervisor.

Source bytes are immutable. No ActionScript, getters, coercions or source
JavaScript run in the host. `Graph`, `text` and `numeric` are lower-level data
helpers; direct calls do not certify run provenance. Callers of the independent
motion/audio helpers must bind their captures too before publishing a work.
Cancellation checkpoints bracket reads and run during per-snapshot projection
and motion work. The host runs synchronous parsing/CPU work off its UI thread.

## Graph contract and bounds

Graph IDs are local array indices, checked against the object's recorded ID.
Accessed objects must be complete. Duplicate decoded property names, dangling
references, truncated strings/objects, missing array cells, invalid lengths,
halted/incomplete states and nonfinite numbers reject. A complete array has a
stored numeric `length` and every index `0..length-1`; prototype properties are
not traversed. Numeric strings are parsed as finite decimal numbers; booleans,
accessor placeholders and special nonfinite-number records are not coerced.

String identity comes from the exact `utf16` units, **not** the hook's lossy
`text` display field. Valid surrogate pairs decode normally. A required string
with an unpaired surrogate rejects conversion; original units remain in the
preserved snapshot, rather than silently changing a title or property name.

Bounds are 64 MiB per snapshot, 128 MiB per movie, 16 MiB per run/config/review,
512 MiB total job input, 4,096 capture iterations, 32,768 graph objects,
640,000 properties per accessed object, 20,000 array cells and 131,072 UTF-16
units per decoded string. Audit/resource limits are 100,000 entries / 4,096
resource declarations; URL membership uses a set. Schedule frames are finite
within ±720,000, including negative spawn frames. Stages require increasing
worker iterations. These are bounded decoding contracts, not a process-memory
quota or a claim of constant UI latency.

## Families and original data

The Rust code ports the repository's MIT
[family contracts](../../tools/danoni-dump/ruffle/native_families.py), using the
[stored-property graph](../../tools/danoni-dump/ruffle/higan.py) read model.
The earlier [native adapter handover](danoni-native-adapters.md#family-contracts)
provides the source discoveries and detailed historical examples.

- Child timeline / DAC 7: require a unique bank, repeated unconsumed cursors,
  complete source/native agreement where source fields exist, supported lane
  inventory, hold pairs, speed pairs and native visual stop toggles. Explicit
  suffixes require every selected source lane. Unmapped active effects reject.
- DAC 5 / fixed DAC 5: preserve source order issues, hold/source agreement,
  fixed/configurable keys, initial pixel increments, inactive comment/empty speed
  tails and per-arrow speed changes. Additive two-pixel and multiplicative speed
  policies remain distinct. Independent onigiri changes are not guessed.
- Scheduled counter: recover **every** tap and hold head as spawn + captured
  travel, then compare complete lane-counted schedules with raw data. Preserve
  native schedule indices, held tails and boost/speed state independently.
- Scheduled constant: prove split judgment arrays against mutated spawn heads,
  untouched tails, duration caches and complete `head - set_cnt` schedules.
  Older engines need a later zero-counter witness after cleanup, with unchanged
  schedules/keys/speed and stable retained metadata. Split, compiled and clock
  initialization iterations are distinct facts.
- ParaFla: use the runtime's parsed pixel-speed bank with proved initial units;
  divided dos can load into unsuffixed fields despite a numeric selector. Key-code
  zero schedules outside the selected mode are retained as inactive evidence,
  not turned into extra playable lanes or silently erased.

Family selection is currently supplied explicitly and then structurally verified.
Automatic family/selector discovery is not implemented. The separately pinned
Higan and Kouhaku adapters have not been replaced by these seven families.

Optional decoration exceptions retain their original diagnostics. The
missing-attachment review accepts only the exact known message, after compilation,
with all required receptors present; a halt still fails. The missing-color review
requires this movie's exact `iro.txt` URL, one captured HTTP-404 response/body
binding and the exact same-iteration fetch-denied/error pair. Other errors or
missing resources cannot be waived. This narrow reader supports plain absolute
http/https movie URLs without query/fragment; general URL/archive resolution
belongs to acquisition. It never invents a response.

## Results and units

`prepare_projection` returns `ddi-danoni-native-projection/1`:

```text
producer: "native-family-projections/1"
request: original typed bindings/options
preparedIteration: integer worker iteration
projection: legacy-compatible family projection object
sourceInputs: {"namespace:path": Binding, ...}
reviewedOptionalColorDiagnostics: original audit entries[]
scope: explicit passive-projection limits
```

`projection` retains the existing keys documented in
[the reference schema](danoni-native-adapters.md#recipe-and-normalized-layout).
It includes `mode`, canonical `lanes` with original `nativeFrames` and paired
`nativeHoldFrames`, `speed/boost/colors`, original metadata and native keys.
Applicable families additionally retain bank IDs, source ordering issues,
selectors, schedules, clock fields, travel, initialization stages, coded palettes,
parsed pixel speeds, inactive schedules/tails and diagnostic exceptions.
No song credits or difficulty stars are invented.

All these native frames are **60 Hz**, not worker iterations, SSC ticks or BPM
beats. `relative_chart(projection,logicalOrigin)` adds
`tapsSeconds/holdsSeconds` and event `seconds` using
`(nativeFrame - logicalOrigin)/60`; it retains every original frame alongside
the derived audio-relative seconds. It does not realign audio, rewrite frames
or quantify original Flash/device latency.

`movie_audio_origin(capture,clockField,streamIndex)` requires an exact snapshot
at a selected captured MP3 start. It retains all stream/slice/seek fields, adds
`logicalFrame`, clock scope, stream index and the next stream's sample boundary.
Those byte offsets/sample frames are not decoded into music by this stage.

`movie_observation(capture,projection,MotionSelection,clockField,checkpoint)`
uses native MovieClip names and selected iterations:

```text
MotionSelection:
  note,receptor: strings
  lane: canonical zero-based lane index
  iterations: worker iteration integers[]
  targetY: native stage pixels | null
  geometryReference: MovieClip name | null
```

At least three unique visible observations must prove stable geometry, linear
upward movement, clock progression and arrival within 1.5 native frames. Twip
rounding is bounded to 0.05 stage pixels. Stored initial pixel speed and active
source multiplier must explain movement; DAC arrows additionally prove stored
per-arrow speed. The result is the existing
`ddi-native-motion-measurement/experimental-1`, including stage-pixel samples,
pixels/sec, reference width, arrow heights/sec, arrival residual and author ratio.

`validate_clock_agreement(origin,motion)` requires every motion sample's
`movieFrame - iteration` (the sample’s selected logical counter) to match the captured audio start, using the
same clock field. It rejects borrowing another run's unrelated counter origin.
Combined extraction must also bind all captures to the same movie, selector and
resource context, then prove decoded audio origin before producing songs.

## CLI

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- project-native /tmp/projection-job.json /tmp/projection-new
```

The external job is `ddi-danoni-native-projection-job/1`, with `roots` and
`request` above. Relative roots resolve against the job file. Filesystem access
uses the offline containment/no-child-symlink rules. A fresh external output
with an existing parent contains only `manifest.json`, the full projection
result. Its final rename marks readiness; cancellation/failure removes owned
partial output. Inputs remain in the host's preserved cache; this folder is
neither self-contained music nor a playable/normalized work.

Errors use `invalid-native-projection`, existing hash/path/read errors, or the
specific `native-source-bank-missing`, `native-motion-segment-unproved` and
`native-clock-mismatch`. They are failures, not guessed fallback charts.

## Local acceptance and two existing proof gaps

The accepted source set contains **116 structural-family charts**. Rust projects
**115 charts / 79 works**; **114 charts / 78 works** also match every applicable
movement, audio-start, cross-run clock and normalized lane/event field. This
covers **68,329 taps / 3,664 holds** and every one of the seven families.
A fresh run of the current Python data-only adapters independently succeeds on
those same 114 and rejects the same two cases. Python is a development oracle,
not a production dependency.

Older frozen references lack some later-added proof fields. The gate records
only those known additions (`initialPixelSpeed`, `compiledIteration`,
`clockInitializationIteration`, `inactiveSpeedTail`, author multiplier and
stage-position tolerance), compares **all original fields**, and independently
requires complete equality to current Python output where supplied. Numeric
JSON spelling may differ; tolerance is 1e-8 + 1e-12 times magnitude, not musical
retiming permission. No projection fields are discarded from Rust results.

The two failures are newly exposed historical-reference gaps, not Rust-only
regressions or automatic removals from the accepted packs:

| Existing chart | Current failure | Follow-up |
| --- | --- | --- |
| RE:RAVE Hard | Loaded source speed data exists without a native speed bank; both current adapters reject. | Inspect the selected source/control path and establish whether it is inactive retained data or an unobserved active speed mechanism. |
| Destined Marionette Original | Raw projection agrees; saved motion spans author speed segments under the strengthened units check. Both current movement helpers reject. | Establish the source's movement policy and capture a suitable segment or add an independently proven per-arrow contract. |

Do not suppress either failure to regenerate old outputs. Existing accepted
exports are unchanged; the other charts of RE:RAVE are independent successes.
Eleven original synthetic tests cover corrupt graphs/strings, null versus
undefined, holes/duplicates, source selection/agreement, consumed cursors,
constant mutations/durations/delayed clocks, counter schedules, DAC speed
semantics/tails, ParaFla inactive lanes, exact stream starts, clock mismatch,
run/config/movie provenance and narrowly reviewed optional-color failures.

All local receipts live under `~/Music/DanOni/reviews/rust-projection-20261013/`.
The CLI/core equivalence run passes; changing a config hash rejects without
creating output. Native/pure tests, formatting, Clippy and `just check` include
this slice. No third-party captures or oracle outputs are fixtures or committed.

```sh
DDI_DANONI_EXTRACTION_REGISTRY="$HOME/Music/DanOni/export-registry.json" \
DDI_DANONI_EXTRACTION_OUTPUT=/tmp/danoni-projection-fresh \
cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --test extraction_corpus -- --nocapture
```

The optional `DDI_DANONI_EXTRACTION_ORACLE` points to local current Python
comparison outputs. Without it the gate compares the frozen source references
and records explicit known additions/rejections. Output must be fresh; omit it
for a read-only run. Absent registry reports a skip.

## Next extraction slices

The [combined producer](danoni-rust-normalization.md) now joins these captures
and SWF demux into normalized artifacts, retaining source frames, provenance,
event losses and independent shared-music origins. Port the separately pinned
Higan/Kouhaku contracts and resolve the two earlier proof gaps plus the combined
producer's M. MOON selector-plan gap before claiming reproduction of every
accepted native chart. Then port/package the restricted worker supervisor,
resource-request loop, limits and cancellation. URL discovery/acquisition remains
milestone 5; desktop library installation remains a host transaction.


## SWF audio inventory and demux, 2026-10-13

`swf::Movie::read(bytes,checkpoint)` accepts FWS or zlib-compressed CWS, verifies
original/decompressed lengths, reads RECT/FPS/frame count and walks nested
DefineSprite timelines. It does not interpret ActionScript. Input is at most
16 MiB, declared output at most 64 MiB, sprite depth at most 32, tags at most
200,000 and stream heads at most 4,096. ZWS, malformed/trailing compression,
truncated tags and missing End reject. The pure Rust `flate2` 1.1.10 zlib-rs
backend is already present through ZIP support; it is now a direct dependency.
Cancellation runs between decompression buffers, tags, MP3 frames and blocks.

`movie.inventory(checkpoint)` returns all stream heads and their MP3 block
inventories. Non-MP3 heads remain visible but their payloads are not extracted.
`movie.demux(route,optionalWholeFileInterval,checkpoint)` selects exactly one
head on an explicit timeline route, such as `root/sprite229`. Lower-level
selection does not certify a runtime capture or identify a song automatically.
`movie.observed_stream(&ObservedStart,checkpoint)` accepts:

```text
ObservedStart (unknown fields reject):
  sliceStart: unsigned byte offset in Ruffle tag data
  sliceEnd: unsigned exclusive byte offset in Ruffle tag data
  sampleRate: unsigned samples per second
  stereo: boolean
  latencySeek: signed 16-bit sample-frame count
```

Both offsets exclude the SWF header/RECT/FPS/frame-count bytes. The reader adds
`movie.header.tagStart` to obtain whole decompressed-file offsets, requires the
selected timeline's exact End, a unique head matching rate/channel/latency, and
continuous payload under the pinned Ruffle buffering model. Other sprites and
menu blocks are excluded. Empty blocks remain part of timing evidence. No
heuristic longest-stream selection or header correction is applied.

MP3 payloads must consist of whole MPEG Layer III frames inside each block;
ID3, a frame crossing a block boundary, inconsistent sample count or rate/channels
reject. Headers share the Rust audio decoder's frame parser, while SWF counting
retains *physical* frames, including metadata frames. The decoder inventory
separately accounts for Xing/Info/VBRI duration metadata. Demux concatenates
unchanged encoded bytes; it does not decode, trim, normalize or transcode them.
The [Adobe SWF19 specification](https://open-flash.github.io/mirrors/swf-spec-19.pdf),
printed pp. 182–186, defines MP3 latency/seek fields and continuous streaming;
the implementation ports this repository's MIT
[static demux reference](../../tools/danoni-dump/ruffle/swf_audio.py) and
[observed-slice selector](../../tools/danoni-dump/ruffle/extract_native.py).
The version was checked against [flate2's release documentation](https://docs.rs/flate2/1.1.10/flate2/).

The output stream object keeps `ddi-swf-audio/experimental-1` semantics:

```text
schema, path, headTagOffset, headMovieFrame, compression
sampleRate, channels, samplesPerMovieFrame, latencySeek
movie: {fps,frames,tagStart,swfVersion,sha256}
blocks: [{movieFrame,tagOffset,sampleCount,seekSamples,mp3Frames,
          mp3Offset,mp3Bytes,sha256,sourceSampleOffset,seekResidualSamples,
          sourceMp3Offset?}]
mp3Sha256, mp3Bytes, declaredSampleFrames, firstBlockMovieFrame
continuousZeroLatencyEligible, ruffleContinuousPayloadEligible
firstRufflePayloadGapMovieFrame: integer | null
selection?: {sliceStart,sliceEnd,excludedBlocks,scope}
nativeSlice?: {tagDataOffsetBase,sliceStart,sliceEnd}
transformation: description of unchanged MP3 frame concatenation
```

Movie frames are one-based timeline frames; sample counts and seeks are **sample
frames**, independent of channel count. Byte offsets refer to the decompressed
whole file except `nativeSlice.sliceStart/End`. `mp3Offset` addresses the selected
output; `sourceMp3Offset` retains the unsliced stream offset. `sourceSampleOffset`
is the cumulative count before that block. `seekResidualSamples` equals cumulative
samples plus signed seek, minus `(movieFrame-firstBlockMovieFrame)*rate/fps`, minus
latency. Zero-latency eligibility additionally requires contiguous block frames,
zero latency and first seek, and zero residual on nonempty payloads. Ruffle
payload continuity is a separate buffered-samples calculation, ignoring absent
frames after the last payload. Neither flag measures decoded musical alignment.

The Rust reader additionally rejects missing MP3 latency headers, duplicate
sprite routes, multiple blocks on one timeline frame, and nonempty End/ShowFrame
tags. It uses the actual End-tag end offset instead of assuming a short header.
All current corpus inputs satisfy these stricter checks. Unknown tags are skipped
as bounded opaque payloads; event sounds (DefineSound/StartSound), external audio,
non-MP3 compression and ZWS need later explicit adapters.

### Source-bound job and artifact

`swf::prepare_demux(&DemuxRequest,&AssetSource,checkpoint)` verifies saved
movie/snapshot/parent/config bindings with the same pinned revision and successful
run checks as projection. `DemuxRequest` has `movie: Binding`, `capture:
CaptureBindings`, `streamStartIndex: usize` and `optionalColor: OptionalColorEvidence
| null`. Unknown fields reject. It reads the selected MP3 start from the original
capture, matches actual movie FPS and the head's samples-per-movie-frame budget,
and requires a complete nonhalted audit with
no unreviewed errors. The existing exact missing-color HTTP-404 exception remains
explicitly bound and its original diagnostics are retained. No decoration error
is automatically waived by this audio-only job.

`PreparedDemux {bytes,manifest}` is available without filesystem features.
The manifest schema is `ddi-danoni-native-demux/1`, producer
`native-swf-mp3-demux/1`, with original `request`, `sourceInputs`, `streamStart`,
`stream` above, `reviewedOptionalColorDiagnostics`, and
`audio:{path:"stream.mp3",sha256}`. Its `scope` explicitly excludes decoded origin,
complete chart contracts and fresh execution. The combined producer now binds
cached resource bytes and matching saved build identities; the fresh supervisor
must still verify/package the actual worker and sandbox implementations.

The native CLI reads an external fresh-output job:

```text
{schema:"ddi-danoni-native-demux-job/1",roots:{namespace:directory,...},
 request:DemuxRequest}
```

```sh
target/danoni-rust/debug/ddi-danoni-export demux-native /tmp/demux-job.json /tmp/demux-new
```

It writes `stream.mp3` and finally `manifest.json`; existing outputs are never
overwritten. Canceled/failed owned outputs are removed. The artifact can supply
the existing Rust decoder/resampler/origin stage through the combined producer,
which binds native PCM and logical chart origin. This CLI is not a URL-to-song importer.

### Local acceptance and audit follow-up

`~/Music/DanOni/reviews/rust-swf-20261013/corpus-verified/` contains the final
local gate. All **85 streams** match every MP3 byte, block/hash/count, source and
selected offset, latency/seek residual, movie header and eligibility flag against
the MIT Python reference: 79 structural streams, Higan and all five Kouhaku songs.
Kouhaku/Higan use explicitly selected static routes here; their pinned chart
contracts and fresh execution are still unported.

Of the 79 structural streams, **63** pass the complete source-bound saved-audio
job. **16** return `native-audio-audit-unproved` with the precise message
`Unreviewed native runtime errors`: FAKE800 and 15 gen4 ParaFla works record
`Tried to instantiate a non-registered character undefined`. Their existing
scheduled-family chart projections prove the decoration exception using earlier
converter/receptor snapshots; the audio captures alone do not contain those
snapshots. The differential gate records these rejections and checks their static
observed-slice demux separately. This is not a new chart/audio mismatch or a
reason to change accepted exports. The [combined producer](danoni-rust-normalization.md) now binds these runs, checks
control-plan/clock identity and exact diagnostic multiplicity, and retains the
original audit before applying the established decoration review. All 16 affected
works pass that join; the standalone audio-only job stays strict.

Four original synthetic tests cover compressed/uncompressed lengths, empty long
blocks, signed timing/seek differences, nested and partial slices, gaps, MPEG
truncation/counts, ambiguous heads, provenance/failed runs, FPS/channel mismatches,
audit failures and cancellation. The 79-stream decoder-origin gate also reruns
against preserved PCM after sharing the MPEG header parser.

```sh
DDI_DANONI_SWF_CASES="$HOME/Music/DanOni/reviews/rust-swf-20261013/cases.json" \
DDI_DANONI_SWF_OUTPUT=/tmp/danoni-swf-new \
cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --test swf_corpus -- --nocapture
```

`cases.json` is local-only `ddi-danoni-swf-parity-cases/1` with a corpus root,
`expectedCases` and hash-bound movie/MP3/report bindings. Each row supplies either
a complete `request`, or an explicit static `route`. A known rejected request
additionally supplies the exact expected code/message and `ObservedStart` for the
separate static comparison. Output is optional and must be fresh. No acquired
movie, music, per-work job, oracle or result is committed.
