# Prepared imports and host library transactions

This slice connects [URL-prepared songs](danoni-rust-url-preparation.md) to the
main library's existing scanner, asset resolver and `ManifestEntry`. It supplies
an immutable validated import, stable identities, duplicate planning and a
cancellable transaction coordinator. Actual filesystem/database storage remains
an implementation of the host interface: this branch does not write the app's
library or choose a desktop storage layout.

The implementation lives in `tools/danoni-dump/rust/src/prepared_import.rs` and
`import_host.rs`. Both are available without `native`; they perform no network,
filesystem, browser or worker operations. The existing local `ddi-library` crate
is now a dependency of this standalone SDK. URL parsing is platform-free and
available in both feature configurations; HTTP/runtime dependencies remain
optional. No external dependency version or game workspace configuration changes.

## Validated handover

Call `prepared_import::validate(&ready_binding, &impl AssetSource, progress)`.
The host supplies exact bytes and the outer `ddi-danoni-url-prepared-run/1` receipt
binding, not a path to one of its successful children. `load_ready(directory,
progress)` is the optional native confined-filesystem adapter. Neither function
falls back to a child manifest when the outer Ready marker is absent.

Admission checks:

1. The outer and source, native dependency-run and native profile success markers
   must be present, bound and successful. All three source/runtime/profile views
   must name the same normalized manifest and normalization receipt, including
   paths and hashes. The prepared source manifest must equal that normalized work.
2. The unique bound runtime context supplies the logical movie URL and FlashVars.
   It is distinct from retrieval/page URLs and Wayback capture dates. Source and
   normalized audio-origin records must cover exactly the selected charts.
3. Every prepared inventory file is read, bounded and hash/length checked. Paths
   are contained and case collisions are rejected. Inputs have the existing
   512 MiB work limit; the inventory has at most 16,384 files.
4. The core work is rebuilt from its own archived `inputArtifacts`, including
   reviews and media receipts, and compared with stored metadata and artifacts.
   Playback, source and media bytes must match exactly. Generated
   `references/*.json` wrappers can differ in object key ordering only: their
   parsed JSON values must match, and their original inventory hashes are still
   checked. This accommodates an embedding host enabling `serde_json/preserve_order`.
5. The one fresh-preparation supplement is source/hash bound, has a recognized
   tempo outcome and remains `musicalQuality:"provisional"`. No extra supplemental
   artifacts can be smuggled into a prepared work.
6. The actual library scanner must find every song's SSC. Its asset resolver must
   select the recorded local music/banner, and the actual SSC parser must expose
   all the expected built-in charts. Current playback payloads are flat song-local
   files with no background/effect assets; other playback shapes stop explicitly.
   Work replay retains full endpoint/hold/lane/grid/motion checks.

Progress stages are `receipts`, `inventory`, `replay`, `library`, `complete`.
Returning an error cancels admission before it yields an import. Verified byte
buffers are retained as immutable `Arc<Vec<u8>>`: installation consumes those
bytes rather than reopening mutable staging paths after validation.

The result is an opaque `PreparedImport`, constructible only through admission.
Its `summary()` is serializable as `ddi-danoni-prepared-import/1`, but a serialized
summary cannot be deserialized into a validated import. It exposes:

- `digest`: stable identity of the candidate's library content.
- `source`, `sourceIdentitySha256`: runtime logical movie URL and sorted FlashVars.
- `readyReceipt`, `preparedReceipt`: exact original provenance bindings.
- `musicalQuality:"provisional"`, `tempoStatus`: `estimated` or
  `exact-source-fallback`.
- `songs[]`: stable song/source/audio identities, payload revision, selected
  native selector indices, actual `ManifestEntry`, file inventory and validation.

Each `PreparedSong` exposes its immutable `summary()` and `files()`. Playback file
names are relative to that song (`song.ssc`, `music.ogg`/`music.wav`, optional
`banner.png`); each inventory row is `{path, sha256, bytes}`. `ManifestEntry.chart`,
`music` and `banner` use those names. Its charts use the main library's sort order
and original parsed `index`, not a new extraction-specific chart index.
The ID must be kept as supplied; recomputing a folder-based `pack::song_id` would
lose the source identity and collide between unrelated `chart-0` directories.

Per-chart `validation[]` contains `chartIndex`, `selector`, `endpoints`,
`maximumNoteErrorMs`, `nativeArrowHeightsPerSecond` and `unrepresentedEffects`.
Errors remain bounded to 9 ms, native velocity must be observed/positive, and
unsupported source presentation remains a limitation rather than being labelled
reproduced. Native selector IDs currently have the canonical `chart-N` shape,
N = 0–15. Built-in source modes remain 5, 7, 9A and 9B; this does not add new
extraction families or custom-key support.

The normal main library summary supplies titles, unknown-artist wording,
previews, chart ordering and note counts. This adapter explicitly honors SSC
`DisplayBpm::Single`, `Range` and `Random`: random/unknown becomes an empty menu BPM.
The current generic library summary otherwise derives SSC menu BPM from actual
timing, which would expose the synthetic 75 BPM of an exact-source fallback.
No main library behavior changes here. Source stars/unknown ratings, correction
factors and pickup previews remain the preparation stage's policies.

