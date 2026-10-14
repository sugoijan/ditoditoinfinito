# Native hold-head witnesses and bounded intro inputs

Implemented 2026-10-14 in the standalone Rust SDK, behind `native`. This extends
[automatic menu profiles](danoni-rust-profiles.md) without changing the worker,
its sandbox/input limits, game crates or accepted exports. No dependency is added.
All acquired scripts still execute only inside the confined native Ruffle worker.
The new planner uses parsed chart frames and native key bindings; there is no
per-song input schedule or gauge/clock patch.

## Motion contract

`extraction::MotionSelection` adds `source: "tap" | "hold-head"`. Omission means
`tap`; serialization omits that default, preserving existing saved jobs. A hold
selection uses the **first complete hold pair in the chosen canonical lane**.
Both endpoints must be finite and the tail must follow the head. Every selected
logical clock observation must precede the head's judgment time.

`movie_observation` applies the existing visible-clip, stable reference width,
linear upward motion, clock progression, source speed/multiplier and arrival
checks. The 1.5-native-frame arrival tolerance is unchanged. Width comes from the
receptor/reference clip, not a stretched hold body. The measurement additionally
records `sourceNoteKind: "hold-head"` and `sourceHoldFrames: [head, tail]`;
`rawFirstFrame` is the head. Units remain native 60 Hz frames and stage pixels.
The separately pinned Higan/Kouhaku bounds proof rejects hold-head selections;
it has not inherited a new geometry contract.

The constant-travel profile proposes `f<lane-clip>_s0` hold clips alongside the
existing `<lane-clip>_s0` taps, using the same travel fractions, six consecutive
observations, unchanged speed segment and exact captured audio/counter offset.
Holds before the earliest tap are tried first; otherwise established tap ordering
is preserved. A proposal never authorizes data without a successful measurement.

Scheduled constant projection also accepts charts containing holds and no taps.
It requires an unconsumed all-string hold bank and the same exact loaded-source,
spawn schedule, paired tail and duration-cache agreement. An entirely empty
chart, mixed rewritten source bank or incomplete hold still rejects.

## Audio support inputs

A hold-head witness can prove movement before game over while a no-input run
still stops its music too early for independent PCM correlation. For charts with
holds before their first tap, the driver first invokes **the complete unchanged
native normalizer**. Only `insufficient-audio-origin` permits an input proposal;
other failures stop the operation. If the normalizer succeeds, no inputs are
added. The final all-chart normalizer still runs in either case.

`profiles::early_hold_inputs(chart, origin, after, slots, checkpoint)` derives
physical events from `nativeHoldFrames`, `nativeLaneIndices`, `nativeKeyCodes`
and `origin.logicalFrame - origin.iteration`:

- Consider only holds whose heads precede the first tap. With no taps, consider
  all holds within the existing observation interval.
- Translate native key codes 37/38/39/40/32 into the worker's reviewed arrow/space
  events. Other keys fail rather than being guessed or remapped.
- Press at the exact integral head iteration and release two iterations after
  the rounded-up tail. This input slack does not change source note timing.
- Require input starts after initialization, in iterations 1–1200, with releases
  no later than 1201. Heads after 1200 are outside this bounded probe.
- Merge overlapping/touching intervals for the same physical key. Each merged
  press must fit the existing 120-frame hold limit; all controls together must
  fit the existing 64-event limit. Limits are never raised for a song.

The driver recaptures initialization/audio with the complete new event list,
checks the selected menu again and requires **exact equality of the entire
projected chart and captured audio origin** with the no-input seed. It then
recaptures and remeasures the original motion selection with that same event
list. Accepted projection, motion and audio roles all bind identical controls.
The portable explicit replay plan includes those controls; it does not need to
rerun an external recipe generator.

The parent receipt adds `playInputs[]`, containing selector index, derived
`events`, hash-bound `seedProjection`/`seedMotion`, `seedNormalizationFailure`,
accepted `projection`/`motion` bindings and `projectionUnchanged`,
`audioOriginUnchanged`, `motionProved` results. A trial is recorded before replay;
failed/cancelled trials retain the seed evidence without a parent Ready marker.
`audio-check` and `play-inputs` are additional progress stages. Existing capture,
wall/output, source integrity and cancellation bounds apply throughout.

