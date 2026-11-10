# Placement points across a multiplayer match

Exploration, 2026-11-08. The [battle design](../plans/multiplayer.md) proposes
finite matches of four stages by default, everyone returning each song and an
overall ranking after the final song. This comparison asks how much winning an
individual stage should outweigh consistently placing well. The tables below
are design candidates and their totals are arithmetic, not simulated outcomes.

## What points should reward

Each stage contributes placement points to a cumulative total. Give stages equal
weight: no doubled final stage, discarded worst result or averaging over stages
played in the initial preset. A missed stage contributes zero, including stages
before a late entrant joined. The normal stage reset gives everyone the same
starting battle resources; a points lead never grants stronger attacks or defense.

Winning should provide a noticeable gain over second, and improving a lower
placement should still earn something. A positive last-place award distinguishes
competing and being eliminated from missing the stage entirely. A player who
has already been eliminated keeps their earned result if they leave to wait for
the next song. Departing while still competing is a forfeit; the scoring proposal
awards no stage points for that forfeit, avoiding an incentive to quit to secure
a guaranteed participation award. Already confirmed stage wins remain final.

## Three eight-player tables

| Place | Linear | Moderate podium bonuses | Strong victory bonus |
| --- | ---: | ---: | ---: |
| 1st | 8 | 12 | 20 |
| 2nd | 7 | 9 | 10 |
| 3rd | 6 | 7 | 7 |
| 4th | 5 | 5 | 5 |
| 5th | 4 | 4 | 4 |
| 6th | 3 | 3 | 3 |
| 7th | 2 | 2 | 2 |
| 8th | 1 | 1 | 1 |
| Did not play / forfeited | 0 | 0 | 0 |

**Linear:** every place gained is worth one point. This rewards average placement
and is especially easy to understand, but reaching first has no greater value
than moving from eighth to seventh. Even three wins can lose to consistent
second places if the remaining result is bad enough.

**Moderate podium bonuses:** add four points for first, two for second and one
for third to the linear table. The jump from second to first is three points,
the same as climbing from eighth to fifth. There is a reason to pursue a win
without making the rest of the field irrelevant. Second-to-third and
third-to-fourth gaps are two; lower gaps remain one.

**Strong victory bonus:** first is worth twice second. This makes winning the
central objective of each stage, and a player can tolerate very poor other
results if they win enough. It can make targeting the match leader more important
than improving one's own survival position. That behavioral effect needs human
testing; the arithmetic alone does not prove how people will target.

## Four-stage results

These are hypothetical performance profiles to compare, not the roster or
standings of one match. Their listed placement order is irrelevant to the total.

| Performance across four stages | Linear | Moderate | Strong victory bonus |
| --- | ---: | ---: | ---: |
| Three wins and one eighth | 25 | 37 | 61 |
| Two wins and two fifths | 24 | 32 | 48 |
| Two wins and two eighths | 18 | 26 | 42 |
| Four second places | 28 | 36 | 40 |
| Four third places | 24 | 28 | 28 |
| One win and three missed stages | 8 | 12 | 20 |

The moderate table is the leading candidate for discussion:

- Four thirds beat two wins followed by two eighths, 28–26: consistently good
  survival can defeat an erratic winner.
- Two wins with two fifths beat four thirds, 32–28: repeated wins are rewarded
  when the other stages are respectable.
- Four seconds beat two wins with two fifths, 36–32: always reaching the final
  positions remains a strong match strategy.
- Three wins with one eighth narrowly beat four seconds, 37–36: a sufficiently
  dominant winner can overcome one disastrous stage.

This provides a more useful design target than saying wins or consistency should
matter more in the abstract. If two wins and two eighths should beat four seconds,
the strong victory bonus is closer to that goal. If every place should have equal
value regardless of podium position, linear is the clearer choice.

Do not add separate points for remaining life, accuracy, damage, attacks sent or
knockouts in this first comparison. Life and accuracy already determine stage
placement under the current rules; another score for them changes their influence
and complicates the standings. A stage won early still awards only its placement
points, independent of how much of the song the winner plays afterward.

## Ties and incomplete participation

For a tied stage placement, average the points of the occupied positions. Two
players tied across third and fourth receive six each under the moderate table;
the next player is fifth. Two tied for first receive 10.5 each, sharing the points
for first and second. This avoids producing extra match points merely because
several players were eliminated at the same checkpoint. Keep exact values for
ranking, with an appropriate fractional display; do not round each stage's award
before summing. Larger tied groups can produce fractions other than halves.

