# Rust offline song preparation

Implemented 2026-10-13 as the next slice of [the migration plan](danoni-rust-plan.md).
The shared Rust API and native CLI now produce **complete SSC songs**, including
preview tags, copied playback music/banners and source provenance. This is offline
preparation from supplied codec-stage assets. It does not decode/re-encode music,
fit new BPMs, execute Flash/HTML5 engines or acquire a URL. Current accepted
exports and their extraction/audio/reference evidence remain unchanged.

## Embeddable API

`work::prepare_work(&WorkRequest, &impl AssetSource, &mut control)` returns
`PreparedWork { manifest, files }`. Each `Artifact` has a portable relative path
and immutable shared bytes. The host can feed these files directly into the
existing song-library scanner/import path; no ZIP, filesystem location or
subprocess is required. The JSON manifest carries provenance/diagnostics rather
than replacing the game's own parsed song model. No app/library/game crate was changed.

`AssetSource::read(namespace, path, maximum_bytes)` is the only input service.
The core validates contained relative names, exact SHA-256s and byte limits;
the host must also confine namespace reads. Inputs are memoized per work, including
shared native references. The callback receives `Progress {stage,completed,total}`
with validation, audio preparation, chart preparation, assembly and completion
stages; returning an error cancels. PCM envelope/candidate scans also check it.
A host running the synchronous call on its job thread supplies its own cancellation
token/progress sink. Per-work progress context is supplied by the caller; this
is not an asynchronous desktop UI or a background job service yet.

With the `native` feature disabled, preparation still works with supplied bytes.
With it enabled, `native::DirectorySource` confines explicit roots, rejects
symlinked components, Git checkouts and over-limit reads. The CLI uses that same
API. Raw HTML is tokenized as data by html5ever; scripts/styles/title/textarea
are excluded from rating excerpts and nothing is evaluated. Charset decoding
supports the current corpus's UTF-8, UTF-8 with BOM, CP932/Shift-JIS and EUC-JP.
NFKC/whitespace normalization and exact unique excerpt/token selection reproduce
the existing publisher-rating checks. Arbitrary difficulty labels never become
numeric ratings.

## Job request

`ddi-danoni-export prepare-work JOB.json NEW_OUTPUT` reads one
`ddi-danoni-offline-job/1` document outside Git:

```text
schema: "ddi-danoni-offline-job/1"
roots: {namespace: directory, ...}
work:
  manifest: Binding
  reviews: {grid?, motion?, ratings?, artwork?}
  media: SuppliedMedia[]
  options: WorkOptions
```

A `Binding` is `{namespace,path,sha256}`. `path` is a contained relative file name;
`sha256` is the lowercase 64-digit content digest. Root directories may be
absolute or relative to the job's directory. Review asset paths resolve relative
to that review file inside the same namespace. No `..`, absolute child names,
backslashes, empty components or drive-like names are accepted. Native roots and
outputs stay outside Git. Use a fresh existing parent for output.

The manifest uses the existing normalized/legacy/synthetic source schemas in
[the preparation contract](danoni-postprocess.md). Unique nonempty chart IDs,
supported mode/lane count, hashed music and lane/hold relationships are required.
Metadata merges work defaults, native-reference metadata and chart overrides.
Shared source audio hashes group charts in source order. Native reference bytes
are preserved/hashed in provenance even where older manifests only bind their
path rather than their digest; this does not retrospectively make that reference
hash part of the old source manifest.

`reviews` defaults empty. Each present grid/motion review must cover every source
chart exactly and bind the exact raw manifest hash. Rating/artwork reviews may
cover a subset but cannot select unknown charts. Existing source rating notation,
chart identities, publisher scales, certainty and excerpt evidence are checked.
Native motion evidence must agree in rate and source SWF identity where supplied.
Banners must have the reviewed 640×200 PNG header/dimensions and match the
plain-title/native-panel provenance. Shared-audio charts require identical reviewed
artwork. This stage validates/reuses artwork; it does not render/crop new images.

`WorkOptions` defaults to `tierPolicy:"stars"`, `tierBounds:[1,3,5,7]`,
`scrollFactor:1`, `displayBpms:{}` and default pickup preview settings:
`{seconds:12,quietRatio:0.35,minimumNotes:4,maximumGapSeconds:1,
noteWindowSeconds:3,leadInSeconds:0.08}`. Display-BPM overrides use `BPM`,
`LOW:HIGH` or `*`; authored numeric CW ranges are recognized separately.
Conflicting metadata for shared music rejects. Equal known mode/meter pairs keep
source chart labels for disambiguation. Unknown ratings retain their original
labels and Edit/0. Song-level shared tempo and per-chart grids remain distinct.

## Supplied media boundary

Every distinct normalized audio hash needs exactly one `SuppliedMedia`:
`{sourceAudioSha256,audio:Binding,pcm:Binding,codec:"wav"|"ogg",provenance:Binding}`.
Playback and PCM bytes are checked before use. PCM must be complete RIFF/WAVE,
mono/stereo integer PCM16, rate 1–192000 Hz, nonempty and at most ten minutes.
Units are **sample frames**, not interleaved sample counts. Hashes include the
complete container. WAV playback must equal the supplied PCM bytes. Ogg playback
is reused with its prior receipt; no new decoder/alignment claim is made.

The codec-independent receipt for future Rust audio producers is
`ddi-danoni-prepared-media/1` with:
`{sourceManifestSha256,sourceAudioSha256,playbackSha256,pcmSha256,
sampleRate,channels,sampleFrames,codec,...producer/evidence fields}`.
All required hashes/units/codec must match the supplied assets and source.
The receipt is caller-supplied evidence, not an independent audit of the codec.
Its extra producer/transform/validation fields are preserved in provenance.

