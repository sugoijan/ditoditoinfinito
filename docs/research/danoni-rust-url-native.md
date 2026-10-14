# URL-to-native extraction bridge

This milestone 5 slice connects [HTTP acquisition](danoni-rust-http.md),
[static discovery](danoni-rust-acquisition.md), automatic native profiles and
[dependency recovery](danoni-rust-profile-recovery.md). It takes a public work
page or direct SWF URL and produces a normalized native work when the existing
profile, chart, motion and audio proofs all pass. It does not install songs,
approve musical estimates, generate final exports, crawl indexes or execute HTML5.

## Host API

`url_native::UrlRequest::new(url)` supplies defaults. Call
`url_native::execute(request, cache, worker, new_output, cancel, site, progress)`
on a blocking job thread. The host supplies the trusted installed `WorkerSpec`,
external cache/output paths, the existing `NetworkCancel`, a synchronous site
access/terms callback and a progress callback returning `Result<()>`.
For an async desktop shell, run this entry point using its blocking-job facility;
calling it within an entered Tokio runtime is rejected rather than nesting runtimes.

Calls sharing a cache must be queued: its exclusive writer lock is held for the
whole URL operation, including native extraction. The blocking coordinator owns
a current-thread Tokio runtime for acquisition.
While waiting, it polls host/dependency checkpoints every 50 ms. A rejected
checkpoint cancels and drains the HTTP future before returning, so a dispatched
request retains its durable failure record. The existing native supervisor still
kills and reaps cancelled workers. A complete cached response may be retained after
cancellation; it cannot turn a failed parent job into a ready work.

`UrlRequest` uses camelCase and rejects unknown fields:

- `url`: anonymous HTTP(S) work page or SWF, including an explicit Wayback snapshot.
- `movieUrl`: optional exact logical SWF URL to disambiguate a page.
- `encodingOverride`: optional recognized page encoding; no song-specific default.
- `selectors`: optional increasing unique menu row indices; otherwise every
  supported observed row is extracted.
- `metadata`: optional host presentation metadata, default empty.
- `fetch`: existing `FetchOptions`, including offline mode, resolver/trust choices,
  body/request limits and whole-operation `wallMs` (default 180,000, max 300,000).
- `dependencies`, `profile`, `capture`: existing strictly bounded limits, with
  their existing defaults and maxima. The parent also checks aggregate output
  against `dependencies.outputBytes`; this is cooperative polling, not an OS quota.

Progress contains `stage` (`acquisition`, `native`, `ready`), the source page/movie `url` and
optional inner `dependency` progress. The complete parent manifest is staged before the final `ready` callback; only
a successful final check permits its rename. Callback rejection or cancellation at the
parent `ready` checkpoint removes the pending manifest and leaves diagnostics and null `completed`, even if a child
native run completed. The outcome exposes `ready`, `output`, `manifest`, `failure`.
No source code, selector clicks, source counters or engine variables are supplied
by the page or caller through this interface.

## Discovery, identities and admission

A URL with a logical `.swf` path is acquired with the existing movie limit;
otherwise the initial response uses the 2 MiB HTML bound. A small extensionless
FWS/CWS response is recognized as a direct movie. Direct movies must have an
admitted FWS/CWS signature. HTML must be a complete HTTP 200 `text/html` response.
HTTP failures never become synthetic pages, DOS files or successful movies.

HTML uses the existing passive parser with the final response URL as its base.
It finds static object/embed and reviewed declarative router forms, preserving
FlashVars and optional declared pixel `viewport: [width, height]` (1–4096 each;
percentages, missing or invalid dimensions are unknown). Duplicate URL/FlashVars
contexts are collapsed. A unique largest viewport is a candidate hint only when
all contexts have known sizes and different logical URLs: it must be at least as
wide as tall, with every smaller sibling narrower than tall and sharing its height.
This handles a broad game beside a narrow Flash side panel without guessing from
file names. Ties, unknown sizes, several broad game viewports, and conflicting
FlashVars on the same URL still require explicit selection or report ambiguity.
The candidate must pass the unchanged native engine fingerprint/runtime proofs;
viewport size never proves engine identity or hidden-chart completeness. Navigation links and scripts are retained
in discovery evidence but are not followed or executed. Referrer-gated works are
not unlocked by spoofing headers: live acquisition remains anonymous.