This tie rule may make a shared stage win less rewarding than an uncontested one;
that is a deliberate proposal, not an already selected rule. It also makes the
elimination checkpoint's width significant: frequent large tied groups would
compress the points and need review alongside the checkpoint design.

For equal match totals, initially share the overall place. An alternative is
countback (more stage wins, then more seconds, and so on), but that gives victory
another advantage beyond the visible table. Choose it explicitly if desired;
do not silently decide a tied total using hidden precision or packet order.

Late entrants receive no score for missed stages, no inherited CPU score and no
compensation. For example, entering at stage three and winning both remaining
stages gives 24 points with the moderate table, not a projected four-stage total
of 48. They can still win if others earn less; late entry is a disadvantage,
not a prohibition on placing well. A reconnect resumes the same participant's
earned points rather than creating a fresh record. Show missed stages separately
from played stages that ended in elimination or a forfeit.

CPUs use the same table and can win a stage or the match. Their scores are not
discounted, transferred to a replacement human or removed to improve the human's
placement. Persistent elimination across songs has its own overall elimination
ranking in the design; these summed points are for the normal return-each-song
format.

## Smaller and larger fields

These exact tables compare full eight-player stages. A two-player duel and a
128-player field need explicit versions rather than extending the numbers
arbitrarily or giving everyone below eighth zero points. At larger sizes, compare
a smooth rank-based base award with bounded podium bonuses, using enough point
precision that improving a lower placement still matters. An equivalent formula
is only useful if players can see the resulting awards.

In particular, a fixed bonus on top of a base that grows with player count loses
its relative importance as rooms grow. Keep the intended victory-versus-
consistency tradeoff comparable across sizes, then test actual match profiles.

Before implementing configurable population sizes, decide whether the table is
based on the initial scoring roster, declared capacity or each stage's starting
population. The latter can make later stages more valuable when new players
arrive if the scale grows with population. Freeze and disclose the chosen policy
before a match; do not compensate specific late entrants or retroactively revalue
earlier results. The initial CPU-filled eight-player reference avoids this
variable for the first arithmetic comparison, but departures and custom late
entry still need their own examples before the scoring design is complete.

## Mixed skill rooms

Follow-up, 2026-11-09. The maintainer favors the moderate placement table as a
possible default and wants an additional way for players with widely different
skills to compete enjoyably. Keep 12, 9, 7, 5, 4, 3, 2, 1 as the preferred
provisional eight-player default. A mixed-skill preset can retain those same
placement points while changing the conditions under which players compete.

There are two different problems: someone may struggle to play the underlying
chart, and someone who can play it may still generate little offense and lose
life much faster than an expert. A placement bonus after elimination addresses
neither experience. The aim is competitive participation during the song, not
only a closer total on the results screen.

### Three approaches worth comparing

| Approach | What changes | Useful range | Main limitation |
| --- | --- | --- | --- |
| Fixed handicaps on a shared chart | Declared assistance levels adjust ordinary-note damage and earned attack charge | Players who can all handle the same chart, but with different accuracy | Cannot make an unreadable or physically unplayable pattern manageable |
| Different charts of the same song | Each player uses an appropriate chart; battle resource rates are normalized for differing note opportunities | Much wider skill gaps | Different density, rests and mine-placement opportunities require their own battle rules |
| Balanced teams | Distribute skill across teams and define a shared match result | Social rooms where collaboration is part of the appeal | A close team result alone does not give a weaker player meaningful personal agency |

**Fixed handicaps** are the smallest experiment. For example, a novice can lose
less life from ordinary chart errors and earn somewhat more power from the good
judgments they do make. Keep judged timing, note placement and raw accuracy
unchanged, and keep direct mine penalties comparable in the first comparison.
Expose a few understandable assistance levels rather than many independent
sliders. Pick them before the match and keep them fixed; do not grant a hidden
bonus because someone is losing this stage or leading the standings. Actual
mistakes still matter, and missing everything does not earn charge. The existing
absolute-accuracy weakening rule may still favor experts, so this is a limited
first experiment rather than a complete answer to a very wide skill gap.

**Different charts** are the more promising long-term route for a genuinely wide
range. A beginner and expert can share the song while each gets a chart that
requires attention. This must be a separately advertised preset: the standard
same-chart competition retains its existing meaning. Participating charts must
align to the same music timeline, and the host/player choices must fit the
room's agreed difficulty policy; choosing an arbitrarily easy chart cannot be
assumed fair just because everyone chose for themselves.