As a migration bridge, the existing `ddi-playable-pack/experimental-1` receipt
is also accepted when source hash, playback hash, rate, channels, frames and
codec match its recorded song. Older receipts do not contain a PCM container
hash: the job explicitly binds that PCM's current bytes, and the result records
this limited provenance. The Rust audio stage will produce the neutral receipt;
it need not pretend to be the old Python preparer.

## Output and publication

`ddi-danoni-prepared-work/1` contains source identity/raw manifest, request and
verified input bindings, an `inputArtifacts` mapping to preserved bytes, typed
artifact inventory entries `{path,sha256,bytes}`, and `songs[]`:

- `id`, `title`, `artist`, `simfile`, `music`, `musicSha256`, `sourceAudioSha256`.
- `pcmSha256`, `pcmSampleRate`, `pcmChannels`, `pcmFrames`.
- `banner`, `artwork`, rich `preview`, `displayBpm`, song `timingBpm` and
  `timingOffsetSeconds`.
- `charts[]`: the prepared-chart diagnostics from the initial Rust slice,
  without duplicated SSC text, plus source rating/label/classification,
  `preservedDuplicateLabel`, `musicalGrid` and `nativeMotion`.
- Explicit supplied-media scope, with no network requests or work execution.

```text
manifest.json                    ready prepared-work receipt
songs/<id>/song.ssc               complete song and chart headers
songs/<id>/music.wav|ogg          supplied playback bytes unchanged
songs/<id>/banner.png             reviewed banner bytes unchanged, if present
references/<id>.wav               supplied PCM unchanged
references/<id>-<n>.json          original chart, merged metadata, reference, rating
provenance/source-manifest.json   exact original manifest bytes
provenance/media-<id>.json        exact supplied-media receipt
provenance/assets/<sha256>        other unique input/evidence bytes
```

Content already present as music, PCM or another artifact is referenced by
`inputArtifacts` rather than copied a second time. SSC formatting uses the
existing Rust chart compiler; metadata/preview headers are now included.
Unsupported effects remain original data and explicit omission counts.

`native::write_prepared` checks complete artifact coverage/digests, writes only
to a **new external directory**, then atomically renames the final manifest as
the readiness marker. Existing directories reject. Ordinary errors/cancellation
remove the newly owned incomplete directory. A process crash/forced termination
may leave incomplete files without a final manifest; there is no automatic
interrupted-job recovery or power-loss durability guarantee yet. It does not
replace existing exports, archive generations or commit to the local library.
Those are separate host/storage transaction responsibilities still in the plan.

## Rebuild the accepted corpus

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- rebuild-accepted \
  "$HOME/Music/DanOni/export-registry.json" "$HOME/Music/DanOni/exports" \
  /tmp/danoni-rust-generation-new
```

`native::accepted_job` creates an explicit portable job using the registry's
original normalized source/reviews, accepted grids and codec-stage music/PCM.
`rebuild-accepted` prepares required normalized works; pending entries stay listed
as pending. The accepted index must bind the exact registry, and changes during
the build reject. It generates jobs and `works/<work-id>/` prepared outputs,
with a `ddi-danoni-rust-offline-generation/1` final manifest. Existing accepted
exports remain unchanged. This is a migration bridge for the current reviewed
policies, not an arbitrary legacy registry-policy interpreter or URL command.
No subprocess, Python, Node, FFmpeg, Chrome or Ruffle is called by these commands.

The accepted generation is reconstructed locally at
`~/Music/DanOni/reviews/rust-offline-20261013/generation/`: **89 works / 93 songs /
144 charts**, with 47 pending registry entries unchanged. Music and all 93 banners
are byte-identical. Rust recomputes complete preview decisions from PCM and
reparses the published ratings; 109 charts are numerically rated (103 single-song
charts plus six Kouhaku charts). All 100,939 judged heads retain the initial
bounded chart projection. This tree is local research output, not redistributable.

## Verification and remaining work

The all-chart and complete-song differential tests use the game's actual parser
and scroll code. They compare layouts, ticks/colours, holds, timing, x1/x2/x3
positions, tiers/meters/labels, full source-rating diagnostics, song metadata,
preview decisions and byte-identical media/banners. A separate optional
`DDI_DANONI_RUST_GENERATION` gate checks written artifact hashes and parses each
written SSC against the accepted songs. All evidence stays outside Git.
Synthetic tests cover changed assets/reviews, malformed PCM, charset aliases,
unique visible rating evidence, cancellation, fresh outputs, symlink/checkout
confinement and neutral media receipts. The pure API also passes without native
filesystem features. `just check` remains the branch gate, alongside the crate's
own tests/Clippy/format checks.

Remaining milestone-2 work: native artwork generation/cropping, optional ZIP and
collection assembly, archival/promotion and interrupted-swap recovery. Then port
codec/audio-origin processing and fresh BPM analysis. The URL/worker/browser/
desktop milestones remain later. Do not describe this as a complete Rust URL
importer or a newly verified audio conversion.


Final acceptance: **24 synthetic Rust tests and three local differential gates**
pass, together with the ten offline tests without filesystem features, Clippy,
formatting and `just check` (REUSE **680/680**). The written-generation gate verifies
**1,596 artifact hashes** and all 144 SSC charts. A saved 9A/9B job was rerun through
`prepare-work` into a separate directory; all **19 files** are byte-identical.
Receipts are under `~/Music/DanOni/reviews/rust-offline-20261013/`, including the
fresh generation, saved jobs, build/test logs and hash-bound validation summary.
The new generation occupies approximately 4.08 GB because it preserves PCM,
original source audio and provenance as well as the playable files.
