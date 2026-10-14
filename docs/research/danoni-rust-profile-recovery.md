# Dependency recovery during automatic native discovery

Implemented 2026-10-14 after the [automatic menu profile](danoni-rust-profiles.md).
`profiles::execute_with_dependencies` connects that source-only entry point to
the existing [bounded dependency coordinator](danoni-rust-dependencies.md).
It can recover required inputs before the menu exists, then discover selectors,
prove movement/audio and produce a normalized work without a per-song capture
recipe. No HTTP client, dependency or game/WASM change is added.

## Shared coordinator and API

```rust,ignore
profiles::execute_with_dependencies(
    &profile_request, &installed_worker, &asset_source, &fresh_output,
    &dependency_limits,
    &mut |missing, checkpoint| host.resolve_cached_dependency(missing, checkpoint),
    &mut |progress| host.progress_and_cancellation(progress),
) -> Result<DependencyOutcome>
profiles::execute_dependencies_job(bundle, job_path, fresh_output, progress)
    -> Result<DependencyOutcome>
```

The first API accepts an already trusted installed `WorkerSpec`; the CLI verifies
the separately supplied bundle. Both are blocking APIs for a job thread. The
source is the existing strict `ProfileRequest`: bound movie, source manifest,
original source-only context, metadata, optional-colour evidence and per-profile/
per-worker limits. Contexts cannot inject click/frame recipes or interpreter
settings. Unknown engine profiles reject before output or worker launch.

Explicit plans and automatic profiles now use the same coordinator, response
validator, body cache, URL grouping, retry loop, budgets and final readiness
marker. An internal workflow adapter handles their different preparation,
execution, request witnesses and resource placement. The existing explicit-plan
public API, job schema and receipt schema remain available.

Preparation snapshots original inputs once, stages a source-only profile seed
and preserves original context and acquisition provenance. Each retry runs a new
complete profile operation in a fresh directory. It reproves the menu, selected
rows, schedule/source agreement, movement, exact stream start and independent
PCM alignment. Captures from a failed attempt are not spliced into the successful
chart proof.

Responses use the existing `MissingDependency`, `DependencyResponse` and
`ResponseProvenance` contracts. Only complete current-worker evidence of exact
anonymous body/header-free HTTP(S) GET cache misses can reach the resolver.
The coordinator rereads and checks the child receipt, worker/build identity,
snapshot/run/config hashes, unchanged inputs, successful exit, logical source
URL and movie hash. A changed receipt cannot authorize a host request.

Before a menu is known, witness `chartIds` is an empty array; it does **not** mean
the work has no charts. A later selected probe can report `chart-<menu index>`.
`captureId` identifies the probe, and the snapshot binding uses namespace
`dependency`, relative to the outer recovery directory. Original/resolved logical
URLs remain distinct from any archive retrieval URL. The host still owns access
policy, robots/terms, rate limits and persistent cache behavior. Evidence of a
request is not fetching authorization.

New responses are appended to the profile's immutable **source context**, which
is shared by every subsequent selected chart and proof role. Existing URL/query,
page URL, FlashVars, charset labels and original resource bytes remain unchanged.
Duplicate observations resolve once per canonical URL. Repeating an already
supplied dependency fails rather than fetching it again. Only HTTP 200 response
bodies with matching hashes, provenance and host policy references become worker
resources; missing responses and other statuses never become synthetic DOS files.

## Optional-colour 404s and required missing files

A required DOS failure can accompany a colour-file 404 already backed by the
existing exact review. The ordinary normalization exception intentionally refuses
captures with additional failures; it must stay that strict.

For request classification only, the adapter constructs a temporary audit subset
containing the exact `iro.txt` denied GET and its exact asynchronous error, plus
nonfailure audit entries. The existing review function must validate source movie,
logical URL, HTTP 404 provenance, multiplicity, iteration and diagnostic text.
Bound original response bytes and review coverage are checked too. Only that
proved optional request is omitted from resolver requests. Required failures and
all original audit entries remain in the saved capture.

This subset is never used for chart projection or normalization. After required
inputs are recovered, the fresh capture must pass the unchanged full audit and
optional-colour review. Wrong reviews, unrelated errors, denied methods, traces,
truncation and altered evidence remain failures. The host does not refetch the
known reviewed 404, nor does the coordinator fabricate an empty colour resource.
The low-level `profile-native` command still reports missing requests without
resolving them; its deferred-review flag is preserved.

## Budgets and readiness

The existing `DependencyLimits` apply across seed, all profile attempts, callbacks,
accepted responses, captures and normalization: defaults are four attempts,
32 captures, 16 new URLs, 32 MiB response bodies, 120,000 ms wall time and 2 GiB
output. Existing maxima remain eight / 64 / 64 / 64 MiB / 300,000 ms / 2 GiB.
The resolver receives the remaining body allowance, capped at 16 MiB, before
acquisition. Failed attempts and rejected responses keep their evidence.

