# Rust native dependency closure

Implemented 2026-10-14 in `tools/danoni-dump/rust/src/dependencies.rs`, behind
`native`. This adds a bounded coordinator above the [explicit selector
orchestrator](danoni-rust-orchestration.md). It discovers missing cached GETs,
asks a **host-supplied resolver** for policy-approved bytes, appends immutable
resource declarations and recaptures in fresh directories. The worker remains
network-disabled. A second [automatic-profile adapter](danoni-rust-profile-recovery.md)
now uses the same coordinator to recover inputs before menu/selector discovery.
Neither API fetches a public URL or installs songs; the final result is a proved
normalized work for the existing preparation pipeline.

## Trusted request evidence

The [worker navigator](../../../tools/danoni-dump/ruffle/src/navigator.rs) now
adds a structured `request` to `fetch-denied` entries:

```json
{
  "iteration": 20,
  "kind": "fetch-denied",
  "detail": "dos.txt?name=num301",
  "truncated": false,
  "request": {
    "schema": "ddi-native-request/1",
    "method": "GET",
    "hasBody": false,
    "hasHeaders": false,
    "originalUrl": "dos.txt?name=num301",
    "resolvedUrl": "https://example.test/work/dos.txt?name=num301",
    "reason": "cache-miss"
  }
}
```

`iteration` is the worker iteration, not a source note frame. `originalUrl` is
exactly what Ruffle requested; `resolvedUrl` uses its URL resolver against the
original logical movie URL, retaining the query. The archive retrieval URL does
not replace this base. No headers or body contents are logged. Reasons are
`cache-miss`, `method-denied`, `body-denied`, `headers-denied`, `invalid-url` and
`unsupported-url`; an unresolved URL is JSON null. Cached responses remain
exact URL matches. Requests carrying headers are now refused even if a cached
URL exists, so no header-dependent representation is silently substituted.

The audit remains `ddi-ruffle-probe/experimental-1`; this is an additive entry
field, identified by its own schema and the full worker build identity. Each
original/resolved URL is bounded to 1,024 Unicode characters; truncation of either
marks the audit incomplete. The existing 256-entry limit remains. The worker
still denies navigation, sockets, process launch and direct filesystem access.
Trace/runtime-error text cannot create a structured request.

The coordinator accepts only complete records from the **current reviewed
worker** for body/header-free anonymous HTTP(S) GET cache misses. It recomputes
URL resolution, checks the original detail, frame bound, resource declarations,
audit truncation and error counts. A URL already supplied in that chart's config
is a no-progress error, not another fetch. Credentials, fragments and unsupported
schemes fail. Navigation/socket/trace/unknown entries and denied methods/bodies/
headers fail instead of requesting resources. Runtime warnings/errors are
retained, never used as requests; all original normalization audit checks must
still pass after resource closure, including exact optional-colour exceptions.
A malformed, truncated, halted or failed transport never authorizes acquisition.

`inspect_requests(&Value)` is a passive helper; its caller must establish source
trust. `execute` does that by binding the planned chart roles, child receipt hash,
current installed worker/build, actual snapshot/config/run hashes, unchanged
inputs, successful exit, movie hash and config equality before inspection. It
never infers requests from a runtime error message or a historical URL-only log.

## Embeddable API and resolver

```rust,ignore
let result = dependencies::execute(
    &native_plan, &installed_worker, &asset_source, &fresh_output,
    &dependency_limits,
    &mut |request, checkpoint| {
        // Host service: enforce public anonymous access, robots/terms,
        // cache/rate limits and request.maximum_bytes during receipt.
        // Store acquired bytes and provenance in the asset source.
        // Honor checkpoint while waiting; return their immutable bindings.
        host.resolve_cached_dependency(request, checkpoint)
    },
    &mut |progress| host.progress_and_cancellation(progress),
)?;
```

The host owns acquisition authorization, HTTP/cache behavior and installed
worker authority. A recorded request is not fetching permission. The coordinator
performs no HTTP, DNS, browser launch, Python/Node/FFmpeg invocation or song
installation. It does not reinterpret acquired ActionScript on the host.
`AssetSource` may use shared cache state/interior mutability so the resolver can
supply new bindings while an operation is running.

`MissingDependency` is `{url, maximumBytes, witnesses}`. `url` is canonical;
`maximumBytes` is the smaller of 16 MiB and the remaining operation body budget
at the resolver call (zero permits only an empty response). The host must enforce
it during acquisition. Each witness contains `captureId`, `chartIds`, `iteration`,
`originalUrl`, `snapshot`, `sourceUrl` and optional `pageUrl`. `snapshot` is a
hash binding in namespace `dependency`, rooted at this operation's output.
Witness chart IDs identify every selected role sharing that capture. Duplicate
observations/captures are grouped by canonical URL, not fetched separately.

The resolver returns `DependencyResponse`:

