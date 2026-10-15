# Reviewed native index and collection imports

Research/implementation date: 2026-10-15. This extends the
[native import-job coordinator](danoni-rust-import-job.md) with static list
inspection and sequential per-work imports. It uses the already cached sources
in the [publication-route survey](danoni-indexes.md); it adds no general crawler,
JavaScript execution, browser navigation or new native engine family. Main-owned
library/shell code and the accepted exports remain separate.

## Entry points and reviewed routes

`collection::inspect(url, html, encoding_hint, maximum_entries)` is passive and
fetches nothing. It returns an informational `Inventory` selected by a reviewed
HTML row structure:

| Route identifier | Exact URL pattern | Selection evidence |
| --- | --- | --- |
| `autumnleaves-flash/1` | `autumnleaves.yukishigure.com/danoni.html` | Six-cell work rows with a `No` number; second cell only |
| `cw7-catalogue/1` | `cw7.sakura.ne.jp/lst/dsNNNN/` | Work rows whose first cell links to the same author's `/dtl/dsNNNN/`; second cell only |
| `7fes06-works/1` | `cw7.sakura.ne.jp/event/7fes06/works.html` | Repeated `div id=music` work blocks, excluding the heading |
| `iso-flash/1` | `eucaly.net/~iso50/flash.html` | Six-cell numbered rows, including full-width digits; second cell only |

HTTP/HTTPS and explicitly supplied supported Wayback snapshots are accepted.
CW key filters are absent or exactly `5`, `7`, `9A`, `9B`; unsupported
filters fail. A missing filter does not admit unsupported charts: each child's
real engine/profile/layout and chart admission still determine compatibility.
Composer, advertising, homepage, detail and navigation links outside the selected
cells are not child works. The adapters are deliberately narrow: layout drift
fails rather than enabling generic anchor crawling. They are structural evidence
parsers, not a browser DOM implementation.

`collection::discover` fetches one reviewed index through the existing
[bounded acquisition layer](danoni-rust-acquisition.md), preserving raw HTML,
HTTP/cache evidence and its replayed inventory. Redirects must stay on that
reviewed index path; archive inputs cannot silently redirect to the live site.
`load_discovery` checks all hash-bound inputs and reconstructs the inventory
from the exact HTML before yielding an opaque `Discovery`. An inventory JSON or
collection result alone cannot authorize execution. The saved evidence proves
what was retrieved, not current permission to access its child sites.

`collection::execute` accepts an index URL in `Request`, discovers it and calls
`import_job::execute` for selected native candidates, sequentially.
`execute_discovered` accepts an opaque discovery for a fresh attempt without
fetching the list again. The host owns a blocking job thread, trusted worker
bundle/font sources, cache, site decisions, cancellation token and target store.
There is no hidden thread or browser. The WASM/main workspace remains unchanged.
A desktop host can start with `Request::new(url)` and supply its usual services:

```rust
let request = collection::Request::new(url);
let result = collection::execute(
    &request, &services, &staging, &mut store, &cancel,
    &mut site_decision, &mut progress,
)?;
```

`Discovery::request()` exposes the saved URL/encoding/entry bound when preparing
a new attempt from an existing discovery; changing execution limits or duplicate
policy does not require downloading the index again.

## Inventory and attempt contract

All JSON field names are camelCase. Versioned schemas:

- `ddi-danoni-index-inventory/1`: `route`, `source` (logical/retrieval URL and
  capture), `bodySha256`, `encoding`, optional `title`, `entries`, `complete`,
  `omitted`, `diagnostics`, `hasInlineScript`.
- Each entry: `id`, `kind`, optional `address`, `label`, `occurrences`, optional
  `reason`. `occurrences` retain zero-based structural row/column, label and raw
  href. Labels normalize whitespace and are bounded to 512 characters. Entry
  IDs bind the logical index and exact destination; they are not library song
  IDs. Exact retrieval URLs deduplicate while retaining occurrences. Semantic
  query strings (notably `id`/`scoreId`), different captures and schemes remain
  distinct. Missing links do not produce guessed destinations.
- `kind` is `candidate`, `unavailable`, `navigation-required` or
  `unsupported-address`. A candidate is a destination, not a promise that the
  native worker supports it. Iso's recorded referrer guard makes its links
  `navigation-required`; these are never automatically fetched or given a
  fabricated referrer. Unsupported archive modifiers and non-HTTP links stay
  explicit. No login, bypass, automatic archive search or live fallback occurs.
- `ddi-danoni-index-discovery-run/1`: `ready`, typed `request`, bindings `http`,
  `body`, `inventory`, complete `inputs` and actual `transfers` (requests and
  wire/decoded bytes). `failure.json` is diagnostic; only `manifest.json` with
  replayable Ready evidence constructs a discovery.