`evidence()` supplies the bound bytes read during admission, relative to the
original Ready root, including original receipts, normalized/origin records,
prepared inputs and references. Hosts can deduplicate them by hash. It is not the
entire acquisition/capture archive: keep the original source job directory when
full runtime reconstruction is required. Receipts bind trusted-producer output;
they do not authenticate a publisher, confer redistribution rights, or reexecute
Ruffle and independently prove its observations during every library import.

## Stable identities and revisions

Every hash is SHA-256 over UTF-8 compact JSON. Identity/revision hashing sorts
object keys recursively, preserves array order, and uses `serde_json` scalar
encoding. It does not depend on the host's optional object insertion-order
feature. The algorithm/schema strings below are part of the hash.

Source identity:

```json
{
  "schema": "ddi-danoni-source-identity/1",
  "source": {
    "movieUrl": "https://example.org/work/song.swf?mode=5",
    "flashVars": {"name": "num001"}
  }
}
```

Its digest is `927e6709e6f4ef15d7864d52926f9629db4f2bfcecff536d95d37bdb7d91a59f`.
The URL must already be canonical anonymous HTTP(S), without fragments or archive
wrappers. Original queries and FlashVars are significant. Scheme changes,
mirrors or differing query order are not guessed to be aliases. A page URL,
capture date, temporary path, title, codec, font and fitted BPM are not identities.

Song identity hashes `{schema:"ddi-danoni-song-identity/1",
sourceIdentitySha256, encodedAudioSha256}`; its library ID is `u-<full digest>`.
`encodedAudioSha256` is the native normalization receipt's original demuxed MP3
hash, before PCM alignment or playback transcoding. Charts grouped by canonical
playback must agree on that original encoded identity. Identical encoded music
under a different logical work/context remains a different song. Ambiguous
independent streams with the same identity stop; no title-based disambiguation
or silent merge is attempted.

A selector subset of the same music keeps its song ID. It changes the payload
revision rather than creating a new song. A source that replaces its actual
encoded music gets a new identity; alias/version migration is a separate explicit
host operation. This conservative policy does not deduplicate old ordinary
folder/ZIP imports by title or sound similarity.

Revision hashes `{schema:"ddi-danoni-song-revision/1", identity, entry, selectors,
validation, files, musicalQuality:"provisional", tempoStatus}`. Music encoding,
SSC/BPM/grid, title/banner, chart coverage, library metadata or validation changes
therefore produce a different revision. Retrieval dates, staging paths and fresh
native observation timestamps do not change an identical library payload.
The whole import hashes `{schema:"ddi-danoni-import-content/1",
sourceIdentitySha256, songs, musicalQuality:"provisional"}`, with songs sorted by
library ID. Exact provenance receipts remain separately available even when
payloads are unchanged; hosts should archive each source job independently.

## Planning duplicates

`import_host::plan(&import, &Snapshot, DuplicatePolicy)` returns an opaque `Plan`
and serializable `ddi-danoni-install-plan/1` summary. A snapshot is
`{revision, songs:[{id, sourceIdentitySha256, identitySha256, revisionSha256,
selectors}]}`. Its revision is a bounded opaque catalog token; rows describe this
adapter's managed songs and must cover every candidate ID already in the store.
Selectors are unique, increasing, nonempty integers from 0 through 15. Ordinary
bundled/legacy entries need not be converted into managed rows.

The default `reject` policy adds absent songs and skips identical revisions. A
different revision under the same song ID returns `library-import-conflict` before
any transaction starts. `keep-existing` preserves conflicting installed revisions;
`replace` plans their atomic replacement. Identity collisions or malformed
catalogs always fail. Even `replace` rejects a selector subset that would remove
installed charts (`library-chart-removal`); intentional chart deletion is a
separate host action, not a side effect of a partial reimport.

Plan summaries contain `importDigest`, `expectedCatalogRevision`, `duplicatePolicy`
and per-song `{id, decision, previousRevisionSha256, revisionSha256}`. Decisions
are `add`, `unchanged`, `keep-existing`, `replace`. `plan_digest` binds the complete
compact serialized plan contract for a host journal. A plan belongs to the exact
validated content digest; a serialized plan is not an installable proof. After a
restart, revalidate/replan or use the host's own durable recovery protocol.

## Host transaction contract

Implement `import_host::LibraryStore`:

- `check_revision(expected)` atomically checks an all-skipped plan without writes.
- `begin(plan)` checks the expected catalog revision and operation preconditions,
  returning its associated `Transaction<'a>`. In particular, an `add` must not
  overwrite an ID missing from an inaccurate snapshot.

Implement `Transaction`:

- `stage_evidence(import)` and `stage_song(song, operation)` stage verified bytes
  and metadata, invisible to library readers. Long host writes should poll their
  own cancellation service. Nothing should reopen an unchecked staging filename.