This is bounded support for observing an intro, not full-chart autoplay. Unknown
physical keys, long merged holds or unsupported source transformations remain
explicit failures. Better playback emulation is not an extraction requirement.

## Selecting part of an observed work

`ProfileRequest.selectors` is optional. Omission requests every supported menu
row. An explicit array contains 1–16 unique, strictly increasing zero-based
indices in 0–15, checked before reading assets and then against the actual
observed menu. For example, `"selectors": [0, 2]` requests those two rows.
Prepared source-only seeds and dependency recovery preserve this field.

Unselected rows remain in `menu` and `excludedSelectors` with reason
`unselected-selector`. Unsupported requested key labels retain the existing
`unsupported-key-label` exclusion. Parent readiness covers the requested
supported rows only; it does not admit the complete work. `failedSelector` names
the current selector when processing fails and is null on success. An out-of-menu
request retains its observed menu/capture but never produces a Ready work.

## Local evidence

The cached [恋色マスタースパーク ＜SlashSpeed＞ work](http://autumnleaves.yukishigure.com/DOSMF08/DarisCradle_SlashSpeed/Slash.html)
was the first profile's early-hold failure. New trials use unchanged acquired
SWF/DOS/colour bytes, no network and no worker rebuild. Local-only jobs, original
failed probes and fresh accepted captures are in
`~/Music/DanOni/reviews/rust-hold-motion-20261014/`:

| Trial | Result |
| --- | --- |
| `run/`, all three selectors | Hold motion passes; row 1 fails source-bank agreement |
| `selected/`, rows 0 and 2 without support inputs | Motion passes; PCM origin fails with only one correlated window |
| `verified/`, rows 0 and 2 with derived inputs | Two charts normalize; nine captures and two motion trials |

Row 0 (`izanaki レク`) contains 913 taps / 43 holds; row 2 (`Darkfox`) contains
777 taps / no holds. Only row 0 needs support: ten key presses, derived from its
frames/key bank, preserve every projected field and the audio origin. The
observed hold arrives one native frame after its source head, inside the
unchanged 1.5-frame measurement bound. Both final charts pass the original
independent audio-origin gate.

Row 1 (`酔舞師`) changes four note heads by 601 frames before compilation and
leaves mixed string/numeric entries in the presumed source bank. The existing
unconsumed-string/source-agreement contract rejects it. It remains explicitly
unselected; no guessed global offset or weaker comparison admits it.

`python-reference/` retains the historical frozen default chart and canonical
WAV, plus a **new data-only Python projection** of the hash-bound row-2 capture.
It records reference source hashes and provenance. Row 2 has no historical
frozen baseline; its independent projection is not described as one. No acquired
script executes during reference projection. `parity-cases.json` drives the
unchanged `profiles_corpus` gate: 1,690 taps / 43 holds / 1,776 real-parser
endpoints and 108 x1/x2/x3 scroll checks pass. Every gameplay/source/event field
compares; canonical PCM differs by at most one signed 16-bit unit. Maximum
measured endpoint error is `1.4211e-11` ms. This does not approve new musical
BPM/phase estimates.

Original synthetic tests cover empty tap lanes, paired/early hold observations,
changed/nonlinear clips, speed boundaries, pinned-path rejection, hold-only
source/schedule agreement, old JSON compatibility, key/clock/budget bounds and
cancellation. Set `DDI_DANONI_HOLD_PROFILE_JOB` to `selected-job.json` and
`DDI_DANONI_WORKER_BUNDLE` to the installed bundle to additionally run successful
recapture, identical role controls, explicit exclusions, cancellation before
support replay and out-of-observed-menu rejection. The full native suite passes
156 tests (including the installed worker/recovery gates); the pure configuration
passes 84. Both strict Clippy configurations, formatting and `just check` pass,
including REUSE coverage of 809/809 files. The earlier seven-work/eight-chart
reference gate also remains green: 5,435 endpoints / 102 scroll checks.

Accepted `exports/index.json` and `export-registry.json` remain unchanged. Broader
native engine recognition and milestone 5's host URL/policy/cache/discovery
service are next. The rewritten selector requires its own evidenced source
transformation contract if pursued. Musical estimate acceptance, HTML5 runtime
references, other-platform confinement and desktop installation remain separate.
