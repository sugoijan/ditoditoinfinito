# Native DanOni managed store

The `managed_store` adapter implements the host transaction contract in
[danoni-rust-import.md](danoni-rust-import.md). It is a native SDK component,
behind the `native` feature, with no application-shell changes. Admission still
requires an opaque `PreparedImport`: disk summaries are data, never permission to
install arbitrary files. The main-owned desktop shell can use this adapter or
implement the same contract with its own database.

## API and commands

Run these on a blocking job thread. A `Store` holds an exclusive, nonblocking
filesystem lock for its entire lifetime; a second handle returns
`library-store-busy`. The shell should own/serialize access to its handle and
pass cloned catalog metadata to its UI. Release the handle before another process
uses the CLI.

```rust,ignore
let prepared = prepared_import::load_ready(ready_root, &mut admission_progress)?;
let mut store = managed_store::Store::open(store_root)?;
let catalog = store.catalog()?;
let plan = import_host::plan(&prepared, &catalog.snapshot(), duplicate_policy)?;
let result = import_host::install(&prepared, &plan, &mut store, &mut control)?;
```

Create a fresh store explicitly with `Store::create`. It never adopts, empties or
reinitializes an existing directory. Its parent must exist outside Git checkouts.
`Store::open` requires the format marker, catalog, transaction directory and
regular lock file. Initialization interrupted before completion fails closed;
there is no automatic repair/adoption of a partially initialized directory.

`catalog()` lists metadata without loading music or archived PCM. `verify()`
audits the current catalog and the complete inventoried contents of its referenced
transactions, including provenance. `read_asset(&song, filename, budget)` accepts
a catalog song and a flat playback filename, checks its receipt, bounds its size
and checks its exact bytes against the saved SHA-256. Cloned older song rows
remain readable after replacement because payloads are immutable and retained.

With `BIN=target/danoni-rust/debug/ddi-danoni-export`:

```sh
$BIN create-store /external/new-library
$BIN install-import /external/ready-work /external/new-library
$BIN inspect-store /external/new-library
$BIN verify-store /external/new-library
$BIN recover-store /external/new-library
$BIN install-import /external/new-ready-work /external/new-library replace
```

`install-import` defaults to `reject`; its optional last argument is `reject`,
`keep-existing` or `replace`. No-op imports create no transaction. Replacement
cannot remove installed selectors, even when explicitly requested. The CLI is a
thin wrapper around the SDK admission, planning and transaction functions. It
does not fetch/extract new works, change accepted exports or import into the web
app's IndexedDB library. Acquisition/preparation and store installation remain
separate calls so the host can show the proposed duplicate decisions first.

## On-disk contract, version 1

The store's JSON metadata uses camelCase unless inherited from the existing
`ManifestEntry` serialization; copied evidence keeps its source schemas. JSON is
UTF-8; paths use forward slashes relative
to their stated root. Hashes are lowercase 64-character SHA-256 hex. A transaction
ID is a fresh 64-character hex token, not a song identity or content hash. Song
IDs retain `u-<full identity hash>` from the admission contract.

```text
store/
  format                         exact bytes: ddi-danoni-managed-store/1\n
  writer.lock                    persistent regular file; never unlinked
  catalog.json                   sole authoritative visible catalog
  pending.json                   only while a transaction needs completion
  pending.new                    reserved journal scratch, possibly partial
  catalog.new                    reserved publication scratch, possibly partial
  transactions/<transaction>/
    plan.json                    exact host PlanSummary
    import.json                  admitted prepared-import Summary
    evidence/<ready-relative>    bound bytes consumed during admission
    songs/<u-identity>/
      summary.json               admitted SongSummary, including diagnostics
      song.ssc
      music.ogg | music.wav
      banner.png                 if present in admitted inventory
    catalog.json                 complete candidate catalog, including old rows
    receipt.json                 durable transaction inventory and catalog hash
```

Only `catalog.json` determines what is installed. A transaction folder or its
`receipt.json` alone never implies installation. Aborted/unfinished transaction
folders are retained and invisible. Existing generations and original source job
directories are neither copied wholesale nor deleted. `evidence/` contains the
admission proof closure, including supplied preparation inputs, not necessarily
the complete acquisition/runtime archive. Keep that original archive separately
when the acquisition/extraction audit is required.

`catalog.json`:

```text
{
  schema: "ddi-danoni-managed-catalog/1",
  revision: "empty" | transaction ID,
  songs: [{
    installed: {
      id, sourceIdentitySha256, identitySha256, revisionSha256,
      selectors: [integer selector, ...]
    },
    entry: ManifestEntry,
    transaction: payload transaction ID,
    files: [{path: flat playback filename, sha256, bytes: integer}, ...]
  }, ...]
}
```

The initial catalog is `empty` with no songs. Committed rows are sorted by song
ID; selector arrays are nonempty, sorted, unique and in 0–15. Playback entry paths
are song-local; resolve them through the row's transaction and song ID, not
relative to the store root. The entry is the same `ddi-library` structure the
host already consumes. Its chart note counts, previews and display BPMs retain
the admitted values. Persisted `summary.json` retains source endpoint-validation errors and
unrepresented effects; `import.json` retains provisional musical quality and
tempo status. Installing does not promote provisional fits to reviewed musical
metadata.

`pending.json`:

```text
{
  schema: "ddi-danoni-managed-pending/1",
  transaction: target transaction ID,
  previousRevision: old catalog revision,
  previousCatalogSha256: SHA-256 of old catalog bytes
}
```

`receipt.json`:

