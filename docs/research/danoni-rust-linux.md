# Linux native worker confinement and software rendering

Continuation of the [portable renderer](danoni-rust-rendering.md) and
[worker bundle](danoni-rust-worker.md) work, 2026-10-15. The implementation is
inside the standalone dump SDK. Main application crates and `docs/PLAN.md` are
unchanged. Windows and Linux architectures other than x86-64 still fail closed.

## Runtime boundary

Linux capture requires **Landlock ABI 6 or later**, enabled by the kernel and
permitted by the host, plus seccomp filters and `close_range`. A capability query
rejects an unavailable/older Landlock kernel before staging. Failure to install
any restriction prevents exec and cannot produce a Ready capture. No fallback
runs a movie without confinement. There is no runtime container, shell, Python,
Node, Java or browser dependency.

The parent prepares Landlock rules and a seccomp allowlist, then applies them
in the single-threaded pre-exec child using raw system calls. `no_new_privs`
prevents privilege gain. Landlock allows reads of the staged inputs, execution
of the exact staged worker and dynamic loader, reads of distribution library
and Vulkan-manifest directories, and regular-file reads/writes in the capture
directory. It grants no general home, `/etc`, procfs or GPU-device access.
Kernel CPU topology, the loader cache, `/dev/null` and `/dev/urandom` have narrow
read grants. Filesystem metadata visibility is not the same as permission to
read file contents; this is an access-control policy, not a hidden filesystem.

Seccomp rejects unknown syscalls and foreign ABIs. It denies network/IPC socket
creation, fork/vfork, namespace operations, ptrace/process memory access,
io_uring, device ioctls, executable-fd launch and kernel-control APIs. `clone3`
returns ENOSYS so glibc can use `clone`, which accepts only thread flags with
shared VM/signal handlers and no namespace or exit-signal bits. Ordinary pthreads
work. Signals to processes outside the Landlock domain are scoped off.
`fcntl` only accepts descriptor/flag commands; `prlimit64` can only read the
current process's limits.

Lavapipe needs `memfd_create` for anonymous shared buffers. Only the exact
`MFD_CLOEXEC | MFD_ALLOW_SEALING` flags are allowed: no huge-page or explicitly
executable memfds. File-size limits still apply. Landlock prevents executing
these unlinked files, and seccomp denies `execveat`. An initial negative run
preserved a SIGSEGV after Mesa used an invalid fd following the denied allocation;
the trace showed `memfd_create` denial, then `ftruncate(-1)`/`mmap(-1)`. No
ActionScript runtime error was discarded to accommodate that failure.

All inherited descriptors above stderr become close-on-exec, including any
host files/sockets that were accidentally opened without that flag. The worker
has a fresh session/process group, cleared environment, private HOME/TMPDIR and
null stdin. Fixed Mesa controls disable the disk shader cache and request two
Lavapipe threads. No driver-loading environment variable is inherited from a
page or host shell. The tested renderer is Mesa CPU Vulkan; hardware graphics
nodes and display services are outside this initial policy.

The shared Unix supervisor preserves wall/CPU/file/output/descriptor/core
budgets, cancellation, input revalidation, artifact/protocol checks, exact
signals and owned-child reaping. Linux samples RSS through the parent's
`/proc/<owned-pid>/statm`; the worker cannot read procfs. The same budget
also includes allocated blocks in the child's unique memfds, sampled through
the parent's `/proc/<owned-pid>/fd`, so open anonymous buffers written without
mapping them are included. Mapped memfd pages can be counted twice;
this is deliberately conservative. `peakRssKiB` remains actual sampled RSS;
`peakAnonymousFileKiB` and `peakAccountedMemoryKiB` record the additional measure.
This is sampled accounting of RSS and open memfds, not a cgroup memory quota;
backing pages held only through mappings after their fd closes are not separately
enumerated. Memory and aggregate output are sampled, not instantaneous allocation
quotas. Linux also installs
SIGKILL on parent-thread death, with a race check; normal host shutdown should
still cancel/join jobs. Unexpected/malformed output remains a failure capture.

