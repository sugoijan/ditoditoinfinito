# Automatic early constant-travel menus

Implemented 2026-10-14 in the standalone Rust SDK. This extends the
[native profile API](danoni-rust-profiles.md) to one older shared レク engine
contract, without adding dependencies or changing the game workspace, native
worker, sandbox or accepted exports. Acquired scripts still execute only inside
the installed confined Ruffle worker. The host accepts immutable source context;
it does not need per-song click, capture-frame or movement recipes.

## Recognition and API

`profiles::EngineProfile::RekuConstantMenuEarly` serializes as
`reku-constant-menu-early/1`. Identification requires all six exact SHA-256
control/placement fingerprints in `profiles/identity.rs`, each occurring exactly
once at its required timeline path/frame/tag type. These bind root initialization
and gameplay-counter actions, menu/game placement and the shared menu-controller
actions. They are hashes of shared engine roles, not approved whole-song hashes or copied
third-party code. The existing bounded FWS/CWS reader never evaluates actions.

`Movie::control_fingerprints(roles, checkpoint)` scans the entire bounded tag
index, emits only requested control roles plus **every** AVM2 tag, and preserves
duplicates. It accepts at most 64 roles and retains the 16,384 emitted-record
limit. Original movie/decompression/200,000-tag/nesting limits and cancellation
checks still apply, including while skipping unrelated artwork. The original
`action_fingerprints` full-inventory API and its limit remain unchanged. The
profile receipt's `fingerprintScope` explicitly identifies this narrower identity
inventory; it is not a full script audit. The complete movie remains hash-bound.
Animation-heavy Last Judgement exceeded the full inventory's emitted-record
limit; it now needs only the small identity inventory, without increasing any
budget or hiding AVM2 behind unselected placement records.

Both profiles require 60 Hz, SWF version at most 8 and no AVM2 tags. Identification
must yield exactly one profile. Missing, changed or duplicate control roles and
ambiguous/hybrid identities reject before work execution. Recognition proposes
controls only; full source/schedule, motion, clock and audio checks still decide
whether a chart is admissible.

| Contract | `reku-constant-menu/1` | `reku-constant-menu-early/1` |
| --- | --- | --- |
| Observed options-menu root frame | 3 | 4 |
| Title click at worker iteration 40 | `(160,200)` | `(160,165)` |
| Next-selector click | `(40,35)` | `(40,45)` |
| Exact supported runtime key labels | `5`, `7` | `5` |
| Offscreen renderer | Off | On |
| Initial stream discovery endpoint | 1200 | 600 |

Coordinates are logical 320×240 stage pixels; iteration counts are worker steps,
not note frames or music timestamps. Other nine-key modes are not reinterpreted
as 9A/9B, and this extension does not change the exporter's existing modes.

Menu tables, selected label/key/suffix checks, zero-based selector filtering,
wraparound and the bounded next/start sequence use the shared implementation.
Repeated observations must match the **identified profile's** menu frame. A
supported-looking menu on the wrong frame is insufficient. Early-profile rows
labelled `7` remain explicit unsupported rows; this engine contract proves only
five-key initialization. The later profile continues admitting its original
five/seven-key rows.

The public profile-aware methods are:

```rust,ignore
profile.read_menu(capture) -> Result<Menu>
profile.selector_controls(menu, index) -> Result<SelectorControls>
```

Existing free `profiles::read_menu` / `selector_controls` functions retain the
original later-profile contract. `profiles::identify`, `execute`,
`execute_with_dependencies`, their job APIs and CLI syntax remain unchanged.
No request flag chooses a profile or overrides its controls/renderer.

Use the same source-only job schema and command as the later profile:

```sh
cargo run --locked --manifest-path tools/danoni-dump/rust/Cargo.toml \
  --target-dir target/danoni-rust -- profile-native \
  /absolute/trusted/worker-bundle /tmp/source-only-job.json /tmp/new-early-work
```

The output must be fresh and outside Git. `profile-native-dependencies` uses the
same recognition path when the host must resolve missing cached inputs. Neither
entry point fetches an arbitrary URL or installs songs into the app yet.

## Initialization, movement and rendering

The early engines delete their numeric source banks before setting `frame_num`
to zero. The existing [constant-travel projection](danoni-gen1-admission.md)
already proves this ordering: selected loaded strings agree with unconsumed
split banks, all heads agree with sparse spawn schedules minus native travel,
paired tails and duration caches agree, and those compiled schedules/keys/speeds
survive intact to the zero-counter witness. `splitIteration`,
`compiledIteration`, `preparedIteration` and `clockInitializationIteration`
remain separate evidence positions. No new source transformation is accepted.

Initial capture frames remain the shared `start-2` through `start+8` window plus
the profile discovery endpoint. Exactly one chart MP3 start must be observed
inside that interval. A fresh replay adds its exact start iteration and reproves
selection and stream identity. Music origin comes from the captured native
counter, not a fixed start frame. Motion candidates still derive from source
frames/travel and the observed counter/worker offset; every accepted tap/hold
witness passes the unchanged geometry, speed-segment, arrival and clock proofs.
The [bounded hold-intro support](danoni-rust-hold-motion.md) remains available
under its original strict preconditions.

Some ordinary gen1 chart paths use `BitmapData.draw`. The profile supplies the
existing offscreen renderer from the start, including menu, initialization,
motion and audio runs. Every generated replay-plan observation preserves that
renderer. No drawing exception or audit error is ignored.

The rendered trials also exposed a macOS process-teardown monitoring race. The
[supervisor correction](danoni-rust-supervisor.md) still stops/reaps an
unmonitorable child; only a confirmed stop/absence and normal exit resolve that
race, retained as `monitorExitRace`. Live failures, signals, limits and
cancellation remain failures. Capture/source validation is unchanged.

