# Multiplayer exploration

Started 2026-10-26. **Status: research supporting a provisional design.** On
2026-11-08 the maintainer asked to move into more concrete multiplayer design
while the player simulator develops independently. The
[battle design](../plans/multiplayer.md) now proposes Room → Match → Stage:
persistent rooms run finite battle sessions (matches) of one or more songs
(stages), four songs by default as a proposal, ending with overall rankings.
Hosted, automatic public and local CPU modes share the battle mechanism.
Newcomers normally wait for the next match; optional late entrants earn no
points for missed stages and receive no compensation. Everyone returns each
song by default, with persistent elimination available to custom sessions.
Winners finish their song. The initial cap is eight participants, with 100+ as
a design target. The working stage loop includes announcer event contexts.
On 2026-11-10, implementation of CPU practice and invite-only rooms was authorized
in a separate [prototype lane](../plans/multiplayer-prototype-handoff.md).
The remaining implementation scope, architecture, balanced defaults and delivery
date remain open. The initial preference is **competitive survival
centered on accuracy**; the alternatives below remain research inputs.

The first follow-up proposes injected mines and temporary Hidden/Sudden effects,
earned through combo or a sustained power bar, with potentially automatic attacks.
That early exploration is retained below as history. The later battle plan's
working proposal is mines-only with automatic attacks and recent-accuracy
weakening that does not spend offensive power; mixed visibility attacks remain
an alternative, not part of the initial prototype scope.

A [worked eight-player match](multiplayer-match-example.md) (2026-10-27) explores
automatic attacks, targeting policies, defense paid from the power bar and
elimination. Its numbers and rules are provisional thought-experiment inputs.

The [reference and configuration study](multiplayer-references.md) (2026-10-27)
takes TETR.IO as the maintainer's main room reference, alongside rhythm and puzzle
battle precedents. The preferred direction is substantial, understandable match
configuration with a sensible default and automatic targeting and attacks during
play. Hosts and communities can explore variants; the worked match is one
candidate configuration, not a universal ruleset.

A [first simulation comparison](multiplayer-simulations.md) (2026-10-31) defines
six scenarios for mines-only play-through versus recent-accuracy weakening that
does not spend attack power. It assumes the independently developed skill-tunable,
mine-sensitive bots and specifies measurements and decision criteria; no
simulations have been run or default selected.

The central question is whether other players can create interesting pressure
while musical timing remains the reason someone wins. Tetris-style interaction
is promising, but a rhythm chart fixes the opportunities to act: players cannot
simply play more notes per second to defend themselves. The proposals below are
hypotheses to compare, not decisions.

The [placement-points comparison](multiplayer-scoring.md) (2026-11-08) explores
how stage wins and consistent high placements contribute to an overall match
result, including ties, missed stages and the limits of an eight-player table.

## The wider choice of multiplayer experiences

| Experience | What players share | What makes it attractive | Main question |
| --- | --- | --- | --- |
| Social rooms | A song, a start and results | Playing together with familiar rules | Is shared presence enough without direct interaction? |
| Accuracy competition | The same chart and rules | A clear comparison of timing skill | Live race, short rounds, teams or tournament? |
| Survival tournament | A sequence of songs or checkpoints | Rising stakes and a shrinking field | Eliminate by life, recent accuracy or relative rank? |
| Battle survival | Performance plus attacks and defense | Opponents change the match | Can interaction preserve meaningful skill comparison? |
| Cooperative survival | A shared objective or life pool | Different abilities can contribute | How to prevent one strong player carrying everyone? |
| Asynchronous challenges | A fixed challenge and recorded performances | Useful without a large concurrent audience | Enough social connection without reciprocal live attacks? |

Local multiplayer, private online rooms and public matchmaking are separate axes.
So are keyboard/pad play and participant count. A compelling eight-player battle
could establish the goal without committing to 99-player matchmaking. Public
large rooms introduce queue population, spectators, trust and ongoing service
operation as product concerns, beyond networking code.

## What the precedents actually suggest

