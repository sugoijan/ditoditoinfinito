# Rust native selector and capture orchestration

Implemented 2026-10-14 after rebasing `danoni-research` onto local `main`
`18791f5`. The rebase replayed all 49 research commits without conflicts; the
pre-rebase branch is `backup/danoni-research-before-rebase-20261014`.

`tools/danoni-dump/rust/src/orchestration.rs` joins bound source inputs, restricted
capture and normalized-work production. One selector configuration per chart
supplies **all** projection, motion and audio runs. A caller cannot omit a click
or change FlashVars in just one role. This is the native app's shared orchestration
API and CLI, not an automatic URL importer: discovery, fetching and automatic
selector/geometry selection remain separate work.

## API and job schema

The module is behind `native`, with no new dependencies or game workspace/WASM
changes. Relevant types and functions are:

```rust,ignore
prepare_plan(&NativePlan, &AssetSource, checkpoint) -> Result<PreparedPlan>
execute(&NativePlan, &WorkerSpec, &AssetSource, new_output, progress)
    -> Result<PlanOutcome>
execute_job(installed_bundle, job_path, new_output, progress)
    -> Result<PlanOutcome>
```

`execute_job` first verifies the host's installed [worker bundle](danoni-rust-worker.md).
The bundle path is a separate trusted CLI argument, never a fetched work field.
`execute` accepts an already trusted `WorkerSpec`; the [supervisor](danoni-rust-supervisor.md)
checks its bytes for every capture. Unsupported platforms never gain an
unrestricted executor. An app should run these blocking APIs on a job thread.

`PlanJob` uses schema `ddi-danoni-native-plan-job/1`:

```json
{
  "schema": "ddi-danoni-native-plan-job/1",
  "roots": {"corpus": "/absolute/local/corpus"},
  "plan": {
    "sourceManifest": {"namespace":"corpus","path":"work/manifest.json","sha256":"<sha256>"},
    "movie": {"namespace":"corpus","path":"work/inputs/movie.swf","sha256":"<sha256>"},
    "metadata": {},
    "limits": {"wallMs":60000},
    "charts": [
      {
        "id": "normal",
        "label": "Normal",
        "config": {"namespace":"corpus","path":"work/inputs/normal.json","sha256":"<sha256>"},
        "options": {"family":"child-timeline-5","mode":"5","sourceSuffix":null,"pinnedSelection":null},
        "projection": {"captureFrames":[100,1200],"mixAudio":true},
        "motion": {"captureFrames":[150,151,152]},
        "audio": {"captureFrames":[100,1200],"mixAudio":true},
        "motionSelection": {"note":"leftMC2","receptor":"left_step","lane":0,"iterations":[150,151,152],"targetY":70,"geometryReference":null},
        "streamStartIndex": 0,
        "optionalColor": null
      }
    ]
  }
}
```

This is an illustrative structure, not a runnable fixture or inferred selector.
All hashes must be actual lowercase SHA-256 of the bound bytes. Relative `roots`
resolve against the job file; paths within a namespace use the usual contained
binding rules. Unknown job/plan/chart/observation fields are rejected.

`NativePlan` contains `sourceManifest`, `movie`, default `metadata`, `charts` and
default supervisor `limits`. `ChartPlan` contains the existing
[projection options, motion selection and optional colour evidence](danoni-rust-normalization.md),
a single bound `config`, and three `Observation` values. An observation has:

| Field | Meaning / default |
| --- | --- |
| `captureFrames` | Required, strictly increasing worker iterations, 1–1200; at most 16 |
| `render` | Capture PPM pixels, default false |
| `mixAudio` | Capture 44,100 Hz stereo signed 16-bit PCM, default false |
| `avmExecutionMs` | Optional reviewed interpreter budget; absent preserves the base config value/default |

Audio observations must enable `mixAudio`. Every selected motion iteration must
be present in the motion observation. Motion's geometry/arrival proof still
requires its existing repeated stable samples; merely observing frames is not
acceptance. `streamStartIndex` selects a runtime-observed MP3 stream start, not a
source BPM or a frame offset. All other normalization units remain unchanged:
60 Hz source frames, audio-relative seconds, measured arrow heights per second.

