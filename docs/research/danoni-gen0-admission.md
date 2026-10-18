# Gen0 native corpus admission

Implemented 2026-10-12. The six gen0 acquisitions now have native exports:
**seven five-key charts, 4,896 taps, 293 holds and 100 speed events**.
These are SWF6/AVM1 works from 2006–2007. Original movies, sidecars, music,
screenshots and generated packs remain local; no third-party fixture is added.

| Work | Native selection | Published meter | Taps / holds | Music origin, native frames |
| --- | --- | --- | --- | --- |
| [sai](https://sound.jp/safaido/) | Hard | 5 | 672 / 68 | 159 |
| [HARASSIN' BOSS](http://varenntainn.oboroduki.com/harasin.html) | Death | 6 | 645 / 8 | 160 |
| Same work | Onigiriress | 5 | 562 / 8 | 160 |
| [U.N.オーエンは彼女なのか？](https://sound.jp/safaido/ungassaku.html) | Hard | 14 | 760 / 43 | 158 |
| [Yet Another 'Drizzly Rain'](http://varenntainn.oboroduki.com/DOVSF/Yet.html) | Special | 15 | 1,066 / 23 | 166 |
| [No Life Queen](http://varenntainn.oboroduki.com/DOVSF/Queen.html) | Hard | 17 | 935 / 137 | 166 |
| [終憶鎮魂曲 ～ Her Selves](http://varenntainn.oboroduki.com/selves/selves.html) | Normal | 6 | 256 / 6 | 10 |

## Native data and clock

The new `dac-5` and `dac-fixed-5` adapters recognize a unique child controller containing
`le_dac`, `do_dac`, `up_dac`, `ri_dac` and `oni_dac`. It requires repeated
unconsumed banks, zero tap/hold cursors and zero controller event cursors.
Each bank must agree with the selected loaded `left/down/up/right/space_data`
fields; the adapter accepts either historical suffix position around `_data`.
Undefined arrays mean empty lanes, while only a terminal empty split token is
removed. Interior empty tokens, invalid frame values, malformed holds,
changed selected data and active diagonal lanes fail.

Holds are the root's `fle/fdo/fup/fri/foni_dac` pairs. The five-key ordering
is left, down, up, right, onigiri. Seven-slot `keyCtrl` arrays are projected
through indices `[0,2,4,6,3]`. Owen instead has fixed five-key boolean controls
and no runtime key-code array: its reference leaves `nativeKeyCodes` null and
preserves the original source. This does not infer custom key definitions.

The root movie frame advances once per worker iteration during play.
Seconds are `(nativeFrame - independentlyObservedMusicOrigin) / 60`, with no
fitted grid or renderer-phase adjustment in the exact reference. Every selector
has six linear first-arrow observations and a captured exact stream-start
iteration. All seven measured arrivals agree exactly with their raw note frame.
Her Selves registers its receptor artwork at y=80, but its controller initializes
arrow travel from `70 + 150 * set_spi` and judges against movie frames. Its
reviewed measurement therefore uses y=70; note timestamps are not shifted.

All chart requests use the previously acquired bodies: `dos6.txt`, `dos12.txt`,
`dos.txt`, DOVSF's `dos1.txt`/`dos2.txt` and `selves/dos15.txt`. Their recorded
final URLs, including sound.jp's HTTPS redirects, supply the offline navigator.
No new HTTP request or invented scheme alias is needed for this batch.

## Per-arrow speed is different from global scroll

Static source inspection and native arrow-local `spd` values distinguish two
contracts. Five configurable-key works use `dac-5`.
`set_spi = 2 * menuSpeed` supplies the base pixels per frame.
A source speed pair `(F,M)` applies to the controller at **F−150** and sets:

```
newArrowPixelsPerFrame = initialPixelSpeed + 2 * (M - 1)
```

Each cloned arrow stores that speed at birth and keeps it through its flight.
Changing the controller affects later arrows. The 150-frame lookahead and a
correspondingly adjusted spawn position keep arrivals at the authored frames.
For HARASSIN's Death chart, baseline 4 and source value 0.5 give 3 px/frame;
Onigiriress's value 0.75 gives 3.5. These are not baseline times 0.5/0.75.
Stored arrow speed, visible movement and authored arrival are checked together.

Owen's fixed controls identify a separate `dac-fixed-5` variant: directional
arrow speed is **initialPixelSpeed × M**, still stored at birth. Its onigiri
lane keeps an independent `oni_spi` baseline. The adapter requires that baseline
to match the initial arrow speed and rejects authored `onisp_data`/`onisp_change`
until separate event support exists. The first arrow alone would not distinguish
these laws because Owen's first authored speed event occurs later in the song;
the final static review of all six controllers caught the difference before
promotion. The earlier unpromoted generation remains local.

The existing reference and normalized-work schema identifiers remain unchanged.
`projection` adds the following fields; frames are source movie frames,
speeds are source-stage pixels per native frame, and multipliers are unitless:

- `motionSpeedPolicy: "per-arrow-at-spawn"`.
- `speedValuePolicy`: `"additive-two-pixels"` for `dac-5`, or
  `"multiplicative"` for `dac-fixed-5`.
- `independentOnigiriPixelSpeed`: present only for the fixed variant, in
  source-stage pixels per native frame.
- `spawnLookaheadFrames: 150`.
- `nativeSpeedChanges`: ordered objects with `nativeFrame` (original F),
  `multiplier` (original M), `applicationFrame` (F−150) and `pixelSpeed`.
- `speed`: ordered `{nativeFrame: F, multiplier: pixelSpeed / initialPixelSpeed}`
  objects for the portable approximation. Their `seconds` in the normalized
  chart use the same independently measured audio origin as notes.

SSC uses these ratios at the authored note frames and retains the existing
inverse-BPM base compensation and player X-mod. Its global `#SCROLLS` cannot
exactly reproduce simultaneously visible arrows carrying different speeds,
or Owen's independently moving onigiri lane.
Every normalized chart records a `per-arrow-speed-approximation` effect and its
native changes. This is a declared visual approximation, not a timing change.
No Life Queen's eight `stop_dac` entries toggle source arrow motion; they remain
visual stop flags, not audio/timing stops. Arbitrary source presentation,
lyrics/background animations and non-default motion options are not recreated.

## Audio, ratings and presentation

All six continuous MP3 streams pass independent native-PCM versus FFmpeg
comparison, including both HARASSIN selectors separately. Minimum accepted
correlation is 0.97780; accepted windows have zero lag spread. Four recordings
use 22.05 kHz stereo, while sai and Her Selves use 44.1 kHz stereo. Canonical
WAV and quality-5 Ogg retain those source sample rates/channels.

Owen's stream begins with empty payload blocks. Its measured decoder origin is
−2,210 comparison samples at 44.1 kHz, so canonical 22.05 kHz PCM prepends
1,105 samples (about 50.11 ms). Seven independent windows agree on that shift.
The original MP3, every block's seek/sample metadata and the comparison receipt
remain intact. This is measured native audio alignment, not a BPM-derived
correction; original Flash/device latency is still unmeasured. All other
recordings have zero measured shift.

The [cached linked catalogue](https://cw7.sakura.ne.jp/lst/ds0065/) supplies seven
integer meters. Its spelling `Onigiriless` corresponds to the native selector
`Onigiriress`: an explicit local review binds the second selection, its suffix
2, the identical four directional banks and the empty onigiri bank. The blank
continuation-row key column inherits the song's five-key mode; an empty onigiri
lane does not turn the work into an unsupported four-key chart. This reviewed
spelling correspondence is not a general label-matching heuristic.

Meters use the same generic classifier as Kouhaku. Readable catalogue titles
and artists are cross-checked against native `gatit`; placeholder tuning strings
remain in the raw reference. Each song gets a source-title banner. Default
musical fitting and pickup preview policies remain unchanged, with no title,
pack or engine-family exceptions. sai's published 98–132 BPM range is source
evidence; a fitted main grid is not a recovered variable-tempo map.

## Local layout and reproduction

- Acquisitions: `~/Music/DanOni/single-song-corpus-v2/autumnleaves/<work>/`.
- Inputs, title/menu probes, bound native proofs and recipes:
  `~/Music/DanOni/native-adapters-v7/<work>/`.
- Normalizations: `~/Music/DanOni/normalized/<work>/`.
- Ratings, artwork, registry snapshots and validation receipts:
  `~/Music/DanOni/reviews/corpus-v2-gen0-20261012/`.

Each work's `recipe-final.json` binds the acquisition manifest and independent
captures, config receipts and PCM hashes. `proof-0-v1` supplies final evidence
except Owen, whose rendered probe hit the unchanged 30-second wall limit:
`proof-0-v2` captures the same native geometry without rendering and completes
in about six seconds. HARASSIN also has `proof-1-v1`. Failed attempts and hashed `static-speed-actions.txt` transcripts remain
local. Work code runs only through the restricted native worker.

Follow the [saved-evidence extraction commands](danoni-native-adapters.md#running-it)
and [offline rebuild procedure](danoni-exports.md). Source speed semantics are
proved per family; do not apply this gen0 formula to other DAC or later
compiled-schedule engines without separate evidence.

## Accepted generation and checks

The corrected generation is promoted to `~/Music/DanOni/exports/`:
**88 songs / 126 charts, 83,630 taps and 3,854 holds**. Single Songs contains
**83 songs / 120 charts**, with 93 published meters and 27 unrated charts.
Every song has a banner. All 246 previous playable files and the entire
Kouhaku ZIP are byte-identical. The registry has 84 required and 52 pending
entries; the original v2 inventory has 68 accepted, one partly accepted and
44 pending source entries. Earlier SUN/ESS selector scope limits remain.

The default grids display approximately 132 BPM (sai), 199 (HARASSIN),
169 (Owen), 175 (Drizzly Rain), 165 (No Life Queen) and 212 (Her Selves).
These are generic estimates, not publisher-tempo overrides or recovered tempo
maps. New chart endpoints differ by at most **8.98196 ms**; the unchanged
126-chart global maximum remains **8.99839 ms**.

Validation covers all 84 required work entries through the real game parser,
94 Python tests, and `just check` including REUSE. Fresh Chrome imports both
packs without errors/warnings, verifies counts and tiers, and decodes all 88
banners. Autoplay runs Owen for 90 seconds through later speed changes:
390 W1 judgments and four completed holds. HARASSIN's second chart has 48 W1
in 20 seconds; No Life Queen has 132 W1 and three holds. No lower judgments,
failures or let-go results occur. These are runtime checks of the exported
charts, not proof that SSC reproduces the original per-arrow presentation.

Final receipts live in `validation-summary.json`, `generation-comparison.json`
and `browser/` under the review folder. Generation index SHA-256:
`75820a61e8be67df36568c534ae038b083266927fd053e00cdadd0814e429696`.
Registry SHA-256:
`34e6778e0bfdfe3bc7e1f3dc2ef964f27bcddb80b63e53422b48999a35043bd3`.

Playable ZIP: `~/Music/DanOni/exports/single-songs/DanOni Single Songs.zip`
(about 216 MiB). The earlier unpromoted additive-only stage and pre-review
normalizations are preserved locally; they are not the accepted generation.