The Linux producer is `linux-landlock-seccomp-supervisor/1`. `run.json` records
the observed/required Landlock ABI, seccomp policy version and parent-death
signal. `runtime/worker-linux.rs` contains the exact compiled policy source;
its hash replaces the macOS `.sb` policy hash in the same receipt fields.
The macOS sandbox profile is unchanged. Successful rendered transport now
includes `renderer.json` in the allowed artifact set after comparing it with
the snapshot; previously the validator checked it and then rejected it as an
unexpected file.

References: [Landlock kernel ABI and restrictions](https://docs.kernel.org/userspace-api/landlock.html),
[seccomp filter semantics](https://docs.kernel.org/userspace-api/seccomp_filter.html),
[Mesa environment controls](https://docs.mesa3d.org/envvars.html).

## Linux packages

The source preparation remains pinned to Ruffle v0.7.1 revision `89a7049965f3090e706265670df309a5128233ac`. A single checked replacement
selects wgpu `MemoryHints::Manual` with fixed 4 MiB allocation blocks for CPU adapters. Ruffle's complete
feature/limit negotiation remains in use, and non-CPU adapters retain the default.
The Rust preparer records the exact replacement hashes, the package verifier
reverses it to verify the original pinned source, and the historical Python
preparer uses the same checked edit. The worker identity changes; previous
workers need rebuilding, while their saved captures remain historical evidence.

This is necessary to retain the **40 MiB kernel file-size limit**: wgpu 30's
performance policy requests a 64 MiB host-memory block, and Mesa 25.0.7 backs
all allocations with a memfd. The second diagnostic captured EFBIG from its
attempt to grow that file, followed by SIGBUS. The intermediate MemoryUsage setting also over-allocated at 4x; the final
manual setting prevents block growth. This is an allocation hint, not a promise
that every work fits; larger allocations still fail under the original budgets. No limit
was raised and no worker failure was reclassified as success.

**Confined Linux rendering currently admits native scale only.** Even the
fixed-block 4x fixture exceeds the tested Mesa configuration's backing-file
budget. `renderScale > 1` now returns `unsupported-native-render-scale` before
staging or launch; it cannot produce a Ready capture. The unconfined original
probe passes at 4x, but restricted enlarged rendering remains a separate gap.
Native-scale artwork and ActionScript bitmap operations remain available.

Packaging/loading accepts host-matching little-endian x86-64 ELF64 executable
or PIE files. It validates header version/type/architecture, bounded program
headers/segments, a nonzero entry point in an executable file-backed load segment,
and the supported dynamic interpreter when present. Shared libraries with no
entry point, corrupt/overflowing/truncated headers, foreign architectures and
Mach-O files reject. This is structural validation in addition to the explicit
host-approved binary hash, not compiler attestation. No `ldd` or loader is run
to inspect an arbitrary binary.

Bundles retain schema `ddi-danoni-worker-bundle/1`, use `platform.os = linux`
and `platform.arch = x86_64`, and inventory `worker-linux.rs` in place of
`worker.sb`. Its bytes must match the SDK's compiled policy. Changing that
policy requires repackaging; it does not require rebuilding an unchanged Ruffle
worker. Existing macOS bundles keep their format. Runtime library/driver
redistribution and complete dependency notices remain separate shipping work.

Java is a **build-time** dependency for Ruffle's playerglobal definitions.
The installed native pipeline does not invoke it. Mesa provides the **runtime**
CPU Vulkan driver for these graphics tests; the Vulkan loader/driver must be
installed or deliberately shipped by the desktop distribution. Driver selection
and `renderer.json` provenance remain the existing portable worker contract.

## Reproduction without corpus material

The transferred probe is `~/sugoijan/ddi-linux-probe-20261015/`. Read its README
and harness before using it: its launcher targets Apple's container CLI and its
Debian setup assumes a disposable container. Neither setup ran on this host.
The original source inventory and worker fingerprints were verified before
building a fresh Linux binary with Rust 1.99 and the reviewed lockfile.

This host initially had no Java or Mesa installation. Public Debian trixie
packages were hash-checked against the HTTPS package index and extracted locally;
no package setup scripts or host installation ran. A development-only bwrap
mount view supplies those Mesa libraries/manifests at ordinary system paths,
retaining the host's glibc. It is not the production confinement layer. This
also explains why those local verification commands have a wrapper whereas the
SDK and packaged worker do not. Temporary dependency/build trees were under
`/tmp/ddi-linux-env`, `/tmp/ddi-worker-target`, `/tmp/ddi-sdk-target` and
`/tmp/ddi-game-target`.

Persistent synthetic evidence is kept separately at
`~/sugoijan/ddi-linux-validation-20261015/`, including prepared sources, bundle,
fresh Linux jobs, capture receipts, test logs and environment provenance.
`development-runtime/root` retains the extracted build/runtime packages, and
`development-runtime/with-mesa` supplies the development mount view without a
system-wide installation. `original-probe` preserves the initial unconfined
synthetic results; `*-verified` captures and
`restricted-rendering-comparison-verified.json` use the final supervisor.
At the initial 2026-10-15 checkpoint the transfer was pending: no corpus
material had been read/modified and no corpus validation had run. The subsequent
dated results below supersede that gate. Historical macOS jobs/evidence are unchanged. Tests use an external
TMPDIR because the agent's temporary `/tmp/.git` guard can race the SDK's
intentional prohibition on Git-contained inputs/outputs.

Normal hosts with the required runtime packages can use the existing
`package-worker`, `verify-worker` and `capture-native` commands. Optional tests:

```sh
export TMPDIR=/absolute/external/scratch
export DDI_DANONI_WORKER_BUNDLE=/absolute/external/linux-bundle
export DDI_DANONI_WORKER_SOURCES=/absolute/external/prepared-sources
export DDI_DANONI_SYNTHETIC_FIXTURES="$HOME/sugoijan/ddi-linux-probe-20261015/fixtures"
export DDI_DANONI_SUPERVISOR_SYNTHETIC_JOB=/absolute/external/blank-job.json
cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml -- --test-threads=1
cargo test --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --no-default-features
cargo clippy --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --all-targets -- -D warnings
cargo clippy --locked --manifest-path tools/danoni-dump/rust/Cargo.toml --no-default-features --all-targets -- -D warnings
just check
```

The original-fixture integration test verifies the three exact transferred
fixture hashes before executing them through `load_bundle` and `capture`.
It checks Vulkan/Cpu/software provenance, native-scale pixel dimensions and
independently expected colours, both ActionScript bitmap reads with zero runtime
errors, and explicit pre-launch rejection of the enlarged configuration.
No synthetic SWFs, downloaded runtime libraries or third-party assets enter Git.

Final validation passed the native SDK suite with the local pinned Git/source,
installed bundle and original-renderer gates enabled; the feature-disabled SDK
suite; strict Clippy with native features and without default features; the six
worker unit tests and strict worker Clippy; formatting; and `just check`, including
wasm checks and REUSE (868 files covered). Corpus-gated test functions returned
without loading a corpus; their harness status is not corpus evidence. Native
tests ran serially because a parallel build/test load had made an existing
150 ms HTTP timeout probe expire before its first request. The final serial
suite passes without changing the timeout or network implementation.

The retained logs include intermediate failures as well as the passing runs.
The final native log is `logs/sdk-native-passed.log`; the validation summary
identifies the passing logs and their hashes. macOS execution was not rerun on
this Linux host.

## Acceptance boundaries

The independent native rendering probe passed the two-frame shape fixture at
1x and 4x, plus `BitmapData.draw`/`getPixel` on both frames, selecting Mesa
25.0.7 Lavapipe (`llvmpipe`, LLVM 19.1.7, Vulkan CPU) on x86-64 Linux 6.18.50.
The rebuilt worker also completes the bitmap capture through the public Rust
supervisor with Ready transport, the complete audit and unchanged limits.
Confinement probes independently test outside/input/runtime writes, outside
reads and symlink targets, a live TCP listener, process creation, namespace/IPC
and cross-process access, inherited descriptors, ordinary threads, resource
limits, cancellation, malformed output, reaping and missing kernel support.

Real-corpus comparison was deferred at the initial checkpoint; see the dated
continuation below. Chart extraction reads
ActionScript state. The synthetic bitmap test demonstrates pixel reads from
ActionScript; it does **not** establish that any real chart depends on those
pixels. Preserve runtime audits and compare fresh Linux captures against frozen
chart/audio/motion references before broadening real-work acceptance. Do not
reuse macOS binaries or rewrite historical absolute-path jobs.

Other pending gates: confined enlarged rendering without relaxing resource
budgets or changing pixel semantics, broader Linux URL/engine coverage, Linux hardware
rendering and actual hardware-to-software fallback, other Linux architectures,
Windows confinement/packaging, driver distribution/notices and main-app desktop
integration. Playable modes remain 5, 7, 9A and 9B. The 93-song/144-chart export
baseline remains historical and includes retired 7i; no new counts are claimed.

## Transferred-corpus validation, 2026-10-16

This continuation starts at `644171a`. The transferred corpus is now available
at `~/sugoijan/DanOni/`; it is read-only evidence. Fresh jobs, cache copy,
captures, converted study outputs, managed libraries and logs are under
`~/sugoijan/ddi-linux-validation-20261016/`. The installed ELF bundle and local
Mesa environment are the hash-verified `bundle-final` and `development-runtime`
from the preceding Linux review. No worker, sandbox, reviewed maximum budget,
profile signature or production admission rule changed during this validation. Profile
jobs use the existing 300-second whole-job allowance; per-capture kernel
CPU/file/descriptor/core bounds and memory accounting remain in force.

### Integrity and comparison boundary

`checks/verified-*.json` retain the checks against recorded source/context
bindings, resource hashes, frozen normalized-manifest bindings, referenced
music hashes and cache-body records. `validation-summary.json` inventories the
final receipts and logs. The original cache/export/registry guards match the
recorded hashes before and after the run. This verifies the files actually used;
it is not a checksum certification of the entire transferred directory.

Historical macOS jobs and their absolute paths remain byte-identical. New jobs
map namespaces to the Linux transfer. The pinned six-chart Kouhaku job uses a
fresh shared movie/config directory so every config resolves the explicit movie
binding. The Linux copy of the export registry changes only its root path; the
selected saved-work gate reads the original accepted source/reviews/media.
No accepted export is regenerated, promoted or replaced.

Each fresh automatic result is compared before starting the next work. The
existing Rust `profiles_corpus` gate checks every source/gameplay field, lanes,
native note and hold frames, seconds, speed/boost/retained events, source clocks,
independently proved audio origin and measured motion. Worker-iteration changes
must explain the exact 735-sample output-cursor displacement without moving
native logical time. Canonical PCM retains the existing maximum one-unit signed
16-bit difference. Generated SSC is parsed by the real chart crate; all endpoints
and x1/x2/x3 scroll checks run. Source-only jobs reuse their existing reviewed
title/artist metadata for this comparison; no note/audio timestamps or extraction
recipes are supplied. Crow also runs independently from its public URL without
those metadata overrides. The checks use the source-time fallback grid; they do
not accept new musical BPM/phase decisions.

Build identities, capture timestamps, hash-bound snapshot changes and relocated
evidence paths are recorded separately from data comparisons. Pinned-reference
cases now declare the exact changed/added worker source paths while binding both
complete identities; the current identity must equal the SDK's reviewed worker.
An undeclared renderer-source change rejects in an explicit negative probe.
No note, clock, motion or audio-origin field is exempted by that declaration.

The raw-audit inventory is explicit about the existing optional-colour review.
Across 153 capture-role observations in the positive native jobs, 21 retain one
runtime-error diagnostic for missing `iro.txt`, across March, Riguru, Saigo,
Regret Enemy and the seven-key Danmaku work. These are the pre-existing exact
movie-hash-bound HTTP-404 reviews: response status, URL, body hash, matching
fetch-denial/error pair and the normalized retained effect are checked. All
21 raw diagnostics remain in the evidence; no new runtime-error waiver is added.
Every other runtime error, halted/truncated audit or unknown failure rejects.
These probe counts include unsuccessful motion proposals and are not chart
counts. The bitmap-rendered Maid captures have zero runtime errors; its
renderer-free error is a separately preserved rejection.

### Representative and expanded results

The representative gate passes three works / three charts / 1,580 taps /
30 holds / 1,640 endpoints. Expansion then passes **28 fresh works /
35 charts / 19,783 taps / 1,134 holds /
22,051 endpoints** from the existing automatic-profile table, with
canonical PCM difference at most 1 and maximum
source-time SSC round-trip error 2.84217094304e-11 ms.

Including rendered Maid, the separately accepted hold-intro selections and
Kouhaku gives **31 distinct fresh native works / 44 charts**:
41 five-key and 3 seven-key charts,
24,492 taps / 1,180 holds / 26,852 endpoints.
Repeated Crow imports and the reused representative March capture count once.
The saved HTML5 gate adds one separate work / two converted charts (9A and 9B),
which are not counted as fresh native execution. 344
used transferred files passed their recorded-hash checks.

| Representative | Actual Linux gate | Result |
| --- | --- | --- |
| Crow | Renderer-free URL import; one 5-key chart | Frozen reference equal except snapshot hash; full snapshot fields equal except build identity |
| 2004 Kouhaku | Six fresh confined captures; five songs / six 5-key charts | Every full snapshot equals the frozen snapshot except build identity; pinned references, notes/events, origin shifts, PCM, measured motion and parsed SSC pass |
| March | Fresh seven-key automatic profile | Complete source/chart/audio/motion/SSC comparison passes |
| Maid | Native-scale Vulkan/software rendering; one 5-key chart | Four capture roles select Mesa CPU Vulkan with zero runtime errors; 526 taps / three holds match the independent frozen chart |
| DarisCradle selected rows 0 and 2 | Existing hold-intro contract; two 5-key charts | 1,690 taps / 43 holds / 1,776 endpoints; full reference, PCM, derived-input, motion and parsed SSC gate passes |
| ふしぎなひかり (`vh-hikari-9ab`) | Saved HTML5 evidence converted on Linux; one 9A and one 9B chart | Two parsed charts / 241 judged heads, exact SSC semantics, preview, source ratings, scroll, accepted music and banner bytes |

The 9A/9B result is conversion/replay of previously captured HTML5 evidence.
It is not fresh HTML5/browser execution, native Flash discovery of those modes,
or an end-to-end HTML5 URL import. The copied registry and explicit work filter
keep retired 7i outside this gate. Accepted audio and artwork are supplied
byte-identically by this bridge; this gate does not re-encode them.

All fresh native works are listed below; DarisCradle means the explicitly
selected reviewed rows, and Kouhaku is the pinned six-selection contract.

| Work | Modes × charts | Taps | Holds |
| --- | --- | ---: | ---: |
| `reku-045-noname08-noname` | 5 × 2 | 1,500 | 6 |
| `reku-047-7keyfestival-march` | 7 × 1 | 398 | 4 |
| `reku-048-7keyfestival-gkbr-riguru` | 7 × 1 | 529 | 6 |
| `reku-050-dosmf08-last-saigo` | 5 × 1 | 325 | 0 |
| `reku-051-dosmf08-60s-wind-wind` | 5 × 1 | 868 | 3 |
| `reku-052-dosmf08-regret-enemy` | 5 × 1 | 1,019 | 0 |
| `reku-18b-7keyfestival-syoujosyuu-danmaku` | 7 × 1 | 540 | 109 |
| `reku-023-alice-alice` | 5 × 1 | 631 | 23 |
| `reku-025-doxf03-higasinokuni-east` | 5 × 1 | 620 | 76 |
| `reku-026-doxf03-phantasm-brigade` | 5 × 1 | 765 | 152 |
| `reku-027-doxf03-purein-asia` | 5 × 1 | 583 | 61 |
| `reku-028-doxf03-last-judg` | 5 × 1 | 698 | 91 |
| `reku-029-doxf03-master-spark` | 5 × 1 | 853 | 57 |
| `reku-030-doxf03-theendof1000-years` | 5 × 1 | 616 | 21 |
| `reku-031-doxf03-meltyblood-meltyblood` | 5 × 1 | 334 | 31 |
| `reku-032-doxf03-voyage-voyage` | 5 × 1 | 419 | 44 |
| `reku-033-doxf03-zankokuka-yamada` | 5 × 1 | 393 | 26 |
| `reku-034-doxf03-snow-dousai` | 5 × 1 | 435 | 12 |
| `reku-035-team-9-yukarin-side-yukarin` | 5 × 4 | 1,683 | 202 |
| `reku-036-team-9-9-side-bakasuta` | 5 × 4 | 1,504 | 52 |
| `reku-037-nosound-mistia` | 5 × 1 | 256 | 8 |
| `reku-038-night-knights` | 5 × 1 | 651 | 1 |
| `reku-039-dolls-dolls` | 5 × 1 | 759 | 47 |
| `reku-040-sls-sls` | 5 × 1 | 1,694 | 58 |
| `reku-004-crow-crow` | 5 × 1 | 656 | 23 |
| `reku-059-moon-m` | 5 × 1 | 122 | 0 |
| `reku-062-doxf04-sacrifice-sac` | 5 × 1 | 401 | 20 |
| `reku-073-river-s` | 5 × 1 | 531 | 1 |
| `reku-010-maid-maid` | 5 × 1 | 526 | 3 |
| `reku-053-dosmf08-dariscradle-slashspeed-maspa` | 5 × 2 | 1,690 | 43 |
| `kouhaku` | 5 × 6 | 2,493 | 0 |

These are tested study outputs, not new accepted export counts. Playable modes
remain 5, 7, 9A and 9B. The historical 93-song / 144-chart export baseline still
includes retired 7i and remains unchanged.

### Audio and artwork differences investigated

The Crow URL import's native reference is exactly equal after comparing and
recording its changed `sourceSnapshotSha256`. Its normalized stereo 44.1 kHz WAV
has the same 6,326,784 sample frames and differs by at most one signed 16-bit unit
(75 samples differ). Its SSC differs only by the absent `#BANNER` declaration:
the historical job references a Mac-only Hiragino font outside the corpus, and
the fresh job explicitly supplies no host font. All note/timing/preview data matches.

**Ogg playback is not bit-identical across these builds.** Decoding both Crow
Ogg files with the same Linux SDK gives maximum sample difference 2,927 and RMS
51.3207 signed units. This is a measured signal difference, not provenance.
Re-encoding the exact frozen WAV on Linux retains a difference from the frozen
Ogg (maximum 2,927, RMS 51.0375), isolating most of the difference to the codec
stage. The one-unit source-rounding changes contribute a smaller RMS 5.3943
between Linux encodes. Encoding the fresh source again on Linux reproduces its
Ogg bytes exactly. Both Linux codec checks retain equal rate/channels/frame
counts, measured zero-sample lag and zero spread. Their signal-to-source error
is close to the frozen encode (RMS about 741.1 in each case).

The pinned bundled aoTuV/Lancer C encoder has architecture-dependent SIMD paths;
compiler/architecture effects are a plausible explanation, not a proved unique
cause. No codec threshold or source-reference tolerance was changed to hide the
difference. See `checks/crow-parity.json` and `checks/crow-codec-isolation.json`.
Cross-platform Ogg bit determinism remains unproved; this does not change the
exact chart/origin result or replace the accepted media.

Maid's title was additionally captured at the exact frozen iterations 1, 30,
60 and 100. Its 320×240 frames differ from the frozen Metal frames: maximum
channel difference 4–50, mean absolute channel difference 0.05385–0.70139 on the
0–255 scale. The two captured state graphs differ in 11 movie-clip `seed` values.
A second confined Linux capture changes eight such state values and 47 pixels
in frame 30, while its other three image files are byte-identical to the first
Linux capture. These observations do not establish exact artwork parity or a
unique cause for every raster difference. All three captures retain zero runtime
errors; full differences and hashes are in `checks/maid-artwork-comparison.json`.
The historical title PPMs have no pre-transfer hash inventory in their run
receipt; their hashes are recorded as observed, not claimed as verified against
a prior transfer checksum. The chart/reference and source integrity checks use
their separately recorded hashes.

The separate complete Maid chart comparison passes despite those title-state
and image differences. Its fresh renderer-free control still rejects with
`native-profile-render-required`; enabling a trusted explicit renderer resolves
the drawing operations without discarding runtime errors. The default profile
renderer policy was not broadened. Calling `BitmapData.draw` is not treated as
evidence that chart arrays depend on the pixels; no such dependency is inferred here.

### Offline URL installation, reopening and resume

Crow's fresh public-URL job discovers its embed and completes two missing-resource
recovery rounds from the private transferred-cache copy, then prepares and
installs one song / one chart. Source receipts report **zero requests, wire
bytes and decoded transfer bytes**. The converted Ogg/SSC and source references
are inventoried; each installed asset is read and verified after reopening.

The real `import_job_corpus` gate now runs on supported Linux as well as macOS.
A second fresh Crow job cancels at `Progress::Planned`, before installation,
leaves the library empty and retains its hash-bound candidate. A new invocation
opens the library and resumes using only the persisted candidate and store,
without worker, cache or font services. It installs the same content digest as
the first job, and the next resume returns `Unchanged`. Separate CLI invocations
reopen the first store, repeat its resume as unchanged, reject a corrupted Ready
hash and preserve the exact catalog bytes. See `resume-check/report.json` and
`checks/library-reopen-cli.json`.

### Rejections and remaining scope

Cynthia with an explicit software-renderer request still returns
`unsupported-native-profile` before creating worker output. Its unreviewed
initialization is not admitted by proving the renderer works.

DarisCradle with all three menu rows remains non-Ready at selector 1:
`invalid-native-projection: Need split, compiled and zero-counter snapshots`.
This matches the later frozen hold-intro failure. The older combined case table
retains its earlier `native-profile-motion-unproved` result; it predates the
hold-intro extension. The first fresh validation expectation used that stale
code; its diagnostic is retained, then checked against the newer documented
contract. Row 1's source-bank rewrite is not waived. Explicitly selected rows
0 and 2 use their existing reviewed contract and independent reference gate;
row 2's reference is a previously saved independent data projection, not a
historical Flash/device timing oracle.

Checks pass the native SDK suite with installed-worker/synthetic gates enabled,
the feature-disabled SDK suite, strict Clippy in both configurations, formatting,
and `just check` including REUSE. Corpus-gated functions without their optional
environment variables are skipped during ordinary regression; their harness
status contributes no works or charts to the counts above. The real gates were
invoked separately with the recorded case/job files and emitted their receipts.
The initial REUSE invocation lacked its removed temporary tool cache; restoring
the existing public development tool and rerunning the check resolves that
setup failure. macOS execution was not rerun on this Linux host.

Remaining gates include broader URL/collection and engine coverage, fresh HTML5
runtime acquisition, confined enlarged rendering, hardware rendering/fallback,
other Linux architectures, Windows confinement, driver redistribution/notices,
main-app desktop integration and musical-quality review. The current run does
not certify cross-platform Ogg bytes or exact artwork reproduction.

### Reproducible commands

The retained `scripts/` and `jobs/` record the execution and hash-check commands.
They are research helpers; installed imports remain native Rust. Use a fresh
external output directory for every new capture, profile or library. Do not
rewrite jobs or evidence in the transferred corpus.

```sh
DDI_CORPUS="$HOME/sugoijan/DanOni"
DDI_REVIEW="$HOME/sugoijan/ddi-linux-validation-20261016"
DDI_SYNTHETIC="$HOME/sugoijan/ddi-linux-validation-20261015"
DDI_BIN="$DDI_SYNTHETIC/bin/ddi-danoni-export"
DDI_BUNDLE="$DDI_SYNTHETIC/bundle-final"
DDI_MANIFEST=tools/danoni-dump/rust/Cargo.toml
export TMPDIR="$DDI_REVIEW/scratch"

# A fresh confined profile, using already verified transferred bindings.
"$DDI_REVIEW/scripts/with-mesa" "$DDI_BIN" profile-native \
  "$DDI_BUNDLE" "$DDI_REVIEW/jobs/maid-rendered-profile.json" \
  "$DDI_REVIEW/reproduce-maid"

python3 "$DDI_REVIEW/scripts/rebind-profile-case.py" \
  "$DDI_REVIEW/checks/representative-profile-cases.json" reku-010-maid-maid \
  "$DDI_REVIEW/reproduce-maid" "$DDI_REVIEW/reproduce-maid-cases.json"
DDI_DANONI_PROFILE_CASES="$DDI_REVIEW/reproduce-maid-cases.json" \
DDI_DANONI_PROFILE_RECEIPT="$DDI_REVIEW/reproduce-maid-parity.json" \
  cargo test --locked --offline --manifest-path "$DDI_MANIFEST" \
  --test profiles_corpus \
  automatic_profiles_match_frozen_charts_and_feed_the_game_parser -- --nocapture

# Read-only semantic replay of the actual fresh Linux acceptance set.
DDI_DANONI_PROFILE_CASES="$DDI_REVIEW/checks/combined-fresh-linux-cases.json" \
DDI_DANONI_PROFILE_RECEIPT=/absolute/fresh-parity-receipt.json \
  cargo test --locked --offline --manifest-path "$DDI_MANIFEST" \
  --test profiles_corpus \
  automatic_profiles_match_frozen_charts_and_feed_the_game_parser -- --nocapture

DDI_DANONI_NORMALIZATION_CASES="$DDI_REVIEW/checks/kouhaku-normalization-cases.json" \
DDI_DANONI_NORMALIZATION_OUTPUT=/absolute/fresh-kouhaku-normalized-output \
  cargo test --locked --offline --manifest-path "$DDI_MANIFEST" \
  --test normalized_corpus -- --nocapture

# Saved 9A/9B conversion only; no browser/SWF execution in this gate.
DDI_DANONI_EXPORTS="$DDI_CORPUS/exports" \
DDI_DANONI_EXPORT_REGISTRY="$DDI_REVIEW/jobs/registry-linux.json" \
DDI_DANONI_WORK_IDS=vh-hikari-9ab \
  cargo test --locked --offline --manifest-path "$DDI_MANIFEST" \
  --test local_corpus complete_songs_match_accepted_exports -- --nocapture

DDI_DANONI_IMPORT_URL_JOB="$DDI_REVIEW/jobs/crow-url.json" \
DDI_DANONI_IMPORT_JOB_ROOT=/absolute/fresh-cancel-resume-review \
DDI_DANONI_WORKER_BUNDLE="$DDI_BUNDLE" \
DDI_DANONI_IMPORT_EXPECTED_READY="$DDI_REVIEW/runs/crow-url" \
  "$DDI_REVIEW/scripts/with-mesa" cargo test --locked --offline \
  --manifest-path "$DDI_MANIFEST" --test import_job_corpus -- --nocapture

"$DDI_BIN" verify-store "$DDI_REVIEW/library"
"$DDI_BIN" resume-import "$DDI_REVIEW/runs/crow-url/import-candidate.json" \
  "$DDI_REVIEW/library"
```

On this host, running the real cancel/resume test at a fresh sibling review path
also requires adding that chosen path as writable in the development-only Mesa
mount view. Production hosts with system runtime packages need no such wrapper.
The exact actual build/check commands, temporary Cargo target directories and
explicit low-debug build settings are retained in the review logs/scripts.
