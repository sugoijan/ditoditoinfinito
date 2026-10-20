# Desktop DanOni URL importer

The optional `danoni-importer` feature builds an isolated native importer and
its pinned extraction worker beside the app. `danoni` remains an alias. The
browser and Windows builds keep Add songs' file workflow and do not run this
preparation. Supported native hosts are macOS and x86_64 Linux. The normal game
binary links only the pure `ddi-importer-protocol`, URL validation and SHA-256;
the acquisition SDK runs in the separate helper process. The helper and SDK
are root workspace members sharing `Cargo.lock`; each requires Rust 1.99. The
generated upstream Ruffle worker retains its separate workspace and pinned lock.

The URL workflow includes the reviewed 2004 Kouhaku multi-song collection:
both its HTML page and direct SWF use the same exact-content profile and the
existing pinned extraction proofs. It prepares all five songs and six charts,
including Tougenkyo Hard. HTML imports use the discovered work-page title as the
pack label (a unique matching heading can refine the document title); direct
SWFs and older unnamed imports retain `DanOni imports`. Reimport the HTML to
update an existing import's pack label while preserving its song IDs. See
[URL admission](../research/danoni-rust-url-native.md) for selector scope.

Add songs → DanOni asks only for the work URL. Desktop imports are explicit,
bounded single-work downloads: they do not prompt for a site decision or fetch
`robots.txt`. Redirects and dependency requests retain public-address checks,
request/byte/time limits and per-origin pacing. Download receipts identify this
as `user-initiated` acquisition, without claiming a site-terms review or robots
evaluation. The SDK's default research/batch policy still respects robots and
does not silently reuse responses acquired under the desktop policy.

The panel shows the current task and its real completed/total counts when known.
A moving activity bar continues between helper messages, including tasks without
counts; reduced motion keeps it stationary. Counts describe the current task,
not an estimated percentage of the whole import. The cached-only switch is not
shown; imports automatically reuse available cached files and download missing
ones. Cancellation and import reports remain available.

## Build and package