```json
{
  "body": {"namespace": "cache", "path": "blobs/<hash>", "sha256": "<hash>"},
  "provenance": {"namespace": "cache", "path": "responses/<id>.json", "sha256": "<receipt hash>"},
  "httpCharset": "Shift_JIS"
}
```

The bound provenance JSON is `ResponseProvenance`:

```json
{
  "schema": "ddi-danoni-dependency-response/1",
  "logicalUrl": "http://example.test/work/dos.txt",
  "retrievalUrl": "https://web.archive.org/web/<capture>id_/http://example.test/work/dos.txt",
  "retrievedAt": "2026-10-14T12:00:00Z",
  "status": 200,
  "sha256": "<body hash>",
  "policyDecision": "<host policy/cache decision reference>"
}
```

This example is illustrative. `logicalUrl` must equal the requested canonical
URL; `retrievalUrl` is a separately validated anonymous HTTP(S) URL, allowing
archive retrieval without changing runtime identity. Reusing a cache response
preserves its original retrieval date and URL. The host's nonempty policy
reference records its decision; the coordinator does not verify site terms from
that text. Original source provenance remains bound in the seed/attempts.

Only HTTP 200 responses become immutable worker resources. Non-200, mismatched
hash/URL/schema, missing policy/date, unsafe URL or unknown HTTP charset fails;
no empty successful response is synthesized for a 404. Body reads are capped at
16 MiB, provenance at 64 KiB. Original response bytes are copied unchanged.
`httpCharset` may be null; a label must be recognized by `encoding_rs`.

New resources are appended only to the charts whose roles requested them. All
three roles continue sharing each chart's selector events, logical URLs,
FlashVars, charsets and resource context. Byte-identical role configs still
deduplicate within an attempt. A URL resolved earlier for another chart reuses
its already accepted bytes without another resolver call. Original inputs are
snapshotted once; the acquired input cache also reads each binding once.
Unchanged roles may execute again on a retry; every such run consumes the total
capture budget and has separate preserved receipts.

## Limits, progress and readiness

`DependencyLimits` uses camelCase JSON, defaults below and rejects unknown fields.
All limits are integers; byte limits use binary MiB/GiB.

| Field | Default | Maximum / meaning |
| --- | --- | --- |
| `attempts` | 4 | 8 complete selected-plan attempts, including the first |
| `captures` | 32 | 64 reserved distinct captures across all attempts |
| `newUrls` | 16 | 64 distinct newly resolved canonical URLs |
| `newBytes` | 32 MiB | 64 MiB accepted response bodies, before further acquisition |
| `wallMs` | 120,000 | 300,000 elapsed ms across planning, captures, normalization and host callbacks |
| `outputBytes` | 2 GiB | 2 GiB across seed, all attempts, dependency evidence and receipts |

Minimums are 1 except `outputBytes`, whose minimum is 1,024 bytes. Budgets cannot
be zero/disabled. Capture reservations count the entire planned attempt before
launch, so a failed early run can consume reserved slots without executing them.
Source/response metadata and original source bodies retain the existing separate
input/inventory limits. New body accounting is conservative per newly resolved
URL even when two responses contain identical bytes. A rejected response stops
the operation; its bindings remain in `responseAttempts` / `responseInputs`.

Before each attempt, capture and normalization, disk reservations include staged
inputs, worker size, permitted capture output and the normalizer's existing
576 MiB allowance (512 MiB artifacts plus metadata). Small output limits may
therefore reject a work whose actual eventual output would be smaller. Additional
disk checks are polled at 50 ms and forced at readiness boundaries; this is not
an OS disk quota. Per-worker process/file/output limits remain unchanged, and
output traversal rejects symlinks and nonregular files.

Checkpoints run during planning, capture inspection, worker supervision, decoding,
resource resolution and before publishing readiness. The host must cooperate
while waiting in its resolver; an uncooperative blocking callback is detected
when it returns, not forcibly killed by another thread. Callback time counts
against the deadline. The existing supervisor still kills/reaps a cancelled
worker. `Progress` contains `stage`, zero-based `attempt`, reserved `captures`,
accepted `newUrls` and accepted body `newBytes`. Inner readiness is reported as
`attempt-ready`; only the coordinator's final `ready` checkpoint authorizes its
parent marker.

Input errors occur before output creation. Staging failure removes only the
new owned output. Later failures retain attempts and `failure.json`; existing
output directories are never overwritten. A late cancellation can leave a
successful child attempt, but the outer operation remains failed. Only
`DependencyOutcome.ready` and the outer `manifest.json` establish completion.
The existing source/motion/PCM proofs remain mandatory; acquiring every missing
resource does not make a blank or unsupported movie a chart.

## Output and CLI

```text
output/
  seed/                             original prepared inputs and replay-job.json
  dependencies/<sha256(logical URL)>/
    body                            exact accepted bytes
    response.json                   exact bound acquisition provenance
  requests/000.json                 grouped source-bound requests for attempt 0
  attempts/000/                     complete original plan-run layout / diagnostics
  attempts/001/                     fresh updated plan and self-contained replay
  manifest.json                     outer completion marker, written last
  failure.json                      instead on failure/cancellation
```