Chart IDs are 1–128 ASCII letters/digits/`-`/`_`, unique ignoring ASCII case.
Labels preserve source text (up to 4 KiB); ratings/classification are unchanged.
Each plan selects 1–16 charts and at most 32 distinct captures. These are explicit
selected subsets; the API does not claim it has discovered every chart of a work.

## Planning and immutable inputs

`prepare_plan` validates the source manifest/movie hash, the 60 Hz movie, every
original configuration and every declared resource before launching anything.
It uses the same strict supervisor validator: no shell/script fields, no live
network, anonymous HTTP(S) source identity, bounded inputs and reviewed events.
The movie named relative to each base config must independently match the
explicit movie hash; an unavailable configured path is not guessed.

The planner snapshots original config bytes and source provenance. Derived
configs rewrite only local movie/resource paths into common `blobs/<sha256>`
paths, make an omitted empty resource list explicit, and apply observation
settings. Original source/page URLs, queries, FlashVars, selector events, charset
labels and other script inputs remain unchanged. The [navigator](../../../tools/danoni-dump/ruffle/src/navigator.rs)
continues resolving requests against the original logical URL and exact cached
GET resources. No acquired script executes in the host.

Three roles derived from one base config therefore keep identical controls.
Only the observation fields already permitted by the normalizer differ. Complete
serialized config bytes identify each `capture-<sha256>`; identical configs reuse
one capture, including between roles/charts. Array order, selector values and
resource declarations are preserved rather than guessed equivalent.

Optional missing-colour reviews keep their exact original bytes and HTTP evidence.
A separate derived review changes response **binding paths** to staged blobs,
keeping body hashes, URLs, status, retrieval date and all other fields. Every
original response must match exactly one reviewed row. The normalizer's exact
404/audit checks still run; no response or exception is synthesized.

Planning is bounded to 4,096 staged files / 128 MiB, including preserved configs,
blobs, request and replay job. Configs remain at most 64 KiB. Preparation failure
or cancellation leaves no output. The prepared result exposes `manifest`,
`files` and `captures`; execution replans the original request, so it never trusts
an arbitrary deserialized prepared object.

The planning receipt uses schema `ddi-danoni-prepared-native-plan/1`, producer
`native-plan-orchestration/1`. It records `request`, original `sourceInputs`, staged
`movie`/`provenance` bindings, `captures` (`{id, request}`), `charts`
(`{id, projection, motion, audio, optionalColor}`), sorted
`artifacts` (`{path, sha256, bytes}`) and `scope`. Hashes bind bytes; sizes are bytes.

## Execution layout and readiness

```text
work-run/
  plan.json                         original typed plan request
  planning.json                     prepared plan / input inventory
  replay-job.json                    portable offline recapture job
  inputs/
    source-manifest.json             byte-identical acquisition provenance
    original/<chart>.json            byte-identical original selector config
    original/<chart>-color.json      optional original review
    <chart>-color.json               optional relocated review
    capture-<sha256>.json            derived shared role configuration
    blobs/<sha256>                  original movie, resources and HTTP evidence
  captures/capture-<sha256>/         supervisor-owned inputs/runtime/capture/run
  normalization-request.json        generated NativeWorkRequest, namespace plan
  normalized/                       standard portable normalized work
  manifest.json                     parent readiness marker, written last
  failure.json                      failure/cancellation instead of readiness
```

`normalized/` follows the existing [normalization contract](danoni-rust-normalization.md).
Its WAV, references, source frames, hold endpoints, effects and scroll evidence
feed the existing Rust song/SSC preparation APIs. This orchestration entry point
does not silently select a musical BPM estimate, generate accepted exports or
install anything into the local library.

`PlanOutcome` has `ready`, absolute `output`, `manifest` and optional `failure`.
Parent receipts use `ddi-danoni-native-plan-run/1`, producer
`native-plan-orchestration/1`, with:

