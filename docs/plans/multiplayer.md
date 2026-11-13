# Multiplayer battle design

Status: provisional experience design, 2026-11-08. The maintainer wants to move
beyond exploration while the player simulator develops independently. This draft
defines persistent rooms, finite battle matches, their stage loop and the events an announcer
could describe. The maintainer's match choices are recorded below; numerical
rules and the detailed operating policies remain proposals. It
does not select a networking stack or claim that the numerical rules are balanced.
The [research](../research/multiplayer.md) and
[simulation comparison](../research/multiplayer-simulations.md) remain the basis
for evaluating alternatives. The simulator is a later validation tool, not a
prerequisite for deciding what happens in a stage.

The [navigation and screen plan](multiplayer-ui.md) (2026-11-09) maps the menu
tree, room roles, preparation, play/watch states and results into shared screen
families, with layout sketches and announcer contexts. Its interface proposals
remain subject to review alongside the mechanism.

Update, 2026-11-10: the maintainer authorized prototyping CPU practice and
invite-only rooms in a parallel implementation lane. The
[handoff](multiplayer-prototype-handoff.md) defines that scope and separates
selected constraints from provisional values. This design lane continues the
remaining multiplayer work; the simulator and balance studies are not complete.
The [mockup archive](../../tools/multiplayer-design/README.md) preserves the reviewed
screens without claiming they implement the game or server.

## The proposed experience

An accuracy-centered survival battle: everyone plays the same chart, accurate
play fills an offensive bar, and a full bar automatically sends scattered mines
to an opponent. Recent accuracy also weakens incoming attacks without spending
that bar. Players concentrate on their notes; their targeting policy is chosen
before the song. Mines can cause direct hits and disrupt surrounding judgments
even when avoided.

Initially cap a stage at eight participants, humans and CPUs combined, while
designing for eventual rooms of 100 or more. A stage is one song; a battle match
contains one or more stages, four by default as a working proposal. Everyone in
a stage in the standard preset uses the same chart, playback rate, judgment rules and input category;
personal calibration, readable scroll speed
and skin remain personal.
The first battle preset uses four lanes and ordinary visibility. Broader layouts,
mixed chart difficulties and Hidden/Sudden attacks remain separate extensions.

