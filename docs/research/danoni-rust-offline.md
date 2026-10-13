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
artwork. `prepare_work` validates/reuses artwork. The separate native artwork API
below generates new source-bound reviews without a browser or FFmpeg.

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
The separate validated-generation publisher below implements archival and
interrupted-swap recovery; a desktop host can instead own its library transaction.

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

The subsequent artwork/collection/publication slice below completes milestone 2.
Next port codec/audio-origin processing and fresh BPM analysis. The URL/worker/browser/
desktop milestones remain later. Do not describe this as a complete Rust URL
importer or a newly verified audio conversion.


Complete-song slice acceptance: **24 synthetic Rust tests and three local differential gates**
pass, together with the ten offline tests without filesystem features, Clippy,
formatting and `just check` (REUSE **680/680**). The written-generation gate verifies
**1,596 artifact hashes** and all 144 SSC charts. A saved 9A/9B job was rerun through
`prepare-work` into a separate directory; all **19 files** are byte-identical.
Receipts are under `~/Music/DanOni/reviews/rust-offline-20261013/`, including the
fresh generation, saved jobs, build/test logs and hash-bound validation summary.
The new generation occupies approximately 4.08 GB because it preserves PCM,
original source audio and provenance as well as the playable files.


## Native artwork

`artwork::title_cards(&TitleRequest, &AssetSource)` and
`artwork::title_panel(&PanelRequest, &AssetSource)` return `Artwork {files,manifest}`.
They are byte-in/byte-out APIs, usable without the native filesystem feature.
`native::generate_artwork` exposes the same producers through `artwork JOB NEW_OUTPUT`:

```text
schema: "ddi-danoni-artwork-job/1"
roots: {namespace: directory, ...}
title: null | {
  title: string, artist: string, sourceManifestSha256: SHA256,
  font: Binding, roles: ["banner" | "cover" | "pack", ...]
}
panel: null | {
  snapshot: Binding, run: Binding, image: Binding,
  frame: integer, clip: string, expectedClipFrame: integer | null
}
```

Exactly one producer is selected. Root/path/hash rules are the offline job rules.
The title producer loads only explicitly supplied font bytes (at most 32 MiB),
with no system font discovery or network/external image loader. It renders our
own escaped SVG using resvg, with black background and white text. Unicode
word/grapheme wrapping retains the full title, reducing the font to a bounded
minimum when necessary. Missing glyphs, clipping and overlong titles/credits
reject for typography review. Input text is at most 300 characters per field.
An app distribution must choose appropriately licensed fonts covering its target
languages; the locally supplied system font is not a proposed bundled font.

Roles are `banner` **640×200**, `cover` **768×768**, `pack` **836×328** pixels.
Outputs are `<role>.svg`, `<role>.png` and `manifest.json`, whose schema remains
`ddi-artwork-study/experimental-1` with a distinct **`plain-title-rust/1`** template.
It binds source manifest, font digest/family, renderer version and per-role
PNG/SVG hashes, dimensions, title/credit and chosen typography. The font is not
copied into the artwork output. A normal `ddi-reviewed-song-artwork/1` review can
bind its banner and evidence for `prepare_work`; raw title generation does not
itself select a chart or alter source data.

The panel producer consumes already captured, hashed worker pixels/state/receipt;
it does **not** execute the SWF. It requires the reviewed pinned Ruffle revision,
a successful unchanged-input run, complete unhalted rendered state/audit, one
selected capture and one visible named clip with complete noncyclic ancestry.
Geometry is transformed from stage pixels to viewport pixels using the captured
six-value affine matrix. The axis-aligned crop includes a one-pixel margin and
must remain wholly in the captured viewport. Worker P6 RGB input is bounded to
1280×960, display entries to 4096 and selected frame to 1–1200. A recipe may bind
the expected clip movie frame. Unexpected dependency/audit outcomes reject.
These checks bind supplied evidence; they are not a replacement for restricted
worker supervision or review of its renderer/build identity.