- `ready`, `stage` and `failure`: ready or the last attempted phase/error.
- `request`, `worker`, `planning`, `planningSha256`: the explicit source/host plan.
- `captures`: object keyed by capture ID; each entry records `ready`, `failure`,
  optional relative capture manifest path plus `manifestSha256`, and the supervisor
  `run` receipt.
- `normalized`: on success, `{manifest, manifestSha256, normalization, normalizationSha256, charts}`;
  paths are relative to this work-run root. It is null on failure/cancellation.
- `scope`: explicit offline selection, not discovery, tempo acceptance or install.

Source/planning failures return `Err` before output creation. Cancellation while
staging removes its newly owned partial tree. Later capture/proof/cancellation
failures normally retain diagnostics in `failure.json`; fatal filesystem errors
may remove that new partial output. Existing directories are never overwritten.

**Only the parent `manifest.json` and `PlanOutcome.ready` authorize the next work
stage.** A completed capture is transport evidence, not chart readiness. A late
cancellation can retain a complete `normalized/manifest.json` inside a failed
parent; it still cannot install that work. The final checkpoint runs after the
parent receipt is serialized/written to a temporary file, immediately before
its ready-marker rename. Cancellation publishes failure instead.

Progress callbacks receive `{stage, captureId, completed, total}` for planning,
staging, capture, normalization and the readiness checkpoint. Returning an error
cancels through existing cleanup/reaping behavior; a progress event itself is
not completion authority. Per-capture limits are unchanged. Conservative disk
reservations allow staged runtime/input/output plus the normalizer's 512 MiB
maximum before each capture and check completed work against 2 GiB. This is a
logical-size policy, not an OS filesystem quota; hostile failed worker outputs
remain subject to the supervisor's monitored limits and failure handling.

## Running and replaying

```sh
cargo build --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --target-dir target/danoni-rust
target/danoni-rust/debug/ddi-danoni-export extract-native /path/to/trusted/bundle /path/to/plan-job.json /path/to/new-work-run
```

The CLI returns the ready receipt or a nonzero error. All application logic is
in the shared Rust API. No Python, browser, FFmpeg or compiler command is needed
at runtime; the installed worker is the same sandboxed native executable.

A portable `replay-job.json` uses only the work-run's staged inputs (`roots.plan`
is `.`), canonical role config, movie/provenance and relocated review bindings.
This supports a clean rerun without the original corpus namespaces:

```sh
target/danoni-rust/debug/ddi-danoni-export extract-native /path/to/trusted/bundle /path/to/work-run/replay-job.json /path/to/new-replay
```

The new run keeps acquisition provenance and gets a fresh capture timestamp.
Do not expect byte-identical run receipts or timestamps. Compare captured JSON
structurally and music/pixels/reference data by their appropriate byte/value rules.

## Local verification and resolved proof gaps

Evidence: `~/Music/DanOni/reviews/rust-orchestration-20261014/`.
No third-party data is committed. `runs-v2/` has replayable Higan/Kouhaku/M. MOON
captures and the first archive-only M. MOON replay. `runs-v3/m-moon/` repeats
that replay with the final parent/child receipt hashes. `runs/` retains the initial diagnostics and
Destined Marionette's revised stable-motion run. `normalization-cases.json`
rebinds their preserved captures into the existing downstream parity gate.

| Work / chart | Result |
| --- | --- |
| Higan Normal | One capture reused by all three roles; original reference/PCM parity |
| Kouhaku, all six charts | Six unique captures; five songs, original filled stars and reference/PCM parity |
| M. MOON `chart-1` (`★ぞろり★`) | Two captures; all roles now include the projection config's frame-330 click at `(70,212)`; full frozen chart parity |
| Destined Marionette Original | Motion measured at iterations 506–511 and 527, logical frames 438–443 and 459, strictly before its frame-460 speed change; existing linearity/arrival/initial-speed checks pass |
| RE:RAVE Hard | Still `native-source-bank-missing`: loaded speed data has no proven native bank; no ready marker |

