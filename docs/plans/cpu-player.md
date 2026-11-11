# CPU player

Implemented for desktop and web. A seeded, adjustable analytical CPU plays
imported or bundled single-player keyboard 4K charts through the normal judge.
No offline profile, model download, training environment or external calculator
is required. Strength is relative to the selected chart, not a transferable
human skill rating or a guaranteed score.

## Launch

Append the CPU parameters to a chart's existing web play route:

```text
#/play?song=<song-id>&chart=0&cpu=1&strength=normal&seed=7
```

Desktop uses the song's library id and zero-based chart index:

```sh
cargo run -- --play SONG --chart 0 --cpu=1 --cpu-strength normal --cpu-seed 7
```

Strengths are `relaxed`, `easy`, `normal`, `strong`, `expert` (multipliers
0.75, 0.9, 1, 1.1, 1.25). Default: normal, seed 1. Seeds are unsigned 64-bit
integers; the same chart, strength, skills and seed reproduce the same inputs. Retry
keeps them; returning to song select does not leave CPU mode enabled.

The play is marked CPU, with no personal history, bests or calibration. Human
lane input, ghosts, per-play transforms and rate changes are disabled; cancel
and retry still work. Fail is off, but matching, timing windows and scoring
remain those of the selected ruleset. Holds/rolls become tap heads; mines,
lifts, fakes and tails are omitted. Source charts are unchanged.

## Skill personality

Automatic mode accepts `&skills=jacks` on the web, or `--cpu-skills jacks`
on desktop. Use `balanced` (the default), `stream`, `jacks`, `chords`, `stamina`
or `technical`, or seven comma-separated multipliers in this order:
**Stream, Jumpstream, Handstream, Stamina, JackSpeed, Chordjack, Technical**.
Each value is 0.5–2 inclusive in steps of 0.001. One is neutral; larger means
better at that pattern. For example:

```text
#/play?song=<song-id>&chart=0&cpu=1&strength=normal&seed=7&skills=1.1,0.9,1,1.25,0.8,1,1
```

```sh
cargo run -- --play SONG --cpu=1 --cpu-skills 1.1,0.9,1,1.25,0.8,1,1 --cpu-seed 7
```

Presets are explicit vectors, not trained players:

| Preset | Stream | Jumpstream | Handstream | Stamina | JackSpeed | Chordjack | Technical |
|---|---:|---:|---:|---:|---:|---:|---:|
| balanced | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| stream | 1.25 | 1 | 1 | 1 | 0.8 | 1 | 1 |
| jacks | 0.8 | 1 | 1 | 1 | 1.25 | 1 | 1 |
| chords | 0.8 | 1.25 | 1.25 | 1 | 0.8 | 1.25 | 0.8 |
| stamina | 0.8 | 1 | 1 | 1.25 | 1 | 1 | 1 |
| technical | 0.8 | 1 | 1 | 1 | 1 | 1 | 1.25 |

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
does not compensate away the selected weaknesses.

## An opponent's ten levels

A multiplayer stage's CPU opponents (`ddi_engine::cpu::Opponent`,
`docs/plans/multiplayer-prototype-progress.md`) play at one of ten levels,
`ddi_engine::cpu::CPU_LEVELS_V0` (`cpu-levels-v0`, `cpu/levels.rs`), shown
as five stars and never as a number (`ddi_ui::stars`). The solo CPU above
keeps its five named strengths.

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

### Measured

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

## Implementation and bounds

`ddi_engine::cpu::Plan` prepares from the chart's judged timestamps before
playback. Density, repeated lanes and chords determine error frequency. An
independent-note analytical anchor chooses a bounded error width; higher
strength reduces width and frequency. The anchor uses a Wife-style reward,
which is distinct from the score displayed by a ruleset. Dense patterns and
input matching can substantially affect the observed result.

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
(the fixture), and levels 1 to 5 press fewer notes the weaker they are. Personality checks compare
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
