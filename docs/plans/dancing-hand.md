# Dancing hand in gameplay

The default native and web builds include the dancing hand. Display options
control its visibility, coordinated field/hand layout and frame size. New settings
start with the hand enabled; saved Off choices remain Off. Four-panel, Solo
six-panel, DanOni five-key, seven-key and 9B, Pump single and Techno single8 charts
are supported. Solo diagonals use the upper mat corners; Oni uses the center;
Pump uses the four corners and center. The hand is an autoplay beside the play:
it plays its own copy of the notes the play judges, at most two at once
(`At once: up to two`, ongoing holds included), while the play judges every note
with the player's own options. The hand never makes a play an assist, never
shows in its mods and never moves its bests (2026-11-06; before, the play judged
the hand's copy and every play with the hand was marked ASSIST). Source
charts and global simplification preferences are preserved.

`hand-plan` receives notes in a three-second window and keeps certified contact
prefixes immutable. It plans taps, jumps, holds, rolls, mines, shock rows and lifts,
including hold release/re-grab/handoffs and bounded rapid-note recovery. The actual
judge validates the chosen contacts. `hand-pose` fits bounded 14-control hand
kinematics, using f64, cached analytic geometry, dense stream preparation and C2
contact curves. Both stages run cooperatively on the main thread. Drawing samples
prepared motion; it does not iterate the pose solver. Numerical tolerances remain
2 mm position, 0.25 mm planted height and 1 mm clearance inside the intended tile.
Isolated contacts target tile centers; shared planted intervals reserve separated
spots, fixed until takeoff.

The pre-start prompt prepares opening motion and submits the first GPU frame.
Start immediately shows Loading if any resource is pending, then schedules audio
once ready. The same effective Player specification and shuffle seed are reused.
Retry and quick restart prepare a fresh buffer; the running audio context is kept.
Unsupported configurations or failed preparation disable the accompaniment without
blocking ordinary play. Mid-chart starts/custom timing tables outside the supported
budgets are not arbitrary-chart guarantees.

The character renderer retains the accepted camera, lighting, glossy nail regions,
dark wrist cap, silhouette backing and nine rounded contact-reactive buttons.
Settings' preview shows a standing pose from the same model. Appearance does not
alter contact timing or the physical mat plane.

Desktop display predictions can move backwards slightly when pacing estimates
settle across plays. Preparation keeps its last forward timestamp: an earlier
request only samples its exact retained, checked pose, with no planning or fitting.
Returning to the same timestamp restores its pose too. Uncovered backwards requests
and jumps beyond the rolling window still require restarting. The runtime debug
report counts `presentation_revisions`; this does not relax contact tolerances or
recover arbitrary buffer underruns.


The next proposed extension is the [double-mat study](hand-double-mat.md),
covering the four remaining playable built-in layouts. It starts with shared
geometry and a reach study, retaining this model and the existing gameplay
boundary. No stretch preparation and its separate general Options toggle are
implemented; double contact/pose/rendering support is not yet enabled.

## Rebased gameplay integration (2026-11-06)

The eight hand commits after `318d3d8c` through `269be984` are consolidated on
local main `526d19ab`. Main's independent hand-copy integration takes precedence:
the hand caps its own copy of judged notes, while gameplay keeps its notes,
options, scoring and replay behavior. The new layouts use that same boundary;
session regressions check both enabled and disabled settings without an assist.
The tight-burst proposal API stays opt-in and is not wired into ordinary play.

The planner matches the accepted hand branch except for API comments updated to
reflect main's independent hand copy. Pose math also matches,
retaining main's lint-only cleanups. Main's runtime assets, notices and licensing
assessment are preserved. Dated studies and library audits below describe the
pre-rebase host; no new full-library audit is implied by this packaging step.

Rebase verification passes: 199 character/planner/pose/session/shell tests, the
supported-layout settings test, clippy for those crates and UI, wasm32 web
compilation, formatting and diff checks. Off-screen desktop `hand_lifecycle`,
`prompt_loading` and `play_flow` all pass; the lifecycle covers 12 consecutive
completed/quit plays across Single, Solo, Pump, seven-key, Techno and 9B.
No browser frame-time certification or new full-library audit was run.

## Failure repair (2026-11-06)

The full production audit covered 12,100 source records and found 374 contact
rejections, seven pose failures and 48 pose CPU timeouts. The repair addresses
shared causes without adding song-specific exceptions. This repair keeps the accepted model, three-second
single-threaded window, f64 computation, exact scored times and published prefixes.
It does not change the 2 mm targeting, 0.25 mm vertical or 1 mm tile-margin limits,
or the 5/10 second audit CPU caps.

- **Mine choices and anticipation.** Four-panel charts again have a safe-center
  escape step as well as an air-time alternative. Replacing the former with a
  fixed return hop had stranded fingers among repeated mines. Clearance is now
  considered when a mine's early window reaches another chart row, before that
  row can reject a still-planted companion. A third option can reserve an
  imminent scored landing as the clearance step, avoiding a needless center
  detour; its source row reuses that one exact press. There are at most three
  hazard alternatives per parent inside the existing bounded beam. A mine's
  early window also participates in the rapid-recovery gate and departure
  deadline: a 150 ms note-to-mine gap can leave only 55 ms to clear its tile.
  This uses the existing 20 ms planted/25 ms airborne floors, rather than
  reducing those limits or changing the hazard window.
- **Physical clearance.** Releasing an aggregate button did not guarantee that
  its fingertip was high enough to avoid a mine. Hop selection now checks its
  height over the hazardous tile at the window boundaries and center, leaving
  0.5 mm target clearance and capping the peak at 15 mm. Invalid choices are
  rejected; the continuous skin/pose checks remain authoritative. Five-key
  charts can stay airborne across a short mine cluster (up to 400 ms between
  first and last mines), without assuming a safe center tile.
- **Roll tails.** Roll maintenance contacts participate in the rapid-recovery
  gate. The existing final recovery tier can omit a redundant tail refresh when
  the preceding press can finish the roll within every active judge table's
  budget. The search reports that omission so it can be credited and replayed;
  it is not rejected merely because no planted/swing duration was compressed.
  A scored head after a roll's end must not erase a needed refresh when its
  proposed replacement falls outside the valid tail window. Close post-tail
  heads still retain their early-hit guard so a roll pulse cannot steal the
  scored note. The broken boundary could drop DDR-A's tail while the other
  tables still passed.
  The original roll head and tail remain unchanged. Every candidate still
  passes actual-judge replay before publication.
- **Continuous clearance.** Preparation keeps 50 micrometers inside the existing
  floor limit. In stream time, low skin or knuckles are raised by root height
  instead of fitting an unrelated full IK pose. This prevents a new IK branch
  from introducing large changes between adjacent samples. A correction near
  an existing knot uses the deficit at the actual failed sample too. Subsequent
  refinements preserve floor clearance. Output remains a prepared C2 curve;
  sampling does not run a correction solve.

Small authored regressions cover mirrored mine clusters, early-window clearance,
five-key flights, roll-tail transfers, exact hazard boundaries and continuous
floor/knuckle clearance without changing digit controls. The broader original
category scenarios and private library inputs remain outside this extraction.

The remaining contact rejections are not all physically impossible. In particular,
our conservative mine-clearance interval can reject an exact tap followed by a
quick release that the actual judge accepts through tap/mine precedence. Studying
that exception would be a change to the current hazard policy; these repairs do
not silently shorten its interval or retime the note. Other remaining cases still
need examination of timing, stance, roll scheduling and bounded-search choices.

### Repair verification

All checks ran off-screen. Fresh contact planning and actual-judge continuation
across 7,101 distinct eligible inputs accept 7,062 and reject 39. Weighted across
7,401 source records, 7,362 accept and 39 reject. No previous contact pass regresses;
all 1,749 original chart-file hashes still match. The 4,699 unsupported-layout
records are unchanged.

The full gameplay/pose rerun covers the **429 former failures plus 61 passing
controls**, with four independent single-threaded processes, the same three-second
window, 480 Hz plus exact-phase checks and the unchanged 5/10 second CPU caps:

| Outcome | Former failures | Controls |
|---|---:|---:|
| Full pass | 330 | 61 |
| Contact rejection | 39 | 0 |
| Pose failure | 0 | 0 |
| Contact CPU timeout | 0 | 0 |
| Pose/validation CPU timeout | 60 | 0 |
| Wall/infrastructure timeout or playback deadline failure | 0 | 0 |
| Settling warning | 0 | 0 |

All 74 contact and six pose regressions in the older 82-chart former-pass cohort
now pass; its remaining two records hit the CPU cap. Of the original 48 CPU
timeouts, two now pass and 46 remain. Another 14 formerly contact-rejected charts
now reach pose preparation and hit its cap; these explain the increase to 60.
One chart whose original pose error is fixed reaches a later contact rejection.
The cohort has one supplemental nonfatal guide-lateness warning (`Blue`), separate
from settling warnings.

Built-in Beatucada Medium and Challenge pass under the normal caps. The prior
`EYE OF THE HEAVEN` warning case still passes with its one nonfatal guide warning.
Separate relaxed-cap diagnostics (60/120 seconds) pass three sampled remaining
timeouts at 10.82–10.99 seconds of pose/validation CPU. Those diagnostic results do
not replace the capped classifications or change production limits.

The 128 shared native tests, browser compilation, clippy (with the inherited
numerical-kernel warnings), 21 production/instrumentation parity tests and the
32 existing authored category scenarios pass: 30 scenarios animate and two
below-floor transfers remain explicit expected rejections. Additional mirrored
Rust cases cover the new boundaries. No private chart or plan is committed.

Private canonical repair cohort and frozen source/binary:
`/private/tmp/ddi-hand-recovery-motion-complete`. The full contact gate is
`/private/tmp/ddi-hand-recovery-all-contacts-complete`. Separate remaining-failure,
timeout, regression and warning results are under ignored
`target/scratch/hand-audit-recovery-20261106/`. This is focused pose verification,
not a new full-library pose audit or desktop/browser frame-time certification.


## Contact follow-up (2026-11-06)

Three shared fixes address the reviewed Glimmer, Silent Hill and converted
four-panel RUDE! rejections. UNIQUE and the `At once` simplification policy are
unchanged.

- **Pending mine boundary.** The rolling validator now follows the actual
  judge's strict late-window expiry. A mine at the exact boundary remains
  pending in the checkpoint and resolves in a later window. It is not counted
  as avoided early; actual mine hits still fail. This fixes the four unresolved
  DDR-A mines in Silent Hill and the same class in D.N.A without changing any
  judge windows or dropping cross-ruleset checks.
- **Lift to exact tap.** A scored lift retains its normal return choice and
  offers one additional bounded candidate when that intermediate landing leaves
  insufficient planted/flight time for the next same-panel head. That finger
  continues its original flight directly to the head (at most 450 ms); its
  companion can land on center for support. The source row reuses the reserved
  landing and emits one exact scored press. Auxiliary validation requires the
  matching physical lift and scored landing. Default planted/swing floors stay
  unchanged. This avoids RUDE!'s unnecessary 140 ms return plus 55 ms plant plus
  60 ms second flight inside a 234 ms interval.
- **Expanded hazard horizon.** A clearance flight can extend the interval
  validated for both fingers. Planning used its original horizon, so a later
  mine newly inside the extended interval was rejected before its companion
  could be lifted. Each clearance now advances the planning horizon too, bounded
  by the sorted, already-received mine list and existing candidate/beam limits.
  In Glimmer, the LEFT landing at 74.310 s exposes the DOWN mine's early window
  at 74.272 s. The accepted DOWN departure is 74.227 s, after its hold ends at
  74.195 s: the passage succeeds without early release. Existing early-release
  allowances remain unchanged; a reduced early-tail variant also passes.

All six affected records pass full off-screen gameplay/contact/pose checks:
Glimmer SM/SSC, Silent Hill SM, converted four-panel RUDE! SM/SSC and D.N.A SM.
There are no contact/pose failures, CPU timeouts, starved frames or settling
warnings in that check. All exact heads are checked, under the existing 2 mm
position, 0.25 mm vertical and 1 mm tile-margin bounds and 5/10 second CPU caps.
The existing 32 category/mirror cases retain 30 full-motion passes and two
expected below-floor rejections. Six new authored mirrored motion cases pass,
and 131 shared native tests pass, including checkpoint expiry/actual-mine-hit
and rolling-publication coverage. Planner clippy has no new warnings (the ten
inherited hand-pose warnings remain); the shared shell compiles for wasm32.

This is targeted verification, not a new full-library audit. Private source
charts, traces, frozen binary and results are under
`/private/tmp/ddi-boundary-lifts-20261106`; the existing category rerun is
`/private/tmp/ddi-boundary-lifts-categories-20261106`. Its small aggregate is
ignored under `target/scratch/hand-boundary-lifts-20261106/`. Only authored
regression patterns are added to the repository.


## Tight-burst preparation study (2026-11-06)

The optional `hand-plan::BurstFallback` preparation API defaults to a 60 ms
eligibility window. After all existing contact policies fail, it proposes at
most nine single-tap alternatives from consecutive three-tap sequences on
three distinct tiles containing the last rejected row. Weak rhythmic
subdivisions are preferred to beat anchors. The report now exposes the last
failed search tick directly, rather than requiring consumers to parse text.
Successful plans, timeouts, unsupported inputs and judge-validation failures
produce no proposals. Committed contacts and all non-tap events are protected.
A proposal changes a copy of the effective chart; retained times, hold ends,
source charts and pose/contact limits stay unchanged.

The off-screen study tested 60, 75 and 90 ms against the 39 previous contact
rejections and 61 passing controls. With the preceding boundary/lift/hazard fixes,
19 of those 39 records already pass unchanged. The other 20 comprise 18 contact
search rejections and two unsupported DanOni RUDE! records (more than two
simultaneous contacts). The fallback produces:

| Eligibility | Additional recovered records | Removed taps | Changed passing controls |
|---|---:|---:|---:|
| 60 ms | 2 | 2 | 0 |
| 75 ms | 2 | 2 | 0 |
| 90 ms | 2 | 2 | 0 |

Both recovered records are the same converted DanOni UNIQUE chart in SM/SSC.
One Oni/center tap in the third burst is removed in each (tick 6987); the first
two tight bursts are retained. Thus 60 ms is the narrower useful tested margin,
not a claim that every faster sequence is impossible. The remaining 18
unresolved records in this cohort are unchanged: 16 contact search rejections
and two unsupported records. There are no new wall timeouts in the sweep; this
is not a full-library rerun. The private search tests the fewest removals first, tries at most 32
candidates and at most three repairs per chart, and accepts only a replanned,
actual-judge-validated result.

Both reduced UNIQUE versions pass full off-screen gameplay and pose checks under
the unchanged 5/10 second CPU caps: 604 retained heads are checked at their exact
times. The gameplay Player and hand receive the same reduced notes in this
study. Retained note records/times are compared exactly before full motion is
checked; no holds, rolls, lifts or hazards are removed.

This API is opt-in and does **not** enable automatic skipping in ordinary
`RollingPlanner`, `Player` or gameplay. The comparison deliberately exercised
shared-chart preparation before playback on the pre-rebase branch. Main now
keeps the hand's simplified copy separate from gameplay: a future host may adopt
proposals only for uncommitted hand input, without changing the judged chart or
published contacts. This study does not establish that live integration or add
a whole-chart preflight to normal play.

Authored regressions cover mirrored failure/replanning, a solvable eligible
burst with no proposals, the exact 60 ms boundary, protected events/committed
contacts, two-tile trills and timeout-versus-failure metadata. The existing
32 category/mirror scenarios retain 30 full-motion passes and two expected
below-floor rejections. All 136 shared native tests pass; the browser target
compiles and planner clippy has no new warnings (the ten inherited hand-pose
warnings remain). Private comparison sources, plans, binary and evidence
remain under `/private/tmp/ddi-burst-fallback-20261106`; category results are in
`/private/tmp/ddi-burst-fallback-categories-20261106`. No private input or study
harness is added to the repository.


## Solo six-panel support (2026-11-06)

The shared hand path now accepts `dance-solo`, including native Solo simfiles and
Solo versions generated from seven-key DanOni charts. The existing Dancing hand
setting applies; saved Off choices and the source chart stay unchanged. Gameplay
and the hand still judge the same `At once: up to two` copy, with exact original
times. The model, materials, camera, stance preferences, contact tolerances and
three-second single-threaded f64 preparation are unchanged.

| Chart lane | Direction | Mat tile |
|---:|---|---:|
| 1 | Left | 4 |
| 2 | Up-left | 7 |
| 3 | Down | 2 |
| 4 | Up | 8 |
| 5 | Up-right | 9 |
| 6 | Right | 6 |

Center (5) is an unscored resting/escape tile on Solo. The inverse mapping used
by judge replay must never turn a center landing into a Solo key press. Input
notes now carry their resolved physical panel, checked against the layout's
lane map. Future-contact and rapid-departure bookkeeping cover all six lanes and
seven physical destinations including center. Holds, rolls, lifts, mine escape
and shock handling use the same existing policies.

The nominal geometry adds the two upper corners while retaining every original
five-panel table entry bit for bit. Physical tile coordinates are shared by
anchors, centered targets, mine clearance, continuous-curve validation and
stream hit checks. This removes the old five-tile assumptions throughout the
pose validation path, rather than accepting corner plans that fail later. The
existing nine-tile renderer needs no new assets.

Off-screen verification:

- All **378 Solo records** in the private library pass rolling contact planning
  with four independent single-threaded processes and a 5 s CPU cap per input.
  There are no contact failures, timeouts or unsupported records in this cohort.
- **11 full gameplay/pose checks** pass under the normal 5/10 s CPU caps: six
  native library charts spanning Easy/Medium/Hard and five authored cases
  covering mirrors, holds/rolls/lifts/diagonal mines, a six-lane DDR-A shock row,
  seven-key conversion and dense diagonal stream motion. There are no pose or
  contact failures, playback deadlines, starved frames, guide-lateness warnings
  or settling warnings. These samples do not replace a full Solo pose audit.
- All 144 shared native tests and the Solo settings test pass. The off-screen
  GPU lifecycle test alternates Single/Solo/Single/Solo through completed and
  quit plays on one host; opening readiness, rendering and availability pass.
  Saved Off settings remain Off. The complete web shell compiles for wasm32.
  Clippy adds no warnings; the ten inherited hand-pose warnings remain.
- The original 32 category/mirror cases retain 30 passes and the two expected
  below-floor rejections. Beatucada Medium, Beatucada Challenge and an authored
  five-key reference produce **bit-identical pose/velocity/acceleration hashes**
  against the pre-Solo build with the same preparation checkpoint schedules.

Private chart inputs, frozen audit binary, plans, comparison schedules and raw
results remain under `/private/tmp/ddi-solo-support-20261106`; the category rerun
is `/private/tmp/ddi-solo-categories-20261106`. Small aggregates are ignored under
`target/scratch/hand-solo-support-20261106/`. No private simfile or development
harness is committed.



## Pump single support (2026-11-06)

`pump-single` uses the existing Dancing hand setting and the same shared
`At once: up to two` effective chart as gameplay. Native charts and versions
converted from DanOni five-key charts keep their source, exact note times,
holds and original game rules. Saved Off choices remain Off.

| Chart lane | Direction | Mat tile |
|---:|---|---:|
| 1 | Down-left | 1 |
| 2 | Up-left | 7 |
| 3 | Center | 5 |
| 4 | Up-right | 9 |
| 5 | Down-right | 3 |

The physical mat remains at 24 mm pitch. Corner coordinates are shared by
contact planning, pose targets, mine checks, continuous-curve checks and dense
stream hit validation. The nominal tables add the two lower corners while
retaining all 196 earlier scalar values bit for bit. The renderer already
has nine contact-reactive tiles.

Center is scored. Pump has the unscored side stances `(4,6)` and `(6,4)` as
hazard escapes, alongside air-time avoidance. The bounded beam considers at most
four hazard alternatives per parent. Companion hops and ordinary lift returns
can land on these unscored tiles, keeping them from consuming a future center
note. Opposite-corner double lifts prepare both fingers before their exact
scored release.

An ordinary single-finger transfer can move its free, unheld companion onto a
side tile when the old or new base spans more than 60 mm. This auxiliary
`support_step` lands with the original head and is validated against that head,
the opposite finger and an unscored physical landing. It respects the committed
release floor and planted/swing limits. A coordinated support hop may overlap
for up to 450 ms. This avoids leaving the stationary finger stranded at an
opposite corner during a one-finger transfer. Normal step policies on the other
layouts are preserved.

Pump accepts the nominal 50.8 mm opposite-corner span and flexible facing,
with a soft reference-facing cost. The pose guide eases its balance preference
toward the midpoint as physical fingertip separation grows from 54 to 68 mm.
The quintic easing also covers approaches and departures, avoiding a switch at
a contact edge. The layout remains attached to visible pose-plan windows, so
this preference applies only to Pump. The model, joint bounds, taller stance,
three-second single-threaded f64 preparation, fit iteration budgets and contact
limits remain unchanged: 2 mm targeting, 0.25 mm planted height and 1 mm tile
interior margin.

Off-screen verification:

- All **144 shared native tests**, the hand-settings test and the real off-screen
  GPU lifecycle test pass. The latter exercises six consecutive Single/Solo/Pump
  plays through completion and quit on one cumulative-clock host. The full web
  shell compiles for wasm32. Clippy adds no warnings; inherited warnings remain.
- **92/94 private Pump records** pass rolling contacts with four independent
  single-threaded processes and 5 s CPU caps. Two `RUDE!` Edit records, `.sm` and
  `.ssc`, reject a row of **three simultaneous lifts** at tick 10608. Those lifts
  survive the existing `AtOnce::Two` transform; the two-finger planner rejects
  them without changing the scored chart. No contact CPU timeouts occurred.
- **12/12 full gameplay/pose samples** pass under 5/10 s CPU caps: six native
  library charts, mirrored mixed-note cases, a five-lane shock row, dense
  opposite-corner streaming, five-key conversion and all ten distinct jump pairs.
  These have no pose/deadline failures, starvation, guide-lateness or settling
  warnings. This sample does not replace a full Pump pose audit.
- The **32 category/mirror expectations** hold: 30 passes and the two expected
  below-floor rejections. Beatucada Medium/Challenge, a five-key and a Solo
  reference retain bit-identical pose/velocity/acceleration digests against
  `37f5ce5c` under identical preparation checkpoint schedules.

Private inputs, frozen binary, plans, schedules and results are under
`/private/tmp/ddi-pump-support-20261106`; category results are under
`/private/tmp/ddi-pump-categories-20261106`. Small aggregates are ignored under
`target/scratch/hand-pump-support-20261106/`. The checked-in tests cover mapping,
exact judge replay, mirrors, all jump pairs with repeated hits, publication,
center repeats/lifts, hazards, both escape orientations, conversion, the shared
gameplay chart, bounded corner poses and timed playback through stops/BPM changes.
Private charts, development harnesses and modeling assets remain uncommitted.



## DanOni seven-key support (2026-11-06)

The builtin `danoni-7` layout now accompanies ordinary gameplay with the hand.
Gameplay and the hand use the same `At once: up to two` copy, preserving source
charts, original note times, holds and saved Off settings. The mat, accepted
model, three-second single-threaded f64 preparation and contact tolerances stay
the same.

| Chart lane | Displayed direction | Mat tile |
|---:|---|---:|
| 1 | Left | 4 |
| 2 | Down-left | 1 |
| 3 | Down | 2 |
| 4 | Onigiri | 5 |
| 5 | Up | 8 |
| 6 | Up-right | 9 |
| 7 | Right | 6 |

This mapping follows the chart's actual glyphs; lane names and numeric-key
bindings alone do not establish arrow direction. A regression compares the
mapping directly with `Layout::builtin("danoni-7")`'s glyphs.

The two unused corners, **7 and 3**, can support escape stances `(7,3)` and
`(3,7)`. Air-time avoidance remains available, including release/re-grab where
the actual judge permits it. Neither unused corner becomes a scored lane in
judge replay. Pump's bounded support-step, wide-reach, flexible-facing and
smooth midpoint-balance treatment is shared with seven-key; support and
companion landings use the layout's own unscored destinations. The existing
nine-tile geometry and render assets are reused unchanged.

An already-shared freeze keeps its spare finger on the held tile, as on five
keys, so alternating sides can maintain aggregate contact. Taps and rolls can
clear the companion onto an unused corner. The rapid-departure cache now covers
nine source lanes, ensuring the seventh lane's tap-to-mine gaps reach the
existing adaptive policy. Its 20 ms planted/25 ms airborne floors, scored times,
published-prefix protection and 2 mm/0.25 mm/1 mm contact limits are unchanged.

Native DanOni holds use their actual cumulative release budget. The StepMania
release/re-grab grace cannot be borrowed for them. Some center-freeze crossings
need more support overlap than the current gap-based maneuver supplies; those
remain rejected when their release exceeds the native budget. A hold/mine
conflict requiring a longer release is rejected too. Matching five-key cases
retain the same outcomes against the preceding build; native maintained holds
and an already-shared center hold pass. No judge budget is loosened.

Off-screen verification:

- **155 shared native tests**, including nine new seven-key planner tests, pass.
  Coverage includes all **21 distinct jump pairs** with repeated hits, mapping,
  mirrored notes, rolling publication, holds/rolls/lifts, all-lane hazards,
  unused-corner and air-time choices, seventh-lane rapid departure and native
  hold-budget enforcement. Ordinary playback tests include mirror turns, holds,
  stops, BPM changes and immediate notes.
- The settings test and real GPU lifecycle test pass. Eight consecutive plays
  exercise Single, Solo, Pump and seven-key twice on one host through completion
  and quit. The web shell compiles for wasm32. Clippy adds no warnings.
- **16 fixed first-party gameplay/pose fixtures** have the expected outcomes:
  **13 pass**, with **three expected native-budget contact rejections**. Accepted
  samples include standard/native rules, mirrored maintained holds, shared-center
  support, all jump pairs, dense diagonal stream motion, seven-lane mines and
  shocks, rapid hazards and hold re-grabs. Accepted samples have no pose/deadline
  failures, timeouts, guide-lateness, starved frames or settling warnings.
- All **32 category/mirror regression expectations** hold: 30 passes and two
  expected below-floor rejections. Beatucada Medium/Challenge, five-key, Solo and
  Pump references retain bit-identical pose/velocity/acceleration digests against
  `7f98195c` under identical preparation checkpoint schedules. Three native
  five-key policy comparisons preserve two rejections and a shared-hold success.

**The full library audit is deferred until after DanOni 9B support**, as requested.
This pass uses authored fixtures and the fixed regression/reference samples only.
Private fixture runner, plans, schedules and results are under
`/private/tmp/ddi-danoni7-support-20261106`, with category results under
`/private/tmp/ddi-danoni7-categories-20261106`. Small ignored aggregates are under
`target/scratch/hand-danoni7-support-20261106/`. No private charts, external
modeling assets or development harnesses are committed.



## Techno single8 support (2026-11-06)

`techno-single8` now uses the hand in ordinary gameplay, including versions
converted from DanOni 9B without Oni. The existing Dancing hand setting applies,
with saved Off choices preserved. Player and hand retain the same shared
`At once: up to two` chart, original note times, holds and actual judge rules.

| Chart lane | Displayed direction | Mat tile |
|---:|---|---:|
| 1 | Down-left | 1 |
| 2 | Left | 4 |
| 3 | Up-left | 7 |
| 4 | Down | 2 |
| 5 | Up | 8 |
| 6 | Up-right | 9 |
| 7 | Right | 6 |
| 8 | Down-right | 3 |

All eight outer tiles are scored; **center (5) is unscored**. Mine/shock avoidance
can use the center or air time, including judge-valid hold release/re-grab.
Center escape landings are excluded from scored-button replay. The bounded
wide-stance support-step and smooth midpoint-balance preferences are shared
with Pump/seven-key, using center as Techno's support destination. Duplicate
center destinations are considered once; foot-specific stance destinations
remain separate from that unique-choice list. Existing layouts keep their
previous order and motion.

The nine-tile geometry, accepted model, joint bounds, fit budgets, three-second
single-threaded f64 preparation, exact touchdown/release times and published
prefixes are preserved. Numerical limits remain 2 mm targeting error, 0.25 mm
planted height and 1 mm clearance inside the intended tile. Source charts and
render assets are unchanged. Native DanOni 9B itself remains outside this pass.

Off-screen verification:

- **165 shared native tests** pass, including eight new Techno planner tests:
  displayed-glyph mapping, all **28 distinct jump pairs** with repeated hits,
  mirrored mixed notes, eight-lane rolling publication, eighth-lane rapid mines,
  center/air avoidance with holds, nine-key conversion and invalid-panel rejection.
  Ordinary gameplay tests cover immediate notes, mirror/left turns, holds,
  stops and BPM changes. The settings test passes, preserving saved Off values.
- The actual GPU lifecycle test passes through **ten consecutive plays**:
  Single, Solo, Pump, seven-key and Techno twice on one cumulative-clock host,
  including completion and quit. The web shell compiles for wasm32. Clippy adds
  no warnings; inherited warnings remain.
- **13/13 fixed first-party full gameplay/pose fixtures pass** under the existing
  5/10 s CPU caps. They cover mixed notes, mirrors/quarter turns, all jump pairs,
  dense diagonal stream motion, eight-lane mines/shocks, rapid hazards,
  hold release/re-grab, stops/BPM changes, same-square repeats/lifts and
  nine-key conversion. No pose/contact/deadline failures, timeouts, guide
  lateness, starvation or settling warnings occurred in these samples.
- All **32 category/mirror expectations** remain unchanged: 30 passes and two
  expected below-floor rejections. The preceding **16 seven-key fixture
  expectations** also hold: 13 passes and three native hold-budget rejections.
  Beatucada Medium/Challenge, five-key, Solo, Pump and seven-key references keep
  bit-identical pose/velocity/acceleration digests against `e8bc2f85` under the
  same preparation checkpoint schedules.

**The full library audit remains deferred until after DanOni 9B support.** This
pass only runs authored fixtures and fixed regression/reference cases. Private
runner, plans, schedules and evidence are under
`/private/tmp/ddi-techno8-support-20261106`; category results are under
`/private/tmp/ddi-techno8-categories-20261106`. Small ignored aggregates are under
`target/scratch/hand-techno8-support-20261106/`. No private charts, development
harnesses, modeling assets or captures are committed.


## DanOni 9B support (2026-11-06)

`danoni-9B` now uses the hand in ordinary gameplay on **all nine mat tiles**.
The existing Dancing hand option applies, preserving saved Off choices. Player
and hand receive the same `At once: up to two` effective chart; source charts,
scored times, release times and actual judge rules remain unchanged.

| Chart lane | Displayed direction | Mat tile |
|---:|---|---:|
| 1 | Up-left | 7 |
| 2 | Left | 4 |
| 3 | Down-left | 1 |
| 4 | Down | 2 |
| 5 | Oni | 5 |
| 6 | Up | 8 |
| 7 | Up-right | 9 |
| 8 | Right | 6 |
| 9 | Down-right | 3 |

The displayed glyphs define this mapping, rather than the importer's legacy lane
names or keyboard bindings. **There is no unscored escape/support tile.** Mines
and shock rows can require both fingers airborne, retaining the existing hazard
window and physical-clearance checks. Native DanOni's cumulative freeze-release
budget is enforced separately from StepMania hold grace.

Sharing the center initially exposed assumptions about an available escape tile.
For a shared roll or rapid center repeat, the companion can now move to a
**temporary scored rest** selected from the other tiles. The bounded choice checks
known upcoming heads and hazards, including physical clearance throughout the
flight; it is not treated as an always-safe destination. A nearby mine can delay
that landing, with auxiliary replay certification tied to the source head and
hazard. A shared lift followed by a tap can keep the companion airborne through
the exact new touchdown. Shared ordinary holds retain aggregate support. An
already-airborne companion no longer causes a redundant second takeoff at a head.
These choices are scoped to 9B; previous layouts retain their motion.

Air-only mine avoidance also records its existing adaptive departure allowance,
as escape steps already did. Otherwise valid rapid hops had been rejected for
missing bookkeeping credit. No planted/airborne floor or hazard margin changed.

9B uses the existing wide-stance flexible-facing and smooth midpoint-balance
preference. The accepted model, skeleton, taller stance, three-second window,
single-threaded f64 preparation, immutable published prefixes, solver bounds and
fit budgets are unchanged. Limits remain 2 mm targeting error, 0.25 mm planted
height and 1 mm clearance inside the intended tile. No rendering assets changed.

Off-screen verification:

- **178 shared native tests** pass, including eleven new 9B planner tests and
  an explicit all-tile air-avoidance test. They cover displayed-glyph mapping,
  all **36 distinct jump pairs** with repeated hits, mixed mirrored notes,
  rolling publication, ninth-lane rapid hazards, center repeats/lifts/rolls,
  delayed companion landings and native release-budget rejection. Settings and
  shared-session checks preserve saved Off choices and the shared effective chart.
- **18 fixed first-party gameplay/pose scenarios** meet their expectations:
  17 animate successfully and one native held-mine conflict remains an explicit
  contact rejection under its actual release budget. Accepted samples cover
  dense opposite-corner streams, all-nine mines/shocks, hold re-grabs, stops and
  BPM changes, exact center re-hits, rolls and maintained native freezes.
  No unexpected contact/pose failures, timeouts, fatal deadlines, starvation,
  guide lateness or settling warnings occur. Maximum exact targeting error is
  0.459 mm, maximum planted height error 0.169 mm and minimum tile margin 2.204 mm.
- The existing **32 category/mirror expectations** remain unchanged: 30 passes
  and two expected below-floor rejections. The prior **16 seven-key** cases keep
  13 passes/three native release-budget rejections; all **13 Techno** cases pass.
  Beatucada Medium/Challenge, five-key, Solo, Pump, seven-key and Techno retain
  bit-identical pose/velocity/acceleration digests against `3ca16c78` under the
  same preparation checkpoint schedules.
- The off-screen GPU lifecycle passes **twelve consecutive plays** across Single,
  Solo, Pump, seven-key, Techno and 9B on one cumulative-clock host, including
  completion and quit. Browser-shell wasm32 compilation and clippy pass with
  inherited warnings only.

**At this support checkpoint, the full library audit had not been run.** The
subsequent maintainer-requested full rerun is recorded at the end of this plan.
These authored scenarios and fixed regression/reference checks do not certify
all nine-key charts or browser frame-time performance. DanOni 9A is outside this
change. Private runner, inputs, plans, schedules and evidence are under
`/private/tmp/ddi-danoni9b-support-20261106`; category results are under
`/private/tmp/ddi-danoni9b-categories-20261106`. Small ignored aggregates are under
`target/scratch/hand-danoni9b-support-20261106/`. No private charts, development
harnesses, modeling assets or captures are committed.


## Package boundary

This is one standalone extraction from `codex/dancing-hand-preview` at `399a1c02`,
based on local `main` at `cfdd2d34`. It includes runtime code, the two byte-identical
runtime data files, required notices and small tests. It excludes research archives,
Blender/FEBio/MMD tools, prerecorded dance clips, audit/cohort/performance utilities,
keyboard-chart demos, visual playground and debug camera/pause overlays.

The development branch remains the source for broader audits and optimization.
No private charts or excerpts are included. The extraction itself is not a new
full-library audit or performance certification. All engine/session/renderer
changes on main, including announcers and restart behavior, remain in place;
restart can briefly show Loading while its hand buffer is prepared.

## Runtime data and notices

`assets/hand/hand.glb` and `model.json` are embedded directly. Their hashes and source
paths are recorded in `assets/hand/provenance.json`. The GLB retains only a
zero-valued static carrier track (two identical keys), not a recorded dance. The
independent mesh originated from Dan Ulrich's CC0 hand; MZiemys' CC0 bones were used
as references. The rig, correctives and calibration are ours (MIT), the motion
planned on Beatucada's CC BY 4.0 chart: `MIT AND CC0-1.0 AND CC-BY-4.0`
(`REUSE.toml`, Credits). The studies compared against WAVEFILE's MikuMikuDance
motion, a reference Miku model and a CMU motion capture; none of them is in these
files, and on 2026-11-06 the maintainer assessed the hand as original work that
does not meaningfully use them, narrowing the conservatively inherited terms the
extraction first carried. `assets/hand/NOTICE.txt` records both.

## Basic checks

- Character loader malformed input, interpolation, morph/skin order and limits.
- Bounded solve, curve math, centered contacts and immutable shared contact spots.
- Synthetic planner inputs and actual judge replay.
- Shared-session effective input, native prompt/loading/cancel/restart and shared
  runtime exact contacts across offsets/mirror/shuffle and frozen startup.
- Desktop completed/quit song lifecycles and later-play pacing revisions; exact
  read-only resampling, unchanged preparation progress and seek rejection.
- Native and wasm compilation, shader validation and off-screen browser smoke
  checks. Full private-library audit tooling stays on the development branch.

Run `cargo test -p ddi-character -p ddi-hand-pose -p ddi-hand-plan -p ddi-shell
-p ddi-session`, then the native off-screen `ddi-desktop` tests `prompt_loading`
and `play_flow`. `trunk build --release` builds the ordinary browser app without a
hand-preview feature flag. Use the repository watchdog and isolated `DDI_HOME` for
local runs.

Verified on the extraction branch: character/pose/planner and shared session/shell
unit tests, centered-stance/planner integration tests, the full UI suite with
`layout`, four character shader/WebGL2 translation tests, native prompt/loading/
cancel/quick-restart and existing play-flow tests. Headless Chrome passes ordinary
play and quick restart on WebGPU and WebGL2 with gesture-required audio, no console
errors, a three-second window and the hand ready on the first reported play frame.
Native/wasm checks, the ordinary Trunk release build, formatting and REUSE pass.
Clippy retains the numerical/loader warnings already present in the hand kernel.
No new full-library audit was run for this extraction.

The following audit and repair results record the pre-rebase branch, whose host
used a shared two-contact gameplay chart. They remain historical measurements,
not a full-library certification of main's independent hand-copy integration.

## All supported hand layouts: full library rerun (2026-11-06)

The maintainer-requested run pins gameplay build `bc1895cf`, freshly importing
all 1,749 private files under `~/Music/Simfiles`. It covers the same **11,898
original records plus 202 generated chart versions**, including converted and
simplified charts, without reusing previous results. Four independent chart
workers run off-screen; each gameplay/planning/pose pipeline stays single-threaded.
The accepted f64 math, three-second window, same `AtOnce::Two` chart for Player
and hand, exact times, immutable prefixes and 2 mm / 0.25 mm / 1 mm contact limits
are unchanged. CPU caps remain 5 s for contacts and 10 s for pose/validation;
the separate wall watchdog is 90 s. No runtime/solver changes accompany this audit.

The run takes **7,386.01 s (2 h 03 min 06 s)**. Of 12,100 source records,
**7,878 are eligible**, representing 7,578 distinct effective inputs.

| Outcome | Original | Generated | Total |
|---|---:|---:|---:|
| Full pass | 7,592 | 184 | 7,776 |
| Contact failure | 17 | 4 | 21 |
| Pose failure | 3 | 0 | 3 |
| Pose CPU timeout | 78 | 0 | 78 |
| Unsupported layout | 4,208 | 14 | 4,222 |

There are **zero contact CPU timeouts, wall/infrastructure timeouts, import or
infrastructure errors, fatal playback deadlines and settling warnings**. All
unsupported records are layouts; none is an unsupported judged note kind.
Six passing records have seven nonfatal guide-lateness events, listed separately
from settling warnings. Full passes represent 7,476 distinct effective inputs.

### Coverage and remaining failures

| Supported layout present in this library | Pass | Contact failure | Pose failure | Pose CPU timeout |
|---|---:|---:|---:|---:|
| Dance Single | 7,213 | 16 | 0 | 78 |
| Dance Solo | 378 | 0 | 0 | 0 |
| DanOni 5 | 90 | 4 | 0 | 0 |
| Pump Single | 90 | 1 | 3 | 0 |
| Techno single8 | 5 | 0 | 0 | 0 |

No direct DanOni 7 or 9B charts occur in this library. Their authored regression
checks above remain separate evidence; the full audit does not create real-chart
coverage where none exists. Unsupported layouts are Dance Double (3,974),
Dance Couple (207), Dance Three-panel (27) and Pump Double (14).

The three pose failures are newly covered Pump inputs:

- *It's Me* Edit: `runtime_stream_arrow`.
- *Mmchk* Challenge: `planted_height`.
- *RUDE!* Edit: `floor_penetration`.

Private traces retain the inputs and diagnostics. `pose-details.csv` separates
the playback time when the failure disables the hand from the future prepared
pose's failure time. No tolerance is loosened to accept them. Three RUDE! contact
failures report “More than two simultaneous contacts” during planning; these
remain contact-stage rejections rather than unrecognized layouts or note kinds.
The other contact cases retain their first rejected tick and source clock.

### Comparison and verification

Against the `07200df0` full production run, all source IDs and original file
hashes match. **477** formerly unsupported records become eligible: **473 pass,
three fail poses and one fails contacts**. Of previously supported failures,
340 contact failures, six pose failures and four pose timeouts now pass.

**Nineteen previous passes now hit the pose CPU cap**, all with matching effective
input identities. None of the former passes becomes a contact or numerical pose
failure. The separate `previous-pass-regressions.csv` lists these timeout
regressions. This comparison spans intervening repairs and layout additions;
it does not isolate a cause. OS placement is not pinned, and process CPU caps
do not normalize asymmetric-core speed. No relaxed-cap or serial rerun replaces
the canonical outcome classifications.

All 21 native adapter/parity tests pass before execution. Independent review
verifies fresh source/catalog/binary/frozen-source hashes, all IDs, aliases, four
worker shards and disjoint result lists; it also checks every passing actual
judge result, sample coverage, numerical bounds and exact contacts. The
274 custom-table records have independently checked COMBOS/autoplay weighting.
Full passes cover **3,024,856 exact heads, 155,550 hold tails, 35,639 avoided
mines and eight lifts**. No shock-bearing chart occurs in the library.
Maximum accepted exact errors are 1.837 mm total and 0.249 mm vertical;
minimum tile margin is 2.361 mm, within unchanged limits. No passing trace has
starvation, a missed head, a dropped hold or a fatal deadline.

Canonical private run and pinned sources/binaries/plans:
`/private/tmp/ddi-library-nine-layouts-20261106-v1`; independent verification and
comparison are in its `review/`. Small ignored aggregates are under
`target/scratch/library-dance-audit-nine-layouts-20261106/`, with `REPORT.md`,
separate contact/pose/timeout/unsupported/settling/guide CSVs, comparison and
verification JSON. No private inputs, plans, development harnesses or captures
are committed. This native gameplay-math run does not render media and is not a
new desktop/browser frame-time certification.


## Focused Pump pose repair (2026-11-06)

This follow-up addresses the three Pump numerical failures from the full audit
above, without changing the contact planner, chart transformations, source notes,
model, timing, finger assignments or accepted motion policy. It keeps the taller
stance, three-second single-threaded f64 window, 1 ms minimum curve spacing,
six refinement passes, existing fit caps and 2 mm / 0.25 mm / 1 mm output guards.
No iterative solve is added to sampling.

- **Stream fit quality.** A small fingertip error no longer hides unsafe knuckle
  height when deciding whether to retry from a style seed or keep a raw fit.
  Low knuckles previously forced a large rigid lift during clearance repair,
  carrying the planted tip above its tile. The bounded retry considers both
  accuracy and clearance; feasible fits keep the original error ordering.
  Final curve/arrow certification is unchanged.
- **Planted height near lift-off.** A hard height violation less than 1 ms from
  an existing knot cannot insert another point under the spacing guard. Repeating
  the same endpoint fit can leave the interpolant invalid. Preparation now sets
  the neighboring control knots' velocity and acceleration to zero in that
  unresolved near-knot case, stopping the fast lift's tangents from bleeding
  into planted time. Adjacent quintics share those derivatives, preserving C2.
  The complete candidate must still pass every ordinary geometry check.
- **Small exact-mode floor dips.** Before choosing a new IK branch, preparation
  may use at most 0.5 mm of rigid root-height translation. The same offset must
  fit both the failed interpolant and its replacement anchor, including model
  bounds, both fingertips, knuckles and planted-height reserve. Both poses pass
  the ordinary target/tile/hazard/roll/lift checks before the offset is accepted.
  The complete curve is checked again. If no safe offset exists, the original
  bounded IK correction remains. This avoids bending the digits unnecessarily
  for a small vertical error; it does not increase any output tolerance.

### Targeted results

| Original failing SM chart | Original prepared-pose failure | Result |
|---|---|---|
| *It's Me*, Edit (`It's Me -Another-.sm:6`) | `runtime_stream_arrow` at 43.36735 s | Full pass |
| *Mmchk*, Challenge (`Mmchk.sm:4`) | `planted_height` at 33.30414 s | Full pass |
| *RUDE!*, Edit (`RUDE!.sm:5`) | `floor_penetration` at 96.61666 s | Clears original pose, then later contact rejection |

RUDE!'s later rejection is “More than two simultaneous contacts”, detected during
preparation at playback time 100.75 s. The pinned baseline's **contact-only**
planner reproduces it independently of pose solving. Its SSC counterpart already
had that rejection in the full audit. This repair leaves that separate planning
issue explicit; no note is skipped or retimed to manufacture a whole-chart pass.
The *It's Me* and *Mmchk* SSC counterparts continue to pass too.

Off-screen checks:

- **181 shared native tests** pass. New first-party cases cover an accurate-tip
  low-knuckle warm start, preserving digit controls during a small exact-mode
  vertical repair, and a short corner/support transfer with a 55 ms planted
  phase followed by a 70 ms flight. The latter checks subframes at 8 kHz near
  lift-off, model/contact/floor bounds, finite derivatives and C2 joins.
  No private chart excerpt or sampled dance is embedded in these tests.
- **79 fixed motion/category expectations** hold: 73 passes and six existing
  expected contact rejections. This includes all 32 category/mirror scenarios
  plus the 9B, seven-key and Techno fixture sets. No new pose failure, timeout,
  fatal deadline, starvation or settling warning occurs in these checks.
- **Seven reference dances** retain bit-identical poses, velocities and
  accelerations under matched preparation schedules: Beatucada Medium/Challenge,
  five-key, Solo, Pump, seven-key and Techno. Their complete contact plans match.
  All three affected SM cases retain identical finger assignments, releases,
  landings and events through their baseline certified contact prefixes.
- The three corresponding SSC expectations remain two passes and RUDE!'s known
  contact rejection. The two repaired SM full passes replay every expected head
  and hold under the actual judge, without guide/settling warnings. They use
  2.69 s and 3.48 s of pose/validation CPU under the unchanged 10 s cap; RUDE!
  uses 2.58 s before its later contact rejection. Accepted exact error maxima on
  those two full passes are 1.514 mm position and 0.063 mm planted height.
- The **12-play off-screen GPU lifecycle**, 21 native adapter/parity tests,
  wasm32 compilation, formatting and clippy pass, with inherited warnings only.

**The full library audit was not rerun.** Its preceding counts remain the
historical `bc1895cf` snapshot, not inferred post-repair totals. The later RUDE!
contact rejection and existing full-library timeouts still need separate work.
This focused check does not certify arbitrary Pump charts or browser frame times.
Private source/binary pins, contact traces, comparison schedules and evidence:
`/private/tmp/ddi-pump-pose-repair-20261106`. Small ignored aggregates and report:
`target/scratch/hand-pump-pose-repair-20261106/`. Private inputs, development
harnesses and experimental snapshots remain outside Git.