Dense charts must not automatically generate more attacks just because they
contain more notes, nor exhaust life faster solely through more error
opportunities. Normalize resource earning and ordinary-note damage against the
available chart opportunities in comparable sections, without awarding free
charge during rests. Receiving pressure also needs comparison: scarce safe mine
positions cannot silently make a hard chart immune, and a sparse beginner chart
should not receive an overwhelming hazard density. These are requirements to
design and test, not an already established normalization formula.

An optional further step is to compare performance against a stable, previously
established expectation for that player's chart/skill profile when computing
charge and weakening. This changes the contest toward how well each person
performs at their own challenge. It should use representative prior play rather
than one personal-best score or an estimate that moves during the current match.
A simple percentage ratio is not sufficient: timing and error distributions
near the top of the scale can differ substantially from those at lower levels.
Players without useful history need a declared provisional setting, not a claim
of accurately measured skill. Friend rooms can begin with host-agreed difficulty
and assistance; automatic calibration and resistance to deliberately understated
skill are later work.

**Teams** are a separate format worth retaining, rather than a prerequisite for
handicaps. A team design would still have to ensure that a weaker member can
attack and contribute while playing; assigning them to an expert's team does
not by itself fix early elimination or prolonged spectating.

### Earlier progression and its limitation

The earlier proposal was to begin with declared fixed handicaps on charts
everyone can play, then study per-player charts for larger skill gaps. The
maintainer identified an important launch constraint: neither players nor the
server will necessarily know their abilities, despite transferable experience
from other rhythm games. Manual handicaps, chart selection and balanced teams
therefore do not meet the desired requirement by themselves. Retain them as
custom-room possibilities; the exploration below supersedes that recommended
progression for the mixed-skill candidate. The standard same-chart preset remains
separate.

Adjustment rules must be declared before play, apply consistently to humans and
CPUs, and remain separate from late-entry policy. A late entrant still earns
zero for missed stages. No catch-up points, discarded results or hidden changes
based on match position are introduced. Personal timing judgments remain honest,
with adjusted battle resources presented as part of the selected preset.

Compare time alive, opportunities to attack, mine disruption and how often each
skill band can win a stage or match. Seek meaningful participation and plausible
upsets, not a guaranteed equal win rate or an automatic win for whichever player
exceeds an uncertain estimate. Human tests must establish whether both stronger
and weaker players understand the adjustments and still feel their play matters.
Announcer events retain their factual meanings; an assisted player really wins
the stage or match under its declared rules, with no patronizing or misleading
separate victory category.

### Intrinsic balancing without prior skill knowledge

Follow-up, 2026-11-09. Study an **adaptive battle** preset: discover manageable
challenge from the current stage, constrain the advantage of surplus accuracy
in the battle economy, and keep damage and elimination meaningful. This is a
proposal, not a selected or validated ruleset. It needs no prior rating or
self-assessed handicap, but necessarily needs some observations before it can
adapt. It cannot identify everyone's appropriate challenge on the first note.

**An adaptive chart ladder.** Use existing charts aligned to the same song and
playback rate. Begin with a short, shared protected opening in which players
cannot be eliminated or attack one another, and battle charge does not accumulate
into a head start. Start conservatively; sustained comfortable play moves a
player toward a harder chart, while sustained difficulty moves them down. Test
faster upward exploration during the opening and slower changes during battle.
The opening must be short enough to remain enjoyable across a four-stage match;
its duration and how to retain useful observations across songs remain open.

A challenge meter should distinguish manageable play from repeated breakdown,
not demand near-perfect timing. Use separate promotion/demotion thresholds and
enough evidence to avoid switching back and forth after a single error. A short
sample of an easy phrase is not proof that the player can handle a later stream,
so adaptation must remain available after the opening. Chart ratings alone do
not establish a uniform ladder: skill demands vary by phrase and pattern.

Switch only future notes at validated musical handoffs with sufficient visual
lead. Already visible notes and active holds must remain coherent. Charts are
not necessarily nested versions of one another; safe handoffs, abrupt density
changes and crossings must be checked. A song with insufficient chart range or
no suitable transitions cannot promise this experience. Automatic generation of
additional chart tiers is separate research, not an assumed capability.