Outputs are lossless `panel.png`, fitted `banner.png` and `manifest.json` with
schema **`ddi-native-title-panel/2`**. The latter retains original SWF/URLs,
retrieval/capture provenance, worker/build identity, exact input bindings, capture
and clip frames, original bounds/matrix, `[x,y,width,height]` crop in viewport
pixels, transformation/version, fit dimensions/matte origin and image hashes.
The banner uses Rust Lanczos3 resizing, preserving aspect ratio on a 640×200 black
matte. This crop has the same pixels as the prior FFmpeg crop; resize pixels and
plain-title typography are explicitly versioned and need not match FFmpeg/Chrome.
Both native-panel `/1` and `/2`, and both plain-title template versions, remain
readable by artwork review validation. Existing accepted song banners are reused.

## Collections and sealed generations

`package::write_zip(sink, &[Artifact])` is a filesystem-free ZIP assembler over a
host-owned `Write + Seek` sink. It checks portable unique paths, sorts names,
sets the fixed ZIP epoch timestamp/0644 permissions and uses Rust Deflate.
Identical supplied files produce identical archives with the pinned dependency
versions. Archive byte identity with Python's ZIP writer is not required.

The native `package_generation` API and `package-generation JOB NEW_OUTPUT` take:

```text
schema: "ddi-danoni-package-job/1"
generation: directory                  ready Rust offline generation
roots: {namespace: directory, ...}      explicit font asset roots
font: Binding
collections: [{id: string, title: string, artist: string}, ...]
```

The job's relative directories resolve against its containing directory. Every
work must belong to exactly one configured nonempty collection (maximum 64).
Collection IDs/titles must be portable single file components. Duplicate and
ambiguous song directories reject. Only a fresh disjoint external output is
created; errors/cancellation clean up that newly owned partial output.

Packaging reconstructs **every work from its self-contained input artifacts**,
revalidates original source/reviews/media and recomputes previews/chart projection.
It compares the complete prepared manifest and every artifact byte, then parses
the full SSC using `ddi-chart`, checking endpoint kind/lane/tick and audio time.
The publication gate additionally enforces the current **9 ms** endpoint ceiling.
No old absolute asset roots are needed to validate the completed generation.
Source generation contents are checked again after assembly.

```text
manifest.json                         ddi-danoni-rust-packaged-generation/1
source-generation.json                original offline-generation metadata
works/<work>/...                       complete evidence and prepared song artifacts
<collection>/artwork.json              font/template and per-role image/SVG proof
<collection>/banner.svg, cover.svg     generated collection typography
<collection>/files/<title>/banner.png  pack banner, 836×328
<collection>/files/<title>/cover.png   pack cover, 768×768
<collection>/files/<title>/<work>--<song>/song.ssc
<collection>/files/<title>/<work>--<song>/music.ogg|wav
<collection>/files/<title>/<work>--<song>/banner.png
<collection>/<title>.zip               exactly the adjacent files/ tree
```

The final manifest retains the offline generation's work rows/counts/pending
entries and registry/accepted-generation identities. It adds `collections[]`
`{id,title,songs,charts,files,zip}`, `artifacts[]` `{path,sha256,bytes}` and explicit
validation/media scope. The sorted exact inventory covers **all files except the
root manifest itself**, including work manifests, evidence, collection graphics
and ZIPs. Trees reject symlinks/special files, depth >64, >100000 files and individual
files >2 GiB. The manifest is bounded to 16 MiB. The older generated CLI jobs are
not copied; their requests/bindings are already retained in each work's manifest.

`package::verify_generation` / `verify-generation DIR` check that entire inventory,
rebuild all works, check SSC endpoints, collection artwork proof/dimensions and
exact collection coverage, and compare every ZIP entry with its adjacent file
and every song file with its verified work artifact. An inventory hash alone
cannot bless missing evidence or a malformed chart. This is a local integrity
and semantic gate, not an authenticity signature or protection from arbitrary
concurrent processes rewriting trusted output directories.

## Publication, archival and recovery

`transaction::promote(stage,destination,history,control)` and CLI
`promote STAGE DESTINATION HISTORY` require a ready **sealed Rust generation**,
an existing external history directory, disjoint canonical paths and a fresh
or previously validated Rust destination. Legacy Python exports are not implicitly
migrated/replaced by this command. Same-filesystem renames are required; there is
no copying/deleting fallback after a cross-filesystem error. The host can bypass
this filesystem publisher and ingest immutable artifacts with its own transaction.

