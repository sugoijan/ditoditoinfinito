# Multiplayer references and room configuration

Research reviewed 2026-10-27, accompanying the [multiplayer study](multiplayer.md).
The maintainer identifies **TETR.IO as a major reference**, wants substantial but
understandable room configuration, and prefers automatic targeting and attacks
because playing the chart already occupies the player's attention. A sensible
default should make a good match easy to start; communities should be able to
experiment and share configurations. No implementation design is selected.

The most relevant comparisons are TETR.IO for configurable rooms, Taiko's Don
Katsu Fight for automatic rhythm-based offense, pop'n music for disruptive rhythm
battles, Puyo Puyo for defense through ordinary play, and Tetris 99 for targeting
policies. Lumines Arise and Tetris Effect Connected add useful comparisons for
musical presentation, focused variants and recovery.

## TETR.IO as the main room reference

**Documented behavior.** The game's public interface separates ranked TETRA
LEAGUE from custom public/private rooms. Its official mechanics FAQ documents
built-in room presets, the `/set` configuration command and community tools for
saving/reapplying setups. [Game][tetrio] · [Mechanics FAQ][tetrio-faq]

**What to borrow.** Give a room a recognizable starting ruleset, then let its
host adjust it and share the result. A preset represents a complete playable
combination, not merely a collection of unrelated default values. If a future
ranked mode exists, keep its fixed comparison rules distinguishable from custom
rooms. Configuration variety need not create separate public matchmaking queues
for every combination.

The FAQ's tools demonstrate an appetite for repeatable room setups. For our own
game, saving and sharing a room configuration could be a first-class affordance:
name, version, starting preset and a short list of deviations. This is a proposal,
not a claim about TETR.IO's current import/export UI.

**Source limit.** The FAQ includes historical material and cannot establish the
current exact preset count, complete setting inventory or numeric defaults. The
live room editor was not inspected past the join screen. Treat this as a study
of documented configuration patterns, not a hands-on audit of the current client.
In particular, do not infer current targeting behavior or room export support
from older community examples, or confuse personal settings export with room
configuration export.

## Taiko no Tatsujin Don Katsu Fight

**Documented behavior.** Bandai Namco's guide says playing builds Taiko Power;
reaching 100% attacks the opponent. Victory comes from reducing the opponent's
health to zero or retaining more health when the song ends. Item notes supply
disruptive and beneficial effects, with fake notes and darkness among the listed
disruptions. [Official mode guide, Japanese][taiko]

**What to borrow.** This is a direct precedent for normal rhythm play producing
automatic offense, with a meter visibly connecting performance to battle events.
It also supplies a clear finite-song ending and supports our interest in visual
interference without requiring a wholly different authored chart.

**What differs.** It is a duel, so it does not solve many-player targeting.
Automatic power attacks and disruptive item notes are distinct mechanisms in the
guide. Combining them into one automatic attack pool is our proposed adaptation,
not an existing Taiko rule. The guide does not establish our proposed exact
accuracy-to-charge formula. Direct health attacks also differ from our example,
where attacks create conditions in which players might make mistakes.

## Pop'n music and nuisance attacks

**Documented behavior.** Konami's pop'n music 7 page describes two-player,
three-button Battle with nuisance attacks. The later Usaneko guide offers
competition with nuisance attacks, without them, or no competition; its Battle
mode also lets the two players choose different difficulties.
[Konami product page][popn7] · [Usaneko guide, Japanese][popn]

**What to borrow.** Interference can be an explicit mode choice within a rhythm
game. We can offer a recognizable battle preset while retaining an accuracy-only
comparison and a mine-only variation. Players should know which experience they
are joining before starting the song.

**What differs.** These historical versions do not establish current service
availability or one universal pop'n battle ruleset. Different-difficulty play is
a social affordance, not evidence that difficulty normalization is solved. The
primary pages above do not establish exact attack charging, reflection or
Hidden/Sudden costs, so those details need further source work before copying
any specific precedent.

## Puyo Puyo and defense through ordinary play

