# Shared DanOni pack preparation and estimates

Target update (2026-10-15): playable modes are 5, 7, 9A and 9B. 7i is
retired. Existing 7i references and dated results below remain historical source
evidence; the current policy is in [the Rust plan](danoni-rust-plan.md#retire-7i-as-a-playable-target-2026-10-15).

Implemented and checked locally on 2026-10-09. The shared
[`prepare_pack.py`](danoni-consolidation.md#retired-tools) (retired),
[`pack_policy.py`](danoni-consolidation.md#retired-tools) (retired) and
[`analyze_tempo.py`](danoni-consolidation.md#retired-tools) (retired) contain no
Kouhaku song rules or source SWF identities. Family-specific native adapters
remain responsible for proving chart frames, lane order and audio origin. These
utilities consume normalized exports, never execute work scripts, and perform
no network requests. Code is MIT under the existing `REUSE.toml` catch-all.

The same policies have been exercised against two real legacy engine families
and original synthetic 7/9-key works. This makes future adapters smaller without
claiming arbitrary SWFs already work. Browser `/1` dumps need a normalization
adapter; their engine-relative frames cannot be assumed audio-relative seconds.
Keep their original dos, key definitions and reference parse as evidence.

## Normalized input contract

An input directory contains UTF-8 `manifest.json`, referenced audio and optional
native references. Supported schema literals are `ddi-legacy-export/experimental-1`,
`ddi-synthetic-export/experimental-1`, and the family-independent
`ddi-normalized-work/experimental-1`. Existing legacy formats keep additional
fields; the loader reads the subset below. This is an experimental contract,
not a new original DanOni community format.

| Field | Type and meaning |
|---|---|
| `schema` | One of the literals above. |
| `charts` | Nonempty array of Chart objects, in source order. |
| `music` | Optional default Music object for all charts. |
| `metadata` | Optional default metadata object. |
| `evidence.reference` | Optional path to default native reference JSON. |
| Chart `id` | Unique nonempty string, stable within this export. |
| Chart `mode` | `5`, `7`, `7i`, `9A` or `9B`. No custom layouts. |
| Chart `lanes` | Ordered Lane array with exactly the mode's lane count. |
| Chart `music` | Optional Music override. Required if no top-level Music exists. |
| Chart `difficultyStars` | Optional integer 0–999, or null. It is a source rating. |
| Chart `metadata` | Optional metadata override. |
| Chart `nativeReference` | Optional reference JSON path overriding the default. |
| Chart `speed` | Optional ordered array of `{seconds:number,multiplier:number}` scroll changes. |
| Chart `colors`, `boost`, `stops`, `effects` | Optional arrays, retained as reference data but not encoded into the SSC. |
| Lane `tapsSeconds` | Array of finite audio-relative note times, in seconds. |
| Lane `holdsSeconds` | Array of `[headSeconds,tailSeconds]`, with head strictly before tail. |
| Lane `name` | Optional informative source lane name. Lane array order is authoritative. |
| Music `path` | Relative path within the input directory, resolved without escaping it. |
| Music `sha256` | Required SHA-256 of the file bytes. A mismatch rejects the input. |

Metadata merges in order: manifest defaults, native reference `metadata`, chart
`metadata`. Recognized fields are `title` (fallback `musicTitle`, then chart id),
`artist`, `difficultyStars`, `rawDifficulty`, `difficulty`, and
`difficultyClassification`. An explicit chart star count takes precedence.
The only legacy free-text rating accepted is a string consisting entirely of
`★` characters; `Hard 6` is not guessed. Unknown stars remain null. Artists are
not invented from a title or tuning string. Native reference JSON and all source
chart/metadata fields are copied to the derived references unchanged.

Built-in lane order follows CW pattern 0 and
[the generated game table](../../chart/src/layout/danoni_table.rs):

| Mode | Ordered lane names |
|---|---|
| `5` | left, down, up, right, space |
| `7`, `7i` | left, leftdia, down, space, up, rightdia, right |
| `9A`, `9B` | left, down, up, right, space, sleft, sdown, sup, sright |

An adapter must supply that order and must not label custom definitions as a
built-in mode. Preserve custom keys in the original export; adding a playable
layout requires an explicit contract extension. Current exact SSC generation
supports the **60 Hz frame grid only**: each note/hold/speed time must equal an
integer frame divided by 60, within 1e-9 seconds, with absolute frame at most
432,000. Other clocks or arbitrary timestamps are rejected rather than rounded.
Negative pre-roll is supported with chart-specific offset padding. Duplicate
cells, overlapping holds/taps in one lane, empty charts and invalid hold pairs
are rejected.

For example, this original synthetic data needs no family identifier (audio
hash is abbreviated here; replace it with the complete digest):

```json
{
  "schema":"ddi-normalized-work/experimental-1",
  "metadata":{"title":"Original synthetic work","artist":"Original fixture"},
  "music":{"path":"sound.wav","sha256":"<64 hexadecimal characters>"},
  "charts":[{
    "id":"normal","mode":"5","difficultyStars":4,
    "lanes":[
      {"name":"left","tapsSeconds":[1],"holdsSeconds":[]},
      {"name":"down","tapsSeconds":[],"holdsSeconds":[[2,3]]},
      {"name":"up","tapsSeconds":[],"holdsSeconds":[]},
      {"name":"right","tapsSeconds":[],"holdsSeconds":[]},
      {"name":"space","tapsSeconds":[4],"holdsSeconds":[]}
    ],
    "speed":[{"seconds":1,"multiplier":1.5}]
  }]
}
```

## Derived layout and manifest

Output must be a fresh directory outside every Git checkout. Source exports
are untouched. Charts sharing the same audio SHA-256 become one song in source
order; distinct audio becomes separate songs. The first chart supplies song
metadata. ASCII chart IDs become directory names; other IDs receive `song-N`
names, with collisions disambiguated.

```text
manifest.json                   ddi-playable-pack/experimental-1
playable.zip                    only the contents of songs/, relative to that folder
songs/<song-id>/song.ssc         one SSC, possibly several charts
songs/<song-id>/music.wav|ogg    untrimmed playback audio
references/<song-id>.wav         decoded PCM16 source used for preparation
references/<song-id>-<n>.json    sourceChart, sourceMetadata, sourceReference
```

`manifest.json` has these fields:

- `schema`, `sourceSchema`, `sourceManifestSha256`, `sourceExport` (absolute local
  path), `prepareSourceSha256`, `policySourceSha256`, `quantizerSourceSha256`, `codec`, and `ffmpeg`
  (installed version line).
- Default `timingBpm:75`, `noteTimingModified:false` and
  `tierPolicy:{name,upperBounds,tiers}`. Bounds are the four increasing inclusive
  maxima for Beginner, Easy, Medium and Hard; larger values become Challenge.
- `songs[]`: `{id,title,music,musicSha256,sourceAudioSha256,pcmSampleRate,
  pcmChannels,pcmFrames,decodeCommand,encodingCommand,encoderVersion,
  encoderSha256,preview,displayBpm,simfile,charts}`. `music` and `simfile` are relative paths;
  `pcmFrames` counts sample frames, not interleaved samples. Encoding fields are
  null for WAV. Commands preserve exact argument arrays and local source paths.
- Each song's `charts[]`: `{id,difficultyStars,difficultyClassification,
  classificationSource,sourceDifficultyClassification,unrepresentedEffects}`.
  Unknown stars are null; source classification is separate from the derived
  string. `unrepresentedEffects` maps populated effect names to array lengths.
- Song `displayBpm` is null unless explicitly reviewed. Otherwise it is
  `{kind,sscValue,source}`; numeric kinds also have `minimumBpm`, `maximumBpm`.
  Kinds are `single`, `range`, `unknown`; `source` is `explicit review input`.
  This metadata is independent of playback timing.
- `preview`: `startSeconds`, `lengthSeconds`, `status`, `thresholdRms` and policy
  diagnostics. New exports use the [pickup policy](#earliest-music-pickup-previews)
  below, including explicit fallback statuses. Older energetic excerpts retain
  their original fields/statuses. `firstActiveSeconds` is an RMS-threshold
  heuristic, not a claim about perceived first sound. RMS uses normalized PCM
  amplitude across all channels; stereo channels are not summed as waveforms.

Default SSC uses `#STEPSTYPE:danoni-<mode>`, an empty description, numeric meter, derived
tier, and exact 75 BPM frame timing: 192 rows per measure and one frame per tick.
For negative pre-roll, notes/events are padded together and chart `#OFFSET`
subtracts that padding. Speed becomes `#SCROLLS`, with its original multiplier.
`#SAMPLESTART`/`#SAMPLELENGTH` describe the preview without trimming music.
Colours, boost, stops and custom effects remain available in references and
explicit omission counts; this exporter does not promise their play semantics.
Original dos exports remain available separately from the family adapters.

## Policies and running

Default stars-to-tier boundaries are **1,3,5,7**: 0–1 Beginner, 2–3 Easy,
4–5 Medium, 6–7 Hard, 8+ Challenge. Unknown rating uses Edit. Use
`--tier-policy edit` for Edit everywhere, or change `--star-tier-bounds` for a
source rating scale. Extraction references do not acquire those categories.

The earlier energetic preview policy selected a mostly active passage in the
middle 20–80% of the track, using half-second RMS and a 15%-of-peak threshold.
Those existing local artifacts remain intact. New exports select the earliest
[pickup supported by audio and chart heads](#earliest-music-pickup-previews),
rather than a later energetic excerpt. `--preview-seconds` accepts 1–30 seconds;
short remaining passages use their available length instead of moving the start
back into the quiet intro. Music stays bounded to ten minutes, mono/stereo PCM16.

```sh
# WAV needs only installed FFmpeg; Ogg additionally needs the compiled helper.
python3 tools/danoni-dump/ruffle/prepare_pack.py SOURCE_EXPORT /tmp/pack-new \
  --audio wav --tier-policy stars --star-tier-bounds 1,3,5,7 --preview-seconds 12

# See danoni-beat-grid.md for the libvorbis helper build recipe.
python3 tools/danoni-dump/ruffle/prepare_pack.py SOURCE_EXPORT /tmp/ogg-new \
  --audio ogg --vorbis-encoder /tmp/encode-vorbis
node tools/danoni-dump/ruffle/verify_audio.mjs \
  SOURCE_EXPORT /tmp/ogg-new /tmp/audio-check-new.json

UV_CACHE_DIR=/tmp/ddi-analysis-cache NUMBA_CACHE_DIR=/tmp/ddi-numba-cache \
uv run --python 3.14 --with-requirements tools/danoni-dump/ruffle/requirements-analysis.txt \
  python tools/danoni-dump/ruffle/analyze_tempo.py SOURCE_EXPORT /tmp/tempo-new.json
```

Optional `--preferred-bpm CHART_ID=BPM` analysis hints select a candidate alias
for local-window fitting, with explicit provenance. They do not force the final
BPM, affect playback or bypass phase checks. The analyzer combines distinct taps
and hold heads, fits chart candidates and stereo-safe spectral onset envelopes,
then reports dominant windows and excluded portions. See
[the beat-grid document](danoni-beat-grid.md#dominant-portions-and-explicit-review-hints)
for thresholds, report fields and current results. Sparse data (fewer than eight
distinct note times), out-of-range tempo searches and unresolved phase need
review; a strong sub-beat fit is not proof of a musical quarter-beat grid.

## Validation and scope

Original synthetic tests cover star overrides/unknowns, configurable boundaries,
intro silence and opposite-phase stereo previews, exact pre-roll/holds/scrolls,
7/9-key shared-audio packs, fractional-time rejection, and overlapping-note
rejection. Tempo tests cover known phase/tempo, aliases, faster minority sections,
phase jumps and insufficient input. Vorbis tests check mono/stereo granule counts,
rates and malformed input; real pack preparation also requires FFmpeg-decoded
count/rate/channels to match. Chrome compares both channels in energetic windows
from each track's thirds, recording quiet windows as unverified rather than
correlating silence. Its report claims decoded alignment, not source Flash or
device latency. All downloaded music, charts and results stay local.

The final shared builder was checked through the game's SSC parser against:

| Work | Compared data | Result |
|---|---|---|
| Kouhaku | Six charts, 2,493 taps, source stars, derived tiers and previews | All timestamps agree to 1e-12 seconds; five mono Ogg tracks have matching counts and zero Chrome sample lag. |
| Higan | 562 taps, 9 holds, 12 scroll events; unknown rating | All timestamps/multipliers agree to 1e-12 seconds; Edit retained; stereo MP3-to-WAV counts and both channels agree at zero lag. |

Higan's 342 colour targets are explicitly recorded as unrepresented, with full
source evidence retained. Its stereo audio also runs through the shared tempo
analyzer without a family-specific path. Synthetic 7/9-key cases establish
normalization/packaging behavior, not a real 7/9-key legacy extraction milestone.
The next corpus work should normalize an existing complete HTML5 7-key dump,
then cover native 7/9-key families with independent timebase evidence. Musical
colours still need a separate reviewed beat-grid model in the game; importer and
play-side changes belong to the other session.


## Display BPM handover: exporter and menu are separate

The initial `2004-kouhaku-generic-pack` ZIP contains no `#DISPLAYBPM`; hypotheses
were left in its separate analysis report. This explains its 75 BPM menu label.
The shared builder now accepts repeatable `--display-bpm CHART_ID=BPM` or
`CHART_ID=LOW:HIGH`; quote `CHART_ID=*` for unknown. Only explicit review input
is emitted. The chart ID selects its shared-audio song; inconsistent overrides
for charts sharing audio are rejected, as are unknown chart IDs, nonfinite or
nonpositive values and reversed ranges. Defaults remain unchanged. The option
is generic and does not load or auto-accept the analyzer's best hypothesis.

```sh
python3 tools/danoni-dump/ruffle/prepare_pack.py \
  "$HOME/Music/DanOni/legacy-pilot/exports/2004-kouhaku" /tmp/display-pack-new \
  --audio ogg --vorbis-encoder /tmp/encode-vorbis \
  --display-bpm tougenkyo=160 --display-bpm run=175 --display-bpm kitakaze=110 \
  --display-bpm korokke=150 --display-bpm emy=200
```

The private `~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-display-bpm/playable.zip`
uses these rounded, reviewed nominal musical labels. 北風's 110 is its dominant
portion's nominal tempo, not a claim of constant tempo through the faster parts.
The game parser reads all five `DisplayBpm::Single` values while all six charts'
2,493 note times, previews and difficulty tiers remain unchanged. Audio hashes
are identical to the prior pack; its existing Chrome proof still applies.
`chart-verification.jsonl` records parser values and the current menu values.

**The current menu still needs a game-side change.**
[`library::manifest::summarize`](../../library/src/manifest.rs:103) honors
`song.display_bpm` only when `song.source.format == SourceFormat::Danoni`.
Reconstructed `.ssc` files have SSC format, even with `danoni-5` steps types, so
this function takes `song.timing.bpm_range()` and emits 75. Merely adding the
standard tag cannot fix that menu. This was reproduced with the real pack:
parsed display values 160/175/110/150/200, menu values all 75.

The implementation session should honor explicit `DisplayBpm::Single` and
`Range` regardless of source format. `Actual` should retain the normal timing
fallback, and DanOni's absent/unknown display tempo should remain blank rather
than expose the synthetic clock. Preserve the desired Random/unknown policy.
Suggested regression: parse SSC with `#BPMS:0=75;#DISPLAYBPM:200;` and assert the
summary's BPM is `200`, while judgement times still use 75. Also cover a range,
ordinary simfiles without a display override and native DanOni's unknown BPM.
The imported manifest stores the menu label at import time; previously stored
entries need reimport or explicit migration/recomputation after that fix.

This is a display-label fix only. Note colours and beat-driven scroll behavior
require the separate musical beat-grid work; never replace `#BPMS` with the
nominal display value without repositioning notes.


## Opt-in musical timing with a strict error budget

The maintainer accepts single-digit millisecond changes for a musical-grid
export. `--musical-grid REVIEW.json` therefore enables a **separate approximate
export**, while the default exact frame export and source references remain
unchanged. It repositions notes together with BPM/offset; simply replacing
`#BPMS:0=75` would change timing substantially. This path works with the current
game's quantization colours and menu without a game-model change.

The review file is UTF-8 JSON, stored outside Git, with schema
`ddi-reviewed-musical-grid/experimental-1`:

```json
{
  "schema":"ddi-reviewed-musical-grid/experimental-1",
  "sourceManifestSha256":"<SHA-256 of the exact source manifest bytes>",
  "charts":{
    "normal":{
      "bpm":159.7875,
      "beatZeroSeconds":0.120866,
      "subdivisionsPerBeat":2,
      "maximumErrorMs":9,
      "phaseStatus":"Provisional subdivision origin; quarter phase unverified"
    }
  }
}
```

Every source chart ID must appear exactly once. Charts sharing audio must share
the same grid. BPM must be finite and in 60–240, absolute beat-zero time at most
600 seconds, subdivisions one of 1/2/3/4/6/8, and maximum error positive and
**below 10 ms** (default 9). Additional grid/review metadata is retained. This is
explicit input, not automatic acceptance of an analyzer report. The source
manifest hash binds the review to its original export.

For each tap or hold endpoint, compute its nearest chosen rhythmic subdivision.
If its shift fits the budget, use that exact subdivision; otherwise choose the
nearest 48-ticks-per-beat tick. Reject any endpoint still beyond budget, any
collapsed/conflicting cell or invalid/overlapping hold. Speed events use fine
ticks, with the same error cap. For negative pre-roll, pad by whole beats and
subtract that padding with chart offset, preserving quantization colours.

With `q=60/(BPM*48)`, export time is `beatZeroSeconds + tick*q`. Nearest-fine-tick
error is at most `q/2`, about 3.1–5.7 ms for these tracks. Snapping frame-rounded
notes to intended quarters/eighths can remove their one-tick colour noise, but
may shift them by up to the explicit 9 ms cap. Holds' endpoints are checked
independently; their duration can change by up to twice that cap. Fitted tempos
are retained rather than rounded integers, to avoid accumulating phase drift.
The beat origin is provisional when only a subdivision fit is known; quarter
versus eighth phase may still need listening/review.

The derived manifest changes as follows:

- Top-level `timingBpm` is null (read each song), `noteTimingModified:true`, and
  `musicalGridReviewSha256` records the supplied review bytes. Its contents are
  copied to `musical-grid-review.json` outside the playable ZIP.
- Songs have `timingBpm` and `timingOffsetSeconds` (negative reviewed beat zero).
  Chart-specific padding is encoded in SSC. `displayBpm` remains a separate
  optional nominal label; it does not change the quantizer.
- Each chart has `musicalQuantization:{grid,noteEndpoints,rhythmicEndpoints,
  fineTickFallbackEndpoints,maximumNoteErrorMs,meanNoteErrorMs,
  maximumScrollEventErrorMs,endpoints,scrollEvents,scope}`. Default exact exports
  have null `musicalQuantization` and null `musicalGridReviewSha256`.
- Each endpoint is `{lane,kind,index,sourceSeconds,exportSeconds,shiftMs,tick,
  method}`. `lane` and source array `index` are zero-based; `kind` is `tap`,
  `hold-head` or `hold-tail`. Signed `shiftMs` is export minus source time;
  `tick` is the unpadded musical tick. Method is `rhythmic-subdivision` or
  `fine-tick-fallback`. No fallback is hidden.
- Scroll event records are `{index,sourceSeconds,exportSeconds,shiftMs,tick,
  method:"fine-tick"}`. Their multipliers remain unchanged in source references
  and SSC. All numerical timing units and error limits above apply equally.

```sh
python3 tools/danoni-dump/ruffle/prepare_pack.py SOURCE_EXPORT /tmp/musical-pack-new \
  --audio ogg --vorbis-encoder /tmp/encode-vorbis \
  --musical-grid /tmp/reviewed-grid.json
python3 tools/danoni-dump/ruffle/test_musical_grid.py
```

### Measured private musical pack

`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-musical-grid-v3/playable.zip`
contains five songs and six charts, with prior audio/previews/tiers preserved.
Independent parsing compares **all 2,493 notes** with their original timestamps;
no notes are lost. Menu summaries now report 160/175/110/150/200 because these
SSC files actually use the fitted musical timing grids, unlike the display-only
variant. The existing display-tag summary issue remains for exact-frame packs.

| Chart | Fitted timing BPM | Maximum additional note shift | Fine-tick fallbacks |
|---|---:|---:|---:|
| 桃源郷 | 159.7875 | 8.431 ms | 0 / 333 |
| 桃源郷 Hard | 159.7875 | 8.478 ms | 0 / 605 |
| RUN | 174.7568 | 7.899 ms | 0 / 401 |
| 北風のメロディー | 109.6703 | 8.893 ms | 186 / 433 |
| ころっけわっしょい | 149.8127 | 8.331 ms | 0 / 407 |
| emy | 199.7594 | 8.703 ms | 2 / 314 |

桃源郷 and RUN classify entirely as quarter/eighth notes; ころっけ is all quarters;
emy has 312 quarter/eighth notes and two finer exceptions. 北風's irregular
sections retain mixed fine subdivisions rather than being forced onto the
110 BPM rhythmic lattice (which could shift them over 100 ms). A reviewed
piecewise tempo map would improve colours there; this exporter currently
supports one constant musical grid per audio track. Its quarter-beat origins
remain provisional, so colours are an estimate rather than recovered authoring
metadata. Changing that phase among equivalent subdivisions can change colour
labels without changing event times.

The same musical policy also passes on Higan: 562 taps, nine hold pairs and
12 scroll events, maximum note/endpoint shift about 8.44 ms. Source colours are
still recorded as unrepresented. Original synthetic tests cover frame-rounded
rhythmic snapping, fine-tick outliers, strict-budget rejection, collapsed holds,
whole-beat pre-roll padding, source-bound review files and a 7-key pack. The
musical ZIP's five audio hashes match the already validated exact pack byte for
byte; no transcode or source-sync correction was added by grid quantization.
All music, per-work review JSON and validation reports remain local.


## Scroll speed must stay independent of musical colouring

Maintainer clarification: inferred musical BPM/phase is wanted for rhythmic
alignment and colours, **not to replace the work's original DanOni vertical
speed**. These are separate concerns: source judgement times, musical grid, and
native motion (base speed, speed changes and per-note boost). Keep that separation
in future normalization and game integration.

The musical SSC variant above changes ordinary beat-based X-mod motion too:
[`ScrollState`](../../engine/src/scroll.rs) uses displayed beat distance times
the player multiplier. With no compensating scroll segments, a track at 199.759
BPM moves about **2.663 times** as fast as its earlier 75 BPM SSC at the same
setting. The 9 ms bound covers note timestamps only, not visual travel speed.
C-mod motion ignores timing BPM, but also ignores author scroll segments, so it
is not a general native-motion preservation solution. The v3 ZIP is therefore a
rhythmic quantization experiment, **not a native-scroll-fidelity export**.

A constant `75/musicalBpm` scroll multiplier can preserve the *previous SSC's*
X-mod rate while changing its musical grid; that is not proof of matching the
original SWF. Native preservation needs the original speed scale and field
geometry/approach time. Higan records `authorSpeed:1.5`, `initialPixelSpeed:3`;
Kouhaku validates `_global.SpeedUp=2`, `SpeedY=188`; at that point these raw
variables had not yet become a verified normalized motion model. The fresh
measurements and explicit baseline export below now cover its default motion. Its reconstructed CW
`difData` speed 1 is an export placeholder and must not be presented as the
original author's speed.

The concurrently advanced main checkout already has `SpeedSource::Chart` and
`speed_for_chart` for native DanOni metadata (`DanoniChart.init_speed`). It maps
modern CW units independently of musical BPM and keeps author scroll changes.
SSC charts lack that native metadata, so selecting chart speed cannot recover it
from v3. Do not change main's code from this research worktree.

Handover target: preserve exact source timing plus measured native motion, and
attach a separate reviewed musical grid/quantization hint used for colours and
beat-driven decoration. If an approximate musical SSC remains useful, make its
scroll compensation and baseline explicit and verify motion against the native
capture before calling it original-speed preservation. Capture consecutive note
positions, source arrow/field dimensions, receptor position and units; record
base velocity and events separately from inferred BPM. The source's frame clock
and music origin remain the reference. Quarter-phase review and 北風's changing
tempo sections are still independent tasks.


## Measured native scroll with a tunable correction

The maintainer approved a trial SSC that keeps the reviewed musical grid and
sets an authored native baseline beneath the player's X-mod. New optional
`--native-scroll REVIEW.json --scroll-factor F` implements that without changing
user settings. This applies with or without `--musical-grid`; defaults without
a native review remain unchanged. A factor other than 1 without a native review
is rejected rather than silently doing nothing.

### Generic motion measurement

[`measure_motion.py`](danoni-consolidation.md#retired-tools) (retired) reads
existing `run.json`/`snapshot.json` from the restricted worker. It never executes
SWFs, scripts, getters or methods. The caller selects moving clip names, a
reference sprite and a previously verified frame clock. It matches **scoped
name paths**, not the local display/object IDs, across adjacent captures.

```sh
python3 tools/danoni-dump/ruffle/measure_motion.py NATIVE_CAPTURE_DIR /tmp/motion-new.json \
  --fps 60 --moving-name-regex '^(left|down|up|right|space)MC[0-9]+$' \
  --reference-name left_step --reference-dimension width
```

The expression/reference above select this particular engine; they are caller
parameters, not generic-tool song rules. Width or height can be the reference.
Choose an appropriate square arrow/receptor dimension, not an average across
wide onigiri symbols. Native FPS must be independently established: do not infer
it solely from the header of an arbitrary movie. The tool rejects failed parent
runs, halted/incomplete graphs and lost/error audit evidence. It needs at least
two moving observations and a stable positive reference dimension. Translation
must preserve width, height and horizontal position, and every accepted observed
velocity must agree within 1e-6 source pixels per frame; varying motion needs a
more specific measurement window/model. Non-translation exclusions are recorded.
Path reuse by scripts can still alias objects; the family adapter must establish
identity/lifetime. Normalized velocity does not imply identical spawn geometry,
field height or reading time on another screen.

The UTF-8 report schema is `ddi-native-motion-measurement/experimental-1`:
`sourceSwfSha256`, `fps`, `clockSource`, `movingNameRegex`, `referenceName`,
`referenceDimension`, `referencePixels`, `signedPixelsPerFrame`,
`pixelsPerSecond` (absolute), `baseArrowHeightsPerSecond`, `samples`,
`references`, `excluded`, `scope`, `limits`. CLI output also records
`sourceCapture`, `snapshotSha256`, `parentReportSha256`, `workerSha256`,
`configSha256`, `sandboxSha256`, `retrievedAt`, `measurementSourceSha256`.
Samples contain scoped `path`, first/last iteration and stage bounds, and signed
pixels/frame and pixels/second. Reference records contain iteration, name,
stage bounds and dimension in pixels. The normalized base is absolute source
pixels/second divided by the selected reference dimension. No work assets become
repository fixtures.

### Review and SSC contract

The source-bound native review is UTF-8 JSON:

```json
{
  "schema":"ddi-reviewed-native-scroll/experimental-1",
  "sourceManifestSha256":"<SHA-256 of exact normalized source manifest bytes>",
  "charts":{
    "normal":{
      "baseArrowHeightsPerSecond":2.7906976744186047,
      "evidence":{"path":"captures/normal/motion.json","sha256":"<measurement bytes SHA-256>"}
    }
  }
}
```

It must cover every chart ID exactly. Base velocities must be finite, positive,
and at most 1000 arrow heights/second; additional metadata is preserved. Review
values may be explicit manual input. When `evidence` contains a path, the tool
requires its hash, resolves it within the review directory, verifies its schema,
base velocity and source SWF identity where the normalized manifest provides
one, and copies the measurement bytes into `references/native-motion-N.json`.
A changed file, rate mismatch or different SWF rejects preparation. Raw captures
and source inputs remain in their separate private archive.

For a chart at constant musical BPM `B`, measured native baseline `V` and
correction factor `F`, the authored base ratio is **`R=V*F*60/B`**. SSC gets an
initial `#SCROLLS:0=R;`; each source relative speed change `m` becomes a scroll
ratio `R*m` at its original/quantized event time. This represents native base
motion plus its relative changes. Per-note boost and other omitted effects
remain explicitly unrepresented. No `#SPEEDS`, player setting or speed-mod mode
is forced. With normal X-mod `X`, resulting base velocity is **`V*F*X`**.
C-mod remains the player's explicit time-based override and ignores the map.
Other selected visual effects retain their existing semantics.

If a positive musical beat origin would leave the initial authored ratio after
audio zero, add whole measures of zero note rows and compensate chart offset.
This begins the native map before music time zero while preserving judgement
seconds, subdivision buckets and the four-beat colour cycle. Those extra rows
are padding, not added silence or delay.

New output fields (still `ddi-playable-pack/experimental-1`):

- Top-level `nativeScrollReviewSha256` (null without review),
  `scrollPolicySourceSha256`, and `scrollCorrectionFactor` (default 1).
  The review is copied to `native-scroll-review.json`, outside the playable ZIP.
- Each chart `nativeScroll` is null or
  `{sourceMotion,correctionFactor,sscBaseRatio,baseArrowHeightsPerSecond,
  playerMultiplierModified:false,mode,formula}`; it also has `evidenceStoredAt`
  when a measurement was verified/copied. `sourceMotion` preserves the entire
  supplied per-chart review. `baseArrowHeightsPerSecond` includes the correction
  factor; `sscBaseRatio` is the value before source relative changes.
- Existing source references and `musicalQuantization` diagnostics remain
  unchanged. `noteTimingModified` concerns musical quantization, not scroll
  compensation; native scroll alone leaves it false.

### Kouhaku result and tuning

Six fresh restricted runs of the same cached SWF verify all six chart defaults.
Tracked clips move **−2 source pixels/frame**, or **120 px/s** at the independently
established 60 Hz clock. The selected `left_step` reference is **43 px wide**,
giving **120/43 = 2.790697674 arrow-size units/s**. Three/twelve/six/six/four/six
tracked intervals agree for 桃源郷/Hard/RUN/北風/ころっけ/emy respectively. Static
inspection also identifies `_y -= _global.SpeedUp`; default `SpeedUp=2` agrees
with the observed motion. Unlike the old reconstructed CW speed placeholder,
this is measured evidence. [Original work page](http://eucaly.net/~iso50/kouhaku.html).

Measurements, raw snapshots, sandbox reports and dedicated cached inputs live at
`~/Music/DanOni/legacy-pilot/motion/2004-kouhaku/`. The review records the archive
mapping without rewriting original retrieval paths/dates. The earlier music
and chart-reference exports remain intact; no source file was refetched.
北風's measurement enters the original menu at iteration 160 instead of waiting
to 250, so its first arrows fit the existing 600-iteration cap. No script data
was patched. All runs use the same pinned worker and restrictive sandbox.

The new private pack is
`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-musical-native-scroll-v3/playable.zip`
(3,393,935 bytes). It keeps the earlier musical note positions, colour buckets,
nominal display labels, audio/previews/tiers and 8.893 ms maximum additional
source timing error. Its native baseline changes by BPM only through the inverse
compensation, not through the resulting motion:

| Song | Fitted BPM | Authored base `#SCROLLS` ratio |
|---|---:|---:|
| 桃源郷 (both charts) | 159.7875 | 1.047903375 |
| RUN | 174.7568 | 0.958141978 |
| 北風 | 109.6703 | 1.526774709 |
| ころっけ | 149.8127 | 1.117674766 |
| emy | 199.7594 | 0.838217880 |

The existing game parser and scroll engine independently verify all 2,493 note
times/colour buckets and all six charts at **X1: 2.7907**, **X2: 5.5814**,
**X3: 8.3721** arrow heights/s, within 1e-5 of the measured target. The new map
adds no additional note-time quantization. Audio hashes match the previously
Chrome-validated musical pack exactly. Reports are `chart-scroll-verification.jsonl`
and `scroll-only-comparison.json` alongside the pack. Synthetic tests additionally
cover changing display IDs, malformed/incomplete motion evidence, author speed
changes, 9-key preparation, source binding, tampered measurements and tunable
factors. This is relative arrow-size motion; different field heights can yield
different approach times. Quarter phase and 北風's irregular colour sections
remain the earlier provisional musical-grid limits.

```sh
python3 tools/danoni-dump/ruffle/prepare_pack.py \
  "$HOME/Music/DanOni/legacy-pilot/exports/2004-kouhaku" /tmp/native-musical-pack-new \
  --audio ogg --vorbis-encoder /tmp/encode-vorbis \
  --musical-grid "$HOME/Music/DanOni/legacy-pilot/exports/2004-kouhaku-musical-grid-review.json" \
  --native-scroll "$HOME/Music/DanOni/legacy-pilot/motion/2004-kouhaku/native-scroll-review.json" \
  --scroll-factor 1 \
  --display-bpm tougenkyo=160 --display-bpm run=175 --display-bpm kitakaze=110 \
  --display-bpm korokke=150 --display-bpm emy=200
python3 tools/danoni-dump/ruffle/test_scroll_policy.py
```

For a subjective adjustment, regenerate to another fresh directory with
`--scroll-factor 1.2` for 20% faster native baseline, or `0.8` for 20% slower.
The factor is finite, positive and at most 100. It changes authored scroll only;
notes, BPM, colour quantization, audio and user X-mod remain unchanged. For a
simple test of the baseline, choose X1 and normal visual scroll; normal user
multipliers work on top as requested. Nothing changes the game's defaults.


## Earliest music-pickup previews

Maintainer feedback: previews should begin when the song gets going, taking the
first note as a candidate but skipping quiet passages and isolated opening
notes. The new default
[`preview_policy.py`](danoni-consolidation.md#retired-tools) (retired) combines
raw audio-relative chart heads with decoded PCM strength; it does not depend on
engine family, key count, musical-grid estimation or source star rating. It
sets `#SAMPLESTART`/`#SAMPLELENGTH` only. Playback audio, note timing, colour grid,
BPM, native scroll correction and player settings are unchanged.

### Selection rules

1. Measure RMS in **100 ms PCM windows**, combining channel power rather than
   waveform amplitudes so opposite-phase stereo does not cancel. Ignore windows
   below one PCM quantization step when finding the typical level. Use their
   **75th-percentile RMS** as the reference, and default quiet threshold
   **35% of that level**, with a one-step floor. A single late loud peak therefore
   does not determine the reference level.
2. Enumerate distinct nonnegative tap/hold-head timestamps within the audio,
   per chart and in chronological order across the work. Chords count once;
   hold tails are not new attacks. Source seconds are used, independently of
   optional musical quantization or whole-measure SSC padding.
3. At each candidate, require nearby audio strength over **150 ms** and
   sustained strength in at least **two of the next three half-second windows**.
   At a short track's end, use the available windows. This rejects a short loud
   transient followed by a quiet opening as the full-music cue.
4. For chart support, require at least **four distinct heads within three
   seconds**, the first head at most **one second** ahead, and the following
   head at most **one second** later. A chart that never supplies such a pattern
   is excluded from density voting. Require a **strict majority** of informative
   charts, so a dense Hard opening alone cannot override a sparse Normal opening
   when both charts have informative patterns elsewhere. This remains a
   conservative song-level heuristic, not semantic detection of an arrangement.
5. Select the **first** candidate satisfying both audio and chart evidence.
   Start **80 ms before it** (clamped at audio zero) to retain the attack, and
   use up to 12 seconds of available music. Do not pull the start backward just
   to fit a full-length excerpt.

When joint evidence is unavailable, fallback in order to the earliest
supported-audio note (`audio-note-fallback`), earliest supported audio window
(`audio-only-fallback`), then the first above-threshold window
(`short-audio-fallback`). No activity yields start zero with `no-activity`.
A fallback is explicitly marked rather than claimed as a fully supported
music/chart pickup. A strong joint choice has status `selected`.

These rules cannot recognize instrumentation, melody entry or a chorus as a
listener does. Intro/middle sections with similar volume can remain ambiguous,
and easy chart density is not an objective measure of musical intensity.
Thresholds are exposed for review and tuning; no song-specific thresholds or
hardcoded starting timestamps are present.

### Tuning and report fields

The generic builder accepts:

| Option | Default | Meaning |
|---|---:|---|
| `--preview-seconds` | 12 | Clip length, 1–30 seconds; shortened at audio end. |
| `--preview-quiet-ratio` | 0.35 | Required RMS relative to typical strength, positive through 1. Higher skips softer openings. |
| `--preview-min-notes` | 4 | Distinct heads required, integer 2–64. |
| `--preview-max-gap` | 1 | Maximum first/following head gap in seconds, 0.1–10. |
| `--preview-note-window` | 3 | Lookahead for head count in seconds, 0.5–20. |
| `--preview-lead-in` | 0.08 | Time before chosen cue in seconds, 0–1; zero starts at the cue. |

`previewPolicySourceSha256` is added to the pack manifest. Each song's preview
records exact `settings`, `anchorSeconds`, `anchorType` (`note`/`audio`),
`firstNoteSeconds`, `firstActiveSeconds`, `thresholdRms`, `typicalRms`,
`activeFraction`, `chartIds`, `informativeChartIds`, `sparseChartIds`,
`requiredChartSupport`, `selectedAudioSupport`, `selectedChartSupport`,
`skippedCandidates`, `quietCandidates`, `sparseCandidates`,
`rejectedJointCandidates`, `firstSkippedCandidates` and `method`, along with
start/length/status. Activity fraction is the fraction of 100 ms windows
intersecting the excerpt that exceed the quiet threshold; it is not a
probability of musical correctness.

Audio support is `{startRms,sustainRms,strongHalfSecondWindows,
availableHalfSecondWindows,supported}`. Chart support maps chart IDs to
`{notes,firstNoteGapSeconds,nextNoteGapSeconds,supported}`. Diagnostic gaps
can be null when insufficient heads exist. At most 16 earlier rejected
candidates are retained, each `{noteSeconds,reason,audio,chartSupport}`;
reasons are `quiet-or-unsustained-audio` or `isolated-or-sparse-notes`.
Skip counts concern candidates before the chosen cue; `rejectedJointCandidates`
counts the whole joint search, which can scan later material before choosing a
fallback. Silent output has only start/length/status, RMS reference/settings and
method because no meaningful cue is available.

```sh
# Append these options to the musical/native-scroll command above if tuning:
# --preview-quiet-ratio 0.45 --preview-min-notes 4 --preview-max-gap 1 \
# --preview-note-window 3 --preview-lead-in 0.08 --preview-seconds 12
python3 tools/danoni-dump/ruffle/test_preview_policy.py
```

### Current private pack and checks

The new local artifact is
`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-pickup-previews-v2/playable.zip`
(3,393,949 bytes), with these default choices:

| Song | Earlier energetic preview | New pickup preview | Chosen source note |
|---|---:|---:|---:|
| 桃源郷 (shared Normal/Hard) | 83.00 s | **14.87 s** | 14.95 s |
| RUN | 60.00 s | **10.9033 s** | 10.9833 s |
| 北風のメロディー | 99.50 s | **8.5367 s** | 8.6167 s |
| ころっけわっしょい | 48.00 s | **8.0033 s** | 8.0833 s |
| emy | 87.50 s | **11.57 s** | 11.65 s |

All five choices have joint `selected` status; their per-work diagnostics show
which quiet/isolated candidates were rejected. Higan independently selects a
4.0333 s head (preview 3.9533 s), skipping three sparse candidates from its first
head at 0.55 s despite already strong audio. This exercises a different engine,
stereo input and hold heads without another family rule or new acquisition.

Seven original synthetic tests cover quiet-but-nonzero intros, loud music with
an isolated early note, misleading loud transients, multi-chart density votes,
chords/hold tails, late pickup on a short track, silent audio, opposite-phase
stereo and invalid settings. All related pack/grid/scroll tests pass. The game
parser/engine comparisons were rebuilt and run with **Rust 1.99.0**, after the
maintainer's upgrade, and check all six charts' preview tags, 2,493 note times,
colour buckets and X1/X2/X3 scroll rates against the accepted native-scroll pack.
A byte comparison of SSC after removing only the two preview tags is identical;
all five audio hashes are also identical. `preview-only-comparison.json` and
`chart-scroll-verification.jsonl` retain that evidence beside the ZIP. Existing
Chrome decoding evidence remains applicable to those identical audio bytes.
The picker has not been claimed to recover an author's original preview;
these are starting points for the maintainer to listen to and adjust.