M. MOON's replay produces equal captured state and byte-identical PCM using only
its staged archive. Normalized chart/data manifests are equal after excluding
only their fresh `capturedAt` timestamps. No engine/normalizer proof was relaxed.
Destined Marionette's measured base scroll remains `4.416403785488958` arrow
heights/s, with an arrival difference of about 0.7 frames. Its earlier request
including logical frame 460 still rejects as `native-motion-segment-unproved`.
The old mismatched M. MOON request still rejects as `native-run-controls-mismatch`.

All four successful works / nine charts / eight songs pass the existing strict
reference/audio/real-game-SSC gate: 4,041 taps, 43 holds, **4,127 endpoints** and
**141 x1/x2/x3 scroll checks**. Maximum time error is about `2.84e-11` ms;
independent canonical PCM differs by at most one signed-16 unit and all measured
origin corrections remain zero. Accepted 93-song/144-chart exports and the
registry remain unchanged. Source-grid validation does not close the separate
native musical-tempo quality review.

Original tests cover shared controls, exact deduplication, original byte
preservation, resources/charsets, tampering, input/observation/count bounds,
optional-review relocation, portable replay, no overwrite and cancellation.
A real blank SWF proves successful transport cannot become an invented chart;
a real Higan plan proves late cancellation blocks parent readiness even after
normalization is complete. Set `DDI_DANONI_WORKER_BUNDLE` and
`DDI_DANONI_NATIVE_PLAN_JOB` to local installed-worker / Higan replay paths to
exercise those optional tests. No test fetches a work. The final native suite passes 120 tests and the pure
suite 78; both feature configurations pass Clippy, and formatting passes.
`just check` passes on the rebased branch, including REUSE for all 796 files.
Local `validation-summary.json` records source/receipt hashes and the parity gates.

Next: build the bounded
request/dependency loop above these immutable plans. Automatic selector/profile
selection, public-URL acquisition, HTML5 capture and desktop installation remain
pending. Worker redistribution notices and other-platform confinement remain
separate requirements.


### RE:RAVE closure, 2026-10-14

The [source/control review](danoni-rust-extraction.md#rerave-selector-controlled-speed-proof-2026-10-14)
proves that Hard intentionally leaves shared speed data inactive; LVUP owns the
native speed bank. A fresh execution of the existing Hard plan now produces a
parent ready marker with two confined captures, complete frozen chart/reference
parity, independently matched PCM and zero audio-origin correction. The
normalized reference retains `inactiveSourceSpeed` without creating an event.
The installed worker, capture controls, sandbox policy and 1,200-frame limit
are unchanged; no late-chart execution or host ActionScript interpretation is
needed for this exact reviewed control path.

The combined fresh gate now covers five works / ten charts / nine songs:
4,963 taps, 43 holds, **5,049 endpoints** and **144 x1/x2/x3 scroll checks**.
Maximum time error remains `2.8421709430404007e-11` ms and independent canonical
PCM differs by at most one signed-16 unit. The old Destined boundary-crossing
and mismatched M. MOON requests remain rejected. The RE:RAVE historical
missing-bank request now succeeds only under its exact movie/selector contract.
All evidence stays in `~/Music/DanOni/reviews/rust-rerave-20261014/`; accepted
exports and registry remain unchanged. Next is the bounded dependency/request
loop above explicit immutable plans, followed by automatic selector profiles
and URL acquisition.

The current worker's URL-only `fetch-denied` record is insufficient to distinguish
an absent GET from a denied POST/body. Dependency orchestration first requires
structured, source-bound request evidence and a reviewed worker identity update;
see the [next-slice constraints](danoni-rust-plan.md). Do not derive host fetches
from trace/error text or assume that any denial is a body-free GET.


## Dependency follow-up, 2026-10-14

The [bounded coordinator](danoni-rust-dependencies.md) now wraps these immutable
plans with structured request discovery, a host cache/resolver callback and
fresh retries. Each attempt still uses the original selector/context and full
normalization checks; the outer dependency marker is the completion authority.
Higan and archived RE:RAVE recover omitted cached resources, and Kouhaku's six
charts run on the revised worker. Public URL acquisition and automatic selector
profiles remain subsequent stages.
