# Dancing hand in gameplay

The default native and web builds include the dancing hand. Display options
control its visibility, coordinated field/hand layout and frame size. New settings
start with the hand enabled; saved Off choices remain Off. Four-panel, dance-double eight-panel, Solo
six-panel, DanOni five-key, seven-key, 9A and 9B, Pump single/half-double/double and Techno single8 charts
are supported. Solo diagonals use the upper mat corners; Oni uses the center (either pad center in 9A);
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
dark wrist cap, silhouette backing and rounded contact-reactive buttons: nine
on single mats, eighteen on dance-double and Pump half-double/double mats.
Settings' preview shows a standing pose and the active mat geometry from the same model. Appearance does not
alter contact timing or the physical mat plane.

Desktop display predictions can move backwards slightly when pacing estimates
settle across plays. Preparation keeps its last forward timestamp: an earlier
request only samples its exact retained, checked pose, with no planning or fitting.
Returning to the same timestamp restores its pose too. Uncovered backwards requests
and jumps beyond the rolling window still require restarting. The runtime debug
report counts `presentation_revisions`; this does not relax contact tolerances or
recover arbitrary buffer underruns.


The [double-mat integration](hand-double-mat.md#dance-double-gameplay-2026-11-08)
now enables dance-double, Pump half-double/double and DanOni 9A using shared physical
geometry, the current hand model, No Stretch and At once Two on the hand's copy.
9A Oni remains one judged button: a contact chooses physical center 5 or 14,
held/rolling contacts keep their chosen center, and replay unions both centers
before emitting press/release edges. Oni hazards constrain both centers. The
planner uses unscored corners and air time for escapes, preserving native
DanOni release budgets. No Stretch's separate general Options toggle is unchanged.
The 18-tile renderer, CPU contact indicators, light spill, camera framing and
Options preview use the same geometry. Pre-start readies its opening CPU/GPU
buffer; restart/layout changes rebuild or invalidate the relevant cached output.

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


## All playable hand layouts: full library audit (2026-11-08)

This run pins production **`94876b81`**, after double-mat and DanOni 9A support.
It freshly imports all **1,749 unchanged private simfiles**: **11,898 original
records + 202 generated versions = 12,100**. Of these, **11,866 records / 11,450
distinct inputs** are eligible. The hand plays its own **No Stretch + At once
Two** copy; the Player and normal gameplay retain their judged chart. This is
the post-rebase independent-hand-copy pipeline, unlike the historical shared-
chart audit above. No runtime, model, motion policy or tolerance is changed for
the audit.

Four independent chart processes each run the single-threaded f64 solver with
the existing three-second rolling window. Native preparation is driven at 60 Hz
with its real work budgets; numerical/contact checks run at 480 Hz and exact
obligation times, preserving the accepted stream-arrow policy. CPU caps remain
5 s contact / 10 s pose+validation, with a separate 90 s wall watchdog. There is
no media rendering. Hardware is an M1 Max (8 performance / 2 efficiency cores),
with four OS-managed workers and no affinity pinning.

**Elapsed: 10,511.87 s (2 h 55 min 12 s).**

| Outcome | Original | Generated | Total |
|---|---:|---:|---:|
| Full pass | 11,481 | 194 | **11,675** |
| Contact failure | 62 | 4 | **66** |
| Pose failure | 66 | 4 | **70** |
| Pose CPU timeout | 55 | 0 | **55** |
| Unsupported layout | 234 | 0 | **234** |

Contact CPU timeouts, wall/infrastructure timeouts, fatal playback deadlines,
import/infrastructure failures and settling warnings are **zero**. Five passing
records have five nonfatal guide-lateness events, recorded separately. Full
passes represent 11,262 distinct inputs; 98.4% of eligible records pass.

### Coverage and failures

| Layout present | Pass | Contact failure | Pose failure | Pose CPU timeout |
|---|---:|---:|---:|---:|
| Dance Single | 7,236 | 16 | 0 | 55 |
| Dance Double | 3,860 | 44 | 70 | 0 |
| Dance Solo | 378 | 0 | 0 | 0 |
| DanOni 5 | 90 | 4 | 0 | 0 |
| Pump Single | 92 | 2 | 0 | 0 |
| Pump Double | 14 | 0 | 0 | 0 |
| Techno single8 | 5 | 0 | 0 | 0 |

Unsupported records are **207 Dance Couple + 27 Dance Three-panel**, neither
currently playable. There are no unsupported judged note kinds. No Pump half-
double or DanOni 7/9A/9B records occur in this library; their authored regressions
remain separate coverage. Twenty-one auditor/parity tests pass, and separate
all-eleven-layout selfchecks give twenty passes plus one intentional native
hold/hazard rejection.

All **70 pose failures are Dance Double**: 20 knuckle height, 11 position,
17 floor penetration, three planted height, 18 tile margin and one subframe
planted-vertical check. Sixty-eight are runtime rejections; two are independent
480 Hz subframe findings (0.255590 mm height versus 0.25 mm, and 2.008824 mm
airborne targeting error versus 2 mm). Both remain failures. Contact failures
comprise 45 no-supported-plan, 17 boundary-continuation and four more-than-two-
contact rejections. The private reports retain exact rejected ticks and both
playback-disable and future prepared-failure times. No tolerance is widened.

A separate authored delayed-opening Pump half-double reproducer also fails the
uninstrumented production adapter with `tile_margin`. It is preserved privately
as follow-up evidence, not added to the library denominator or fixed during this
run. Remaining double-layout preparation/continuation failures are the next
correctness work; CPU optimization remains separate.

### Comparison and verification

Against `bc1895cf`, all source IDs and 1,749 file hashes match:

- **All 7,776 former passes still pass.** No previous pass fails or times out.
- **3,988 formerly unsupported records become eligible:** 3,874 pass, 44 fail
  contacts and 70 fail poses.
- **23 of 78 previous pose timeouts now pass; 55 remain**, all Dance Single.
- Two of the three previous Pump numerical failures now pass; the other reaches
  its later contact rejection. All 21 existing contact failures remain contacts.

This comparison spans hand-copy policy/identity and physical auditor changes,
not an isolated performance or motion change. Caps and worker count are retained;
no relaxed-cap or serial rerun replaces the canonical results.

Independent verification checks fresh binary/catalog/frozen-source hashes,
complete IDs, equal alias outcomes, all four shards, disjoint result partitions,
all passing judge/contact/numerical checks and original inputs unchanged. Input
identity includes **both original gameplay and effective hand notes**, keeping
raw gameplay tally differences out of shared motion-result aliases. The auditor
uses explicit physical mat centers and both Oni hazard targets. Retained opening
support spots are checked against the requested target's tile; exact scored
landings are independently checked against their declared physical tile.

Passes cover **4,328,104 exact heads, 223,741 hold tails, 45,670 avoided mines and
eight lifts**. No shock-bearing chart occurs here; authored shock checks remain
separate. Review checks 34,425 source-record judge reports and 33,186 distinct-
input replays, plus independent original-game COMBOS/autoplay weighting for all
300 passing custom-table records. Accepted maximum exact error is 1.986351 mm,
maximum planted height 0.248653 mm and minimum tile margin 1.046918 mm, within
the unchanged 2 / 0.25 / 1 mm limits. No passing trace misses a head, drops a hold,
starves or has a fatal deadline/settling warning.

Canonical private inputs, frozen code, binaries and traces:
`/private/tmp/ddi-library-all-layouts-20261108-v1`. Native adapter/parity evidence:
`/private/tmp/ddi-all-layout-library-adapter-20261108`; authored studies:
`/private/tmp/ddi-all-layout-audit-selfchecks-20261108`. Small ignored aggregates:
`target/scratch/library-dance-audit-all-layouts-20261108/`, including `REPORT.md`,
separate contact/pose/timeout/unsupported/settling/guide CSVs, `pose-details.csv`,
comparison and independent verification JSON. Private inputs, plans, development
harnesses and captures remain outside Git. This is native gameplay-math coverage,
not desktop/browser rendering or realtime frame-time certification.


## Dance Double pose repairs: targeted checks (2026-11-08)

The 70 Dance Double pose-failure records from the full audit above now yield
**67 full passes and three later contact rejections**, with **zero pose failures,
contact/pose/wall timeouts, fatal deadlines or settling warnings**. This is a
targeted rerun, not a new full-library result. Four independent chart workers
retain the original 5/10 s CPU caps and 90 s wall watchdog; each chart still runs
single-threaded with the three-second window and f64 math.

The fixes address two distinct faults. Before its first scored row was
committed, an empty rolling boundary could forget the chosen double-mat opening
support and revert to the default tile IDs. The boundary now retains that rest.
A first-party regression covers Dance Double, Pump half-double/double and
DanOni 9A, including publications before the first scored note.

Some correction fits also stalled near joint bounds, while others repeatedly
refitted an already-valid anchor instead of the failing interpolated pose.
Double-mat correction now uses the failed interpolant as a warm start, can move
an unprotected numerical knot, and retains every contact/phase timestamp. A
bounded balance-guide nudge toward the fingertip midpoint escapes stalled fits;
low-knuckle cases can use more of it. Checked C2 tangent repairs reduce local
overshoot. Ordinary preparation still uses six passes; difficult double-mat
curves can use six further passes with numerical knots down to 0.25 ms. The knot
cap remains 32,768 and every fitting checkpoint remains capped at eight
iterations. Presentation sampling remains solve-free and published prefixes
remain immutable. Single-mat preparation retains its previous behavior.

Double-mat preparation keeps reserve inside the unchanged published limits:
1.90 mm targeting, 0.225 mm planted height and 40.01 mm knuckle clearance during
checks, with the existing −0.20 mm floor reserve. Output still allows 2 mm total
error, 0.25 mm planted height, 1 mm tile clearance and a 40 mm knuckle minimum;
no model, timing or contact tolerance was relaxed. The independent 480 Hz and
exact-event checks measure maxima of **1.914968 mm** target error and
**0.221871 mm** planted height, with **2.368132 mm** minimum tile clearance.
Completed charts use at most 0.151 s contact CPU and 6.352 s pose/validation CPU.
All 6,933 already-played scored heads retain their exact baseline times/tiles.

The three later contact rejections are **south Challenge (tick 7152)**,
**Couleur=Blanche Medium (4728)** and **チルノのパーフェクトさんすう学園 Hard
(6372)**. Each also rejects with the unchanged pinned baseline planner in
contacts-only execution. They remain separate failures, not whole-chart passes;
this work does not change their contact policy.

Validation includes 119 native hand tests and authored stalled-fit/floor
regressions; the 79-case category gate (73 passes and six expected contact
rejections, no mismatches/timeouts/poses); clippy on the hand crates and shell;
and 12 automatic/mirrored Dance Double cases in native and headless Chrome/WASM,
with identical whole contact plans, dense edge checks and immutable C2 prefixes.
Seven accepted single-mat reference dances, including both Beatucada charts,
retain bit-identical contacts, poses, velocities and accelerations against the
pinned full-audit baseline with replayed work schedules.

**The full library audit was not rerun.** Its counts above remain historical.
Private raw evidence: `/private/tmp/ddi-double-pose-repair-20261108`, with the
production rerun in `final/`. Small ignored summaries and source hashes:
`target/scratch/hand-double-pose-repair-20261108/REPORT.md`. Private charts,
plans, probes and development binaries remain outside the repository.


## Mine-to-head and staggered clearance (2026-11-08)

The three contact rejections exposed by the Double pose repairs now pass:
**south Challenge**, **Couleur=Blanche Medium** and
**チルノのパーフェクトさんすう学園 Hard**. Manual contact studies first passed the
actual SM5, DDR-A and ITG judges with exact note times. The implementation then
passes all three complete charts through the native gameplay/pose pipeline.

- **Reserve the scored landing.** A bounded additional hazard candidate can
  continue a clearing flight directly to a received tap or hold, including a
  repeat on the same tile. It can take off earlier instead of extending a fixed
  short hop until its early-boundary height becomes impossible. Directed flights
  remain at most 450 ms and retain current planted/swing floors and lift limits.
  South now keeps the left finger on L6 while the right taps R4 at 63.429 s,
  lifts at 63.484 s and lands into its hold at 63.857 s.
- **Plan simultaneous clearance together.** The additional candidate groups
  hazards at the same timestamp, checking the completed stance rather than
  rejecting an intermediate one-foot destination. When the legacy escape
  variants fail, two bounded grouped escape alternatives are available. The
  total stays at most five candidates per parent within the existing beam.
  Couleur can put both fingers safely on an unscored center during its full
  mine row, then land UP at exactly 32.108 s. Cirno uses independent finger
  departures/landings and safe-center support between its tight scored presses.
- **Known scored approaches after a mine.** Cirno's taps about 84 ms after a
  mine pass all actual judges but conflict with the old unconditional 95 ms
  clearance rule. A scoped per-foot proof requires the exact received scored
  event and matching incoming swing. Clearance remains required through the
  mine and until 5 ms after the midpoint where the scored head becomes closer
  than the old mine. A finger coming from that same tile must have departed
  before its early window. The exception expires at the next landing; it does
  not authorize an unscored repress, an unrelated finger, or mine-time contact.
  The pose plan derives and bounds this cache from validated events/tracks;
  no serialized permission or new source-note timing is introduced. A pruned
  view retains only proofs for declared surviving flights, keeping future
  scored events hidden. Probe times include the permission boundary.
- **Do not steal the next note.** Fixed unscored hazard returns are rejected
  when they would press a received upcoming head early. An exact known head
  can still reuse its landing. For incoming charts, a scored-tile return is not
  committed until its whole early-hit neighborhood is received, or EOF proves
  there is no future head. The neighborhood comes from the effective judge
  tables, separately from hold-release budgets. Existing boundary logic keeps
  the candidate tentative and the published prefix before its mutable flights.
  This fixes a later Couleur section without rewinding or changing note times.

| Full chart | Exact heads | Max target error | Max planted height | Pose/validation CPU |
|---|---:|---:|---:|---:|
| south Challenge | 267/267 | 0.866 mm | 0.071 mm | 4.881 s |
| Couleur=Blanche Medium | 440/440 | 1.105 mm | 0.164 mm | 5.068 s |
| チルノのパーフェクトさんすう学園 Hard | 391/391 | 1.710 mm | 0.184 mm | 7.462 s |

The final targeted cohort includes the 67 recently passing Double controls:
**70/70 pass**, with zero contact/pose failures, CPU/wall timeouts, fatal
deadlines, guide warnings, starvation or settling warnings. Four independent
chart workers retain the original 5/10 s CPU caps and 90 s wall limit; each
chart still uses one thread, f64 and the three-second window. The maximum
pose/validation CPU is 7.669 s. Independent 480 Hz and exact-event checks retain
2 mm total targeting, 0.25 mm planted height, 1 mm tile clearance, 40 mm knuckles
and all authored model bounds. The model/assets, No Stretch/At once Two policy
and actual judge windows are unchanged. All 67 controls retain identical scored
events (including finger assignments) and holds; 63 also retain identical swing
tracks, while four have changed clearance/takeoff tracks.

Validation includes 126 native hand tests, five authored mirrored scenario
families (same-tile hold, jump, alternating hits, mine clusters and incoming-note
uncertainty), negative proof/visibility tests, the 79-category gate (73 passes
and six expected contact rejections, no mismatches/poses/timeouts), and clippy
with warnings denied. The ten new motion cases pass in headless Chrome/WASM
with whole plans identical to native, dense edge checks and frozen C2 prefixes.
Seven accepted reference dances retain bit-identical contacts, poses, velocities
and accelerations against `cbcee284` with replayed work schedules.

**The full library audit was not rerun.** Earlier aggregate audit counts remain
historical; this follow-up only certifies its three repaired records and 67
controls. Private raw evidence/probes are under
`/private/tmp/ddi-mine-to-head-repair-20261108`; small ignored summaries are in
`target/scratch/hand-mine-to-head-repair-20261108/REPORT.md`. Private charts,
plans and development tools remain outside Git.


## Original contact-failure cohort: targeted rerun (2026-11-08)

Rerunning the **66 original contact-failure records** against `259654b1` now gives
**43 full passes and 23 remaining contact failures**. There are **zero pose
failures, contact/pose CPU timeouts, wall/infrastructure timeouts or failures,
unsupported-layout/note-kind results, settling warnings, guide warnings or starved
frames** in this cohort. All 66 distinct effective input identities and all 49
source-file hashes match the baseline; this includes 62 original and four
generated chart records. The three separately repaired south/Couleur/Cirno
records were not part of these original 66.

| Layout | Full pass | Contact failure |
|---|---:|---:|
| Dance Double | 33 | 11 |
| Dance Single | 10 | 6 |
| DanOni 5 | 0 | 4 |
| Pump Single | 0 | 2 |
| **Total** | **43** | **23** |

The remaining failures comprise **19 no-supported-plan rejections** and **four
reported more-than-two-simultaneous-contact rejections** (RUDE! Pump and converted
DanOni, SM/SSC). The latter are contact-stage capacity rejections on supported
layouts, not newly unsupported layouts or note kinds; their cause remains to be
investigated. No judge-boundary continuation rejection remains: 14 of the former
17 continuation records pass fully, while three advance to later planning
rejections. Of 45 former no-supported-plan records, 29 pass and 16 still reject.

Four independent off-screen chart workers keep the single-threaded Rust gameplay
pipeline, f64, three-second window, 5/10 s contact/pose CPU caps and 90 s wall
watchdog. Checks retain 60 Hz preparation, 480 Hz plus exact obligations and all
current model/contact limits. No media was rendered or recorded. The run took
53.29 s; passing charts use at most 8.325 s pose/validation CPU. Their maximum
target error is 1.573714 mm, maximum planted height 0.190899 mm and minimum tile
clearance 1.812192 mm.

**This was not a full audit. The separate 55 prior pose CPU timeouts were not
rerun.** No runtime, policy, model or tolerance changed in this task. Remaining
records and rejected ticks/music times are in separate ignored CSVs under
`target/scratch/hand-contact66-rerun-20261108/`, alongside `REPORT.md`, summary and
run configuration. Playback-disable time is listed separately from the rejected
row's music time, since lookahead can report a future failure before playback
reaches it. Raw private results, inspections, plans, pinned executable and adapter
source are in `/private/tmp/ddi-contact66-rerun-20261108`; private chart data and
developer tools remain outside Git.


## Shared At once Two lift cap (2026-11-08)

The four remaining RUDE! Pump/converted-DanOni capacity rejections came from
three simultaneous lifts. The shared simplifier counted lifts but protected
all of them from removal, following the original StepMania policy. **Up to two**
now trims leftmost excess lifts after its existing tap/new-head removals, in
both the ordinary count pass and the combined No Stretch/count pass. Existing
holds/rolls count through their tail; kept notes retain exact ticks, timestamps
and attributes. Any and One keep their previous behavior, including One's
legacy lift protection. This is a shared engine policy, not an animation-only
skip or a relaxation of contact/model tolerances.

All four affected full charts now pass: RUDE! Edit as Pump Single and converted
DanOni 5, each from SM and SSC. Testing each with the default hand copy and with
At once Two selected for gameplay gives **8/8 passes**, with no contact/pose
failures, CPU/wall timeouts, settling/guide warnings or starvation. Each version
removes exactly one lift at tick 10608; no other effective note changes from the
previous inspection. With gameplay's option enabled, its judgeable notes and
the hand notes match exactly, including attributes and song times. The default
and selected-option runs also produce identical effective hand notes.

Across these eight runs, maximum target error is **1.663403 mm**, maximum planted
height **0.166938 mm** and minimum tile-edge clearance **2.484880 mm**, within the
unchanged 2 mm / 0.25 mm / 1 mm limits. All scored contacts and hold tails pass
the actual judge and exact-event checks. The three-second rolling window,
single-threaded f64 solver and 5/10 s contact/pose CPU caps remain unchanged.

Verification: **382 native engine/hand tests pass**. Authored regressions cover
three lifts, mixed tap/hold/roll heads, active holds through the tail, hazards,
retained metadata, idempotence and gameplay/hand agreement across single and
double layouts.
The off-screen gameplay driver also preserves all **79 category expectations**
(73 passes, six expected contact rejections; zero mismatches, pose failures or
timeouts). Warnings-denied engine/hand-plan all-target clippy, hand-plan wasm32
compilation and formatting pass. No full library audit or media capture was run.
Private inputs, inspections, traces and reports remain under
`/private/tmp/ddi-lifts-repair-20261108`; only authored tests and documentation
are committed.


## Silent Hill: alternating support and flight (2026-11-08)

Silent Hill's Dance Double Challenge contact rejection at tick 8352 is repaired.
A private controlled sequence first passed SM5, DDR-A and ITG with five exact
taps and all 16 mines avoided. The finger on pad 2 lifts while the other hits
pad 1, then they exchange support. Brief overlapping airborne phases are allowed
by the existing flight model; an escape landing between every hit is unnecessary.

Two planner restrictions blocked it:

- At the received horizon, both fixed center escapes failed the existing
  nominal stance limits beside the seam-facing supporting finger: 4.72 mm and
  67.28 mm lie outside Dance Double's unchanged 6–66 mm envelope. A bounded
  recovery can now park the single threatened finger on a nearby **unscored**
  tile, keeping its companion's track fixed. It reuses the existing legal-stance
  support helper, at most two targets, normal lift/hold checks, mine clearance
  and actual judge validation. Its auxiliary landing must still match an
  original hazard; a forged witness is rejected.
- Reserving a tap and a later escape for the same finger made the earlier tap
  disappear from the old last-landing check. Local-support recovery histories
  now consume that exact matching tap/swing/auxiliary reservation without
  rewinding their later pending track. Long-note heads keep the old final-landing
  requirement, so this adds no untracked hold-release allowance.

The recovery is attempted **only after every legacy candidate for a row fails**,
once per nearby-hazard row on a double mat, within the existing CPU cap. It
retains the five-alternative per-parent checkpoint bound, single-threaded cooperative
checkpoints, received-note visibility and frozen prefixes. Successful legacy
histories never opt into the intermediate-tap rule. Seven full-chart controls
(the four RUDE! SM/SSC Pump/DanOni versions, south, Couleur=Blanche and Cirno)
retain identical scored events, finger assignments, holds and swing tracks.

The full Silent Hill chart now passes: **282/282 exact events, 33 holds and all
416 mines avoided**, with actual SM5, DDR-A and ITG judge replays. Maximum target
error is **0.899732 mm**, maximum planted height **0.079054 mm**, and minimum tile
clearance **2.471524 mm**. Contact CPU is 0.0734 s and pose/validation CPU 4.1912 s,
within the unchanged 5/10 s caps. All eight affected/control charts pass with no
contact/pose failures, CPU/wall timeouts, settling/guide warnings or starvation.
Model/assets, source note times, No Stretch, At once Two, f64, the three-second
window and contact/pose tolerances are unchanged.

Authored tests cover alternating support/flight in both mirror orientations,
exact event times, judge replay, one-finger-only clearance, retained holds,
hazard-witness rejection, C2 published prefixes and dense skin/floor/contact
checks. The off-screen category gate retains all **79 expectations** (73 passes,
six expected contact rejections, no mismatches/pose failures/timeouts).
All **128 native hand tests** pass, including both mirrored motion studies and
the local-support/witness guards. Warnings-denied hand-plan/hand-pose all-target
clippy, hand-plan wasm32 compilation and formatting pass.

**No full audit or original-66 rerun was performed.** Earlier aggregate counts
remain historical. Private controlled plans, chart inputs, traces, comparisons
and run reports are under `/private/tmp/ddi-silent-support-20261108`; they are not
repository fixtures or runtime assets.


## Remaining contact-failure records after lift/support fixes (2026-11-08)

A targeted rerun of the **18 unresolved records** against `e7e9e8de` gives
**six full passes and 12 contact-planning rejections**. The four RUDE! versions
and Silent Hill were already verified passing and excluded. There are **zero
pose failures, contact/pose CPU timeouts, wall timeouts, infrastructure failures,
unsupported charts, settling/guide warnings or starved frames** in this rerun.

New passes are **Axion Double Challenge (SM/SSC)** and **G-SYMPHONiCORE Double
Hard and Challenge (SM/SSC)**. The remaining 12 no-supported-plan records are:

- Dance Single: KEEP ON MOVIN’ Challenge, Another Phase Challenge, 人マニア Hard,
  混乱少女♥そふらんちゃん!! Challenge, Megalovania Challenge (SM/SSC): six records.
- Dance Double: KEEP ON MOVIN’ Challenge, 1116 Challenge, 人マニア Hard and
  混乱少女♥そふらんちゃん!! Challenge: four records.
- Converted DanOni 5: UNIQUE Edit (SM/SSC): two records.

All failures remain at their previous rejected ticks. All 18 distinct effective
input identities and 13 source hashes match the preceding run. Four independent
workers keep each chart single-threaded, f64, the three-second window, the
5/10 s contact/pose CPU caps and 90 s wall watchdog. Native preparation is 60 Hz;
independent validation is 480 Hz plus exact obligations. The run took **8.98 s**.
No runtime, motion policy, model or tolerance changed in this audit task.

Passing charts have at most 0.449435 mm target error and 0.075447 mm planted
height, with at least 2.433671 mm tile clearance. Maximum completed-chart
pose/validation CPU is 3.223 s. Reports and separate failure/timeout CSVs are in
`target/scratch/hand-contact18-rerun-20261108/REPORT.md`; private evidence and the
pinned driver are under `/private/tmp/ddi-contact18-rerun-20261108`.
**This was not a full audit. The earlier 55 pose CPU timeouts were not rerun.**


## Timed mine departures and arrivals (2026-11-08)

Tracing 1116, 人マニア and 混乱少女♥そふらんちゃん!! shows two related restrictions:
1116's between-mine tap and the fast stream's final tap needed a scored departure
inside the old conservative early window, while 人マニア's next tap was 100 ms
after a mine, outside the old 95 ms guarded-approach source-gap threshold. The
fast stream's adaptive attempt got beyond its first reported rejection to the
last pre-mine tap; a later 115.8 ms head-to-mine interval needed the same rule.
A controlled 1116 passage first passes all six exact hits and avoids 16 mines
under SM5, DDR-A and ITG without changing the judge or chart timing.

The planner can now reserve guarded departures and arrivals during the existing
failed-row retry, on single and double layouts. All existing candidates must
fail before the retry runs. It keeps the beam/checkpoint bounds, min planted and
swing times, lift cap, hold budgets and actual-judge replay. Successful legacy
histories keep their decisions. A candidate arrival before a mine is provisional:
acceptance still needs a matching received scored head and real outgoing swing;
another finger's future time or the current window end cannot act as a release.

Physical checking derives the corresponding per-foot intervals from exact
heads and their incoming/outgoing tracks. Arrival clearance ends at the midpoint
plus 5 ms; departure clearance starts at the midpoint minus 5 ms. The **170 ms**
source-gap bound is where those guards meet the **unchanged ±90 ms physical
window**. It does not change judge windows or output tolerances. Unknown or
wrong-foot heads, lifts/unscored contacts and late/missing departures do not
qualify. A proof expires at the next landing. The non-serialized derived cache
has a shared 32,768-entry cap; visibility retains the declared departure but
not future scored events, and drops proofs without their actual tracks.

All **five affected full charts pass** through the off-screen gameplay/pose
pipeline: 1116 Double Challenge, and Single/Double charts of 人マニア Hard and
混乱少女♥そふらんちゃん!! Challenge. Exact hit counts are respectively
739, 425/423 and 804/757, with all holds and mines satisfied. Maximum exact
target error is 1.267169 mm, planted height at most 0.078598 mm, and minimum tile
clearance 2.137036 mm. Pose/validation CPU is at most 5.883 s within the unchanged
10 s cap (contact cap 5 s). The f64, single-threaded three-second runtime,
model/assets, No Stretch, At once Two and accepted dense-stream policy remain
unchanged. There are no contact/pose failures, timeouts, settling/guide warnings
or starvation in the five affected and 14 control runs. All 19 accepted inputs
are unchanged; all **14 controls retain identical scored events, assignments,
holds and swing tracks** compared with their accepted versions.

Verification: **132 native hand tests pass**, with authored mirrored arrivals,
departures and brackets, C2 prefix checks, strict exact-mode contact checks and
negative proof/visibility tests. The two isolated brackets retain 83 ms gaps
with a quiet interval so that precision study stays in exact mode; the full
charts exercise normal stream behavior. Older blanket-window assertions now
independently check the proven scored-contact intervals and remain strict
outside them. The independent gameplay audit also probes both sides of each
guard boundary while keeping every existing geometric limit. All **79 category
expectations** hold (73 passes, six expected contact rejections; no mismatches,
pose failures or timeouts). All-target clippy with warnings denied, hand-plan
wasm32 compilation and formatting pass. Private raw evidence, traces, controlled
plans, independent checker and `REPORT.md` remain under
`/private/tmp/ddi-timed-air-20261108`; no private charts or development tools
are committed.

**No full library audit was run.** The seven other records from the previous
12-contact cohort and the 55 historical pose CPU timeouts were not rerun here.


## Full library rerun after timed-hazard repairs (2026-11-09)

Pinned runtime **`cff1cefc`**, fresh inventory of the same **1,749 private files**:
**11,898 originals + 202 generated = 12,100 records**, with 11,866 eligible
records / 11,450 distinct eligible inputs. Four independent chart workers each
keep the single-threaded f64 solver, three-second window, No Stretch + At once
Two hand copy and existing contact/model limits. Preparation runs at 60 Hz;
independent checks run at 480 Hz, exact obligations and both sides of mine guard
boundaries. Caps remain 5 s contact / 10 s pose+validation CPU and 90 s wall.
Everything runs off-screen; there are no videos or runtime/model changes.

**Elapsed: 11,113.28 s (3 h 5 min 13 s), excluding inventory and review.**

| Outcome | Original | Generated | Total |
|---|---:|---:|---:|
| Full pass | 11,591 | 200 | **11,791** |
| Contact rejection | 7 | 2 | **9** |
| Pose failure | 0 | 0 | **0** |
| Pose CPU timeout | 66 | 0 | **66** |
| Unsupported layout | 234 | 0 | **234** |
| Settling warning | 0 | 0 | **0** |

Contact CPU/wall timeouts, fatal deadlines, infrastructure/import failures and
incomplete cases are zero. Two passing records have two nonfatal guide-lateness
events, kept separate from settling warnings. **99.37% of eligible records pass.**
Unsupported records remain 207 Dance Couple and 27 Dance Three-panel charts;
there are no unsupported judged note kinds. This corpus still lacks Pump
half-double and DanOni 7/9A/9B examples; all 21 serial auditor self-check
expectations pass across the eleven supported layouts.

Against the previous full run (`94876b81`), **all 70 former pose failures and
61 of 66 former contact failures pass**. Five contact rejections remain: KEEP
ON MOVIN' Double Challenge, Megalovania Single Challenge (SM/SSC) and converted
UNIQUE DanOni 5 Edit (SM/SSC). KEEP ON MOVIN' Single and Another Phase Single now
pass. There are also **four new contact regressions**: G-SYMPHONiCORE Single
Challenge/Edit (SM/SSC), rejecting at playback 33.25 s because air-time clearance
does not span an original hazard. The accepted Double controls are distinct.

Of the 55 former pose timeouts, 54 remain and one passes; **twelve former passes
now time out**, yielding 66 CPU timeouts (65 Single, one Double). These are not
geometric rejections. The new records are DYNAMITE RAVE(Long ver.) Single/Double
Hard; The Words I Never Said Easy; Chemicals Challenge/Hard/Medium; Big Sky
Beginner (SSC); Drone Strike Medium (SM/SSC); Luminescence Challenge; and Summertime
(2013 mix) Easy (SM/SSC). Runtime repairs and stricter guard probes are included;
this audit does not isolate extra CPU cost, and caps do not normalize asymmetric
cores. No relaxed-cap outcome replaces a canonical result. Net full passes rise
by 116, while 16 former passes do not pass (four contacts, twelve CPU timeouts).

Independent review verifies exact source/catalog IDs, all four shards, frozen
sources/pins, unchanged inputs, alias consistency, disjoint categories, actual
judge replays, weighted original-chart autoplay, contact bounds and complete
prefixes. Passing records cover 4,373,636 exact heads, 229,032 hold tails, 61,406
avoided mines and 24 exact lifts. All source hashes match; effective identities
match for 12,096 records, with only the four accepted RUDE! lift simplifications
differing. Review caught an inherited inspector count error for those four:
original gameplay has five lifts, the effective hand source has four. Fresh
pinned-binary identity checks and archived lift ticks/times prove four; only
review metadata is corrected, leaving frozen catalogs, runtime outputs, original
gameplay autoplay, limits and outcomes untouched. Future inspectors must count
hand obligations from the hand source. The erratum has its own verification JSON.

Reviewed reports and separate existing/new contact/timeout CSVs are under
`target/scratch/library-dance-audit-timed-air-20261108/REPORT.md`. Raw catalogs,
frozen sources, pins, plans and review logs stay private at
`/private/tmp/ddi-library-timed-air-20261108`; no private inputs or developer tools
are committed. The next correctness focus is the four G-SYMPHONiCORE Single
regressions, before performance work on the timeout cohort.


## G-SYMPHONiCORE guarded-departure validation (2026-11-09)

The four Single Challenge/Edit regressions (SM/SSC) came from a mismatch between
search/physical guards and auxiliary-contact validation. Preparing ahead at
playback 33.25 s rejected a future flight: the finger hits UP at 35.992 s,
releases at 36.047 s and lands on RIGHT with the UP mine at 36.133176 s. The
existing scored-head guard requires clearance from 36.057588 s, but auxiliary
validation still demanded takeoff before the full conservative 95 ms window.

Auxiliary validation now recognizes that existing guarded source departure for
an exact received destination head. It requires the connected incoming flight,
a matching same-finger/panel non-lift source event, its received head's tick and
exact time, and the existing midpoint-minus-5ms departure guard. Mine identity,
known destination head, bounded flight, actual judge replay and physical checks
remain mandatory. Ordinary hops/unscored escapes keep their prior validation.
Search choices, model/skin, timing, three-second single-threaded f64 execution,
recovery budgets and 2 mm / 0.25 mm / 1 mm contact limits are unchanged.

All **four affected full charts pass** with exact zero-delta hits: Challenge has
1,090 hits, 29 holds and 59 avoided mines per format; Edit has 1,306 hits, 21 holds
and 49 avoided mines. Maximum affected target error is 0.212441 mm, planted
height 0.018526 mm and minimum tile margin 2.495298 mm. Pose/validation CPU stays
below 3.611 s against the unchanged 10 s cap (contacts: 5 s).

Verification is off-screen and targeted: **35/35 charts pass**, covering all 20
G-SYMPHONiCORE records and the previous repair's affected/control charts. Four
independent workers keep each chart single-threaded; native preparation is
60 Hz and independent validation 480 Hz plus exact guard boundaries. All **31
unaffected controls retain identical complete contact plans**; the four repaired
charts preserve their previously checked events/holds and flights through
33.233333 s. Source identities, model and pose sources are unchanged. There are
no contact/pose failures, timeouts, settling/guide warnings or starved frames.

A new authored mirrored unit case fails before the fix and passes after it,
rejecting seven forged proofs per mirror. Mirrored rapid-repeat departures also
join whole-plan and rolling skin/prefix checks. All **133 native hand tests** and
**79 category expectations** pass (73 passes, six expected contact rejections),
along with all-target warnings-denied clippy, WASM32 compilation and formatting.
Raw evidence, pinned driver, source/model hashes, full plans and comparisons stay
private under `/private/tmp/ddi-gsym-contact-20261109/REPORT.md`.

**No full library audit was rerun.** The preceding full audit remains a historical
snapshot, including its timeout cohort; the targeted four passes do not revise
the global counts. Private inputs/tools remain uncommitted.


## Staggered landings and post-mine presses (2026-11-09)

All five remaining contact records in the requested cohort now pass complete
off-screen gameplay/pose checks: KEEP ON MOVIN' Double Challenge, Megalovania
Single Challenge (SM/SSC), and converted UNIQUE DanOni 5 Edit (SM/SSC).
The fixes are shared planning/validation rules, with no chart-name exceptions,
new note reductions, retiming, model/skin changes or relaxed contact tolerances.

- **KEEP ON MOVIN':** reserve the received scored landing rather than a short
  unscored return that strands the finger before the next mine row. A scored
  reservation is not an unscored restore stealing a future note. The other
  finger can land independently. The successful solution stays within the
  original 450 ms outward-flight bound and existing hazard scan; a larger
  exploratory allowance was unnecessary and removed.
- **UNIQUE:** adaptive timing passes the first two bursts but cannot swap roles
  for UP/ONI/RIGHT with a fixed 60 ms companion return. The free companion may
  instead land on its exact received next head inside that interval. Both source
  and destination head identities and opposite-finger ownership are validated;
  the reservation is consumed once. Planted/swing floors remain at least
  20/25 ms and holds/lifts/rolls retain their original rules.
- **Megalovania:** LEFT mine to LEFT tap is 0.750 ms; UP mine to that tap is
  0.125 ms. The actual judge accepts a precise press after the mine passed.
  The old approach helper excluded all heads within 10 ms and demanded excessive
  lift near touchdown. The narrow post-mine case now requires an incoming finger
  from another tile released before the full 95 ms conservative window, with an
  exact received non-lift head. It does not protect a stationary/same-tile finger,
  late release, wrong finger or unknown head. Other-finger hazards and later
  landings remain checked. This is a proven note/mine-priority interval, not a
  change to judge windows, contact times or geometric limits.

New staggered choices form a **last recovery tier**, gated behind failure of all
existing tiers and known rapid notes. It must add an actual reserved flight
beyond the frozen seed, within the same shared CPU budget and bounded beam.
Successful older tiers keep their decisions. Visibility keeps declared flights
backed by received heads while hiding future scored events; approach-proof
capacity and expiry remain bounded. Independent clearance checks cover the new
precedence case without calling the production rule helper.

**41/41 targeted charts pass**, with four independent workers, each single-
threaded/f64 and the existing three-second window. Preparation is 60 Hz;
independent checks are 480 Hz plus exact events and mine-boundary probes.
The 5/10 s contact/pose CPU caps and 90 s wall watchdog remain unchanged. All
**36 controls retain identical complete contact plans** (including all previously
accepted G-SYMPHONiCORE records and KEEP ON MOVIN' Single), and the five repaired
records retain their previously checked contact/flight prefixes. All input
identities and source hashes match. No targeted failures, timeouts, settling/
guide warnings or starvation occur. Across all 41, maximum exact error is
1.803627 mm, planted height 0.183544 mm and minimum tile margin 2.137036 mm,
within unchanged 2 mm / 0.25 mm / 1 mm bounds. Model, fitting and kinematic
sources remain unchanged; floor, skin, joint and 40 mm knuckle guards stay active.

All **136 native hand tests**, **79 category expectations** (73 passes, six expected
contact rejections), warnings-denied all-target clippy, WASM32 compilation and
formatting pass. Authored mirrored cases exercise all three patterns; negative
and visibility tests check both head receipts, source clearance, finger ownership,
expiry and frozen prefixes. Dense tests use the accepted stream policy between
notes with strict exact-head checks. The burst skin fixture includes its normal
hold/quiet continuation; a separate forced-empty-tail experiment hit a runtime
publication deadline. The following investigation resolves that test-harness
issue without changing production motion.

Raw evidence, source/model hashes, pinned final driver, full results and exact
comparisons stay private at `/private/tmp/ddi-staggered-landings-20261109/REPORT.md`.
**No full library audit was rerun** and historical global counts are not rewritten.
Private charts, plans and development tools remain uncommitted.


## Empty-tail harness investigation (2026-11-09)

The burst-only study's `runtime_publication_deadline` is reproducible at 7 s,
but is a test-caller contract error, not a solver CPU timeout or a gameplay
failure. The helper extends the pose plan to 14 s yet passes the contact
publication's `finished` flag directly to motion preparation. That flag means
all note rows were received; `DanceRuntime::request_bounded` instead requires
the final motion horizon. Stream fitting therefore closes at the current
window's temporary endpoint (7.25 s), and cannot extend its prepared output
through the empty tail. At 7 s the remaining overlap is too short for its
existing smooth publication splice.

Gameplay already uses the correct signal: `RuntimePreview::request_motion`
passes `known >= duration`, and `prepare` requests the retained plan every
frame, including after the last contact publication. Matching that signal alone
clears the original experiment. The shared test helper now also matches the
per-frame request cadence, caps received time at the motion endpoint, and checks
all 14 s instead of stopping its physical probes at 11 s.

A permanent burst-only regression covers both true left/right mirrors at 4 Hz
and 60 Hz update cadences. It verifies exact scored heads, frozen pose/velocity/
acceleration prefixes, current stream policy, floor/knuckle/joint bounds and
prepared coverage/sampleability through the exact 14 s endpoint. All four
variants pass without deadline failures or late-guide jobs. All existing shared
helper studies also exercise their full quiet tail.

All 137 native hand tests and all-target warnings-denied clippy pass off-screen.
Production solver, gameplay, timing, contact tolerances and model are unchanged;
no library audit was run. Reproduction traces and check logs are private at
`/private/tmp/ddi-empty-tail-20261109/`. The earlier synthetic-tail caveat is
resolved; historical full-audit counts and timeout cohorts are not revised.


## Single-commit gameplay handoff (2026-11-09)

Consolidate the 20 hand commits after the already-integrated No Stretch change
onto local `main` at `9b67a48d`. The full accepted history remains on
`codex/hand-audit-recovery` at `f6fb247f`; the cherry-pick branch is
`codex/hand-gameplay-update`. No fetch, pull, push, merge or history rewrite is
needed to prepare this commit.

The payload includes shared mat geometry, 18-tile rendering and gameplay for
Dance Double, Pump half-double/double and DanOni 9A; the shared excess-lift cut;
wide-motion, timed-hazard, staggered-landing and post-mine fixes; and authored
regressions and their documentation. No Stretch is already on `main`, and its
shared region lookup is now supplied by the same mat geometry. Research tools,
private inputs, audit plans/outputs and modeling archives remain outside Git.

Retain `main`'s newer preload API, shared model cache, on-demand debug sampling,
renderer texture interface and centralized `ddi_character::HAND_LAYOUTS`.
Extend that list by the four supported double layouts. Store validated mat
geometry on the runtime, rather than restoring its removed layout-string or
developer window controls. Options' preview uses its current style's geometry.
Core planner/pose source files carried by this patch remain byte-identical to
the accepted hand branch; assets, exact timing, three-second single-threaded
f64 preparation and existing contact bounds are unchanged.

Consolidation checks are off-screen: **1,208 native tests pass**, with two existing
ignored tests; all-target warnings-denied clippy passes for the nine affected
runtime packages; the full app checks for `wasm32-unknown-unknown`; and the
desktop hand lifecycle test passes through 20 consecutive completed/quit plays
across ten layouts/samples on one host. Native desktop checks omit the unrelated
video/importer helpers. Formatting for the affected runtime packages and diff
checks pass. No full library audit is rerun; prior audit results remain historical.

The unsigned cherry-pick commit has local `main` as its sole parent. Neither
`main` nor the source branch is advanced. Private consolidation logs and source
parity checks remain at `/private/tmp/ddi-hand-consolidation-20261109/`.

## Under load (2026-11-14)

The maintainer's captures (Firefox 160, WebGPU, 2704×1528 at 120 Hz):
quiet, the hand costs about 1 ms of a frame's main thread and frames hold
120 a second with it on (46 late of 11,263; one frame of 23 ms in the
hand's lap); with other processes competing for the CPU the same stage
had 16–17% late frames with the hand on against 3.5–4% with it off. The
hand is an autoplay beside the play, never the play: under load it gives
way so the field, the judging and the sound stay smooth. Two parts:
its preparation keeps to a bound it cannot overrun, and a rule over a
play's late seconds sheds the hand step by step.

### What made the lap overrun

- **The share doubles when motion runs short.** `frame_budget` gives a
  quarter of the refresh (2 ms at 120 Hz), half that with 1.5 s of motion
  ready and twice it with less than 0.75 s. In dense stretches, and on a
  loaded machine (where a frame's wall-clock share buys less work, since
  the thread is preempted inside it), the motion runs short and the share
  is 4 ms a frame: the lap that "doubles in dense stretches" and the 2–4
  ms under load. The shares are as they were; what changed is that they
  are now kept.
- **Firefox's clock reads whole milliseconds.** The loop stopped when the
  clock said the share was spent; at 1 ms resolution it ran on to the next
  tick, up to a millisecond past the share (a test reproduces it: 1.004 ms
  past on a millisecond clock with the clock alone).
- **No checkpoint is long by itself.** Over Beatucada's Challenge at 120
  frames a second natively, 2.68 million checkpoints: the longest 0.18 ms
  (a guide step), none over 0.25 ms; the only one ever measured longer
  (5.5 ms, once) came on a loaded machine and did not come again: a
  preemption inside it, not work.
- **The 23 ms frame** (song second 72.98 of the capture
  `…hand-on-agents-niced.json`) was 8 ms of preparation and 15 ms of the
  hand's picture, in a second when the GPU was 37–43 ms behind
  (`gpu_done`) and frames around it started 2–6 ms late. The picture's
  part is the frame's first writes to the GPU (the hand's vertices, its
  texture and its copy under the field): where a backed-up WebGPU queue
  makes the main thread wait, not work of the hand's. The preparation's
  8 ms is a pause in the thread at that moment (a collection, the
  browser) past a 2 ms share on a millisecond clock.

### The bound now

`RuntimePreview::prepare` starts a checkpoint only where the clock leaves
room for it in the frame's share and, on a coarse clock (one never seen to
move by less than 0.25 ms: Firefox's millisecond; Chrome's 0.1 ms and the
native clock are fine), where the work counted does too: each stage's
checkpoint (`search`, `judge`, `guide`, `source`, `correct`, `publish`,
`arrows`, …) has an expected cost (`dance_runtime::Costs`, 5 µs at first),
learnt from what whole frames measured (each frame moves the costs of the
stages it ran by `1 + 0.05 · (measured − expected) / expected`, linear in
the measurement so that a coarse clock's readings average out over frames).
The first checkpoint of a frame always runs, so the hand never stops for
an estimate. (Counting on a fine clock too made the estimates, which
spread a frame's measurement over the stages it ran, stop frames short of
their share now and then; the motion then ran shorter and took the doubled
share more often: natively the 95th percentile of a frame's preparation
went from 1.2–1.4 to 2.0–2.1 ms. On a fine clock the clock alone keeps
it, a checkpoint past at most, 0.2 ms natively.) On a millisecond clock with 4 µs checkpoints, from the first half second
on no frame went past its share by 0.25 ms, and fewer than one in fifty by
0.1 ms (`the_bound_holds_on_a_millisecond_clock`, at the share and at half
of it, where the clock alone ran a millisecond past), and the hand kept
up over the whole dense chart. A hand that falls behind anyway **rests where it stood**: its last pose
stays shown and nothing more is prepared, where it used to fade out as a
failure. Falling behind is its motion running out, or a deadline passed
while the machine was busy: the planner's search over its wall-clock
limit (`ContactTimeout`), the motion's `runtime_stream_starved`,
`runtime_publication_deadline` or `runtime_unprepared_window`
(`dance::fell_behind`; the reason in the debug state's `rest_reason`). A
chart it cannot play still stops it as before. (Seen in Firefox with
twelve busy workers in the page: the hand went lighter at 5.9 s and,
before this, stopped 3 s later on a missed deadline.) Nothing it planned changes: the planning cursor, the
published contacts and their times, and the judged chart are as they were;
only how much work a frame may do.

### The rule (`hand-load-v0`, `ddi_session::hand_load`)

Watched while the hand plays beside a running play (on, supported, not
failed, from the song's start to its end), on the same measured seconds as
the automatic video setting's rule (`FpsMeter`, `ddi_platform::video::VideoGuard`):

| Value | What |
| --- | --- |
| `WARMUP_MS` 2000 | the first two seconds watched are not judged |
| `LATE_SHARE` 0.9 | a second is late when its frame rate is under 90% of the refresh (the fastest measured since the play started) |
| `LATE_SECONDS` 3 | late seconds in a row that take each step |
| `SETTLE_SECONDS` 1 | after a step, a second not judged |
| `HITCH_MS` 250 | a second holding a frame this long is a stall (a hidden tab): it and a new warm-up count for nothing |
| `LIGHTER_SHARE` 0.5 | the share a lighter hand prepares in |

1. **Lighter**: half its share of a frame to prepare in, and its picture
   drawn every other frame (the frame between keeps the last picture: the
   field's underlay is placed again without a copy). The preparation is
   most of the hand's main-thread time (0.8 of 1.0 ms in the quiet
   capture) and its picture most of its GPU time (0.65 ms a frame at
   2704×1528: 9.96 against 9.31 ms with the hand off); halving both sheds
   the most for the least seen: its motion stays smooth at 60 pictures a
   second on a 120 Hz display. Should its motion run out at half the
   share, it rests (above).
2. **Resting**: nothing prepared; its picture stands still where it was
   (no new picture is drawn: its texture is reused), shown.
3. **Gone**: neither prepared nor drawn for the rest of the play.

Each step after its own three late seconds, one way within a play (a new
play starts as always: `PlayView::start`); never on a single late second.
If frames are still late with the hand gone nothing more is shed here (a
movie has its own rule). What the player sees: the hand going still and
then away; the results say "The dancing hand was turned off for this song:
frames were being dropped (Options)." when it went (as a movie's "Video was
turned off for this song: …"), nothing for the lighter or resting steps.
No setting was added: the video rule's own setting (Auto) chooses whether
a movie may be turned off, and the hand has none of its kind; the hand's
own setting (on or off) stays what it was. The field, the judging, the
sound and the input are never touched, nor anything of a play's record or
replay (`hand_load_offscreen`: the loaded play's judgments are the quiet
one's).

### Testing it under load

- `load=MS` (web, any route, kept for the tab) and `--load-ms N` (desktop)
  burn that much main thread in each play frame (booked with the scene);
  the session and shell tests and the desktop's offscreen test use fake
  clocks, where a late frame is a number.
- `ddi_session::hand_load`'s tests: no step over a quiet song (and at 110
  of 120), the steps one way each after its own late seconds, a single
  late second or a stall takes none, only while the hand plays, fresh at
  a new play. `dance::preload_tests::the_hand_gives_way_and_falls_behind_into_rest`:
  lighter halves the share; resting prepares nothing and keeps its pose;
  gone; a hand with no time to prepare rests, shown, without an error.
- `crates/desktop/tests/hand_load_offscreen.rs`: Beatucada's Challenge
  autoplayed offscreen, quiet at 60 frames a second (no step over the
  whole song, a picture every frame) and loaded from its sixth second (a
  frame every 25 ms): lighter at 9.1 s (80 pictures in 160 frames, still
  preparing), resting at 13.1 s (nothing drawn or prepared), gone at
  17.1 s, the results' note, and the same judgments as the quiet play.
- In the browsers, `load=20` on a solo autoplay: lighter, resting and gone
  about 4 s apart (Chrome: song seconds 3.8, 7.9 and 11.9; Firefox: 4.4,
  8.5 and 12.7), the results' note in both.

### Measured (2026-11-14)

Beatucada's Challenge autoplayed with the hand on, solo, 1280×720 at
scale 1, a release build as deployed, the frame recorder on (`perf=1`):
the frame's main-thread total, the hand's lap (its preparation and its
picture) and its preparation alone, in ms, mean / p95 / p99 / most, over
song seconds 3–45; late frames are intervals over 1.5 refreshes (headless
Chrome refreshes at 60 Hz, Playwright's Firefox at 120). "Before" is the
build of `dev` before this round (`7c2655e1`), "after" this round's. The
machine was not quiet: other processes (another session's builds; the
maintainer's own Firefox) kept the load average at 8–47 throughout (said
beside each row), and every run of ours was at the lowest priority.
"Loaded" adds twelve busy Web Workers in the page for the window (the
page's own contention, on a ten-core machine).

| Run | Load avg | Total | Hand lap | Preparation | Late |
| --- | --- | --- | --- | --- | --- |
| Chrome before, quiet | 35 | 2.02 / 3.5 / 4.5 / 7.2 | 1.49 / 2.6 / 3.1 / 4.6 | 1.10 / 2.1 / 2.2 / 4.1 | 0 of 2,521 |
| Chrome after, quiet | 14 | 2.03 / 3.4 / 4.1 / 6.5 | 1.52 / 2.6 / 2.8 / 5.1 | 1.14 / 2.1 / 2.2 / 3.2 | 0 of 2,521 |
| Chrome before, 12 workers | 15 | 1.49 / 2.9 / 3.2 / 4.0 | 1.08 / 2.4 / 2.4 / 2.7 | 0.81 / 2.1 / 2.1 / 2.2 | 0 of 2,520 |
| Chrome after, 12 workers | 11 | 1.51 / 2.9 / 3.1 / 4.1 | 1.11 / 2.4 / 2.4 / 2.7 | 0.84 / 2.1 / 2.1 / 2.2 | 0 of 2,520 |
| Firefox before, quiet | 75 | 1.67 / 3 / 4 / 5 | 0.81 / 2 / 3 / 3 | 0.64 / 2 / 2 / 2 | 2 of 5,037 |
| Firefox after, quiet | 47 | 1.54 / 3 / 3 / 5 | 0.72 / 2 / 2 / 4 | 0.49 / 2 / 2 / 3 | 0 of 5,041 |
| Firefox before, 12 workers | 41 | 2.00 / 4 / 5 / 8 | 0.98 / 2 / 3 / 5 | 0.75 / 2 / 2 / 4 | 320 of 4,666 (6.9%) |
| Firefox after, 12 workers | 14 | 1.98 / 4 / 6 / 9 | 0.92 / 3 / 3 / 5 | 0.65 / 2 / 3 / 5 | 381 of 4,602 (8.3%) |

Within the noise of a machine this busy the two builds cost the same:
quiet, the hand stays at about 1 ms (Firefox's columns are whole
milliseconds but their means), and Firefox's quiet runs' 99th percentile
of the hand's lap went from 3 to 2 ms. The page's workers did not
reproduce the maintainer's 16–17% late frames (they made Firefox's 7–8%
late and Chrome's none); in the "after" Firefox run the hand went lighter
at 48 s, after the table's window. Over the whole song (100 s) quiet,
neither browser took a step.

With the load injected on the frame's own thread (`load=MS`):

| Run | Steps (song s) | Total | Hand lap | Late |
| --- | --- | --- | --- | --- |
| Chrome, `load=14` (60 Hz: 84% of the frame) | none | 15.40 / 16.8 / 17.1 / 17.7 | 1.03 / 2.4 / 2.4 / 2.6 | 0 of 3,551 |
| Firefox, `load=6` (120 Hz: 72%) | none | 7.08 / 8 / 9 / 12 | 0.34 / 1 / 2 / 3 | 19 of 6,985 |
| Chrome, `load=20`, full | lighter 3.8, resting 7.9, gone 11.9 | 21.04 / 22.6 / 22.8 / 22.8 | 0.73 / 2.3 / 2.6 / 2.6 | (every frame) |
| … lighter | | 21.22 / 22.0 / 22.6 / 22.8 | 0.86 / 1.5 / 2.2 / 2.5 | |
| … resting | | 20.53 / 21.0 / 21.4 / 21.7 | 0.01 / 0.1 / 0.3 / 1.1 | |
| … gone | | 20.46 / 20.9 / 21.0 / 21.4 | 0.00 / 0.1 / 0.1 / 0.2 | |
| Firefox, `load=20`, full | lighter 4.4, resting 8.5, gone 12.7 | 21.00 / 23 / 24 / 24 | 0.36 / 2 / 2 / 2 | (every frame) |
| … lighter | | 21.28 / 23 / 23 / 23 | 0.65 / 2 / 2 / 2 | |
| … resting | | 20.44 / 22 / 23 / 24 | 0.01 / 0 / 0 / 2 | |
| … gone | | 20.61 / 22 / 22 / 23 | 0.22 / 1 / 1 / 1 | |

(The `load=20` runs were recorded with the burn booked in the hand's lap,
before it moved to the scene's, and with the work counted on every clock:
their hand column here is the recorded lap less the 20 ms, at most the
hand's.) Resting and gone cost the frame nothing of the hand's;
lighter about half its preparation.

Natively (`dance_runtime::tests::measure_the_hand_paced_in_real_time`,
release, a frame every 1/120 s of real time, the song's time the real
time, 40 s; `DDI_HAND_THREADS` busy threads beside it): a frame's
preparation, ms, p50 / p95 / p99 / most, and frames over 2 and 4.17 ms.

| Run | Load avg | Preparation | Over 2 / 4.17 ms |
| --- | --- | --- | --- |
| before, quiet (three runs) | 32, 15, 10 | 0.26–0.98 / 1.20–1.41 / 2.02–2.72 / 6.6–30.8 | 51–114 / 8–13 |
| before, 12 threads | 11 | 0.26 / 1.15 / 1.82 / 10.2 | 33 / 3 |
| after, quiet | 8 | 0.98 / 1.32 / 2.08 / 8.0 | 67 / 5 |
| after, 12 threads | 8 | 0.97 / 1.22 / 2.05 / 8.6 | 62 / 5 |

The most a frame took natively is a preemption on a busy machine, as
before: no checkpoint is that long. Neither build starved or stopped.