Runtime `sourceUrl` is the selected logical movie URL, including its query;
`pageUrl` is the final page's logical URL. Actual requested/final retrieval URLs,
HTTP redirects and archive captures remain in acquisition evidence. Dependencies
are admitted only from the existing source-bound current-worker GET witnesses.
Their logical URLs are unchanged in the worker context; archive retrieval inherits
the movie response's actual final snapshot (retained as `dependencyRetrievalBase`), with no guessed capture or live fallback.

`Fetcher::fetch_checked` adds a per-resource-hop identity constraint before both
cache hits and new requests. The archive guard therefore refuses a redirect to a
live URL even if the redirect itself is cached. This guard cannot grant access:
site terms, robots, DNS/public-address checks and all ordinary transport limits
still apply to new GETs. Cached historical responses require no new network policy
or GET. Body-only archive saves without complete HTTP receipts are not silently
promoted to cache hits by this bridge.

The initial context contains the selected movie/FlashVars, with no assumed DOS
paths. The native dependency coordinator observes required URLs and obtains them
through the same Fetcher. Only HTTP 200, bounded, hash-verified responses with
recognized charsets become worker resources. Exact optional-colour 404 reviews
are not guessed or generated; works needing one currently stop explicitly.
Unsupported engine identities also stop, with acquired evidence preserved.

## Self-contained output and schemas

```text
new-output/
  acquisition/
    cache/r-<hash>.json              immutable response capsule copies
    cache/blobs/...                 exact response bodies and bound policy bodies
    http/<sha256(requested URL)>.json
    discovery.json                  when the input was HTML
    context.json                    source-only native context
    source-manifest.json
    dependencies/<sha256(logical URL)>.json
  native/                           existing profile/dependency-run layout
    seed/
    attempts/NNN/                   complete independently proved attempts
    dependencies/
    manifest.json or failure.json
  manifest.json or failure.json     outer readiness, written last
```

Cache body paths are copied as recorded, so historical layouts need not be exactly
`blobs/sha256/`. All reads are contained and digest-checked. `Fetcher` now implements
`AssetSource`, sharing its already verified body pool rather than reopening body
files. Staged input handles share immutable bytes with a 4096-file / 512 MiB bound.
The confined worker receives staged resources and still has no live network.

Each `http/*.json` is the unchanged `ddi-danoni-http-resource/1` receipt. The bridge
copies its body/index bindings and any nested bound robots-chain evidence into
`acquisition/cache/`, preserving their exact bytes and hashes. Historical metadata
without bound policy bodies stays explicitly historical metadata; missing evidence
is not synthesized. Later mutable cache updates cannot invalidate these copies.

`source-manifest.json` is `ddi-danoni-url-source/1`: `sourceSwfSha256`, original
`source` address, selected `movie` (including FlashVars/viewport), dependency retrieval
base, recorded selection policy, acquisition receipt
bindings and relative roots. Dependency provenance remains the existing
`ddi-danoni-dependency-response/1`: exact logical/retrieval URLs, original retrieval
date, status, body hash and `policyDecision` pointing to a bound full HTTP receipt.
`url:<path>#sha256=<digest>` resolves in the outer `url` namespace.

The outer schema is `ddi-danoni-url-native-run/1`:

- `ready`, original `request`, `failure` (null on success).
- `roots`: `url: acquisition`, `cache: acquisition/cache`, relative to this root.
- `acquisitions`: ordered HTTP-receipt bindings, including failed HTTP responses
  when available; their namespace is `url`.
- `inputs`: complete immutable acquisition inventory as namespace/path/SHA bindings.
- `transfers`: actual Fetcher wire/decoded bytes and new GET attempts, including
  robots/redirects; all zero for a fully cached run.
- `completed`: null on failure; otherwise a bound `native/manifest.json` receipt
  and `normalized` descriptor. Both normalized paths are relative to the outer
  root, prefixed `native/`; their existing hashes and chart count are preserved.

Normalized data/units and child replay formats are unchanged. A child ready marker
is not the parent completion marker. Existing outputs are never overwritten.
Failures after acquisition begins retain evidence and `failure.json`; invalid
configuration and pre-cancellation fail before creating output.

