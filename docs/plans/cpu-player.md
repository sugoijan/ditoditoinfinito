# CPU player

Implemented for desktop and web. A seeded, adjustable analytical CPU plays
imported or bundled single-player keyboard 4K charts through the normal judge.
No offline profile, model download, training environment or external calculator
is required. Strength is relative to the selected chart, not a transferable
human skill rating or a guaranteed score. The multiplayer CPU opponents play
at absolute levels instead (`cpu-levels-v1`, below): what a level keeps up
with is the same on every chart, and the solo CPU can play at one of them.

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
(`cpu-levels-v1`, below): `level=N` on the web or `--cpu-level N` on
desktop, 1 to 10, is its strength; without one it plays **level 5**
(`ddi_engine::cpu::SOLO_LEVEL`: the middle of the scale, at home on
Medium charts, so that a CPU started with no flag plays the common
charts well and the hard ones badly, as a level should). The five named
strengths, `strength=` / `--cpu-strength relaxed|easy|normal|strong|expert`,
are still taken and are **levels 6 to 10** of the new scale
(`strength_level`), as they were levels 6 to 10 of the chart-relative one;
a level and a strength cannot be combined. Prepared profiles (`--cpu
FILE`, a `cpu=` path) play their strength, chart-relative, as before.
The chart-relative model (`cpu-levels-v0`, the five strengths'
multipliers 0.75, 0.9, 1, 1.1, 1.25) stays in the engine by name with its
fixtures, reachable from prepared profiles, the tests and the studies.
The prompt and the results say "Level 5 of 10". Seed 1 by default. Seeds are unsigned 64-bit
integers; the same chart, level, skills and seed reproduce the same inputs. Retry
keeps them; returning to song select does not leave CPU mode enabled.

The play is marked CPU, with no personal history, bests or calibration. Human
lane input, ghosts, per-play transforms and rate changes are disabled; cancel
and retry still work. Fail is off, but matching, timing windows and scoring
remain those of the selected ruleset. Holds/rolls become tap heads; mines,
lifts, fakes and tails are omitted. Source charts are unchanged.

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
(`cpu-levels-v1`, below) the axes multiply the limits their patterns meet
instead (a streamer keeps up with faster streams, a jacker with faster
jacks), with nothing normalized to the chart.

## An opponent's ten levels

A multiplayer stage's CPU opponents (`ddi_engine::cpu::Opponent`,
`docs/plans/multiplayer-prototype-progress.md`) play at one of ten levels,
shown as five stars and never as a number (`ddi_ui::stars`). Since
2026-11-13 the levels are absolute (`cpu-levels-v1`,
`ddi_engine::cpu::CPU_LEVELS_V1`, `cpu/absolute.rs`; `CPU_LEVELS` names the
scale opponents play at): every multiplayer CPU (practice, online rooms
computed on every device, the bots' CPUs, the audience's, the studies)
plays them. The earlier levels (`cpu-levels-v0`, `CPU_LEVELS_V0`, below)
are kept by name for comparison and as the solo CPU's five strengths.

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

### The model (`cpu-levels-v1`, PROVISIONAL)

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

### The ten levels (`CPU_LEVELS_V1`, PROVISIONAL)

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

### Measured (`cpu-levels-v1`)

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
online CPUs' proof, its fixture recorded again). Planning
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
The absolute levels (`cpu-levels-v1`): a row's demand in real units (a
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
CPU's `level=` presses as the opponent at that level does. Personality checks compare
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
