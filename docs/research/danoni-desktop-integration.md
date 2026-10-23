# Desktop integration after the main rebase

The 2026-10-16 rebase makes a real desktop consumer available. The first step is
an executable integration probe under `tools/danoni-dump/desktop-probe/`, linking
`ddi-danoni-export` and `ddi-desktop` directly. It does not add the exporter to
the game's workspace, change WASM dependencies, or add a URL control to the UI.
The probe has been a root workspace member sharing `Cargo.lock` since
2026-10-23 (its standalone lockfile went stale with every game dependency
change); no new third-party library is introduced. Production runtime attestation will need to
bind the final application graph, not just the SDK's own lockfile.
Desktop video is disabled in the probe; the exported SSC/audio/banner path does
not need the movie decoder.

## Compatibility with main (2026-10-18)

The research tip `9c3f78e` is integrated with main `e66e8d4`. The SDK's
admission/store boundary now uses `ddi_library::path` values and converts
only to plain strings for evidence/artifact records. Desktop OS paths use
checked `to_path(base)` conversions. The manifest wire representation and
source/audio identity rules are unchanged. Probe and verifier lockfiles record
main's existing platform dependencies; no dependency versions are refreshed.
The shared desktop Opus decoder already trims by integer end granules, so the
older floating-point tail-trimming observation does not require another fix.

`just danoni-check` runs standalone formatting, warnings-as-errors Clippy and
synthetic tests with locked dependencies, including minimal/default SDK
features, optional Vorbis audio/preparation tests and the independent FFmpeg
Opus comparison. The runner clears optional `DDI_DANONI_*` corpus/runtime
switches, sets a scratch `DDI_HOME` and defaults to two build jobs with debug
symbols disabled and SDK test code unoptimized (dependencies retain their
normal optimizations) to keep builds manageable on CI runners. CI runs the
full recipe on Linux and macOS; Windows runs `--minimal`. It neither downloads a worker nor imports
public URLs. Root `just test` and `just check` remain separate.

The desktop tests cover managed admission, cancellation before commit,
duplicate/replacement identity, reopening, old snapshot playback and corrupt
payload rejection. A second test generates SSCs through the SDK compiler for
5, 7, 9A and 9B keys, with mono/stereo Opus ending on an odd frame count. It
checks the real desktop scanner, exact decoded frame counts and fake-clock
60/144 Hz autoplay, including holds and history. No GPU, output device or
third-party assets are needed.

The optional `danoni-importer` feature now puts acquisition in a separate
native helper, with a verified snapshot adapter, owned cancellable jobs and
Add songs URL UI. The normal game links the pure IPC protocol rather than the
SDK. Feature builds prepare a pinned worker/helper bundle automatically and
compile its content ID and manifest digest into the client; discovery and
runtime resources are executable-relative. No operator host JSON or binding to
the final game executable is part of this path. The build collector inventories
the exact normal helper/SDK/worker graphs and completes their copied-code/font
notices; the packaging command audits dynamic dependencies and preserves source
retrieval information. See [the distribution guide](../plans/danoni-importer.md)
and [the application integration guide](../plans/danoni-import.md).

Windows confinement, managed-store durability and the recorded large-batch
memory issue remain follow-ups. Historical corpus figures below are not rerun
by synthetic CI or the relocated empty-catalog package smoke test.

## The first boundary to test

The SDK's managed store is a private, transactional library. Its current
`catalog.json` is the publication point. Transaction directories retain older
revisions and cancelled staging data. **Do not add the whole store as a desktop
song folder**: recursive discovery could expose obsolete or cancelled songs and
derive different IDs from transaction paths.

The adapter verifies the committed catalogue and its receipts, then maps its
entries to desktop `LibSong` values. It retains each source/audio-based SDK ID
and resolves playback files inside the corresponding immutable transaction.
`work` is `None`: these files are prepared SSCs, not raw HTML5 DanOni pages.
The catalogue has to be idle and verified before any row is registered. Existing
snapshots can still play after a later replacement, because their files remain.
The store assumes a private filesystem with cooperative writers; it is not a
sandbox against concurrent external file tampering.

The probe uses the desktop's test registry to supply these entries to its real
song loader, audio decoder, image loader and `Play` session. The optional desktop
adapter instead merges SDK-owned verified playback snapshots into regular library
refresh, retaining folder/bundled entries and obtaining immutable playback paths
only through the store API. Pack naming is a host presentation decision;
`DanOni imports` is the current presentation.

## Background jobs

The probe runs `import_job::execute` or `import_job::resume` on a named thread.
Progress is a single coalesced stage, not an unbounded event queue. Completion
has a separate non-lossy result. Cancellation uses the SDK's `NetworkCancel`.
The test/CLI waits for the worker, but an event-loop consumer must poll completion
and join only after the worker is finished. Dropping the probe handle requests
cancellation; it is not a production shutdown/drain manager.

The URL route uses `Installation::request` and the same host-configured cache,
origin decisions and verified worker as the installed CLI. It does not execute
an external exporter command. The worker remains a confined child process.
Source URLs never choose installation paths or site policy. Existing runtime
platform, engine and rendering limits still apply.

## Running

Run `just danoni-check` for the complete standalone recipe (Python 3 and
ffmpeg required), or check only the probe separately from `just check`:

```sh
DDI_HOME="$PWD/target/scratch/danoni-desktop/home" \
  cargo test --locked --manifest-path tools/danoni-dump/desktop-probe/Cargo.toml \
  --target-dir target -- --test-threads=1
cargo clippy --locked --manifest-path tools/danoni-dump/desktop-probe/Cargo.toml \
  --target-dir target --all-targets -- -D warnings
```