```text
{
  schema: "ddi-danoni-managed-transaction/1",
  transaction: this transaction ID,
  importDigest: admitted import digest,
  catalogSha256: SHA-256 of transaction/catalog.json,
  files: [{path: transaction-relative path, sha256, bytes}, ...]
}
```

The receipt inventory covers the plan, import summary, all evidence and all song
files/summaries. Its catalog is bound separately to avoid self-reference; the
receipt itself is not in its own inventory. Metadata documents are limited to
16 MiB. Catalogs have at most 100,000 entries; playback files are flat,
case-unambiguous, at most 128 MiB each, with at most 16 per song. Receipt
inventories have at most 32,768 entries, each at most 512 MiB; admission imposes
its own tighter bounds before writing. Unknown fields in the store's own
metadata types are refused. Existing library entry types keep their own rules.

## Publication and recovery

1. Hold the writer lock. Check the actual catalog revision and every operation's
   previous revision/existence. A stale or incomplete caller snapshot cannot let
   an Add overwrite an installed ID.
2. Create a new transaction directory. Durably write `pending.new`, rename it to
   `pending.json`, and sync the store directory before staging anything visible.
3. Write evidence, songs and metadata into that transaction's final immutable
   paths, using exclusive creation. Sync each file; sync all created directories
   before publication. The candidate catalog contains all prior unaffected rows
   and all changed rows. It cannot be prepared with missing songs/evidence.
4. Write and sync the transaction catalog and receipt. Write and sync an exact
   copy as `catalog.new`, then sync the store directory.
5. Rename `catalog.new` over `catalog.json`: the single visibility point. Sync
   the store directory, remove `pending.json`, and sync the directory again.

Before publication begins, failures are known uncommitted and the coordinator
rolls back. Drop also attempts rollback before publication. Rollback retires only
the recovery markers after checking the old state; staged content is retained.
An error after the publication attempt starts returns `NeedsRecovery` and never
triggers rollback, automatic retry or a later cancellation callback.

Planning/listing/writing refuses outstanding recovery markers. With the writer
lock held, `recover()` reads the visible catalog and journal:

- Exact old revision **and** old catalog hash: verify the old installed state,
  retire the markers, return `Aborted`.
- Target revision: require the exact receipted candidate and verify all referenced
  inventories, stabilize the catalog directory, retire the markers, return
  `Committed`.
- Any other relationship, missing committed content, corrupt inventory or unsafe
  path: return an error and preserve the recovery record.

Without a published journal, recovery verifies the visible catalog and can retire
only the reserved scratch files, returning `Clean`. This covers interruption
before journal publication or after journal deletion. An uncertain commit may
therefore resolve as `Clean` with its target catalog revision; the caller must
compare that revision with its returned transaction ID. Recovery is idempotent.
It never deletes song/evidence directories, including abandoned attempts. Disk
quotas, retention and explicit garbage collection are follow-up work; do not
remove an old transaction while a reader may hold one of its catalog rows.

## Durability scope and acceptance

This first adapter requires Unix. It uses same-directory atomic rename, regular
file `sync_all`, directory `sync_all` and a held filesystem lock. Tests run on the
maintainer's macOS filesystem. Local filesystems must honor these operations;
network filesystems, external modification of the store, Windows publication
semantics and true sudden power loss are not certified. This is a private store
with cooperative writers, not protection against a hostile local process racing
path validation. Symlinks and traversal paths are refused; hash checking detects
payload corruption but does not authenticate publishers or authorize execution.

The fault matrix injects errors before and after publication; child-process tests
exit without Rust destructors at eight transaction boundaries during a multi-song
replacement. Reopening/recovery must expose every old row or every new row,
verify assets and retain old snapshot reads. Additional tests cover cancellation
after the first song, dropped staging, lock contention, no-op/stale/conflicting
imports, selector shrink, partial journal scratch, unknown stores, corrupt
published content and symlinks. These are protocol/process-crash tests, not a
power-cut experiment.

For optional real-work acceptance, set `DDI_DANONI_IMPORT_CASES` to the earlier
handover case list and `DDI_DANONI_MANAGED_STORE` to a **nonexistent** external
store directory, then run `cargo test ... --test managed_corpus`.
`DDI_DANONI_MANAGED_REPORT` optionally saves the report. The test reopens the store
after each import, verifies every referenced transaction and playback file,
checks expected duplicates and rejects failed/cancelled Ready roots before
installation. It leaves the fresh store for inspection. With no managed-store
environment variable, that external test does not execute. Do not point it at an
existing app library; exclusive fresh creation refuses that path.


The local 2026-10-15 checkpoint under
`~/Music/DanOni/reviews/rust-managed-store-20261015/` contains separate fresh API
and CLI stores. Both admit Higan and No Name/灼冬赫夜 as two songs / three charts,
reopen and verify every referenced inventory and playback asset, recognize an
independently captured Higan as unchanged, and reject both cancelled parents.
The CLI's nine command invocations cover creation, install, inspection, full
verification and recovery. API/CLI installed identities, revisions, entries and
playback inventories agree exactly; their fresh catalog/transaction tokens
intentionally differ. Menu BPMs remain the admitted provisional 155/149. Local
reports retain exact paths and hashes; no third-party data enters the repository.


Final validation passes 234 native / 94 feature-disabled SDK tests, strict Clippy
for both configurations, formatting and `UV_OFFLINE=1 just check` (847/847 REUSE).
The native suite includes the local managed-store, prepared-import, historical
cache and page-discovery oracles. No new source work is fetched or executed; the
accepted export/registry/cache indexes keep their pre-test hashes. Local
`validation.json` binds source files, lockfiles, the native CLI, reports and logs.