- `ddi-danoni-collection-result/1`: `request`, absolute `directory`,
  `discoveryDirectory`, `discoverySha256`, `complete`, `entries`, `transfers`,
  `accountingComplete`, optional `stop` and `reportError`. Each entry records
  inventory index/ID/label/destination and a `result` with `status`:
  `not-selected`, `not-attempted`, `skipped` with reason, `failed` with error,
  or `completed` with the unchanged single-work `Outcome`. Inspect its nested
  installation status (`installed`, `unchanged`, `needs-recovery`).

Layout for `execute`:

```text
NEW_OUTPUT/
  manifest.json                 collection status checkpoint (not library proof)
  index/
    manifest.json               Ready discovery
    inventory.json
    http/<hash>.json            HTTP resource evidence
    cache/...                  copied hash-bound cache bodies/indexes/robots
  work-0000/                    only attempted candidate entries get a directory
    manifest.json               Ready source+prepared candidate, or failure.json
    import-candidate.json       when preparation completed; independent resume
    source/...
    prepared/...
  work-0004/...
```

`execute_discovered` writes only the attempt directory and child folders; its
result references the separate discovery directory and exact manifest hash.
Retain both for a self-contained audit. No source scripts run during proof replay.
No index label, numeric row, catalogue difficulty or collection author overrides
song metadata. Source star extraction, musical fitting, previews and artwork
continue to use the existing per-work pipeline. Supported work layouts stay
5/7/9A/9B; a link's lane-count text is not a layout definition.

## Bounds, cancellation and partial results

`Request` contains `index: IndexRequest`, `work: import_job::Request` (the child
template), optional `selected` inventory indices, `maximumWorks`, `wallMs` and
`outputBytes`. `Request::new(index_url)` supplies defaults. A selected list must
be nonempty, unique and in range. Null selects all discovered entries. Iteration
uses inventory order. Unavailable/browser-required/unsupported entries are
reported without consuming a child attempt. The child template's URL is replaced
with the exact discovered retrieval URL; a global `movieUrl` override is rejected.
A common chart-selector list applies to every attempted work, so leave it null
unless every selected work is known to have that selector.

Default limits: 512 distinct inventory entries (maximum 1024), 16 child attempts
(maximum 64), 15 minutes whole-attempt wall time (maximum one hour), 2 GiB attempt
output (maximum 4 GiB). Existing HTML limits apply: 2 MiB page, 2048 links/rows,
32768 tags, depth 64, 64 cells per row, 16384 bytes per cell and 4096 per anchor.
Entry truncation is explicit (`complete=false`); omitted counts are omitted
occurrences after the distinct-entry bound, not a guarantee of catalogue size.
MFV2's cached 5.4 MB `ALL_List.php` remains outside the 2 MiB page bound; it
needs a separately reviewed route/pagination strategy.

`index.fetch` is the shared network ceiling, including index requests for a full
execution. Child budgets are clipped to the remaining requests/wire/decoded
bytes and remaining wall/output budget, as well as their existing smaller
per-work bounds. Shared transport settings (including DNS/TLS) apply to children;
robots freshness uses the stricter bound. Exhausting any network budget forces
later children offline; cached works can still succeed. `offline=true` on the
index request forces all children offline. Sequential fetchers release the cache
writer lock between stages; the existing cache persists host cooldown timestamps,
so a new child does not reset pacing. Site decisions, robots checks, redirects,
Vary/context matching and no automatic GET retry remain in the fetcher.

Each child source success/failure receipt accounts for its actual transfers.
A missing, ambiguous, mismatched or over-budget receipt stops later work with
`accountingComplete=false`; unknown usage is never silently counted as zero.
The current child was bounded by the remaining allowance. Reused discovery has
its own historical counters; a fresh saved-discovery attempt starts at zero.
Output/deadline checks are cooperative at existing progress boundaries, not an
OS disk quota; the library's retained payload/history is outside attempt output.

Installation is atomic **per work**. A failed independent work can leave a
partial result and continue; cancellation between works preserves prior installed
children. A maximum-work/output/time stop leaves remaining entries `not-attempted`.
Any `needs-recovery` outcome or rollback/recovery error stops the collection;
there is no automatic retry or whole-collection rollback. A late cancellation
cannot relabel the last committed selected child. `complete` means all selected
entries completed without uncertainty, discovery truncation or a stop; it does
not claim every possible work at that publisher was discovered.

The parent status file is a convenience checkpoint, not a transaction journal.
After any child might publish, the SDK returns its actual outcome even if saving
the parent report fails (`reportError`); a failed report stops later work. No
fallible final callback follows completion. After a process crash, inspect/recover
the managed store and resume individual hash-bound `import-candidate.json` tokens
as documented by the single-work coordinator. Do not infer a library rollback
from an older parent status file. Automatic crash resume for a whole collection,
garbage collection and UI scheduling are separate follow-ups.