## CLI

```json
{
  "schema": "ddi-danoni-url-native-job/1",
  "cache": "/outside/repo/danoni-cache",
  "request": { "url": "https://reviewed.example/work/song.html" },
  "origins": { "https://reviewed.example": "Actual host-reviewed public access/terms decision" }
}
```

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- extract-url-native \
  /trusted/installed-bundle /outside/repo/job.json /outside/repo/new-output
```

The bundle is verified before execution. Relative cache paths resolve beside the
job. A cached-only job can set `request.fetch.offline: true` and use an empty
`origins` map; it cannot fetch missing files. The CLI prints a successful outer
manifest or the existing JSON error/nonzero exit, with diagnostics in the new
output. Its inputs are a URL plus host configuration, not per-song capture recipes.

## Verification and next work

Original fixtures cover unique/ambiguous static selection, FlashVars, shared and
self-contained HTTP/policy evidence, stable bindings after cache changes,
viewport selection/refusal, runtime nesting refusal, checkpoint-driven cancellation
with durable failed attempts, cached archive-to-live
redirect refusal, pre-cancellation and retained failure without worker execution.
Real native runs use separately copied cache indexes and immutable local bodies,
never writers on the original caches, and fetch no third-party resource again.
Evidence is under `~/Music/DanOni/reviews/rust-url-native-20261014/`:

- Higan's page selects its 400×350 game beside the 125×350 Flash panel, recovers
  DOS and colour data, then yields one chart: 562 taps / 9 holds. Lanes, speed,
  boost, stops, initial pixel speed, star rating, audio timing fields and WAV hash
  exactly match the earlier pinned-family extraction. The automatic family also
  retains all 70 original colour triples in `nativePresentation.iro_dac`, explicitly
  flagged `unmapped-native-presentation`; it does **not** map them to the pinned
  adapter's colour events. Generic colour-event interpretation is a known gap,
  not covered by the gameplay/PCM equality claim.
- No Name's page selects the game beside its Flash panel, discovers DOS/colour
  requests, recovers both, then yields both observed 5-key charts. The existing
  independent frozen-profile gate passes 1,500 taps / 6 holds / 1,512 real game
  parser endpoints and 12 x1/x2/x3 scroll checks. Maximum timestamp residual is
  `2.8421709430404007e-11` ms; independently decoded PCM differs by at most one
  signed-16 unit. The parity re-normalization supplies frozen **presentation
  metadata only** for comparing its old title/artist fields; the URL run itself
  uses URL plus generic resource/time limits, with no per-song metadata, click
  recipe, note edits, audio offset or estimator parameters.
- A real one-chart No Name run cancelled at the parent ready callback retains a
  successful `native/manifest.json`, but has only an outer `failure.json`, null
  `completed` and zero network requests. Its complete acquisition inventory is
  rehashed in the SDK test. A child success never authorizes installation.

A direct No Name SWF URL with the host selector filter `[0]` also succeeds: its
normalized chart exactly matches the first chart from page discovery, with no
HTML/discovery file required. All acquisition inventory hashes are rechecked and
the pending parent manifest is absent after successful publication.

Both page-URL runs and the direct-SWF trial use cached acquisition exclusively. The larger wall allowances in
these debug validation jobs are generic operation budgets, not chart/music tuning.
Output-tree checks are polled at 50 ms and forced at readiness, matching the
existing native coordinator rather than walking the directory on every inner
normalization checkpoint.

Next connect normalized output to the existing audio/tempo/artwork/song preparation
APIs and prove URL → prepared songs, then add reviewed frame/index routes, broader
native profiles and explicit optional-resource reviews. HTML5 browser control,
archive body-only admission, arbitrary script navigation and desktop installation
remain separate work. Accepted exports and registry are not regenerated here.

Final checks pass 211 native and 85 pure SDK tests, strict Clippy for both feature
configurations, formatting and `UV_OFFLINE=1 just check` (833/833 REUSE files).
The native suite includes the existing 1,629 cache/robots and 91 discovery cases.
The eight original bridge tests and real-worker late-cancellation test were
rerun after the final wrapper changes, including staged-manifest removal. The
original two cache indexes, export registry and accepted export index retain their
previous hashes. No third-party source, audio or downloaded response is committed.
