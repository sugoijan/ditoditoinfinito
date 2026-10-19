# Desktop DanOni URL imports

The optional `danoni-importer` feature adds a **DanOni URL** tab to **Add songs**.
It builds a native importer helper and its pinned Ruffle worker, then stages a
complete, executable-relative runtime beside the game. `danoni` remains a feature
alias. The default game does not build or download this runtime.

Supported acquisition hosts are macOS and x86_64 Linux. Windows and WASM retain
the ordinary folder/ZIP import; they do not show this tab or prepare a native
worker. Only the SDK's reviewed classic Flash engines are supported, not every
DanOni page or HTML5 work. Imported songs use the ordinary SSC/Opus playback path.

## Build and package

The feature requires Rust **1.99**, Python 3, Git, a JDK (17 in CI), CMake,
pkg-config and the platform's native compiler prerequisites. Java builds Ruffle's
player globals; Python prepares and packages the runtime. Neither is invoked by
the installed game or helper. Acquisition and audio conversion use Rust APIs;
the FFmpeg command is not an importer runtime dependency.

```sh
cargo run --features danoni-importer
```

`desktop/build.rs` invokes `tools/danoni-importer/build.py` on supported native
hosts. The builder fetches the pinned public Ruffle sources, prepares the checked
worker sources, builds the SDK/helper/worker in separate targets outside the
application target, and collects their exact normal-dependency notices. Its
cache defaults to `~/.cache/ditoditoinfinito/danoni-importer`, outside Git.
`DDI_IMPORTER_BUILD_CACHE`, `DDI_IMPORTER_UPSTREAM`,
`DDI_IMPORTER_WORKER_SOURCES` and `DDI_IMPORTER_WORKER_BUNDLE` are build-time cache
inputs, never user runtime configuration. Cached artifacts are verified before
reuse. The helper's SDK dependency graph must agree with the reviewed SDK graph,
including the Opus encoder version; a divergent graph fails preparation.

For a package without the separate background-video module:

```sh
cargo run --locked -p xtask -- gen-songs
cargo run --locked -p xtask -- gen-noteskins
cargo build --locked --profile desktop --no-default-features --features danoni-importer
python3 tools/danoni-importer/build.py package \
  --app target/desktop/ditoditoinfinito --output target/package
```

The ordinary app retains video by default. A video-enabled distribution must also
include the existing decoder module and its complete LGPL notices, matching
source archives and rebuild inputs; the packager refuses an incomplete payload.
The `package` command audits native dependencies and copies the exact importer
selected by the application's compiled binding, assets and generated indexes,
and the required notices. Keep the resulting prefix together when distributing
or moving it. Generated binaries, worker sources and acquired songs stay outside
Git; they are not repository assets.

macOS uses system frameworks and its system sandbox launcher. Linux additionally
requires the SDK's supported Landlock ABI (at least 6), its x86_64 seccomp policy,
and a Vulkan loader/software ICD such as Mesa. There is no unconfined fallback.
Building a host worker while cross-compiling to a different supported target is
rejected. Unsupported targets return before native preparation.

## Application and helper boundary

The game links the small platform-free `ddi-importer-protocol` crate, not the SDK.
Its compiled bundle id and manifest hash identify an immutable directory:

```text
ditoditoinfinito
assets/
danoni-importer/<content-id>/
  manifest.json
  bin/ddi-danoni-importer
  runtime/...
```

Before spawning the helper, the application checks the complete bounded inventory
against that compiled manifest hash, rejecting missing, changed, extra, symlinked
or invalid paths. The helper discovers `runtime/` relative to itself and performs
the SDK's own runtime, worker, font and inventory checks. No `host.json`, manually
supplied application hash, research checkout or developer home path is required.
This provides package integrity; it does not replace platform signing or identify
a trusted publisher by itself.

