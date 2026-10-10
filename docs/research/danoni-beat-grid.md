# Numeric difficulty, Ogg exports and estimated musical grids

Research and local validation: 2026-10-09. The maintainer tested the
[2004 Kouhaku pack](danoni-kouhaku.md) and reports good audio sync, with a possible
roughly 20 ms discrepancy on a few songs whose origin is uncertain. This is
useful play-test evidence, not a measured per-song offset. No timing correction
has been applied; source Flash compensation remains unverified.

The [default-tempo policy](danoni-default-tempo.md) now applies bounded fitting
automatically to all normalized works; the experiments below remain historical
evidence. Explicit Kouhaku reviews still take precedence.

## Difficulty is a source rating

The Kouhaku adapter `/2` exposes `difficultyStars` as an integer: **3, 8, 6, 5,
4, 5**, in source selection order. `rawDifficulty` preserves the original star
string as provenance; `difficultyClassification` is null. The extraction makes
no Easy/Medium/Hard mapping and does not sort charts by an invented category.
For example:

```json
{"difficultyStars":3,"rawDifficulty":"★★★","difficultyClassification":null}
```

This metadata sits on the native reference's `metadata`; the extraction manifest
also exposes each chart's numeric count. The simfile meter now carries that
number, its description is empty, and `Edit` is solely the mandatory simfile
category placeholder. CW export uses the numeric third component of the
`difData` label/maker/level tuple, with empty name/maker fields. Preserve source
rating scale and let the library/UI choose classification and sorting. Older
adapter `/1` references retain their original free-text field; they are not
rewritten in place.

## Smaller audio with the tested origin

A derived pack at
`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-stars-ogg-v2/` contains five songs
and six charts. Its `playable.zip` is approximately **3.2 MiB**, versus about
26.3 MiB for the WAV ZIP. Vorbis quality 5 encodes the tested untrimmed mono
22,050 Hz WAVs. Original MP3/WAV extraction remains in its sibling directory.

