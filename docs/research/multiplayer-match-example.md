# An illustrative multiplayer battle

Exploration, 2026-10-27. This is a worked thought experiment accompanying the
[multiplayer study](multiplayer.md), not a selected design or a simulated result.
All numbers are invented for comparison. The candidate combines scattered mines,
temporary Hidden/Sudden and an accuracy-based power bar that launches attacks
automatically. It tests whether targeting, defense and elimination support
accuracy under pressure.

Historical example: the later [battle plan](../plans/multiplayer.md) and
[prototype handoff](../plans/multiplayer-prototype-handoff.md) take precedence
for implementation. In particular, this example's mixed attacks and
power-spending defense are comparison candidates, not the current mines-only,
non-spending weakening direction. Its numerical trace is not a balance result.

The subsequent [reference and configuration study](multiplayer-references.md)
places this example among configurable room presets. Its automatic actions fit
the maintainer's preference to keep attention on notes; its targeting, defense
and elimination choices remain variables for comparison.

## Conditions for this example

Eight players play the same three-minute, four-lane chart at 120 BPM, with the
same competitive judgments and input category. Their device calibration and
comfortable scroll speeds remain personal. For this example the baseline notes
are fully visible, without voluntary visibility modifiers or lane covers. That
restriction is a cost of this candidate, not a decision for multiplayer generally.

Each player starts with 100 life and zero power. There is no combo multiplier,
knockout power bonus or passive life loss. Power comes only from original chart
judgments; mine avoidance earns none. A precise hit earns 2 power, a less precise
but good hit 1, and a loose hit or miss zero. These are illustrative judgment
bands, not mappings to a particular existing ruleset. Power caps at 100 and does
not decay during rests. Mistakes do not erase saved power.

Original taps, hold heads and lifts each supply their ordinary single judgment
opportunity; tails and repeated hold ticks do not multiply charge. This leaves
hold-heavy sections with fewer opportunities, which is acceptable for a first
same-chart comparison but is a balance question. All damage examples below use
taps and mines so no hold-accounting assumption hides in the arithmetic.

Every four beats, a common battle checkpoint resolves matured defense requests
first, then launches ready attacks. Reaching 100 between checkpoints holds the
bar full until the next checkpoint. This gives an explicit ordering independent
of packet arrival and introduces up to two seconds of launch delay at this tempo.
These checkpoints are a game rule for discussion, not a networking specification.

## Targeting and attack admission

Choose a targeting policy before play:

| Policy | Selection among eligible opponents | Intended role |
| --- | --- | --- |
| Spread | Seeded rotation, avoiding repeats where possible | Default; varied encounters with little monitoring |
| Retaliate | Most recent attacker, otherwise Spread | Creates an understandable rivalry |
| Leader | Highest current life, then original-chart accuracy | Applies pressure to strong survivors |

Only an opponent with no reserved attack, active effect or recovery interval is
eligible. A targeting preference cannot bypass this limit. For this example,
there is one attack slot per recipient and no backlog. If no opponent is
eligible, the attacker keeps the full bar and tries at a later checkpoint.

When several attackers choose the same opponent at one checkpoint, a rotating
priority allocates the slot; unsuccessful attackers try another eligible target
under their policy or retain their charge. Priority is shared match state rather
than packet arrival order. This arbitration would need evaluation for bias; it
does not imply any particular player deserves the slot.

An admitted attack costs 100 power. The target and type then remain fixed and
become visible to both players. New attacks from everyone use the repeating
sequence **Mines, Hidden, Mines, Sudden**, advancing only on an admitted launch.
This avoids a random draw determining whether someone receives their hardest
effect, while still allowing the match's timing to vary. Pattern predictability
and synchronized volleys are tradeoffs to examine.

## Warning and automatic defense

Admission produces a warning, followed by a scheduled defense checkpoint, a
content commitment and the future affected section. Exact lead time depends on
when its notes first become visible. Defense must settle before that visibility;
the target does not get to cancel hazards already being read. Each warning gets
at least one full four-beat earning interval before defense resolves. If network
or reading lead requires more time, schedule a later section.

At the defense checkpoint, **50 power automatically halves the attack**. Otherwise
the full attack is committed. At most 50 is spent, even if the bar is full. The
remaining power stays in the bar. New attacks launched at this checkpoint cannot
retroactively claim a defense payment at the same checkpoint.

| Attack | Full effect | With defense |
| --- | --- | --- |
| Scattered mines | 8 safely placed mines over an 8-beat section | 4 of those mines across the same section |
| Hidden | An affected section spanning 8 beats | A section spanning 4 beats at the same strength |
| Sudden | An affected section spanning 8 beats | A section spanning 4 beats at the same strength |

Counts and durations are examples, not equivalent measured difficulty. Mine
placement respects required inputs and timing windows while allowing distracting
hazards in otherwise unused lanes. The defended mine set is a subset of the
announced full pattern. Only sections supporting the full valid pattern are
eligible; if none is available within the match's allowed horizon, no launch is
admitted or paid for. This feasibility rule may make particular charts or late
sections unsuitable; it must not turn into a hidden weak attack.

Visibility strength must leave usable reading time at each player's scroll
speed. Effects apply to a selected future group of notes with intelligible
boundaries; they do not abruptly change every note currently on screen. Mines
remain readable, including original chart mines. A visibility attack is
ineligible where that cannot be maintained.

No effects overlap. A recipient becomes eligible again only after the affected
notes and hazards have cleared and a four-beat recovery interval has passed.
Thus even defended mines reserve their full eight-beat span; a shorter defended
visibility section may finish sooner, but must still clear from the field.
Using note clearance rather than only a timer also protects long scroll leads
and holds across boundaries.

