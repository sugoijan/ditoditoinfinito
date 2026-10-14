# Rust restricted native capture supervisor

Implemented 2026-10-13 within [milestone 4](danoni-rust-plan.md#4-native-extraction-and-supervision).
`supervisor::capture` now stages verified inputs and a trusted installed Ruffle
worker, executes it under the tested macOS sandbox, monitors resource limits,
validates capture transport and returns portable bindings for
[normalization](danoni-rust-normalization.md). Production capture no longer needs
the Python launcher. The worker itself was already Rust; its pinned build/hook
preparation recipe remains the separate existing `ruffle/prepare.py`.

This does not implement URL fetching, dependency-request retries, selector
recognition or desktop installation. A complete capture is evidence for the
family/motion/audio adapters, not automatic admission of a chart.

## Host API and trust boundary

```text
capture(&CaptureRequest,&WorkerSpec,&AssetSource,newOutput,checkpoint)
  -> Result<CaptureOutcome>

WorkerSpec:                         trusted installed host configuration
  executable: filesystem path
  sha256: lowercase SHA256
  buildIdentity: filesystem path
  buildIdentitySha256: lowercase SHA256

CaptureRequest:                     camelCase; unknown fields reject
  config: Binding
  movie: Binding
  provenance: Binding | null
  limits: Limits                    defaults below

CaptureOutcome:
  ready: bool
  output: canonical owned output directory
  run: parent receipt
  manifest: ready receipt | null
  failure: structured Error | null
```

Worker paths/hashes come from the host's installed package, never from a work's
page, dos, SWF, cached resource or recipe. The CLI job is an explicit trusted
operator/host document. Hashing an arbitrary binary does not prove its source;
the **host's approved executable hash** is the authority. The supplied build
identity and the worker's embedded result must also match the tracked expected
identity exactly: Ruffle v0.7.1 revision
`89a7049965f3090e706265670df309a5128233ac`, lockfile, template, preparation script,
stored-property/snapshot hooks and worker audio/main/navigator source hashes.
`expected_build_identity()` exposes that reviewed manifest. There is no search of
PATH for the worker, arbitrary argument vector or caller-supplied sandbox policy.

The library is native-feature gated. Latest stable `libc 0.2.190` (macOS only)
and `url 2.5.8` were verified through crates.io/Cargo; `cargo update` also advanced
the existing ZIP dependency to 9.0.3. The game workspace and its WASM dependencies
are unchanged. Non-macOS calls return `unsupported-native-confinement` without
staging or executing a worker. Linux/Windows confinement remains unimplemented;
there is no unrestricted fallback.

Run synchronous staging/parsing and supervision on a host worker thread. The
checkpoint callback supplies cancellation before staging, during resource/tag
work, between staged files, before launching, at each monitoring iteration and
through protocol/pixel validation. It must not panic during ordinary cancellation.
An unwind still drops the owned process guard, kills/reaps a running child and
removes the incomplete owned output. Normal runtime failures/cancellation retain
diagnostics; input validation errors before launch return `Err` without a new
output. Fatal filesystem errors may prevent receipt retention and clean up the
owned incomplete tree. Existing destinations are never overwritten.

## Versioned request/config contract

The config's original bytes are retained exactly, including URL queries, selector
events, FlashVars, charsets and budget settings. It is the existing reviewed
worker config, now parsed into a strict bounded Rust contract:

```text
movie: contained relative path
sourceUrl: anonymous absolute HTTP(S) URL without fragment
pageUrl: same URL constraints | null
flashVars: {string:string,...}         default {}; at most 128 entries
captureFrames: 1..16 strictly increasing integers in 1..1200
render, mixAudio, inspectGlobals: bool default false
renderScale, inspectionScale: 1..4 | null; defaults 1
avmExecutionMs: 50..10000 | null; default 500
events: Event[]                       default []; at most 64
resources: Resource[]                 default []; at most 64

Event (type-tagged, unknown fields reject):
  {type:"click",frame:1..1200,x:0..320,y:0..240}
  {type:"enter",frame:1..1200,holdFrames:1..120|null}
  {type:"key",frame:1..1200,code:Enter|ArrowLeft|ArrowRight|ArrowUp|ArrowDown|Space,
   holdFrames:1..120|null}
  {type:"text",frame:1..1200,value:1..32 printable characters}

Resource:
  url: anonymous absolute HTTP(S) URL without fragment
  path: contained relative path
  sha256: lowercase SHA256
  httpCharset: known encoding label | null
```

Keys release after the held interval (default one frame), no later than 1201.
Render scale above one requires rendering. URLs/FlashVars are bounded to 8,192
bytes per text field; control characters in typed text reject. Canonically
duplicate resource URLs reject, including case/default-port aliases. Resource
charsets use the existing `encoding_rs` labels. Unknown config/event fields do not
silently enter a future worker protocol.

Config is at most 64 KiB, movie at most 16 MiB with the existing 64 MiB SWF
expansion/tag bounds, each cached resource at most 16 MiB and declared resources
at most 64 MiB total. Optional provenance is at most 16 MiB and must identify the
same movie hash; its original acquisition dates/unknown values remain unchanged.
Actual binding bytes are checked by the existing bounded `AssetSource`. The
config-relative movie must resolve to the explicit movie binding. Each declared
resource resolves relative to that config; all bytes are hash-checked. No live
fetch, guessed missing body or extra cache-directory exposure occurs.

## Fixed confinement and resource policy

A new private output directory contains separate `inputs`, `runtime` and `capture`
subdirectories. Only bound files are staged. The verified executable is copied
into `runtime` and made owner read/execute only; its manifest and fixed
[sandbox profile](../../tools/danoni-dump/ruffle/worker.sb) are retained alongside it.
Inputs, worker, build manifest and sandbox bytes are verified again after exit.
This also avoids relying on source cache files remaining stable during execution.

The launcher invokes `/usr/bin/sandbox-exec` directly with structured arguments,
then the exact staged executable and config. It clears the environment and supplies
only fixed PATH, private HOME/TMPDIR; stdin is null, stdout/stderr go to owned files.
There is no shell or Python/Node/FFmpeg/ps subprocess. The child creates a separate
session/process group. Before exec, async-signal-safe native calls apply CPU,
file-size, descriptor and no-core limits. The sandbox denies networking, child
process creation, user-file access and input/runtime writes, while permitting
system runtime/renderer services and capture-directory writes under the same
profile already tested by the original launcher.

| Limit | Default | Allowed maximum |
| --- | --- | --- |
| Worker wall time | 30,000 ms | 60,000 ms |
| Sampled resident memory | 2,048 MiB | 2,048 MiB |
| Kernel CPU time | 20 s | 20 s |
| Kernel individual file size | 40 MiB | 40 MiB |
| Aggregate capture output | 128 MiB | 256 MiB |
| Open file descriptors | 128 | fixed |
| Core files | zero bytes | fixed |
| Worker output entries | 32 | fixed |
| Monitor interval | 10 ms | fixed |

Wall/CPU/RSS budgets cannot be zero; file/aggregate budgets are at least 1 KiB.
Limits cannot be raised beyond the reviewed maximum. Memory is read directly
with macOS `proc_pidinfo(PROC_PIDTASKINFO)`. Task teardown can precede wait-status
availability: a failed memory read gets at most 10 ms of exit-status-only checks,
while still honoring wall time/cancellation. A completed child is reaped; a live
child whose memory remains unavailable is killed. This is sampled RSS,
not an instantaneous allocation quota. Aggregate output is sampled too; kernel
file-size limits supply the independent per-file bound. Observed peak RSS and
actual signals remain in the receipt rather than implying complete peak coverage.

A stop kills the owned process group and waits for the child. An armed RAII guard
also does this on early return/unwind. It is disarmed as soon as the owned child
has been reaped, avoiding signals to a subsequently reused PID. CPU/file kernel
termination can yield different signals; exact signal/nonzero exit is retained.
No killed child is reused for another work. Hard parent-process termination is
not a cross-platform parent-death guarantee; the desktop host must cancel/join
normal shutdown, and the child still has independent kernel CPU/file limits.

Native API references: [Rust `CommandExt` and pre-exec requirements](https://doc.rust-lang.org/stable/std/os/unix/process/trait.CommandExt.html),
[libc's macOS bindings](https://docs.rs/libc/0.2.190/libc/).

## Transport validation and precise output

```text
manifest.json                     readiness marker, only after full validation
failure.json                      diagnostic receipt, only on runtime failure
run.json                          parent report; outside worker-writable capture
inputs/capture.json               original config bytes
inputs/<movie path>               original movie bytes
inputs/<resource path>            original declared cache bytes
inputs/source-manifest.json       optional original provenance
runtime/ddi-ruffle-probe           verified staged executable
runtime/build-identity.json        exact reviewed build manifest
runtime/worker.sb                  exact fixed policy
capture/snapshot.json             stdout; may be incomplete on failure
capture/stderr.txt                 worker diagnostics
capture/audio.pcm                 only if mixAudio=true; stereo S16LE 44,100 Hz
capture/frame-<iteration>.ppm      only if render=true; exact RGB P6 pixels
```

Parent `run.json` retains schema `ddi-ruffle-run/experimental-1` so existing
passive readers can consume it. Additions include producer
`macos-sandbox-supervisor/1`, signal, monitor/input-verification errors and exact
limits/poll interval. It records exit code, termination (including cancellation,
wall/RSS/output/file/CPU limits), UTC start, elapsed seconds, observed peak RSS,
actual worker/config/sandbox hashes and `inputsUnchanged`. Nonzero exits and
incomplete captures cannot pass normalization's successful-run checks.

A resident-memory sample can become unavailable while a completed macOS process
tears down, before its wait status is exposed. The existing bounded 10 ms exit
recheck still applies. If it expires, the supervisor **still stops the owned
process group and child, then reaps**. Only confirmed stop/absence plus a normal
zero exit can resolve this teardown race: `monitorExitRace` retains the original
monitor error/resolution, while `termination` and `monitorError` become null.
A live unmonitorable child is killed and keeps its failure; signals/nonzero
status, failed stop confirmation, cancellation and every resource-limit reason
remain failures. No worker is resumed and no additional unmonitored grace is
introduced. Full output/source/protocol validation and final cancellation checks
still follow. The original predicate/status regression and installed worker
limit/cancellation tests cover this distinction.

After successful exit, snapshot schema/release/revision/build/config/movie hash
must agree; FPS must equal the actual parsed movie header and render settings
must agree. Captured iterations must exactly match the requested ordered list.
Every state must be complete/nonhalted; requested globals must be present.
Audits must have complete details and exact runtime-error counts. PCM frame count
is `floor(lastIteration * 44100 / sourceFps)`, with exact dimensions/byte count.
Every requested image must have the exact P6 header and scale-derived RGB size.
Unexpected files, directories, symlinks or missing outputs reject readiness.

**Runtime audit entries remain intact.** A complete capture may contain denied
resource requests, navigation or runtime errors: those are evidence for the
upcoming dependency loop or narrowly reviewed family diagnostics. Transport
readiness does not suppress or authorize them. Family projections still reject
unproved errors. Truncated audits, truncated state, a halted VM or malformed
transport are failure captures, never ready charts.

Ready/failure receipts use `ddi-danoni-native-capture/1`:

```text
producer, ready, request, worker, build, sandboxSha256
sourceInputs: {"namespace:path": original Binding,...}
stagedInputs: [{path,sha256,bytes},...]
runtime: {worker,buildIdentity,sandbox}   contained artifact paths
run: complete parent report
failure: Error | null
artifacts: [{path,sha256,bytes},...]     worker outputs; not host receipt files
bindings: null on failure; otherwise:
  capture: CaptureBindings             namespace "capture"
  movie: Binding
  nativePcm: Binding | null
  provenance: Binding | null
scope
```

A host maps namespace `capture` to the returned directory and passes those
bindings to `prepare_projection`/`prepare_native_work`. All parent/config/snapshot
hash and control checks still apply. To join separately captured selectors of a
shared-movie collection, the host must deliberately consolidate **identical
verified** config/movie/resource bytes into a common immutable input namespace.
The normalization contract's explicit config-relative movie identity is retained;
path aliases are not guessed. The local six-chart trial does that consolidation
and records the copied hashes. Request provenance is preserved rather than
invented from the fresh run date.

`manifest.json` is atomically renamed last. Failed runs retain `failure.json`,
parent report and available raw outputs, with no readiness marker. The CLI returns
nonzero for failure/cancellation; the embedding API returns a `ready:false`
outcome so the host can present diagnostics or perform a later allowed fetch.
Neither interface promotes or publishes captures into the accepted library.

## CLI, local acceptance and next steps

```text
{schema:"ddi-danoni-native-capture-job/1",roots:{namespace:externalDirectory,...},
 worker:WorkerSpec,request:CaptureRequest}
```

Relative root/worker/build-identity paths resolve against the job. Inputs/outputs
stay outside Git; existing output directories reject.

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- capture-native /tmp/capture-job.json /tmp/capture-new
```

Local evidence: `~/Music/DanOni/reviews/rust-supervisor-20261013/`:
`jobs/`, `captures-v1/`, `capture-trial-v1.json`, `normalization-cases.json`,
`shared-kouhaku-inputs/`, `normalized-verified/` and `cli-verified/`. All captures
in this trial use Rust-owned supervision; Python was used only for original fixture/job generation
and passive differential assertions, with no movie execution in the host.

The trial covers all **seven pinned charts**, original two-frame stored-state/
getter/child-bank fixtures, rendered pixel checks, exact PCM impulses, polled
keys and stored global data. Incomplete graphs, exhausted audits and halted AVM
runs fail without a readiness manifest. Original native helper probes separately
verify actual OS outside-read/write, input-write, symlink-target, live-listener
network and child-spawn denial, plus wall/RSS/CPU/file/aggregate-output stops,
cancellation, crashes, malformed output, early bad bindings/worker identity,
no-overwrite behavior and actual child reaping. A real original Ruffle fixture
also tests cancellation after complete transport and before the final readiness
marker; its completed child output remains a failure capture.

Fresh Rust-supervised captures feed the same Rust normalized producer and real
SSC parser: **2 works / 6 songs / 7 charts**, **3,073 endpoints**, **57 x1/x2/x3**
authored-speed checks. Current/frozen references match; independent PCM differs
by at most one signed unit, with zero decoder shifts. Numeric stars, generic
tiers and shared-song grouping survive parsing. This still uses the exact-source
fallback grid and does not accept 75 BPM as musical tempo or replace the accepted
exports. Their index/registry hashes remain unchanged. The previous
78-work/113-chart structural regression also passes after the dependency refresh. CLI normalized
files match the library outputs byte-for-byte; repeat synthetic captures and
rendered pixels match exactly, and existing output directories reject.

Next: port worker build/package preparation and turn the existing selector and
capture recipes into reproducible Rust requests; resolve the three recorded
structural selector gaps through this supervisor. Then implement bounded native
request/dependency iteration alongside milestone 5's public URL acquisition.
Recipe-free URL input, Linux/Windows confinement, HTML5 control, native musical
alias/phase review and desktop library integration remain open.


Worker preparation and installation now have a [Rust bundle API](danoni-rust-worker.md).
It keeps the existing capture identity unchanged, prepares an isolated pinned
source tree and verifies an installed bundle into the `WorkerSpec` above.
`prepare.py` is retained only as a historical build oracle; no Python is required
for fresh source preparation, packaging or runtime supervision.