**Documented behavior.** Sega's Puyo 2 guide describes chains sending garbage
and configurable offsetting of incoming garbage. Other exposed options include
the time before garbage pressure increases and the number of multiplayer rounds.
[Sega's game and settings guide][puyo]

**What to borrow.** Defense need not require a separate defensive input. The
same successful play that produces offense can reduce incoming pressure. Also,
offsetting, escalation timing and round count are understandable match choices;
they describe meaningful differences in play rather than exposing every formula.

Compare three named defense rules in our study: play through the effects,
automatically weaken them, or cancel pending pressure. Keep their explanations
short enough to fit in the room summary. A numerical defense percentage can be
an advanced adjustment once each rule's interaction with charging is understood.

**What differs.** Puzzle players can develop a chain at a chosen time, while a
chart supplies fixed opportunities. A defense deadline must respect the upcoming
notes and rests. Also, full cancellation could make equally strong players see
few effects; weakening preserves some interaction but may trap the target in
repeated defensive spending. The precedent makes cancellation worth testing,
not automatically preferable.

## Tetris 99 and policy based targeting

**Documented behavior.** Nintendo describes policies targeting random opponents,
attackers, players near defeat and badge holders, plus direct manual targeting.
Its Attackers policy can send to multiple attackers, and knockout badges increase
attack power. [Nintendo's official guide][t99]

**What to borrow.** Selecting an intention is less demanding than repeatedly
choosing an individual opponent. For our default, a usable policy should already
be selected and work for an entire song with no additional input. A different
policy can be chosen in the lobby or between rounds.

**What differs.** Tetris 99 supports switching policies during play; we should not
assume that this is comfortable on a dance pad or during a dense chart. Its
multi-target retaliation and badge economy are substantial balance rules, not
mere target-selection preferences. Our candidate Retaliate selects one opponent;
it must not silently inherit duplicated outgoing attack power.

The host could select a common targeting rule or permit a small set of player
policies. Those are alternative room rules. The latter needs visible defaults
and restrictions on valid targets, so every player need not configure anything.
Manual targeting or launch, if ever supported, should be an explicit custom-room
variant rather than a hidden competitive advantage in the automatic preset.

## Lumines Arise and Tetris Effect Connected

**Documented behavior.** Lumines Arise clears completed squares with a musical
timeline. Burst is manually activated after charging above 50%; its multiplayer
mode uses Burst to pressure an opponent with garbage, with ranked, custom and
local play. The game also provides dedicated training missions and a Quick Burst
challenge. [Developer's game guide][lumines]

**What to borrow.** Make the consequences of an attack legible in the music and
give players a way to learn a mechanic without a full match. An eventual solo
practice setting that applies the room's mine or visibility attacks could help
players assess unfamiliar configurations. This does not require multiplayer
implementation during the current research phase.

**What differs.** Burst is manually activated. Its meter is a useful visual
reference, but its additional button decision is not our desired default.
Beat-synchronized puzzle clears also do not prove a network timing solution for
an audio-judged chart.

**Documented behavior.** Tetris Effect Connected's developer tips describe
Friend/Local adjustments such as Zone Battle speed and handicaps, and escalating
Zone Battle phases. They also describe automatic boss defense in Connected VS.
[Developer tips][tec]

**What to borrow.** Tie choices to a specific mode rather than showing every
option everywhere. Study escalation and recovery as explicit rules, with
automatic state changes where attention is scarce. Friendly handicaps should be
clearly distinguished from competitive rules shared by all participants.

## A configuration model to explore

The following is our synthesis from the references, not a feature list to
implement. The broad principle is **room configuration before play, automatic
execution during play**. Player calibration, bindings and comfortable scroll
speed remain separate from room rules. Any appearance restrictions required for
visibility attacks must be visible before players ready up.

### Presets and ordinary room choices

Begin with a small number of coherent presets: **Standard Battle**, **Mines Only**
and **Accuracy Race** are useful comparison candidates. They describe different
experiences without claiming to be balanced yet. A Custom state records edits
to a preset. Additional presets should emerge from distinct successful ways of
playing, rather than from every possible combination of settings.

| Setting family | Ordinary choices worth comparing | Deeper options, shown only where relevant |
| --- | --- | --- |
| Attack pool | Mines; mines plus Hidden/Sudden | Enabled effects, frequency weights or fixed sequence, repetition limits |
| Pressure | Gentle; Standard; Intense | Charge curve, attack cost, duration/strength, recovery and queue limits |
| Targeting | Shared Spread; Retaliate; Leader; player choice | Allowed policies, repeat-target limits, target allocation |
| Defense | Play through; weaken; cancel | Cost, amount, reserve behavior and priority relative to offense |
| Survival | Life elimination; finish the song | Starting life, damage weights, capped recovery, ending and tie rules |
| Match format | One song; several rounds | Round count, song selection, chart eligibility and difficulty policy |

The short choices are a route into the deeper configuration, not a substitute
for it. For example, Pressure must name a coherent set of compatible values.
Increasing charge rate, effect duration and shortening recovery independently
can accidentally turn a modest preset into near-continuous interference.
If a host overrides one bundled value, show Custom pressure and retain a clear
description of what actually changed.

Room-wide player limits, privacy and whether spectators can stay are separate
lobby settings. They should not bury the rules that change a played note or an
attack. Host edits between rounds should update the rule summary and require
players to ready again; they should not silently alter a song already underway.

### A provisional default to compare

Keep the previous [worked match](multiplayer-match-example.md) as one candidate:
same chart and rate, accuracy-driven automatic launch, a default Spread policy,
mines and bounded Hidden/Sudden, automatic weakening, one incoming attack at a
time, a recovery interval and a finite-song ending. No target selection, launch
button or defensive input is required during the song.

The 100/50 power costs, exact damage numbers and fixed attack sequence remain
example parameters. Automatic weakening is especially unsettled: the Puyo
comparison argues for testing cancellation, while play-through-only defense
could prove easier to understand. We should not call any of these Standard until
players have compared their effects on enjoyment, accuracy and comeback chances.

### What should remain reliable across custom rules

Some properties make a configuration valid rather than balanced: no unavoidable
mine conflicts with required notes, enough warning and readable exposure,
defined resolution of late events, bounded effects, unchanged audio judging,
and an explicit match ending. These should not become ordinary host sliders.
The visual effect can be stronger without allowing an unreadable required task.

Validation should evaluate combinations, not only each number separately.
Examples: reject overlapping effects that hide actionable mines; reject a
defense cost above a capped meter's reachable value when paid defense is enabled;
explain that disabling attacks makes targeting and attack defense irrelevant;
and require an ending rule when recovery could otherwise let all players survive
indefinitely. Disable irrelevant controls and explain invalid choices rather
than silently changing the host's intended rules.

Passing those checks does not prove a configuration is fun or balanced. Maintain
a distinction between a **tested preset** and a **valid custom configuration**.
Supported limits keep experimentation meaningful, while community favorites
provide evidence for revising defaults. Results from different rule variants
should not silently compete on one leaderboard.

### Making community experimentation useful

Save/share named configurations with a version and reproducible rules. Before
joining, show the preset plus its deviations in plain language. For example:

> Standard Battle, modified: mines only; cancellation defense; longer recovery.
> Automatic targeting and launch. One song; survivors ranked by life, then accuracy.

A result or replay should retain which configuration was used. Once players can
try it, compare configurations with the same chart and similar participants;
collect feedback on distraction, clarity, recovery and perceived fairness, along
with attack frequency, time under effects, defense spending and survival. High
usage alone can reflect novelty or a popular host rather than good balance.
Avoid turning preferences into a universal ranking of configurations: different
groups may reasonably prefer different experiences.

## Comparing six attack and defense combinations

This is a qualitative prediction, not a playtest result. Hold chart, targeting,
charge earnings, life rules, warning, incoming limits and recovery fixed. Mixed
means mines, Hidden and Sudden occur as separate attacks; it does not introduce
overlapping concealment and mine hazards. Preserve the earlier constraint that
all actionable mines remain readable.

For a concrete comparison, use the worked example's 100-power outgoing attack.
Play-through spends nothing on resource defense. Weakening automatically spends
50 to halve an incoming mine count or visibility section when affordable.
Cancellation instead spends 100 to remove the pending attack entirely when
affordable. Both paid defenses resolve before offense and before affected notes
become visible. These costs are illustrative: equal nominal power per removed
portion does not prove equal difficulty, since halving mine count or effect
duration need not halve disruption. Cancellation with partial repayment is a
further variant, not silently included in these six cases.

| Combination | Likely experience | Main advantage | Main concern | Default potential |
| --- | --- | --- | --- | --- |
| Mines only, play-through | Keep playing precisely while distinguishing mines from notes; charge reliably progresses toward retaliation | One clear bar purpose and consistent reading conditions | Every admitted attack lands; recovery and pressure caps must do more work | Strongest simple baseline and a plausible default |
| Mines only, weakening | Accurate play makes the incoming hazard field smaller, then the player handles what remains | Visible defense without eliminating battle events | Repeated 50-power payments can delay retaliation; the threshold can feel abrupt | Leading candidate if defense is legible and players can fight back |
| Mines only, cancellation | Maintain enough precision to erase incoming volleys; occasionally face a full volley | Clear moments of completely defending an attack | 99/100 cliff, reduced visible interaction among strong players and full hits on weaker ones | Useful defensive variant; less convincing as the initial default |
| Mixed, play-through | Adapt repeatedly between hazard discrimination, remembering timing and reading with shorter notice | Most immediate variety with the simplest meter | Personal reading preferences can make attacks very unequal; uninterrupted adaptation can feel exhausting | More promising as an explicitly varied preset |
| Mixed, weakening | A changing set of challenges, with charge shortening or reducing them | Variety with an automatic way to limit exposure | All the shared-bar costs plus a less consistent meaning of “half an attack” | Best richer alternative to the mine-only candidates |
| Mixed, cancellation | Try to preserve normal reading by canceling disruptions; cope with a full effect when unable | Complete avoidance of an unwanted effect can feel rewarding | All-or-nothing exposure and unequal effect value make automatic spending hard to judge | Specialist custom option before a default contender |

### The same moment under three defense rules

Suppose a player reaches 100 power during an incoming mine warning and an
eligible outgoing target exists. At the scheduled defense checkpoint:

| Rule | Incoming result | Power afterward | Outgoing result |
| --- | --- | --- | --- |
| Play-through | 8 mines | 0 after launching | Sends an attack now |
| Weakening | 4 mines | 50 | Retaliation waits for more accurate hits |
| Cancellation | No mines | 0 after defense | Retaliation waits for a full recharge |

This exposes the actual choice between presets: how much ordinary offensive
progress is automatically converted into protection. All three require no
extra in-song input. Play-through still has defense through performance: avoid
the mines, retain accuracy and keep earning attacks. Paid defense adds a resource
decision made by the rules on the player's behalf; it does not necessarily add
more player agency.

At 75 power, weakening still halves the attack and leaves 25; the other two
receive it in full and retain 75. That discontinuity is why costs and the
automatic spending rule need to be explained before comparing player reactions.
Do not interpret any difference as the attack pool alone being more balanced.

### The most promising defaults to investigate

**Mines only with weakening is promising if defensive spending is bounded**,
because it gives accurate play a visible defensive consequence while ensuring
that attacks still reach the field. The repeated-targeting analysis below narrows
this candidate to capped spending rather than a payment on every attack.
Scattered mines preserve the original note times and their
visibility, but can still disrupt precision through distraction and hesitation.
Neither their pressure nor the value of a smaller pattern should be assumed
negligible just because the player avoids every mine.

**Mines only with play-through is the essential comparison and may be the better
default.** It makes a simpler promise: fill the bar to attack. Being targeted
does not automatically consume charge, so a player who handles the distraction
well can still retaliate. If players cannot predict weakening payments, or
targeted players repeatedly defend without getting to send anything, favor this
version before adding a second meter or more rules. Incoming limits and recovery
remain essential; choosing play-through does not mean allowing unlimited attacks.

**Mixed attacks with weakening is the strongest varied preset.** It offers
different reading challenges with bounded exposure. It could become the default
if mine-only play feels repetitive and players find the visibility transitions
fair and enjoyable. More attack types alone are not evidence of a better match.
Mines under stress remain a substantive candidate even without forced movement.

Cancellation deserves testing because entirely stopping an attack can feel good.
It is less attractive as the first default hypothesis: strong defenders may see
few hazards, while anyone just below the cost faces the full effect. Mixed
cancellation also spends the same resource whether the next effect is personally
easy or difficult. With no manual choice, that can feel wasteful. These are
predicted tradeoffs, not reasons to remove the option from configurable rooms.

### Evidence that would change the preference

Compare the two mine-only leaders first. Look for whether players understand why
their bar decreased, whether pressure leaves opportunities to retaliate, whether
distraction creates tension without obscuring the music, and whether weaker
players can recover their performance. Then add the mixed pool while keeping
the defense rule fixed. Watch for losses dominated by one visibility effect,
scroll preference or transition rather than sustained musical precision.

Compare cancellation at similar overall pressure as a second pass; simply
counting fewer mine hits would favor any mode that removes more mines. Record
original-note accuracy, visible attack exposure and outgoing opportunities as
well. Existing solo replays cannot predict the hesitation or adaptation these
effects cause. A finite-song ending remains necessary for this comparison:
perfect play can leave survivors under any of the six combinations.

## Repeated targeting and opportunities to retaliate

Follow-up thought experiment, 2026-10-27. Repeated targeting exposes a weakness
in the earlier preference for weakening: automatic defense must not be allowed
to consume every attempt to build an outgoing attack. The unchanged 50-power
payment on every affordable incoming attack is not a convincing default without
a limit on cumulative spending.

There are two distinct feedback loops. Mines may disrupt precision, reducing
earned power; this is part of their intended pressure. Automatic defense can
also consume earned power; this is a consequence of the resource rules. A
player who keeps performing adequately should not be prevented from attacking
solely by the second loop.

### A concrete power trace

Assume the target earns 40 power in each interval before an incoming attack's
defense checkpoint, starts at zero and needs 100 to launch. Each interval is
long enough to respect effect clearance and recovery. No other checkpoint in
this constructed trace finds a full bar, and an outgoing target is eligible
when needed. Power caps at 100, with excess lost as in the worked example.
Keep earned judgments identical across cases to isolate the resource rules;
real players may perform differently under different mine counts.

Compare ordinary weakening with a candidate limit: **at most one 50-power
weakening payment between the player's own attacks**. An unaffordable attempt
does not consume that allowance. Sending an admitted attack resets it.

| Incoming attack | Play-through | Weakening every time affordable | Weakening once per outgoing attack cycle |
| --- | --- | --- | --- |
| 1 | 40 retained; full mines | 40 retained; full mines | 40 retained; full mines |
| 2 | 80 retained; full mines | 30 retained; fewer mines | 30 retained; fewer mines; defense allowance used |
| 3 | Reaches 100 and attacks; 0 retained; full mines | 20 retained; fewer mines | 70 retained; full mines |
| 4 | 40 retained; full mines | 10 retained; fewer mines | Reaches 100 and attacks; 0 retained; full mines; allowance resets |
| 5 | 80 retained; full mines | 0 retained; fewer mines | 40 retained; full mines |

With unchanged performance and attack arrivals, unrestricted weakening repeats
the power cycle 40, 30, 20, 10, 0 indefinitely. The player never attacks even
though they keep earning charge. Capped weakening delays the first retaliation
by one interval rather than consuming progress indefinitely. It achieves that
by leaving subsequent attacks at full strength, not by making the target immune.
The table does not predict which version the player survives longer.

There is another ordering hazard: if every opportunity to launch also resolves
an affordable defense first, a bar capped at 100 can be repeatedly reduced below
the launch threshold. Extra charge alone does not necessarily fix this. The
spending limit should survive checkpoint timing changes; avoid treating the
symptom by raising gain until it disappears on one chart.

### Two protections with separate jobs

**Bound exposure for both rules.** Keep one incoming reservation/effect and a
recovery interval that belongs to the recipient, regardless of who attacks.
Several opponents taking turns must not bypass it. Do not allow a backlog that
spends the entire recovery period displaying the next unavoidable sequence of
warnings. Spread can favor players with less recent attack exposure, while
Retaliate and Leader still obey recipient eligibility.

After sustained focus, compare a longer protected interval or a limit on attack
exposure over a recent window. Prefer one understandable exposure rule over
several overlapping cooldowns. For a trial, two successive attack sections could
be followed by an extended clear section. Those counts are placeholders; exposure
should also account for duration and strength, since a defended volley differs
from a full one. Protection schedules should use the chart's objective earning
opportunities: a rest alone is not an opportunity to rebuild power. Do not make
protection last until the player personally earns enough, which would reward
deliberately withholding performance.

Spread alone cannot solve the final duel, where only one opponent exists. The
same exposure limits must remain workable with two players. When no recipient
is eligible, full offensive bars wait rather than bypassing protection, and
an attack consumes power only when admitted. This limits how much simultaneous
stored pressure can arrive when protection ends.

**Bound defensive spending in the weakening variant.** The once-per-attack-cycle
rule makes its price finite: after one 50-power payment, subsequent earnings
remain available toward the 100-power attack. If the player survives and keeps
earning, paid defense cannot indefinitely prevent them reaching a full bar.
Actual launch still depends on a valid target and enough song time; this is
not a guarantee of retaliation before elimination or the song ending.

An outgoing attack re-arms only the ability to pay for one weakening. It grants
no free charge or immunity. Show the automatic defense's ready/spent status
beside the bar so players understand why one volley was reduced and the next
was not. This adds one state to learn, which is a real cost compared with
play-through. A target receiving a full volley while retaining power may find
the protected offensive progress helpful or may wish it had been spent saving
their life; that preference is exactly what testing should resolve.

### What these protections preserve

Mines can still distract, cause direct hits or reduce accuracy, slowing future
attacks through actual performance. Focus can still force one defensive payment,
occupy attention and deplete life. Recovery provides an opportunity to regain
composure, not automatic healing or guaranteed success. Keep knockout bonuses,
meter wipes and charge rewards for merely being targeted out of this comparison;
they would add further incentives and feedback loops before the basic one is
understood.

A shared finite defense budget is one way to keep the existing model. Two
alternatives worth separate study are weakening based on recent accuracy without
spending the offensive bar, or always giving a full offensive bar priority over
defense. The former removes the spending trap but grants accuracy both benefits;
the latter removes the full-bar ordering problem but does not necessarily allow
the bar to fill under repeated earlier deductions. A separate shield meter
also changes both the economy and how much the player must read. None is a free
balance fix.

### Shortlist after the spending comparison

Compare **mines with play-through plus bounded exposure** against **mines with
one paid weakening per outgoing attack cycle plus the same exposure limits**.
Unrestricted repeated weakening is no longer the leading default hypothesis.
Play-through remains the simpler option, with offensive progress fully tied to
performance; capped weakening buys one smaller challenge at a bounded delay
to retaliation. Do not declare the latter better until players understand its
automatic spending and still find full subsequent volleys manageable.

Measure time and earned power before the first retaliation under focus, frequency
of eliminated players never getting to send, actual accuracy during and after
effects, and whether power loss is understood. Run a final-duel case as well as
many-player focus. Distinguish players who could not earn enough power because
they struggled with the chart from players whose earned power was repeatedly
consumed by the rules. Structural arithmetic can expose the latter; human play
is required to judge the former and the resulting experience.

## Weakening from recent accuracy without spending power

The next candidate, proposed by the maintainer, separates the offensive meter
from automatic weakening. **Recent accuracy determines how much an incoming
attack is reduced; defense never consumes attack power.** This removes the
resource-spending trap and the once-per-attack-cycle allowance. It is a simpler
player-facing rule, though its accuracy window and strength curve still need
careful definition.

Keep the previous example's identical judgment stream: 40 power earned in each
interval produces retained power of 40, 80 and then an attack, just as in
play-through. Being targeted does not change that arithmetic. Actual earnings
can still fall when mines disrupt precision, so the rule preserves offensive
progress without guaranteeing a counterattack before elimination.

### A bounded version to compare

The player's original-chart judgments update a recent-performance estimate.
When an incoming attack is committed, sample that estimate once to determine its
strength. For an illustrative eight-mine volley, weaker recent performance might
leave all eight, better performance six and excellent performance four. These
are example outputs of a gradual bounded curve, not three chosen accuracy tiers
or an established conversion from mine count to difficulty. Maximum weakening
is partial, so perfect accuracy does not imply immunity.

Decide the mine subset before it becomes visible and keep it fixed. Subsequent
judgments influence later attacks, not mines the player is already reading.
The same principle applies to the affected section of a visibility attack.
There is a delay between the accuracy sample and the effect arriving; the warning
should state the committed result, rather than imply that its strength continues
to follow the live accuracy display.

The player-facing explanation can be: “Accurate play charges your attacks and
softens incoming ones.” Keep one offensive bar. An incoming warning such as
“8 mines → 5” can show weakening without a second spendable meter or a
defense-ready state. There is no defensive price, reserve or rearming rule to
track.

### Defining recent performance

Use required original-chart judgments, with misses included. Do not award
defense for avoiding injected mines or dilute the measure with their count;
keep mine-hit penalties in life for this initial comparison. Handle holds once
according to a declared judgment policy, rather than letting repeated hold ticks
dominate the estimate. Existing chart accuracy can remain the result statistic;
the recent estimate is a separate indicator of current performance.

A window over recent required judgments is a useful first hypothesis: it has
consistent sample size and naturally pauses during rests. It lasts fewer seconds
in dense passages and longer in sparse ones. A time-based window is an alternative
but needs a minimum-evidence rule. Neither is selected here. At the beginning,
use a shared baseline and blend toward observed performance as evidence arrives;
do not treat no notes, missing reports or a single hit as perfect accuracy.
Silence should not repair accuracy by aging all mistakes out without new play.

### The remaining risk is a performance spiral

Accuracy now provides two benefits: faster offense and stronger protection. A
strong player can benefit on both sides; a struggling player can take harder
attacks while charging more slowly. Under focus, one disrupted phrase could
otherwise reduce defense, making the next phrase even harder. Removing the
spending trap does not remove that feedback loop.

Compare modest maximum weakening, a gradual accuracy-to-strength curve and
enough history that one miss cannot collapse protection. Preserve recipient-wide
exposure limits and meaningful recovery intervals across all skill levels.
Recovery is the opportunity to demonstrate better play and rebuild the recent
estimate; weaker players do not need to qualify for the basic warning or recovery
protections. Avoid extra meter wipes, combo multipliers or mine-hit reductions
to defense in the first comparison, which would compound the same failure.

The initial sample-size and curve choices should be tested on the same chart
and rules, not silently normalized against each player's personal best. Separate
per-player normalization would change the meaning of accuracy-based competition.
Constant weakening, independent of accuracy, is a useful diagnostic comparison
if we need to separate the benefit of fewer mines from the effect of the
performance feedback itself.

### Current default shortlist

**Mines-only with bounded weakening from recent accuracy** is now the preferred
weakening candidate to compare with **mines-only play-through**. It preserves the
bar's single purpose and avoids repeated defensive deductions without adding a
per-cycle spending state. The earlier spending variants remain options for
study, not requirements of weakening.

Prefer this new candidate if players understand the warning, retain opportunities
to attack and find the defensive response rewarding. Prefer play-through if the
accuracy feedback amplifies skill gaps or creates extended downward spirals.
The unresolved issue has moved from automatic resource allocation to how strongly
current performance should affect incoming difficulty. No default is selected
before observing that interaction in human play.

The [first simulation comparison](multiplayer-simulations.md) defines a provisional
curve, shared conditions, six scenarios and decision criteria for this shortlist.
It uses the maintainer's planned skill-tunable, mine-sensitive bots to examine
retaliation, recovery, pressure and skill advantage before human validation.

## Terminology for rooms and multi-song competition

Follow-up, 2026-11-08. The maintainer wants a persistent room containing repeated
finite battle sessions, each with one or more songs and an overall result. Four
songs is the proposed default, not a claim about another game's default length.

The reference vocabulary does not provide one universal hierarchy:

- **DDR:** Konami's guides use Final Stage and Extra Stage, with the latter
  described as an additional song. Stage therefore has a direct dance-game
  precedent. [DDR WORLD guide](https://p.eagate.573.jp/game/ddr/ddrworld/howto/how_ea_pass.html?id=pass).
- **StepMania and ITGmania:** their fallback English catalogs use stages,
  SongsPerPlay, course selection and Nonstop/Oni/Survival course categories.
  These support stage as familiar vocabulary but make course an already loaded
  name for a general multiplayer contest. This is a vocabulary comparison, not
  a claim that every theme presents the same labels.
  [StepMania catalog](https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/Themes/_fallback/Languages/en.ini),
  [ITGmania catalog](https://raw.githubusercontent.com/itgmania/itgmania/master/Themes/_fallback/Languages/en.ini).
- **TETR.IO:** the official mechanics FAQ explicitly calls configurable spaces
  rooms and their controllers hosts. This supports Room for the persistent
  multiplayer container; the FAQ's historical preset details are not needed for
  this naming choice. [Mechanics FAQ](https://tetrio.github.io/faq/mechanics.html).
- **Tetris 99 and Fall Guys:** Nintendo uses rounds in its Tetris 99 tips;
  Fall Guys describes Shows containing Rounds and refers to private lobbies.
  Round is a plausible cross-genre alternative to Stage, while Show is more
  closely tied to Fall Guys' presentation.
  [Nintendo tips](https://www.nintendo.com/en-gb/News/2019/March/Boost-your-block-dropping-abilities-with-our-TETRIS-99-tips--1526705.html),
  [Fall Guys Tool Up update](https://www.fallguys.com/news/fall-guys-tool-up-update?lang=en-US).

The proposed vocabulary for this game is **Room → Match → Stage**. Room persists;
Match is the user's battle session with overall standings; Stage is one song's
competition. Lobby describes the waiting/preparation view of a room, without
introducing a fourth entity. Match is our choice for a finite contest, not a
claim that every reference uses it for an identical structure. Song remains the
plain label when selecting content or specifying Songs per match. Battle names
the mode, and playlist names the chosen sequence of songs.

The current [design](../plans/multiplayer.md) uses those names consistently.
Earlier round/session wording in the research remains historical. This naming
separates Stage won from Match won and makes Final stage refer to the last song,
not the last two survivors. Avoid adding Course, Set or Show as competing names
for the same match.

## Further reference work

The highest-value next study is **TETR.IO room setup, Taiko automatic attacks and
Puyo defense**, followed by pop'n's separation of interference from score play.
Those references address the maintainer's main questions more directly than
collecting a long list of games with multiplayer labels.

Use a small comparison matrix initially: mines alone versus mines plus
visibility, crossed with play-through, weakening and cancellation defense. Keep
targeting, chart, charge curve and ending fixed. Then vary targeting independently.
This lets community experimentation reveal which rule caused a difference,
without prematurely selecting a design or requiring every host to balance the
game from scratch.

[tetrio]: https://tetr.io/
[tetrio-faq]: https://tetrio.github.io/faq/mechanics.html
[taiko]: https://switch.taiko-ch.net/mode/donkatsu.php
[popn7]: https://www.konami.com/amusement/products/am_popn_7/
[popn]: https://p.eagate.573.jp/game/popn/usaneko/p/howto/more.html
[puyo]: https://games.sega.com/3dclassics/puyo2.html
[t99]: https://www.nintendo.com/en-gb/News/2019/March/Boost-your-block-dropping-abilities-with-our-TETRIS-99-tips--1526705.html
[lumines]: https://lumines.game/about-the-game/
[tec]: https://www.tetriseffect.game/2021/12/02/the-tectips-collection/
