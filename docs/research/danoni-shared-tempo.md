# Shared music, independent chart timing

Updated: 2026-10-11. The automatic musical-grid policy now fits charts sharing
music without assuming their note phases are identical. This addresses the weak
coloring evidence found in the [gen3 admission](danoni-gen3-admission.md).
Original frame references, canonical music, ratings, previews and native movement
measurements remain unchanged. No work names select an algorithm or threshold.

## Why a single pooled phase failed

The earlier estimator combined every chart's distinct head times into one
circular phase fit. Authors can choose different offsets and slightly different
tempo approximations for the same recording. Their phase vectors can cancel,
or drift apart over the song, even when individual charts have a clear lattice.
That caused fallback to a coarsely estimated audio pulse and many fine ticks.
Fine fallback preserved timing but did not establish useful musical coloring.

Cached diagnostics confirm this distinction. Cynthia's pooled best coherence was
0.396; individual charts support a tempo near 187, with different phases. Its
レク chart remains incoherent at that tempo. タイヨウノハナ's authors fit roughly
184.955–185.249 BPM, and G Free's roughly 189.773–189.895. These observations are
estimates of authored lattices, not proof that the recording changes tempo.
The [original pages](http://ikazch.ikaduchi.com/cynthia/cynthia.html),
[タイヨウノハナ](http://autumnleaves.yukishigure.com/DOXF04/flower/SUN.html) and
[G Free](http://autumnleaves.yukishigure.com/DOXF04/G/Free.html) identify the
separate author selections; the saved native references establish their times.

## Generic policy

`shared_tempo.py` runs only for a music group with multiple charts. The existing
single-chart estimator and explicit source-bound musical-grid reviews retain
precedence. Cached raw audio analysis is reused; no scientific dependency was
added and no source resource is fetched again.

- Each chart with at least eight finite distinct tap/hold-head times contributes
  one vote. Search 60–360 BPM, subdivisions 1/2/3/4/6/8, with minimum lattice
  spacing of three 60 Hz source frames. Temporary spectral matrices are chunked.
- Candidate ranking keeps the existing audio votes, coherence weighting and
  half/double/triplet ambiguity policy. Chart phases are fitted independently.
  A local tempo refinement is restricted to ±0.15%, the existing stable-window
  consensus tolerance. It is also bounded against the final shared tempo.
- A strict majority of eligible charts must have coherence at least 0.85 and
  95th-percentile lattice residual at most 12 ms, with the shared tempo within
  the existing 2% audio-agreement window. The shared tempo is the median of the
  supporting chart fits. A missing majority retains the previous conservative
  grid; a dense chart cannot substitute for a majority.
- Phase support is checked separately for each chart. Its fraction of heads
  within half a native frame must exceed both 35% and a uniform-phase safeguard
  (`chance + 3 * sqrt(chance * (1-chance) / distinctHeads)`, where
  `chance = min(1, (1/60) / subdivisionPeriodSeconds)`). This is a rejection
  safeguard, not a probability of musical correctness. Unsupported phases use
  the shared tempo with the existing audio phase and fine-note fallback.
- A supported chart uses its own phase and bounded tempo refinement. The
  quarter-beat alias is selected nearest the audio pulse. This does not recover
  a known editor downbeat; musical alias ambiguity remains explicit.

The quantizer is unchanged: every tap, hold head/tail and scroll event is checked
against the source. Rhythmic snapping is limited to 9 ms; collision-safe fine
fallback preserves same-lane relationships. There is no deleted note, shortened
hold, fitted audio offset, resampled music or estimated variable-tempo map.

## SSC and reference contract

SSC v0.83 already supports [split chart timing](formats-sm-ssc-dwi.md): each
`#NOTEDATA` section writes its own complete `#OFFSET`, `#BPMS` and `#SCROLLS`.
The previous preparer unnecessarily required the entire reviewed grid JSON to
be identical for charts sharing music. It now requires exact chart-ID coverage
and permits chart-specific phases, subdivisions and BPMs. The song header uses
the shared consensus where available; each chart's timing overrides it.

Native scroll compensation is calculated with that chart's own BPM:
`sscBaseRatio = 60 * nativeArrowHeightsPerSecond / chartBpm`. Author speed events
multiply it, and the player's Xmod multiplies the result. The player's Cmod is
unchanged. Separate offsets and pre-roll padding preserve source times; the
existing four-beat padding preserves the color cycle.

The existing `ddi-reviewed-musical-grid/experimental-1` review remains keyed by
chart ID. `bpm`, `beatZeroSeconds` and `subdivisionsPerBeat` can differ between
charts with the same music. BPM units are quarter beats/minute; phase is seconds
on the unchanged canonical music clock. New per-chart fields are:

| Field | Meaning |
| --- | --- |
| `estimationMethod` | `independent-chart-phase`, or `shared-tempo-with-fine-note-fallback`; earlier single-chart methods remain valid |
| `chartFit` | That chart's independently fitted BPM, subdivision phase/period, coherence, residuals and half-frame support fraction |
| `phaseEvidence` | Supported flag, distinct-head count, matched fraction, uniform-phase fraction and required fraction; null for insufficient chart data |
| `sharedTempo` | Consensus BPM, eligible-chart count, supporting chart IDs, status and refinement fraction |
| `audioPulse` | Existing audio BPM/phase/refinement evidence used for agreement and the quarter-beat alias |

`automatic-tempo/shared-charts.json` (`ddi-shared-tempo-analysis/experimental-1`) records the complete group candidates and
fits, selected candidate, consensus/support list and audio pulse. It binds the
normalized source manifest and policy code hashes. Decision cache identities
include `shared_tempo.py`; unchanged `analysis.json` can be reused when only the
decision policy changes. Cache hits still recheck every source lane.

## Local comparison

The three motivating works keep every source endpoint and chart identity:

| Work | Rhythmic endpoints before → after | Total endpoints | Shared BPM |
| --- | --- | --- | --- |
| Cynthia | 450 → 3,428 (12% → 91%) | 3,786 | 186.996 |
| タイヨウノハナ | 735 → 2,986 (24% → 97%) | 3,067 | 185.052 |
| G Free | 366 → 1,490 (23% → 95%) | 1,562 | 189.834 |

Endpoints include hold tails; this is a lattice-coverage measure, not a
correctness score. Cynthia/レク still uses the conservative phase fallback;
its source reference does not establish the same beat lattice as its siblings.
The largest shift in this comparison is 8.9889 ms. Published difficulty meters
are unaffected, including that chart's catalogue level 6.

Diagnostics and prior generations remain outside Git:

```text
~/Music/DanOni/reviews/shared-tempo-20261011/
  <work>/candidate-v1/             first fits and source-bound comparisons
  *-probe.json, *-windows.json     independent phase/window investigation
  browser/                        final imports and unchanged-media receipts
~/Music/DanOni/tempo-cache/<key>/
  analysis.json, shared-charts.json, grid.json
```

Rebuild with the existing [export command](danoni-exports.md), in the cached
analysis environment for new decisions. No native Ruffle recapture is needed:
source schedules, movement and audio-origin evidence remain immutable.

## Validation and active generation

The final generation keeps 44 songs / 72 charts, 52,627 taps and 1,906 holds.
Only eight SSC files change: RE:RAVE, Straight Soul, ふしぎなひかり, 灼冬赫夜,
END of AIR, Cynthia, タイヨウノハナ and G Free. The other 36 SSCs, all 88 music/
banner files, every preview and source rating, and the entire Kouhaku ZIP remain
byte-identical or unchanged in metadata. Source frame references are immutable.

All 72 charts pass the actual game-parser/engine verifier, including held
endpoints, collision relationships, the 9 ms budget and native movement at
x1/x2/x3. Fresh-profile installed Chrome imports both packs with zero errors or
warnings and decodes all 44 banners. Separate fresh-session autoplay checks of
Cynthia/mi (5-key) and Cynthia/Air (7-key), through 20 seconds of music time,
produce 15 and 70 top-tier tap judgments respectively, no lower judgments, no
failed session and no let-go. This is an opening-section check, not a full-song
playability claim. Screenshots and source-bound debug receipts stay local.

The 86 Python tests and `just check` / REUSE lint pass. No dependency, game crate
or native worker changes. The final packs are promoted with the existing archive
and rollback journal. Updated SSC content produces new import IDs; an existing
user library can retain its older copies when this generation is reimported.