Installed libvorbis **1.3.7** is the latest stable release listed by
[Xiph](https://www.xiph.org/downloads/). The small MIT
[`encode_vorbis.c`](../../tools/danoni-dump/ruffle/encode_vorbis.c) helper accepts
bounded mono/stereo PCM16LE on stdin and uses that public encoder API; it performs no
resampling or timestamp correction. The derived manifest records source/output
hashes, encoder binary hash/version and command. The system FFmpeg build lacks
`libvorbis`; attempted Python/libsndfile encoding crashed on full tracks, so
those incomplete local attempts were rejected and retained separately.

Independent FFmpeg decoding matches all five source sample counts. Installed
Chrome also decodes identical lengths, rates and channel counts. Correlation
windows near each track's beginning, middle and end have their maximum at
**zero sample lag** (correlation about 0.979–0.999); no timing shift is observed
from this transcode. The report's scope is decoded PCM alignment, not device
latency or original Flash compensation. Vorbis timing is carried by Ogg granule
positions, including end trimming.
[Specification: Ogg encapsulation](https://xiph.org/vorbis/doc/Vorbis_I_spec.html#x1-132000A).

A synthetic short-track test also exposed FFmpeg padding/trimming differences
for arbitrary final packets despite correct granule duration. The repacker
therefore checks actual decoded counts and rejects a mismatch; the Chrome check
adds independent decoder evidence. It does not claim all decoders produce the
same PCM for every possible Vorbis file.

## 75 BPM and note colours

75 BPM is the exact **storage clock**, chosen in PLAN decision 4: at 48 ticks
per beat, one 60 Hz frame is one tick. It is not musical tempo. The present
player derives quantisation from `note.tick.quantization()`; beat-based colours
therefore follow this synthetic grid, which explains the observed mismatch.
Changing `#BPMS` alone would change note times. Adding only `#DISPLAYBPM` can
change displayed tempo but cannot fix note subdivisions/colours.

A useful musical grid needs both **tempo and beat phase**. A sequence of note
times can identify a small subdivision period while leaving its interpretation
as quarters/eighths/sixteenths/triplets ambiguous. Audio tracking is also an
estimate: the chosen tempo prior can select a different pulse, and perceived
beats need not coincide with every chart note. Librosa's method uses onset
strength, tempo estimation and dynamic programming; it returns estimated beats,
not original author metadata.
[Librosa 1.0.0 implementation](https://github.com/librosa/librosa/blob/1.0.0/librosa/beat.py),
[algorithm documentation](https://librosa.org/doc/0.11.0/generated/librosa.beat.beat_track.html).

The optional [`analyze_tempo.py`](../../tools/danoni-dump/ruffle/analyze_tempo.py)
fits chart phase/period candidates, obtains audio beat tracks under 60/120/180
BPM priors, and checks 20-second chart windows. It deliberately keeps competing
hypotheses and sets `automaticAcceptance:false`, `timingModified:false`.
Coherence is a circular grid-fit statistic, **not a correctness probability**.
Neither the maximum score nor agreement between two methods proves the original
editor BPM or metre. The community's
[Chart Reverser](https://github.com/cwtickle/danoniplus-reverser) similarly needs
first-number/BPM/tempo changes to reconstruct an editor grid.

### Measured candidates for this collection

Local audio analysis and chart-grid fits provide these provisional hypotheses:

| Song | Chart-grid candidate | Audio evidence | Interpretation |
|---|---:|---|---|
| 桃源郷 (both charts) | 159.7875 BPM; equivalent 79.8935/106.525 subdivision grids | About 159.01 with 120/180 priors | Strong regular grid; roughly 160 is plausible. |
| RUN | 174.7565; equivalent 87.3785/116.5045 | About 175.19 with 180 prior; 116.13 with 120 prior | Strong regular grid; roughly 175 is plausible, but pulse choice matters. |
| 北風のメロディー | Global 109.807 or 219.6135 | Global audio estimates disagree; chart windows vary | Insufficient evidence for one global BPM/phase. |
| ころっけわっしょい | 149.8125; equivalent 74.9065 | About 149.80 or 74.90 depending on prior | Strong regular grid; roughly 150 is plausible. |
| emy | 99.8795; equivalent 199.759/66.5865/133.173 | About 100.35 or 198.77 | Strong grid; 100 versus 200 requires musical interpretation. |

For the four strong grids, 95th-percentile distance to the fitted subdivision
lattice is about 7.6–7.9 ms, consistent with frame rounding. This is a **grid-fit
residual**, not the audio-sync error. 北風's global residual is about 43 ms;
several later windows fit very well individually at slightly different periods.
That could reflect musical tempo variation, author timing changes or offsets;
this experiment does not distinguish them. Do not round the fitted BPM to a
nearby integer or assume a constant global phase without checking drift.

### Proposed game integration (owned by the importer/gameplay session)

Keep judgement timestamps on the existing exact frame clock. Add an optional,
separate musical-beat description for display, rhythmic colours and beat-driven
animation. This can be author-supplied, estimated or reviewed; unknown remains
unknown. It must carry beat-zero time and tempo segments/beat positions, evidence
identity, status and alternatives. Native chart timing must not depend on it.

The analysis JSON is a research report, not a stabilised importer schema.
`ddi-tempo-analysis/experimental-1` records source manifest/audio hashes, script
hash and package versions; per track it stores `chartCandidates` (BPM,
subdivisions, phase, coherence/residuals), `audioEstimates` (prior, BPM and beat
times in seconds), and `chartWindows`. `gridPhaseSeconds` is only subdivision
phase. It does **not** establish which subdivision is the quarter beat. A
consumer must choose/review a quarter-beat phase before deriving note colours.

Replacing the exact clock with a conventional 48-tick musical grid would
quantise frame timestamps again. At 175 BPM one tick is about 7.14 ms. If such
an approximate simfile export is later offered, label it separately and report
its maximum note-time error; keep exact frames as the reference.

## Reproduction

Optional analysis dependencies are pinned to the current stable releases:
librosa 1.0.0, NumPy 2.5.3, SciPy 1.18.1 and SoundFile 0.14.0. Librosa 1.0.0 is
ISC-licensed and requires Python 3.12+; the tested isolated environment uses
3.14. They are independent of the game and native Ruffle build.
[Release metadata](https://pypi.org/project/librosa/).

```sh
UV_CACHE_DIR=/private/tmp/ddi-analysis-cache \
NUMBA_CACHE_DIR=/private/tmp/ddi-numba-cache \
uv run --python 3.14 --with-requirements tools/danoni-dump/ruffle/requirements-analysis.txt \
  python tools/danoni-dump/ruffle/analyze_tempo.py \
  "$HOME/Music/DanOni/legacy-pilot/exports/2004-kouhaku" /private/tmp/tempo-new.json \
  --preferred-bpm emy=200 --preferred-bpm kitakaze=110

# macOS/Homebrew example; installed libvorbis/libogg, not vendored code.
clang -std=c11 -Wall -Wextra -Werror -O2 \
  -I/opt/homebrew/include -L/opt/homebrew/lib -Wl,-rpath,/opt/homebrew/lib \
  tools/danoni-dump/ruffle/encode_vorbis.c -lvorbisenc -lvorbis -logg \
  -o /private/tmp/encode-vorbis
python3 tools/danoni-dump/ruffle/prepare_pack.py \
  "$HOME/Music/DanOni/legacy-pilot/exports/2004-kouhaku" /private/tmp/kouhaku-ogg-new \
  --audio ogg --vorbis-encoder /private/tmp/encode-vorbis
node tools/danoni-dump/ruffle/verify_audio.mjs \
  "$HOME/Music/DanOni/legacy-pilot/exports/2004-kouhaku" \
  /private/tmp/kouhaku-ogg-new /private/tmp/audio-check-new.json
```

The shared builder reads normalized exports, performs no SWF execution or
fetching, and keeps the source export intact. New manifests use
`ddi-playable-pack/experimental-1`; the older `stars-ogg-v2` artifact retains its
`ddi-kouhaku-playable-pack/experimental-1` schema and Edit placeholders. See the
[generic post-processing contract](danoni-postprocess.md) for source ratings,
configurable derived tiers, active previews, layouts and output fields. Simfile
timing remains 75 BPM. Synthetic tests cover tempo/phase ambiguity, changes,
mono/stereo previews, 7/9-key packs and Vorbis granule durations; real pack
comparisons check note/hold/scroll timestamps and independent Chrome audio
alignment. Assets and all per-work analysis results remain local.


## Dominant portions and explicit review hints

The generic analyzer accepts `--preferred-bpm CHART_ID=BPM` to choose among
existing candidate aliases. These are recorded review inputs, never song-name
rules or assertions that the requested integer is exact. Unknown chart IDs and
conflicting hints for charts sharing music are rejected. The maintainer's emy
200 BPM interpretation selects the roughly **199.759** candidate. 北風's 110 hint
selects the roughly 109.807 global candidate as a starting point.

Twenty-second windows fit near that candidate. Only windows with coherence at
least 0.94 and p95 residual at most 12 ms are eligible; the dominant cluster is
weighted by distinct note times, allowing BPM differences of 0.15% of the seed.
At least two windows must agree. The retained union is refitted, and a poor joint
phase fit is labelled `tempo-consensus-phase-unresolved`, even if individual
windows share a tempo. All excluded indices, counts and retained fraction are
reported. This rejects inconsistent sections from the estimate, not the export.
The same policy applies to any normalized work; taps and hold heads contribute,
chords are counted once, and stereo onset envelopes are combined after spectral
analysis so opposite-phase channels do not cancel in a waveform downmix.

For 北風, three consecutive windows from about **28.62–88.62 s** retain **198 of
379** distinct note times. Their joint fit is **109.6703 BPM**, coherence 0.9939,
p95 residual **7.89 ms**. Introductory and later faster/inconsistent portions are
excluded from that fit. Every original note remains in the pack; this is not a
whole-song tempo map. emy's reviewed fit uses all 287 distinct note times at
about **199.7594 BPM**. Reports retain `automaticAcceptance:false` and the
unverified quarter-beat phase warning. The generic algorithm's synthetic tests
include a faster minority section and equal-tempo sections with phase jumps.

The newest private pack is
`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku-generic-pack/playable.zip`:
five songs, six SSC charts, 3,391,870 bytes. Twelve-second active previews start
at 83.0/60.0/99.5/48.0/87.5 seconds in song order. Derived tiers are Easy,
Challenge, Hard, Medium, Medium, Medium; the original numeric stars and source
references remain separate. All 2,493 taps match the source seconds to 1e-12.
FFmpeg sample counts and fresh Chrome energetic-window comparisons pass for all
five Oggs, with zero sample lag. These changes do not fix the game's beat colours;
the separate musical-beat model above is still needed.


## Optional approximate musical-grid export

After accepting single-digit millisecond quantization, the maintainer requested
notes on the fitted musical grid too. The
[opt-in export contract](danoni-postprocess.md#opt-in-musical-timing-with-a-strict-error-budget)
adds source-bound reviewed tempo/phase/subdivisions and a strict 9 ms cap.
Regular notes snap to their chosen rhythmic subdivision; outliers use fine
48-tick positions instead. This is a separate approximate SSC, not a rewrite of
the exact reference clock. All 2,493 Kouhaku taps survive with maximum additional
shift **8.893 ms**; Higan taps/holds/scrolls also pass the cap. Full signed endpoint
and fallback reports are retained locally.

The private `2004-kouhaku-musical-grid-v3/playable.zip` now provides musical timing
BPMs and rhythmic colour buckets to the existing game. 北風's changing portions
remain fine-tick fallbacks, and quarter phase remains provisional. Default exact
exports still use 75 BPM and need the separate display/beat-model integration;
the optional approximation does not replace that architectural plan.


### Follow-up: musical colours must not change native scroll

The maintainer clarifies that original DanOni vertical speed should be retained.
The musical SSC experiment changes X-mod scroll speed with its BPM, in addition
to its bounded timestamp quantization. It is not a native-motion export; emy's
rate at the same X-mod is about 2.663 times its former 75 BPM SSC. The
[scroll handover](danoni-postprocess.md#scroll-speed-must-stay-independent-of-musical-colouring)
records the current chart-speed support on main, missing SSC metadata and the
needed independent native motion/grid representation. Default exact reference
exports remain intact. The target is native timing/motion with musical colours,
not musical-BPM-driven replacement of the author's speed.


### Native base speed beneath the user's multiplier

The maintainer approved the SSC compensation trial. Fresh restricted captures
now measure default Kouhaku motion in all six charts: 120 source px/s divided by
a 43 px reference arrow width, about 2.7907 arrow-size units/s. The generic
[measured-scroll contract](danoni-postprocess.md#measured-native-scroll-with-a-tunable-correction)
adds a source-bound review and `--scroll-factor`, with inverse-BPM `#SCROLLS`
ratios. The player's X-mod applies unchanged on top. Native effects and source
relative speed changes are separate from musical-grid estimation; no musical
BPM or free-text speed label is used as a substitute for measured base motion.

The local `2004-kouhaku-musical-native-scroll-v3/playable.zip` retains the prior
musical positions/colours and adds the measured baseline. Independent engine
checks verify X1/X2/X3 rates for every chart, with no additional timing error or
audio changes. Reference-normalized motion can still feel different with a
different playfield height; factors 1.2/0.8 offer simple 20% adjustments without
altering BPM, notes or player settings. The earlier v3 musical-only ZIP remains
an uncompensated experimental variant, not the newest native-scroll trial.
