# Rust native worker preparation and bundles

For the subsequent x86-64 Linux implementation, confinement requirements and
validation scope, see [the Linux handover](danoni-rust-linux.md). The dated
macOS-only results below remain historical evidence.

Implemented 2026-10-13 in `tools/danoni-dump/rust/`, behind `native`.
This completes the Python preparer's replacement and adds a library API for
loading a verified, installed worker. The [supervisor](danoni-rust-supervisor.md)
remains the only movie executor. Building a worker is a maintainer task; desktop
users will receive it with the app and need no Git, Cargo, Java or Python.

## Trust and source preparation

`worker_build::prepare_sources(upstream, new_output, checkpoint)` reads the fixed
Ruffle [v0.7.1 commit](https://github.com/ruffle-rs/ruffle/tree/89a7049965f3090e706265670df309a5128233ac)
`89a7049965f3090e706265670df309a5128233ac` from a local Git
object database. It reads raw blobs through `git cat-file --batch`, with object
replacement, hooks, filesystem monitoring, user Git configuration and lazy
network fetching disabled. The pinned commit must already exist and the selected
blobs must already be cached. HEAD, mutable checkout files and untracked source
files do not choose the compiled source. No upstream files are edited.

Selected paths are `core/`, `render/`, `swf/`, `flv/`, `wstr/`, `video/`,
`tools/asc/`, plus root `Cargo.toml`, `Cargo.lock`, `LICENSE.md` and `README.md`.
The three existing hook appends and our two inspection modules go into the
owned copy. Only regular tracked blobs are allowed; Git executable modes are
preserved on Unix. Paths, object responses, counts and sizes are validated:
10,000 entries, 32 MiB per blob and 256 MiB total source bytes maximum.

Preparation starts with a fresh directory outside Git and publishes its
`manifest.json` last. An existing output is never overwritten. Cooperative
cancellation or an error removes only the newly owned partial directory and
kills/reaps its Git object reader. Checkpoints occur between source files; this
build-time helper does not apply the movie supervisor's process budgets. It
executes Git only, never Cargo, Python, a movie or acquired work scripts.

```text
sources/
  upstream/                         pinned selected files, with explicit hooks
  worker/
    Cargo.toml                      template with ../upstream path dependencies
    Cargo.lock                      byte-identical reviewed worker lockfile
    build-identity.json             existing worker protocol identity
    src/{main,audio,navigator}.rs    byte-identical reviewed host files
  licenses/DDI-MIT.txt
  manifest.json                     source readiness/provenance
```

Relative Cargo paths let the source tree move before compilation. Build artifacts
are separate from this inventory: default Cargo output is `worker/target/`.
Additional source files, including an untracked `worker/build.rs`, prevent
packaging. Symlinks are rejected, including an output-directory symlink named
`worker/target`.

The source manifest schema is `ddi-danoni-worker-sources/1`, producer
`rust-worker-sources/1`. Fields are:

- `build`: the exact `supervisor::expected_build_identity()` object.
- `upstream`: `{url, revision, files}`; `files` maps each original relative
  path to `{blob, mode, sha256}`. `blob` is the Git SHA-1 object identifier;
  `sha256` hashes original file bytes before hooks. Neither is a note frame.
- `hooks`: `{path, appendSha256}[]`, identifying each appended UTF-8 hook.
- `manifest`: `worker/Cargo.toml`.
- `artifacts`: `{path, sha256, bytes}[]`, sorted by relative path and covering
  all prepared files except the manifest itself. Sizes are integer bytes.
- `scope`: preparation only; compilation is an explicit trusted build step.

The existing `build-identity.json` schema is intentionally unchanged. Its
historical `prepare.py` fingerprint remains part of the reviewed recipe identity;
Rust does not execute or copy that script. Changing hooks, templates or lockfiles
still changes the expected identity and invalidates older capture compatibility.

## Packaging and installed-worker API

`worker_build::package_worker(sources, executable, executable_sha256,
new_bundle, checkpoint)` takes an explicitly selected compiler output. It verifies
the complete prepared inventory, expected host/hook source bytes, template,
lockfile, hook append/original hashes and upstream revision. It snapshots each
verified input once, so a subsequent filesystem change cannot substitute bytes
while they are copied. The caller supplies the actual executable SHA-256.
Packaging accepts a thin native Mach-O executable header matching the current
macOS host's architecture; transport validation still belongs to the supervisor.
It neither compiles nor runs this executable.

```text
bundle/
  ddi-ruffle-probe                   native executable, mode 0500 on Unix
  build-identity.json
  worker.sb                         exact reviewed confinement policy
  licenses/
    DDI-MIT.txt
    Ruffle-LICENSE.md                full upstream licence/notice document
    upstream/swf/LICENSE-{MIT,APACHE}
    ...                             other named upstream licence/notice files
  provenance/
    Cargo.lock
    source-manifest.json
  manifest.json                     bundle readiness marker
```

The bundle schema is `ddi-danoni-worker-bundle/1`, producer
`rust-worker-package/1`. Fields are:

- `platform`: `{os, arch}`, using Rust host names (`macos`, `aarch64`, etc.).
- `build`: exact reviewed worker identity.
- `worker`: `{executable, sha256, buildIdentity, buildIdentitySha256}`;
  filenames are relative to this bundle and fixed to the names above.
- `sandboxSha256`: SHA-256 of the exact tracked `worker.sb` bytes.
- `sourceManifestSha256`: SHA-256 of the copied source receipt.
- `artifacts`: sorted `{path, sha256, bytes}[]`, covering every bundle file
  except `manifest.json`, including required runtime, licence and provenance files.
- `scope`: an operator-selected local bundle; hashes do not attest a compiler or
  replace an installer signature.

`worker_build::load_bundle(installed_bundle, checkpoint)` checks schema,
platform, expected identity, complete artifact coverage, absence of symlinks and
unexpected files, every content hash, policy and provenance hash. It returns the
existing `supervisor::WorkerSpec` with absolute contained paths and hashes. The
supervisor independently checks those bound bytes again before staging/execution.

```rust,ignore
let worker = worker_build::load_bundle(&host_installed_bundle, &mut checkpoint)?;
let outcome = supervisor::capture(
    &bound_capture_request, &worker, &asset_source, &fresh_capture_output,
    &mut checkpoint,
)?;
```

The bundle location and authority belong to the host; a song URL or fetched
manifest cannot choose them. Verification is integrity against trusted installed
metadata, not authentication of an arbitrary downloaded binary. Building from
reviewed source is distinct from executing an acquired movie inside confinement.
An app distributor must separately attest/sign its installation as appropriate.

These are local development bundles. The full upstream licence document,
selected source notices and DDI licence are preserved, but this is not yet an
audited redistribution bill of materials. Before shipping the desktop worker,
reconcile notices with the actual worker `Cargo.lock` and embedded assets,
including the Noto font's licence ([pinned font recipe](https://github.com/ruffle-rs/ruffle/blob/89a7049965f3090e706265670df309a5128233ac/core/assets/update-font.sh)). This does not change third-party songs' strictly
local status. The currently tested capture platform remains macOS; other-platform
confinement and universal-binary installation contracts are pending.

## Commands

Build the SDK CLI normally, then prepare sources outside Git:

```sh
cargo build --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust
git clone --depth 1 --branch v0.7.1 --filter=blob:none --sparse \
  https://github.com/ruffle-rs/ruffle.git /tmp/ruffle-source
git -C /tmp/ruffle-source sparse-checkout set core render swf flv wstr video tools/asc
target/danoni-rust/debug/ddi-danoni-export prepare-worker \
  /tmp/ruffle-source /tmp/ruffle-prepared
cargo build --locked --manifest-path /tmp/ruffle-prepared/worker/Cargo.toml
```

Java must be available to the reviewed Ruffle playerglobal build. Its compile
scripts are upstream library code, not acquired songs. Compilation uses the
independent reviewed worker lockfile; the game and export SDK lockfiles do not
change. No bit-for-bit binary reproducibility is claimed across paths/toolchains.

`package-worker JOB.json NEW_BUNDLE` accepts this host-controlled job schema:

```json
{
  "schema": "ddi-danoni-worker-package-job/1",
  "sources": "/tmp/ruffle-prepared",
  "executable": "/tmp/ruffle-prepared/worker/target/debug/ddi-ruffle-probe",
  "executableSha256": "<actual 64-character lowercase SHA-256>"
}
```

Paths may instead be relative to the job file; unknown job fields are rejected.
Obtain the executable hash with `shasum -a 256`. Package and inspect its returned
worker specification:

```sh
target/danoni-rust/debug/ddi-danoni-export package-worker /tmp/package-job.json /tmp/worker-bundle
target/danoni-rust/debug/ddi-danoni-export verify-worker /tmp/worker-bundle
```

Use that specification as the trusted `worker` in the existing
[capture job](danoni-rust-supervisor.md), or use both library APIs directly.
Git/Cargo/Java are build-time tools; capture, normalization, audio and SSC
preparation use the compiled Rust APIs and installed worker.

## Local validation, 2026-10-13

Evidence is local at `~/Music/DanOni/reviews/rust-worker-build-20261013/`.
The fresh pinned clone/prepared source are in scratch. The source receipt has
1,334 original blobs and 1,343 prepared artifacts (11,270,817 bytes).
The rebuilt worker hash is
`b45b079ee0d4bb2bab72aac60f56384e7e54becaa2d9d12a44c23026640fb5c0`.

Seven fresh chart captures (Higan plus all six Kouhaku selections) have equal
parsed snapshots and byte-identical PCM compared with the previous verified
worker. Four original synthetic cases also match stored state/key behavior and
PCM/pixels. JSON object order can vary, so snapshots are compared structurally;
PCM and PPM files are compared byte-for-byte. Truncated graph/audit and looping
fixtures keep their expected failure outcomes, without a ready marker.

The downstream gate compares original references, decoder origins, PCM and
real-parser SSC endpoints/scroll behavior. All 3,073 endpoints and 57 x1/x2/x3
checks pass for two works / seven charts / six songs, with zero origin corrections
and at most one PCM unit of difference against the independent baseline.
Accepted exports and the registry remain unchanged.

The independent confinement fixture now uses a host-injected input symlink to
probe resolved-path denial without racing the output monitor. A separate hostile
output-symlink case verifies early termination and no readiness. Production policy
and budgets are unchanged.

The tests also cover dirty/symlinked upstream worktree independence, no overwrite,
missing/nonrepository source, cancellation cleanup, source changes, unexpected
build scripts, forged worker-source inventories, changed bundle bytes, missing
inventory coverage, symlink escapes, forged policies and binary/platform mismatch.
An optional original blank SWF exercises `load_bundle` directly into the real
restricted supervisor. Synthetic Mach-O header fixtures used by integrity tests
are never executed.

The final native suite passes 112 tests, the pure suite 78, and both feature
configurations pass Clippy and formatting. `just check` passes, including REUSE
coverage for all 722 files. Local `validation-summary.json` records these checks
and hashes.

Set `DDI_DANONI_RUFFLE_SOURCE` to a complete local pinned object cache and
`DDI_DANONI_WORKER_BUNDLE` to the built bundle to run these optional checks with
`cargo test --test worker_build`. Ordinary tests never fetch upstream sources.

Next: reproducible selector/capture orchestration and the three structural
selector proof gaps; then the bounded resource-request loop and public URL
acquisition. Native tempo-quality review, HTML5 capture and desktop library
integration remain separate milestones.


The subsequent [2026-10-14 orchestration slice](danoni-rust-orchestration.md)
now executes explicit selector plans through this installed bundle API, with
portable replay and two of the three structural proof gaps closed. This does
not add automatic URL acquisition or change the worker/notice contract above.


## Structured request worker, 2026-10-14

The [dependency slice](danoni-rust-dependencies.md) adds method/body/header and
resolved logical URL evidence to denied fetches, without networking or a new
sandbox permission. Its navigator fingerprint and resulting build identity
change. Source preparation/packaging APIs and the pinned Ruffle revision,
lockfile and sandbox policy remain unchanged. The new local bundle is under
`~/Music/DanOni/reviews/rust-dependencies-20261014/bundle/`; old bundles are retained
but current supervision requires the updated identity. Historical bound saved
captures remain usable by passive adapters; only the new structured evidence
can drive dependency resolution. No compiler/toolchain is added to import time.


## Linux CPU allocation policy, 2026-10-15

The [Linux continuation](danoni-rust-linux.md) adds one exact, reversible source
replacement in `render/wgpu/src/backend.rs`: CPU adapters request wgpu's `Manual` hint with fixed 4 MiB blocks; other adapters keep the default. Ruffle retains all feature
and limit negotiation. `rewrites` in the source manifest binds the original and
replacement expression hashes, and packaging validates both the exact edit and
the recovered original source hash. The historical Python preparer uses the same
edit, so its fingerprint changes the expected worker identity. Rebuild old worker
packages; previous captured evidence remains historical. This avoids a default
64 MiB Lavapipe memfd allocation exceeding the unchanged 40 MiB kernel file limit.
