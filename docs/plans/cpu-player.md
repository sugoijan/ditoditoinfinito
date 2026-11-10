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
frame stalls and absence of personal score records. Personality checks compare
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