## CLI

Build the standalone crate as in its [README](../../tools/danoni-dump/rust/README.md).
Commands use the same SDK:

```sh
ddi-danoni-export discover-index INDEX_JOB.json NEW_DISCOVERY
ddi-danoni-export create-store NEW_STORE
ddi-danoni-export import-index-native BUNDLE COLLECTION_JOB.json NEW_OUTPUT STORE
ddi-danoni-export import-discovered-native BUNDLE COLLECTION_JOB.json DISCOVERY NEW_OUTPUT STORE
ddi-danoni-export verify-store STORE
```

An index job needs no worker or library:

```json
{
  "schema": "ddi-danoni-index-job/1",
  "cache": "/absolute/local/cache",
  "origins": {},
  "request": {
    "url": "https://autumnleaves.yukishigure.com/danoni.html",
    "maximumEntries": 512,
    "fetch": {"offline": true}
  }
}
```

A collection job has schema `ddi-danoni-collection-job/1`, the same `cache` and
`origins`, optional asset `roots`, and a `request` with `index` as above,
`work` in the existing [import-job request](danoni-rust-import-job.md) format,
optional `selected`, and optional bounds. For a live request the host must supply
an explicit reviewed public-access decision per origin; an empty `origins` map
is suitable only for offline replay. Relative configuration roots/cache resolve
against the job file, not process cwd. The trusted worker bundle must already be
installed/verified; no downloaded pack supplies executables or service settings.

Successful CLI JSON output can describe a partial collection. Inspect `complete`,
per-entry status, `stop`, `accountingComplete` and `reportError`; process success
alone does not mean all songs installed. All source data, captures, candidates,
cache and library directories remain local outside the repository.


## Local validation

All inputs remain under `~/Music/DanOni/reviews/rust-collections-20261015/`.
Four frozen cached lists are independently enumerated with Python's HTMLParser
(using web-compatible CP932 for legacy Japanese pages), then compared in exact
order with Rust's retained labels and retrieval URLs. This executes no publisher
scripts and adds no corpus fixtures to Git. The frozen snapshots contain:

| Snapshot | Distinct entries | Disposition |
| --- | --- | --- |
| AutumnLeaves | 116 | Native candidates; one duplicate anchor retains two occurrences |
| CW `ds0301/?keys=5` | 272 | Candidates from 311 work rows; exact URL duplicates merge |
| 2006 festival | 31 | Candidates from 29 music blocks; multiple links remain explicit |
| iso | 37 | 34 navigation-required links and three unavailable rows |

These are sample-specific counts, not estimates of ecosystem prevalence or
current site availability. The earlier rough AutumnLeaves row count included its
six-cell heading; `No` without a number is not a work. Empty synthetic numbered
work rows are still retained as unavailable. Wave-dash differences in Python's
strict `shift_jis` codec are not Rust parser differences: web Shift_JIS uses the
Windows mapping, which the independent oracle now matches explicitly.

Run the optional read-only comparison with:

```sh
DDI_DANONI_COLLECTION_REVIEW="$HOME/Music/DanOni/reviews/rust-collections-20261015" \
  cargo test --locked --offline --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --test collections_corpus -- --nocapture
```


The full offline URL test imports Higan from the actual AutumnLeaves index into
a fresh managed store, then retains that success when the exact discovered HTTPS
No Name URL has no cached response. Its existing cached work was retrieved over
HTTP; the tool does not invent a redirect or silently substitute schemes. The
collection reports `complete=false`, `accountingComplete=true` and zero requests,
wire bytes and decoded transfer bytes. Higan's song identity, revision, library
entry and all three playback file hashes match the earlier frozen import. The
known work remains independently usable through its explicit HTTP URL.

A second attempt reuses the bound discovery, selects Higan alone, and recognizes
the installed revision as unchanged. It reports complete selected scope, zero
transfers and an unchanged catalog, with no index fetch. Both attempts use the
trusted confined worker; the parent status is never treated as installation
proof. The original accepted export index, registry and both source cache indexes
remain byte-identical.

Validation passes **263 native / 94 feature-disabled tests**, strict Clippy in
both configurations, formatting and `just check` (**859/859 REUSE**). Nineteen
new synthetic tests exercise structure/capture/query identity, exact proof replay,
bounded selection, transfer debiting, independent failure, cancellation, host
callback failure, navigation exclusion, uncertain commit and post-commit report
errors; a separate local gate checks all four frozen index inventories. Local
`validation.json`, `oracle.json`, discovery closures and per-work receipts retain
the commands, exact files and scope. No third-party sources or generated packs
are committed.
