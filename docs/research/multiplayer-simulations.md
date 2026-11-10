# First multiplayer simulation comparison

Exploration, 2026-10-31. This is an experiment proposal, not an implementation
plan or a report of results. It follows the [multiplayer study](multiplayer.md)
and the [non-spending weakening candidate](multiplayer-references.md#weakening-from-recent-accuracy-without-spending-power).
The maintainer is developing bots with tunable MSD skill profiles and mine
sensitivity in parallel; this study assumes that capability without specifying
its implementation.

The question is whether **mines-only play-through** or **mines-only weakening
based on recent accuracy, without spending attack power**, is the better default
candidate. Success means meaningful pressure with opportunities to recover and
retaliate, while accuracy remains consequential. Longer survival alone is not
evidence that weakening is better.

## Shared experimental conditions

Use the [worked match](multiplayer-match-example.md) as an illustrative starting
point: 100 life, zero starting power, a 100-power automatic attack, precise/good
original judgments earning 2/1 power, loose judgments and misses earning none.
Loose judgments cost 1 life, misses and mine hits 8; no healing, power wipes,
combo multipliers or knockout rewards. These are test inputs, not balanced rules.

Use taps-only charts without authored mines for the first controlled experiment,
then validate finalists on holds, lifts and authored mines with explicit scoring
and placement policies. All players in a match use the same chart and judgment
rules. Keep input category, chart timing and bot perception settings fixed within
a comparison. Do not infer suitability for every supported chart or input type.

Full attacks contain eight safely placed scattered mines across eight beats.
There is one recipient slot covering reservation, the effect and recovery, with
no backlog. Keep the same warning/commitment lead in both variants, long enough
to commit before the affected content is first visible. All variants reserve
the full attack span even when fewer mines remain. Recovery lasts at least four
beats after clearance and until eight subsequent original judgment opportunities
have occurred, whichever is later. This provisional opportunity floor prevents a
rest from serving as the entire recovery period. It does not depend on whether
the recipient hits those notes.

Attacks launch at four-beat checkpoints; only admitted launches spend power.
Use rotating arbitration for simultaneous requests. If no valid recipient or safe
eight-mine section is available within the permitted horizon, retain power and
record the blocked launch. Never fill unsafe positions to satisfy the experiment.
Use identical admission and elimination ordering in both variants. End-of-song
survivors rank by life, then original-chart accuracy, as in the worked example;
record how often this ending replaces a last-player-standing result.

Start with these deliberately provisional defense settings:

| Rule | Play-through | Accuracy-based weakening |
| --- | --- | --- |
| Defensive power cost | None | None |
| Recent-performance score | Logged for comparison | Determines reduction at commitment |
| Mine count | Eight | Four to eight |
| Subsequent changes to visible mines | None | None |

For a reproducible first curve, assign experimental performance values of 1 to
precise judgments, 0.8 to good, 0.4 to loose and 0 to misses. These values are a
separate research indicator, not a change to the game's accuracy statistic or a
claimed mapping to a shipped ruleset. Average the last 32 required tap judgments,
including misses. Initially fill the window with 0.9-valued prior observations,
replacing one per real judgment; rests never age observations out.

Reduction is `clamp((recent_score - 0.8) / 0.2, 0, 1) * 0.5`. Remove
`floor(8 * reduction + 0.5)` mines using a seeded, stable removal order over the
full valid pattern. Thus scores 0.8, 0.9 and 1 give eight, six and four mines.
The estimate is gradual but integer mine counts introduce steps: log them rather
than assuming continuous difficulty. Retained mines are a subset of the same
full pattern, fixed at commitment; no reward comes from mine avoidance. Freeze
these settings for the first batch. Check whether the tested bots actually span
the curve before interpreting it; if nearly all sit on a plateau, revise the
declared curve and rerun both arms rather than selecting favorable runs.

## Six initial scenarios

| Scenario | Setup | What it should reveal |
| --- | --- | --- |
| 1. Equal-skill room | Eight matched bots using Spread, repeated at three baseline skill levels | Whether weakening preserves interaction and whether ordinary matches already need it |
| 2. Sustained focus | Eight matched bots; three prefer one focal recipient whenever eligible, others Spread; focal bot Retaliates | Whether recipient-wide recovery and non-spending defense leave usable retaliation opportunities under repeated pressure |
| 3. Weaker recipient | Same focus setup, but focal bot one calibrated skill band below opponents | Whether weakening helps the struggling player or mostly disappears precisely when needed |
| 4. Stronger recipient | Same setup, focal bot one band above opponents | Whether accuracy's offense and defense benefits make that player excessively dominant |
| 5. Final duel | Two bots, first matched and then adjacent skill bands; each targets the other | Whether the rules work when redistribution cannot relieve pressure; whether weakening makes interaction negligible |
| 6. Disruption and recovery | Matched bots in a controlled focus sequence; one externally imposed poor phrase, followed by ordinary play and legal repeated volleys | Whether one disturbance lowers protection and compounds subsequent errors, or performance recovers before pressure resumes |

Focus is a stress condition, not a proposed default targeting policy. Rotate the
focal seat and attacker identities. Attackers still have to earn power and obey
admission; there is no unlimited external attack source in full matches. The
poor phrase in scenario 6 must be identical between variants, affect original
judgments, and end at a declared point. It is an experimental disturbance, not
an extra permanent bot handicap.

Run every scenario on at least three chart structures: relatively even density,
dense bursts with sparse recovery sections, and charts emphasizing different
MSD skills at comparable baseline challenge. Where possible use several charts
per structure, rather than treating one song as representative. Calibrate skill
bands from no-attack performance on those charts; report actual accuracy and
survival, not just the nominal skill label. Exclude or separately label cases
where the chart alone eliminates most bots before attacks could matter.

Independently cross low, medium and high mine sensitivity, covering direct hits,
disruption while successfully avoiding mines, and recovery time. Include a
zero-sensitivity control for validation. Do not assume that baseline skill fixes
mine tolerance. The bot's response should depend on the presented pattern and
its state, not whether the ruleset is named play-through or weakening. More mines
need not have a linearly greater effect.

## Separate defense effects from match feedback

First use a controlled exposure harness: present the same full candidate patterns
on the same legal schedule in both variants. Apply each variant's own weakening
and allow its performance to evolve. This isolates response to pressure; it does
not establish that live opponents could generate that schedule. Log offensive
power and readiness even when there is no live target.

Then run full matches where targets, charge, survival and attack times evolve
independently under each rule. Pair chart, initial bot state and exogenous random
seeds, not the eventual judgment streams or attack schedules. Use stable random
keys for original notes and separate attack/placement streams so removing a mine
does not shift every later random draw. Rotate seats. Keep a no-attack baseline
for each chart/profile combination.

Start with 100 paired seeds per scenario/chart/profile condition as a pilot,
not a claim of statistical sufficiency. Report paired differences with uncertainty
intervals at the match/run level; do not count notes or players within a match as
independent runs. Expand uncertain or inconsistent conditions. Reserve additional
charts and seeds for checking any tuning selected from the pilot.

## Outcomes to record

| Outcome | Measurement and interpretation |
| --- | --- |
| Retaliation opportunity | From first focused attack commitment to next admitted outgoing attack; report deaths before sending and song-end censoring separately. Also record time to a full bar, separating difficulty earning power from target/placement admission delays. |
| Offensive suppression | Power earned per original judgment opportunity during a common observation horizon, relative to the paired no-attack run. Separately report actual launches, capped-power time and survival; a survivor has more opportunities to accumulate attacks. |
| Meaningful pressure | Direct mine hits, surrounding original-note errors, incremental life loss, eliminations and changes to match outcomes relative to no attacks. Avoided mines can still cause substantial disruption. |
| Recovery | Post-disturbance performance against the paired undisturbed trajectory, plus actual and estimated accuracy, committed mine strength and time/opportunities until recovery. A provisional recovery marker is two consecutive 32-note blocks within 0.03 of baseline performance score; also plot trajectories so the marker does not conceal oscillation. |
| Skill advantage | Win rate and placement by baseline skill band, especially the stronger recipient; compare the change between rulesets rather than interpreting any strong-player win as a problem. |
| Match shape | Match length, survivors at song end, admitted attacks per playable minute, exposure per recipient, and blocked launches by reason. Identify interactions lost through eligibility or lack of safe placement. |

Always report scenario and sensitivity groups separately before aggregating.
For recovery tests, show elimination and unrecovered runs alongside recovered
ones; survivor-only averages could make the harsher rule appear healthier.
Use common fixed observation horizons for suppression, recording death explicitly
rather than inventing post-elimination judgments.

## What would change the preference

**Favor accuracy-based weakening** if focused players more often send before
elimination, regain performance sooner and suffer fewer prolonged suppression
episodes, across skill bands and mine sensitivities. The improvement must coexist
with consequential mine pressure and must not chiefly accrue to the strongest
players. A longer match with the same inability to retaliate is not success.

**Favor play-through** if the shared exposure/recovery rules already provide
comparable opportunities, or if weakening contributes little beyond additional
survival time, largely protects already strong bots, makes attacks inconsequential,
or worsens the post-error performance spiral. Its simpler rule is a reason to
prefer it when differences are small or unstable, not evidence to disregard a
consistent recovery benefit.

**Choose neither yet** if both suppress focused players through elimination,
or conclusions reverse across reasonable bot-sensitivity assumptions. Revisit
attack pressure and recovery first. If safe placement or admission dominates the
results, fix the experiment's feasible chart conditions before judging defense.

If weakening looks useful, add a diagnostic third arm with constant weakening
(initially six mines), holding every other rule fixed. If necessary compare
constant reduction levels around the observed mean dose of the adaptive arm.
This separates the benefit of fewer mines from the benefit or harm of accuracy
feedback; equal mine counts are not assumed to mean equal difficulty. If constant
weakening provides the same recovery with less skill amplification, lowering
baseline pressure is a credible simpler alternative to either initial candidate.

Only then probe a smaller maximum reduction (25%) and shorter/longer histories
(16/64 judgments), changing one factor at a time. Revalidate on reserved charts
and seeds. These are sensitivity checks, not a search for a favorable win rate.
No numerical effect-size acceptance threshold is established yet: use pilot
uncertainty and practical differences to propose one before a confirmation batch.

Simulations can reject fragile rules and identify promising ranges. Human tests
must still establish whether scattered mines create enjoyable tension, whether
the committed warning is understood, and whether players perceive weakening as
earned protection. Neither bot survival nor win rate settles those questions.
