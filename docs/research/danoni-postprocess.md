# Shared DanOni pack preparation and estimates

Implemented and checked locally on 2026-10-09. The shared
[`prepare_pack.py`](../../tools/danoni-dump/ruffle/prepare_pack.py),
[`pack_policy.py`](../../tools/danoni-dump/ruffle/pack_policy.py) and
[`analyze_tempo.py`](../../tools/danoni-dump/ruffle/analyze_tempo.py) contain no
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
- `preview`: `startSeconds`, `lengthSeconds`, `status`, `thresholdRms`; selected
  previews also have `firstActiveSeconds`, `activeFraction` and `method`.
  `status` is `selected` or `no-activity`. `firstActiveSeconds` is an RMS-threshold
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

The preview picker uses half-second RMS windows, an activity threshold of 15%
of peak RMS (at least one PCM quantization step), and three consecutive active
windows where available. It selects an energetic, mostly active 12-second
excerpt after the intro, generally within the middle 20–80% of the track.
`--preview-seconds` accepts 1–30 seconds and short tracks use their available
length. Silent tracks report `no-activity`; this is a useful excerpt heuristic,
not chorus recognition. Long music is bounded to ten minutes, mono/stereo PCM16.

```sh
# WAV needs only installed FFmpeg; Ogg additionally needs the compiled helper.
python3 tools/danoni-dump/ruffle/prepare_pack.py SOURCE_EXPORT /private/tmp/pack-new \
  --audio wav --tier-policy stars --star-tier-bounds 1,3,5,7 --preview-seconds 12

# See danoni-beat-grid.md for the libvorbis helper build recipe.
python3 tools/danoni-dump/ruffle/prepare_pack.py SOURCE_EXPORT /private/tmp/ogg-new \
  --audio ogg --vorbis-encoder /private/tmp/encode-vorbis
node tools/danoni-dump/ruffle/verify_audio.mjs \
  SOURCE_EXPORT /private/tmp/ogg-new /private/tmp/audio-check-new.json

UV_CACHE_DIR=/private/tmp/ddi-analysis-cache NUMBA_CACHE_DIR=/private/tmp/ddi-numba-cache \
uv run --python 3.14 --with-requirements tools/danoni-dump/ruffle/requirements-analysis.txt \
  python tools/danoni-dump/ruffle/analyze_tempo.py SOURCE_EXPORT /private/tmp/tempo-new.json
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
  "$HOME/Music/DanOni/legacy-pilot/exports/2004-kouhaku" /private/tmp/display-pack-new \
  --audio ogg --vorbis-encoder /private/tmp/encode-vorbis \
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
python3 tools/danoni-dump/ruffle/prepare_pack.py SOURCE_EXPORT /private/tmp/musical-pack-new \
  --audio ogg --vorbis-encoder /private/tmp/encode-vorbis \
  --musical-grid /private/tmp/reviewed-grid.json
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
Kouhaku validates `_global.SpeedUp=2`, `SpeedY=188`, but those raw variables have
not yet become a verified normalized motion model. Its reconstructed CW
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
