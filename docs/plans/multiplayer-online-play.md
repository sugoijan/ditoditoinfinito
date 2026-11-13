# Online play: the shared base

Status (2026-11-13): the contract is built and tested in `crates/room`
(since the second round of live play: `live-v2`'s 100 ms ticks and a view
delay each device measures, "The views' delay, lateness and ends")
(`ddi_room::live`, the fake room, the reducer's additions); the server's
side (`../ddi-server`) plays live; the game's side (the client front) is
built and tested against the fake room
([the progress note](multiplayer-prototype-progress.md), "Online play,
the client front"). Not yet tried against a real server.

Read with [the prototype's progress](multiplayer-prototype-progress.md)
(what exists: M5 online rooms, M8 attacks, the rounds of 2026-11-12),
[the battle design](multiplayer.md) and [the screens](multiplayer-ui.md).

## Why

The maintainer wants private online rooms at parity with CPU practice, then
rooms of 1+72 players, then watching without a member limit (2026-11-12).
Until now an online stage was played by each member alone and reported once
at its end (M5d): no live miniatures, no attacks, no early settlement,
nothing to watch. Practice has all of that because every play is judged on
one device (`ddi_session::battle`). Online, each device judges its own
play; what practice shares in memory has to cross the network: when the
song starts, what each competitor's play did, what the attack ledger
decided, and when the stage is decided.

## What does not change

- The room is one Durable Object running `ddi-room`'s reducer
  (`../ddi-server`'s `RoomDO`); `ddi-room` stays free of the engine and of
  I/O. The room never reads a chart.
- Results stay self-reported (see "Trust").
- CPU practice keeps its local path (`LocalAuthority`, the session's
  `AttackRunner`). The room runs the same ledger
  (`attacks::StageBattle`: `attacks-v1`, `placement-v2` sections,
  `power-classes-v0`, `lead-v0`, `settlement-v0`, the targeting policies
  with changes in play) on the same reports, so one trace gives one
  outcome on both (tested), except where a report comes after its
  deadline (below).
- Protocol 2 stays: every addition is optional on the wire, and a peer
  that predates it is never sent what it cannot read.

## The parts

| Part | Where | What it is |
| --- | --- | --- |
| Live policy | `RoomPolicy::live` (`LivePolicy`: the contract `live-v1`, the timing `live-v2`) | The room says it plays live and how fast; its absence says the authority predates it |
| Rules | `live::LIVE_V1`, `live::LIVE_V2` (`LiveRules`) | Every number below, named and versioned |
| View delay | `live::delay` (`ViewDelay`, `VIEW_DELAY_V0`, `ViewMeter`) | How far behind the song a device draws the others' fields: what its frames measure, a second at most |
| Clock | `live::clock` | `Ping`/`Pong` and `ClockEstimator` |
| Start | `Event::StartCommitted::song_zero_at`, `StagePlay::song_zero_at` | Song time zero as an instant on the room's clock |
| Battle plan | `live::plan` (`BattlePlan`, `PlanDigest`, `seed_for`), `Command::SetBattlePlan`, `Event::BattlePlanSet`, `StageContent::battle` | The chart's checkpoints and safe sections, offered by the picker's device, checked by digest |
| Stream | `live::stream` (`Moment`, `Batch`, `Outbox`, `Frame`, `FrameReader`), `live::wire` | Each competitor's moments and reports up; one combined frame a tick down |
| Room side | `live::server` (`LiveServer`, `LiveStage`) | Sans-IO: the stage's streams, the ledger at deadlines, the decision, the frames, resume |
| Client side | `live::client` (`LiveLink`), inside `OnlineAuthority` | Hello, pings, queued messages, batches until acknowledged, frames in order |
| Reports | `attacks::Reporter` | A competitor's checkpoint reports from its judgments, as practice counts them |
| Fake | `fake::FakeServer` (`Latency`, `OffsetClock`, `fault_uplink`) | The room's live side in process, latency and faults both ways |

### Compatibility, both ways

| Peer | Meets | What happens |
| --- | --- | --- |
| New client | Old server | The room's policy has no `live`: the client sends no live frame (a live text frame or any binary message would close its socket with 4400, as the fake shows). It plays as M5d did. |
| Old client | New server | It ignores `live` in the policy, `song_zero_at`, `battle` on content and every live message (text it cannot parse is passed over; binary is dropped by both shells' sockets). It never says hello, so it is sent no frames. It plays on its own Start press and reports at the end. The room treats it as a competitor that does not play live (next row). |
| Old client in a live stage | The room | Not in the ledger (not attackable, earns nothing), not streamed; the room makes no early decision for that stage (it cannot know when that device's player went out) and settles on the reports as before. |
| New shell transport without binary | Its own client | `Transport::binary()` false: no live socket, the M5d path. The web's and desktop's transports must say true (client front). |
| A client that predates `live-v2` | A room made after it (`timing: live-v2`) | It reads the contract's id alone (`live-v1`) and plays live by `live-v1`'s numbers: a batch every 250 ms, its views a second behind. The room ticks every 100 ms; nothing it decides depends on a device's batching. Its field reaches the others a little later than a new client's (their delays rise toward it, or it stalls now and then). |
| A new client | A room made before `live-v2` (no `timing`), or a server that predates it | `live-v1`'s numbers: batches every 250 ms, the room's frames every 250 ms; its views still adapt (about 0.6 to 0.85 s at 20 to 80 ms each way, never more than a second). |
| A room in play across a deploy | The new server | A stored room keeps its policy as stored: one made before `live-v2` ticks at 250 ms until it closes. Only rooms made after the deploy (or stored before live play, given it on load) tick at 100 ms. |

## One clock

A device asks the time with `{"v":2,"live":{"kind":"ping","n":7,"t":<its ms>}}`
and the room answers at once with `{"kind":"pong","n":7,"t":…,"s":<room ms>}`.
`ClockEstimator` takes each exchange's offset (the room's time less the
midpoint of the device's send and receive) and its round trip; the
estimate is the median offset of the three exchanges with the lowest round
trips among those of the last minute, its uncertainty the largest of their
half round trips plus their disagreement. One bad sample (a clock that
jumped, an answer held on one way) is left out; asymmetric delays stay
inside the uncertainty (tested over 32 seeds with up to 80 ms of jitter
each way and a 25 ms asymmetry).

Cadence (`LIVE_V1`): five pings 100 ms apart when a socket opens, then one
every 2 s while a stage is followed or a stream is open, every 10 s
otherwise. The room stamps the pong with its clock when it reads the ping
(in a Worker, `Date.now()` holds still during a handler and moves with I/O,
which is what is wanted: the instant the message was taken).

`OnlineAuthority::room_now()` gives the room's time on the device's clock.
The game maps it to its audio clock (client front).

## One start

Where the policy plays live, the reducer puts the song's time zero on the
committed start: `song_zero_at = start_at + prelude_ms` (`StartCommitted`,
kept in `StagePlay`). `start_at` is the countdown's end as before (3 s after
the commit); the prelude (1 s, `LivePolicy::v1`) leaves room for every
device's own lead-in (a play is drawn from song second −0.8). The phase
machine is unchanged: Countdown, then Stage at `start_at`.

A device schedules its play so that song time 0 is heard at `song_zero_at`
on the room's clock: it begins drawing at song second −0.8 and the music
from its own offset, as a play does, with nothing waiting on a press. The
prompt's Start press of M5d gives way to the room's countdown in live
rooms.

A device that cannot make the instant (the commitment reached it late, a
reconnect, its audio not ready) joins late, up to `late_join_ms` (5 s of
song time): its play starts part-way, the song where the room's clock has
it, the notes it missed judged as misses (a staged start, as the announcer
studio's plays do). Later than that it does not start: it reports a forfeit
and watches. Being late never gains anything. The room ends the stream of
a device that said it would play live and never streamed by 5.75 s of song
time as `NoShow` (a forfeit).

With the readiness window: unchanged; the window decides the lineup, the
countdown the instant. A committed starter leaving or losing its
connection before `start_at` still cancels the countdown (existing rule).
After it, a lost connection is the stream's affair (below). A watcher (or a
member who joins mid-stage) plays the music from `song_zero_at` and draws
the frames a presentation delay behind.

## The battle plan

The room cannot read charts, so the facts its ledger needs come with the
pick: once its `Pick` (or `ChooseContent`) is accepted and the match has
attacks, the picker's device computes the plan and sends it with
`{"kind":"offer_plan","stage":…,"plan":{…}}`:

- the ids it was made under (`attacks-v1`, `placement-v2`,
  `power-classes-v0`, the ruleset `ddi`),
- the stage's seed, `live::plan::seed_for(room, stage, chart)` (from the
  room's id, the stage's and the chart's digest: everyone computes it,
  nobody chooses it),
- every checkpoint's song time (every four beats from the fourth to four
  beats past the last note, as `AttackRunner::new` makes them),
- the schedule: a slot every four beats with its section where the chart
  has room (each shock's tick, lane and time, when it clears), as
  `ddi_session::battle::attacks::schedule` makes it,
- its digest: FNV-1a 64 over a canonical binary form (`fnv1a64:` and 16
  hex digits).

The room checks it (`BattlePlan::check`: its own digest, the version, at
most 2,048 checkpoints and slots, 16 shocks a section, everything in time
order; ids it knows) and that the sender chooses the room's songs or is an
operator, keeps it (`LiveDurable`, which the server stores: the room may
sleep between the pick and the start) and issues `SetBattlePlan`, whose
event `BattlePlanSet` puts the digest on the stage's content for everyone.
A second plan with another digest is refused (`plan_set`).

Every other device computes its own plan from the same chart, the match's
rules and the seed, and compares digests before it says it is ready to
play live: `{"kind":"prepared","stage":…,"horizon_us":…,"plan":"fnv1a64:…"}`
beside its `ReportLoading{Loaded}`. A different digest is a content
mismatch, as a chart digest's is: the room refuses it (`plan_mismatch`) and
the device reports `Loading::Failed { code: "battle_plan_mismatch" }`, so it
sits the stage out. `horizon_us` is its reading horizon
(`ddi_engine::battle::reading_horizon` under its options, after a raised
speed); the room makes its lead from it (below). A stage without attacks
still has `prepared` (with no plan): it is how the room knows who plays
live.

A typical 2-minute chart's plan is about 25 KB as JSON; the bounds keep any
plan under the WebSocket's 1 MiB message limit.

## The play stream

### A moment

What a field needs to draw a competitor's play without judging it again
(the miniature's `FieldFeed`, the spectator's focused field):

| Field | Meaning |
| --- | --- |
| `at_ms` | when it was judged, song ms |
| `tick`, `lane` | which note of the chart (or which battle shock); 0 where there is none |
| `kind` | `tap` (its tier, the engine's `Judgement::index`, the best first and a miss last; its offset in ms, negative early, for the early/late word; `w0` under DDI), `hold_end` (ok or dropped: a hold's state), `mine` (a chart's mine), `shock` (a battle shock stepped on: the view locks the lane for the rules' lock time from it), `boo` (a counted stray press), `refused` (an attack's shocks the device refused, with the attack's id) |
| `life_ppm`, `combo`, `score` | the HUD's numbers after it (score: DDI's sum, as the room shows it) |

Humans play the full chart, so a remote competitor's field is drawn from
the chart itself, not the CPUs' tap-only copy (client front). Nothing is
inferred from a note reaching the receptors: a note leaves the field when a
moment says it was hit.

### Up: batches

A competitor's device cuts a batch every 100 ms under `live-v2` (250 ms
under `live-v1`; `batch_ms`) while its play
runs, and at once for its end: its number (`seq`, from 1), the song time
its play has been judged to (`through_us`), the moments and checkpoint
reports since the last one, and its end (`finished`, `out` with its time,
`forfeit`). `Outbox` keeps every batch until a frame acknowledges it and
sends the unacknowledged ones again after 1 s, and all of them on a new
socket. The room takes batches in order only (an old one is passed over, a
later one left for the resend), from the socket's member only (the
participant is never read from the message), within bounds (512 moments,
16 reports, 4,096 judgments a report). A device sends its stream's end
before the `ReportOutcome` that follows it (`OnlineAuthority::submit` sends
what the live side has first).

### Down: one frame a tick

The room does not relay each batch to each member. Every 100 ms under
`live-v2` (250 ms under `live-v1`; `tick_ms`) it builds one frame and sends the same bytes to every socket
that said hello: its number `n` (one more each frame sent; an empty tick
sends none), the room's time, and for each competitor with news its last
batch taken (`seq`: its acknowledgement), its `through_us`, its new moments
and its end; the attacks committed and the checkpoints run since the last
frame (each checkpoint's record: everyone's power after it and what it
changed); and, when it changes, who the stage waits with on a final attack.
The frame is encoded once (`wire::encode_frame`) and kept.

`FrameReader` takes frames in order once each. A gap (a later `n`) asks
`{"kind":"resume","stage":…,"after":n}` (again after 2 s at most), and the
room sends the frames after `n` while it keeps them (120 frames, 30 s);
otherwise a live snapshot (each competitor's status: live or not, `seq`,
`through_us`, end, life, combo, score, power; every commitment; the plan's
digest; settling) and the last 16 frames (4 s of moments). A socket that
opens on a stage in progress asks the same, so a watcher or a returning
player gets the stage at once.

### The views' delay, lateness and ends

A device draws the others' fields a delay behind the song: their notes
scroll, their moments show and their shocks come and go that far behind
the music, so that a moment shows as its note reaches the receptors
(`ddi_session::battle::feed::FieldFeed`, `set_delay`). The delay is the
device's own, measured, and display alone: nothing the room decides, no
judgment and nothing another device sees depends on it
(`live::delay`, `VIEW_DELAY_V0`, PROVISIONAL, 2026-11-13).

**What it is made of.** A moment reaches a view after the sender's
batching (up to a batch period), the trip up, the wait for the room's
tick (up to a tick), the trip down, and the receiving frame; a field's
stream is judged to the sender's song time when the batch is cut, so
just before the next frame comes, a view is behind it by about
`batch + tick + up + down` (with each trip's jitter) and the two clocks'
error. Under `live-v1` that is up to 500 ms of periods alone, and the
views waited a constant second (`presentation_delay_ms`).

**What a device does now.** Each time a frame brings a competitor's
news, the device samples how far that competitor's stream had fallen
behind its own song time just before (the peak of what a view must
cover); a field whose stream is not judged to the view's time (a stall)
is sampled at once and counted. The target is the 99th percentile of
the last 8 s of samples and 40 ms, within 150 ms and the rules' ceiling
(`presentation_delay_ms`, a second); the delay drawn with rises toward a
higher target at 500 ms a second (the view runs at half speed meanwhile,
never backwards) and falls toward a lower one at 50 ms a second. It
starts from a guess: a batch, a tick, three of the clock's half round
trips and twice the margin. One device, one delay for every field it
draws (the lines between fields need one time); a field whose stream is
slower than the rest stalls now and then rather than holding everyone
back (its samples are a small share of the window's).

**Where the time came out.** `live-v2` halves both periods (a frame and
a batch every 100 ms), the adaptive delay takes out the constant's
slack. Considered and left out: a room forwarding a batch at once
instead of on its tick (in a full room batches arrive all through a
tick, so it saves nothing there unless the frames multiply, and moving
the room's alarm for it costs a storage write each time; in a small
room it would save about 50 ms on average); batches cut as soon as there
is a moment (a stream's judged time moves on with every frame, so there
is always something to send: the period is the bound).

**Measured** (`crates/room/tests/live.rs`, `view_delay_study`, in
process through `FakeServer` with `Latency`, each way the floor plus a
uniform jitter, nine competitors and a watcher, every device's view
measured from the song's eighth second to its 38th, three seeds;
`the_views_follow_half_a_second_behind_at_typical_latencies` holds the
80 ms row):

| Timing | Each way (ms) | Delay mean / p95 / max (ms) | Field-time stalled | Stalls a field-minute |
| --- | --- | --- | --- | --- |
| `live-v1`, constant second (as before) | 80 + up to 40 | 1,000 | 0 | 0 |
| `live-v1`, constant second (as before) | 200 + up to 100 | 1,000 | 0.83% | 13.9 |
| `live-v1`, constant second (as before) | 200 + up to 200 | 1,000 | 13.7% | 92 |
| `live-v1` with the adaptive delay | 20 + up to 20 | 631 / 640 / 650 | 0 | 0 |
| `live-v1` with the adaptive delay | 80 + up to 40 | 781 / 800 / 810 | 0 | 0 |
| `live-v1` with the adaptive delay | 200 + up to 100 | 1,000 (the ceiling) | 0.83% | 13.9 |
| `live-v2` | 20 + up to 20 | 292 / 310 / 310 | 0 | 0 |
| `live-v2` | 50 + up to 30 | 370 / 410 / 410 | 0.03% | 0.9 |
| `live-v2` | 80 + up to 40 | 479 / 500 / 500 | 0 | 0 |
| `live-v2` | 80 + up to 80 | 524 / 540 / 550 | 0 | 0 |
| `live-v2` | 150 + up to 75 | 693 / 730 / 740 | 0 | 0 |
| `live-v2` | 200 + up to 100 | 822 / 850 / 870 | 0 | 0 |
| `live-v2` | 200 + up to 200 | 994 / 1,000 / 1,000 | 0.01% | 0.3 |
| `live-v2`, 73 competitors | 20 + up to 20 | 299 / 310 / 310 | 0 | 0 |
| `live-v2`, 73 competitors | 80 + up to 40 | 480 / 490 / 490 | 0 | 0 |
| `live-v2`, 73 competitors | 200 + up to 100 | 837 / 860 / 880 | 0 | 0 |

So: half a second or less at typical latencies (20 to 80 ms each way),
a little over it where the jitter is as large as the latency; at 200 ms
each way it rises toward the second, and at 200 ms with as much jitter
it sits at the ceiling (as `live-v1`'s constant did, with a hundredth of
its stalls). Stalls: under 0.1% of field-time and about one a
field-minute at worst in every row that is not at the ceiling. The
state is in `__DDI_DEBUG.live.delay` (`ms`, `target_ms`, `rules`,
`live_rules`, `stalls`, `stalled_now`).

**Lateness.** A competitor whose stream falls behind the view's time
holds: its notes not yet judged keep scrolling as unjudged, its last
numbers stay (a stall, which raises the delay); past 2 s (`stale_ms`)
the field is marked stale; when its batches come, the field catches up
(their moments apply at once, newest numbers win). One late competitor
never holds the others: the frame carries whatever came.

A stream ends with its device's own end (`finished`, `out`, `forfeit`), or
the room's: `dropped` once the member is Disconnected (the server's 30 s
reconnect grace) or left, `no_show` for a device that said it would play
live and never streamed. The room keeps taking and sending a stage's
streams for up to 10 s after the room moved on (`LiveServer::LINGER_MS`), so
the last batches of a stage its last reports settled still reach everyone.

## Attacks through the room

Each competitor's device runs an `attacks::Reporter` over its own play
(its chart judgments with their times and tiers, its going out, its policy
changes): once its play is judged past checkpoint k it reports k
(`LiveReport`: the existing `attacks::Report` without the participant) in
its next batch. These are the reports practice's `AttackRunner` makes.

The room's `LiveStage` runs checkpoint k when every competitor in its
ledger has reported k (or ended), or at its deadline, whichever is first:
the deadline is k's song time on the room's clock plus 750 ms
(`report_deadline_ms`), checked every tick. A competitor missing then is
taken as reporting nothing (its life as last known); its report, when it
comes, counts at the next checkpoint (its judgments credited there: the
ledger counts every judgment before the checkpoint's time). The ledger
runs in its fixed order and commits attacks; each commitment goes out in
the next frame.

A commitment (`Commitment`) carries what every device needs to put the
same shocks into the same play: the attack's id, the checkpoint, the
sender, the recipient and the plan's slot. The cells (tick and lane) are
the plan's, which every device has and checked by digest; the engine places
them by tick and lane (`Player::add_hazards`), the recipient's device into
its play, every device onto that competitor's field.

The roster's leads: the room's ledger seats every competitor that said it
plays live, with its member's targeting policy and a lead of
`LEAD_V0.of(horizon) + 1.25 s` (`network_lead_us`): practice's lead plus
what the network takes (below). The same roster gives the same outcome on
both paths; online sections start about 1.25 s later than in practice.

### The timing budget

For a checkpoint at song time c (all times song time on the room's clock;
"typical worst" is a slow home link):

| Step | Time after c |
| --- | --- |
| The device's play judged past c (a miss is final after its window, ≤ 0.2 s under DDI) | ≤ 0.2 s |
| Its next batch cut | ≤ 0.45 s |
| The trip up, and the clock's error | ≤ 0.65 s |
| The deadline | 0.75 s |
| The checkpoint run at the next tick (all in, or the deadline) | typical 0.4–0.7 s, worst 1.0 s |
| The frame down | worst 1.15 s |
| The clock's error | worst 1.2 s |

The earliest section of an attack committed at c starts at
`c + horizon + 1.5 s (warning) + 0.1 s (delay) + 1.25 s (network)`. The
target must show its first shock a horizon and the warning before that,
at `c + 1.35 s`. The commitment reaches it by `c + 1.2 s` at worst: 0.15 s
to spare at worst, 0.6–0.9 s typically, more for later sections.

Under `live-v2` (a batch and a tick every 100 ms) the batch is cut by
0.3 s, the checkpoint runs by 0.85 s at worst (0.6 s with every report
in), the frame is down by 1.0 s and the clock's error leaves it at
1.05 s: 0.3 s to spare at worst. Measured (2026-11-13, the study below
under `live-v2`, 36 stages): the least time to spare 1.81, 1.51 and
1.11 s at 20, 80 and 200 ms each way (with as much again of jitter),
against 1.51, 1.27 and 0.78 s under `live-v1`; no attack refused, 150
taken at each latency. The deadline (0.75 s) and the network's lead
(1.25 s) are unchanged: there is room to take 0.25 s off the lead
(attacks would land that much sooner online), which is the maintainer's
to decide ("Open questions", 11).

Measured (2026-11-13): commitments are taken from frames as they arrive
(not at the views' delay). In process, a person among three bots at 20,
80 and 200 ms each way with as much again of jitter, no attack was
refused in 36 stages; the least time to spare when one came was 1.51,
1.27 and 0.78 s (a slow speed adds to it). With every report in, a
checkpoint runs a tick after the last one, not at its deadline. Against
a local server, four bots: 1.49 to 2.18 s. The progress note's "A person
in a live room went unattacked" has the numbers and the debug state
(`__DDI_DEBUG.live.me`) that shows each attack's fate.

A commitment that reaches its target later than its first shock's warning
is due (a stalled network, a reconnect) cannot be shown fairly. The
target's device refuses it, as practice refuses a delivery a stalled frame
made late, and says so in its stream (`refused` with the attack's id: the
views take those shocks off its field). The attack is spent and dropped,
not moved a section later: practice never moves one, the sender's bar was
spent at the commitment either way, and to the ledger it is an attack its
recipient avoided entirely (it clears at its time). A dishonest device
could refuse every attack this way; the room cannot tell that from a slow
network (see "Trust").

## Settlement and the stage's end

The room decides a stage as practice does (`SETTLEMENT_V0`), from the
streams' ends and the ledger, as soon as it can, and only where every
competitor plays live:

- at most one competitor still in (by the streams: `out` is out,
  `forfeit`, `dropped` and `no_show` are forfeits), and no committed attack
  against someone still in whose play has not been judged past its clear;
- while the one left waits on such an attack, the frames say who and when
  the one before went out (`settling`); if they go out to it, they share
  first with those out in the same checkpoint interval as that one
  ("Drawn: final attack");
- then it confirms the ranking (`Command::ConfirmStage`, the authority's)
  and the phase goes to SongCompletion: the winner finishes the song.

A stream's end is checked for a decision at once, before the
`ReportOutcome` the same device sends next, so a final attack's draw is
never preempted by the reports' plain ranking.

The reports still come: each device sends its `ReportOutcome` (with its
`RunDetail`) when its play ends. New in the reducer, for a live room's
stage confirmed early: a report in SongCompletion is taken for the
standings' detail alone (the result does not change), the song ends once
everyone reported, and the report deadline (`self-reported-v0`: 60 s after
the first report of the song played to its end, 20 minutes at most) ends it
otherwise. The room ends the song itself (`EndSong`) once every stream has
ended and every device that ended its own stream reported, so a dropped
or absent competitor does not hold the song's end for a minute; a stage not
decided early (two or more finished) is settled on the reports as before,
with forfeits for the dropped.

A forfeit (a quit) is the device's `forfeit` end then its forfeit report. A
connection lost inside the grace: the stream stops, the views mark that
field stale, the ledger takes it as reporting nothing (it earns nothing
meanwhile, it can still be attacked), and when it returns its device sends
what was not acknowledged (its late reports counting at the next
checkpoints) and resumes the frames. Past the grace (`Disconnected`): the
room ends its stream as `dropped`, a forfeit for the decision and out for
the ledger from its last judged time.

## Watching and an audience

A member who watches is sent the frames like everyone else and is asked
nothing: no batches, no readiness. It plays the music from `song_zero_at`
and draws the fields a presentation delay behind, from the frames (the
spectator view of 2026-11-12 is fed the same way).

An audience beyond the members (not built): counted, not listed, read
only. Two ways leave the room's cost flat: relays (a second Durable Object
or a Worker that holds a few hundred sockets, takes the room's frames once
and fans them out) or cached segments (the room appends each second's
frames to a segment in storage, viewers fetch segments over HTTP a few
seconds behind). Frames are already the same bytes for everyone and
numbered, so either needs only a way in: an audience ticket that admits a
socket without a member, and a count the room shows. The room's member
limit then stays a limit on players and people with a seat.

What the round of 1+72 learned for it (2026-11-13): every member is
listed in the room's state, which the server writes as one row on every
change and sends whole in every snapshot: 12 KB at 73 players, 58 KB with
300 members watching, 166 KB with 1,000; a join or a connection change
costs 0.06 ms at 73 and 0.36 ms with 300 watching (native; the reducer and
the row's JSON), and sends an event to every member's socket, so a wave of
`n` joining costs `n²` sends. Members therefore stop at a bound
(`RoomPolicy::DEFAULT_MAX_MEMBERS`, 384: a full field and about 300
watching, what the load test carried); an audience must not be members:
no row in the state, no event per arrival, a ticket of its own, the count
kept apart (a number in the state changed at most once a second), and its
frames from a relay or segments, as above. The frames themselves already
cost the room nothing more per viewer than a send.

## Encoding and numbers

Batches and frames are binary WebSocket messages (`wire`: varints, deltas
within a list, a tag byte with the version, every read bounded and never
panicking); everything else live is JSON text. Measured with the stream's
own types (`tests/live.rs`, `the_streams_sizes`):

| One competitor, sent a second | JSON | Binary |
| --- | --- | --- |
| 4 notes a second (4 batches, reports every checkpoint) | about 1.0 KB | about 124 B |
| 12 notes a second | about 2.3 KB | about 247 B |

| One frame (a tick) at 12 notes a second | JSON | Binary |
| --- | --- | --- |
| 9 competitors | 4.3 KB | 449 B |
| 33 competitors | 15.6 KB | 1.6 KB |
| 73 competitors | 34.3 KB | 3.5 KB |

The room's traffic at 12 notes a second, binary, every competitor's socket
following, no audience (batches 4 a second and pings 0.5 a second in;
frames 4 a second to each socket out):

| Competitors | Room in | Room out | One client |
| --- | --- | --- | --- |
| 9 | 41 messages/s, 2.5 KB/s | 36 messages/s, 16 KB/s | 4.5 up (0.3 KB/s), 4 frames down (1.8 KB/s) |
| 33 | 149 messages/s, 9 KB/s | 132 messages/s, 210 KB/s | 4.5 up, 4 down (6.4 KB/s) |
| 73 | 329 messages/s, 20 KB/s | 292 messages/s, 1.0 MB/s | 4.5 up, 4 down (14 KB/s) |
| 73 and 300 watching | 329 messages/s | 1,492 messages/s, 5.2 MB/s | |

As JSON every downstream figure is about ten times as large (73 competitors:
137 KB/s to each socket, 10 MB/s out of the room). Binary it is: both
browsers and the Worker runtime carry binary messages (the web socket's
`binaryType = "arraybuffer"`; workers-rs takes `WebSocketIncomingMessage::Binary`
and sends bytes; tungstenite's `Message::Binary`). Compression
(permessage-deflate) is not relied on.

What it costs the room (`what_a_tick_costs_with_seventy_three_competitors`,
native, optimized, on the maintainer's Mac; the Worker's wasm will be
slower, perhaps two to three times): reading 73 batches 20–40 µs a tick;
the tick itself (the ledger's checkpoints, a frame built and encoded once)
30–55 µs on average, at most about 0.5 ms (a checkpoint tick with
launches). 73 competitors through the whole fake room (sockets, latency,
the reducer) for 40 s of room time: about 2 s of test time. The
per-message cost of sending a frame to each of a few hundred sockets is the
server's to measure (backend front).

Nothing in the contract assumes nine: ids are 64-bit, frames carry only
competitors with news, deadlines do not depend on the count, the ledger's
work is bounded per checkpoint (tested at 128 seats), and the fake runs 73
competitors. The room's cap is 73 since 2026-11-13 (`RoomPolicy`;
[the progress note](multiplayer-prototype-progress.md), "Rooms of 1+72
players"), its member bound 384, and fields of nine to 73 are awarded by
the top-biased tables (`ddi_room::TopBiased`): the 73-player tests run
under the room's own policy and awards.

## Trust

Results stay self-reported; this is where a dishonest client could gain,
undefended:

- its moments and reports (tiers, life, score, out time) are what it says:
  more power, a longer life, a better place;
- it can refuse every attack (`refused`); the room counts refusals but
  cannot tell them from a slow network;
- it can hold its reports back: they then count a checkpoint later, which
  shifts its power, never gains it more;
- a picker or operator could offer a plan that does not match the chart:
  every honest device would then sit the stage out (a mismatch), which is
  disruption, not gain;
- it cannot speak for another member (the room takes the participant from
  the socket), and its clock only schedules its own play.

## Platform facts (checked 2026-11-13)

What the plan assumed of Cloudflare's Workers and Durable Objects,
checked against their documentation (and workers-rs 0.8.7's source),
with what the server's room does about each.

- **Message size.** A WebSocket message *received* by a Worker or a
  Durable Object may be up to 32 MiB; a larger one closes the socket with
  1009. No limit is documented for sent messages. (The plan's "1 MiB"
  above was the older figure; nothing here needs more.) The room refuses
  any binary message over 64 KiB itself, before decoding it (`bad_batch`,
  the socket kept). Sources:
  [Workers WebSockets](https://developers.cloudflare.com/workers/runtime-apis/websockets/),
  [Durable Objects limits](https://developers.cloudflare.com/durable-objects/platform/limits/).
- **Binary messages, hibernation, workers-rs.** The hibernatable
  `webSocketMessage` handler gets binary data as an `ArrayBuffer`; the
  `websocket_standard_binary_type` change (Blob by default, compatibility
  dates from 2026-03-17) does not affect it, and the server's
  compatibility date is 2026-03-01 anyway. workers-rs's `#[durable_object]`
  turns it into `WebSocketIncomingMessage::Binary(Vec<u8>)`;
  `WebSocket::send_with_bytes` copies into a fresh `Uint8Array` on every
  send, so the room sends a frame as ONE `ArrayBuffer` to every live
  socket (`web_sys`'s `send_with_array_buffer`). Sources:
  [changelog 2026-04-21](https://developers.cloudflare.com/changelog/post/2026-04-21-websocket-standard-binary-type/),
  workers-rs `worker-macros/src/durable_object.rs`, `worker/src/websocket.rs`.
- **Attachments.** A socket's attachment holds at most 16,384 bytes
  serialized (the room's: participant, session expiry, connection id and
  the hello flag, well under). Source:
  [WebSockets best practices](https://developers.cloudflare.com/durable-objects/best-practices/websockets/).
- **The clock in a handler.** "APIs that return timers, including
  `performance.now()` and `Date.now()`, only advance or increment after
  I/O occurs" (against Spectre): read at a handler's start, `Date.now()`
  is the moment the message was delivered, which the pong carries.
  Source: [Workers performance](https://developers.cloudflare.com/workers/runtime-apis/performance/).
  (So no handler can time its own computation: the local runtime's traces
  give wall time only, below.)
- **Alarms and hibernation.** One alarm an object, `setAlarm` replacing
  it; at-least-once, retried with backoff from 2 s when the handler
  throws; one `alarm()` at a time. An object is eligible to hibernate
  after 10 s without an event; alarms are events, so a stage's alarm every
  250 ms keeps the room in memory for the stage (its live part is never
  lost to hibernation) and the room sleeps again once `next_wake` is
  none (10 s after the stage's streams end at most). Hibernated sockets
  stay connected, also when the object takes new code; a forced update of
  a running object disconnects them. Sources:
  [Alarms](https://developers.cloudflare.com/durable-objects/api/alarms/),
  [lifecycle](https://developers.cloudflare.com/durable-objects/concepts/durable-object-lifecycle/),
  [WebSockets best practices](https://developers.cloudflare.com/durable-objects/best-practices/websockets/).
  The room therefore closes with 1012 the sockets still open when it is
  given live play on loading (they reconnect and read the live policy).
- **Billing** (the figures below are `live-v1`'s; under `live-v2` a
  full room's batches are 10 a second each and its alarms 10 a second:
  about 138,000 requests, 36,000 alarms and 36,000 rows written an hour,
  about 7 cents an hour; and 73 competitors with 300 watching send the
  room about 920 messages a second, under the 1,000 a second the next
  item names). Incoming WebSocket messages are billed as requests at
  20:1; outgoing messages and protocol pings are free; alarm invocations
  are requests; each `setAlarm` is a row written (SQLite objects); duration
  is wall time at 128 MB while the object is active and not eligible to
  hibernate. A stage keeps its room active, so for an hour of play:
  duration 0.125 GB × 3,600 s = 450 GB-s; 73 competitors' 330 messages a
  second in = about 59,000 requests; 14,400 alarms and as many rows written.
  At the published paid rates ($12.50 a million GB-s, $0.15 a million
  requests, $1.00 a million rows written, after the included amounts)
  that is about 3 cents an hour of a full room; watchers add only their
  pings (one every 2 s each, 1/20 of a request). The room writes its
  state only when the reducer changes it: batches, pings and most ticks
  write nothing. Source:
  [Durable Objects pricing](https://developers.cloudflare.com/durable-objects/platform/pricing/).
- **CPU and memory.** 30 s of CPU by default per request, alarm or
  message for a SQLite object (configurable to 5 minutes), each message
  resetting it; 128 MB of memory an isolate (JavaScript heap and Wasm
  together), which objects of the same class may share. Sources:
  [Durable Objects limits](https://developers.cloudflare.com/durable-objects/platform/limits/),
  [Workers limits](https://developers.cloudflare.com/workers/platform/limits/).
- **Sockets and sends.** At most 32,768 hibernatable WebSockets an object,
  "but the CPU and memory usage of a given workload may further limit the
  practical number"; a soft limit of 1,000 requests a second an object
  (whether incoming WebSocket messages count toward it is not said: the
  room assumes they do; 73 competitors and 300 watchers send about 500 a
  second). No limit on sends a second is documented: an object is one
  thread, so its CPU is the ceiling. Sources:
  [Durable Object state](https://developers.cloudflare.com/durable-objects/api/state/),
  [Durable Objects limits](https://developers.cloudflare.com/durable-objects/platform/limits/).

### The server's load test (local, 2026-11-13)

`../ddi-server`'s `ddi-load` (a development tool) against a `wrangler dev`
(workerd 1.20261023.1) of a release `dev-auth` build on the maintainer's
Mac, driver and runtime on one machine over loopback: a 40 s synthetic
stage at 12 notes a second with attacks, every person the game's own
client on a socket of its own. Frames built and delivered: 165 of 165 to
every socket in every run, no refusal, close or unanswered command, the
stage settled each time.

| Competitors | Watching | Frame spacing p50 / p99 / max (ms) | Room in (msg/s, KB/s) | Room out (msg/s, KB/s) | workerd CPU (ms/s) | Alarm wall p50 / p99 (ms) |
| --- | --- | --- | --- | --- | --- | --- |
| 9 | 0 | 250 / 276 / 282 | 38, 2.5 | 45, 16 | 43 | 3 / 11 |
| 9 | 100 | 250 / 273 / 292 | 101, 5.9 | 659, 245 | 75 | 5 / 18 |
| 9 | 300 | 250 / 287 / 307 | 232, 13 | 2,537, 997 | 145 | 8 / 37 |
| 33 | 0 | 250 / 277 / 330 | 140, 8.6 | 225, 200 | 86 | 4 / 19 |
| 33 | 100 | 250 / 273 / 279 | 204, 12 | 1,046, 871 | 123 | 6 / 18 |
| 33 | 300 | 250 / 289 / 378 | 333, 19 | 3,329, 2,510 | 189 | 10 / 38 |
| 73 | 0 | 250 / 282 / 305 | 309, 19 | 718, 970 | 126 | 3 / 29 |
| 73 | 100 | 250 / 260 / 265 | 375, 23 | 1,886, 2,386 | 139 | 5 / 12 |
| 73 | 300 | 250 / 266 / 286 | 504, 30 | 4,861, 5,509 | 237 | 8 / 26 |

What it shows: the room's tick holds 250 ms (p99 within 40 ms, the
driver's own loop included) and nothing is lost up to 73 competitors and
300 watchers; the bytes match the plan's estimates (73 and 300: 5.5 MB/s
out); the local runtime spends at most a quarter of a core on the whole
room (both workerd processes, sockets' I/O included: an upper bound for
the object). What it does not show: the deployed platform (another CPU,
the network, the isolate's memory: the processes' resident size,
250–1,200 MB here, counts every room the runtime kept and the runtime
itself, not one object's), CPU per tick (no in-object timer exists, the
alarm's wall time includes its storage writes), and clients far away.
A run against the deployed service is the maintainer's: a `dev-auth`
build never deploys, so it needs the bots' sign-in (`POST /auth/bots`,
eight a room and sixty an hour a token, for this many the limits raised
for the run) or a temporary deployment of its own, and the competitor
cap raised for that room only.

### The load test under `live-v2` (local, 2026-11-13)

The same driver and setup (a fresh `wrangler dev` of a release `dev-auth`
build of its own on port 8797, a 40 s stage at 12 notes a second with
attacks), the rooms now on `live-v2`'s 100 ms ticks, and every socket's
view measured as the game draws it (`ddi-room`'s `ViewMeter` with
`view-delay-v0`, from the song's eighth second; the driver and the
runtime on one machine over loopback, so the delay is the periods'
alone, about a quarter second):

| Competitors | Watching | Frame spacing p50 / p99 / max (ms) | Room in (msg/s, KB/s) | Room out (msg/s, KB/s) | workerd CPU (ms/s) | Alarm wall p50 / p99 (ms) | View delay mean / p95 / max (ms) | Field-time stalled |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 9 | 0 | 100 / 104 / 105 | 85, 3.3 | 93, 23 | 81 | 2 / 4 | 248 / 250 / 250 | 0 |
| 9 | 100 | 100 / 117 / 138 | 147, 6.7 | 1,229, 328 | 158 | 6 / 19 | 262 / 270 / 280 | 0 |
| 9 | 300 | 100 / 110 / 122 | 278, 13.8 | 4,163, 1,237 | 280 | 9 / 17 | 256 / 260 / 270 | 0 |
| 33 | 0 | 100 / 105 / 125 | 311, 11.5 | 399, 288 | 184 | 3 / 6 | 250 / 250 / 250 | 0 |
| 33 | 100 | 100 / 126 / 145 | 374, 15.0 | 1,745, 1,220 | 246 | 5 / 23 | 259 / 270 / 280 | 0 |
| 33 | 300 | 100 / 112 / 140 | 505, 22.1 | 5,089, 3,391 | 391 | 10 / 23 | 259 / 270 / 357 | 0 |
| 73 | 0 | 100 / 106 / 135 | 687, 25.3 | 1,103, 1,385 | 263 | 4 / 6 | 251 / 260 / 260 | 0 |
| 73 | 100 | 100 / 109 / 135 | 753, 28.9 | 2,795, 3,372 | 309 | 6 / 9 | 254 / 260 / 270 | 0 |
| 73 | 300 (fresh runtime, twice) | 100 / 108–109 / 148–205 | 882, 35.9 | 6,833, 7,653 | 475–479 | 10 / 13 | 258–268 / 260–310 / 270–410 | 0–0.16% |

Every frame built reached every socket, no refusal, close or unanswered
command, every stage settled, except once: the first 73-and-300 run, on
a runtime that had served the eight rooms before it (1.1 GB resident),
fell behind; its frames stopped for up to 18 s and the clients, which
then resent every unacknowledged batch each second, flooded it (17,000
messages a second in); it never recovered and the runtime had to be
restarted. The clients now back off their resends (`Outbox`: the wait
doubles to 8 s while unanswered); twice more on a fresh runtime the
same room ran clean. Against `live-v1`'s figures the room takes two to
two and a half times the messages and twice the CPU: about half a core
of the local runtime for 73 competitors and 300 watching (was a
quarter). The deployed platform may be two to three times slower than
this Mac: a full room with a large audience could pass one core there.
For that size the tick could fall back to 250 ms where the room has
many live sockets (a server rule, the views adapt by themselves): not
built, the maintainer's to decide ("Open questions", 10).

## What the backend front builds (`../ddi-server`, `RoomDO`)

1. New rooms' policy with `live: LivePolicy::v1()`; a stored room without
   it gets it when the object loads (before its first snapshot), as rooms
   from before the host's succession were.
2. `ClientFrame::Live` read from text; binary messages taken
   (`websocket_message` with `Binary`) and refused above a size (say 64 KiB)
   before decoding.
3. A per-socket live flag, set by `hello`, kept in the socket's attachment
   so that hibernation keeps it; frames go only to those sockets.
4. A `LiveServer` beside the reducer: `receive` for live text,
   `receive_binary` for batches, `tick` after every change of the room and
   on the alarm; its `LiveOut` applied: replies to the sender (text as
   `ServerLiveEnvelope` JSON, kept frames as binary), broadcast frames as
   binary to live sockets (the same bytes), its commands through the
   reducer as the authority's, their events sent as today.
5. The alarm at the earliest of the room's `next_deadline()` (which now
   covers SongCompletion) and `LiveServer::next_wake()` (every 250 ms during
   a stage).
6. `LiveDurable` (plans and preparations) stored after every `offer_plan`
   and `prepared` that changed it, restored on wake. The stage in play is
   kept in memory only: an object restarted mid-stage loses it, and that
   stage settles on its reports as before (say so in the logs).
7. Pongs stamped with the room's clock when the ping is read.
8. A load test with bots streaming (the client front gives `--mp-bots` live
   streams): 9, 33 and 73 competitors (the development policy's cap), a few
   hundred watching sockets, CPU per tick and bytes out measured on the
   deployed runtime; the cap stays nine for people.
9. Native tests: the fake's scripts against the object's handlers where it
   can, the plan's limits, the binary size limit, a restart mid-stage.

## What the client front builds (the game)

1. Transports: the web's (`binaryType = "arraybuffer"`, `Binary` events,
   `send_binary`, `binary()` true) and desktop's (tungstenite binary both
   ways).
2. `RoomClient`: `live_mut().set_me`; for a match with attacks, the
   picker's device computes the plan (`AttackRunner`'s checkpoints and
   `schedule` with `seed_for`) and offers it after its pick; every device
   computes its own on `BattlePlanSet`, compares, then sends `prepared`
   (its reading horizon under its options, a raised speed included) and
   reports Loaded, or `battle_plan_mismatch`. Without attacks, `prepared`
   with no plan.
3. The synchronized start: the play scheduled from `song_zero_at` and the
   clock estimate onto the audio clock (web: the context's clock; desktop:
   the host clock), the prompt's Ready flow ending in the room's countdown
   with no Start press; a late join up to 5 s (a staged start from where
   the room's clock is), beyond it a forfeit and watching.
4. The stream: the play's judge events turned into moments (tiers,
   offsets, W0, hold ends, mines, shocks, Boos, refusals, with life, combo
   and score), an `attacks::Reporter` for the reports (judged to the play's
   judged time), the stream's end before the `ReportOutcome`.
5. A remote ledger in `ddi_session::battle` for online stages: no local
   `AttackRunner`; commitments from the frames put into the play (refused
   when too late, with a `refused` moment) and onto the fields; power bars,
   waiting lines, the warning and the battle's lines from the checkpoints'
   records; the decision from the room's `StageConfirmed`.
6. Miniatures and the spectator view fed from frames
   (`ddi_session::battle::feed`) at the presentation delay, the full chart
   for people, stale fields marked, locks from shock moments; watchers play
   the music from `song_zero_at`.
7. The debug state (`__DDI_DEBUG`) for the clock, the stream, the frames;
   tests against `FakeServer` with `Latency` and `OffsetClock`.
8. Bots (`--mp-bots`) streaming as players do, for the backend's load test.

## Order

1. This base (done).
2. The backend and the client fronts in parallel: the client against the
   fake room, the server natively and with the fake's scripts.
3. Together: two browsers and desktop against `wrangler dev`, then the
   deployed server; bots for load.
4. Scale: an award rule for fields over nine, the cap raised by policy,
   the miniatures' density for many opponents (`multiplayer.md`, "Eight
   first and larger parties later").
5. The audience (relays or segments).

## Left out on purpose

Server-side judging (results stay self-reported); the audience beyond the
members; CPUs in online rooms; raising the cap; award rules over nine; a
live stage surviving the room's object restarting; compression; a JSON form
of batches and frames on the wire (their JSON exists for tests and tools);
moving a refused attack.

## Provisional choices

| Choice | Where | Effect | Decision needed |
| --- | --- | --- | --- |
| `live-v2` (a room's `timing`, 2026-11-13): a frame and a batch every 100 ms; `live-v1`'s 250 ms kept by rooms made before | `live::LIVE_V2`, `LivePolicy::v2` | Remote fields about half a second behind at typical latencies, at two and a half times the messages (about 7 cents an hour of a full room, was 3) | The periods |
| `view-delay-v0`: each device's view delay the 99th percentile of the last 8 s of measured lags and 40 ms, within 150 ms and a second, up at 500 ms a second, down at 50 | `live::delay::VIEW_DELAY_V0` | A remote field runs a little slow for a moment when the network gets worse, a little fast as it recovers; one slow competitor's field stalls now and then rather than holding the others back | The numbers; whether a slow field should hold everyone back instead |
| Checkpoints decided 750 ms after their instant; a later report counts at the next | `LIVE_V1.report_deadline_ms` | A slow device's power arrives a checkpoint late | The deadline |
| 1.25 s more lead online | `LIVE_V1.network_lead_us` | Online attacks land about 1.25 s later than practice's | The margin |
| A late commitment is refused by its target and dropped, not moved | client rule, `MomentKind::Refused` | Rarely, an attack does nothing | Drop or move |
| Song zero 1 s after the countdown | `LivePolicy::v1` | A stage starts 4 s after its commit | The prelude |
| A device joins up to 5 s late, later it forfeits and watches; the room ends a silent one at 5.75 s | `LIVE_V1.late_join_ms` | A slow device misses the first notes or the stage | The bound |
| The room decides an online stage early (survival too): the winner finishes the song | `LiveStage::decide` | Online rooms behave as practice | Confirm for survival rooms |
| A device that predates live play is not attacked and stops early decisions | `LiveStage::new` | A mixed room plays as M5d | Refuse such devices in live rooms instead |
| Disconnected past the grace (30 s) is a forfeit | `LiveStage::room_ends` | A dropped player is out of the stage | The grace for a stage |
| The stage's seed from room, stage and chart; plans digested with FNV-1a 64 | `live::plan` | None visible | None |
| A stage's streams kept 10 s after the room moved on | `LiveServer::LINGER_MS` | None visible | None |

## Open questions for the maintainer

1. (Answered 2026-11-13: aim for half a second or less, a second where it
   must.) Built as `live-v2` and a measured delay; is the price (two and a
   half times the messages, about 7 cents an hour of a full room) right?
2. Late join (up to 5 s, missed notes as misses) or always sit out a stage
   the device cannot start on time?
3. An attack that reaches its target too late: dropped (as now) or moved to
   the next safe section?
4. Online attacks land about 1.25 s later than practice's (the network's
   margin): fine, or should practice use the same lead so the two train
   the same?
5. Should a live room refuse devices too old to play live, rather than
   letting them in without attacks and early decisions?
6. Online survival rooms now end early as practice does (the winner
   finishes): confirm.
7. The audience: relays or cached segments; counted where?
8. An award rule for fields over nine, needed before the cap can rise.
9. A room's object restarting mid-stage loses that stage's live part (it
   settles on reports): acceptable for the prototype?
10. A full room with a large audience costs about twice the CPU under
    `live-v2` (half a core locally for 73 and 300 watching): keep 100 ms
    ticks for every room, or tick at 250 ms past some number of live
    sockets?
11. Online attacks could land 0.25 s sooner under `live-v2` (the
    network's lead has that room to spare): take it, or keep the margin?

## Tests

`crates/room`: the estimator (one exchange, jitter and asymmetry over 32
seeds, one bad sample left out, a backwards clock, old samples, pongs); the
plan (digest, order, bounds, JSON, the seed); the binary codec (batches and
frames read back equal, every truncation and many corruptions refused
without a panic, sizes); `Outbox` and `FrameReader`; older peers' JSON read
and written alike; and through the fake room (`tests/live.rs`): five
competitors and a watcher streamed with jitter, frames dropped, delayed and
duplicated, batches lost on the way up and a socket dropped mid-stage,
every recipient's fields ending equal to what was sent; one attack trace
through the ledger locally and through the room with the reports in the
jitter's order (the same records, commitments and plan cells everywhere);
a stalled uplink's late reports counted at the next checkpoint without
stalling the room (the documented difference); the plan offered by a
non-picker, forged, offered twice and computed differently (refused, the
mismatching device sitting the stage out); a survival stage decided by the
room as it goes, the winner finishing and the reports kept; a connection
lost past the grace as a forfeit that does not hold the song's end; a
final attack holding the decision and its knockout a shared first; a
member coming in to watch mid-stage given the stage as it stood (a
snapshot and the last frames) and following it to its end; a room that
does not play live never sent a live frame; 73 competitors through the
fake; the cost of a tick with 73.