This defense is deliberately incomplete. Accurate play buys a smaller challenge,
then surviving that challenge is the remaining defense. It prevents perfect
cancellation from removing all interaction, but it taxes targeted players' attack
output. Immunity is not the reward for having a full bar.

## Life and elimination

For the example, precise/good hits lose no life; loose hits lose 1; a missed tap,
mine hit or dropped hold loses 8. A failed hold head is one failure, not also an
automatic second drop penalty. Life never recovers in this version. A mine can
hurt indirectly through the hesitation it causes even when it is avoided.

Zero life eliminates a player. The underlying failure is what causes damage;
receiving an attack does not subtract life by itself. A notification can say
that a player fell during another's attack, but this does not prove the attack
caused every mistake. There are no knockout rewards in this comparison.

At a checkpoint, finalize eliminations from the preceding interval before
resolving defense and new launches. Eliminated players cannot launch. Attacks
they already launched remain; attacks whose recipients are eliminated expire
without retargeting or a refund. That preserves the meaning of previous choices
and can produce mutual knockouts. Simultaneous elimination is a tie, never a
race between result packets. A winner is not announced while an already
committed attack can still produce a mutual knockout.

If one player remains after outstanding attacks resolve, they win. If several
finish the song, rank them by remaining life, then original-chart accuracy, with
an exact tie shared. Attack sections cannot extend beyond the playable song;
late full bars may therefore remain unspent. This creates a bounded survival
match, but does **not** guarantee a last-player-standing finish. A strict battle
royale would need an additional round or another explicit ending rule. We should
evaluate that choice before adding passive damage or ever harsher attacks.

## A sequence within the match

Follow Ada, Bea and Cy among the eight players. The following state is a
constructed example, not a prediction of their performance. Each listed judgment
batch occurs before its stated checkpoint; every checkpoint has completed the
prior interval's judgments before resolving battle actions.

| Event | Power and life | Interaction |
| --- | --- | --- |
| Ada launches her first attack | Ada 100 → 0 power. Bea has 35 power. | Spread selects eligible Bea; 8 scattered mines are announced for a future section. |
| Bea plays through the warning | 10 precise hits add 20: 35 → 55. | At the scheduled defense checkpoint she spends 50, retaining 5. The attack becomes 4 mines. |
| The mines pass through Bea's field | Bea avoids all 4, but hesitates and makes 3 loose judgments: 100 → 97 life. | The mines matter without a mine hit. Those loose hits also earn no power. |
| Bea recovers and counterattacks later | Further original-chart hits eventually fill her bar: 100 → 0. | Retaliate selects Ada once she is eligible; Bea's first attack is mines too. |
| Ada cannot afford defense | Ada has 42 at the defense checkpoint, so retains 42. | The 8-mine attack lands in full. One mine hit and 2 loose taps take life from 100 to 90. |
| Cy launches his second attack later | Cy spends 100; his sequence now selects Hidden. Bea reaches 64 before its defense checkpoint. | Bea spends 50, retains 14 and faces the shorter Hidden section. |
| Bea survives Hidden | One missed tap takes life from 97 to 89. | She keeps her saved power. The miss does not also wipe out her ability to retaliate. |
| Bea receives Sudden much later | After other play she has 17 life and only 30 power at defense. | Full Sudden causes 2 missed taps and 1 loose judgment: 17 − 16 − 1 = 0. Bea is eliminated. |

Each later attack on Bea occurs after the earlier attack and recovery have
cleared. The example does not stack mines with Hidden or Sudden. Other players
continue the same loop; their choices determine who pressures Ada and Cy while
this rivalry develops.

The important feedback is visible: pressure can lower accuracy, slowing charge;
spending charge on defense delays retaliation; surviving the smaller effect
allows the player to build toward an attack again. The one-slot limit and recovery
bound the loop, but do not prove it is balanced. Sustained targeting could still
keep someone permanently spending on defense, especially in a final duel.

## What this example helps us compare

**Targeting:** Spread shares encounters, Retaliate creates understandable rivalries,
and Leader encourages pressure on strong survivors. Excluding a low-life targeting
policy from this first comparison avoids deliberately concentrating fire on those
already struggling. It is a choice to test, not a prohibition for other modes.

**Defense:** The shared bar makes the opportunity cost clear. The sharp 49/50
threshold could nevertheless feel arbitrary; proportional weakening is a useful
comparison. A separate shield meter would reduce competition between defense and
offense, but also change the central economy. A play-through-only version tests
whether automatic resource defense is needed at all.

**Elimination:** Every loss has a gameplay cause, and the absence of meter wipes
and knockout bonuses limits additional snowballing. Permanent life loss creates
attrition but gives no health comeback. A small capped recovery during protected
intervals is an alternative to compare, not assumed in the trace above.

**Final duel:** The default fallback gives both players a target, but an early
lead may let one repeatedly attack while the other spends 50 defending. Recovery
and charge rates need examination together. If shared-bar defense creates an
inescapable disadvantage, change that economy before raising attack intensity.

**No errors:** If everyone plays flawlessly, attacks continue but nobody loses
life. An end-of-song tie is allowed by this example. Guaranteeing exactly one
winner would require choosing a different product rule rather than pretending
that attacks alone guarantee elimination.

The first useful comparison would hold targeting, chart and power earnings fixed
and contrast this half-strength defense with cancellation and play-through-only
defense. Separately compare mine-only play against the mixed attack sequence.
Observe original-note accuracy, mine hits, time spent under attack, defense
spending and opportunities to retaliate. Human play would be needed to learn
whether distraction creates exciting pressure or frustrating loss of control.
