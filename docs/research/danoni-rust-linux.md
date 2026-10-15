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
`~/sugoijan/DanOni/` is not used while its transfer is pending. No corpus material was read or
modified and no corpus validation was run. Historical macOS jobs/evidence are unchanged. Tests use an external
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

Real-corpus comparisons remain pending the transfer. Chart extraction reads
ActionScript state. The synthetic bitmap test demonstrates pixel reads from
ActionScript; it does **not** establish that any real chart depends on those
pixels. Preserve runtime audits and compare fresh Linux captures against frozen
chart/audio/motion references before broadening real-work acceptance. Do not
reuse macOS binaries or rewrite historical absolute-path jobs.

Other pending gates: confined enlarged rendering without relaxing resource
budgets or changing pixel semantics, fresh URL-to-library/corpus parity on Linux, Linux hardware
rendering and actual hardware-to-software fallback, other Linux architectures,
Windows confinement/packaging, driver distribution/notices and main-app desktop
integration. Playable modes remain 5, 7, 9A and 9B. The 93-song/144-chart export
baseline remains historical and includes retired 7i; no new counts are claimed.