An automatic profile's capture count is unknown before observing its menu.
The coordinator counts reservations at each capture boundary, includes previous
attempts, clamps the next profile's capture/wall allowance to the remaining
operation budget and reserves disk space before worker launch/normalization.
A reservation can count even when a disk check prevents launch. Explicit plans
retain their prior full-attempt reservation behavior. Inner profile limits may
be stricter; no caller-controlled field raises the supervisor's maxima.

Inner readiness is reported as `attempt-ready`. Only the outer final `ready`
checkpoint can publish completion; cancellation there leaves the successful child
intact but removes the outer pending marker and clears `completed`. An unavailable
resolver, exhausted budget or later chart-proof failure cannot publish a ready
work. Cooperative resolver waits count against the operation deadline. No library
or accepted export is modified.

## Job, layout and receipt

The offline CLI uses job schema `ddi-danoni-native-profile-dependency-job/1`:

```json
{
  "schema": "ddi-danoni-native-profile-dependency-job/1",
  "roots": {"corpus":"/absolute/local/corpus","cache":"/absolute/local/cache"},
  "request": {
    "movie":{"namespace":"corpus","path":"work/movie.swf","sha256":"<hash>"},
    "sourceManifest":{"namespace":"corpus","path":"work/manifest.json","sha256":"<hash>"},
    "context":{"namespace":"corpus","path":"work/context.json","sha256":"<hash>"},
    "optionalColor":null
  },
  "limits": {"attempts":4,"captures":32},
  "responses": {
    "https://example.org/work/dos.txt": {
      "body":{"namespace":"cache","path":"blobs/<hash>","sha256":"<hash>"},
      "provenance":{"namespace":"cache","path":"response.json","sha256":"<hash>"},
      "httpCharset":null
    }
  }
}
```

This is a structural illustration: replace hashes/paths with actual bindings
and any additional fields from the [profile contract](danoni-rust-profiles.md).
`limits` and `responses` are optional; an absent response map entry records
`native-dependency-unavailable`. Namespace roots resolve relative to the job file.
Unknown job fields reject. The CLI only consults the host-prepared response map.

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- profile-native-dependencies \
  /absolute/trusted/worker-bundle /tmp/recovery-job.json /tmp/new-recovery-output
```

```text
recovery/
  seed/profile-job.json              portable source-only profile job
  seed/inputs/                      original context, provenance and bound blobs
  attempts/000/                     failed profile and all original probes
  attempts/001/                     fresh profile, normalized work and replay plan
  requests/000.json                 grouped source-bound required GET witnesses
  dependencies/<hash-of-URL>/        exact accepted body and response provenance
  manifest.json                     outer success written last
  failure.json                      instead on failure/cancellation
```

The outer schema is `ddi-danoni-native-profile-dependency-run/1`, producer
`native-profile-dependency-closure/1`. Fields are the common dependency receipt:
`ready`, original profile `request`, `limits`, `worker`, `attempts`, `responses`,
`responseAttempts`, `responseInputs`, cumulative `counts`, `completed`, `failure`
and `scope`. `completed` binds the successful child receipt and its normalized
manifest/normalization paths and hashes relative to the recovery root. A failed
operation has null `completed`. The successful child's `replay-job.json` remains
an explicit, portable native plan; it needs neither the resolver nor original
acquisition directories.

## Local verification and remaining work

The local trials remove required resources from previously acquired contexts:
March needs DOS alongside its reviewed missing `iro.txt`; No Name needs DOS and
colour data, then exposes two 5-key charts. Original retrieval URL/date/hash and
robots-aware acquisition provenance back each response; no resource is fetched
again. The recovered three charts match the frozen corpus through 1,898 taps,
10 holds, 1,918 real-parser endpoints and 15 x1/x2/x3 scroll checks. Maximum
measured timing error is `2.8422e-11` ms; canonical PCM differs by at most one unit.
These checks approve source/scroll/audio parity, not musical estimate quality.
March reserves six captures and resolves one 2,372-byte body; No Name reserves
eight captures and resolves two bodies totaling 8,403 bytes. Jobs/captures/results stay in
`~/Music/DanOni/reviews/rust-profile-recovery-20261014/`.

Set `DDI_DANONI_WORKER_BUNDLE` and `DDI_DANONI_PROFILE_DEPENDENCY_JOB` for the
installed-worker tests (the supplied job should be the missing-DOS March case).
They cover successful pre-menu closure without refetching the optional 404,
read-once original inputs/responses, portable replay, cumulative capture counts,
late outer cancellation, attempt/capture/body/output bounds, unavailable host
responses and changed child receipts. Original synthetic tests verify exact
optional-request classification and rejection of altered evidence. The existing
explicit-plan dependency tests also run against the shared implementation.

This closes required-resource recovery before automatic menu discovery for the
reviewed profile. It does not broaden engine recognition or solve the early-hold
`life = 0` motion gap. Next prove generic hold-motion/derived play inputs, extend
engine-family contracts, then add milestone 5's host URL/policy/cache and bounded
index-discovery services. Musical fitting acceptance, HTML5 browser references,
other-OS worker confinement and desktop library installation remain separate.