The owned child communicates over anonymous stdin/stdout pipes. Protocol version
1 uses a four-byte big-endian length followed by JSON, bounded at 32 MiB per
frame. URLs, parameters, progress, origin decisions, snapshots, cancellation and
terminal results use that channel. Stderr is drained separately into a bounded
diagnostic tail. The UI never blocks on a pipe write or child exit. No control,
progress or result JSON files are exchanged between the game and helper. The
SDK's internal confined-worker captures, durable evidence, recovery journal and
managed catalog remain filesystem data.

Hello and Initialize precede each job. Initialization and imports stream a
catalog as Begin/Song/End messages. The app admits only a complete snapshot and
keeps its last complete one if a stream is interrupted. A committed import is
still reported as installed if a later refresh or pipe teardown fails; an
incomplete refresh cannot restore hidden rows from an older snapshot.

## Add songs and storage

Paste a complete HTTP(S) work URL in the DanOni tab. **Cached files only** requests
an offline import. Stages come from the running job; counters are shown only when
known. Import stays open for progress and the report. Cancel requests cooperative
cancellation; closing the dialog keeps the job running, and Retry retains the
submitted URL and offline choice.

When acquisition needs another exact origin, the tab asks for **Allow once**,
**Remember** or **Decline**, with Decline focused initially. The decision explains
public-file access and the site's terms; robots rules remain enforced. Remember
persists the user's exact-origin choice through application storage. Unique
request ids bind both UI actors and the client response, so an old pointer release
cannot approve a newer origin. Waiting for a decision is cancellable and bounded.

The shell chooses storage beneath its ordinary data/cache folders:
`data/danoni/library`, `cache/danoni/network` and `cache/danoni/staging`.
`DDI_HOME` relocates these as it does other application storage. The SDK rejects
acquisition/store outputs inside Git checkouts; agent runtime tests therefore
use a relocated package outside Git and a scratch home beneath that package.

Only the SDK's verified playback snapshot is registered. The app never recursively
scans transaction directories. Admitted song/chart ids remain stable; optional
onigiri-free playback variants are derived from a clone of each verified SSC,
leaving catalog entries original-only. Ordinary folder songs win an id collision.
Replacement retains immutable old files for active plays and invalidates future
previews. Removing an imported row or pack hides it through library settings;
no managed-store files are deleted. Successful reimport restores only that work's
verified rows. Startup, failed imports and cancellation do not unhide songs.

Exit requests cancel and asynchronously drain owned work. A nonresponsive helper
is killed after a grace period; startup verifies/recovers the SDK journal before
publishing its next snapshot. Linux workers also have a parent-death policy.
Hard helper crashes on macOS retain the SDK worker's existing confinement and
execution limits; this is distinct from successful cooperative process-group
teardown. Automatic resumption of interrupted extraction and collection review
remain follow-ups.

## Verification

`--danoni-bundle-info` prints the compiled bundle binding before opening storage,
a window or audio. `--danoni-importer-check` uses the application's real bundle
verifier and pipe client to initialize the helper, load a complete catalog and
shut down, without network access. It reports `ready`, the managed song count and
`bundledSongs`, allowing a relocated package to check its asset discovery too.
The helper's `--check-bundle` checks its relative SDK runtime independently.

Synthetic coverage includes framing limits/truncation, real child pipes, origin
decisions/cancellation, bounded diagnostics, commit/refresh ordering, managed
SSC variants and UI keyboard/pad/compact layouts. The offscreen
`danoni_import_offscreen` test draws the tab and origin prompt with the actual
painter/fonts. CI separates these cheap checks from a real pinned-worker build
and relocated package smoke test on macOS/Linux with an empty `PATH`.

`DDI_IMPORTER_TEST_ONLY=1` skips native preparation in debug test builds, leaves
the binding empty and hides URL import. It cannot produce an accepted package;
release builds reject it. It is for protocol/client tests, not a way to build a
distribution. Live URL acquisition, audible sync and platform interactive checks
must be reported separately from synthetic and package checks; no third-party
songs are committed as test fixtures.
