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
  path), `prepareSourceSha256`, `policySourceSha256`, `codec`, and `ffmpeg`
  (installed version line).
- `timingBpm:75`, `noteTimingModified:false` and
  `tierPolicy:{name,upperBounds,tiers}`. Bounds are the four increasing inclusive
  maxima for Beginner, Easy, Medium and Hard; larger values become Challenge.
- `songs[]`: `{id,title,music,musicSha256,sourceAudioSha256,pcmSampleRate,
  pcmChannels,pcmFrames,decodeCommand,encodingCommand,encoderVersion,
  encoderSha256,preview,simfile,charts}`. `music` and `simfile` are relative paths;
  `pcmFrames` counts sample frames, not interleaved samples. Encoding fields are
  null for WAV. Commands preserve exact argument arrays and local source paths.
- Each song's `charts[]`: `{id,difficultyStars,difficultyClassification,
  classificationSource,sourceDifficultyClassification,unrepresentedEffects}`.
  Unknown stars are null; source classification is separate from the derived
  string. `unrepresentedEffects` maps populated effect names to array lengths.
- `preview`: `startSeconds`, `lengthSeconds`, `status`, `thresholdRms`; selected
  previews also have `firstActiveSeconds`, `activeFraction` and `method`.
  `status` is `selected` or `no-activity`. `firstActiveSeconds` is an RMS-threshold
  heuristic, not a claim about perceived first sound. RMS uses normalized PCM
  amplitude across all channels; stereo channels are not summed as waveforms.

SSC uses `#STEPSTYPE:danoni-<mode>`, an empty description, numeric meter, derived
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
