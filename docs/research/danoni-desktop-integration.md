# Desktop integration after the main rebase

The 2026-10-16 rebase makes a real desktop consumer available. The first step is
an executable integration probe under `tools/danoni-dump/desktop-probe/`, linking
`ddi-danoni-export` and `ddi-desktop` directly. It does not add the exporter to
the game's workspace, change WASM dependencies, or add a URL control to the UI.
The standalone lockfile records the combined dependency graph; no new
third-party library is introduced. Production runtime attestation will need to
bind the final application graph, not just the SDK's own lockfile.
Desktop video is disabled in the probe; the exported SSC/audio/banner path does
not need the movie decoder.

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
song loader, audio decoder, image loader and `Play` session. A production adapter
must instead merge managed entries into desktop's regular library refresh,
without clearing the folder/bundled entries. Store path construction should move
behind a managed-store API before production adoption. Pack naming is a host
presentation decision; the prototype uses `DanOni imports`.

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

Build/check this standalone package separately from `just check`:

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

1. Add a native-only dependency at the agreed production crate location, plus a
   desktop managed-library adapter preserving IDs, atomic snapshots, existing
   folder entries and immutable files used by in-progress plays.
2. Attach the job owner to the desktop event loop: wake notifications, explicit
   shutdown cancellation/drain, progress and terminal errors, durable restart
   tokens, duplicate/recovery outcomes and library refresh after commit.
3. Add the URL input and import-status UI, with host-owned cache/runtime/site
   policy. Exercise cancel/retry, restart and partial collections through it.
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