A history lock and destination lock use OS advisory file locks; they release on
process exit and cover cooperating publishers even with different history roots.
Files/directories are flushed before the swap. A versioned journal is persisted
with write/fsync/rename/directory flush before and after each directory rename.
Old generations become `history/archive-<nonce>` without deleting their contents.
The final stage-to-destination rename is the publication commit point. There is
a short interval with the old directory archived and destination absent; this is
not a portable atomic exchange of two directories. An integrating host must
coordinate library readers or retain its current in-memory generation during it.

Each `history/transaction-<nonce>.json` is **`ddi-danoni-publication/1`**:
`{schema,phase,stage,destination,archive,newManifestSha256,oldManifestSha256}`.
Paths are canonical absolute directories; the optional old hash is null on first
publication. Phases are `prepared`, `old-archived`, `promoted`, `committed` or
`rolled-back`. Raw root-manifest digests bind the sealed inventories.
The result **`ddi-danoni-publication-result/1`** contains
`{schema,status,destination,stage,archive,journal,generationSha256}` and optional
`recoveredError` when the commit point succeeded despite a later ordinary error.

`recover JOURNAL.json` / `transaction::recover` examine directory identities and
run full validation, accounting for a rename occurring before its journal update:

- New destination present, stage absent and expected archive valid: commit.
- New stage intact, old destination intact: rollback/prepared cancellation.
- New stage intact, destination absent and expected old archive valid: restore old.
- First publication still staged with destination absent: rollback, keep stage.
- Unknown, missing, changed or ambiguous generations: reject and preserve evidence.

Recovery is idempotent while those transaction locations remain available.
Ordinary cancellation/errors before commit trigger recovery with cancellation
suppressed, restoring the old destination. Errors after commit report its actual
committed state. Interrupted temporary journal bytes are preserved separately
once the existing journal and directory identities permit recovery. A crash during
the *first* journal write can leave only a partial temporary file, before any old
rename: it blocks another publication for operator review rather than guessing
its incomplete paths. Incomplete *preparation/packaging* directories are not
resumable jobs; they lack a valid final readiness manifest.

These filesystem/locking/flush and interruption cases are tested on macOS in this
slice. Actual power cuts and other OS/filesystem durability guarantees have not
been tested; native extraction confinement remains a later platform gate.

## Artwork/collection acceptance, 2026-10-13

A fresh local generation under
`~/Music/DanOni/reviews/rust-packaging-20261013/` contains **89 works / 93 songs /
144 charts / 1,977 inventoried artifacts**, with both collection file trees/ZIPs
and generated pack artwork. The isolated publication trial moves it to
`published-trial/`; accepted `~/Music/DanOni/exports/` is unchanged. All supplied
song audio/banners retain byte identity. All five Kouhaku native title crops are
pixel-identical to the prior lossless crops; Japanese title cards and retained
native typography were visually inspected. Source PCM, old grids and codec
receipts remain supplied inputs, so this does not prove Rust codecs/BPM analysis.

Synthetic tests now exercise typography/glyph rejection, escaping, full-title
retention, exact pixels, malformed/ambiguous geometry, reproducible ZIPs, source
reconstruction, changed/extra files, cancellation, first publication, archival,
stale and partial journals, repeated recovery and cooperating publishers.
**36 synthetic tests and three local differential gates** pass, together with
**15 artwork/offline tests without filesystem features**, Clippy, formatting and
`just check` (REUSE **684/684**). Final standalone CLI verification rechecks all
1,977 sealed artifacts and both archives. The sealed root manifest digest is
`fae929b7851c37931c24eb4eab70d0612fc83ab79d772ad17f0f1859230ee055`.

Receipts include build/check logs, explicit local jobs, generated images,
per-crop comparison and sealed/publication manifests; third-party inputs/results
and the system font stay outside Git.

Dependencies added for this slice, checked against crates.io on 2026-10-13:
[resvg 0.48.1](https://docs.rs/resvg/0.48.1/resvg/),
[image 0.25.10](https://docs.rs/image/0.25.10/image/),
[zip 9.0.2](https://crates.io/api/v1/crates/zip) and
[unicode-segmentation 1.13.3](https://docs.rs/unicode-segmentation/1.13.3/unicode_segmentation/).
The game parser is a local dependency of the standalone tool; its workspace and
WASM dependency tree remain untouched.