A [mixed-skill preset study](../research/multiplayer-scoring.md#mixed-skill-rooms)
(2026-11-09) explores meaningful survival and retaliation across skill levels,
retaining the same placement points. The maintainer's launch constraint is that
neither players nor the server can be assumed to know their skill levels.
Manual handicaps, selected difficulties and balanced teams remain custom-room
possibilities but do not satisfy that constraint alone. The latest candidate is
an adaptive chart ladder learned during play, a protected opening and bounded
accuracy-based attack earning, initially compared with play-through defense.
Safe chart transitions, distinguishing attack disruption from base-chart
difficulty, and deliberate underperformance remain unresolved. A stock-based
knockdown format is a separate alternative. These are research proposals, not
changes to the standard preset or compensation for late entry; no adaptive
formula or implementation has been selected.

2026-11-13: a simple, non-adaptive rule is built instead, as a match rule:
**At zero life: Out / Step down** (`step-down-v0`,
`docs/plans/multiplayer-prototype-progress.md`, "Step down"). It reacts only
to life reaching zero: the competitor plays the song's next easier chart
from a bar line past their reading horizon, with half their life back, and
places below everyone who stayed on the stage's chart; on the easiest chart
zero life is out. Nothing guesses skill and nobody steps back up. The
adaptive ladder above stays research.

The working preset is **mines with accuracy-based weakening**. Keep **mines with
play-through** as a named comparison preset with otherwise identical rules.
Neither is a validated public default. There is no manual attack button or manual
target selection during play.

## Room, match and stage

Proposed player-facing vocabulary, replacing the earlier round/session wording:

| Term | Meaning | Example use |
| --- | --- | --- |
| Room | The persistent place, with its host or automatic controller, visibility, members and settings; it can run many matches | Create room, Join room, Leave room |
| Match | One finite competition, formerly called a battle session; one or more stages followed by overall rankings | Start match, Match standings, Match results |
| Stage | One song played competitively within the match | Stage 2 of 4, Final stage, Stage results |
| Lobby | The room's waiting/preparation view; not a second container or an additional competition level | Waiting in the lobby |
| Song | The musical content selected for a stage | Songs per match: 4, Next song |

Use Battle as the mode's description, not as another level between match and
stage. Do not alternate round and stage, or session and match, in player-facing
controls. A song/chart is content; a stage is a particular play of it, so the same
song could occupy two stages. In explanations, a match can still be described as
a battle session. A course is not the name of the room or the competition;
reserve playlist for the host's ordered song choices. The
[terminology comparison](../research/multiplayer-references.md#terminology-for-rooms-and-multi-song-competition)
records the reference vocabulary and why this combination is proposed.

The lifecycle is **room lobby → match → stages with intermissions → match results
→ the same room lobby**. Room membership and settings persist; match standings
and competitive eligibility begin afresh. An automatic public room repeats this
cycle, including a results interval and a new admission window, rather than
treating an endless song rotation as one unfinished match.

## Ways to play

The maintainer selected Twitch OAuth for initial online participant identity
(2026-11-09), allowing other third-party providers later without game-managed
registration/passwords. Keep a stable internal player record for membership,
host authority, results and reconnects. The [sign-in flow](multiplayer-ui.md#sign-in-and-participant-identity)
proposes authentication before online room entry, including waiting/spectating,
while local CPU practice remains independent of sign-in. Authentication does
not bypass admission or reset a player's competitive state.

The maintainer's choices (2026-11-08): a room runs multiple battle sessions, called
matches in this proposal. Each contains one or more songs, with four proposed as
the default; the host controls its length and song selection. Normally
everyone returns after each song; custom lobbies may keep players eliminated
across songs; the last survivor can finish the song. There are three ways to
organize a match, sharing the same battle mechanism:

| Way to play | Who controls it | Songs and progression | Participation |
| --- | --- | --- | --- |
| Hosted room | A human host controls settings, visibility, the song queue and when to advance | Host-chosen songs or playlist, shown before each stage; optional automatic advance | Publicly listed or private by invite link; configurable CPU seats |
| Automatic public room | The server runs successive finite matches and advances through intermissions | A known playable pool, a small set of advertised battle/difficulty presets and bounded start timers | Humans normally enter between matches; clearly identified CPUs fill remaining places at roster lock |
| Local CPU match | The local player controls a match without needing an online lobby | Chosen songs or a local playlist; adjustable CPU skill and mine sensitivity | One human with CPU opponents, for learning and practice |

**Songs per match** accepts one or more, with four as the provisional default.
The count and scoring rules are visible and locked before the first stage; four
is our pacing proposal, not a claim about DDR's standard song count. The host can
prepare a playlist or choose each remaining song during intermission. Once a
stage is readied, changing its song clears that stage's readiness. Changes to
match length or competitive rules apply to the next match; stopping the current
one early is shown as an interrupted match, not a completed four-stage result.

Here, **hosted** describes the human's controls, not a requirement that their
computer be the network authority. Online hosted and automatic matches should
share the server's battle rules. Local play should run the same rules locally,
with no lobby-service requirement once its content is available. CPU availability
depends on a usable opponent implementation; the design can advance while the
skill-tunable simulator remains in development.

Public visibility means a room appears in discovery. Private means absent from
discovery and joinable with its invite link; anyone given that link can use it
subject to capacity and the host's admission rules. Hosts can close admission
and replace an invite link. Visibility changes govern future access; removing
someone already present is a separate host action. Room ownership, control
permissions and automatic progression are distinct, as below. The end of an
ordinary match does not close the room or invalidate its invite link.

### Room control, co-hosts and server administrators

Follow-up, 2026-11-09. The maintainer wants to explore automatic progression,
optional host transfer (Off by default), co-hosts and server administrators who
implicitly have co-host powers in every room. Administrators must be able to
enter to play or just watch, particularly to help hosts during early testing.
The following permissions and timeout values are proposals, not implemented
server capabilities.

Keep three independent facts: one room owner (the Host), people authorized to
operate the room, and whether its stage loop advances automatically. None
implies that its holder is a competitor or that their device runs the server.
Named co-hosts can operate the room without taking ownership. Their grants are
scoped to this room and can be revoked by its host; persist them across matches
in that room, not across every room the same users enter.

| Action | Host | Named co-host | Server admin in this room |
| --- | --- | --- | --- |
| Inspect roster, readiness and content blockers | Yes | Yes | Yes |
| Choose unstarted songs; prepare next-match rules | Yes | Yes | Yes |
| Start, cancel readiness window, hold/resume automatic progression | Yes | Yes | Yes |
| Manage permitted competitor/CPU seats at admission boundaries | Yes | Yes | Yes |
| Grant/revoke co-host, transfer ownership, change room visibility/invite policy | Yes | No | Not implied by co-host status |
| Play or watch | Independent participation choice | Independent participation choice | Independent participation choice |

These controls obey the same frozen-stage and match-rule boundaries as the host.
Co-hosts/admins cannot silently alter a committed chart, award points, resurrect
an eliminated player or bypass content readiness. Broader server moderation
powers, if needed, are separate from the implicit co-host permission described
here. Commands are authorized and serialized by the server against current room
state; stale/conflicting edits fail with the updated state instead of producing
two starts or overwriting an unseen playlist change. Record who changed a song,
held progression or transferred ownership in a compact room activity history.

Server-admin status is assigned by the game backend to a stable authenticated
identity. A Twitch display name, Twitch channel moderator status or a client
flag never establishes it. An admin may discover and enter a private/closed
room for assistance without its invite; make this access part of the private
room's stated meaning. Upon entry, list the admin visibly with an Admin badge,
even when watching. Passive means non-playing, not invisible. The host does not
need to grant the co-host permission, and ordinary room co-host revocation
cannot remove a backend-admin capability.

Default the administrator's entry choice to Watch, with Play available. Watching
uses no competitive seat, generates no score row or readiness requirement, and
does not satisfy the minimum ready-human rule. Reserve bounded operator-observer
capacity so a full match does not prevent assistance; its limit remains a
server-capacity decision. Playing follows normal seat availability and admission
boundaries: next match by default, or next stage if late entry is enabled, with
zero points for missed stages. No live-song entry or inherited CPU score. An
admin arriving as a player may operate the room while waiting for their seat.
They may remain an observer in one room without becoming its automatic owner.

### Automatic advancement in a hosted room

Expose **Advance: Host / Automatic**, with Host as the conservative initial
default. Automatic handles the stage loop of a match after its first stage is
started explicitly. Repeating matches automatically is a separate choice;
default hosted rooms stop at match results. Server-hosted public rooms retain
their already-defined repeating match cycle.

To enable automatic advance, require the remaining playlist to be selected and
the required shared content available to the server. Clients can prefetch later
songs. A host's local file selection alone cannot promise that their disconnect
will leave the server able to distribute it. If a required upload fails or a
later song is removed, show a specific blocker and hold at the stage boundary;
do not silently substitute a random song, skip a scoring stage or label an
unfinished match complete. A co-host/admin can fix the queue through the same
automatic-sharing flow using their own local library.

Try **30 seconds between songs** as the first automatic intermission. This
includes the readiness window: a final 15-second reminder is not another timer
added after 30 seconds. Count from the previous song's natural end, after any
winner completion. Evaluation and room views show the same deadline. At its end,
commit ready, loaded competitors and sit out the others under the hosted
readiness rules. Retain the full interval even if everyone readies early.
If content or the minimum ready population is missing, hold with that reason;
do not repeatedly restart a full timer because one absent player never readies.
An operator resolves/resumes a held room with a fresh, visible readiness window.

**Hold after this stage** stops advancement at the next boundary; it never pauses
the current song. During intermission, **Hold** cancels its start window. Resume
provides a new visible readiness window rather than starting from an expired
deadline. Authorized co-hosts and admins can use these controls immediately.
Ownership never changes merely because a helper operates the room.

An AFK or disconnected host sits out competitive stages under the same rules as
any other participant. Automatic progression continues without that host when
the playlist, content and remaining population are valid. Lack of menu input
is not reliable evidence of being AFK: playing, watching and reading evaluation
are normal. Avoid an inactivity heuristic that silently transfers ownership.

### Host departure and optional transfer

Manual transfer remains available: the host nominates a successor who accepts.
Co-hosting is sufficient for routine assistance and does not require a transfer.
For an optional automatic-transfer experiment, prefer a designated co-host over
an arbitrary participant, with **Transfer on disconnect: Off** by default.
If enabled, propose a 60-second disconnect grace before offering ownership to
the nominated co-host. Require acceptance; no response leaves the owner absent
rather than assigning someone who may also be AFK. The nomination/acceptance
policy and grace duration need testing before implementation.

Reconnecting before transfer restores the existing owner. After an accepted
transfer, reconnecting restores participation/results but does not seize the
room back. Do not include visiting admins in successor selection merely because
they have implicit co-host powers. A deliberate Leave can offer Transfer and
leave, or Leave with the room's existing co-host/automatic policy in effect.

| Situation | Proposed behavior |
| --- | --- |
| Host AFK, automatic progression On | Continue prepared stages; host may sit out |
| Host AFK, manual progression, co-host/admin present | Helper starts or enables an eligible automatic sequence |
| Host disconnected during a song | Song continues; host's player slot follows ordinary disconnect rules |
| Host disconnected, automatic progression valid | Continue; ownership grace does not block stages |
| Host absent, manual progression, authorized helper present | Helper operates immediately without ownership transfer |
| Host absent, manual progression, no helper, transfer Off | Hold at boundary with Waiting for host; players can leave or an admin can assist |
| Transfer enabled, designated co-host accepts after grace | Change owner visibly; preserve room, invitation and match records |
| No ready human competitors remain | Hold; a watching admin does not keep CPU-only matches running |

Empty/ownerless room expiry and invitations to replacement co-hosts are lifecycle
work still to specify. Do not claim that these policies keep a manual room moving
when no authorized person is present. For early testing, prepared automatic
matches plus named co-hosts/admin assistance cover the common cases without
enabling automatic ownership transfer.

### Stage resets and match results

By default, elimination lasts for the current song only. At the next song,
everyone participating starts with full life, empty attack power, the shared
initial accuracy estimate and no outstanding attacks, recovery or targeting
history. No snowball advantage carries from a win. Eligibility to enter the
match is distinct from returning after elimination within it.

Stage results feed match standings, which remain separate from battle resources.
After the last song completes, show the match's overall winner or tied winners
and rankings, with each player's stage results and total. The proposed default
is to add placement points earned in each stage, with equal stage weights.
The [placement-points comparison](../research/multiplayer-scoring.md) studies
linear, moderate podium bonuses and a strong victory bonus. The leading
eight-player table, favored by the maintainer on 2026-11-09 as a possible default,
is 12, 9, 7, 5, 4, 3, 2, 1 points. It remains provisional pending playtesting.
Tied stage positions would share the average of their occupied awards;
the numerical policy for changing field sizes remains open. Missed stages award zero points,
not an estimated placement or an average based on fewer appearances. Equal
totals can share a rank in the first scoring proposal. The standings retain
departed participants' earned results; leaving and rejoining cannot erase those
results or reset a persistent elimination. CPU-filled practice results do not
imply a ranked matchmaking or rating system.

### Joining the room and joining a match

Room admission and competitive admission are separate. With the host's room
access permission, a newcomer may join the lobby or spectate while a match is
running. **Allow joining during a match** defaults Off: that player waits until
the next match, even if a stage ends or a CPU seat is available. Waiting players
remain in the room for the next admission window.

With the option On, newcomers can enter at the next stage boundary, subject to
capacity and readiness. They start with zero match points and normal fresh-stage
life, power and defense history. Earlier stages remain marked as not played and
award zero; there is no catch-up score, multiplier, averaged total, extra life,
weaker opposition or inherited CPU score. The known disadvantage is intentional.
Before entering, show the next stage number and explain that missed stages earn
no points. The results show stages played without adjusting the ranking. Staying
in the room lets them start the following match alongside everyone else.

Never admit someone midway through a song. A returning participant reconnects
to their existing identity and results under the reconnect rules; that is not a
new entrant. A human taking a CPU seat gets their own result row, while the CPU's
earlier results remain its own. Preserve an eliminated participant's identity
until the match ends, so re-entry cannot bypass elimination.

### Persistent elimination

Expose **Return after each song: On/Off** for custom matches, default On. With
it Off, an eliminated player watches the remaining songs until the match finishes.
Survivors still reset life, power and recent accuracy at each new song; preserving
elimination does not implicitly preserve every resource. For the first custom
preset, persistent elimination requires mid-match joining Off: otherwise a new
entrant could skip the elimination that defines this format. Combining those two
options needs a separate qualification rule and is not a supported initial mix.
Lock this option and the match's ending rule before its first stage:
end when one contender remains (after pending attacks settle), or at the announced
last song using the stage's survivor ranking if several remain. The winner still
finishes that song. A completed match then makes everyone eligible again. Carrying
life/power across songs would be a different, later custom rule. This format's
overall ranking follows elimination progression and the final survivor result,
rather than the default reset-each-stage placement-points proposal.

### Automatic starts and CPU seats

An automatic room should start each match with at least one ready human and
enough CPUs to reach its advertised target population; do not wait for eight humans. Use a
bounded gathering period and a separate bounded content/readiness period. Players
who cannot finish initial loading wait for the next match with a clear reason; they do not
hold an open lobby indefinitely. If no human is ready, wait for one instead of
running empty matches of CPUs. Hosted rooms may choose to wait for their party.

Before committing the first stage, freeze the match roster and fill vacant seats
with CPUs. Humans arriving later normally wait for the next match. A hosted room
with mid-match joining enabled can admit them into vacant or CPU seats between
stages. Never replace a CPU with a human, refill an eliminated seat, or silently turn a
disconnected human into a CPU midway through a song. A human can rejoin their own
slot under the stage's reconnect policy; they do not inherit someone else's play.
CPU identities are visible in the roster/results. Their skill profile and random
seed are fixed for the stage; do not secretly change skill to favor a player.

Initial CPU fill does not silently replace an existing participant who misses
a later stage's readiness deadline. That missed stage earns no points; under
the normal return-each-song rule the participant can ready for a later stage
with their own existing match record. Under persistent elimination, a stage
forfeit ends their competitive participation for that match.

A CPU sends, receives, weakens and survives attacks under the same rules as a
human, including mine sensitivity and charge generation. Human and CPU seats both
count toward capacity and survivor milestones. Automatic rooms advertise their
intended skill range; avoid using an expert CPU to fill a beginner match.
The song pool must be accessible and identified consistently for its players;
hosted rooms let the host choose from their entire local library and
automatically share committed stage songs using the existing
[shared-song service](shared-songs.md), as requested on 2026-11-09. This is not
restricted to songs already installed by every participant. Required content
must still finish sharing/loading before that stage starts. Connecting room
permissions and Twitch identity to the existing upload service is integration
work; the service already supports shared-song upload and playback.

### Hosted readiness without an indefinite wait

Proposal, 2026-11-09: keep the host's choice of when to advance, but allow Start
to open a visible readiness deadline when some eligible players are unready.
Try 15 seconds, then the ordinary synchronized countdown. If everyone is already
ready and loaded, Start goes directly to that countdown. An active readiness
window does not shorten when someone readies. The host can wait instead or
cancel; individual players cannot extend it by toggling readiness.

Enable this once the chosen song is shared/available and a valid ready population
exists: provisionally at least one human and two competitors including CPUs.
A playing host must ready themselves; a spectating host need not. A single other
player's download, disconnect or unready state does not block this action. Show
who would currently sit out. Changing the upcoming song cancels the window and
clears that stage's readiness; starting a new window is an explicit host action.

At the deadline, commit only eligible, authenticated, ready and loaded players.
Others sit out this stage with zero points, retain earlier results and stay in
the room. No kick, score reset or catch-up compensation. Previously admitted
participants can return at a later stage under the normal return-each-song rule,
even when newcomer mid-match joining is Off. Persistent elimination keeps its
existing rule: a missed stage ends that player's match. Do not replace missed
seats with fresh CPUs between stages. Initial match CPU fill still applies;
an unready person excluded from the initial match roster waits as a newcomer,
not as an already-admitted participant entitled to return next stage.

If the final roster is below the announced minimum, return to preparation with
that reason. Otherwise exclude unavailable people without repeatedly reopening
the same readiness window. Shared-song failure and an absent host are separate
blockers; automatic advance or host transfer addresses host AFK. The population
basis for placement points with sit-outs remains an explicit scoring decision;
do not assume eight actual starters.

Results inspection neither readies a player nor blocks the deadline. Keep Ready
and the timer reachable in evaluation; ready players may keep reading until the
committed start. No activity/mouse-motion heuristic substitutes for consent to
play. This supersedes all-ready-only hosted starts while preserving the rule
that actual starters must have loaded the required content.

## From one song to the next

1. **Configure and ready.** The host or automatic controller chooses the song/chart
   and preset. Players
   see the survival rule, attack type, defense and their targeting policy before
   readying. A change to competitive settings clears readiness. All admitted
   players must have matching playable content and finish essential loading before a start is
   scheduled. Announcer recordings are optional and never delay readiness.
2. **Start together.** Agree on a future start and show a countdown. The stage
   freezes its configuration and participant list; later arrivals spectate. A
   roster-invalidating failure before the committed start cancels that start.
   Publish the revised roster and a new start; the automatic mode uses its
   readiness deadline instead of repeatedly waiting for that seat. CPU fill
   follows the match-admission rules above, not an unconditional refill each song.
3. **Play and exchange attacks.** Each player's judged original notes update
   life, offensive power and recent accuracy. At shared battle checkpoints,
   ready bars attempt automatic launches. All notes keep their musical timing.
4. **Eliminate and spectate.** Zero life ends competitive participation. The
   player can follow the remaining battle or leave; their placement remains.
   Their normal return is the next song. Finishing noncompetitively after an
   elimination remains an option to study, especially if early elimination
   otherwise leaves a long wait.
5. **Confirm the winner and finish the song.** A final result can be confirmed
   before the music ends. The last survivor keeps playing on the same timeline;
   the room does not cut their song off. Other players can watch, review their
   result or see the upcoming song. The next stage waits for the current song's
   natural end; early victory does not let an automatic host skip the remainder.
6. **Intermission and next stage.** Show stage placement, the ending reason,
   performance, attack statistics and running match standings. Preview the
   next chart. Waiting newcomers enter only if mid-match joining is enabled;
   otherwise they remain spectators until the next match. Hosted matches
   advance when the host starts the ready roster, or by an opted-in timer;
   automatic matches use a visible intermission countdown. Players may sit out
   or leave. Local practice lets its player advance; repeating a scored stage
   starts a separate practice attempt rather than adding another scoring chance
   to the same match.
7. **Match results and another match.** After the final stage's song completes,
   show overall rankings and the winner or tied winners. Keep the room, host and
   invite link; clear competitive state only for the next match. Admit waiting
   players into available seats or replace CPU seats before the next roster locks.
   The host starts the next match; an automatic room uses a visible results and
   gathering period before starting its next finite match.

## The three performance quantities

| Quantity | What changes it | What it does |
| --- | --- | --- |
| Life | Original-chart errors and mine hits reduce it | Reaching zero eliminates the player |
| Attack power | Accurate original-chart judgments earn it; an admitted launch spends it | A full bar automatically attempts an attack |
| Recent accuracy | A bounded history of required original-chart judgments, including misses | Sets incoming mine reduction when that attack is committed |

Keep these roles distinct. A miss does not wipe saved power; mine avoidance does
not generate power or accuracy samples; receiving an attack has no direct life
cost. A mine hit costs life but is not an additional reduction to the defense
estimate. Its indirect effect on nearby required notes still matters.

The numerical starting point is the simulation study's: 100 life, 100-power
attacks, precise/good hits earning 2/1 power, loose hits and misses earning none,
and loose/missed/mine-hit penalties of 1/8/8 life. Use its 32-judgment estimate
with a shared opening prior and a maximum 50% reduction. These are rehearsal and
comparison inputs, not an established mapping to the game's judgment presets.
Hold, lift and authored-mine accounting must be specified before broadening the
initial taps-only test charts. Chart density changes the pace of both earning
and damage even when everyone has the same opportunities; judge that across
charts rather than treating a single song as balanced.

The defense estimate pauses during rests and changes gradually with new play.
In the standard preset, do not give one bot or player a private accuracy scale
based on their personal best. The separate mixed-skill study considers declared
skill adjustments. The score used for final ranking stays separate from this
short history.

## One attack from earning to recovery

**Ready to send.** Reaching full power marks the bar ready. At the next four-beat
checkpoint it attempts to send. If no opponent or safe future section is eligible,
the full bar waits; it does not repeatedly spend, create an attack backlog or
announce launches that did not happen. Further earnings while capped are lost
in this initial rule, rather than banked as multiple attacks.

**Choose an eligible opponent.** Each recipient can have only one reserved or
active attack, followed by a protected recovery interval. Eligibility belongs
to the recipient, so several opponents cannot take turns bypassing it. Within
that constraint the available pre-song policies are:

| Policy | Selection | Role |
| --- | --- | --- |
| Spread | Fewest admitted incoming attacks this stage; seeded rotation among ties | Working default; distributes encounters |
| Retaliate | Most recent incoming attack's sender if eligible; otherwise Spread | Automatic rivalry without requiring a button |
| Leader | Eligible opponent with highest life, then original-chart accuracy; seeded ties | Alternative for applying pressure to stronger survivors |

At a checkpoint, finalize prior eliminations first. Resolve launch attempts in
rotating player order, reserving recipients as attacks are admitted. An
unsuccessful attacker can try another eligible opponent before retaining its
power. Use the finalized pre-launch state for life and accuracy comparisons;
an attack accepted in this batch changes Retaliate's memory for the next batch.
This avoids packet order or an arbitrary per-player update sequence determining
instant retaliations. The rotation itself still needs fairness tests.

**Commit and warn.** Admission spends the 100 power and fixes the sender,
recipient, affected future section and mine pattern. At that same moment,
snapshot the recipient's recent accuracy and remove the corresponding subset
from the full pattern. Show the incoming warning and final strength immediately.
This simplifies the older worked stage: there is no later spending/defense
checkpoint. Better play during this warning helps future attacks, not the mine
pattern already announced.

**Deliver the pattern.** Start with eight scattered mines over eight beats,
reduced to between four and eight in the weakening preset. The retained mines
are a deterministic subset of the full pattern. Commit before any affected mine
could first become visible, with a shared minimum warning lead and enough reading
time for the permitted player settings. Never insert hazards into already-read
content. Safe placement must respect press/release windows and held lanes; an
empty chart cell alone is not proof of safety. If a full safe pattern cannot be
scheduled within the allowed horizon and song end, no launch is admitted.

**Recover.** Keep the entire eight-beat section reserved even when weakened.
After the affected content clears, protect the recipient for at least four beats
and eight subsequent original judgment opportunities, whichever finishes later.
This provisional rule prevents a rest consuming all recovery. It does not wait
for the player to earn a particular amount of power, which would reward holding
back. Mines still matter through performance loss and life depletion, but cannot
overlap or form a queue waiting to consume the recovery interval.

This is the full loop in the duel too. Attackers may wait at full power while
their only opponent recovers. The duel does not silently enable faster attacks,
disable defense or introduce sudden death.

## Elimination and a truthful result

Original-note failures and mine hits are judged locally against heard song time;
the shared stage state confirms eliminations and results. The announcer and
opponent displays use confirmed events, not guesses from a delayed life bar.

An eliminated player cannot launch again. Already committed attacks remain;
those aimed at someone already eliminated expire without a refund or retarget.
One surviving player is not yet a confirmed winner while outstanding attacks
can still cause a mutual knockout. Resolve their remaining committed hazards
before finalizing the result. Ordinary note judgments and damage
continue while those attacks are pending; being the last live player grants no
temporary immunity. If the last survivor falls during that unresolved interval,
the contest is drawn among the final contenders. Simultaneous last eliminations
also produce a draw, rather than a packet-arrival winner. The
[ending study](multiplayer-ui.md#concrete-ending-study) now supplies a scripted
Sol/Kai example with survival and final-knockout branches. The settlement rule
remains provisional: the authoritative checkpoint and exact final-contender
cohort still need agreement before implementation.

Once victory is confirmed, lock the competitive result and let the winner finish
the song without interruption. Further misses or authored-mine hits can affect
their displayed chart-performance statistics but cannot revoke the win or stop
the completion through battle failure. No new attacks launch. Keep the frozen
competitive result distinct from the remaining chart-performance tally; do not
feed the extra solo portion back into placements or match points. Leaving this
optional completion after a confirmed win also cannot turn that win into a
forfeit. An announcer's victory event belongs to confirmation, not to the last
note; song completion is a separate event and should not repeat the victory.

If multiple players reach the song's competitive end, rank survivors by life,
then whole-play original-chart accuracy. Exact ties share the place. Eliminated
players rank below survivors by elimination checkpoint, with simultaneous
eliminations tied. This is a bounded survival battle; it can end with a winner
without everyone else being eliminated. It does not guarantee a last-player-
standing finish. The results must say which ending occurred.

Do not award a knockout to whichever player most recently attacked someone:
several attacks and ordinary chart mistakes may have contributed. Record launches,
received attacks, weakening and mine hits without inventing kill credit. No
knockout bonuses or kill-streak announcements belong to this first design.

Voluntary departure while still competing is a forfeit. A connection loss first
becomes a visible connection state; any grace period and forfeit deadline are shared stage
rules, and returning cannot clear already committed attacks. A failed stage
authority ends the stage as interrupted, not as a fabricated win. Exact timeout
values and reconnection behavior remain design work before an online prototype.

The [connection-loss study](multiplayer-ui.md#connection-loss-and-return-study)
now separates a continuous local play with an unresolved network gap, an
authoritatively accepted same-stage return, and a confirmed disconnect forfeit
followed by watching or next-stage entry. Its proposed forfeit earns zero while
preserving earlier points; an already confirmed elimination or stage win cannot
be reset by reconnecting. Reopening the client or reestablishing an ended
authenticated session does not by itself resume the competitive song. These are
reviewable UI outcomes, not an accepted grace duration or synchronization
algorithm. Fair recovery of attack history and judging must be established before
the design promises mid-stage resumption; otherwise use room/watch recovery only.

## Eight first and larger parties later

Eight is the initial operational cap, counting CPU and human participants. The
design target is 100+ participants, with 128 a useful future test size. It is
not a claim that the current game or infrastructure can run that load. Capacity,
participant identities, placement, spectator lists and match state must not
encode an eight-seat assumption.

The battle needs to scale in pressure as well as computation. At 128 players,
one recipient still has one attack slot and the same protected recovery. More
opponents must not multiply incoming mine density or bypass protection. Attack
arbitration needs bounded work over eligible recipients and fair service to
waiting senders; do not scan every player against every other player each frame
or reward whichever request reaches the server first. Track capped-bar waiting
time as well as attacks received: a large room can be survivable yet frustrating
if many accurate players rarely get to send.

Keep these larger-room constraints in the later implementation design:

- **Control and authority:** a human host's room controls are separate from the
  online authority that admits attacks, orders eliminations and closes stages.
  New competitors normally enter between matches; the custom mid-match option
  admits them only between stages, even in large rooms.
- **Information:** deliver exact local attack events and bounded opponent-state
  updates. Do not require every client to render or simulate 127 full charts to
  know who is alive. A selected spectator view can request more detail.
- **Interface:** the field, current threat and actual outgoing target remain
  prominent. A compact population/standing view and an inspectable roster replace
  a fixed row of eight large opponent panels. Targeting policies use the same
  meanings regardless of how many opponents can be shown.
- **Announcements:** preserve local warnings and sparse survivor milestones;
  never narrate every elimination in a hundred-player stage. A preset may add
  larger milestones, but announcements coalesce to the current state and never
  produce a catch-up queue.
- **Population:** automatic matches consolidate compatible ready players before
  filling with CPUs; spawning a separate almost-empty lobby for every arrival
  defeats the point. The allocation policy should honor the advertised wait and
  capacity limits without multiplying public presets prematurely.
- **Cost:** CPU behavior, admission, state updates and spectator traffic all
  need budgets. Bot-heavy rooms are an important stress case, not assumed cheap
  merely because they have fewer human connections.

Validate at increasing populations (8, 16, 32, 64, 128) before raising the live
cap, including simultaneous full bars, mass eliminations, ready/load failures,
reconnects and bot-heavy matches. Measure timely attack commitments, launch
fairness, simulation cost, bandwidth and field readability separately. The
single-song simulation scenarios still apply inside a match; add stage-reset
and match-lifecycle checks rather than replacing their battle comparisons.

## What the player needs to notice

Keep the normal local chart and HUD central. The reviewed screen study places
one thin offensive bar above life, with matching power bars on faithful opponent
miniatures. At eight players, the other seven fields occupy stable side slots
and approximate their own chart, noteskin, scroll settings, judgments and hazards.
The spectator view promotes the followed player's field without borrowing their
identity or results. Large-party preview density and transport remain open.

Persistent accuracy/score, targeting-policy text and sender/countdown prose are
hidden by default during play. A shock-arrow cue and small strength dots indicate
incoming mines; recovery has a restrained indicator. A fully charged bar may
electrify while waiting, but the target curve appears only after a committed
attack. Its traveling pulse and recipient highlight convey a launch, not damage.
The [play/watch study](multiplayer-ui.md#play-and-spectator-layout) records the
latest layout and reduced-motion treatment. Recent accuracy belongs in results
or optional detail rather than another permanent meter. All material competitive
information remains available with the announcer disabled.

## Announcer events implied by the mechanism

These are event meanings and contexts, not recording text or implemented pack
folder names. The main distinction is between warning of a committed challenge,
reporting an action actually taken, and announcing a final result.

| Event type | Trigger and context | Who hears it and frequency |
| --- | --- | --- |
| Match beginning | A new match's first stage is about to begin | Its participants; once per match, distinct from the start of later stages |
| Next stage approaching | Intermission has a confirmed next song and a valid start countdown | Admitted next-stage participants; includes returning players, distinct from a live attack warning |
| Final stage approaching | The next song is the announced last song of the match | Match participants before play; never infer this from two survivors. In a one-song match, combine it with the opening rather than stacking cues |
| Readiness deadline approaching | The current stage's readiness window reaches its warning threshold | Affected eligible players; at most one warning per window, not a Go cue or one line per second |
| Stage preparing | Ready, loaded competitors have been committed to a start | Those participants; once per valid countdown, invalidated if canceled; excluded players receive the sit-out state instead |
| Stage beginning | Opening transition into the shared song | Participants; fit the lead-in, reusing Ready/Go's role rather than adding three overlapping introductions |
| Attack incoming | A future attack against the local player is committed, with readable warning lead | Recipient only; primary battle warning, one event per attack |
| Attack launched | The local player's attack is admitted and power spent | Sender only; optional acknowledgment, lower priority than their incoming warning |
| Attack weakened | The committed incoming mine count is reduced by recent accuracy | Recipient only; optional feedback attached to that same attack, not a second compulsory line after every warning |
| Life critical | Local life crosses the battle preset's danger threshold while still competing | Local player; a transition, not a remark on every mistake. With no healing, it occurs at most once per stage |
| Survivors milestone | Confirmed live participant count becomes four or two, having started above that count | Remaining participants and spectators; only the current milestone, skipped if several eliminations pass it |
| Local elimination | Local competitive participation ends through confirmed zero life | Eliminated player; once, supersedes critical-life and any pending local attack chatter |
| Stage won | Local player is the unique confirmed winner | Winner; once at confirmation, possibly while still playing the song, whether by survival or song-end ranking |
| Stage not won | Final result is a loss for a local player who survived to the end | That player; once. Already eliminated players need not hear a second defeat line |
| Stage drawn | A shared first place or mutual last knockout is confirmed | Tied participants; once, distinct from victory and defeat |
| Stage interrupted | The stage cannot produce a valid competitive result | Participants; once after confirmation, distinct from an individual forfeit |
| Song finished after victory | A confirmed winner reaches the song's end after continuing to play | Optional completion acknowledgment to that player; no repeated win announcement or changed placement |
| Match concluded | The scheduled match completes and aggregate rankings are confirmed, or the custom persistent-elimination ending is satisfied | Match-level win/loss/draw for participants and a neutral outcome for spectators; distinct from winning an individual stage |
| Match interrupted | The host stops early or a failure prevents a valid completed match | Participants; retained partial standings are labeled incomplete, not announced as a completed match victory |

These are separate meanings, not a requirement to speak every row. In particular,
next-stage, final-stage and stage-preparing events should choose one appropriate
intermission cue instead of stacking introductions. Resetting elimination or
swapping CPU seats is normally roster/UI information, not another spoken line.
Joining an existing match under the custom option also does not replay its
opening announcement. A one-song match can coalesce stage and match victory into
one outcome cue; a four-song match must not announce its first stage winner as
the overall winner.

Keep host disconnection, admin/co-host entry and ownership changes visual by
default. An operator holding or canceling progression invalidates pending start
cues; resuming creates a new readiness window rather than replaying stale cues.
Helping the room does not make a watching operator a participant or change the
announcer's perspective.

Spectators use neutral stage-level milestones and the final outcome; they never
hear an incoming attack addressed to themselves. Following a particular player
does not implicitly change second-person voice perspective. A neutral completion
event can show the winner on screen without requiring spoken player names.

Do not announce a full bar as an instruction to attack: launching is automatic
and may still be waiting for eligibility. Keep targeting changes, recovery ending,
each opponent elimination, every mine hit and every accuracy change visual in
the first preset. Hidden/Sudden, if later added, specialize the same incoming
attack event; an effect-ending event may then matter because readability changes.
Mines do not need an additional spoken start and end around each warning.

### Conflicts and freshness

For the first battle voice policy, allow one spoken line at a time. Terminal
local states and confirmed outcomes outrank incoming warnings; incoming warnings
outrank critical life, milestones and outgoing/weakening feedback. A line about a
state that has ended is discarded. A canceled start must not leave a queued Go.
An incoming warning's deadline is before the affected section becomes visible;
if it cannot be spoken in time, use the persistent visual warning and a short
nonverbal cue rather than a late announcement. The hazard schedule never waits
for a recording. Higher-priority events can interrupt lower-priority speech;
cosmetic feedback does not queue behind it to create a delayed conversation.

Treat weakening as a detail of the incoming event when they coincide, not two
lines competing for the same warning slot. Coalesce remaining-player changes
to the latest confirmed count and revalidate before speaking. Local elimination
suppresses a simultaneous survivor milestone for that player. Do not announce
victory provisionally, even if the interface currently shows only one survivor.

Suppress routine single-player combo, periodic praise, clear/fail and grade-result
chatter in the working battle voice preset. Chart performance remains visible;
the stage outcome owns the battle-result voice. This avoids a chart clear being
mistaken for a win, or a full combo line covering an incoming attack.

## Relationship to the announcer support on main

Current support is documented in [Announcer lines](../announcer-lines.md),
[Announcers](announcers.md) and [Announcer studio](announcer-studio.md). The typed
catalog is in `crates/library/src/announcer.rs`; the shared cue logic is in
`crates/session/src/announcer.rs`, with studio scenes under its `scene/` module.
Packs, variant selection, a voice bus and staged auditions already exist.

The battle events above are new design work. `evaluation win`, `evaluation lose`
and the old `gameplay battle ...` names are currently recognized as unused, not
spoken multiplayer support. The current single-player announcer allows overlap
and drops gated cues; battle's priority, interruption and expiry policy needs an
explicit extension. Do not map a new event to an old name merely because it
sounds similar. Folder names, fallback chains, localization and studio scenarios
should follow after the event semantics are agreed. Missing optional lines stay
silent without changing competitive information or stage behavior.

## How to progress before the simulator is ready

First walk through scripted stage traces using these rules: a normal exchange,
repeated focus, a full bar with no target, the final duel, local elimination
alongside a milestone, and an outstanding attack causing a mutual knockout.
These traces establish event order, HUD information and which voice event wins
each collision. They require no model of human skill and provide no balance
evidence. No multiplayer audition scene exists yet in the studio.

Next review the rule choices that change the experience: Spread versus Retaliate
as the default, defense sampled at commitment, the amount of protected recovery,
and whether song-end ranking is an acceptable finish. Also walk through an early
win followed by song completion, everyone returning with reset state, a newcomer
waiting through the remaining stages and entering the next match, and a custom
late join taking a CPU seat with zero points and no inherited history. Also walk
through a custom match with persistent elimination and a one-song match whose
stage and overall results coincide.
Check match events separately from stage outcomes, including a large population
dropping through several survivor milestones at one checkpoint. A later small human
prototype can judge readability and tension while the simulator eventually runs
the six comparison scenarios. Keep numerical tuning and the final default open.

Online implementation also needs a separate contract: authoritative ordering of
stage events, content identity including music, future attack scheduling with
bounded lateness, attack/replay logs and versioning, and disconnect handling.
Each player's audio clock remains authoritative for their note judgments;
network updates must never move note times or stall local music. The draft above
defines what those systems must preserve, without choosing their transport.
