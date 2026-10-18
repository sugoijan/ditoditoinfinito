# DanOni SDK consolidation

Reviewed 2026-10-17. This consolidates the research branch's 75 commits from
`2ffd27e648ed97cce9690d1e1bc400d9d8939a6e` through
`fe891275040f17eed4735f13fc5550c9782cf5c0`, followed by the cleanup described here.
The original history is retained locally at
`backup/danoni-before-consolidation-20261017`. No corpus files are changed or
committed. Main-game crates and `docs/PLAN.md` are outside this cleanup.

Use [the SDK README](../../tools/danoni-dump/rust/README.md) for current APIs and
commands, [runtime setup](danoni-rust-runtime.md) to install the worker, and
[the desktop probe](danoni-desktop-integration.md) for host integration tests.
Earlier research documents retain their dated measurements, URLs, contracts and
commands as historical evidence; commands listed below have been retired.

The 2026-10-18 merge adds current-main path compatibility and standalone CI;
see [the integration update](danoni-desktop-integration.md#compatibility-with-main-2026-10-18).
The dated measurements here retain their original scope.

## Retired tools

The earlier implementation remains recoverable from the backup branch. Its frozen
local outputs remain comparison oracles; the Rust corpus tests read them directly,
without running the retired scripts or regenerating expected values with Rust.

| Removed implementation | Current replacement / reason |
| --- | --- |
| Python `run.py` and native launcher tests | Rust `supervisor`, including process/resource limits and confinement tests |
| Python `higan.py`, `kouhaku.py`, `native_families.py`, `extract_native.py`, `swf_audio.py`, `measure_motion.py` | Rust `extraction`, `swf`, profiles and orchestration; source-bound corpus projection/normalization gates |
| Python `native_audio.py`, `prepare_pack.py`, `repack_kouhaku.py` and C `encode_vorbis.c` | Rust `audio`, `preparation`, `work` and the optional in-process Vorbis encoder |
| Python `analyze_tempo.py`, `automatic_tempo.py`, `shared_tempo.py`, `musical_grid.py` | Rust `tempo` and `timing`; accepted legacy analysis receipts remain readable |
| Python `pack_policy.py`, `preview_policy.py`, `scroll_policy.py`, `source_ratings.py` | Rust `policy`, `preview`, `reviews` and SSC preparation |
| Python `capture_title.py`, `artwork_review.py`, Chrome `artwork.mjs` | Rust `artwork`; the old decorated artwork experiment is retired, plain title/source-panel art remains |
| Python `rebuild_corpus.py` | Rust `native::rebuild_accepted`, `package`, `transaction`; the one-off old-layout bootstrap is retired |
| Python fixture/export prototypes and tests for retired implementations | Rust synthetic tests, the original runtime capability fixture and frozen local corpus receipts |
| `requirements-analysis.txt` | No NumPy/SciPy analysis environment needed by the retained tools |

The old `prepare.py` is no longer a runnable entry point. Its exact bytes now live
in `ruffle/legacy-build-identity.txt` because existing worker bundles bind that
hash under the original name. Rust `worker_build` already owns source preparation.
This move preserves the worker identity byte for byte, avoiding an unnecessary
worker rebuild or invalidation of saved captures. The frozen file must not be
edited as code. A future worker identity revision can retire it explicitly.

## Retained helpers and dependencies

- Node/Chrome HTML5 capture and offline replay remain the engine oracle. Their
  `acorn`, `ajv`, `playwright-core` and `robots-parser` dependencies are used.
- `ruffle/normalize_cw.py` remains the saved-CW normalization bridge, with Node
  metadata decoding and development-only FFmpeg audio decoding. `compare_ports.py`
  compares exact native/CW frame multisets. Their filesystem helpers moved out of
  the removed batch exporter into `export_io.py`, without changing their policy.
- `cw-roundtrip.mjs`, `verify_audio.mjs`, `verify-game.mjs` and the native
  `pack-verify` program provide independent checks against the engine/decoders.
- `collect-runtime-notices.py` and `build-native-runtime.sh` remain maintainer
  build tools. They are not invoked during song import.
- Cargo dependency analysis found no unused direct dependencies in the SDK,
  desktop probe or pack verifier. No dependency versions were changed just to
  refresh lockfiles during a behavior-preserving cleanup.

## Four review passes

The review follows the local `simplify` skill: reuse, simplification, efficiency,
and abstraction level. High-confidence findings were applied; the large branch
was not mechanically rewritten.

| Pass / location | Cost | Applied change or boundary |
| --- | --- | --- |
| Reuse: `rust/src/work.rs:32`, `rust/src/preparation.rs:228` | Two filename-extension mappings to maintain | Reuse the existing codec mapping |
| Simplification: retired scripts above | Two implementations, duplicate tests and analysis dependencies | Remove migrated SWF tools and their private tests; keep the unmatched HTML5 path |
| Efficiency: `rust/src/assets.rs:110` | Hashing whole immutable media again for every binding | Cache the digest alongside the bytes; still check each caller's digest and byte limit |
| Efficiency: `rust/src/assets.rs:170` | Copying host-shared reference bytes and hashing them twice | Use `read_shared` and the same cached digest |
| Abstraction: CW helpers importing `rebuild_corpus` | Tiny file operations pulled in the entire Python export/analysis stack | Move only the needed existing helpers to `export_io` |
| Documentation: tool READMEs | Historical milestone prose presented completed stages as pending | Current entry points plus links to detailed contracts |

Filesystem guards, provenance/readiness checks and typed stage boundaries remain.
They enforce different contracts; superficially similar validation is not assumed
redundant. Public command/result schemas, accepted media, note/scroll timing,
font selection, sandbox policy and the worker build identity are unchanged.

## Next work

1. **Opus exports.** Implemented in the follow-up [Opus contract](danoni-rust-opus.md).
   The original next-step scope was: Local `main` now uses `rusty-opus` 1.0.1 for decoding; upstream
   also provides a pure-Rust encoder. Add Ogg Opus output as a separately validated
   functional change, then make it the default for app imports and keep Vorbis
   optional for other-player compatibility. Reuse the game's decoder where the
   strict import contract permits it. Explicitly test encoder delay/pre-skip,
   resampling and final granule trimming against an independent decoder, including
   short/odd-length audio, mono/stereo, leading silence and the real corpus. Chart
   offsets must not be used to hide an encoder-origin shift.
   Sources: [rusty-opus](https://docs.rs/crate/rusty-opus/1.0.1),
   [Ogg Opus mapping, RFC 7845](https://www.rfc-editor.org/rfc/rfc7845.html).
2. **Production desktop handoff.** Keep URL jobs off the UI thread, refresh the
   library only after managed-store commits, and expose progress/cancellation.
   The isolated desktop probe already exercises the real loader and play session.
3. **Packaging.** The installed runtime currently includes both embedded and
   external font bytes plus the standalone SDK executable. A host-library package
   can avoid those copies, but changing its inventory requires an explicit runtime
   contract change. Revisit binary size after Opus and packaging changes.
4. **Remaining platform/format work.** HTML5 orchestration and Windows confinement
   are separate unfinished work, not obsolete code to delete. Keep their support
   status explicit.

## Validation

Validation on macOS arm64 / Rust 1.99:

- `just check`, including REUSE lint; strict SDK Clippy with default and no default
  features; formatting and diff checks.
- SDK test suites: 282 default-feature tests and 98 without default features,
  including the new immutable-cache binding/size/copy-on-write checks.
- Retained Python/CW helper tests: four pass. Browser suite: 54 pass in the full
  run; the remaining Chrome case timed out at 15 seconds during compilation and
  passed unchanged in isolation in about one second (55 checks total).
- Accepted export comparison: **93 songs / 144 charts**, 96,419 taps, 4,520 holds;
  media byte-identical, previews and numeric ratings preserved, game-parser and
  all 105,459 note-endpoint comparisons pass. Maximum existing quantization error
  remains 8.9984 ms. No source or exported pack was modified.
- Saved native projection oracle: 69,251 taps / 3,664 holds pass, retaining the
  test's explicitly classified proof gaps. Positive normalization comparison:
  **78 works / 113 charts** pass against the preserved baseline.

One historical normalization expectation is stale: the 2026-10-13 case table says
RE:RAVE must reject with `native-source-bank-missing`. The untouched SDK library
from the earlier desktop probe already accepts it. Both old and cleaned SDKs
produce **all 22 artifacts byte-identically**; the table's other two rejection
cases still produce the same errors. The original table remains untouched. Its
full run is recorded as failed, with the positive 78-case subset and the separate
old/current rejection comparison recorded explicitly, not presented as a clean
pass of the obsolete full table. Refresh that local rejection inventory before
using it as a future all-green gate.

Logs, positive-case inputs and the baseline-library/artifact hashes are local at
`~/Music/DanOni/reviews/consolidation-20261017/`. No fresh public downloads, source
SWF execution or Linux/Windows runtime validation were needed for this cleanup.
