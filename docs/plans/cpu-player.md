# CPU player

Implemented for desktop and web. A seeded, adjustable analytical CPU plays
imported or bundled single-player keyboard 4K charts through the normal judge.
No offline profile, model download, training environment or external calculator
is required. Strength is relative to the selected chart, not a transferable
human skill rating or a guaranteed score. The multiplayer CPU opponents play
at absolute levels instead (`cpu-levels-v2` since round V, 2026-11-17,
below): what a level keeps up with is the same on every chart, its hits
loosen long before it misses, as a person's, and the solo CPU plays at
one of them.

## Launch

Append the CPU parameters to a chart's existing web play route:

```text
#/play?song=<song-id>&chart=0&cpu=1&level=5&seed=7
```

Desktop uses the song's library id and zero-based chart index:

```sh
cargo run -- --play SONG --chart 0 --cpu=1 --cpu-level 5 --cpu-seed 7
```

Since 2026-11-14 (the maintainer: "The CPU CLI bots should move to the
new levels") the solo CPU plays the opponents' absolute scale
(`ddi_engine::cpu::CPU_LEVELS`, `cpu-levels-v2` since round V, below):
`level=N` on the web or `--cpu-level N` on desktop, 1 to 10, is its
strength; without one it plays **level 5** (`ddi_engine::cpu::SOLO_LEVEL`:
the middle of the scale, an ordinary player's A on the charts it is at
home on, so that a CPU started with no flag plays the common charts well
and the hard ones badly, as a level should). The five named
strengths, `strength=` / `--cpu-strength relaxed|easy|normal|strong|expert`,
are still taken and are **levels 6 to 10** of the new scale
(`strength_level`), as they were levels 6 to 10 of the chart-relative one;
a level and a strength cannot be combined. Prepared profiles (`--cpu
FILE`, a `cpu=` path) play their strength, chart-relative, as before.
The chart-relative model (`cpu-levels-v0`, the five strengths'
multipliers 0.75, 0.9, 1, 1.1, 1.25) stays in the engine by name with its
fixtures, reachable from prepared profiles, the tests and the studies;
round V left it and the prepared profiles exactly as they were (they
play the tap-only copy at their strength, chart-relative).
The prompt and the results say "Level 5 of 10". Seed 1 by default. Seeds are unsigned 64-bit
integers; the same chart, level, skills and seed reproduce the same inputs. Retry
keeps them; returning to song select does not leave CPU mode enabled.

The play is marked CPU, with no personal history, bests or calibration. Human
lane input, ghosts, per-play transforms and rate changes are disabled; cancel
and retry still work. Fail is off, but matching, timing windows and scoring
remain those of the selected ruleset. At a level (the default) the CPU plays
the chart a person is given (`cpu-play-v1`, below: holds kept down, rolls
re-tapped, lifts released, the chart's mines avoided; fakes are not played);
from a prepared profile it plays the tap-only copy as before (`cpu-play-v0`:
holds and rolls become tap heads; mines, lifts, fakes and tails are
omitted). Source charts are unchanged.

## Skill personality

Automatic mode accepts `&skills=jacks` on the web, or `--cpu-skills jacks`
on desktop. Use `balanced` (the default), `stream`, `jacks`, `chords`, `stamina`,
`technical`, `evasive` or `reckless`, or seven or eight comma-separated
multipliers in this order:
**Stream, Jumpstream, Handstream, Stamina, JackSpeed, Chordjack, Technical,
Evasion** (seven leave Evasion at 1, as every vector was before it).
Each value is 0.5–2 inclusive in steps of 0.001. One is neutral; larger means
better at that pattern, and for Evasion better at getting around a battle's
shocks (below). For example:

```text
#/play?song=<song-id>&chart=0&cpu=1&strength=normal&seed=7&skills=1.1,0.9,1,1.25,0.8,1,1
```

```sh
cargo run -- --play SONG --cpu=1 --cpu-skills 1.1,0.9,1,1.25,0.8,1,1 --cpu-seed 7
```

Presets are explicit vectors, not trained players:

| Preset | Stream | Jumpstream | Handstream | Stamina | JackSpeed | Chordjack | Technical | Evasion |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| balanced | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| stream | 1.25 | 1 | 1 | 1 | 0.8 | 1 | 1 | 1 |
| jacks | 0.8 | 1 | 1 | 1 | 1.25 | 1 | 1 | 1 |
| chords | 0.8 | 1.25 | 1.25 | 1 | 0.8 | 1.25 | 0.8 | 0.8 |
| stamina | 0.8 | 1 | 1 | 1.25 | 1 | 1 | 1 | 1 |
| technical | 0.8 | 1 | 1 | 1 | 1 | 1 | 1.25 | 1.25 |
| evasive | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1.6 |
| reckless | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 0.6 |

Evasion's values (round 4 of `docs/plans/ddi-ruleset.md`): `technical` dodges
better (mine-dodging is footwork), `chords` mashes onto more shocks, and two
balanced presets, `evasive` and `reckless`, differ in Evasion alone (the
practice roster's dodgers and stompers). The others stay at 1.

These borrow MSD names but are **heuristic pattern controls, not MSD ratings**.
The prompt/results show the numeric vector. Balanced preserves the previous
seeded inputs exactly. Prepared profiles reject nonbalanced skills rather than
silently ignoring them. Strength and skills are independent; there is no
normalization to equal final scores. A specialist can score differently on
different patterns at the same strength. Omission, early/late symmetry, input
matching and the motor remain unchanged. No focus state or correlated mistakes
are added.

The causal demand proxies use row speed, recent two/three-plus-note chord
shares, same-lane repetition, sustained six-second load, interval irregularity,
same-hand single-note transitions and chord-size transitions. Hands assume the
usual keyboard split (lanes 0/1 and 2/3). Demands overlap; this is deliberately
not a disjoint classification. Nonnegative weights sum to at most one, with
unused weight representing ordinary precision. Each row's effective ability is
`strength × exp(sum(demand[i] × ln(skill[i])))`; this scales error frequency
and bad-component width with the existing bounds. Quiet gaps reduce load.
The whole-chart analytical anchor is chosen before applying personality, so it
does not compensate away the selected weaknesses. At an absolute level
(`cpu-levels-v1` and `cpu-levels-v2`, below) the axes multiply the limits their patterns meet
instead (a streamer keeps up with faster streams, a jacker with faster
jacks), with nothing normalized to the chart.

## An opponent's ten levels

A multiplayer stage's CPU opponents (`ddi_engine::cpu::Opponent`,
`docs/plans/multiplayer-prototype-progress.md`) play at one of ten levels,
shown as five stars and never as a number (`ddi_ui::stars`). Since
2026-11-13 the levels are absolute (`cpu/absolute.rs`; `CPU_LEVELS` names
the scale opponents play at), since 2026-11-17 (round V, room model
revision 14) `cpu-levels-v2` (`ddi_engine::cpu::CPU_LEVELS_V2`, played by
`cpu-model-v2`, `ABSOLUTE_V2`): every multiplayer CPU (practice, online
rooms computed on every device, the bots' CPUs, the audience's, the
studies) and the solo CPU at a level play them. `cpu-levels-v1`
(`CPU_LEVELS_V1`, `ABSOLUTE_V1`) is retired from play: nothing a room,
the solo CPU or a play reads it; it is kept by name, pressing exactly as
it did, for the studies' comparison (`DDI_CPU_SCALE=v1`,
`DDI_LEVELS_SCALE=v1`), as the earlier levels (`cpu-levels-v0`,
`CPU_LEVELS_V0`, below) are kept for comparison and as the solo CPU's
five strengths.

### Why absolute

The adaptive-charts round measured that a `cpu-levels-v0` CPU's strength
is relative to whatever chart it plays: a level-4 CPU played a Challenge
chart about as well as a Beginner one, so under Adaptive charts nearly
every CPU from level 4 climbed to the hardest chart. The maintainer
(2026-11-13): "rework the CPU model so that its performance is 'absolute'
and not relative to the chart being played". A `cpu-levels-v1` CPU has
abilities in real units, a row's demand is read from the chart in real
units, and nothing is fitted per chart: an easy chart is played nearly
perfectly by a middling CPU and a chart above its head breaks it.

### The model (`cpu-levels-v2`, PROVISIONAL, round V)

What a level is, since 2026-11-17 (the maintainer: "Go ahead with
re-measuring the CPU levels' and re-adjusting them, including the wider
timing within accuracy (for sloppy combos) to increase its human-like
behavior"; the round's measurements and fit below, "Round V"). A row's
demand is read exactly as `cpu-levels-v1` reads it (the next section:
`RowDemand`, the plan's demands made once with `ABSOLUTE_V1`'s shape), and
a CPU's abilities are still in real units, the same on every chart
(`Ability`): its **spread** (now about its own lean), its **speed**, its
**floor**, and two new ones, its **lean** and its **drift**. How it plays
(`cpu-model-v2`, `ddi_engine::cpu::ABSOLUTE_V2`, `cpu/absolute.rs`'s
`Personal`):

- **Timing = a steady lean + a spread about it.** Each play of a CPU has
  a lean, drawn from its seed (a standard draw of four keyed uniforms,
  Irwin–Hall, times its lean's standard deviation, `lean_ms`; within 2.5
  of them; to a tenth of a millisecond): early or late, as a person whose
  calibration is off is. The same seed leans the same on every chart it
  plays (every rung of a ladder). About it the lean drifts: a knot every
  16 s, each its own draw times `drift_ms`, a straight line between.
- **The spread about the lean** is the level's on a comfortable row, from
  a law of two normals on the plan's grid as before (98% of presses at
  the spread, 2% three times as wide: a far thinner tail than v1's 12% at
  2.5 times), times the row's widening, times the play's own factor:
  - it **widens from 0.65 of a limit** (v1: 0.85), one and a third as wide
    at the limit and two thirds wider at 1.3 times it (`widen` 1.0), so a
    loaded CPU's hits fall in W2 and W3 while its combo holds;
  - a **play's form**: the spread times `1 + 0.35 z` for a standard draw
    `z` (within ±2), halved where `z` is below 0: some plays much looser,
    a few a little tighter (people's plays vary far more than sampling
    makes them, below);
  - **a stumble** (a note left out, or a press more than 90 ms off its
    note as aimed) doubles the spread, easing back over two seconds:
    loose presses and breaks come in bursts;
  - **fatigue**: a quarter wider by the play's last row, from nothing at
    its first.
- **Notes are left out from the limit** (v1: from 0.85 of it), on
  `cpu-model-v1`'s curve past it (`floor + 0.067 x + 0.1 x²`), and as ever
  at the level's floor; the larger of its two loads (speed, a lane's) as
  before. So misses come from load (dense passages, a lane repeating) and
  lapses, rare at rest; breaks (a W4, a W5, a Miss, a mine) come from the
  spread loosening (load, a bad day, a stumble's burst, the end of a song)
  and the lean pushing one side out.
- **A press is aimed within 180 ms of its note** (the law's own bound),
  lean and drift included, as every v1 press was: it is judged against
  its note or left out, never a stray press (a Boo where Boos count).
- **Determinism**: every new draw is keyed by the seed (`Purpose::Lean`,
  `Drift`, `Form`), never taken from the motor's stream, and the new
  arithmetic is sums, products, divisions and a square root (correctly
  rounded everywhere); the law's cache key (tenths of a millisecond) as
  before. The same chart, level, seed, skills and shocks press the same
  at any frame rate, natively and in wasm32 (the online CPUs' fixture
  recorded again and checked in wasm32-wasip1).
- **What it feeds**: the full chart's holds and rolls (`hold-model-v1`)
  are played as before, with the new timing where a note's timing law is
  used (heads, roll taps, a lift's release) and the new lapse curve where
  a hold may be let go (its chance is a share of the row's lapse past the
  onset, which is now the limit: holds are let go a little less at a
  given load). The chart's mines and a battle's shocks are met with
  `MINE_RESPONSE_V1` by the level's index, unchanged.
- **`cpu-levels-v1` is `cpu-model-v1` with none of this** (no lean, no
  drift, no form, no burst, no fatigue, the spread's onset the misses'):
  it presses exactly as before (checked against the CPUs' fixture recorded
  under it, `the_cpus_play_alike`, when v2 was built).

### The ten levels (`CPU_LEVELS_V2`, PROVISIONAL, round V)

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Spread about its lean (ms) | 32 | 30 | 27 | 25 | 22.5 | 20.5 | 17.5 | 14.5 | 11.5 | 8 |
| Speed (notes/s) | 2.5 | 3.1 | 3.8 | 4.6 | 6.0 | 7.2 | 8.1 | 8.9 | 10.1 | 12.0 |
| Floor (%) | 0.4 | 0.3 | 0.2 | 0.15 | 0.1 | 0.08 | 0.06 | 0.04 | 0.03 | 0.02 |
| Lean, standard deviation (ms) | 16 | 14 | 13 | 12 | 12 | 11 | 8 | 6 | 4 | 2 |
| Drift, standard deviation (ms) | 5 | 5 | 5 | 4 | 4 | 4 | 3 | 3 | 2 | 1.5 |

Fitted to targets stated before the fit (round V, below) over the local
arcade mixes under DDI Normal, the shape from one player's kept plays. In
words, on the charts each is at home on (by the chart's notes a second;
accuracy / Percent of the plays that stay in): level 1 clears charts
under 2 notes a second with loose hits and several breaks (70 / 66, a B
or a C, a full combo in 6% of them) and is out on most of the next band;
level 3 clears 2 to 3 notes a second (75 / 71); levels 5 and 6 play the
3 to 4.5 notes a second band like an ordinary person (80 / 76 and 84 /
81: an A, a full combo in 6% and 13% of plays, most of them below 90%
accuracy, 38% of the taps in W2), level 6 the 4.5 to 6 band too (77 /
73, out 4%); level 8 is an S on 4.5 to 6; level 10 near-perfect on every
band (96 to 98, W0 and W1 87% of its taps, a W2 here and there, a break
on the hardest passages: 0.09 a hundred notes, 0.24 on 6 notes a second
and more). The stars mean the same thing as before (a level's number in
a stored roster or a kept practice roster means the new level).

### The model (`cpu-levels-v1`, retired from play since round V)

A CPU's abilities, the same on every chart (`Ability`):

- **Spread** (milliseconds): the standard deviation of its presses'
  offsets on a comfortable note. Its timing law is a mixture: 88% of
  presses from a normal law of that spread, 12% from one 2.5 times as
  wide (a press now and then far off).
- **Speed** (notes a second): the rate it keeps up with before it
  degrades. A repeated lane's limit is 0.65 of it (presses a second on
  one lane), a sustained stretch's 0.75 of it (notes a second over eight
  seconds).
- **Floor**: the chance it leaves a note out where nothing is hard.

A row's demand, read from the chart (`RowDemand`, at plan time, never
normalized by the chart's own average or peak):

- **speed**: 0.7 × the notes in the second before the row + 0.3 × the
  rate of the gap since the row before (rows a second, at most 20: rows
  at one time after a warp are not infinitely fast);
- **sustain**: the notes in the eight seconds before the row, a second;
- **lane**: the rate of its busiest lane (one over the time since that
  lane's last note);
- **irregular** (0 to 1): the gaps before and after the last row
  changing by a factor of two or more is 1 (both within 0.3 s).

How it plays a row (`ABSOLUTE_V1`): its **load** is speed over its
speed limit plus 0.7 × how far sustain is over its sustained limit; its
lane load is lane over its lane limit. The seven pattern axes of
`Skills` multiply the limit their pattern meets: Stream, Jumpstream and
Handstream the speed limit of a row of one, two, three or more notes;
Stamina the sustained limit; JackSpeed and Chordjack a single or chorded
row's lane limit; Technical divides the widening of an irregular rhythm
(at most 0.35). Then:

- its **spread** is its own × (1 + 3 × the load past 0.85) × the
  rhythm's widening, at most 150 ms;
- its **lapse** (the chance the note is left out) is its floor + 0.067 ×
  x + 0.1 × x², where x is the larger of the two loads past 0.85, at most
  0.95: about one note in a hundred at the limit, one in twenty at 1.3
  times it, three in five at three times it.

The motor and the plan's bounds are as before (three draws a note, the
third deciding a lapse, 12 ms holds, the same grid of 89 edges); the
timing law of a row is made on that grid and cached by its spread in
tenths of a millisecond. The arithmetic is the plan's own (sums,
products, divisions, the irregularity's logarithm and the normal tail's
exponential, as the skills' laws already used), and the proof that the
online CPUs play alike passes natively and in wasm32-wasip1 with its
fixture recorded again (`ddi-session` `the_cpus_play_alike`).

**The shape of the fall-off** (where it starts, how fast the spread widens
and the misses rise, the lane and sustained shares, the tail) was checked
against one player's local play history, judged again from the kept
plays' logs on the machine it was kept on; that history is not included
in the repository in any form. The player is one player at one level: the
shape is the reference, and the levels themselves are rungs chosen by
measurement over charts, never against people. A level is what a CPU
keeps up with, not a skill rating.

### The ten levels (`CPU_LEVELS_V1`, retired from play since round V)

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Spread (ms) | 17 | 14 | 11.5 | 10.5 | 9.5 | 8.8 | 8 | 7.3 | 6.6 | 5.5 |
| Speed (notes/s) | 2.2 | 2.6 | 3.0 | 3.45 | 3.95 | 4.7 | 5.5 | 6.4 | 7.3 | 9.0 |
| Floor (%) | 0.4 | 0.3 | 0.2 | 0.15 | 0.1 | 0.08 | 0.06 | 0.04 | 0.03 | 0.02 |

Anchored by measurement over the local arcade mixes (the starting points
of 2026-11-13, the maintainer's to adjust), under DDI Normal: "at home"
on a slot is staying in at about 85 to 92%, a slot above is struggling
(often out, 60 to 75%), a slot below is 95% or more.

| Level | At home on | Measured there (percent, out) |
| --- | --- | --- |
| 1 | Beginner, lost on Medium | Beginner 86.9%, 1.5% out; Medium 40.6%, 78% out |
| 3 | Easy | 90.3%, 3.2% out (Beginner 93.7%, Medium 73.3%) |
| 5 | Medium | 89.4%, 7.0% out (Easy 95.7%, Hard 70.0%) |
| 7 | Hard | 89.4%, 14% out (Medium 96.6%, Challenge 74.5%) |
| 9 | Challenge | 90.4%, 18% out (Hard 96.6%) |
| 10 | Challenge nearly perfectly | 96.4%, 2.2% out; the stream packs' Challenge 88% |

Levels 2, 4, 6 and 8 lie between. The steps were found in four rounds
over a quarter of the corpus, then measured over all of it (below).
Two things fall short of the targets and are left for the maintainer:
level 3 scores 93.7% on Beginner where "a slot below" asks 95%, and a
level at home goes out on the hardest charts of its slot more as the
slots get denser (7% of Medium charts at level 5, 18% of Challenge at
level 9): the slot is wide.

### Measured (`cpu-levels-v1`, 2026-11-13, under DDI before round U)

`DDI_CPU_CORPUS=<dir>[:<dir>…] DDI_CPU_RULES=ddi-easy,ddi-normal,ddi-hard
cargo test --release -p ddi-engine --test local_cpu -- --nocapture`
(`crates/engine/tests/local_cpu.rs`; the opponents' levels by default,
`DDI_CPU_SCALE=v0` the earlier ones, candidates through
`DDI_CPU_ABILITIES=<spread>:<speed>:<floor>,…`, a quicker look over every
n-th simfile with `DDI_CPU_SAMPLE=n`): every dance-single chart as
written, read in place, on its tap-only copy, the fail off (the percent of
the whole chart; "out" its gauge emptying at any point). Measured on
2026-11-13 over the thirteen arcade mixes of the maintainer's local
library (1,094 simfiles, 4,542 dance-single charts), eight seeds a chart
and level; aggregates only.

DDI Normal (the rooms' default), `cpu-levels-v1`, by difficulty slot:

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| All (4542): percent | 48.3 | 60.7 | 71.0 | 78.4 | 84.7 | 90.2 | 93.8 | 96.1 | 97.5 | 98.8 |
| All (4542): out % | 54.0 | 42.8 | 33.0 | 24.0 | 16.4 | 10.3 | 6.6 | 3.5 | 1.7 | 0.2 |
| Beginner (1011): percent | 86.9 | 90.7 | 93.7 | 94.9 | 96.3 | 96.9 | 97.5 | 98.0 | 98.5 | 99.1 |
| Beginner (1011): out % | 1.5 | 0.4 | 0.1 | 0.1 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Easy (1068): percent | 72.6 | 83.6 | 90.3 | 93.4 | 95.7 | 96.9 | 97.6 | 98.2 | 98.6 | 99.1 |
| Easy (1068): out % | 21.3 | 8.4 | 3.2 | 1.3 | 0.4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Medium (1068): percent | 40.6 | 59.0 | 73.3 | 82.6 | 89.4 | 94.2 | 96.6 | 97.9 | 98.5 | 99.1 |
| Medium (1068): out % | 78.2 | 51.2 | 30.2 | 15.8 | 7.0 | 1.8 | 0.4 | 0.1 | 0.0 | 0.0 |
| Hard (1068): percent | 11.1 | 27.6 | 43.8 | 57.8 | 70.0 | 81.8 | 89.4 | 94.0 | 96.6 | 98.7 |
| Hard (1068): out % | 98.2 | 92.0 | 77.8 | 58.5 | 40.1 | 24.4 | 14.0 | 5.1 | 1.8 | 0.2 |
| Challenge (327): percent | -3.4 | 7.0 | 19.2 | 31.9 | 45.3 | 61.6 | 74.5 | 84.1 | 90.4 | 96.4 |
| Challenge (327): out % | 99.4 | 98.5 | 94.0 | 85.9 | 72.7 | 57.6 | 44.6 | 32.0 | 17.8 | 2.2 |

The same charts under `cpu-levels-v0` (relative to the chart):

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| All (4542): percent | 60.5 | 66.6 | 71.9 | 76.6 | 81.0 | 85.9 | 90.8 | 92.9 | 94.4 | 95.8 |
| All (4542): out % | 23.8 | 13.6 | 6.1 | 0.6 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Beginner (1011): percent | 66.9 | 73.3 | 77.7 | 81.9 | 85.6 | 89.0 | 92.9 | 94.4 | 95.4 | 96.3 |
| Medium (1068): percent | 58.8 | 64.9 | 70.7 | 75.6 | 79.9 | 84.9 | 90.3 | 92.6 | 94.2 | 95.7 |
| Challenge (327): percent | 51.7 | 57.2 | 62.8 | 68.3 | 74.3 | 81.8 | 87.3 | 90.1 | 92.5 | 94.7 |
| Challenge (327): out % | 81.3 | 63.5 | 41.1 | 5.8 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

From Beginner to Challenge, under v0 level 1 loses 15 points, level 5
11 and level 10 two, and level 5 never goes out; under v1 level 1 loses
90, level 5 51 and level 10 three, and every level from 1 to 9 has
charts it is lost on. Beginner charts no
longer cost a strong level anything (v0's level 10 scored 96.3% on them,
its timing law widened on the easiest notes; v1's 99.1%).

DDI Easy and Hard (`cpu-levels-v1`), in short (percent / out %):

| Slot | Easy timing: levels 1 / 3 / 5 / 7 / 9 / 10 | Hard timing: levels 1 / 3 / 5 / 7 / 9 / 10 |
| --- | --- | --- |
| Beginner | 90.6/0 · 96.3/0 · 98.0/0 · 98.7/0 · 99.1/0 · 99.4/0 | 84.3/6.8 · 91.6/0.6 · 94.4/0.1 · 96.4/0 · 97.7/0 · 98.7/0 |
| Easy | 77.6/1.6 · 93.2/0.1 · 97.4/0 · 98.7/0 · 99.2/0 · 99.5/0 | 68.8/44 · 87.9/9.9 · 93.9/1.7 · 96.5/0 · 97.9/0 · 98.8/0 |
| Medium | 46.4/25 · 77.4/3.6 · 91.9/0.4 · 97.9/0 · 99.1/0 · 99.5/0 | 36.3/95 · 70.0/54 · 87.2/19 · 95.4/2.0 · 97.8/0.2 · 98.7/0 |
| Hard | 15.0/72 · 48.1/31 · 73.4/11 · 91.3/0.7 · 97.5/0 · 99.1/0 | 8.2/100 · 40.6/93 · 67.3/63 · 87.7/25 · 95.7/6.2 · 98.2/1.1 |
| Challenge | -1.2/91 · 22.2/64 · 48.4/41 · 76.9/8.3 · 91.7/0.4 · 97.1/0 | -5.0/100 · 16.9/99 · 42.8/88 · 72.4/58 · 89.1/34 · 95.7/12 |

By the simfile's meter (the older mixes' scale of ten and the later one
of twenty together; the meter is never an input to the model), DDI
Normal:

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Meter 1 to 3 (982): percent | 87.4 | 91.1 | 93.9 | 95.1 | 96.4 | 96.9 | 97.5 | 98.0 | 98.5 | 99.1 |
| 4 to 6 (1106): percent | 72.6 | 83.6 | 90.3 | 93.5 | 95.8 | 96.9 | 97.6 | 98.2 | 98.6 | 99.1 |
| 4 to 6 (1106): out % | 21.1 | 8.4 | 2.0 | 0.5 | 0.1 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 7 to 9 (1010): percent | 44.3 | 62.7 | 76.5 | 85.2 | 91.2 | 95.2 | 97.1 | 98.0 | 98.6 | 99.1 |
| 7 to 9 (1010): out % | 76.9 | 44.0 | 23.1 | 11.4 | 3.5 | 0.4 | 0.1 | 0.1 | 0.1 | 0.0 |
| 10 to 12 (734): percent | 19.5 | 40.1 | 58.2 | 71.8 | 82.6 | 91.4 | 95.8 | 97.6 | 98.4 | 99.1 |
| 10 to 12 (734): out % | 99.2 | 95.0 | 72.4 | 36.7 | 10.2 | 1.1 | 0.6 | 0.5 | 0.3 | 0.0 |
| 13 to 15 (470): percent | -5.0 | 7.7 | 24.1 | 40.7 | 56.5 | 73.1 | 84.8 | 92.3 | 96.2 | 98.7 |
| 13 to 15 (470): out % | 100.0 | 100.0 | 99.9 | 98.0 | 83.6 | 45.9 | 14.2 | 1.8 | 0.1 | 0.0 |
| 16 and more (240): percent | -13.9 | -10.5 | -4.1 | 6.0 | 19.7 | 39.8 | 58.0 | 73.0 | 83.5 | 94.2 |
| 16 and more (240): out % | 100.0 | 100.0 | 100.0 | 100.0 | 100.0 | 100.0 | 94.9 | 61.1 | 30.7 | 3.8 |

By the chart's mean notes a second:

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Under 1 (416): percent | 89.2 | 91.7 | 94.2 | 95.0 | 96.3 | 96.7 | 97.4 | 97.9 | 98.4 | 99.0 |
| 1 to 2 (1175): percent | 84.9 | 90.4 | 93.7 | 95.1 | 96.4 | 97.0 | 97.6 | 98.1 | 98.5 | 99.1 |
| 2 to 3 (1005): percent | 60.6 | 76.9 | 87.3 | 92.4 | 95.5 | 96.9 | 97.7 | 98.2 | 98.7 | 99.1 |
| 2 to 3 (1005): out % | 49.8 | 10.3 | 1.5 | 0.3 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 3 to 4.5 (1224): percent | 23.2 | 44.1 | 61.9 | 74.9 | 84.9 | 92.6 | 96.3 | 97.8 | 98.5 | 99.1 |
| 3 to 4.5 (1224): out % | 100.0 | 91.4 | 62.1 | 30.0 | 7.5 | 0.6 | 0.0 | 0.0 | 0.0 | 0.0 |
| 4.5 to 6 (473): percent | -6.6 | 6.2 | 23.1 | 39.9 | 55.9 | 72.8 | 84.8 | 92.4 | 96.1 | 98.6 |
| 4.5 to 6 (473): out % | 100.0 | 100.0 | 100.0 | 99.7 | 85.4 | 44.9 | 14.4 | 2.7 | 0.5 | 0.1 |
| 6 to 8 (230): percent | -14.3 | -10.9 | -4.0 | 6.9 | 21.5 | 42.1 | 60.2 | 75.0 | 85.2 | 95.1 |
| 6 to 8 (230): out % | 100.0 | 100.0 | 100.0 | 100.0 | 100.0 | 99.9 | 92.3 | 55.7 | 25.1 | 2.3 |
| 8 and more (19): percent | -16.0 | -14.8 | -12.6 | -8.6 | -0.8 | 16.3 | 36.3 | 55.0 | 69.5 | 87.0 |

(A percent below zero is a play out early with misses, each of which
DDI scores below nothing.)

**The spread across seeds** (a chart's standard deviation of the percent
over its eight seeds, averaged over the charts; DDI Normal): v1 2.2
points at level 1 down to 0.4 at level 10 over all charts (v0: 3.0 to
0.9); on Beginner 2.5 to 0.6 (v0: 4.3 to 1.3), on Hard 1.7 to 2.0 at
levels 1 to 5, where those levels are past their limits, and 0.3 at
level 10. Every table above averages eight seeds (the bundled songs
sixteen).

The stream packs (eleven keyboard and stream packs, 356 simfiles, 1,622
charts, eight seeds, DDI Normal), v1 beside v0: their Challenge charts
88.1% and 30% out at level 10 (v0: 94.3%, never out), 75.5% and 49% out at
level 9; their Edits 84.5% and 51% out at level 10. A third of their Easy
charts put every level up to 6 out (v0 too, at levels 1 to 3): charts
named Easy that are not. The bundled songs (19 charts, too few to choose
by): 52.9 / 73.1 / 85.7 / 94.6 / 97.9 / 99.1% at levels 1 / 3 / 5 / 7 / 9
/ 10 under DDI Normal, out 51 / 35 / 12 / 5.9 / 0 / 0%.

**A level watched** (`a_level_watched`, opt-in: `DDI_WATCH_LEVEL=3 cargo
test --release -p ddi-engine --lib a_level_watched -- --ignored
--nocapture`; four seeds, DDI Normal, out as a stage puts it out): a
level-3 CPU on Beatucada plays its Beginner (meter 1) at 93.7 to 95.8%,
a handful of its 38 notes off the top tier and one missed in one seed;
its Easy (3) at 93.6 to 96.2%; its Medium (7) at 89.3 to 92.6%, two to
seven misses of 217, never out; its Hard (11) out at 15 s in two seeds
and at 56 s in a third (6.7 to 37%), 72.7% the one seed it stayed in; its
Challenge (11, its densest stretches shorter) out at 70 s in two seeds
(44 to 47%), 68.5 and 71.9% where it stayed in, with 37 and 33 misses.

**Determinism and cost.** The same chart, level, seed, skills and shocks
give the same presses at any frame rate, natively and in wasm32 (the
online CPUs' proof, its fixture recorded again), as far as the maths
libraries' `exp`, `ln` and `powf` agree: natively the platform's, in
wasm Rust's port of musl's, which may differ in the last bit; the model
quantizes what they feed before it decides (review finding engine-4,
`cpu/absolute.rs`'s note), and the room keeps a CPU's first report where
two devices differ. Planning
(`planning_cost`, opt-in: `cargo test --release -p ddi-engine --lib
planning_cost -- --ignored --nocapture`, and with `--target wasm32-wasip1`
under a WASI runner; the 31 bundled single charts, 286 rows on average):
a plan 141 µs natively and 223 µs in wasm (the same plan for both
models: v1's demands are made with it); a CPU's presses 57 µs (v1 level 8)
against 50 µs (v0 level 8) and 90 µs (v1 level 3) against 233 µs (v0
level 3, whose laws are made a row at a time), in wasm 79 / 65 / 116 /
264 µs. 72 CPUs a stage (`adaptive_cost_of_72_cpus`, Beatucada's Medium,
release): loaded in 30 ms as picked and 115 ms with a full ladder (48 and
114 ms under v0), played out in 74 and 198 ms (81 and 179 ms); a frame of
72 online CPUs 0.013 ms mean, 1.0 ms at most (`what_computing_the_cpus_costs_a_frame`).

### The earlier levels (`cpu-levels-v0`, kept)

- **Levels 6 to 10 are the five strengths**, unchanged: `relaxed` to
  `expert`, 0.75 to 1.25. The same chart, seed and skills give the same
  presses as before there were ten (a fixture of press hashes taken before
  the change: `levels_six_to_ten_press_exactly_as_the_five_strengths_did`).
- **Levels 1 to 5 are weaker**, by a lower strength and a *lapse*. Strength
  alone stops weakening a CPU at about 0.3: the timing law's width is
  bounded at 80 ms and its share at every note, and a wide press is still
  a judged press, so such a CPU scores about 47% with no misses and its
  gauge almost never empties, on a Beginner chart as on a Challenge one.
  The lapse is the chance a note is left out: the level's lapse times the
  plan's frequency at the note (0.1 for a note on its own, up to 1 where
  rows are a tenth of a second apart, a lane repeats or three arrows land
  together), bent by the CPU's skills as its ability is. So a weak level
  misses a few notes of an easy chart and many of a chart above its head,
  where its gauge empties. The motor's three draws per note are the same
  (the third decides a lapse), so a level without lapse is exactly the
  strength it names.

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Strength | 0.40 | 0.47 | 0.54 | 0.61 | 0.68 | 0.75 | 0.9 | 1.0 | 1.1 | 1.25 |
| Lapse | 0.20 | 0.19 | 0.16 | 0.12 | 0.07 | 0 | 0 | 0 | 0 | 0 |

PROVISIONAL. The values were chosen by measurement, never against people:
strengths in even steps down from 0.75, each lapse set so that the mean
percent under the rooms' ruleset steps down about evenly from level 6
(about nine points a level) and level 1 is one a beginner beats: about
half the points of a Beginner chart, out of about a quarter of them, and
out of nearly every Hard one.

#### Measured (`cpu-levels-v0`)

`DDI_CPU_CORPUS=<dir>[:<dir>…] cargo test --release -p ddi-engine --test
local_cpu -- --nocapture` (`crates/engine/tests/local_cpu.rs`; candidates
through `DDI_CPU_LEVELS=<strength>:<lapse>,…`) plays every dance-single
chart as written under local folders, read in place, on its tap-only copy,
with an opponent at each level, to the song's end with the fail off: the
percent is of the whole chart, "out" is the gauge emptying at any point (a
stage would end the play there). It prints aggregates only. Third-party
songs used this way stay local.

Measured on 2026-11-12 over twelve arcade mixes in the maintainer's local
library (724 simfiles, 2,974 dance-single charts of every difficulty),
sixteen seeds a chart and level. A seed's draws are the same on every
chart (the first notes' lapses fall alike), so a few seeds mislead: one
weak candidate's mean over all charts ran from 22% to 40% between single
seeds; sixteen average it out.

ITG FA+ (the rooms' ruleset), by difficulty slot (charts):

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| All (2974): percent | 39.9 | 49.5 | 58.9 | 67.8 | 76.5 | 86.2 | 91.9 | 94.2 | 95.8 | 97.3 |
| All (2974): out % | 68.9 | 60.1 | 44.0 | 23.5 | 3.7 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Beginner (641): percent | 53.7 | 62.2 | 69.5 | 76.2 | 82.5 | 88.8 | 93.7 | 95.4 | 96.6 | 97.7 |
| Beginner (641): out % | 23.5 | 12.1 | 5.9 | 0.9 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Easy (699): percent | 50.3 | 59.0 | 66.9 | 74.6 | 81.5 | 88.3 | 93.4 | 95.2 | 96.5 | 97.6 |
| Easy (699): out % | 49.0 | 32.8 | 16.3 | 3.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Medium (699): percent | 37.3 | 47.2 | 57.7 | 67.1 | 75.8 | 85.5 | 91.6 | 94.1 | 95.7 | 97.2 |
| Medium (699): out % | 90.5 | 82.0 | 49.3 | 17.4 | 0.5 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Hard (699): percent | 26.4 | 37.4 | 48.4 | 59.0 | 70.0 | 83.5 | 90.1 | 93.0 | 95.0 | 96.9 |
| Hard (699): out % | 98.6 | 96.5 | 84.3 | 51.9 | 6.6 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Challenge (236): percent | 19.5 | 29.8 | 41.3 | 53.1 | 66.5 | 82.9 | 89.2 | 92.2 | 94.5 | 96.7 |
| Challenge (236): out % | 99.7 | 99.0 | 94.9 | 79.3 | 25.4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| All (2974): life at the end % | 13.7 | 20.5 | 33.8 | 56.2 | 86.5 | 99.2 | 99.8 | 99.8 | 99.9 | 99.9 |
| All (2974): notes missed % | 5.8 | 5.5 | 4.6 | 3.4 | 2.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

("Life at the end" counts an emptied gauge as none.) By the chart's mean
notes a second, which compares across the mixes' two meter scales:

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Under 1 note a second (183): percent | 54.9 | 62.9 | 70.0 | 76.4 | 82.6 | 88.9 | 93.6 | 95.3 | 96.6 | 97.7 |
| Under 1 note a second (183): out % | 12.0 | 4.5 | 2.3 | 0.4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 1 to 2 (862): percent | 53.4 | 62.0 | 69.3 | 76.2 | 82.6 | 88.9 | 93.7 | 95.4 | 96.7 | 97.7 |
| 1 to 2 (862): out % | 29.0 | 15.2 | 7.7 | 1.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 2 to 3 (693): percent | 43.4 | 52.1 | 61.9 | 70.8 | 78.7 | 86.9 | 92.6 | 94.7 | 96.1 | 97.4 |
| 2 to 3 (693): out % | 79.8 | 64.5 | 28.3 | 5.5 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 3 to 4.5 (846): percent | 30.1 | 41.6 | 52.5 | 62.5 | 72.2 | 84.0 | 90.6 | 93.4 | 95.3 | 97.0 |
| 3 to 4.5 (846): out % | 98.7 | 95.9 | 77.4 | 33.9 | 0.7 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 4.5 to 6 (273): percent | 20.5 | 30.8 | 42.2 | 53.8 | 66.8 | 82.6 | 89.1 | 92.3 | 94.6 | 96.7 |
| 4.5 to 6 (273): out % | 100.0 | 100.0 | 99.5 | 90.0 | 13.3 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 6 to 8 (108): percent | 12.1 | 21.3 | 33.4 | 46.7 | 62.8 | 82.5 | 88.4 | 91.4 | 94.1 | 96.5 |
| 6 to 8 (108): out % | 100.0 | 100.0 | 100.0 | 99.9 | 55.3 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 8 and more (9): percent | 6.7 | 15.3 | 28.2 | 42.7 | 60.8 | 82.8 | 88.2 | 91.0 | 93.8 | 96.4 |
| 8 and more (9): out % | 100.0 | 100.0 | 100.0 | 100.0 | 88.9 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

By the simfile's meter (the older mixes' scale of ten and the later one of
twenty together):

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Meter 1 to 3 (617): percent | 53.9 | 62.3 | 69.6 | 76.3 | 82.5 | 88.8 | 93.7 | 95.4 | 96.6 | 97.7 |
| Meter 1 to 3 (617): out % | 21.8 | 10.8 | 5.2 | 0.7 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 4 to 6 (776): percent | 49.1 | 57.9 | 66.0 | 73.8 | 80.9 | 88.0 | 93.3 | 95.1 | 96.4 | 97.6 |
| 4 to 6 (776): out % | 51.4 | 35.4 | 18.2 | 3.6 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 7 to 9 (734): percent | 36.9 | 46.8 | 57.4 | 66.8 | 75.6 | 85.5 | 91.6 | 94.1 | 95.7 | 97.2 |
| 7 to 9 (734): out % | 91.7 | 83.4 | 49.6 | 16.8 | 0.5 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 10 to 12 (506): percent | 28.3 | 40.0 | 50.9 | 61.1 | 71.3 | 83.7 | 90.3 | 93.2 | 95.2 | 97.0 |
| 10 to 12 (506): out % | 99.3 | 97.4 | 85.4 | 43.8 | 1.2 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 13 to 15 (239): percent | 19.7 | 30.0 | 41.4 | 53.2 | 66.4 | 82.6 | 89.0 | 92.2 | 94.5 | 96.7 |
| 13 to 15 (239): out % | 100.0 | 100.0 | 99.7 | 91.4 | 16.1 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| 16 and more (102): percent | 11.3 | 20.4 | 32.7 | 46.1 | 62.6 | 82.7 | 88.4 | 91.4 | 94.0 | 96.5 |
| 16 and more (102): out % | 100.0 | 100.0 | 100.0 | 100.0 | 60.4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

DDI Normal over the same charts (its gauge forgives far more):

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| All (2974): percent | 61.4 | 67.5 | 72.6 | 77.4 | 81.7 | 86.1 | 91.0 | 93.1 | 94.5 | 95.9 |
| All (2974): out % | 20.0 | 9.6 | 3.8 | 0.3 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Beginner (641): percent | 67.9 | 74.3 | 78.7 | 82.9 | 86.4 | 89.5 | 93.2 | 94.7 | 95.6 | 96.5 |
| Beginner (641): out % | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Easy (699): percent | 66.2 | 72.4 | 77.2 | 81.7 | 85.4 | 88.7 | 92.7 | 94.4 | 95.4 | 96.4 |
| Easy (699): out % | 1.1 | 0.2 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Medium (699): percent | 59.9 | 65.9 | 71.5 | 76.4 | 80.8 | 85.3 | 90.6 | 92.9 | 94.4 | 95.8 |
| Medium (699): out % | 12.1 | 2.7 | 0.3 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Hard (699): percent | 54.9 | 61.0 | 66.5 | 71.7 | 76.8 | 82.8 | 88.7 | 91.4 | 93.4 | 95.2 |
| Hard (699): out % | 46.3 | 20.4 | 6.5 | 0.3 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Challenge (236): percent | 52.6 | 58.1 | 63.7 | 69.2 | 75.0 | 81.9 | 87.5 | 90.3 | 92.6 | 94.8 |
| Challenge (236): out % | 75.3 | 52.4 | 27.9 | 2.5 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

The bundled songs beside them (19 charts, too few to choose by) agree:

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Bundled, ITG FA+ (19): percent | 40.3 | 49.8 | 59.1 | 67.7 | 76.4 | 86.2 | 91.9 | 94.2 | 95.9 | 97.3 |
| Bundled, ITG FA+ (19): out % | 65.8 | 61.8 | 46.7 | 29.3 | 5.6 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Bundled, DDI (19): percent | 61.7 | 67.8 | 72.8 | 77.5 | 81.8 | 86.2 | 91.0 | 93.1 | 94.5 | 95.9 |
| Bundled, DDI (19): out % | 26.0 | 11.8 | 4.9 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

A cross-check of the hard end (six keyboard and stream packs, 134
simfiles, 570 charts, eight seeds, ITG FA+):

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| All (570): percent | 23.9 | 33.2 | 44.0 | 55.4 | 68.4 | 85.0 | 90.3 | 92.7 | 94.9 | 96.9 |
| All (570): out % | 84.5 | 79.1 | 73.1 | 64.4 | 54.8 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Hard (134): percent | 14.3 | 24.2 | 36.2 | 49.0 | 64.0 | 83.5 | 89.1 | 91.8 | 94.3 | 96.6 |
| Hard (134): out % | 99.6 | 99.3 | 94.9 | 84.0 | 62.4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Challenge (80): percent | 4.3 | 13.3 | 26.3 | 41.1 | 59.2 | 83.5 | 88.3 | 90.8 | 93.8 | 96.4 |
| Challenge (80): out % | 100.0 | 100.0 | 100.0 | 100.0 | 100.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |

What the tables say:

- A weaker level scores no better on average, in every row.
- The five strengths are close together in percent (86 to 97) and their
  gauges never empty on any chart, however dense: a strength is relative
  to the chart and never leaves a note out. Level 5 is out of a quarter of
  the arcade Challenge charts and of every stream pack's: the step from 5
  to 6 is the largest on hard charts. Respacing the five strengths would
  change the solo CPU's named strengths and the plays of existing seeds:
  left for a decision.
- Under ITG FA+ a gauge starts at half and a miss costs a tenth, so a few
  early lapses end a weak level's stage: its out rate depends on the first
  measures as much as on the chart. Under DDI the same levels survive most
  easy charts.
- Rooms play DDI alone since 2026-11-12 (the DDI ruleset's round 3). The
  same measurement under its three difficulties (`DDI_CPU_RULES=ddi-easy,
  ddi-normal,ddi-hard`), the levels not retuned, is in
  [the prototype's progress note](multiplayer-prototype-progress.md),
  "Measured: the ten CPU levels under DDI": percent 61.4 to 95.9 under
  Normal, out of the stage on a fifth of the charts at level 1 and never
  from level 5 on; never out under Easy; Hard about where ITG FA+ was.

## Round V (2026-11-17): the levels measured again and made more like a person

**The maintainer's words.** On the combo studies: "Maybe the 'sloppiness'
is something that could be added to the CPU, so we can get more human
behavior. Part of what I feel like causes it in practice is poor
calibration." Then: "Go ahead with re-measuring the CPU levels' and
re-adjusting them, including the wider timing within accuracy (for
sloppy combos) to increase its human-like behavior." Built:
`cpu-levels-v2` (the model and the levels above), room model revision 14
(every device computes the room's CPUs).

**How it was measured.** `cpu_levels_study` (`ddi-session`,
`crates/session/src/battle/combo_study/levels.rs`; `cargo test --release
-p ddi-session --lib cpu_levels_study -- --ignored --nocapture`, the
corpus with `DDI_LEVELS_CORPUS`, the kept plays with `DDI_LEVELS_PLAYS`,
candidates with `DDI_LEVELS_ABILITIES` and `DDI_LEVELS_MODEL`): every
dance-single chart as written of the thirteen arcade mixes of the
maintainer's local library (996 simfiles, 4,147 charts, read in place;
aggregates only), four seeds a chart and level, each chart its own seeds
(so that a level's leans are many), DDI Normal, the chart a person is
given, the fail off (the whole chart judged; "out" the gauge emptying at
any point). The same measures for the CPUs and for the reference, from a
play's judgments in order, its taps put on DDI Normal's grid by their
offsets: the accuracy and the Percent as round U has them, the letters,
the tiers (W0 to W5, Miss), misses and breaks (a W4, a W5, a Miss, a mine)
a hundred notes, full combos and how accurate they are, where along the
play breaks fall, how often a break follows a break, a play's lean (its
hits' mean offset) and spread about it, and how the spread and the breaks
grow with the notes in the second before a note. Grouped by the chart's
notes a second (comparable between a person's plays and the corpus: the
kept plays name no chart's slot) and by how the play did.

### The reference's shape (aggregates only)

The maintainer's kept plays, read in place, read-only (the desktop's
`plays/` folder alone; nothing of them in the repository but these
aggregates): 505 plays judged under DDI on Normal's grid (played under DDI
or judged again under it), not autoplayed, not quit; one player, mostly
on familiar charts: **its shape, not the level of any CPU**. Its charts
are mostly 3 to 4.5 notes a second (300 plays) and 4.5 to 6 (164).

- **Clears** (95% of plays): accuracy 82.2, Percent 79.2 (median 79.8),
  S 9.5%, A 60.2%, B 23.8%, C 1.6% of plays; 13.3% full combos, 62.5% of
  them below 90% accuracy (none below 70).
- **Tiers**: W0 21.9, W1 19.9, W2 38.6, W3 16.2, W4 2.2, W5 0.7, Miss 0.5%;
  3.4 breaks and 0.5 misses a hundred notes: **its breaks are loose hits
  (W4), not misses**. A break follows a break 30% of the time (3.4% of
  notes break): bursts.
- **Where breaks fall**: 7, 6, 8, 9, 10, 11, 11, 11, 11, 16% of them in
  each tenth of a play's notes: more toward the end.
- **Load**: the spread about the play's lean is about 30 ms up to 6 notes
  in the second before a note, 36 at 6 to 8, 56 at 8 to 11, 76 beyond;
  misses 0.1 to 0.2 a hundred notes up to 6, 0.4, 3.3 and 10.7 beyond:
  **hits loosen from about 6 notes a second, misses start at about 8**.
- **By how the play did**: at a given accuracy its tiers are what a
  spread gives (clears at 80 to 85%: W0+W1 42.5, W2 40.6, W3 14.9; at 90
  to 95%: 61.8, 34.1, 3.7), and full combos are 2% of the clears at 70 to
  80%, 5% at 80 to 85, 21% at 85 to 90 and 65% at 90 to 95.

**Calibration against skill.** A play's lean averages −9.5 ms (early;
10th, 50th and 90th percentiles −23.3, −9.5 and +3.4 ms; 11.7 ms in size
on average), varies 11.0 ms between plays (sampling alone would give 1.8)
and drifts within a play (its quarters' means 5.9 ms about it, sampling
alone 3.1). Between sessions (plays 45 minutes apart: 68 sessions) the
sessions' means vary 8.9 ms, within a session 8.6 ms: the lean changes
from play to play, not only from day to day. Every one of the seven audio
devices' plays lean early on average (−6 to −13 ms on the four with five
plays or more), and 23 distinct audio offsets were in effect: a
calibration that leaves the player a little early, set again often.
Taken away (each play's hits about its own lean), W0+W1 rises from 41.8
to 48.3% and W3 falls from 16.2 to 12.3%; the breaks barely move (W4 and
beyond 2.9 to 2.5%). Plays leaning 20 ms or more average 74.2% accuracy
against 79.5 to 82.9% for the rest. So a lean costs the top tiers, the
spread of about 30 ms is skill, and the breaks are mostly skill under
load. The model takes both: a lean of the CPU's own and a spread about
it.

### The levels before (`cpu-levels-v1`) under today's rule

Over the same charts, measured the same way (DDI Normal, round U's rule):

| All charts (4,147) | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | Person |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| plays | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 505 |
| out % | 54.2 | 42.3 | 32.8 | 25.7 | 17.3 | 10.7 | 7.3 | 3.8 | 1.8 | 0.2 | 4.8 |
| accuracy, whole chart | -12.8 | 16.5 | 41.1 | 56.4 | 70.4 | 82.1 | 89.7 | 94.5 | 96.3 | 98.7 | 79.3 |
| clears: accuracy | 75.2 | 77.0 | 85.5 | 86.6 | 89.1 | 91.6 | 95.0 | 96.7 | 97.1 | 98.7 | 82.2 |
| clears: Percent | 71.3 | 72.6 | 82.6 | 84.0 | 85.9 | 89.0 | 93.8 | 95.8 | 96.3 | 98.3 | 79.2 |
| S or better / A / B / C, D / F, % of plays | 6 / 16 / 13 / 11 / 54 | 5 / 28 / 13 / 12 / 42 | 32 / 17 / 9 / 9 / 33 | 39 / 17 / 9 / 9 / 26 | 48 / 20 / 9 / 6 / 17 | 59 / 19 / 7 / 3 / 11 | 77 / 9 / 4 / 2 / 7 | 86 / 6 / 3 / 1 / 4 | 89 / 6 / 2 / 0 / 2 | 96 / 3 / 0 / 0 / 0 | 10 / 60 / 24 / 2 / 5 |
| W0+W1 / W2 / W3 / W4+W5 / Miss, % | 18 / 14 / 11 / 7.9 / 47.9 | 26 / 18 / 12 / 7.9 / 36.4 | 38 / 18 / 10 / 4.9 / 28.0 | 46 / 19 / 10 / 4.3 / 20.6 | 56 / 19 / 8 / 3.4 / 14.0 | 66 / 18 / 6 / 2.1 / 8.0 | 76 / 14 / 4 / 1.3 / 4.6 | 83 / 12 / 2 / 0.7 / 2.3 | 88 / 10 / 1 / 0.4 / 1.4 | 95 / 4 / 0 / 0.1 / 0.4 | 42 / 39 / 16 / 2.9 / 0.5 |
| misses a hundred notes | 47.95 | 36.45 | 28.01 | 20.59 | 13.99 | 7.99 | 4.61 | 2.30 | 1.36 | 0.37 | 0.49 |
| breaks a hundred notes | 55.99 | 44.40 | 32.92 | 24.97 | 17.43 | 10.14 | 5.96 | 3.00 | 1.77 | 0.50 | 3.38 |
| full combos, % of clears | 13.4 | 6.6 | 33.4 | 38.3 | 24.4 | 39.6 | 66.6 | 75.0 | 77.8 | 83.7 | 13.3 |
| their accuracy (below 90, %) | 92.5 (1) | 93.7 (3) | 97.1 (0) | 96.7 (0) | 97.5 (0) | 97.4 (0) | 98.3 (0) | 98.7 (0) | 98.7 (0) | 99.3 (0) | 88.1 (62) |
| breaks in the first / last tenth, % | 7 / 11 | 7 / 11 | 6 / 12 | 6 / 12 | 5 / 12 | 6 / 12 | 5 / 15 | 3 / 17 | 4 / 16 | 6 / 19 | 7 / 16 |
| break after a break, % | 80 | 75 | 69 | 63 | 55 | 42 | 32 | 24 | 18 | 12 | 30 |
| lean (its mean size) / spread about it, ms | 4.6 / 59 | 3.2 / 49 | 2.8 / 35 | 2.2 / 30 | 2.3 / 25 | 1.5 / 20 | 0.9 / 15 | 0.8 / 12 | 0.6 / 11 | 0.5 / 8 | 11.7 / 32 |


By the chart's notes a second (accuracy / Percent of the plays that stay in / out % / full combos % of those), `cpu-levels-v1`:

| Notes a second (charts) | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | Person |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| under 2 (1441) | 82 / 78 / 0 / 18 | 86 / 82 / 0 / 11 | 95 / 94 / 0 / 61 | 96 / 95 / 0 / 74 | 95 / 93 / 0 / 45 | 96 / 95 / 0 / 70 | 98 / 98 / 0 / 99 | 99 / 99 / 0 / 100 | 98 / 99 / 0 / 100 | 99 / 99 / 0 / 100 |  |
| 2 to 3 (899) | 55 / 50 / 49 / 0 | 67 / 62 / 8 / 0 | 84 / 79 / 1 / 6 | 89 / 86 / 0 / 13 | 94 / 92 / 0 / 20 | 96 / 94 / 0 / 45 | 98 / 98 / 0 / 86 | 99 / 98 / 0 / 94 | 99 / 99 / 0 / 96 | 99 / 99 / 0 / 99 | 82 / 80 / 0 / 21 |
| 3 to 4.5 (1097) | out 100 | 44 / 41 / 89 / 0 | 58 / 53 / 58 / 0 | 66 / 61 / 32 / 0 | 78 / 73 / 7 / 0 | 88 / 84 / 1 / 5 | 95 / 93 / 0 / 33 | 98 / 97 / 0 / 62 | 98 / 98 / 0 / 72 | 99 / 99 / 0 / 85 | 84 / 82 / 3 / 19 |
| 4.5 to 6 (465) | out 100 | out 100 | out 100 | out 100 | 50 / 46 / 84 / 0 | 66 / 60 / 41 / 0 | 78 / 72 / 15 / 0 | 90 / 85 / 2 / 6 | 94 / 90 / 0 / 16 | 98 / 97 / 0 / 41 | 80 / 75 / 3 / 2 |
| 6 and more (244) | out 100 | out 100 | out 100 | out 100 | out 100 | 36 / 33 / 100 / 0 | 54 / 49 / 94 / 0 | 70 / 64 / 61 / 0 | 78 / 72 / 30 / 0 | 92 / 87 / 3 / 2 | 61 / 56 / 65 / 0 |

What they say: **v1's CPUs are far more precise than a person and break
the other way round.** Every v1 level's clears are at least as accurate as
the reference's, yet its breaks are misses (v1's level 5 misses 14 notes
a hundred, the reference 0.5); a weak level misses long before its hits
loosen (level 3 on 3 to 4.5 notes a second: W0+W1 34%, misses 17 a
hundred), and when a v1 CPU keeps its combo it is precise (no full combo
of any level below 90% accuracy but 3% of level 2's). Under round U's
rule (a miss −120) the levels' accuracy over every play fell (level 1
−12.8, level 5 70.4) and their documented percents (61 to 96, measured
before round U) no longer describe them.

### The targets (stated before the fit)

On the arcade mixes under DDI Normal, by the chart's notes a second
(accuracy / Percent of clears, out %, full combos % of clears), the shape
of every level the reference's (looser hits long before misses, breaks
in bursts, more toward the end, a loose full combo ordinary for the
middle levels on charts within their reach and rare for level 1), the
middle levels around the reference on its band:

| Level | At home on | There | The band above |
| --- | --- | --- | --- |
| 1 | under 2 notes/s | 70 / 63 (B), out under 5, full combos 2–5%, breaks 5–8 a hundred | 2–3: out 25–50% |
| 2 | under 2 | 74 / 68, out under 2, 5–8% | 2–3: out 10–25% |
| 3 | under 2 and 2–3 | 78 / 72 and 74 / 69, out under 8 | 3–4.5: out 30–60% |
| 4 | 2–3 | 78 / 73, out under 3, about 10% | 3–4.5: out 10–30% |
| 5 | 3–4.5 | 80 ± 2 / 76 ± 2 (an A), out under 8, 8–15%; W0+W1 40–45, W2 about 38, W3 about 15% | 4.5–6: out 20–40% |
| 6 | 3–6 (the reference's) | 3–4.5: 84 ± 2 / 81 ± 2, out under 3, 12–20%; 4.5–6: 80 / 75, out under 8 | 6 and more: out 40–70% |
| 7 | 4.5–6 | 85 / 82, out under 4, about 12% | 6 and more: out 25–45% |
| 8 | 4.5–6 | 89 / 87, out under 2, about 25% | 6 and more: out 10–25% |
| 9 | 6 and more | 88 / 85, out under 8; 4.5–6: 93 / 92, about 45% | |
| 10 | everything | 95 or more up to 6 notes/s, out under 3 beyond; W0+W1 90% or more | |

Every level at least as good as the one below on accuracy, Percent,
breaks and outs in every group of charts; the gaps wider at the bottom.
Leans of about 20 ms at level 1, 11 at level 6 and 2.5 at level 10;
breaks clustered (a break after a break 20 to 40% of the time at the
middle) and rising toward the end.

### The fit (`cpu-levels-v2`)

Six candidates, each over the whole corpus (35 s each on four threads):
the first gave level 6 the reference's tiers on its band but no full
combo at all (its breaks spread evenly: a 6% tail of wide presses); a
thinner tail, a play's form and bursts after a stumble brought full
combos back; level 6's spread grew faster with density than the
reference's (8 to 11 notes in a second: 84 ms against 56) until the
widening was softened (from 0.65 of a limit, `widen` 1.0); the speeds of
levels 5 to 10 rose (an ordinary player keeps up with 4.5 to 6 notes a
second on their charts), those of 1 to 4 barely. The result, measured the
same way:

| All charts (4,147) | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | Person |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| plays | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 16588 | 505 |
| out % | 53.2 | 41.0 | 27.8 | 17.5 | 8.2 | 3.8 | 1.5 | 0.3 | 0.0 | 0.0 | 4.8 |
| accuracy, whole chart | 1.0 | 28.8 | 50.4 | 64.5 | 76.4 | 81.7 | 86.6 | 90.5 | 94.0 | 97.5 | 79.3 |
| clears: accuracy | 66.4 | 70.4 | 74.1 | 77.1 | 80.7 | 83.3 | 87.1 | 90.6 | 94.0 | 97.5 | 82.2 |
| clears: Percent | 62.4 | 66.5 | 70.6 | 74.0 | 78.2 | 81.3 | 85.8 | 89.8 | 93.7 | 97.4 | 79.2 |
| S or better / A / B / C, D / F, % of plays | 0 / 7 / 20 / 19 / 53 | 0 / 15 / 27 / 17 / 41 | 2 / 27 / 30 / 14 / 28 | 4 / 39 / 30 / 10 / 18 | 10 / 53 / 24 / 5 / 8 | 18 / 58 / 17 / 3 / 4 | 40 / 51 / 7 / 1 / 2 | 65 / 32 / 2 / 0 / 0 | 88 / 11 / 0 / 0 / 0 | 99 / 1 / 0 / 0 / 0 | 10 / 60 / 24 / 2 / 5 |
| W0+W1 / W2 / W3 / W4+W5 / Miss, % | 11 / 14 / 16 / 23.0 / 35.9 | 15 / 18 / 20 / 22.9 / 23.5 | 21 / 24 / 22 / 19.1 / 13.8 | 26 / 29 / 22 / 15.1 / 7.3 | 34 / 34 / 20 / 8.9 / 2.5 | 40 / 37 / 17 / 5.1 / 1.0 | 49 / 36 / 11 / 2.3 / 0.6 | 59 / 33 / 6 / 0.8 / 0.3 | 72 / 25 / 2 / 0.2 / 0.1 | 87 / 12 / 0 / 0.0 / 0.1 | 42 / 39 / 16 / 2.9 / 0.5 |
| misses a hundred notes | 35.99 | 23.50 | 13.80 | 7.33 | 2.46 | 1.03 | 0.56 | 0.29 | 0.14 | 0.05 | 0.49 |
| breaks a hundred notes | 59.14 | 46.56 | 33.05 | 22.49 | 11.43 | 6.14 | 2.88 | 1.15 | 0.38 | 0.09 | 3.38 |
| full combos, % of clears | 4.3 | 6.6 | 11.0 | 14.8 | 21.1 | 28.0 | 40.5 | 54.8 | 70.9 | 86.8 | 13.3 |
| their accuracy (below 90, %) | 82.1 (99) | 83.5 (96) | 84.8 (93) | 85.8 (88) | 86.9 (80) | 88.0 (68) | 90.4 (40) | 92.6 (17) | 95.1 (3) | 97.8 (0) | 88.1 (62) |
| breaks in the first / last tenth, % | 7 / 11 | 7 / 11 | 6 / 12 | 5 / 13 | 5 / 14 | 4 / 15 | 4 / 17 | 4 / 18 | 5 / 19 | 6 / 16 | 7 / 16 |
| break after a break, % | 80 | 73 | 65 | 56 | 43 | 34 | 27 | 19 | 12 | 6 | 30 |
| lean (its mean size) / spread about it, ms | 14.4 / 85 | 12.4 / 73 | 11.4 / 60 | 10.4 / 50 | 10.0 / 39 | 9.2 / 32 | 6.6 / 26 | 5.1 / 21 | 3.4 / 16 | 1.8 / 11 | 11.7 / 32 |


By the chart's notes a second (accuracy / Percent of the plays that stay in / out % / full combos % of those), `cpu-levels-v2`:

| Notes a second (charts) | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | Person |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| under 2 (1441) | 70 / 66 / 0 / 6 | 75 / 72 / 0 / 11 | 79 / 76 / 0 / 21 | 81 / 79 / 0 / 29 | 83 / 82 / 0 / 40 | 85 / 84 / 0 / 50 | 88 / 88 / 0 / 63 | 92 / 92 / 0 / 76 | 95 / 95 / 0 / 88 | 98 / 98 / 0 / 95 |  |
| 2 to 3 (899) | 56 / 52 / 45 / 0 | 66 / 61 / 9 / 1 | 75 / 71 / 1 / 3 | 80 / 76 / 0 / 9 | 83 / 80 / 0 / 18 | 85 / 83 / 0 / 28 | 89 / 88 / 0 / 43 | 92 / 91 / 0 / 61 | 95 / 94 / 0 / 77 | 98 / 98 / 0 / 90 | 82 / 80 / 0 / 21 |
| 3 to 4.5 (1097) | 42 / 38 / 99 / 0 | 53 / 48 / 83 / 0 | 62 / 57 / 40 / 0 | 71 / 66 / 10 / 1 | 80 / 76 / 1 / 6 | 84 / 81 / 0 / 13 | 88 / 86 / 0 / 28 | 91 / 90 / 0 / 47 | 94 / 94 / 0 / 65 | 98 / 97 / 0 / 84 | 84 / 82 / 3 / 19 |
| 4.5 to 6 (465) | out 100 | out 100 | 49 / 45 / 100 / 0 | 55 / 50 / 80 / 0 | 69 / 64 / 22 / 0 | 77 / 73 / 4 / 2 | 84 / 81 / 1 / 10 | 89 / 87 / 0 / 22 | 93 / 93 / 0 / 50 | 97 / 97 / 0 / 77 | 80 / 75 / 3 / 2 |
| 6 and more (244) | out 100 | out 100 | out 100 | out 100 | 54 / 49 / 93 / 0 | 64 / 59 / 55 / 0 | 73 / 69 / 23 / 0 | 81 / 77 / 4 / 2 | 90 / 87 / 1 / 15 | 96 / 96 / 0 / 53 | 61 / 56 / 65 / 0 |

Against the targets: levels 2 to 9 are within them or at their edge;
level 1 breaks more than asked (10 a hundred notes on its band, 5 to 8
asked: its spread and lean together push one press in ten past W3) and
goes out on 2 to 3 notes a second a little more (45%); level 4 is a
little strong on 2 to 3 (80 / 76) and level 8 on 6 notes a second and
more (out 4%, 10 to 25 asked); level 10's W0+W1 is 87%, under the 90
asked (a W2 one tap in eight: "here and there" read generously). Level 6
against the reference on its band: 3 to 4.5 notes a second 83.6 / 81.1,
out 0.3%, full combos 12.9% (the reference 84.0 / 81.7, 2.7%, 18.8%),
tiers 44 / 39 / 15 / 1.7 / 0.2 (45 / 40 / 13 / 1.6 / 0.2); 4.5 to 6 77.1
/ 72.8, out 4.1%, 1.9% (79.7 / 75.4, 3.0%, 2.5%). At a given accuracy
the CPUs' tiers now match the reference's closely (clears at 70 to 80%:
W0+W1 32, W2 37, W3 26, W4+W5 4.8 against 33, 40, 23, 3.8; at 90 to 95%:
61, 35, 4 against 62, 34, 4), and a loose full combo is ordinary for the
middle levels (68% of level 6's full combos below 90% accuracy, 80% of
level 5's; the reference's 62%). Breaks rise toward the end (level 6:
4% of them in a play's first tenth, 15% in its last; the reference 7 and
16) and come in bursts (a break after a break 34% at level 6, the
reference 30%). Leans: 9 to 14 ms in size at levels 1 to 6, 1.8 at level
10 (the reference 11.7).

**The ordering test** (`local_cpu_order`, `ddi-engine`, opt-in:
`DDI_CPU_CORPUS=… DDI_CPU_ORDER=1 DDI_CPU_SEEDS=4 cargo test --release -p
ddi-engine --test local_cpu local_cpu_order -- --nocapture`): every group
of 30 charts or more (all, each slot, each meter range, each band of
notes a second, by holds: 21 groups over the arcade mixes), each level at
least as good as the one below on the accuracy and the Percent of the
whole chart (within 0.3 points), the breaks (within 0.2 a hundred notes)
and the outs (within half a point): **none out of order**. The bundled
charts' test (`absolute_levels_are_monotone_in_level_and_in_the_chart`, every
bundled single chart, within half a point over four seeds) passes too.

### What it does to multiplayer (measured, not retuned)

Life under DDI is drained by W5s (−5 on Normal), misses (−10), mines and
shocks (−5) and holds let go, and filled by W0 to W2 (+2) and W3 (+1); a
W4 does nothing. Looser hits with fewer misses keep a CPU in: the middle
levels go out much less, the weakest a little more (their looser hits
add W5s). Attack power is credited from the tiers (W0 and W1 two, W2
one): looser hits earn less. The ladder's step up reads the share of taps
in the three best judgments (W0 to W2): more W3s, fewer early steps up.

**Attack rates** (`attack_rates`, the six bundled charts of the study,
four seeds, DDI Normal, eight CPUs at one level, `cpu-levels-v1` →
`cpu-levels-v2`):

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Bars a minute | 1.09 → 0.83 | 1.44 → 1.05 | 1.79 → 1.30 | 2.03 → 1.60 | 2.38 → 1.97 | 2.74 → 2.28 | 3.03 → 2.64 | 3.41 → 3.03 | 3.62 → 3.36 | 3.89 → 3.73 |
| Attacks launched a minute | 0.60 → 0.42 | 0.87 → 0.56 | 1.34 → 0.89 | 1.53 → 1.18 | 1.87 → 1.53 | 2.16 → 1.87 | 2.38 → 2.20 | 2.83 → 2.55 | 3.04 → 2.77 | 3.28 → 3.13 |
| Out % | 67.7 → 76.0 | 66.7 → 66.7 | 64.6 → 51.0 | 52.6 → 27.6 | 25.5 → 16.1 | 17.2 → 4.2 | 16.7 → 1.0 | 1.0 → 0.0 | 0 → 0 | 0 → 0 |
| Life an attack received | 16.5 → 18.2 | 16.9 → 17.0 | 13.7 → 15.5 | 12.6 → 15.3 | 11.6 → 12.6 | 9.7 → 11.0 | 9.2 → 11.7 | 4.4 → 5.7 | 4.5 → 4.6 | 1.4 → 1.4 |

A mixed field (levels 1, 2, 3, 4, 5, 6, 8 and 10): bars a minute 2.60 →
2.17, attacks launched a stage 24.7 → 22.8, attacks launched a minute a
seat in 2.06 → 1.73, seats out 38.0% → 30.2%, attacks that knocked their
recipient out 0.3% → 1.5%. Under DDI Hard the same direction (level 5
1.50 → 1.34 attacks a minute, level 10 3.25 → 3.01). **Nothing breaks**:
attacks a stage fall by a twelfth, a weak CPU still goes out of most
stages and a strong one of none, a knockout by an attack stays rare.

**Steps on the Adaptive ladder** (`adaptive_step_up_study`, `adaptive-v5`
unchanged, every second song file of the arcade mixes, one seed, ten
CPUs one a level, DDI Normal, attacks on; from a Medium pick, steps up a
stage (of them early), steps up a minute, steps up that fell back within
sixteen bars, finished one rung below / on the pick / one above / two
above, %):

| Level | `cpu-levels-v1` | `cpu-levels-v2` |
| --- | --- | --- |
| 1 | 0.12 (0.06), 0.07/min, 50% back; 55 / 18 / 3 / 0 | 0.14 (0.04), 0.08/min, 53%; 53 / 27 / 4 / 0 |
| 2 | 0.46 (0.32), 0.27/min, 50%; 44 / 38 / 9 / 1 | 0.14 (0.07), 0.08/min, 41%; 41 / 46 / 6 / 0 |
| 3 | 0.64 (0.50), 0.37/min, 40%; 25 / 47 / 25 / 1 | 0.79 (0.27), 0.45/min, 25%; 12 / 44 / 42 / 1 |
| 4 | 0.93 (0.85), 0.53/min, 39%; 17 / 45 / 35 / 2 | 1.04 (0.86), 0.59/min, 24%; 7 / 37 / 50 / 6 |
| 5 | 1.12 (1.03), 0.64/min, 31%; 7 / 39 / 47 / 7 | 1.19 (0.79), 0.68/min, 13%; 1 / 20 / 66 / 13 |
| 6 | 1.24 (1.17), 0.71/min, 16%; 2 / 25 / 59 / 14 | 1.30 (0.94), 0.75/min, 4%; 0 / 5 / 76 / 19 |
| 7 | 1.32 (1.26), 0.76/min, 12%; 1 / 11 / 70 / 18 | 1.32 (1.28), 0.76/min, 1%; 0 / 1 / 72 / 27 |
| 8 to 10 | 1.31 to 1.32, 0.75/min, 0 to 6% | the same, 0 to 1% |

The CPUs still spread over the ladder (from a Medium pick the weakest
mostly step down, the middle ones climb a rung, the strongest two), the
middle levels step up as often a minute but more of them by the normal
rule (six bars, 80% in the three best) than the early one, and fall back
far less (they keep up with what they climbed to); level 2 climbs less
(its hits below the 80% now), level 3 more. From a Beginner pick the
early steps up fall at the middle (level 5 2.59 → 1.24 a stage), the
steps a stage barely (2.75 → 2.67).

**A person among the CPUs.** The chance one of the reference's plays
beats one CPU of each level on charts of the same density (out below
every clear, then the Percent: what a stage ranks by, life aside), and
the place that makes among eight CPUs of that level (of nine):

| Notes a second (plays) | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 3 to 4.5 (300), v1 | 99% (1.1) | 98% (1.1) | 97% (1.2) | 94% (1.5) | 73% (3.2) | 42% (5.7) | 10% (8.2) | 2% (8.9) | 1% (8.9) | 0% (9.0) |
| 3 to 4.5, v2 | 99% (1.1) | 98% (1.1) | 96% (1.3) | 88% (1.9) | 67% (3.7) | 50% (5.0) | 30% (6.6) | 14% (7.9) | 5% (8.6) | 0% (9.0) |
| 4.5 to 6 (164), v1 | 98% | 98% | 98% | 98% | 98% (1.1) | 93% (1.6) | 63% (4.0) | 21% (7.3) | 7% (8.4) | 0% (9.0) |
| 4.5 to 6, v2 | 98% | 98% | 98% | 98% | 84% (2.3) | 57% (4.5) | 27% (6.8) | 10% (8.2) | 2% (8.8) | 0% (9.0) |

Against v1 such a player beat most CPUs up to level 6 because those went
out, and lost to every one from level 8 on whatever they did; against v2
they are an even match for level 6, and the levels around it are
reachable either way.

**Shocks** (`local_cpu_shocks`, four attacks of eight shocks a chart over
the arcade mixes, four seeds; `MINE_RESPONSE_V1` unchanged): the shocks
stepped on are the level's, as before to the noise (level 5 at Evasion 1:
1.41 → 1.48 of 8; level 10 0.16 both), the life an attack costs a little
more at the weak levels (level 1 28.0 → 29.3) and knockouts the same to a
few points (level 5 26.2% → 26.9%). Since round U a shock stepped on costs
50 points (10 before), so a level-5 CPU loses about 37 points an attack of
four shocks to them besides the life; the response's numbers were made
up before there were people's plays to compare (a stage's plays are never
kept, so no person's shock rate is known): **left as they are**, a
question for the maintainer whether the middle levels should step on
fewer once a person's rate is known.

### Cost

`planning_cost` (the 31 bundled single charts, release, natively; the
machine shared): a CPU's presses on the tap-only copy 162 µs a chart at
level 8 and 327 µs at level 3 under v2 (59 and 93 under v1: a play's own
factor gives its rows more distinct spreads, each a law made once), on
the full chart 160 and 284 µs; a plan 178 µs as before. A stage of 72
CPUs (`adaptive_cost_of_72_cpus`, Beatucada's Medium): loaded in 17 ms
as picked and 103 ms with a ladder of four rungs, played out in 51 and
176 ms (v1 31 / 100 and 77 / 179 when measured, on a busier machine); a
frame of 72 online CPUs 0.013 ms mean, 0.98 ms at most
(`what_computing_the_cpus_costs_a_frame`): as before.

### The solo CPU and prepared profiles

The solo CPU at a level (`level=N`, `--cpu-level N`, level 5 by default,
the named strengths its levels 6 to 10) plays `cpu-levels-v2` (`CPU_LEVELS`;
its prompt and results still say "Level 5 of 10"). Prepared profiles and
the five named strengths on a profile play `cpu-levels-v0`, chart-
relative, on the tap-only copy, exactly as before.

### What else moved

- `the_cpus_play_alike`'s fixture (the online CPUs' proof) recorded again,
  natively and checked in wasm32-wasip1; the pinned practice and live
  stage traces recorded again (the progress note, "Round V").
- Tests that asserted what particular CPUs did in a seeded stage, their
  meaning kept: the step-down fields test seats two-star CPUs (they climb
  to Hard and fall back under v2, three-star ones keep up); the attack
  lines test seats three level-9 CPUs (under v2 the level-10 CPUs'
  attacks against the autoplayed player came at every one of its own
  launches, so the rule passed every launch line over: nothing showed a
  launch said); the early-win voice test hears the 20-combo line before
  the win (the CPUs slip out later).
- Not changed: what a person's play judges, scores or shows; the
  ruleset; the ladder's rule; the attack ledger's rules; the CPUs' play
  of holds, rolls, lifts and mines but where a note's timing or lapse
  feeds it (above).

## What a CPU plays: the full chart (`cpu-play-v1`)

Since 2026-11-14 (the maintainer, who saw no holds on the CPUs'
miniatures: "CPUs should play the full charts") a CPU at an absolute
level plays the chart a person is given, through the production judge
under the stage's rules, with the same notes counted: its life, its
judgments and its score are on the chart's own scale, with nothing put on
another scale (`CPU_SCORE_V0` retired). `ddi_engine::cpu::CPU_PLAY` names
what the absolute levels play (`CpuPlay::FullV1`, `cpu/full.rs`); the
tap-only copy (`CpuPlay::TapsV0`, `cpu-play-v0`) is kept by name for the
prepared profiles, the chart-relative levels (`cpu-levels-v0`) with their
fixtures exactly, and the studies' comparison (`Opponent::planned_as`,
`DDI_CPU_PLAY=v0|v1` in `local_cpu`). Every multiplayer CPU plays it
(practice, the online CPUs every device computes, the bots' CPUs and the
bots' own plays, the audience's local computation, the studies), and so
does the solo CPU at its levels; the room model's revision is 7 for it.

What a CPU does with each note (`hold-model-v1`, `HOLD_MODEL_V1`,
PROVISIONAL):

- **Every note's timing** is the absolute model's: the motor's three
  draws a note in rows of the chart's taps, hold and roll heads and lifts
  (they count in a row's demand as notes, as before), each row's spread and
  chance of being left out from its demand. A head left out is the hold
  never pressed: its Miss alone under DDI.
- **What is held counts in a row's demand** (`RowDemand::held`, the lanes
  held down by holds and rolls begun before the row and lasting past it):
  the row's speed demand times `1 + 0.25` for each lane held
  (`AbsoluteModel::held_load`). A tap-only copy holds nothing, so the
  term is zero there and `cpu-play-v0` presses exactly as before.
- **A hold**: the head pressed as a tap is, the key kept down to the
  hold's end and let go 12 ms after it (or 50 ms before the lane's next
  note, which the hold's grace covers). **Letting go early** is a failure
  of its own: at each row while the hold lasts (in other lanes) with half
  the chance of leaving a note out that the level's curve gives that
  row's load (`drop_share` 0.5), the load counted 1.25 times where the row
  asks something of the held lane's hand (lanes 0 and 1, 2 and 3), and at
  each whole second of the hold with the level's floor; Stamina and
  Technical divide those chances by the square root of their product. A
  hold under a busy stream is where it goes; one with nothing else to do
  is nearly always kept. Let go, the key stays up: under DDI the hold is
  Off when its grace runs out, its units stop and its life follows the
  ruleset.
- **A roll**: the head pressed, then re-tapped every 0.12 s (inside every
  ruleset's roll window, so one tap left out is survived and two in a row
  are not under DDI Normal and Hard) with the timing law of the row before each tap, each
  tap left out with that row's lapse; it is let go early as a hold is (its
  taps stop), and its taps keep 0.09 s from a mine of its lane, a tap
  either side of one instead.
- **A lift**: the key pressed 80 ms before its release, the release on the
  lift's time with the timing law.
- **The chart's own mines**, as a battle's shocks are
  (`MINE_RESPONSE_V1`, Evasion): one with no note of its lane within half
  a second costs nothing; one beside a note of its lane is stepped on (a
  press in its window, up to 45 ms either side) with the level's chance,
  bent by Evasion where it is hard to get around; and a press of the
  timing law that the judge finds nearer a mine than its note steps on
  it, as a person's would. A mine **inside a hold** in the held lane
  (charts do this): kept held over, so stepped on, with that same chance
  (as a mine beside a note), else the key is let go 30 ms before it and
  taken again 100 ms after it (past every mine window), which the hold's
  grace covers. The simplest honest rule: what a level does about a mine
  beside a note, it does about one under its held key.
- **Fakes** are not played. Warps, stops and delays are the judge's: the
  CPU presses on the judged timeline as before.
- **The motor**: a lane is busy while a hold or roll is in it; a tap's key
  is held 12 ms; no two presses of one lane overlap (a press due while its
  lane is busy waits for it). Four lanes are four keys, so a four-lane
  chart never asks for more than four at once.
- **Deterministic**: every draw beyond the motor's is keyed by the seed,
  the note (its tick and lane) and what it is for, so the same chart,
  level, seed, skills and shocks give the same presses at any frame rate,
  natively and in wasm (the online CPUs' proof, `the_cpus_play_alike`,
  its fixture recorded again and checked in wasm32-wasip1) where the
  maths libraries' `exp`, `ln` and `powf` agree (see "Determinism and
  cost").
- **Battle shocks**: placement keeps them out of a hold's span in its lane
  (`placement-v2`); a shock's response moves a hold's press with its key
  kept as long; a shock crossed with a held key (placement never puts one
  there) locks the lane and lets the hold go, as the judge says.
- **Adaptive charts**: a CPU's hold crossing a step's boundary is played
  to its end as the old chart's (the new rung's presses in that lane wait
  for the key to be up), as a person's is.

### Measured (`cpu-play-v1`)

`DDI_CPU_PLAY=v0|v1 DDI_CPU_CORPUS=… DDI_CPU_RULES=ddi-easy,ddi-normal,ddi-hard
cargo test --release -p ddi-engine --test local_cpu -- --nocapture`, the
same thirteen arcade mixes as the levels' study (1,094 simfiles, 4,542
dance-single charts, read in place, eight seeds; aggregates only),
2026-11-14, the machine shared (load 9 to 19). Besides the percent and
the out rate the study now counts the holds and rolls kept and let go
(of those whose head was hit: a head missed is its Miss alone), the
chart's mines stepped on (of every mine of the chart), and DDI's score
against the best the chart gives.

**The anchors did not move**, so the levels are not re-anchored. At home
(DDI Normal, percent and out %), `cpu-play-v0` → `cpu-play-v1`:

| Level | At home on | v0 | v1 |
| --- | --- | --- | --- |
| 1 | Beginner | 86.9, 1.5 | 87.4, 1.4 |
| 3 | Easy | 90.3, 3.2 | 90.9, 3.1 |
| 5 | Medium | 89.4, 7.0 | 89.9, 6.9 |
| 7 | Hard | 89.4, 14.0 | 89.7, 13.9 |
| 9 | Challenge | 90.4, 17.8 | 90.4, 17.9 |
| 10 | Challenge | 96.4, 2.2 | 96.4, 2.2 |

Every cell of every slot under DDI Easy, Normal and Hard moved by 2.3
points of percent or less (the most at the weakest levels: kept holds
are easy points) and by 4.7 points of out rate or less (the most on DDI
Hard's Challenge charts, where the mines and dropped holds put a few more
out: level 5 88.1 → 91.4%, level 7 57.7 → 62.4%; elsewhere 2.1 points or
less).

DDI Normal, `cpu-play-v1`, by slot:

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| All (4542): percent | 49.8 | 61.8 | 71.8 | 79.0 | 85.1 | 90.4 | 93.9 | 96.2 | 97.6 | 98.8 |
| All: out % | 53.8 | 42.6 | 32.9 | 24.1 | 16.5 | 10.4 | 6.7 | 3.5 | 1.7 | 0.2 |
| All: holds let go % | 2.3 | 1.6 | 1.2 | 0.8 | 0.5 | 0.3 | 0.2 | 0.1 | 0.0 | 0.0 |
| All: mines stepped on % | 15.9 | 14.4 | 12.7 | 11.2 | 9.6 | 8.0 | 7.9 | 3.6 | 3.5 | 1.2 |
| Easy (1068): holds let go % | 0.9 | 0.5 | 0.2 | 0.1 | 0.1 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| Medium (1068): holds let go % | 2.3 | 1.4 | 0.9 | 0.5 | 0.3 | 0.1 | 0.1 | 0.0 | 0.0 | 0.0 |
| Medium: mines stepped on % | 11.4 | 10.2 | 9.1 | 8.0 | 7.0 | 5.7 | 5.7 | 2.7 | 2.7 | 0.7 |
| Hard (1068): holds let go % | 4.5 | 3.2 | 2.2 | 1.4 | 0.9 | 0.5 | 0.2 | 0.1 | 0.1 | 0.0 |
| Hard: holds played a chart | 8.0 | 10.4 | 12.4 | 14.0 | 15.3 | 16.4 | 17.1 | 17.4 | 17.6 | 17.7 |
| Challenge (327): holds let go % | 4.1 | 3.3 | 2.6 | 2.0 | 1.5 | 0.9 | 0.5 | 0.3 | 0.2 | 0.1 |
| Challenge: mines stepped on % | 16.5 | 14.9 | 13.2 | 11.6 | 9.8 | 8.3 | 8.2 | 3.7 | 3.7 | 1.2 |

(The arcade Beginner and Easy charts have no mines. The mines' rows move
in steps where `MINE_RESPONSE_V0`'s levels share an avoidance: 6 and 7,
8 and 9. "Holds played" falls at the weak levels because a head they miss
is no hold kept or let go.) DDI Easy and Hard let go about as often
(Hard: 5.2% at level 1 on Hard charts, 0.0 at level 10; Easy 3.2%).

**A person's score and a CPU's now compare directly.** On the 617 arcade
charts with 30 holds or more (DDI Normal), a level-10 CPU scores 3,312
thousand on average against the 3,385 thousand the charts give at best
(every note W0, every hold kept): 97.8% of the best, 98.3% of the points;
level 7 2,862 thousand (84.5%), level 5 2,199 (64.9%), out on 39% of them.
Before, the copy's score was put on the chart's scale as if the holds had
gone as its taps did; now a kept hold is the points a person's kept hold
is, and one let go costs it as it costs them.

The stream packs (DDI Normal, 1,622 charts, eight seeds) and the bundled
songs (19 charts, sixteen seeds) move the same way, by a point or two of
percent at the weakest levels and less than a point of out rate at
every level (the packs' Challenge charts: level 10 88.1 → 88.4%, out
29.6 → 29.3%; the bundled songs all: 52.9 / 73.1 / 85.7 / 94.6 / 97.9 /
99.1 → 54.6 / 73.9 / 86.1 / 94.6 / 98.0 / 99.1% at levels 1 / 3 / 5 / 7
/ 9 / 10).

**Cost** (`planning_cost`, the 31 bundled single charts, release,
natively; the machine shared): a plan 195 µs (141 before: the full
chart's rows, notes and demands are made with it, beside the tap-only
copy's), a CPU's presses on the full chart 59 µs at level 8 and 75 µs at
level 3 (the tap-only copy's 58 and 93). 72 CPUs a stage
(`adaptive_cost_of_72_cpus`, Beatucada's Medium): loaded in 31 ms as
picked and 100 ms with a full ladder (30 and 116 before), played out in
77 and 179 ms (72 and 201); a frame of 72 online CPUs 0.014 ms mean,
1.09 ms at most (0.013 and 1.02 before). In wasm32 (wasm32-wasip1 under a
WASI runner): a plan 261 µs (223 before), the full chart's presses 80 and
96 µs (the copy's 79 and 115). Drawing the CPUs' holds on their
miniatures costs GPU time (72 of them on a chart of 34 holds: about 4.5
ms a frame more natively): `docs/plans/multiplayer-prototype-progress.md`,
"CPUs play the full chart", "Cost".

### Adaptive charts under `adaptive-v3`, `adaptive-v4` and `adaptive-v5`

A CPU steps by the stage's rule, as a person does (its play is a
`Player`). Under `adaptive-v2` (follow-up round F) a step up needed 95%
of the taps of its four bars in the two best judgments, which the weak
levels' spread rarely gives (levels 1 to 3 practically never climbed
from a Medium pick). Since feedback round H
(`ddi_engine::ladder::ADAPTIVE_V3`) that is the early step up, and a
normal one comes after eight clean bars with 80% of their taps in the
three best judgments: from a Medium pick level 3 climbs in a third of
stages (3% before), level 5 in 87% (58%), levels 8 to 10 as before. A
CPU at full life is nearly always above that floor (eight bars with no
floor measured the same); the floor is for people, whose spread is
wider. The tables: `docs/plans/multiplayer-prototype-progress.md`,
"Feedback round H".

Since round D1 (`ddi_engine::ladder::ADAPTIVE_V4`, room model revision
11) the early step up asks 90% of its four bars' taps in the three best
judgments (95% in the two best before): nearly every step up a CPU
makes is early now, the first one four to eight bars sooner (from a
Medium pick level 3 climbs in 55% of stages, level 5 in 94%, its first
step up about bar 18), a few more of them falling back within sixteen
bars (level 5: 31%, 27% before), and on Beginner picks the weak levels
go out of the stage about twice as often. The tables: the same note,
"Round D1". Since round P1a (`ADAPTIVE_V5`, 2026-11-16) the normal step
up takes six bars, not eight: the CPUs, nearly all of whose steps up are
early, barely change (the weakest levels on Beginner picks go out a few
points more often); the same note, "Round P1a".

## Evasion: a CPU against a battle's shocks

The eighth skill axis (the maintainer's M8 decision D8; round 4 of
`docs/plans/ddi-ruleset.md`): how a CPU opponent fares against the
shocks an attack puts into its play. Like the other axes it is a model
of a CPU, never a measure of a person's skill, and PROVISIONAL.

`MINE_RESPONSE_V1` (`mine-response-v1`, `ddi_engine::cpu::mines`):

- Each level keeps its response as `mine-response-v0` had it (the chance
  it steps on a shock, how far its presses within 0.3 s of one move, the
  chance one is left out; from its level's avoidance).
- Evasion bends that response shock by shock: the chances times
  `Evasion^-γ`, the moves times `Evasion^-γ/2`, with
  `γ = 1 + (busy + beside) / 2`. `busy` is how full the chart is around
  the shock (0 with two notes or fewer within half a second either side,
  all lanes, 1 at eight or more); `beside` is a note of the shock's own
  lane within half a second, where `placement-v2` aims six shocks of
  eight. So a dodger (Evasion above 1) steps on fewer shocks everywhere
  and most fewer where they are hard to get around; a stomper (below 1)
  on more, most where they are hard. Capped at 95%.
- **Evasion 1 is the level's response exactly**: every seed, every
  roster CPU without an eighth value and every seven-value vector plays
  as before (a fixture of presses, shocks stepped on and misses taken
  from the build before, `ddi-engine`
  `evasion_one_responds_exactly_as_mine_response_v0_did`).
- Deterministic per chart, level, seed and skills at any frame rate: the
  context comes from the CPU's chart (its judged notes), the draws are
  the same keyed draws as before.

The roster's dodgers and stompers (`cpu_roster.json`, stars unchanged,
their personality shown as Evasive or Reckless): Brisk Otter (level 1)
and Tessel Moth (8) reckless, Velvet Metronome (5) and Saltglass Rook
(10) evasive.

### Measured

Between eight CPUs of one level, all at one Evasion, over the attack-rate
study's six bundled charts under DDI Normal (`DDI_ATTACK_ONLY=evasion
cargo test --release -p ddi-session --lib attack_rates -- --ignored
--nocapture`, four seeds; an attack received: shocks stepped on of
eight, the life it cost (a shock 5, a note missed in a locked lane 10),
and the share of attacks whose recipient went out while it passed):

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Stepped on, Evasion 0.5 | 5.09 | 4.61 | 4.70 | 4.21 | 3.91 | 3.44 | 3.23 | 1.54 | 1.60 | 0.58 |
| Stepped on, Evasion 1 | 2.40 | 2.23 | 2.11 | 1.87 | 1.66 | 1.38 | 1.22 | 0.54 | 0.54 | 0.19 |
| Stepped on, Evasion 2 | 1.22 | 1.04 | 0.95 | 0.81 | 0.70 | 0.60 | 0.48 | 0.20 | 0.18 | 0.06 |
| Life an attack, 0.5 | 62.8 | 61.1 | 63.9 | 61.9 | 59.3 | 56.8 | 49.8 | 27.1 | 28.2 | 10.6 |
| Life an attack, 1 | 32.8 | 33.2 | 31.7 | 30.9 | 29.0 | 25.6 | 21.3 | 9.2 | 9.8 | 3.3 |
| Life an attack, 2 | 18.1 | 17.2 | 15.7 | 14.0 | 13.2 | 11.5 | 8.5 | 3.5 | 3.3 | 1.1 |
| Knocked out by it %, 0.5 | 73.3 | 61.4 | 49.4 | 44.2 | 29.7 | 22.9 | 20.3 | 1.8 | 2.2 | 0.0 |
| Knocked out by it %, 1 | 23.1 | 18.3 | 11.8 | 7.5 | 3.9 | 1.0 | 0.7 | 0.0 | 0.0 | 0.0 |
| Knocked out by it %, 2 | 12.1 | 4.6 | 4.6 | 1.8 | 0.0 | 0.0 | 0.1 | 0.0 | 0.0 | 0.0 |

Over the maintainer's local arcade mixes, read in place (thirteen
folders, 996 simfiles, 4,147 dance-single charts; aggregates only;
`DDI_CPU_SHOCKS=1 DDI_CPU_CORPUS=… cargo test --release -p ddi-engine
--test local_cpu local_cpu_shocks -- --nocapture`, four seeds): each
chart given up to four attacks where `placement-v2` puts them (sections
16 beats apart or more), one CPU alone, the fail off; knocked out is
the share of plays whose gauge emptied while one of the attacks passed:

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Stepped on, Evasion 0.5 | 5.85 | 5.22 | 4.66 | 4.08 | 3.56 | 3.00 | 2.99 | 1.24 | 1.23 | 0.47 |
| Stepped on, Evasion 1 | 2.71 | 2.49 | 2.19 | 1.79 | 1.49 | 1.25 | 1.22 | 0.56 | 0.55 | 0.17 |
| Stepped on, Evasion 2 | 1.23 | 1.07 | 0.96 | 0.85 | 0.72 | 0.57 | 0.54 | 0.23 | 0.22 | 0.07 |
| Life an attack, 0.5 | 68.0 | 62.4 | 55.7 | 48.5 | 41.9 | 35.1 | 34.9 | 14.7 | 14.6 | 5.5 |
| Life an attack, 1 | 29.5 | 26.7 | 23.5 | 19.5 | 16.4 | 14.0 | 13.5 | 6.2 | 6.0 | 1.9 |
| Life an attack, 2 | 14.0 | 12.0 | 10.4 | 9.1 | 7.6 | 6.1 | 5.5 | 2.4 | 2.2 | 0.7 |
| Knocked out %, 0.5 | 99.6 | 99.6 | 99.6 | 99.2 | 97.6 | 91.5 | 90.7 | 14.6 | 14.1 | 0.4 |
| Knocked out %, 1 | 91.2 | 84.7 | 71.2 | 49.3 | 29.7 | 14.3 | 12.3 | 0.6 | 0.6 | 0.0 |
| Knocked out %, 2 | 39.5 | 25.5 | 12.3 | 6.0 | 2.1 | 0.7 | 0.4 | 0.0 | 0.0 | 0.0 |

By difficulty slot the same order holds in every row; an attack costs
more on harder charts (level 1 at Evasion 1: 20.7 life on Beginner
charts, 40.5 on Challenge). What they say: at a level, Evasion 0.5 about
doubles the shocks stepped on and Evasion 2 about halves them (a little
more than that, since busy stretches and notes beside the shocks count
for more); the life an attack costs moves the same way, so a weak level
at 0.5 is out by four attacks on nearly every chart and a strong one at
2 hardly feels them. The axis spans more than a level step does, which
is why the presets and the roster use 0.6 to 1.6: a decision for the
maintainer, with the shares in `γ`.

**Measured again under `cpu-levels-v1`** (2026-11-13; `MINE_RESPONSE_V1`
unchanged, its inputs the level as before: a level's avoidance, Evasion
bending it). Over the same arcade mixes (4,542 charts, four seeds, up to
four attacks a chart, the fail off):

| Level | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Stepped on, Evasion 0.5 | 5.85 | 5.22 | 4.65 | 4.06 | 3.54 | 2.97 | 2.97 | 1.21 | 1.21 | 0.45 |
| Stepped on, Evasion 1 | 2.64 | 2.43 | 2.14 | 1.74 | 1.43 | 1.20 | 1.19 | 0.54 | 0.53 | 0.16 |
| Stepped on, Evasion 2 | 1.12 | 0.98 | 0.88 | 0.78 | 0.65 | 0.51 | 0.50 | 0.20 | 0.20 | 0.06 |
| Life an attack, 1 | 28.8 | 26.0 | 22.6 | 18.5 | 15.4 | 13.0 | 12.8 | 5.7 | 5.6 | 1.7 |
| Knocked out %, 0.5 | 98.2 | 98.9 | 99.2 | 98.8 | 97.1 | 90.3 | 90.3 | 14.3 | 13.7 | 0.4 |
| Knocked out %, 1 | 83.4 | 76.2 | 61.9 | 42.7 | 26.4 | 14.9 | 12.1 | 0.8 | 0.6 | 0.0 |
| Knocked out %, 2 | 37.9 | 27.7 | 18.7 | 11.4 | 6.1 | 2.6 | 1.1 | 0.2 | 0.1 | 0.0 |

The shocks stepped on are as before to the noise (the response is the
level's, not the model's); what changed is the gauge they land on. A
weak level now loses life to the chart itself on the busier charts, so
an attack ends it more often (level 3 at Evasion 1: 61.9% knocked out,
71.2% under v0; level 1 83.4% against 91.2%, level 5 26.4% against
29.7%): fewer of its plays were still in when the attacks came, and on
easy charts it holds its life better than v0's lapses let it (Beginner,
level 1 at Evasion 1: 20.0 life an attack, 74.8% knocked out). Levels 6
and 7, and 8 and 9, share an avoidance (`MINE_RESPONSE_V0`'s table), so
their rows match. The response is sane on every chart: monotone in level
and in Evasion in every slot, bounded (at most 95%), and no level is
knocked out by four attacks on Beginner charts at Evasion 2 more than
7% of the time.

Between eight CPUs of one level (`attack_rates`, the six bundled charts,
four seeds, DDI Normal, Evasion 1), an attack received cost 34.2 / 25.3 /
22.1 / 18.8 / 8.9 / 2.6 life at levels 1 / 3 / 5 / 7 / 9 / 10 (v0: 32.8
/ 31.7 / 29.0 / 21.3 / 9.8 / 3.3) and Evasion 0.5 and 2 bend it as before
(level 5: 56.8 and 8.8).

## Implementation and bounds

`ddi_engine::cpu::Plan` prepares from the chart's judged timestamps before
playback. Density, repeated lanes and chords determine error frequency. An
independent-note analytical anchor chooses a bounded error width; higher
strength reduces width and frequency. The anchor uses a Wife-style reward,
which is distinct from the score displayed by a ruleset. Dense patterns and
input matching can substantially affect the observed result. That anchor
is `cpu-levels-v0`'s alone: an absolute level (`cpu-levels-v1`) fits
nothing to the chart; its plan's part is each row's demand in real units
(`RowDemand`, four numbers and the row's count), made with the rest of the
plan, and its presses take one timing law a distinct spread (tenths of a
millisecond), cached for the script.

The compact plan stores rows/frequencies, seven demand weights per row, and six small timing
distributions. Nonbalanced skills compute one timing law per row at setup;
no dense probability matrix is stored.
The shared motor uses deterministic SplitMix64 draws, 12 ms key holds and a
0.1 ms release gap across the whole song. The session submits those timestamped
inputs directly to the production judge and applies its offset exactly once.
It retains the input schedule across frame stalls.

Automatic mode accepts 1–65,536 playable rows, at most 262,144 projected notes,
within one day. It supports source-defined four-lane single-player layouts.
Invalid times, duplicate lanes at a tick, empty projections and non-4K layouts
are refused. A motor queue extending beyond the play's end is refused too.

Optional prepared-profile support remains: desktop `--cpu FILE` reads a JSON
artifact with `source_file`, `chart_index` and `profile`; web
`cpu=cpu/custom.json` reads `{"profile": ...}` from a site-relative path.
Profiles must match the selected source digest/timing and satisfy the bounded
schema in `crates/engine/src/cpu.rs` (32–4,096 rows, up to 64 MiB). No prepared
profiles or fitted weights are bundled. This path is separate from automatic
analytical generation.

## Verification

Native tests cover source binding, projection, short/long charts, limits,
reproducibility, average strength ordering, input matching, clock offsets,
frame stalls and absence of personal score records. The opponent's ten levels:
each plans and plays the same at any frame rate, a weaker one scores no
better on average, levels 6 to 10 press exactly as the five strengths did
(the fixture), and levels 1 to 5 press fewer notes the weaker they are.
`cpu-levels-v2` (round V): a CPU leans as its seed says and the same on
every chart, its spread widens under load while it still presses nearly
every note, and past its limit it leaves notes out
(`a_v2_cpu_leans_and_loosens_before_it_leaves_notes_out`); `cpu-levels-v1`
has none of v2's own timing (`cpu_levels_v1_has_none_of_v2_s_own_timing`);
every v2 level better than the one below
(`ten_levels_the_last_five_the_frozen_strengths`); over a local
corpus, the ordering test (`local_cpu_order`, opt-in, above).
The absolute levels (`cpu-levels-v1`, the tests now on `cpu-levels-v2`): a row's demand in real units (a
stream of ten notes a second asks about ten, half the rate half), no
failure below the onset and more past it, bounded; monotone in level on
every bundled single chart (within half a point over four seeds) and in
the chart's slot for a level (within two points, six between charts of one
meter: Beatucada's Hard and Challenge have the same meter and its
Challenge plays a little easier); the weakest level presses nearly every
note of a slow stream and fewer than half of one sixteen a second; each of
the seven pattern axes bends the limit its pattern meets (strong does
better than balanced, weak worse, on synthetic streams, jumpstreams,
handstreams, a long stream, jacks, chordjacks and an irregular rhythm);
extreme rows (a millisecond apart, rows at one time) stay valid; the solo
CPU's `level=` presses as the opponent at that level does. The full chart
(`cpu-play-v1`, `cpu/full/tests.rs`): holds kept where nothing is hard
and let go more the weaker the level under a busy stream (none on the
tap-only copy), a row asking more while a hold is down, rolls kept by
re-tapping, lifts released on time, a mine with no note of its lane near
costing nothing and one beside a note stepped on more by weaker levels
and less with Evasion, a mine inside a hold let go around or stepped on,
the same presses at any frame rate and from the same seed, extreme charts
(rows at one time, a hold of no length and one over everything, mines
everywhere) valid, shocks kept off holds and a lock letting a held key
go; the online CPUs' fixture recorded again and checked in wasm32;
`cpu-play-v0`'s fixtures unchanged. Personality checks compare
matched demanding/easier patterns, exercise all seven axes through judged
inputs across 32 seeds, and reject invalid vectors/prepared-profile modifiers:

```sh
cargo test -p ddi-engine -p ddi-session --lib
DDI_IMPORTER_TEST_ONLY=1 DDI_HOME="$PWD/target/scratch/cpu/home" \
  cargo test -p ddi-desktop --no-default-features --test cpu_play --test cpu_play_flow
```

Use the repository watchdog for runtime tests. The UI test draws offscreen and
skips without a GPU; set `DDI_CPU_REQUIRE_GPU=1` to require it. `DDI_CPU_PROFILE`
optionally switches the desktop tests to an external prepared artifact.

`tools/cpu-player/fixtures.py` generates original synthetic SM/WAV zip fixtures
under `target/scratch/cpu-automatic`. After building the native `cpu_chart`
example and serving a static web build, `tools/cpu-player/check.mjs` imports
these through the actual browser importer and compares complete logs, judgments
and scores with native playback in Chrome and Firefox. It covers short charts,
5,000-row charts, timing changes and invalid layouts, plus every personality
preset, custom vectors, retry reproducibility and narrow-screen display. Set `DDI_PLAYWRIGHT` to the
installed `playwright-core/index.mjs`, `DDI_CPU_WEB_URL` to the local server,
and optionally `DDI_CPU_WEB_OUTPUT` for receipts; follow the browser setup in
`.claude/skills/verify/SKILL.md`. Nothing requires research files or real songs.

`tools/cpu-player/export_basis.py` regenerates the timing grid and bin reward
means using numpy/scipy and rustfmt. The generated constants are mathematical
quantities, not trained weights or chart-derived data; playback needs only Rust.