- `commit()` atomically rechecks the catalog revision (or keeps an exclusive
  writer lock throughout), then publishes every selected payload, provenance and
  catalog row as one operation. It returns `Committed {catalogRevision}`,
  `Rejected {error}` only when definitely unpublished, or
  `NeedsRecovery {transactionId, reason}` when publication/durability is uncertain.
- `rollback()` idempotently cleans only a definitely uncommitted transaction.
  Drop/crash recovery belongs to the host. After an uncertain commit, its Drop
  implementation must preserve the journal and data needed for recovery.

`import_host::install(import, plan, store, progress)` stages the whole change set.
Its checkpoints are `begin`, `evidence`, `songs`, `ready-to-commit`. A checkpoint or
stage failure rolls back; a failed rollback is `library-rollback-failed`, retaining
the original error and requiring host recovery. A rejected commit rolls back.
An uncertain commit returns `needs-recovery`, with no automatic rollback or retry.
A successful commit returns `installed` counts. An all-skipped plan checks the
catalog revision and returns `unchanged` without starting a transaction.

There is deliberately no fallible notification after commit: cancellation that
arrives after the atomic visibility point cannot relabel installed songs as a
failed/cancelled job. The caller reports the committed result. If commit itself
cannot determine whether publication occurred, it must use `NeedsRecovery`.
These interfaces do not make ordinary per-file copies atomic or prove a desktop
store's crash durability; its journal, locking, fsync/database transaction and
recovery implementation need separate platform acceptance tests.

A desktop job calls the same Rust APIs in sequence on its blocking job facility:

```text
url_prepare::execute → successful outer Ready
  → prepared_import::load_ready (or validate over host bytes)
  → host reads managed snapshot
  → import_host::plan
  → import_host::install through the host's transaction
  → committed / unchanged / needs recovery
```

Keep these native dependencies out of WASM. The existing URL/runtime macOS
confinement and Unix persistent-cache limits still apply. No storage or UI code
under `app/`, `library/`, `chart/` or the engine changes in this slice.

## Inspection and verification

The import-inspection command is read-only:

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- inspect-import /external/ready-directory
```

It validates and prints the handover summary; it does not install. Contract tests
construct synthetic charts/audio in memory and cover real library summaries,
identity goldens and partial selections, byte/receipt/path tampering, duplicate and
replacement policies, stale/no-op snapshots, invisible staging, multi-song
rollback, every cancellation checkpoint, failed cleanup and uncertain commit both
before and after publication. The optional `import_corpus` gate reads existing
Ready outputs, installs them only into an in-memory host and verifies duplicate
identity, library asset resolution and rejection of cancelled jobs.

Set `DDI_DANONI_IMPORT_CASES` to a local JSON document with `songs`, `charts` and
`cases:[{ready,expected}]`; expected is `installed`, `unchanged` or `not-ready`.
`DDI_DANONI_IMPORT_REPORT` optionally saves its receipt. No third-party work is a
repository fixture. Local real outputs and receipts stay under
`~/Music/DanOni/reviews/rust-import-handover-20261015/`. The accepted corpus/export
registry and original acquisition caches are not regenerated.


The 2026-10-15 local checkpoint admits Higan and No Name/灼冬赫夜 into one
in-memory catalog: two songs / three charts with distinct stable IDs and menu
BPMs 155/149 from the still-provisional fitted grids. A separately captured Higan
output has identical library content and plans no change. Both cancelled Ready
parents are refused. Per-chart diagnostics cover all 2,092 source endpoints and
preserve the existing presentation-omission warnings. Read-only CLI and direct
API summaries agree exactly.

The same frozen outputs, identities, revisions, library summaries and duplicate
decisions also agree with `serde_json/preserve_order` enabled in the host graph.
The six synthetic contract tests pass in that configuration too. Final checks
pass 224 native and 94 feature-disabled SDK tests, strict Clippy for both default
feature configurations, formatting and `UV_OFFLINE=1 just check` (843/843 REUSE).
The native suite includes the historical cache/page oracles and the local
prepared-import cases. Absent their environment variables, external corpus tests
skip execution. No new work was fetched or executed for this slice, no real
library was installed, and original accepted exports/registry/cache indexes retain
their previous hashes. Local `validation.json` binds sources, binaries, logs and
these receipts. Durable host-store/crash-recovery acceptance remains outstanding.


## Durable native adapter

The subsequent [managed-store slice](danoni-rust-managed-store.md) implements
these transaction traits with immutable payload/evidence directories, an atomic
catalog and explicit recovery. It adds installation/inspection/verification CLI
commands for a separately initialized external store. The in-memory checkpoint
above remains the earlier handover test; the new document describes durable-store
acceptance and the exact disk schema. Desktop shell integration is still owned by
the main implementation session.


The [native import-job coordinator](danoni-rust-import-job.md) now connects this
handover to URL preparation and a catalog store, with full-plan progress and a
hash-bound candidate token for restart. It preserves the same terminal commit/
recovery rules; source extraction is not repeated by resumed installation.