Rendered 1200-iteration discovery probes exceeded the existing process CPU limit
in the first trial. The early profile therefore uses a fixed 600-iteration
initial discovery endpoint, sufficient in the preserved independent gen1
captures. This is a profile-wide resource bound, not a per-song timestamp or
raised worker allowance. Starts after that interval reject explicitly; very late
motion proposals may still exceed the unchanged worker CPU/wall limits. Quiet
music must still supply enough independently correlated PCM windows. Original
long-probe failures remain local, with their restricted receipts and no Ready
marker. The first profile keeps its existing non-rendered 1200-step behavior.

Per-worker capture count, rendering/inspection scale, CPU/wall/RSS/output and
interpreter maxima are unchanged. Whole-profile/dependency budgets and late
cancellation remain enforced. Rendered multi-chart jobs may request
`limits.wallMs: 300000`, the already-supported maximum, rather than the default
180000. This changes the host operation allowance only; per-worker CPU/wall,
capture count, interpreter and every extraction proof remain unchanged. A usable
renderer, complete audit and independently proved native audio origin are
requirements, not optional warnings.

## Local validation and scope

Local-only source inventory, source-only jobs, failed probes, fresh captures and
reference comparisons are under
`~/Music/DanOni/reviews/rust-gen1-profiles-20261014/`. No network request occurs.
All supplied movie/DOS bytes and their acquisition provenance come from the
existing public-work cache; script input queries and charsets are preserved.
The static census checks all 25 cached gen1 sources. Seventeen share every
required early-profile role. The remaining eight still need separate reviewed
control variants; they are not accepted by loosening a hash or guessing a layout.
These are fresh automatic-operation counts, not changes to the existing export
registry: earlier manually captured works remain in their accepted generation.

Examples include [明治17年の上海アリス](http://autumnleaves.yukishigure.com/alice/alice.html),
[MeltyBlood](http://autumnleaves.yukishigure.com/DOXF03/MeltyBlood/Melty.html),
[ゆかりんファンタジア](http://autumnleaves.yukishigure.com/Team_9/yukarin_side/Phantasia.html)
and [最後の審判](http://autumnleaves.yukishigure.com/DOXF03/Last/Judgment.html).
The collaboration's chart count/labels are discovered from runtime state rather
than supplied from its catalogue or old recipe. The last example exercises
ordinary bitmap drawing with the actual renderer.

All 17 matching works complete successfully, covering **23 five-key charts,
12,894 taps / 962 holds and 187 parsed speed events**. The fresh successful
operations use 86 captures and 23 motion trials; none needs manual play inputs.
Both Yukarin and ばか☆すた enumerate four collaborator rows. The latter exceeds
the default whole-operation allowance and succeeds with the existing 300-second
host budget. Original parallel/long-probe/180-second failures remain separate;
the complete retried operation, not successful children from a failed parent,
authorizes its final work.

The final frozen-reference/parser gate checks **14,818 note/hold endpoints** and
621 x1/x2/x3 scroll checks.
Canonical decoded PCM differs by at most one signed 16-bit unit and maximum
measured endpoint error is `2.8422e-11` ms. The original later-profile eight-chart
gate remains green (5,435 endpoints / 102 scroll checks). The full current native
suite passes 162 tests, including installed-worker discovery/recovery, live
memory/cancellation limits and early/hold profile gates. The pure configuration
passes 85 tests. Both strict Clippy configurations, formatting and `just check`
pass, with REUSE coverage of 811/811 files. Accepted export/registry hashes remain
unchanged.

`cases.json` binds complete successful outputs to untouched normalized Python
references and canonical WAVs. The unchanged `profiles_corpus` gate compares
all gameplay/source/event fields, accounts explicitly for capture-provenance
positions, compares decoded PCM within one signed 16-bit unit, and checks every
exported endpoint with the real SSC parser plus x1/x2/x3 authored scroll. These
checks do not promote musical BPM/phase estimates or a new playable generation.

Automatic controls can move the replay's worker epoch relative to the old manual
recipe. Yukarin's fourth selector starts two iterations later, so its stream
iteration moves from 225 to 227 and its output cursor moves by exactly 1,470
samples (`2 × 735`, the worker's 44.1 kHz mixer at verified 60 Hz). The stream
head's `samplesPerMovieFrame` is retained source metadata, not the mixer stride.
Its **native logical music origin and every source note
frame remain unchanged**. The comparator verifies that exact step/sample
relationship, then compares every other origin/slice/clock field unchanged.
A delayed `clockInitializationIteration` must equal the independently proved
zero-counter `preparedIteration` in both references. Negative tests reject an
extra sample or a changed native origin/slice; these provenance allowances never
shift source note seconds or an SSC offset.

Synthetic tests cover both complete fingerprint sets, altered/missing/duplicate
roles, AVM2/hybrid rejection, cancellation through skipped artwork, wrong menu
frames, unsupported key labels and
profile-specific selector wraparound. Set `DDI_DANONI_EARLY_PROFILE_JOB` to the
four-row collaboration job and `DDI_DANONI_WORKER_BUNDLE` to the installed bundle
for an additional fresh execution test: every observed row, source/compiled/zero
clock ordering, identical role controls and required rendering in the portable
replay plan. Existing installed-worker dependency/cancellation and later-profile
hold-input tests remain applicable.

The next integration slice is milestone 5's host URL/policy/cache/discovery
service for the proven profile subset. Remaining engine variants can be added
incrementally under their own runtime proofs. Arbitrary customized sources,
HTML5 browser-reference capture, other-platform worker confinement and desktop
library installation remain separate work.
