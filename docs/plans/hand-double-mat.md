# Double mat for the remaining hand layouts

Plan started 2026-11-06; No stretch and geometry/reach foundation implemented 2026-11-07.
Dance-double, Pump half-double/double and DanOni 9A gameplay are enabled as of 2026-11-08. Based on local
main `865cac6d`, including the accepted 3x3 hand work cherry-picked as `fd136299`.

The subsequent [2026-11-08 full library audit](dancing-hand.md#all-playable-hand-layouts-full-library-audit-2026-11-08)
is complete: 11,675 passes, 66 contact failures, 70 Dance Double pose failures,
55 pose CPU timeouts and 234 unsupported unplayable layouts. All prior passes
remain passes. No runtime/tolerance changes accompany the audit; double-layout
correctness repairs and remaining CPU caps are follow-up work.

The [targeted Dance Double pose repairs](dancing-hand.md#dance-double-pose-repairs-targeted-checks-2026-11-08)
cleared the 70 original pose rejections: 67 charts passed completely and three
reached later baseline contact rejections. The subsequent
[mine-to-head follow-up](dancing-hand.md#mine-to-head-and-staggered-clearance-2026-11-08)
now passes all 70 records, including those three full charts. There are no pose failures, timeouts or
settling warnings in that cohort. Model, contact timing and published tolerances
are unchanged. Native/category regressions, seven reference-dance parity checks
and 12 headless WASM Double studies pass. The full audit has not been rerun.

## Preparation status (2026-11-07)

The general **Options → Notes → No stretch** toggle and automatic hand-copy
simplification are implemented; see [the policy and verification contract](chart-simplification.md#no-stretch-2026-11-07).
The source regions are StepMania's dance/Pump masks, with ongoing-hold checking,
the Pump half-double subset and 9A's Oni allowed via either center. The two-pad
reach study should start from these allowed pairs instead of assuming that every
pair on the full 3x6 grid must be supported. These masks are not an anatomical
reach certificate. Pair filtering covers all four layouts. Dance-double and Pump
half-double/double now have rolling contact planning and judge replay, checked
through the existing pose runtime and 18-tile renderer. DanOni 9A now selects physical Oni centers and aggregates their logical contacts. The implementation and evidence are recorded below. Existing
single-mat motion and gameplay independence are preserved.

## DanOni 9A gameplay (2026-11-08)

9A now uses the existing 18-tile renderer and three-second, single-threaded
rolling contact/pose pipeline. No Stretch and At once Two remain automatic on
the hand's copy. Gameplay's judged notes, simplification options, timing,
model assets and numerical contact/anatomical bounds are unchanged.

- L4/L2/L8/L6/Oni/R4/R2/R8/R6 maps to
  **4/2/8/6/(5 or 14)/13/11/17/15**. `layout_panels` supplies canonical 5 for
  the input record; it is not the complete Oni target set. `MatLayout::targets`
  and `lane_for_tile` own physical alternatives and reverse logical identity.
- The beam considers at most two Oni center variants per parent. Each variant
  must pass the physical No Stretch refinement, not just a logical lane mask.
  The 64 surveyed ordered scored pairs span 4.72–67.28 mm nominally; the
  9A-only cheap candidate envelope is 4.7–68 mm. Narrow crossed assignments
  and seam-facing pairs were falsely rejected by dance-double's 6–66 mm
  envelope. This is not a pose/contact tolerance relaxation.
- Center selection prefers the arrow bank of up to eight received rows (within
  a hold/roll's end, or 0.75 s for taps/lifts); mismatches cost 24 for held
  notes and 4 otherwise. This prevents an avoidable opposite-bank opening
  hold when the following known arrows already identify its bank. Unknown
  notes are never consulted. Published physical contacts are immutable;
  holds and roll refreshes retain the selected center. The existing validated
  same-center handoff/re-grab policies remain; arbitrary between-center
  teleports or unvalidated mid-hold reassignment are not introduced.
- Replay and window checkpoints union 5/14 into one Oni button before
  generating edges. A landing on the other center while Oni is already down
  cannot score a re-hit; a lift requires both contacts to leave. Shared
  companion clearance follows its actual source center. Roll refreshes bind
  to the physical held center and original logical roll identity.
- Oni mines/shocks expand to both physical center constraints, while the
  source/judge still sees one hazard. Future Oni reservations cover both
  centers. Hazard-adjacent landings and lift/roll auxiliary validation resolve
  aliases to the original note. Native DanOni release budgets remain distinct
  from StepMania grace and are checked by the actual judge.
- All eight unscored corners are possible support destinations, ranked to at
  most two per assignment. Twelve corner-pair escape recipes are ranked to
  at most two per parent; airborne clearance remains available. Lifts return
  to a reachable unscored corner on their source pad. Opening empty windows
  use unscored 9/16; received opening arrows may choose their local upper
  corners (or seam corners for a split row) only before any physical prefix
  is certified. Canonical Oni=5 must not bias a right-bank opening to the left.

### Verification

- **668 affected native tests + 15 settings tests** pass. The 9A cases cover
  all admitted distinct-lane jumps and repeated jumps, mirrored bank choices,
  delayed/immediate openings, held Oni, held-arrow/Oni lifts, rolls, all-lane
  mines/shocks, rapid Oni repeats and native hold-budget rejection. Aggregator
  tests prove that overlapping center contacts cannot synthesize a re-hit or
  prematurely release Oni.
- **32 native and headless Chrome/WASM rolling motion cases** pass with
  bit-identical complete contact plans, 120 Hz samples and 8 kHz contact-edge
  probes. Both StepMania and native DanOni judges are covered. Exact touchdown/
  lift times, frozen C2 prefixes, 2 mm targeting, 0.25 mm planted height,
  1 mm tile margin, 40 mm knuckle height and current floor/joint bounds hold.
- The gameplay adapter passes with offsets, immediate mirrored starts and
  Oni holds/hazards. Actual off-screen GPU lifecycle coverage runs **20
  consecutive plays** across ten layouts on one desktop host, including
  9A twice. Startup preload, mat identity, drawing and non-starved playback
  remain checked through completions and early quits.
- The private **79-category gate** gives **73 passes and six expected contact
  rejections**, with zero mismatches, new pose failures or timeouts. It is a
  regression gate, not the full library audit.
- All **ten previous layouts' complete plans and 754 rolling publications**
  (including committed plans/certified horizons) remain bit-identical to
  `1f809923`. Strict planner/shared-shell clippy, full-game WASM compilation,
  formatting and diff checks pass.

Evidence is kept ignored in `target/scratch/danoni9a/`: `regressions.log`,
`settings.log`, `native-gpu.log`, `adapter.log`, `cohort-results/`, `parity.log`,
`native-plans.json`, `browser-results.json`, `app-wasm.log` and `clippy.log`.
Probe/baseline crates and captures stay uncommitted. No full library audit is
part of this change; targeted success is not an all-chart reach guarantee.

## Pump half-double and double gameplay (2026-11-08)

Both Pump double layouts now use the existing 18-tile renderer, settings gate,
preload/restart path and single-threaded 3-second rolling contact/pose pipeline.
No Stretch and At once Two remain automatic on the hand's own chart copy;
gameplay's judged chart and general Options preferences remain independent.
No note times, accepted model assets, joint bounds or pose/contact tolerances change.

### Mapping, reach and bounded choices

- Half-double's six lanes map to **5/9/3/10/16/14**; full double's ten lanes map
  to **1/7/5/9/3/10/16/14/18/12** from `MatLayout::panels`. Both centers
  are scored; neither is treated as a safe resting/escape tile.
- These layouts share the frozen 18-tile shape/distance tables. Their static
  survey admitted all **36 / 68 ordered scored pairs** at 24 mm pitch. The
  largest nominal anchor separation is 90 mm for L5/R5 (72 mm between physical
  tile centers). Pump therefore uses a 6–90 mm candidate envelope; dance-double
  retains 6–66 mm, with legacy single envelopes unchanged. Every scored pair
  must still pass the original No Stretch masks. The envelope is a cheap
  candidate filter, not a new anatomical guarantee or relaxed numerical guard.
- Half-double has twelve unscored support tiles; full double has eight. Support
  transfers rank reachable candidates and offer at most two per assignment.
  The side escape recipes are 4/6, 6/4, 6/13, 13/6, 13/15 and 15/13, ranked
  from current physical contacts to at most two per parent. The existing head
  clearance and airborne alternatives remain; the hazard branch stays bounded.
- Empty opening windows start at unscored 6/13. Before any prefix is certified,
  received first heads can choose unscored sides on their pad, or split support
  for an opening pair across pads. A leading hazard does not substitute a scored
  center rest. Positive published boundaries are never reseated. This fixes the
  empty-window judge checkpoint mismatch exposed by delayed first notes.
- Lifts/shared-center repeats return to local unscored sides. A lift under a
  hold checks which side remains reachable: returning a free R5 finger toward
  R6 while L5 is held would create an unnecessary wide span; it can use R4
  instead. The held contact and exact scored release/re-hit times are preserved.
  Replay checks the causal lift/landing and permits only those local unscored
  side returns. Hold grace, release/re-grab, handoffs, rolls and adaptive timing
  remain the existing policies.
- Version-2 plan geometry, pad-specific hazard checks, CPU/GPU presentation,
  camera framing and paused source invalidation reuse the dance-double path.
  Neither finger is assigned permanently to one pad. At this Pump checkpoint, DanOni 9A was still gated; its extension is recorded above.

### Verification

- **660 native tests + 15 settings tests pass**, including all existing mat,
  engine, character, renderer, planner, pose, session and shared-shell tests.
- New authored Pump contacts cover all lanes, repeated center jumps, holds,
  rolls, lifts under holds, mines/shocks, immediate/delayed openings and mirrors.
  All **44 distinct allowed jump pairs** (15 half-double + 29 full double)
  can be hit and re-hit without simplification. The reach/escape invariants check
  every 36/68 ordered pair and unscored-support identity against the shared map.
- The common double-motion test checker now serves both dance and Pump tests.
  **32 Pump motion cases** pass native and real headless WASM at 120 Hz plus
  8 kHz edge probes: exact hit/lift times, frozen published poses/C2 transitions,
  2 mm targeting, 0.25 mm planted height, 1 mm tile margin, 40 mm knuckles and
  existing control bounds. Complete generated contact plans match native/WASM.
- The actual desktop off-screen GPU test passes **18 consecutive plays** across
  nine tested layouts, including both Pump doubles, quit and layout changes.
  Runtime tests also check actual GLB-deformed fingertip lights at played panels,
  including Mirror/Shuffle and offsets. Saved Off preferences remain Off.
- Eight headless **WebGPU/WebGL2 gameplay cases** cover each Pump layout and its
  mirror through center holds, releases and full hazard passages, with ready
  opening images, both pads lighting and restart. No console errors, guide
  lateness or starvation occur. Final held-lift logic is also covered by the
  32-case native/WASM numerical gate. Captures are actual gameplay renders.
- The established **79-category gate matches**: 73 passes, six expected contact
  rejections, zero mismatches/new pose failures/timeouts. No full audit is run.
- Built-in Beatucada **Pump half-double Hard (8)** and **Pump double Easy (9)**
  both pass the complete native adapter, each over 6,662 sampled frames.
  Maximum reported target errors are 0.21324 / 0.61931 mm; planted heights are
  0.01964 / 0.00813 mm, inside the unchanged bounds. Frozen-clock checks are
  numerical validation, not realtime CPU-budget certification.
- Eight prior layouts' whole contact plans and **584 rolling publications**,
  including committed plans/certified horizons, are bit-identical to `c627ef6d`.
  Strict planner/shared-shell clippy, full-app WASM compilation, formatting and
  diff checks pass; the separate release browser bundle passes its boot check.

Ignored evidence/tools are under `target/scratch/pump-double/`: `regressions.log`,
`pump-tests.log`, `gameplay-tests.log`, `native-gpu.log`, `parity.log`,
`cohort-results/`, `beatucada-summary.json`, `browser-checks.json`, `browser.log`,
`browser-results.json`, `wasm-browser.log`, `app-wasm.log`, `clippy.log` and PNGs.
Private first-party packs, probe/baseline crates and captures stay uncommitted.
The 9A extension is recorded above. The subsequent [2026-11-08 full audit](dancing-hand.md#all-playable-hand-layouts-full-library-audit-2026-11-08)
records broader coverage and remaining failures; this support checkpoint does
not claim every Pump chart will prepare successfully.

## Dance-double gameplay (2026-11-08)

Stage 4 and the dance-double portion of stage 6 are implemented. Both default
builds now enable the hand for dance-double when its saved setting is On. The
hand still uses its own No Stretch + At once Two chart copy, the same 3-second
single-threaded planner and bounded f64 pose preparation. Ordinary gameplay,
scoring and the player's general simplification preferences remain independent.
The model, wrist cap, skin corrections, note times and numerical guards are
unchanged. At this dance-double checkpoint, Pump doubles and 9A were still unsupported;
the Pump extension above now enables half-double and double.

### Presentation and lifecycle

`ddi-character` now consumes `ddi-dance-mat` for render-slot centers and physical
identity. Slots keep the accepted row-major visual ordering within each pad;
their physical IDs reflect exported X relative to the solver. The first/single
pad's color slots and f32 center recipe stay unchanged. A full 18-slot schema is
shared by the tile mesh, press travel, contact glows, strike colors and light
spill. Inactive slots are zero; the shader traverses only active tiles. The same
versioned geometry controls camera ground anchoring, paused redraw identity and
the first-pad UP framing helper. Options shows the active mat with a standing
reference. Visual travel still stops at the original zero contact plane.

The character WGSL uniform supports 18 emitters and packed press/glow values,
with no storage/compute buffers. Native shader validation, uniform layout checks
and GLSL ES 3.00/WebGL2 conversion cover both vertex/fragment entry points.
Single-mat appearances retain their selected materials, lighting and backing.

Preloading binds the same effective Player/geometry as play and submits a first
GPU frame before audio begins. Cache identity now includes chart notes and
physical geometry, as well as timing/rules/options: edited or different charts
cannot accidentally reuse one another's motion. A changed procedural source or
geometry invalidates a paused draw and contact-light history. Device replacement
continues rebuilding device-owned resources; restart warms a fresh opening buffer.

Immediate mirrored starts exposed the planner's old left-pad-only opening rest:
with a first right-pad head at local 0.5 s it could reject the first prepared
curve (`position` near 0.3803 s). For dance-double, the first received head may
now choose its pad's unscored center **before any prefix is published**. This is
not a future-chart lookahead or a retimed hit. A positive certified boundary or
existing track prevents reseating; delayed first notes continue from the frozen
rest. Tests check both cases, including right-pad starts, offsets, Mirror and
Shuffle, without relaxing timing, contact bounds or the taller stance.

### Verification and limits

- **656 native tests passed** across mat, engine, character, renderer, planner,
  pose, session and shared shell; **15 settings tests** also pass.
- The actual desktop off-screen GPU lifecycle test passes **14 consecutive
  plays**, including double, six single layouts, a double quit and
  subsequent layout changes. It checks opening availability, geometry/slot
  counts and uninterrupted prepared output. Runtime tests additionally verify
  the actual GLB-deformed fingertips light the physical panels of scored hits.
- The established **79 category expectations match**: 73 pass and six expected
  contact rejections; no new contact/pose failures or timeouts.
- Headless **WebGPU and WebGL2** play a private first-party crossing/hold chart
  and its mirror, with ready opening CPU/GPU frames, both pads lighting,
  restart, full scored passage and return to a single-mat chart. No console
  errors, guide lateness or buffer starvation occur. Observed targeting and
  planted-height errors remain inside the existing 2 mm / 0.25 mm bounds.
- The built-in **Beatucada dance-double Beginner** (source chart 2) passes the
  complete native gameplay adapter: 6,426 sampled frames, maximum reported target
  error 0.16924 mm and planted height 0.01491 mm. This focused run drains work
  at fixed playback timestamps; it is not a realtime performance certification.
- Strict character/renderer/planner/shared-shell clippy, full release browser build, formatting and
  diff checks pass. Browser captures show the actual gameplay compositor;
  no visible browser or desktop window is opened.

Ignored evidence is in `target/scratch/double-gameplay/`: `final-regressions.log`,
`settings.log`, `native-lifecycle.log`, `skin-contact.log`, `cohort-results/`,
`clippy.log`, `trunk.log`, `browser-checks.json`, `beatucada-double.json` and
WebGPU/WebGL2 PNGs. The zip,
source chart and browser driver are private first-party verification artifacts;
no real library files or development tools are added to the runtime extraction.
The browser harness reloads between distinct routes to avoid reading a previous
song's still-present global debug snapshot; the desktop test exercises persistent
host transitions. This is focused integration verification, **not a full library
audit or a guarantee for every double chart**. Next expand reviewed coverage and
then move to the remaining double layouts.

## Rolling dance-double contacts (2026-11-07)

At this planner checkpoint, stage 3 was implemented for **dance-double only**, on the accepted 18-tile,
24 mm pitch / zero extra seam geometry. This was a planner/pose checkpoint, before
the production renderer/host update above. At that point the nine-tile renderer
still rejected double geometry. Other double layouts continue to stay gated.

### Search and publication contract

- `layout_panels` resolves the eight scored targets from `MatLayout::panels`:
  4/2/8/6/13/11/17/15. Every layout's mapping shares that one table; for 9A it
  gives Oni's left center, and `MatLayout::fixed_panels` deliberately returns no
  flat physical mapping because its Oni still requires center selection.
- The existing rolling planner uses its three-second window, bounded 256-parent
  beam and cooperative single-threaded checkpoints. Incoming rows use the same
  At once Two + No Stretch preparation, including ongoing holds. Neither finger
  is assigned permanently to one pad. No chart times, model bounds, precision or
  contact tolerances change.
- Separate frozen 18-tile shape/distance tables match explicit `Plan::anchor`,
  including its within-tile clamp. Native/WASM search avoids runtime `atan2`
  ordering differences; legacy nine-tile tables/arithmetic are retained.
- Scored stance pairs must pass the physical No Stretch regions. A conservative
  nominal-anchor span gate of 6–66 mm covers all 42 policy-admitted ordered pairs
  from the static survey (largest span 65.521 mm, smallest shared edge stance
  6.78 mm). This is a candidate filter, not a new anatomical reach certificate.
  Unscored rests use the same envelope and still face unchanged pose validation.
- When wide transfers need a companion step, rank reachable unscored tiles by
  travel and stable ID, offering at most two per assignment. Candidate order and
  beam retention remain deterministic; there is no enumeration of 18² stances
  per parent. Existing layouts retain their allocation-free support list.
- Mine crossing tests use physical positions on both pads. The two bounded
  escape alternatives are shared left/right centers, alongside the existing
  head-clearance and airborne alternatives. Lift returns use the source pad's
  unscored center. Hold grace, re-grab, handoff, roll and adaptive timing policies
  remain the existing policies, replayed against the effective eight-lane chart.
- Generated `ContactPlan`s use version 2 and explicit `mat` metadata. Rolling
  candidates, committed/pruned plans and the pose loader retain that identity;
  replay rejects absent/mismatched double geometry. Single-mat serialization
  omits the new optional field and retains version 1.

### Verification

The permanent double fixtures cover steps across the seam, wide jumps, repeated
jump contacts, long split support, rapid held transfers, rolls, lifts, mines and
shocks, in both horizontal mirrors. Incoming raw three-note/stretch rows verify
that the rolling copy matches whole-chart simplification. Every allowed distinct
No Stretch jump pair can be hit and re-hit. A focused candidate test separately
checks a cross-finger hold handoff; flexible facing can also solve the complete
rapid-transfer fixture with a crossed assignment, so it does not force that
maneuver when a cheaper valid solution exists.

Automatic rolling pose checks use 120 Hz samples plus 8 kHz probes around contact
edges. They check exact tap/lift times, 2 mm target error, 0.25 mm planted height,
1 mm inside-tile margin, 40 mm knuckle clearance, anatomical bounds and finite
velocities/accelerations. Already-published poses remain bit-identical and their
velocity/acceleration remain continuous across subsequent windows. Preparation
uses a constant display-duration endpoint, as gameplay does; future contacts
still enter only through the received three-second window.

- Native mat/engine/planner/pose/session/shell tests: **479 passed**, including
  the existing 12 manual crossing/mirror studies and new automatic studies.
- Existing 79 category/layout/ruleset expectations: **73 pass, six expected
  contact rejections**, no mismatches, pose failures or timeouts. This is the
  focused regression cohort, not a full library audit.
- Seven existing-layout whole plans and **499 rolling publications** are
  bit-identical to `0ac774f6`, including committed plans and certified horizons.
- All **12 automatic/mirrored studies pass headless Chrome/WASM** pose checks;
  their complete whole contact plans equal native output, including assignments,
  lift/landing times, auxiliary contacts and hold policies. Rolling candidate and
  committed publication/horizon traces also match (digest `9880881f96645e3e`).
- Affected-crate clippy (all targets, warnings denied) and full application
  `wasm32-unknown-unknown` compilation pass.

Ignored evidence/tools are under `target/scratch/double-contact-planning/`:
`regressions.log`, `cohort-results/{summary,results}.json`, `legacy-parity.log`,
`native-plans.json`, `browser-results.json`, `clippy.log`, `app-wasm.log` and the
private baseline/probe crates. No library charts, captures or development assets
are added. Frozen-time checks drain cooperative work before sampling; they are
not a realtime CPU-budget certification or a full library audit. Geometry,
rendering and visual review remain separate integration steps.

## Geometry and reach foundation (2026-11-07)

Rebased `codex/hand-audit-recovery` onto local main `0a9530ef` after the
maintainer's No Stretch cherry-pick; no fetch, pull, push or main mutation.
The earlier tip is retained as `codex/hand-before-mat-geometry-20261107`.
Before committing, local main advanced with the documentation-only localization
plan `00f1e2f2`; this change is rebased onto that tip as well. The parity baseline
remains `0a9530ef` (identical runtime code).

`ddi-dance-mat` now owns validated physical `Tile` identities, versioned
`Geometry`, all eleven built-in lane target sets, reverse lookup, unscored
candidates and the unchanged logical No Stretch masks. Engine filtering and the
seven supported hand mappings use this source. The single-mat nominal shape and
distance tables remain untouched. `MatLayout::pair_allowed` refines the 9A
logical mask with its actual center: the left two regions use L5, the right two
R5. Both centers map back to the same logical Oni lane. This API does not yet
implement Oni edge aggregation, hazard expansion or physical center selection.

Offline `hand-pose::Plan` version 2 requires an explicit `mat` object and a known
layout with the same pad count and pitch. Mat schema version 1 carries `pads`,
`pitch_mm`, `seam_gap_mm`. Version-1/missing-version legacy plans retain their
single-mat behavior; doubled IDs, a named double layout without the new geometry,
unknown versions and mismatched layouts/pitches are rejected. Centered contact
reuse compares geometry identity. The runtime rejects a geometry change rather
than splice it into previously prepared output. Study contact centering, anchor
positions, mine checks, tile margins and stream arrow checks use the explicit
geometry. Physical millimeters still precede the solver's -16 mm root offset;
render x is reflected relative to solver x. These are distinct coordinate spaces.

**Original foundation checkpoint:** this did not complete stage 1 or enable live
double support. Planner tables/candidates, incoming-note mapping, logical replay,
host cache keys and eighteen-tile rendering were still gated work. The rolling
implementation above now covers the dance-double planner/replay portion. Direct `Rolling` callers must rebuild for geometry changes; the
production `DanceRuntime` guard enforces this at its boundary. Existing live
layouts continue using legacy single geometry and unchanged render assets.

### Bounded static survey

Measured all 324 ordered physical pairs at each of 24, 22 and 20 mm pitch,
zero extra seam gap. The accepted model, joint bounds and corrective skin were
unchanged. Each pair used its centered contact spots, including separated spots
for a shared tile. The bounded search tried the ordinary style seed plus six
ranked styles in each of four yaw families (-135/-45/45/135 degrees), each with
48 ordinary fit iterations, three 8-iteration refinements, and the existing
contact correction. Balance targeted the support midpoint. This is offline study
work, not the proposed runtime cost or a new preparation policy.

A fit must meet 2 mm targeting, 0.25 mm planted height, 1 mm tile margin,
40 mm knuckle clearance and all existing model bounds. An amber/near-bound
screen flags a control within 2% of its range, knuckle height below 42 mm,
margin below 2 mm, error above 1.5 mm or height above 0.2 mm. In practice many
ordinary straighter stances approach a lower digit-control bound; amber alone is
not a rejected pose. Neither a fit nor a failed bounded search proves anatomical
feasibility/impossibility, dynamic stability or absence of skin self-intersection.

| Pitch | All ordered pairs fit | Near-bound subset | No fit found |
|---|---:|---:|---:|
| 24 mm | 298 / 324 | 225 | 26 |
| 22 mm | 306 / 324 | 181 | 18 |
| 20 mm | 318 / 324 | 189 | 6 |

More importantly, the physical pairs admitted by each layout's No Stretch policy
(including shared-tile states, both finger assignments and both 9A centers):

| Layout | Allowed ordered pairs | Fits at 24 mm | Fits at 22 mm | Fits at 20 mm |
|---|---:|---:|---:|---:|
| dance-double | 42 | 42 | 42 | 42 |
| Pump half-double | 36 | 36 | 36 | 34 |
| Pump double | 68 | 68 | 68 | 66 |
| DanOni 9A | 64 | 64 | 64 | 62 |

At 20 mm, a shared center's existing ±9 mm contact separation leaves only
0.583 mm nominal edge margin (tile half-width 9.583 mm), below the unchanged
1 mm requirement. Smaller pitch is therefore not an unconditional improvement.
At 24 mm every policy-admitted static pair fit. For contrast, L4/R6 and L1/R9
are rejected by No Stretch and remained unresolved in the bounded pose search,
with about 7.99/12.91 mm worst tip error respectively. Keep **24 mm / gap 0** as
the study baseline; final adoption awaits broader motion and visual review.

### Minimal motion study and checks

Two hand-authored six-second contact tracks passed the existing cooperative,
three-second, single-threaded runtime: a left-to-right seam crossing (including
shared R4), and a held L6 contact while the other finger visits the right pad.
No chart/contact planner or automatic lane assignment was used. The runtime
validated its prepared contact curves and retained its immutable published
prefixes; independent 120 Hz samples showed:

| Study | Worst target error | Worst planted height | Lowest knuckle |
|---|---:|---:|---:|
| Crossing | 0.01285 mm | 0.00922 mm | 42.8206 mm |
| Held seam | 0.01392 mm | 0.00619 mm | 42.4998 mm |

An additional slow Pump wide-jump track (L9/L3 → L5/R5 → R7/R3,
0.9-second flights) was **rejected**, before its first liftoff: the prepared
curve hit `floor_penetration` at local 0.5483996342 s, with first release at
0.7 s. It produced no published frames. Static endpoint reach therefore does
not certify the interpolated motion. The reduced `wide-jump-plan.json` and log
are retained beside the passing studies; no contact or floor guard was relaxed.
The follow-up below resolves this original approach/clearance failure. Broader
mirrored cases and a checked turning study still precede stage-2 completion or
double gameplay.

These slow tracks alone did not certify rapid seam jumps, turns, all center
choices, holds/rolls/lifts against the judge, hazards or native/WASM motion equality.
The later motion repairs below and automatic planning checks above provide the
next evidence. Render/host integration and broader choreography review remain. Retain the
accepted single-mat tables and fail safely on unsupported cases.

Verification: 340 native tests passed (engine, hand-plan, hand-pose and the new
mat crate), strict clippy passed, and the full game compiled for wasm32. A private
comparison against `0a9530ef` sampled 847 poses/contacts over seven single layouts;
all f64 outputs were bit-identical. Regression fixtures now check version/ID
rejection, logical/physical mask equivalence, center alternatives, pad-specific
mines, geometry-change refusal, seam/held motion and exact retained prefixes.
No full library audit or live double-layout browser playback was run.

First-party private measurements and off-screen previews remain uncommitted at
`target/scratch/double-mat-foundation/`: `reach.json`, `reach-map.svg/png`,
`crossing-poses.png`, the passing and rejected `*-plan.json`/`*-frames.json` studies, baseline parity
harness and native/clippy/wasm logs. The PNG pose sheet uses the actual accepted
GLB deformation with plain study lighting/materials, not a replacement game look.
The raw study tools and baseline source copy stay outside the runtime extraction.

## Wide-jump startup repair (2026-11-07)

The original six-second Pump study now passes without changing the model,
24 mm geometry, release/landing times, contact spots, anatomical bounds,
2 mm / 0.25 mm / 1 mm tolerances or the preparation-time floor reserve.
This corrects the original failure recorded above; it does not complete stage 2.

### Cause and bounded fix

`DanceRuntime::new` always seeded its rolling guide from `Plan::empty`, the
legacy neutral center of one mat. The explicit double plan actually began at
L9/L3. The first guide publication therefore blended an unrelated center pose
into the authored opening contacts. Per-sample contact fitting switched between
very different solutions: source yaw changed about 52.63° between 0.541667 and
0.550000 s, while both fingers remained planted until 0.7 s. Interpolating those
poses produced approximately 1 mm floor penetration. More local correction knots
could not resolve that abrupt switch within the existing six-pass/1 ms limits.
The 0.5483996342 s rejection was a consequence, not an endpoint reach failure.

Explicit-geometry plans now give `Rolling` a constant opening seed plan in the
same geometry, using the already assigned initial contact spots (including
separation for a shared tile). This copies only fixed geometry, placement,
layout and initial contacts: no future tracks, events, mines, rolls or phase
edges. Preparation still receives those through the same three-second window.
The initial bounded solve and all subsequent solve/correction/validation policies
are unchanged. No draw-time solve or fallback is added. Legacy plans still use
exactly the old neutral startup path.

### Off-screen checks

- **462 native tests pass** across dance-mat, engine, hand-plan, hand-pose,
  session and shell. The authored crossing fixture now includes the wide jump
  and explicit touchdown events. It checks exact liftoff/landing classification,
  tile margins, anatomical/knuckle/floor/contact bounds, finite derivatives,
  C2 joins and retained publication prefixes. Independent probes cover the first
  2.5 s at 1 kHz and the old failure/phase edges at 8 kHz. New unit cases check
  opening geometry/shared spots, omission of future data and legacy startup.
- **All 79 existing category/mirror/layout expectations hold** through a private
  headless driver that includes the current production `RuntimePreview` source:
  73 complete passes, six expected contact rejections, zero pose failures,
  timeouts, starved frames or late guide jobs. Expected rejections remain the
  9B native held-mine conflict, three seven-key native hold/hazard cases and the
  mirrored pair of below-floor transfers. Exact sampled error/height maxima are
  0.08289 mm / 0.01626 mm. This motion gate drives cooperative checkpoints with
  playback time fixed, not a per-frame performance certification or a rerun of
  the historical library audit's 5/10 s CPU caps.
- **Seven complete single-mat reference plans are bit-identical** to the pinned
  pre-fix solver `efc7ef83`: Beatucada Medium/Challenge, five-key, Solo, Pump,
  seven-key and Techno. All 17,927 60 Hz poses, velocities and accelerations match
  under the same rolling preparation schedule.
- Strict hand-pose clippy and full-game wasm32 compilation pass. All checks are
  off-screen; no full library audit or double-layout browser playback is implied.

The three original six-second studies now all complete. Their independent 120 Hz
samples show these maxima/minima (conservatively rounded):

| Study | Worst target error | Worst planted height | Lowest knuckle |
|---|---:|---:|---:|
| Crossing | 0.000017 mm | 0.000008 mm | 42.7108 mm |
| Wide jump | 0.000013 mm | 0.000003 mm | 42.7717 mm |
| Held seam | 0.000014 mm | 0.000009 mm | 42.4998 mm |

### Remaining double-study limits

A broader matrix runs each track unchanged, mirrored horizontally (including pad
identity), with finger assignments swapped, and both together. This improved
from **6/12 complete passes to 9/12**, with no previously passing case rejected.
The input plans and exact event/track times match before and after.

| Variant | Crossing before → after | Wide jump before → after | Held seam before → after |
|---|---|---|---|
| Original | pass → pass | floor → pass | pass → pass |
| Mirror | knuckle → pass | knuckle → position | floor → floor |
| Swap fingers | floor → pass | pass → pass | floor → floor |
| Mirror + swap | pass → pass | pass → pass | pass → pass |

The remaining mirrored wide jump fails `position` at 1.2968310217 s during flight;
it previously failed `knuckle_height` at 0.5070632017 s before publication. The
mirrored held track still fails `floor_penetration` at 1.3218310217 s, and the
swapped held track still fails that guard at 3.6088820302 s during a later
snapshot. These are retained rejected cases, not certified dances. There are no
study timeouts. The periodic-yaw follow-up below fixes all three rejections.
Shared-tile turn quality and a deliberate turning track remain review items;
double-layout gameplay remains disabled.

Private, uncommitted evidence: `target/scratch/wide-jump-clearance/` contains the
pre-fix source pin, diagnostic trace, identical before/after authored plans,
12-variant frame/log pairs, 79-case reports, full runtime-parity output and
native/clippy/wasm logs. Research tools and captures are not part of the runtime
commit; no private input or plan is added to the repository.

## Mirrored/held repair: periodic world yaw (2026-11-07)

The three remaining numerical rejections above are fixed. The unchanged twelve
six-second input plans now all complete in native code and in headless
Chrome/WASM. This keeps the accepted model/assets, 24 mm geometry, every
release/landing/event time, contact spot, anatomical bound and contact tolerance.
At that motion-repair checkpoint, double-layout gameplay remained gated; the
2026-11-08 integration above now enables dance-double.

### Cause

All three failed guides cross the world-yaw coordinate boundary. In the mirrored
wide jump, consecutive raw guide fits near 1.30/1.3167 s have headings about
-160°/+180°; the swapped held track similarly switches from about -137° to
+180° near 3.61 s. The mirrored held track changes from +179° to -156° near
1.33 s. These are nearby physical headings, but arithmetic averaging/interpolation
treats them as widely separated numbers. Gaussian smoothing therefore imposed an
unwanted turn through zero, changing the support/refit solution. Interpolating
those poses caused the reported floor/knuckle/position failures. Repeated local
contact refinements could repair individual points but not the wrong angular
path. Continuation-only seeding was tested and discarded: it moved the failures
and rejected some previously passing tracks.

### Change and boundary

- Explicit-geometry plans use periodic **world yaw only** when the calibrated
  root range contains the entire -180°…+180° revolution. All finger joints and
  narrower root ranges retain their original bounded linear treatment.
- Exact and stream guide smoothing compare equivalent angles near the current
  sample. Prepared quintics unwrap adjacent headings into a continuous internal
  phase before deriving velocity/acceleration. Publication aligns the new
  curve's equivalent winding at the blend midpoint and retains the committed
  prefix. Exact/stream blends use the same angular convention.
- Model/renderer poses normalize yaw back inside ±180°. Angular velocity and
  acceleration are derivatives of the continuous phase and are **not** wrapped.
  The numeric angle coordinate can wrap while the actual rotation remains C2.
  Internal phase is not a new model bound or another articulated joint.
- Existing style selection, solver iteration/checkpoint budgets, balance rules,
  target positions/times and final contact/floor certification are unchanged.
  Legacy plans retain the original arithmetic path. No runtime sampling solve,
  worker or full-chart preparation is added.

### Verification

| Study | Original | Mirror | Swap fingers | Mirror + swap |
|---|---|---|---|---|
| Crossing | pass | pass | pass | pass |
| Wide jump | pass | pass | pass | pass |
| Held seam | pass | pass | pass | pass |

All twelve before/after JSON input plans match, including their exact events and
tracks. Native and browser runs each check 8,652 displayed samples, model bounds,
floor/knuckle clearance, exact touchdown edges and C2 joins. Across the matrix,
worst sampled error is below 0.09361 mm, planted height below 0.00864 mm and the
lowest knuckle above 42.2239 mm. These are checked against the unchanged
2 mm / 0.25 mm / 1 mm and 40 mm limits.

**467 native tests pass** across dance-mat, engine, hand-plan, hand-pose, session
and shell. The permanent authored regression now covers all twelve variants,
all six seconds at 1 kHz plus 8 kHz around the former failures and phase edges,
retained exact prefixes, angular derivatives, tile contacts and model bounds.
New focused cases cover short arcs through ±180°, equivalent-winding C2
publication, derivative units across a wrap, periodic stream smoothing and the
full-turn model-range gate. All previously retained bounds are still asserted.

The existing **79 category/layout/mirror expectations** remain 73 passes and six
expected contact rejections, with zero pose failures, timeouts, starved frames or
late guide jobs. Seven complete accepted single-mat reference plans retain
bit-identical poses, velocities and accelerations at all **17,927 samples**
against `9b3aa82d`. Strict clippy and full-game wasm32 compilation pass.
The browser probe runs the actual Rust/WASM pose solver in headless Chrome;
it does not enable double-layout gameplay or certify per-frame performance.
No full library audit was run.

### Movement-quality follow-up

The fixed cases pass numerical motion/contact validation. Shared-tile approaches
can still demand rapid reorientation with the current within-tile contact slots;
for example the mirrored crossing contains a roughly 61.35° physical yaw change
between adjacent 120 Hz samples near 2.35 s (the prior path already had rapid
turning elsewhere). Review that approach and add a deliberate turning study
before calling stage 2 complete. The approach-aware shared-slot follow-up below resolves this entry issue for
the measured crossing variants. Broader facing choreography remains later work;
no timing or tolerance was altered to conceal it.

Private evidence stays uncommitted at `target/scratch/mirrored-motion-repair/`:
`SUMMARY.json`, the `9b3aa82d` source pin, original diagnostics, discarded
continuation experiments, `aligned-*` native plans/frames, `browser-results.json`,
79-case reports, full-motion parity output and native/clippy/wasm logs. Temporary
instrumentation was removed. Research harnesses, private inputs and build
artifacts are excluded from the runtime commit.

## Shared-tile turn smoothing (2026-11-07)

The rapid mirrored/swapped shared-tile entry is now smoothed through contact
placement, retaining the accepted model, event times, tiles, finger assignments,
contact tolerances, taller-stance bounds and periodic world-yaw handling. The
contact spots inside a shared tile can change for a new explicit-geometry plan;
a spot already published or planted is never moved to obtain a smoother turn.
Legacy single-mat placement and motion remain unchanged.

### Cause and placement rule

Shared slots previously always put the index finger on the negative-X side and
the middle finger on the positive-X side. In the mirrored crossing this forced
the incoming index finger to pass across the planted middle finger before
landing beside it. The fitted hand then turned rapidly to reverse their order.
Filtering yaw alone would fight the fixed fingertip targets.

For explicit geometry, choose the shared separation axis from the fingers'
positions before their first common landing. The incoming finger stays on its
approach side. Both slots remain the existing distance apart (11 mm on arrows,
18 mm at center), inside the same tile; isolated contacts still target centers.
A simultaneous landing uses both incoming positions. An initially shared tile
with no approach history keeps the existing reference axis.

Overlapping same-tile contact intervals form a connected run. That run keeps
one axis through alternating re-hits, so a held finger cannot rotate its spot
when the other lifts and returns. A later same-tile jump carries the preceding
spot order even when both tile centers coincide. Across rolling revisions,
already published initial contacts and flight endpoints take priority over a
new approach; pruning old flights inherits their axis from those frozen spots.
The existing prefix-reuse policy is unchanged and is shared by the placement
and endpoint-copy code.

Implementation uses an ordered overlap sweep, compact union/find and frozen
endpoint lookups, with linear storage. It fits no poses, changes no note data and
runs during contact preparation. No worker or drawing-time solve is introduced.
The placement rule applies to explicit-geometry studies; ordinary gameplay
continues on its accepted legacy path. Solver limits and the 2 mm position,
0.25 mm planted-height and 1 mm tile-interior guards are unchanged.

### Results and checks

The measured shared approach occupies 2.0–2.6 s of the six-second crossing.
Largest physical yaw change between adjacent 120 Hz samples in that interval:

| Variant | Before | After |
|---|---:|---:|
| Original | 0.184° | 0.184° |
| Mirror | 61.352° | 2.210° |
| Swap fingers | 14.399° | 0.352° |
| Mirror + swap | 0.968° | 0.968° |

The mirrored peak interval-average speed falls from about 7,362°/s to 265°/s;
the swapped peak falls from 1,728°/s to 42.2°/s. Sampled analytic shared-approach
speeds are 22.01/264.70/41.72/115.78°/s respectively. Permanent dense checks keep
this authored approach below 360°/s and the entire crossing below 900°/s, so a
regression cannot merely relocate the snap. These are study regression guards,
not new universal runtime speed limits or relaxed contact tolerances.

**All twelve crossing/wide-jump/held variants complete** in native code and
headless Chrome/WASM, including exact touchdown-edge, tile, model-bound, floor,
knuckle and C2 checks. Before/after JSON event/track inputs match. The changed
within-tile spots are intentional. Maximum sampled targeting error across the
matrix is below 0.16683 mm, planted height below 0.01557 mm and the lowest
knuckle above 42.1709 mm. All remain inside the existing limits.

**470 native tests pass** across dance-mat, engine, hand-plan, hand-pose, session
and shell. New cases check mirrored non-crossing approaches, fixed planted
spots, simultaneous/diagonal landings, connected re-hit/return chains, same-tile
jumps, preserved prefixes and pruning. The twelve solver regressions retain
1 kHz full-track probes and 8 kHz probes around phase/previous failure times,
adding independent angular-speed checks.

The **79 category/layout/mirror expectations** remain 73 passes and six expected
contact rejections: zero new pose failures, timeouts, starved frames or late
guide jobs. All **17,927 legacy reference poses, velocities and accelerations**
match `78fff341` bit for bit. Strict clippy and full-game wasm32 compilation pass.
Browser checks run the actual WASM solver off-screen, including the shared-turn
speed gate. No full library audit or double-layout gameplay enablement was run.

The shared-entry issue recorded above is resolved for these studies. Review
deliberate turn recipes and broader choreography before enabling the remaining
layouts; this does not replace contact planning, replay or scene integration.
Private source pins, input/pose/derivative captures, measured before/after turn
rates and all verification reports remain uncommitted under
`target/scratch/shared-tile-turns/` (`SUMMARY.json`, `aligned-*`,
`browser-results.json`, `cohort-results/`, native/clippy/wasm/parity logs).

## Scope and invariants

Target one existing hand, with index/middle fingers as its two contacts, on two
side-by-side 3x3 mats. Either finger can use either mat; there is no permanent
finger-to-pad assignment. Preserve the accepted hand, taller stance, flexible
reference facing, balance, materials and exact contact times.

The four remaining **currently playable built-in layouts** are `dance-double`,
`pump-halfdouble`, `pump-double` and `danoni-9A`. Together with the seven supported
layouts, these cover `Layout::builtin_ids()` today. This does not add unplayed
StepMania couple/routine layouts, all 24 DanOni data definitions or arbitrary
custom layouts. Layout coverage also does not promise a feasible hand dance for
every chart: rapid transfers, distant simultaneous holds and hazards may still
need an explicit contact/pose rejection.

Keep main's integration boundary: gameplay judges its original effective chart;
the hand simplifies its own copy to at most two concurrent notes, ongoing holds
included. Never alter gameplay notes, scoring, replays or timing to fit the hand.
Preserve published contacts, native DanOni release budgets, current tap/hold/roll/
lift/mine/shock policies, single-threaded f64 preparation and the three-second
rolling window. Keep the current 2 mm targeting, 0.25 mm planted-height and 1 mm
tile-interior limits. Unsupported or failed hand preparation cannot block play.

## Mat and lane contract

Use stable physical tile identities: `(pad, keypad_cell)`, presented as `L1..L9`
and `R1..R9`. These label mats, not fingers. A compact internal index may still be
u8; do not scatter new arithmetic IDs through the planner and shaders. Single-mat
plans retain their existing identities and coordinates.

```text
             UP / reference front
    L7 L8 L9     R7 R8 R9
    L4 L5 L6     R4 R5 R6
    L1 L2 L3     R1 R2 R3
```

One canonical geometry description supplies tile centers, bounds, pad offsets,
scored lane mappings, reverse tile-to-lane lookup and unscored support candidates
to planning, pose preparation, replay validation and rendering. Keep physical
millimeters distinct from render meters and the existing hand-root offset.

For square pitch `p` and an extra seam gap `g`, place pad centers at
`x = -(3p + g)/2` and `+(3p + g)/2`, with local keypad coordinates unchanged.
Start the study with `p = 24 mm`, `g = 0`: a 3-row, 6-column mat footprint.
Single-mat geometry remains centered at zero. Pitch and gap are frozen before
planning a play; changing geometry must invalidate its warm/prepared state.
Camera framing and viewport sizing never change physical reach.

Proposed lane mappings, in the current chart model's order:

| Layout | Lane targets |
|---|---|
| dance-double | L4, L2, L8, L6, R4, R2, R8, R6 |
| pump-halfdouble | L5, L9, L3, R1, R7, R5 |
| pump-double | L1, L7, L5, L9, L3, R1, R7, R5, R9, R3 |
| danoni-9A | L4, L2, L8, L6, Oni, R4, R2, R8, R6 |

Pump half-double keeps the actual middle six panels of the two Pump pads; it
must not be recentered as six arbitrary lanes. The source definitions are
`crates/chart/src/layout/mod.rs` and `layout/danoni_table.rs`. Resolve final lane
indices after the Player's transforms; do not apply Mirror/Shuffle a second time.
DanOni's `Lane.pad` is zero for both arrow banks, so it cannot by itself resolve
9A's bank identity. Use its explicit built-in mapping.

**9A Oni mapping, accepted for the No stretch policy:** let its one logical lane use either
L5 or R5 as alternative targets. Select and freeze a physical landing before
publishing it; a hold stays there except for an explicitly validated handoff.
This avoids an arbitrary left/right bias or an extra tile between the mats.
The two arrow banks remain distinct scored lanes.

This requires lane-to-target-set mapping rather than one tile per lane. Replay
aggregates both Oni targets into the one logical button, emitting only actual
aggregate press/release edges. Moving between centers does not automatically
count as a repeated Oni hit: a real lift and new aggregate contact are required.
An Oni mine/shock makes both possible centers hazardous, regardless of which
was last selected. Validate re-grabs, handoffs, rolls and lifts using the actual
judge and native hold budget. Review the presentation of the two alternative targets in the physical study;
the logical lane remains one Oni button.

Version the contact/geometry contract explicitly. Legacy plans decode as the
accepted single mat; doubled tiles must require the double geometry, and unknown
versions/IDs are rejected. Geometry identity belongs in preparation/cache keys,
checkpoint compatibility and renderer state, not just the layout string.

## Reach is the first experimental gate

At the existing 24 mm pitch, opposite tile centers are 67.88 mm apart on a single
mat and 129.24 mm apart across the double mat, before within-tile offsets. The
current planner's nominal-anchor stance guard tops out at 51 mm for wide layouts;
that guard is not a measurement of maximum anatomical reach and must not simply
be raised to accept double poses.

Root translation can move a hand between pads, but cannot increase its maximum
simultaneous fingertip span. Start by solving static contact pairs with the
accepted joint bounds, knuckle clearance, skin contacts and balance. Test both
finger assignments, reference/opposite facing families, same-tile placements,
outer extremes and held cross-pad spans. Screen all 18x18 ordered tile pairs;
follow with scored-pair and transition tests per layout. Symmetry may reduce
geometric work, but mirrored targets need their own right-hand skin/rig checks.

Produce a readable reach map: feasible, near-bound and infeasible pairs, with
maximum residuals and the limiting controls. Show slow examples of each category.
A narrow failure caused by a poor seed is different from an impossible span.
Then compare a small fixed set of more compact double-mat pitches if necessary.
Smaller tiles must still accommodate separated same-tile contacts and the existing
1 mm edge margin; shrinking the grid alone is not an automatic solution.

Choose a single double-mat geometry from this evidence and visual review. Do not
stretch the fingers, widen anatomical bounds, change scale mid-song or animate
an impossible pair. If no acceptable geometry covers required pairs, decide
explicitly between limited one-hand coverage and a separate two-hand study before
claiming all remaining layouts are supported. Two hands would change contact
ownership and choreography substantially, so it is not this plan's default.

## Implementation stages

1. **Shared geometry, with single-mat parity.** Replace scattered keypad-only
   assumptions with the canonical mat description. Cover the planner's fixed
   nine-tile index/tables and future-note arrays, pose panel validation/centering,
   mine crossing checks, logical replay mappings, startup/cache keys and scene
   presentation. Preserve existing single-mat coordinates and arithmetic paths
   where their frozen tables matter. Pass all existing layout/contact regressions
   and compare accepted single-mat poses and C2 curves before enabling double.

2. **Static reach and a minimal crossing study.** Build the reach map above and
   review a small manually specified contact plan: within-pad steps, crossing
   the seam, both fingers on one pad, split support, one held contact with the
   free finger on the other mat, plus a wide jump and turn. Select geometry only
   after comparing static reach and checked motion. This stage does not expand
   the gameplay support list.

3. **Rolling contact planning, first dance-double (implemented above).** Preserve the bounded beam
   rather than enumerating all 18-tile transitions. Use geometry-derived or
   precomputed deterministic shape/distance data, retaining the current single-
   mat constants. Use cheap relative-reach gates before costly pose work; cache
   by geometry and relative placement where valid. Support candidates should
   come from nearby reachable unscored tiles, not a global scan of every stance.
   Neither finger is forced to stay on its nominal side. Include same-tile
   re-hits, rapid transfers, cross-finger holds and safe return from hazards.
   Hazard checks include crossed tiles during flight, not just its endpoints.
   Keep candidate order, cooperative checkpoints and preparation budgets bounded.
   Check native/WASM decisions; do not introduce uncached platform-sensitive
   angle calculations into the hot path in place of frozen geometry tables.

4. **Pose, balance and rendering integration (implemented for dance-double above).** Solve root travel across both
   pads while retaining the taller stance and support-aware tilt. Reuse relative
   pose seeds through translation/rotation where appropriate; do not multiply
   the style library by every absolute tile combination. Validate subframes,
   exact touchdowns, near-liftoff floor/knuckle clearance and C2 joins across
   window publications. Keep orientation continuous at the angle wrap.
   Replace render-only nine-tile arrays, mesh generation, shader indexing,
   contact lights, tile travel, glow spill, backing bounds and camera framing
   with the same geometry. Off-screen native and browser WebGPU/WebGL2 checks
   must show correct tiles and equal CPU/GPU geometry; no runtime solves on draw.

5. **Pump half-double, Pump double and 9A (implemented above).** Share the geometry/planner path,
   adding explicit mappings and any proven seed/stance needs. Pump side rests
   generalize to reachable unscored tiles on either mat, checked against known
   notes/hazards. For 9A, validate alternative Oni targets and aggregate logical
   edges before enabling it. Preserve native DanOni's distinct release budget.
   Do not infer safety from a center/side/corner name alone.

6. **Gameplay coverage and audit.** Add all four layouts to the host only after
   their fixtures pass actual-judge replay and pose/contact validation. Extend
   startup, preview, cache-invalidation, completed/quit/restart lifecycle and
   hand-On/Off scoring-isolation tests. Use bundled dance/Pump charts, generated
   conversions/simplifications and authored 9A cases; private files and traces
   stay outside Git. Run a representative rolling/native/WASM cost cohort first.
   After visual review, run the full supported-layout audit with four independent
   chart tests, reporting contact failures, pose failures, CPU/wall timeouts,
   unsupported layouts and settling/guide warnings separately. Do not interpret
   a completed layout mapping as a full pass on its charts.

## Regression priorities and acceptance

- Same glyph on different pads must never collide in IDs, replay, mines or lights.
- Exact note times, holds and published planning prefixes survive every window;
  the hand's On/Off state never changes gameplay chart/options/results.
- Within-pad, seam-crossing and outermost jumps; matching/opposing finger motion;
  both fingers on one tile, on one mat and on different mats; both assignments.
- Cross-pad holds with free steps, release/re-grab and handoffs; rolls and lifts;
  mines/shocks on the seam or all scored targets; recovery after rapid bursts.
- Oni repeated notes and mixed arrow/Oni jumps must produce the correct logical
  edges whichever center was chosen. Mines apply to both alternative centers.
- Existing seven layouts retain accepted motion/contacts and startup behavior;
  no tolerance or runtime budget is loosened merely for double support.
- No full-chart preflight, worker distribution, silent skip policy, rig/asset
  replacement or new idle choreography is introduced as part of this extension.

The first reviewable delivery should be stages 1 and 2: unchanged single-mat
motion, an explicit double geometry/mapping diagram, the reach map and a short
crossing study. They settle the key physical decision before broad planner work.