Build prerequisites are Rust 1.99, Python 3.11+, Git, a C/C++ compiler, CMake,
pkg-config and a JDK (Ruffle's playerglobal build). Linux also needs the usual
ALSA/udev development packages. A first feature build obtains the pinned public
Ruffle release and locked Cargo inputs. It never obtains songs or a runtime from
a pasted song URL. Later builds reuse an external cache. The builder limits
Cargo to two jobs for the SDK/helper and one for the Ruffle worker (including
LLVM code generation), disables LTO for helpers, and checks every child process
against 2 GiB RSS. Default/web/Windows checks invoke no helper preparation tools.

```
cargo run -p xtask -- gen-songs
cargo run -p xtask -- gen-noteskins
cargo build -p ditoditoinfinito --features danoni-importer
python3 tools/danoni-importer/build.py package --app target/debug/ditoditoinfinito --output /tmp/ddi-package
```

The app keeps video enabled by default. A deliberately smaller package can use
`--no-default-features --features danoni-importer`; this forwards the app's video
choice to desktop. Video-enabled packages require the decoder helper
`crates/desktop/build.rs` staged beside the app (`video/ddi-video-helper`, whose
sandbox self-test the packager runs), its notice and FFmpeg description, the
exact FFmpeg and zlib source archives, and the shims, scripts and helper
crates it is rebuilt from (`video/helper-source/`, the LGPL's relinking
route). The package command refuses an incomplete payload.

The package includes the app, bundled assets and indexes, credits/licenses,
optional video payload, and `danoni-importer/<id>/`. Move the whole directory.
It needs no Python, Java, Cargo, FFmpeg command, source checkout, `host.json`,
host-executable hash, or maintainer-specific path at runtime. The importer helper
and worker are ordinary native executables. Linux system prerequisites include
ALSA/udev for the app and the Vulkan loader plus a Mesa software ICD for the
worker. Built packages target their compiler/OS baseline; a build on a newer
glibc is not an older-glibc compatibility claim. macOS uses system frameworks.
Non-system native dynamic dependencies are refused by the package audit.

`--danoni-bundle-info` prints the app's compiled content ID/manifest digest
without opening a window or audio device. `--danoni-importer-check` additionally
checks the actual app/client anonymous-pipe handshake and verified empty catalog.
Both can be run from a moved package with an empty PATH. The helper's
`--check-bundle` independently checks its installed runtime. The IPC control
channel is a bounded binary/JSON protocol over anonymous pipes; the SDK still
uses its own receipted capture/normalization evidence files internally.

## Build cache and integrity

`crates/desktop/build.rs` invokes the build-only Python driver. It selects only the
SDK and helper packages in one build with the root `importer` profile (unwinding,
no LTO, optimization level 1, 256 codegen units), with optimization disabled
for the SDK itself to stay under the 2 GiB compiler-memory limit on Linux.
Its codec and rendering dependencies remain optimized. Their shared external
target directory and the worker's separate target directory avoid the parent app's
target lock and jobserver; the child build never selects desktop. Root release
settings still apply to the game. The root lock participates in the build-cache
fingerprint and the SDK's compiled runtime identity. The default cache is
`~/.cache/ditoditoinfinito/danoni-importer`.
`DDI_IMPORTER_BUILD_CACHE`, `DDI_IMPORTER_UPSTREAM`, `DDI_IMPORTER_WORKER_SOURCES`
and `DDI_IMPORTER_WORKER_BUNDLE` are optional **build-time** cache overrides.
They are not consulted by a running app. Sources/bundles are verified against
pinned Git objects, reviewed hooks, exact source preparation and platform IDs.

A bundle manifest inventories every helper/runtime/notice byte by size and
SHA-256. Its immutable ID and manifest digest are compiled into desktop.
Discovery uses only the app executable's sibling `danoni-importer/<id>/`.
Changed, missing, extra or symlinked files are rejected. The helper's own
runtime lives at `../runtime` relative to its `bin/` directory. No source,
build-cache or home directory is compiled into runtime discovery. Assets are
staged beside native build artifacts as well, including arbitrary Cargo target
directories; a package always uses its sibling assets.

`DDI_IMPORTER_TEST_ONLY=1` is a deliberate debug-only exception for cheap
Clippy/unit IPC tests: no runtime is prepared and discovery remains disabled
with empty bindings. It is refused for release/custom profiles; such a binary
cannot pass packaging. CI has separate cheap fixture tests and a real native
package lane (main pushes/manual runs) that installs build prerequisites,
prepares the real worker/helper, relocates the package and checks it with an
empty PATH. Third-party generated workers/runtimes are never committed.

## Distribution notices

`notices.py` traverses each exact SDK CLI, helper and worker normal dependency
graph for the build target, excluding dev/build/proc-macro code. Component
selection uses `cargo tree` so unrelated workspace members' features cannot
expand shipped notices. SDK/helper notices both carry the shared root lock;
worker notices carry its independent pinned lock. Unsupported license expressions
or missing grants stop the build; a NOTICE alone is not treated as a grant. Workspace-root licenses are
inherited, first-party REUSE/copied-code notices are preserved, and ten published
crate archives missing text have reviewed exact-VCS supplemental notices.
Where upstream explicitly declares MIT but ships no full grant, the collector
uses the project's established SPDX/author fallback and records that fact.
The objc2 licensing overview, including its Apple SDK discussion, is retained
verbatim. These are mechanical notice/policy checks, not compiler attestation.

Ruffle's compressed Noto Sans/Noto Sans Math subset retains both pinned font
copyright/OFL notices and its exact subset/merge recipe. The SDK's Iosevka and
Japanese Noto font notices also ship. Every normal Rust package lists an exact
crate source archive URL (or pinned Git source); `SOURCE-CODE.txt` explains how
to obtain the unmodified MPL-covered Symphonia sources. No MPL source files are
modified by this build. Missing grants are not hidden behind an approval flag.

The maintainer-facing research SDK/old installed-runtime scripts remain useful
for research. The app feature follows this automatic, content-addressed bundle
workflow rather than the old operator-selected host configuration.
