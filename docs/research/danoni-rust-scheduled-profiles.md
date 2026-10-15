# Automatic variable-travel native profile

Implemented 2026-10-15 in the standalone Rust SDK. This extends the
[automatic menu profiles](danoni-rust-profiles.md) to a verified subset of the
[gen3 source corpus](danoni-gen3-admission.md). It connects the existing
`scheduled-counter` projector to automatic controls and motion selection; it
does not introduce a new chart representation, change the accepted exports or
modify the main workspace. All acquired work code runs in the trusted confined
native worker. No dependency, worker build, sandbox rule or process limit changes.

## Scope and recognition

`profiles::EngineProfile::RekuScheduledMenu` serializes as
`reku-scheduled-menu/1`. Two complete six-role control combinations in
[`profiles/identity.rs`](../../tools/danoni-dump/rust/src/profiles/identity.rs)
identify it. The roles bind root initialization (frame 2), schedule compilation
(frame 4), gameplay counter (frame 5), the menu controller and gameplay-controller
sprites, and the exact menu placement. Their SHA-256 values are fingerprints of
shared engine roles, not whole-song allowlists or copied ActionScript.

The two combinations differ only in the reviewed gameplay-counter payload.
Each role must occur exactly once at its required path/frame/tag type. A movie
must match a complete combination, not an arbitrary mix of individually known
hashes. All AVM2 tags remain visible to the bounded inventory and reject admission;
60 Hz, SWF version at most 8, unique profile recognition and existing
file/tag/nesting/inventory limits still apply. The union of requested control
roles is deduplicated before scanning.

This slice proves **automatic five-key controls only**. Seven-key rows remain
explicit unsupported rows, and `profile.options(Mode::Seven)` rejects. The
original later constant-travel profile retains its previously verified 5/7-key
support. 7i/9A/9B remain supported by other export paths; no nine-lane chart is
reinterpreted as an admitted built-in mode. Generic `scheduled-counter` projection
still supports its independently verified explicit 5/7-key recipe inputs.

Five cached works match these two static combinations. Four pass complete
fresh automatic extraction:

