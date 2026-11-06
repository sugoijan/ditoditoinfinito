# Double mat for the remaining hand layouts

Plan started 2026-11-06; No stretch preparation implemented 2026-11-07.
No new hand layout is enabled yet. Based on local
main `865cac6d`, including the accepted 3x3 hand work cherry-picked as `fd136299`.

## Preparation status (2026-11-07)

The general **Options → Notes → No stretch** toggle and automatic hand-copy
simplification are implemented; see [the policy and verification contract](chart-simplification.md#no-stretch-2026-11-07).
The source regions are StepMania's dance/Pump masks, with ongoing-hold checking,
the Pump half-double subset and 9A's Oni allowed via either center. The two-pad
reach study should start from these allowed pairs instead of assuming that every
pair on the full 3x6 grid must be supported. These masks are not an anatomical
reach certificate. Pair filtering covers all four layouts; double contact/pose
planning, actual center assignment, render geometry and host enablement remain
future work. Existing single-mat motion and gameplay independence are preserved.

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

3. **Rolling contact planning, first dance-double.** Preserve the bounded beam
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

4. **Pose, balance and rendering integration.** Solve root travel across both
   pads while retaining the taller stance and support-aware tilt. Reuse relative
   pose seeds through translation/rotation where appropriate; do not multiply
   the style library by every absolute tile combination. Validate subframes,
   exact touchdowns, near-liftoff floor/knuckle clearance and C2 joins across
   window publications. Keep orientation continuous at the angle wrap.
   Replace render-only nine-tile arrays, mesh generation, shader indexing,
   contact lights, tile travel, glow spill, backing bounds and camera framing
   with the same geometry. Off-screen native and browser WebGPU/WebGL2 checks
   must show correct tiles and equal CPU/GPU geometry; no runtime solves on draw.

5. **Pump half-double, Pump double, then 9A.** Share the geometry/planner path,
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