`requests/NNN.json` is `ddi-danoni-missing-dependencies/1` with `attempt` and
`requests`. Each attempt uses the existing plan-run schema and its replay job can
run from that attempt's staged archive alone. Failed runs are never overwritten.

The outer schema is `ddi-danoni-native-dependency-run/1`, producer
`native-dependency-closure/1`: `ready`, original `request`, `limits`, `worker`,
`attempts[]`, `responses{}`, `responseAttempts[]`, `responseInputs{}`, `counts`,
`completed` and `failure`. Attempts contain index, readiness, receipt path/hash
and failure. Responses are keyed by canonical URL and retain the request,
original source bindings, parsed provenance, copied body path/hash/size and
receipt path/hash. `completed` identifies the successful attempt/receipt and
its normalized manifest/normalization paths and hashes relative to this root.
It is null on failure. Units/reference formats inside normalized works are
unchanged: exact 60 Hz source frames and audio-relative seconds, with musical
fitting and SSC generation handled by the existing preparation APIs.

The offline CLI calls the same API:

```sh
target/danoni-rust/debug/ddi-danoni-export extract-native-dependencies \
  /path/to/installed-bundle /path/to/dependency-job.json /path/to/new-output
```

Job schema `ddi-danoni-native-dependency-job/1` contains `roots`, the existing
`plan`, optional `limits` and optional `responses`. Roots resolve relative to
the job file. `responses` maps canonical logical URLs to the bound
`DependencyResponse` objects above. The CLI uses only this host-prepared cache;
a missing map entry yields `native-dependency-unavailable` and retained request
evidence. It never automatically fetches a URL. Errors also include
`invalid-native-dependency`, `native-dependency-limit` and
`native-dependency-no-progress`, plus existing source/capture/normalization errors.

## Local validation and remaining work

Evidence stays in `~/Music/DanOni/reviews/rust-dependencies-20261014/`.
The new pinned worker is `fac54b70fe8eadeb823cc02cbc63b56cdddabbd3d31585f6eaa600cc001d19a1`;
build identity `512292dc3f6994baed032e2c87ad761a4ebc2a5a5695b9985ac9fd935f7ca78b`.
The previous bundle remains untouched. Source hooks/lockfile/revision and sandbox
policy are unchanged; the navigator source fingerprint changes, so current
supervision rejects older bundles. Historical bound saved captures remain valid
inputs for passive reference comparisons; their URL-only denials cannot drive
this coordinator.

- Higan begins without cached DOS/colour declarations and recovers two existing
  responses (4,640 bytes): two attempts / two captures, then full normalization.
- Archived RE:RAVE Hard begins without DOS and recovers one response (7,422 bytes):
  two attempts / four captures. Its logical geocities URL and original March 2019
  archive retrieval URL remain separate; original retrieval provenance is kept.
- Kouhaku's six charts run on the revised worker without dependency acquisition.
  The combined five-work / ten-chart gate also retains M. MOON and Destined
  Marionette's previously proved runs and the two historical rejection cases.
- Original MIT fixtures prove grouped multi-chart dependencies, one resolver call
  per URL, one source read per binding, no invented chart, no overwrite, method/
  body/header/URL restrictions, truncation, provenance/status/charset checks,
  attempt/capture/URL/byte/output/deadline budgets and cancellation. A complete
  Higan child remains noninstallable after cancellation at outer readiness.

Run the optional real-worker tests with `DDI_DANONI_WORKER_BUNDLE` and
`DDI_DANONI_DEPENDENCY_JOB` pointing to the local bundle and Higan dependency job.
No test fetches third-party data. Neither accepted exports nor the registry is
regenerated. This closes bounded dependency coordination for explicit plans;
automatic selector profiles, policy-aware URL discovery/acquisition, HTML5
capture, native musical-tempo quality, desktop installation and redistribution
notices remain separate work. A missing optional 404 resource still needs the
existing reviewed exception; the dependency loop does not invent one.


Final validation passes 129 native tests, 79 pure tests and two worker unit
tests, Clippy for both SDK feature configurations and the worker, formatting,
and `just check` (REUSE: 799/799 files). The five-work / ten-chart comparison
passes **5,049 endpoints / 144 x1/x2/x3 scroll checks**, maximum time residual
`2.8421709430404007e-11` ms, independent canonical PCM at most one signed-16
unit apart and zero origin corrections. The parity gate binds old/new complete
worker identities and permits only the reviewed `src/navigator.rs` fingerprint
transition; no note/effect/movement/audio fields are ignored. Its optional local
witness fields are `previousWorkerBuild` and `currentWorkerBuild` hash bindings.
The final CLI run retains `maximumBytes` and response-attempt evidence and
produces identical normalized chart/music data to the parity inputs. Local
`validation-summary.json` records all receipts and check-log hashes; accepted
export index and registry hashes are unchanged.