| Work URL | Frozen reference | Taps / holds | Automatic motion trials |
| --- | --- | --- | --- |
| [風神少女](http://autumnleaves.yukishigure.com/Crow/aya.html) | `reku-004-crow-crow` | 656 / 23 | 1 |
| [月の雫](http://autumnleaves.yukishigure.com/moon/drop.html) | `reku-059-moon-m` | 122 / 0 | 1 |
| [Sacrifice -断罪-](http://autumnleaves.yukishigure.com/DOXF04/Sacrifice/Sac.html) | `reku-062-doxf04-sacrifice-sac` | 401 / 20 | 1 |
| [River flows](http://autumnleaves.yukishigure.com/River/flow.html) | `reku-073-river-s` | 531 / 1 | 1 |

Maid shares the control signature but its title uses `BitmapData.draw`. It fails
the renderer-free runtime audit with `native-profile-render-required` and is not Ready. Fingerprint recognition proposes
controls; it never excuses runtime errors or establishes chart compatibility.
Destined Marionette's older menu, the two customized collaboration menus,
Sonezaki's changed compiler and other nonmatching combinations remain outside
this profile. No source filename or song ID selects parameters.

## Controls and settings

The observed menu is root frame 3. A title click at worker iteration 40 uses
logical stage coordinates `(160,193)`; each next-selector click uses `(40,69)`
at iterations `43 + 2*n`. For `clicks` derived from the observed initial row,
Start is at `max(50, 48 + 2*clicks)` and `(105,220)`. The observed original
Practice option is selected at `start - 3`, `(60,169)`, after selector changes
and before the two pre-Start observations. These are profile-wide controls,
not per-song recipes or note/audio timestamps.

The menu must report `motion == "OFF"` and a known `ber` value of `Default` or
`Practice`. Both pre-Start observations must independently show `ber ==
"Practice"`, `startlife == 100` and zero `d1/d3/d4/d5/d6` penalties. A click's
presence is insufficient. The complete menu labels, key labels, count, selected
index and difficulty suffix must also match the repeated observation contract.
No gauge, variable, clock, chart or resource response is rewritten. Every role
uses identical inputs and chosen settings. The frozen independent references
used the same published Practice option.

The new profile uses the existing renderer-free worker and a 600-iteration
initial stream-discovery endpoint. Its per-activation interpreter deadline is
10,000 ms, the supervisor's existing maximum and the value already used by the
independent gen3 captures. Crow's compiler exceeded the previous profiles'
2,000 ms deadline; the failed halted capture is retained. Existing profiles
retain their original deadlines, rendering and discovery endpoints. Kernel CPU,
wall, memory, file/output and whole-job limits remain in force. A slow compilation
can still fail; no timeout or runtime error becomes a valid reference.

## Variable-travel proposals and exact references

The unchanged projector reads original loaded/split note data and verifies every
head against `mkArrow`/`mkFArrow` plus `judgeY` at a pre-consumption zero counter:
`spawn + judgeY[spawn] == source head`. Tails, keys, parsed speeds, boosts,
metadata and source files retain their existing exact-reference semantics.
`frame_num` is the native clock; worker iterations are evidence positions only.

`profile.motion_candidates(capture, chart, origin, checkpoint)` locates the exact
`preparedIteration` snapshot, verifies the unconsumed clock, and reads its stored
`judgeY` properties without coercions/getters. For each nonempty lane's first tap
and hold head, it requires a unique matching schedule spawn and derives that
head's own travel. It does not approximate variable travel with a global
`set_cnt` or add a guessed travel field to the reference schema. Duplicate/missing
snapshots, missing or ambiguous travel and malformed clocks/schedules reject.
Cancellation checks cover schedule lookup and proposal enumeration.

The shared proposal algorithm then uses half/quarter/three-quarter flight
positions and the observed counter/worker epoch to propose six-frame windows.
Existing bounds, positive/stable speed checks, hold-head priority and clip/lane
names still apply. Each candidate must pass the unchanged actual geometry,
linear movement, arrival, author multiplier and independent audio-clock proof.
No proposal by itself authorizes admission. Known early-hold support remains
available under its previous strict conditions; these four successful runs need
no support key events.

`profile.options(mode)` selects the correct projector; the old free `options`,
`read_menu`, `selector_controls` and `motion_candidates` wrappers retain their
original constant-travel contract. The automatic driver now uses profile-aware
methods. Request/receipt/job schemas and CLI commands are unchanged. The portable
replay plan binds generated configs and retains the profile's interpreter limit,
settings and observation roles; it needs no manual adjustments.

## Rendering diagnostic and pending coverage

The first rendered Crow probe failed before work execution when Metal could not
connect to `MTLCompilerService` (error 141, “Reentrancy avoided”). An original
one-frame blank movie reproduces that failure both under the existing sandbox
and outside it, including a repeat with the normal host environment. A temporary experiment allowing extensions only for that probe's
output directory removed a file-extension diagnostic but did not restore Metal.
No experimental sandbox policy is used for corpus work or committed.

The renderer-free path gives complete clean audits for the four admitted works.
Maid and the candidate Cynthia build instead issue explicit
`Render backend does not support BitmapData.draw` errors. Those probes remain
failed, without Ready outputs. Cynthia's different initialization combination
was removed from production recognition pending a complete rendered 5/7-key gate;
a regression rejects that unreviewed initialization. Its previously admitted
manual/frozen exports remain unchanged.

Next restore working Metal startup for the original blank fixture and repeat
confined rendered probes before adding a bitmap-capable automatic variant.
This is a measured local runtime limitation, not proof of an unsupported chart
format or a reason to discard bitmap calls. Broader classic/gen4 menu controls
can proceed separately under their own signatures and reference comparisons.

## Local evidence and checks

All third-party material stays under:

```text
~/Music/DanOni/reviews/rust-scheduled-profiles-20261015/
  static-survey.json           control hashes of 12 cached gen3 candidates
  contexts/<work>/             contained exact bytes and source-only context
  <work>-job.json              only immutable source bindings and normal limits
  crow-1/                     rendered startup failure
  crow-2/                     strict interpreter-halt failure
  crow-3/                     successful automatic source-only run
  reku-*/                     remaining successful/failed automatic runs
  cases.json                  four-work frozen reference gate inputs
  parity-result.json          source/chart/audio/SSC/scroll comparison
  cache/                      separate exact-record offline source cache
  *-url/                      complete public-URL preparation/import attempts
  library/                    fresh local managed store
```

The same optional gate used for previous native profiles compares all original
source/chart/event fields against the untouched Python-produced normalized
manifests and checks canonical PCM within one signed 16-bit unit. For that
comparison only, `cases.json` supplies the same already reviewed title/artist/page
metadata as each frozen reference; the fresh source-only captures and URL import
jobs do not receive those overrides. Musical or engine thresholds do not vary
per song. Evidence-only path/worker-epoch changes retain the existing explicit
comparator rules and cannot alter native source time.

The four-chart comparison passes **1,710 taps / 44 holds / 1,798 endpoints**,
with maximum exact-source SSC round-trip error below **0.000001 ms**, canonical
PCM difference at most **1** signed unit, and **63 x1/x2/x3 scroll checks**. This
is an exact-source comparison; the normal URL preparation pipeline separately
runs the generic musical fitter, previews, source ratings and optional host-font
banners. No accepted pack is regenerated or promoted.

Run the local read-only gate:

```sh
DDI_DANONI_PROFILE_CASES="$HOME/Music/DanOni/reviews/rust-scheduled-profiles-20261015/cases.json" \
DDI_DANONI_PROFILE_RECEIPT=/tmp/new-gen3-parity.json \
  cargo test --locked --offline --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust --test profiles_corpus \
  automatic_profiles_match_frozen_charts_and_feed_the_game_parser -- --nocapture
```

Fresh source-only runs use `profile-native BUNDLE JOB.json NEW_OUTPUT`; normal
URL preparation/import uses the existing `prepare-url-native` or
`import-url-native` commands and the same SDK APIs. No Python/Node runtime is
required by those production commands. The Python scripts in this review only
prepare study jobs, inspect passive evidence and drive verification commands.


Four additional offline URL jobs, with only their original public page URL and
shared trusted services, each prepare and install one song in a fresh managed
store. Every source receipt reports zero requests/wire/decoded transfer bytes.
The combined catalog has four songs / four five-key charts and verifies after
reopening. The jobs use generic musical fitting, previews, star handling and
host-font title banners, without per-song extraction/analysis parameters. These
new local candidates do not change the accepted exports, export registry or
original source-cache indexes.


The full SDK regression passes **269 native / 94 feature-disabled tests**,
strict Clippy in both configurations, formatting and `just check`
(**861/861 REUSE**). Synthetic checks cover every complete signature combination,
changed/missing/duplicate/AVM2/hybrid roles, unreviewed multimode initialization,
settings and selector controls, per-head/ambiguous travel, clock/snapshot failures,
cancellation and specific bitmap-required errors without audit suppression.
The final CLI reproduces the specific Maid failure, rejects Cynthia before
creating a worker output directory, and recognizes a fresh repeated Crow URL
import as unchanged. The catalog remains byte-identical after that repeat.


A combined read-only replay gate also passes the earlier profiles together with
these new cases: **28 works / 35 charts**, **19,783 taps / 1,134 holds / 22,051
endpoints** and **786 x1/x2/x3 scroll checks**, with canonical PCM still within
one signed unit. It retains the earlier known rejection and compares the same
untouched references, including previously proved seven-key charts. This proves
normalization/preparation regression parity, not fresh rendered execution of
bitmap-dependent profiles in the current environment. The combined table and
receipt are `combined-cases.json` and `combined-parity.json`; `validation.json`
pins exact source, case, output, library and implementation hashes.
