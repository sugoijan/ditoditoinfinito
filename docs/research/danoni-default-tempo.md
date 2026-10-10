# Default musical timing and CW banner exports

Updated: 2026-10-10. The shared SSC preparer now attempts musical BPM/phase fitting
by default for every normalized work. Verified saved CW references can also be
normalized without reopening the source site, so their songs receive the same
reviewed `#BANNER`, pickup preview, Ogg and scroll policies as legacy conversions.
This supersedes the earlier exact-clock defaults and pending CW bridge described
in the [exports history](danoni-exports.md). Game crates are unchanged.

## Run and evidence

The initial capture and offline oracle verification still use the browser dump
tool. Conversion of an already verified capture uses saved plain data and static
inspection of its cached engine source, with no downloaded JS execution:

```sh
python3 tools/danoni-dump/ruffle/normalize_cw.py \
  "$HOME/Music/DanOni/corpus/example/manifest.json" \
  "$HOME/Music/DanOni/normalized/example"
```

The output must be a new directory outside Git. It contains:

- `manifest.json`: existing `ddi-normalized-work/experimental-1` contract;
  separate chart IDs `chart-<original zero-based CW index>`, per-chart music
  bindings, source title/artist/creator, numeric source rating or null.
- `source/`: byte-preserved original dump, including unsupported chart rows.
- `references/chart-<n>.json`: `ddi-cw-normalized-reference/1`, original reference,
  captured metadata and the audio-origin formula.
- `references/chart-<n>-motion.json` and `native-scroll-review.json`: source-bound
  proof of the default movement units and rate.
- `media/<source audio hash>/source.<ext>`, `music.wav`, `decode.json`: retained
  original and ordinary PCM decode, with no trim or added origin correction.

Only complete, verified built-in 5/7/7i/9A/9B captures are selected. 9i and 9t
remain evidence only. Nonstandard playback rates, nonzero user/fractional
adjustment, dummy notes, corrupt hold projections, unrecognized movement units
and captured Extreme/Soft velocity modes are rejected. This is deliberately a
standard-engine adapter, not a reconstruction of arbitrary custom JS behavior.

Before preparing a banner review, bind it to this normalized manifest using the
[existing artwork contract](danoni-artwork.md#reviewed-banner-integration).
Difficulties sharing music must share a banner. Current CW additions use plain
white source titles on black, including Japanese glyphs, at 640×200.

Build the registry with the existing `rebuild_corpus.py` command, running it in
the pinned analysis environment for the first fit:

```sh
uv run --with-requirements tools/danoni-dump/ruffle/requirements-analysis.txt \
  python tools/danoni-dump/ruffle/rebuild_corpus.py build \
  "$HOME/Music/DanOni/export-registry.json" \
  "$HOME/Music/DanOni/exports.staging-new" \
  --vorbis-encoder /private/tmp/encode-vorbis \
  --verifier target/debug/ddi-danoni-pack-verify
```

Use `--offline` on `uv run` once that environment is cached. The build itself
makes no source network requests or browser launches. Complete cached decisions
can be reused without loading scientific packages. The existing requirements
pin Librosa, NumPy, SciPy and SoundFile; no new dependency was added here.

`prepare_pack.py --tempo-policy auto|exact` defaults to `auto`; a registry recipe
can set `policy.tempoPolicy`. A source-bound explicit `musicalGrid` review takes
precedence, preserving Kouhaku's accepted grid. `--tempo-cache DIR` stores reusable
analysis and decisions; registry builds use `<corpus root>/tempo-cache/`.
Cache identities bind source manifest, analysis/decision/quantizer code and the
analysis requirements. Cached decisions are checked again against every lane.
Changing just the decision policy can reuse the same hash-bound raw analysis.

Prepared output adds `automatic-tempo/analysis.json` and `grid.json`, plus the
existing `musical-grid-review.json`. When evidence is insufficient or a chosen
grid cannot meet the timing budget, `automatic-tempo/insufficient.json` explains
why the work retains its exact source-frame clock. Source defects still fail
conversion. The prepared manifest records `tempoPolicy`,
`automaticTempoApplied` and every applied note/hold/scroll endpoint shift.

## What fitting changes

The generic policy searches 60–360 BPM and compares chart lattices with three
audio pulse estimates. It refines pulse periods without treating missed beats as
slower music, trims inconsistent residuals and considers stable chart portions.
Ranking balances chart coherence and audio agreement, with a small continuous
preference around 150 BPM and a triplet-alias penalty. These are shared policy
constants, not rules for named works. Valid authored CW BPM ranges constrain
alias selection when evidence offers a candidate inside that range; explicit
reviewed display metadata still wins. Unknown `----` is absent metadata.

Quarter-beat phase is estimated from the audio pulse, not recovered from an
editor project. Half/double/triplet ambiguities and changing tempos remain real
limits. The report preserves alternatives, fit scope and residual statistics;
its scores are not correctness probabilities. Jam with Pancake's authored
168–325 BPM range is preserved for display, while its export uses one fitted
constant grid. It does not claim a recovered multi-segment tempo map.

Rhythmic snapping applies only within **9 ms** of source time. Other endpoints
use the nearest fine tick at 48 ticks/beat. If snapping would merge same-lane
notes, collapse a hold or make notes overlap, the whole lane uses fine positions.
Every endpoint and lane relationship is checked; failing the budget is never
permission to remove a note or change a hold. Source reference frames remain
unchanged. Estimated timing changes both menu BPM and beat-based note coloring;
merely adding `#DISPLAYBPM` would not change coloring.

Inverse `#SCROLLS` compensation keeps the measured DanOni default movement while
multiplying the user's X speed. For standard CW captures, the cached engine
must prove 50 px arrows and `2 × selected speed × baseSpeed` px/frame at 60 FPS.
Thus the base is `2.4 × initialSpeed × baseSpeed` arrow heights/second. Its
SSC baseline ratio is `60 × baseArrowHeightsPerSecond / fittedBpm`, with author
speed events multiplying that ratio. Source script hashes bind both raw resource
bytes and the browser-decoded text, accounting for BOM/CRLF differences. Without
native-motion evidence, the preparer preserves the prior 75 BPM SSC movement and
labels that limit; it does not invent a native measurement. Existing C speed
semantics are unchanged.

SSC does not express DanOni per-note boosts, custom color/lyric scripts or the
source `endFrame` finish behavior. Omissions remain in `unrepresentedEffects` and
original references. In particular, Straight Soul's 5-key chart has nine per-note
boost events, which its earlier native CW package could carry. This bridge
preserves its base/global speed but omits those local boosts. Source ratings
remain null where no numeric rating exists; this work does not invent stars from
names or initial scroll speed.

## Current corpus validation

All **19 songs / 23 charts / 12,896 taps / 332 holds** now have SSC/Ogg exports and
reviewed banners. The five formerly native CW songs use the bridge. Source CW
normalization is independently compared with the game's native DanOni parser,
then SSC timing is compared with the normalized reference. Global author scroll
ratios and X1/X2/X3 baseline movement are checked with the actual game engine.
Kouhaku's accepted note times, quantization buckets and audio bytes are preserved.
All other works fit automatically, with maximum endpoint shifts below 9 ms.
The canonical packs remain under `~/Music/DanOni/exports/`; originals, tempo
cache and previous generations stay local. Reimporting formerly native CW songs
creates SSC-based IDs, so an existing user library may retain its old copies.