The synthetic test generates its own original chart/audio/receipts in a temporary
folder; it downloads nothing. For real songs, use a **fresh external output**
and isolate desktop state. The input is a ready candidate directory containing
its outer `manifest.json`, not a bare SSC folder or a normalized source manifest:

```sh
DDI_HOME="$PWD/target/scratch/danoni-desktop/home" \
  cargo run --locked --manifest-path tools/danoni-dump/desktop-probe/Cargo.toml \
  --target-dir target -- ready /external/new-desktop-review \
  /external/candidate-one /external/candidate-two
```

This revalidates each candidate, installs it into a fresh managed store, repeats
it as an unchanged import, reopens the committed catalogue, and plays each chart
through desktop at 60 and 144 Hz using a silent fake clock. It decodes music at
48 kHz, checks preview lookup and banner decoding where a banner exists, and
keeps results in memory. `install-N.json` preserves each completed installation
before playback; `report.json` is written only after all checks pass. Source
candidates and existing libraries are not modified. A missing banner remains a
missing banner and is recorded as such.

To exercise acquisition as well, supply a matching installed runtime and a host
configuration as described in [the runtime handover](danoni-rust-runtime.md):

```sh
DDI_HOME="$PWD/target/scratch/danoni-desktop/home" \
  cargo run --locked --manifest-path tools/danoni-dump/desktop-probe/Cargo.toml \
  --target-dir target -- url /external/new-url-review \
  /external/host.json https://reviewed.example/song.html
```

The probe overrides the host configuration's library/staging paths with its new
review directory. Cache/runtime/origin policy and offline mode remain explicit
host configuration. It then uses the same admission/store/desktop playback path.
There is no automatic runtime installation or broad URL-support claim.

A separate `folders NEW_EXTERNAL_RUN SONG_FOLDER...` command exercises existing
exported SSC folders through desktop's ordinary scanner and the same playback
audit. It preserves those folders and reports `managedImportTested:false`;
it does not prove SDK admission, catalogue identity or URL extraction for them.
Raw HTML5 works and zip inputs are outside this probe's playback audit.
Automated runs use `target/scratch/wd.sh` around the built probe/test binary,
with `DDI_HOME` set to the isolated scratch home above.

## Follow-ups after this gate

1. Validate the newly embedded application graph against an explicitly installed
   matching runtime on representative reviewed works. Synthetic snapshot and job
   tests do not establish fresh URL extraction or audible sync.
2. Add durable host restart/resume controls and partial-collection review; retained
   SDK staging evidence is available for explicit diagnosis/resume today.
3. Complete enabled-graph credits, runtime notice/license inventory and trusted
   publisher attestation before enabling redistribution or default builds.
4. Run the same probe and UI flow on Linux with its tested installed runtime;
   verify banners and play offscreen, then previews and interactive play with
   real output. Windows confinement/runtime packaging remains a separate gate.

Passing fake-clock autoplay establishes loadability, layout support and judgement
integration. It does not establish audible sync, musical BPM quality, note-colour
quality, GPU rendering, UI responsiveness, or cross-platform extraction parity.

## First macOS result, 2026-10-16

The final probe passes on the rebased branch against main `2ffd27e`. No changes
to the desktop/game/library sources were necessary for this first consumer.

| Input path | Works / songs | Charts | Layouts | Evidence |
| --- | ---: | ---: | --- | --- |
| Saved Ready candidates → admission → fresh managed store → desktop | 5 / 5 | 6 | 5 | Crow, Higan, No Name (two difficulties), River flows, 月の雫 |
| Existing exported folders → desktop scanner → desktop | 3 / 3 | 4 | 7, 9A, 9B | Decipher, トルコ行進曲, ふしぎなひかり (both layouts) |

Every chart completes at both 60 and 144 Hz: **4,689 top-tier heads and 74 held
holds per frame-rate pass**, zero lower judgements, zero dropped holds. Each
play writes one in-memory history record. All eight songs have decodable 640×200
banners, valid preview lookup, finite audio decoded at 48 kHz, and catalogue note
counts matching the parsed chart. All five managed candidates can be installed,
reopened and re-imported as unchanged, preserving their admitted IDs. This does
not assert source timing or musical-grid correctness beyond their pre-existing
preparation receipts.

Local receipts, installed copies and reports:

- `~/Music/DanOni/reviews/desktop-integration-20261016-final/`: five Ready imports,
  six charts, install/repeat receipts and final playback report.
- `~/Music/DanOni/reviews/desktop-layouts-20261016/`: scanner/playback report for
  the three exported song folders; their original files stay in `exports/`.
- The earlier `desktop-integration-20261016/` is retained as the initial run;
  use the `-final` report for the final probe implementation.

The original synthetic lifecycle test additionally covers host-thread
cancellation at the pre-commit boundary, no visible cancelled song, re-open,
duplicate no-op, replacement with stable ID, no obsolete duplicate rows, playback
of both old and new snapshots, and rejection of a corrupted committed asset.
The host remains able to poll progress and cancel while the import thread waits.

`just check` (including REUSE and WASM) and the probe's strict all-target Clippy
pass. The probe's only nonempty test is the lifecycle integration test; the real
song gates are explicit local CLI runs, not silently skipped corpus tests.
**Fresh URL acquisition was not rerun here**: the URL adapter compiles, while this
macOS gate starts at saved Ready candidates. Run `url` with the matching installed
runtime on Linux next, then wire the tested consumer into the desktop library
refresh and UI. No accepted export, user settings/history or installed runtime
was replaced, and no third-party song/media was committed.