**Bound the advantage in attack earning.** Reward sustained competent play with
most of the available charge; reserve a small, bounded extra benefit for higher
accuracy. Normalize against opportunities within comparable musical sections,
rather than paying a fixed amount per note. A denser chart should not grant
several times the firepower, and rests should not grant free charge. This is an
accuracy-dependent plateau, not automatic attacks independent of performance.
Exact thresholds and rates need testing. Personal accuracy results remain raw.

For the first adaptive experiment, use play-through defense with the existing
incoming-pressure cap and recovery opportunities. Adding raw-accuracy weakening
immediately would reward experts a second time and make it harder to identify
whether adaptation works. Compare a bounded weakening benefit afterwards; this
does not replace the standard preset's preferred weakening candidate. Safe mine
capacity and cognitive disruption still differ between charts, so resource-rate
normalization alone cannot establish equal attack pressure.

**Separate challenge adjustment from battle consequences.** Moving to an easier
chart does not heal battle life, refund power, or erase committed mines. Attack
disruption must still matter. However, a controller that interprets every
attack-induced mistake as excessive base difficulty can make attacks defeat
themselves by lowering the target's challenge. Prefer ordinary-play observations
over transient attack errors, and test recovery windows as opportunities to
reassess. Do not simply freeze adaptation while targeted: sustained focus could
otherwise trap a player on a chart they cannot handle. How to obtain enough
clean evidence, and how to permit an urgent downshift safely, remain open design
questions. No existing committed hazard may become unsafe after a transition.

This produces a plausible experience: a beginner settles on a sparse chart and
an experienced player climbs toward a demanding one; both can earn attacks by
maintaining control. Neither needs to declare a skill level. The expert retains
an advantage in timing, recovery and mine reading, but should not also receive
unbounded firepower from note count and perfect accuracy. These are intended
effects, not measured results.

**A more forgiving alternative: knockdowns instead of immediate elimination.**
If staying involved matters more than strict survival, test a separate format
where reaching zero life costs a limited stock and returns the player at a safe
phrase boundary with brief protection. Chart adaptation can occur at that
handoff. More than one mistake sequence is then needed to end participation,
without knowing anyone's skill beforehand. This changes the survival rhythm and
requires stock, ranking and protection rules; do not add it to the first adaptive
experiment automatically. Ordinary life recovery alone would not solve an
unplayable chart.

**Limits and discriminating tests.** Deliberate underperformance is a central
test, not a solved problem: an expert may sacrifice some judgments to stay on an
easy tier and exploit the attack plateau. Errors already costing life/charge and
comfortable play forcing promotion make this costly, but do not prove honest
play is best. Compare honest play against deliberate promotion avoidance. Do
not promise equal winning chances or a ranked-ready format. At the easiest and
hardest available charts, adaptation has no further range.

Start with two honest players with a wide ability gap and no history, then test
an abrupt hard phrase, repeated targeting during a necessary downshift, sparse
versus dense charts, insufficient chart range and deliberate underperformance.
Measure early elimination, time spent overwhelmed or unchallenged, transition
disruption, attacks earned per active musical section and whether an attack's
effect is erased by adaptation. Use human trials while the simulator develops;
the future bots need adaptation-aware behavior as well as mine sensitivity.

Announcer contexts would add adaptation-phase completion and, only if useful,
an upcoming challenge change. Provide a readable visual transition cue; avoid
constant spoken promotions/demotions. Battle danger still refers to battle life,
not the challenge meter. A stock format would separately need knockdown,
return-to-play and last-stock events. No recording lines are specified here.

## What to test next

Use the moderate table as the first candidate, with linear and the strong victory
bonus as comparisons. Evaluate the distribution of stage wins among match winners,
how often repeated high placements beat volatile winners, and how many players
are still mathematically able to win before the final stage. Also check large
stage ties and whether players understand their possible final placements from
the standings. These are scoring evaluations; they do not establish attack balance.

Keep four stages equally weighted during those comparisons. Changing match length
or doubling the final stage at the same time would obscure why a result changed.
A mathematically clinched match should not automatically cancel the remaining
scheduled songs. The announced match still completes and players can improve
their stage results and lower overall placements.

Announcer context follows confirmed standings: a stage win is not a match win;
an optional lead-change event belongs between stages after points are settled.
Only describe a match victory before the final stage if a separate, precisely
defined clinching event is adopted. The initial policy keeps the match outcome
announcement on the completed match's results, even if the leader was already
unreachable. No general comeback encouragement should assert that a player can
still win when the remaining points make that impossible.
