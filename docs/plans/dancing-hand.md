# Dancing hand in gameplay

The default native and web builds include the dancing hand. Display options
control its visibility, coordinated field/hand layout and frame size. New settings
start with the hand enabled; saved Off choices remain Off. Four-panel and DanOni
five-key charts are supported, with Oni on the mat's center. The hand is an
autoplay beside the play: it plays its own copy of the notes the play judges, at
most two at once (`At once: up to two`, ongoing holds included), while the play
judges every note with the player's own options. The hand never makes a play an
assist, never shows in its mods and never moves its bests (2026-11-06; before, the
play judged the hand's copy and every play with the hand was marked ASSIST). Source
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