- **Tetris 99:** clearing lines both attacks and helps fend off incoming garbage;
  targeting policies include random opponents, attackers, near knockouts and
  badge holders. Knockout badges increase attack power. The useful analogy is
  turning competent ordinary play into offense and defense, with a small number
  of strategic choices. [Nintendo's official tips][t99]
- **Tetris Effect Connected:** Zone Battle has escalating phases; later garbage
  becomes harder to manage and Zone resources become scarcer. Its Connected VS
  guidance explicitly distinguishes attacking and defending. This suggests
  studying match escalation separately from the basic attack effect.
  [Developer tips][tec]
- **PAC-MAN 99:** eating frightened ghosts sends Jammer PAC-MAN to opponents;
  targeting and strategy choices sit around the ordinary maze game. Its online
  service ended in October 2023. It is a historical comparison, not an available
  online playtest. [Official game page][pac99]
- **osu! multiplayer:** head-to-head and team competition, including accuracy as
  a win condition, provide a useful reference for the simpler competition we
  should compare against. Its tag co-op modes are documented as osu! mode only;
  they are not an osu!mania precedent. [Official wiki][osu]

These are behavior references, not evidence that their networking or balance
would suit this game. The important difference is agency: Tetris players choose
placements and pace, while a 4K VSRG player normally follows a prescribed chart.
Automatic attacks based only on accuracy could become an accuracy leaderboard
with extra animation. Conversely, too much targeting or resource management
could distract from playing the music.

## Three battle candidates to compare

### Accuracy under pressure

Everyone plays the same chart. Good performance cancels incoming pressure and
can send excess pressure onward. Uncancelled pressure makes future timing errors
more costly to survival, or temporarily reduces recovery. Notes, music and
judgment windows remain stable.

This is the closest fit to the stated preference. It raises the consequence of
mistakes rather than the chart's motor or reading difficulty. Whether that feels
like a battle is an open question. If flawless players are immune, they may all
survive; a defined end-of-song ranking, escalating shared challenge or additional
round is needed. If attacks cause unavoidable damage, targeting can overwhelm
accuracy. Neither outcome should be hidden inside a balance formula.

An alternative makes accuracy repay a debt before a checkpoint, with any unpaid
amount costing life. That is easier to communicate, but defense must be feasible
given the notes actually available before the deadline. A rest cannot silently
turn into an undefendable attack window.

### Accuracy under generated challenges

The same competition includes short, announced changes to a future chart
section. Attacks select bounded transformations, such as adding a second lane
at existing tap times. Success under the extra challenge protects survival;
ordinary chart accuracy remains separately visible.

This provides a tangible answer to “the opponent made my play harder,” without
authoring a second chart for every attack. Its unresolved cost is comparability:
two players no longer face the same task. A lower accuracy can represent a better
performance under heavier pressure. Survival rank and unmodified chart accuracy
would express different achievements.

### Accuracy elimination with limited interaction

At announced checkpoints, players below a threshold or at the bottom of a recent
performance window leave the match. A small defensive reserve or bounded attack
can influence survival without rewriting notes.

This is a useful comparison because it supplies the shrinking-field tension
without requiring disruptive modifiers. Relative elimination guarantees progress
but can eject a player who made no obvious mistake. Threshold elimination is
easier to explain but may leave many survivors. Ties, very short windows and
sparse phrases need explicit treatment in either version.

## Increasing difficulty without authored chart variations

| Mechanism | What it changes | Potential fit | Main failure mode |
| --- | --- | --- | --- |
| Greater loss for timing errors | Survival cost, not notes | Strong accuracy emphasis | Snowballing punishment; feels abstract |
| Reduced life recovery | Recovery from future play | Preserves the musical task | Passive effect, especially near the end |
| Accuracy debt | Required performance before a deadline | Legible defense objective | Impossible repayment in sparse sections |
| Chord augmentation | Add a lane at an existing note time | Keeps authored musical onsets | Excess chords, hold conflicts, pad infeasibility |
| Phrase remapping | Change lanes for a future phrase | Keeps note count and rhythm | Can make a pattern easier; breaks held notes at boundaries |
| Rhythmic echoes or subdivision fills | Add new note times | Strong feeling of incoming workload | On-grid does not mean musically appropriate |
| Temporary holds or mines | Add duration or avoidance | Uses familiar note vocabulary | Conflicting presses/releases; device-dependent burden |
| Hidden notes, covers or scroll effects | Reading and visibility | Accuracy under bounded reading pressure, or a party variant | Unequal effects on reading preferences and accessibility |
| Narrower timing windows | Judgment strictness | Direct precision challenge | Moving success criteria make accuracy hard to compare |
| Individual music speed changes | Physical tempo and audio | A distinct experimental mode | Shared musical timeline diverges |

StepMania already supplies a precedent for algorithmic note modification:
`NoteDataUtil::Wide` augments eligible rows with additional taps. That establishes
feasibility of a transform, not quality or competitive fairness across arbitrary
charts. [StepMania 5.1 source][sm-transform]

For the accuracy preference, compare survival pressure, **injected mines**,
**temporary Hidden/Sudden** and same-onset chord augmentation. Bounded visibility
attacks can test accuracy under reading pressure; the earlier suggestion that
they necessarily imply a party mode was too restrictive. Echoes are a more
ambitious musical experiment; individual music-speed attacks change the shared
musical experience more substantially.
Increasing scroll speed does not increase the number of physical actions per
second. Mirroring is not inherently harder. Neither should be priced as a
reliable amount of difficulty.

### What automatic generation would have to respect

An attack can express an amount of pressure or a transformation class; it need
not transmit a hand-authored sequence of notes. Conceptually, the recipient's
future chart section can supply candidates, which are checked against:

- Existing taps, holds, rolls, mines and lifts, including notes across the
  section boundary. No duplicate lane/time, note inside an occupied hold, forced
  mine hit or required release immediately followed by an impossible press.
- A cap on simultaneous inputs, repeated-lane speed, total local density and
  added strain. Keyboard, pad and touch need different constraints; a shared
  nominal lane count does not make their physical tasks equivalent.
- Authored timing semantics: stops, delays, warps and nonuniform scroll. A fixed
  tick gap does not establish a safe gap in seconds.
- Musical evidence. Existing onsets are the strongest readily available cue.
  A BPM grid alone does not establish swing, phrasing or whether a silent space
  should remain silent. Some imported charts have no musically useful beat grid.
- The combined result of overlapping attacks. Two individually valid additions
  can together create an impossible chord or an excessive burst.
- Sufficient advance visibility, with unchanged notes already being read and
  holds that have already begun. An “eight-beat warning” alone is insufficient
  across different tempos and scroll settings.

If no safe transformation fits, deferral, expiration, a refund or conversion to
another pressure effect are different balance choices. Dense charts must not
quietly confer immunity, and sparse charts must not receive much stronger attacks
merely because they have more empty space. Attack amount cannot simply equal
“number of notes inserted.” Conservative eligibility may mean only a subset of
the library supports generated attacks.

Precomputing eligible changes before a match is another possibility: runtime
attacks activate validated opportunities. That avoids per-song manual chart
variants, but still requires authorship and evaluation of the generator's rules.
There is no established guarantee of universally musical generated difficulty.

## Mines and temporary visibility attacks

Mines and visibility effects could provide direct interaction without adding new
required musical onsets. They test avoidance and reading while the original taps
retain their timing. Accuracy would mean maintaining precision under interference,
rather than comparing everyone on an identical task. That is compatible with the
stated preference if the interference remains readable and bounded.

### Injected mines

Keep original notes intact and add mines at selected future opportunities. This
differs from replacing taps with mines, which changes the required musical task.
StepMania's `AddMines` does both row replacement and placement after holds; it is
a behavioral reference, not an algorithm to adopt unchanged.
[StepMania 5.1 source][sm-transform]

An illustrative attack places mines in unused lanes alongside a short sequence
of existing tap rows. The player must distinguish the required presses from the
hazards. Another candidate places a mine sufficiently after a hold ends to demand
a deliberate release. The second changes release demands and needs particular
care on pads: a player may normally rest a foot on a panel between notes.

The [current judge](../../crates/engine/src/judge.rs) can hit a mine on a press
within its window or when its row is crossed with the lane held down. Therefore,
“there is no note in this cell” is not a sufficient placement rule. Check required
presses and their accepted timing ranges, held intervals, releases and neighboring
notes in actual seconds, including attack combinations. Give players enough time
to see and respond to a new release requirement.

Scattered mines are a meaningful candidate even when they require no additional
movement or release. As the maintainer points out, distinguishing hazards from
notes can disrupt reading and cause hesitation under stress. Their pressure can
come from attention and perceived danger as well as physical avoidance; prompt
keyboard releases do not remove that reading burden. Compare scattered mines
with placements that demand deliberate release rather than assuming only the
latter can be effective. Both need genuine clearance from required actions so
they do not punish otherwise valid timing. Evaluate their effect on precision
and hesitation as well as mine hits, separately for keyboard, pad and touch.
More mines need not mean proportionally more challenge.

### Temporary Hidden and Sudden

Hidden removes late visual guidance as notes approach the receptors; Sudden
shortens their visible approach. Our [appearance code](../../crates/engine/src/appearance.rs)
already expresses those effects without changing judgment times. This is a useful
foundation, but it does not supply an attack scheduler or balanced transitions.

A candidate attack announces the effect, applies it to an identified future
section and restores the player's baseline afterward. A global instantaneous
toggle could erase notes already being read. A smooth fade alone does not solve
that: the chosen activation must preserve a reasonable reading opportunity for
every affected note, including holds spanning the boundary.

Duration and strength are separate variables. Compare a mild effect lasting
several seconds against a shorter stronger effect. Define readable exposure in
time as well as field distance: the same cover position can leave much less
reaction time at a faster scroll speed. Existing lane covers, reverse and
voluntary Hidden/Sudden settings need an explicit interaction rule. Replacing
Hidden with Sudden could help a player, while stacking them could nearly erase
the field. Personal familiarity also changes the effect's difficulty.

For an initial comparison, do not combine injected mines with visibility attacks;
never let an attack's hazards become unreadable. Queue or cap repeated effects
so several attackers cannot create permanent concealment. Whether competitive
rooms constrain baseline appearance settings or adapt attacks to them remains
open, including the accessibility consequences of either choice.

### Earning and automatically sending attacks

| Trigger | Attraction | Balance question |
| --- | --- | --- |
| Every N combo | Simple milestone and visible tension | Combo can reward merely adequate timing; misses repeatedly delay the weaker player's next attack |
| A bar filled by accurate judgments | Rewards precision continuously | Raw per-note charge makes dense passages produce more attacks |
| Sustained performance above a threshold | Rewards consistency rather than isolated bursts | How are rests, sparse sections and recovery after an error handled? |
| Accuracy charge with a capped combo bonus | Both precision and streaks matter | Does the extra rule earn its complexity, or amplify the leader too much? |

On the same chart, a simple per-note meter gives everyone the same baseline
opportunities and may naturally make choruses more combative. Density is not
automatically unfair in that setting. It becomes a larger problem across
different charts or if injected objects themselves earn charge. Combo is an
imperfect proxy for accuracy, with its threshold dependent on the chosen ruleset.

One hypothesis worth comparing uses accurate original notes to fill a bar, lets
errors slow or partially reduce it, and automatically spends a fixed amount when
ready. A capped streak bonus could reward sustained precision without erasing
all progress after one miss. Rest sections should have a deliberate freeze or
decay policy rather than making silence count as bad performance.

Automatic launch keeps hands and feet on rhythm inputs. It need not mean an
instant effect: the opponent receives a warning and the attack begins at a safe
future opportunity. Target policy and attack type can be chosen before play,
rotated predictably, or influenced during breaks. Unrestricted random selection
makes competitive outcomes harder to explain.

Compare two defense models: **play through the attack**, making mines and
visibility the counterplay themselves; or **cancel queued attacks with charge**,
spending defense first and sending the remainder. Cancellation adds comeback
potential but can let equal players cancel indefinitely. Spending the same
charge on both full defense and full offense would be a different economy.
Even without cancellation, cap incoming pressure so targeting cannot overwhelm
every response. A miss should not necessarily cost life, erase the meter and
remove defense all at once.

The working loop to discuss is: accurate play earns charge, a full bar sends an
announced attack automatically, the target maintains precision while avoiding
mines or adapting to visibility, and the effect ends with room to recover.
Its important unresolved question is whether that interaction feels competitive
and musical across skill levels, not merely whether it can be programmed.

## The attack economy and fairness

**What earns pressure?** Raw combo and raw hit count reward dense sections.
One candidate evaluates accuracy over common chart segments and awards a bounded
budget per segment. A segment with no judged notes earns nothing; sparse segments
need enough evidence before awarding a maximum. Continuous timing error, discrete
judgment grades and miss/hold penalties should be compared: grade thresholds are
easy to explain, while continuous precision is more sensitive to calibration.
None of these automatically makes different difficulties equally competitive.

**What can players decide?** Compare automatic cancellation and targeting with a
choice to bank, defend or send a burst. Automatic defense keeps attention on
rhythm; explicit choices offer more strategy. Target selection can be a policy
chosen before play or changed during breaks, especially for pad users without a
free hand. Consider attacker retaliation, random targeting, leader pressure and
fixed duels; each changes the value of skill, luck and coordination.

**How much can arrive?** A bounded incoming queue, capped simultaneous effects and
some recovery opportunity could limit pile-ons. Knockout rewards create a reason
to attack but can amplify an already large skill gap. Badge-style attack growth
is therefore a variable to test, not a default to copy. Decide whether attacks
already sent by an eliminated player survive.

**Do attacked notes generate resources?** Keep added notes out of ordinary attack
earnings in the first thought experiment. Otherwise an attack may feed the
recipient, who attacks back harder, creating a resource loop. Added challenges
can have their own survival accounting. Original chart score, battle survival
and generated-challenge performance should remain distinguishable; their final
relationship is still open.

**Who competes together?** Same song, same chart, same rate and shared competitive
judgment rules are the clearest baseline. Legitimate device calibration remains
personal. Difficulty handicaps, mixed layouts and mixed input methods expand
access but need separate evidence: dividing by note count does not normalize
reading, stamina, pattern difficulty or attack opportunities. Small skill bands
improve fairness but also split the matchmaking population.

**How does a match end?** Compare one song with an explicit survivor ranking,
several rounds, and survival until one player remains. Universal accuracy-based
escalation preserves equal conditions; individual attacks create asymmetry.
Escalation and ties must not depend on which result packet arrived first. Early
elimination also needs a worthwhile next action: spectate, continue privately or
leave and find another match. Choose the intended session length before a lobby
size or “99” identity.

## Synchronization as a gameplay choice

There are three separate requirements: judging my inputs against what I hear,
agreeing when attacks take effect, and displaying other players' progress. Only
the first requires immediate local response. Remote score displays can lag;
changes to the task I must play need reliable advance notice.

| Musical arrangement | Advantage | Cost |
| --- | --- | --- |
| Same song and chart, aligned start | Comparable opportunities and shared climaxes | Everyone must have matching content ready |
| Same song, different difficulties | Friends of different abilities can join | Unequal attack/defense capacity; normalization is a research problem |
| Independent songs or independent progress | Flexible libraries and schedules | Incoming attacks land in incomparable musical contexts |
| Round or phrase exchange | Attacks can be collected and applied with ample notice | Less immediate interaction; delay must still feel like a battle |

Aligned starts do not make device audio clocks identical. Web Audio explicitly
defines its own time coordinate system and an output timestamp mapping to the
performance clock. [W3C Web Audio specification][audio] Our existing
[song clock](../../crates/engine/src/clock.rs) already maps timestamped input to
heard time. Preserve that authority; network packet arrival is not a timing
judgment, and waiting for opponents must not stall a player's music.

A promising hypothesis is to schedule effects sufficiently far into each
recipient's future and reconcile battle outcomes separately. An illustrative
120 BPM timeline, **not a selected delay budget**, shows the constraints:

| Song time | Event |
| --- | --- |
| 20.0 s | An accuracy segment finishes; the resulting attack is submitted |
| By 20.3 s | Assume delivery and match adjudication have completed |
| 24.0 s | A selected future section begins and the attack takes effect |

If its first note would become visible at 22.5 s, the content decision must be
settled before then, with margin. Thus the deadline precedes **first visibility**,
not only the hit time. A different scroll speed may expose that note earlier.
Pressure-only attacks still need a clear warning and time to defend. Durations
in seconds must be checked alongside musical boundaries; not every imported
chart has usable phrases or ordinary four-beat measures.

Late events require one agreed rule: move to a later opportunity, cancel, or
apply a defined connection-failure outcome. A receiver cannot silently choose a
different effect time. Repeated lateness cannot grant indefinite immunity.
Disconnects, suspended audio and device changes likewise need a visible policy;
rejoining between rounds is a candidate, without silently seeking music during
a judged phrase. Small drift may be tolerated within the scheduling allowance;
the acceptable bound and any correction at breaks need measurement.

Local feedback can be immediate while battle results wait for a bounded input
submission window. That distinction matters when a defense and an attack meet
at the same checkpoint: ordering and elimination must follow the agreed match
timeline, not connection speed. Avoid undoing already heard or played challenges.

Deterministic lockstep waits for required remote inputs and can stall under
jitter; buffering trades responsiveness for resilience. [Fiedler's original
article][lockstep] This makes shared per-frame lockstep a poor starting hypothesis
for uninterrupted music. It does not rule out deterministic evaluation of a
battle event log. Transport selection belongs in a later design once acceptable
interaction delay and failure behavior are known.

Online battle also needs a coordinator outside the current static site. A
player-hosted room, managed service and independently hosted server have
different trust and operating costs. Peer connectivity does not itself solve
agreement or cheating. Server re-judging can validate consistency of submitted
inputs and attack budgets; it cannot prove that a human produced the inputs or
that reported calibration is honest. Private friendly play and ranked public
competition may warrant different goals.

## What the current game provides

The menu entry is a disabled hint in
[`start.rs`](../../crates/ui/src/screens/start.rs). There is no selected multiplayer
contract behind it. Relevant foundations and limits are:

- Shared gameplay and sessions across web and desktop, with the audio-led clock.
- [Existing transforms](../../crates/engine/src/transform.rs): deterministic turns,
  seeded shuffle and cuts. They are applied when
  [`Player`](../../crates/engine/src/player.rs) constructs its judge, rather than
  providing a live stream of attack-driven chart changes.
- [Appearance options](../../crates/engine/src/appearance.rs) supply reading
  effects, not an established attack system. Parsed simfile `#ATTACKS` are also
  not applied ([known gaps](../PLAN.md#known-gaps-before-phase-9-2026-10-15-reviewed-2026-10-22)).
- [Play history and replays](../plans/play-history.md) retain inputs, options,
  seeds and results. Battle replay would additionally need the accepted attack,
  defense and timing decisions and the version of any generation rules. A seed
  by itself does not reproduce a changing algorithm or event order.
- [Chart digests](../../crates/chart/src/digest.rs) identify notes, timing and
  layout, but exclude music and use a non-cryptographic hash. They are useful for
  locating charts, not proof of matching audio or a security boundary. A battle
  would need agreed content identity, alignment and rule versions; song titles
  and local library IDs are insufficient.

Matching files on participants' machines, a freely licensed shared pool and
content distribution are separate options. The current ability to import a pack
does not imply it should be redistributed through multiplayer. A common pool
also simplifies readiness and chart eligibility for generated attacks.

## Questions to settle before a design

The next useful study is a comparison of **accuracy under pressure**, **generated
challenges** and **checkpoint elimination**, using the same imagined match.
The following can be explored through worked examples, existing chart excerpts
and recorded plays before implementing any multiplayer feature:

1. Trace a sparse phrase, a dense stream, a hold section and a stop/warp through
   each candidate. Can both the attack and its counterplay be explained simply?
2. Compare equal players, a large skill gap, coordinated targeting and a comeback.
   Does accuracy primarily determine survival? Does the attack system add a
   decision beyond playing accurately?
3. Examine generated additions on keyboard and pad charts separately. Distinguish
   invalid patterns, physically excessive patterns and musically poor patterns;
   passing structural checks is only the first of those tests.
4. Walk through delayed attacks, late defense reports, clock drift, suspension
   and simultaneous eliminations. Is the visible outcome still understandable?
5. Compare a one-song match with several short rounds, including the experience
   of the first eliminated player and the possibility of multiple perfect plays.

Useful evidence would include why players lose, how often attacks can be
defended, whether targeting overwhelms performance, and whether added challenges
feel musical. Existing solo replays can estimate a pressure-only economy; they
cannot show how someone would have responded to different notes or visible
pressure. Those require eventual human evaluation if the concept survives.

Goal discovery is complete when we can describe the audience, musical experience,
source of player interaction, basis of fairness and intended match length in a
short paragraph, along with what would make us reject the concept. Lobby sizes,
protocols, formulas and implementation milestones can follow that choice.

[t99]: https://www.nintendo.com/en-gb/News/2019/March/Boost-your-block-dropping-abilities-with-our-TETRIS-99-tips--1526705.html
[tec]: https://www.tetriseffect.game/2021/12/02/the-tectips-collection/
[pac99]: https://pacman.com/en/games/pacman99.php
[osu]: https://osu.ppy.sh/wiki/en/Client/Interface/Multiplayer
[sm-transform]: https://raw.githubusercontent.com/stepmania/stepmania/5_1-new/src/NoteDataUtil.cpp
[audio]: https://www.w3.org/TR/webaudio-1.0/#dom-audiocontext-getoutputtimestamp
[lockstep]: https://www.gafferongames.com/post/deterministic_lockstep/
