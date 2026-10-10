# Corpus v2: verified first admission batch

Updated 2026-10-10. The [v2 source survey](danoni-corpus-v2.md) describes
113 acquired works, including probes that are not yet complete dumps. This
first extraction batch admits **12 works / 26 charts**: the three already
verified CW works and all nine sources statically classified as Higan-build.
The combined local exports contain **31 songs / 49 charts, 39,021 taps and
1,176 holds**. Source data, music, native captures, artwork and playable ZIPs
remain local; none are repository assets or redistributable fixtures.

## Accepted works

| Work | Played modes / charts | Published meters | Coverage |
| --- | --- | --- | --- |
| [Brilliant Blue](https://apoi108.sakura.ne.jp/danoni/nfes20_beginners/B_Blue.html) | 5 / 1 | 11 | CW v50.2.0, freezes, source-bound page rating |
| [Zombie](https://vorhandensein.sakura.ne.jp/danoni/428_zombie/) | 5 / 1 | 32 | Recent CW work, declared 150 BPM, collaborative chart credit |
| [はい、ニーズ](https://vorhandensein.sakura.ne.jp/danoni/433_needs/) | 5, 7, 7i, 9A / 12 | 17, unknown, 41; 14, unknown, unknown; 12, unknown, 40; three unknown | Multiple authors and difficulty scales; 11/23-key charts retained in source evidence but excluded from playable output; lyrics and variable music tempo |
| [灼冬赫夜](http://autumnleaves.yukishigure.com/noname08/noname.html) | 5 / 2 | 13, 13 | Real selector choices Remake/Hard; independently checked shared audio |
| [トルコ行進曲](http://autumnleaves.yukishigure.com/7keyfestival/march.html) | 7 / 1 | 10 | Constant-travel native schedule, freezes, missing optional colour file |
| [蠢蠢秋月](http://autumnleaves.yukishigure.com/7keyfestival/gkbr/riguru.html) | 7 / 1 | 12 | Source catalogue 154 BPM, freezes, missing optional colour file |
| [最期に君がいた](http://autumnleaves.yukishigure.com/DOSMF08/Last/saigo.html) | 5 / 1 | 4 | 100 BPM fit, slow native baseline, missing optional colour file |
| [60's wind - stars](http://autumnleaves.yukishigure.com/DOSMF08/60s-wind/60wind.html) | 5 / 1 | 13 | Freezes and fitted musical grid |
| [ENEMY WITHIN](http://autumnleaves.yukishigure.com/DOSMF08/REGRET/ENEMY.html) | 5 / 1 | 14 | Dense taps, missing optional colour file |
| [恋色マスタースパーク ＜SlashSpeed＞](http://autumnleaves.yukishigure.com/DOSMF08/DarisCradle_SlashSpeed/Slash.html) | 5 / 1 | 13 | Early long freezes, speed changes before audio zero, native author multiplier already 2× during observation |
| [END of AIR](http://autumnleaves.yukishigure.com/DOSMF08/END_of_AIR/END.html) | 5 / 3 | 12, unknown, unknown | Three genuine selector choices: レク / KKKKK / MFV2; independent shared-audio checks; MFV2 movement observed after a 2× author change |
| [弾・幕・決・壊 (レク)](http://autumnleaves.yukishigure.com/7keyfestival/syoujosyuu/K.html) | 7 / 1 | 11 | 109 freezes, faster native baseline, missing optional colour file |

Native ratings come from exact cached rows of
[レク's work catalogue](https://cw7.sakura.ne.jp/lst/ds0065/), matched to the
work link, key mode and chart creator. Selector counters are not ratings.
SlashSpeed's catalogue credits `izanaki&レク`; its runtime label has the same
creators separated by a space. END of AIR's other creators have no verified
rating in that row and remain unrated. NEEDS' page comment calls one 7i chart
`SuperHard`, while the captured engine labels it `Hard`; that association is
left unresolved. The [shared classifier](danoni-source-ratings.md) is unchanged.
**32 of 43 single-song charts** now have verified numeric meters. The six
Kouhaku charts retain their existing ratings.

All 31 songs have reviewed title banners. New banners use the existing plain
source-title renderer; they are not claimed to be original author artwork.
Native engine colour scripts and CW lyrics/custom visual events remain evidence,
with omissions listed in the prepared reports.

## New native contract and proof

The static Higan resemblance was useful triage, but did not identify a working
adapter. These nine movies display source version **2.17** and use constant
travel subtraction, rather than the earlier adapter's variable `judgeY` table.
The new `scheduled-constant` family accepts 5/7-key charts and requires:

- A pre-consumption `frame_num == 0` capture of split `timeline`/`ftimeline`,
  followed by the compiled `mkArrow`/`mkFArrow` snapshot.
- Every tap and hold head agrees with `raw head - set_cnt`; tails remain raw.
  Mutated arrays and every native freeze-duration cache entry must agree too.
- The initial unit conversion is `set_spi == 2 * dif_spi`. Native speed events
  retain their engine-computed logical frames and chronological order.
- A captured stream start establishes the audio-relative clock, and at least
  three linear movement samples establish actual arrow arrival and geometry.
  Arrival must agree within 1.5 source ticks; audio alignment is independently
  checked against the native PCM over energetic windows.

These are observed 60 Hz defaults, not a promise to recover arbitrary offsets,
source modifications or custom motion. Five-key native lane indices are
`[0,2,4,6,3]`; seven-key indices are `0..6`. Normalized timing and reference
schemas remain those in [the adapter handover](danoni-native-adapters.md).

An author speed multiplier may already be active when motion can first be
measured. The adapter proves `observed pixels/tick = initial pixels/tick ×
active author multiplier`; it records the multiplier separately and uses the
initial speed for SSC baseline compensation. This prevents double application
in SlashSpeed and END of AIR/MFV2. Player x1/x2/x3 remain multiplicative.

SlashSpeed fails before its first ordinary tap unless the opening freezes are
held. Its recipe sends real native key events with 90-tick holds; it neither
writes runtime properties nor disables judgment. The complete graph needs the
new bounded `inspectionScale: 4`. Default budgets remain unchanged; the maximum
permits 4,096 objects, 80,000 properties, 16,384 UTF-16 units per string and
1,048,576 total UTF-16 units. CPU/wall/memory and request limits are unchanged.

Five works request `iro.txt` that the acquisition cache records as HTTP 404.
`optionalColorReview` binds each actual response body, URL, retrieval date and
movie. It permits only that exact denied request and matching asynchronous
error. Original audits and diagnostics are retained; the worker does not invent
an empty successful colour response. Missing chart/music files and unknown
errors still fail. Existing `iro_dac` arrays stay in `nativePresentation` and
unmapped-effect evidence; they are not silently treated as recovered SSC colour
instructions.

## Presentation and musical timing

Default BPM fitting, pickup previews, Ogg conversion and native scroll policies
apply to the new works. NEEDS' automatic fit selected a half-tempo alias near
126.4 BPM. Its source page explicitly publishes **200–252.5 BPM** and identifies
**252.5 as the main BPM**. A hash-bound musical-grid review selects that factual
main tempo through the existing generic review interface. Phase is estimated
from the chart lattice and audio pulse; the report includes the weak whole-song
fit statistics. This is not recovery of the full variable-tempo map. Fine-tick
fallback preserves outlier timing, with every head/tail bounded below 9 ms.
The menu rounds 252.5 to 253 BPM; SSC timing retains 252.5 exactly. There
are no song IDs or BPM overrides embedded in the conversion code.

Rated charts normally omit `#CHARTNAME`. When two charts sharing a song and key
mode have the same known meter, the preparer preserves their original names
and sets `preservedDuplicateLabel: true`. Thus 灼冬赫夜's Remake/Hard both remain
Challenge 13 while preserving distinct identities. This changes no accepted
v1 simfile: none has that equal-meter ambiguity.

## Local evidence and reproduction

```text
~/Music/DanOni/
  single-song-corpus-v2/           original immutable acquisition corpus
  .cache-v2/                      separate source cache; not merged or refetched
  native-adapters-v2/<id>/
    inputs/                      original blobs/configs and optional-404 receipts
    recipe-final.json
    <capture>/{snapshot.json,run.json,audio.pcm,...}
  normalized/<id>/               hash-bound CW or native normalization
  reviews/corpus-v2-admission-20261010/
    admission-inventory.json     all 113 v2 sources, accepted/pending reasons
    <id>/{native-scroll-review.json,ratings/,plain-title-v1/,...}
    vh-433-needs/musical-grid/    published-tempo review and cached source body
    browser/                     fresh-profile import report and screenshots
  export-registry.json
  exports/                       accepted packs and validation/evidence reports
  exports-history/               earlier generations and promotion journals
```

Native outputs can be reproduced from each saved `recipe-final.json`, the
pinned restricted worker and a **fresh external output directory**. Capture
configs preserve selector actions, keys, budgets and observation iterations.
The [worker instructions](../../tools/danoni-dump/ruffle/README.md) describe
building/running it; [the export handover](danoni-exports.md) gives the offline
build and promote commands. The registry binds the normalized manifest and
all review/evidence hashes. Building the current packs requires no browser,
source script execution or source network access. Chrome is used separately
for import validation and plain-title rendering.

The registry now has **136 entries: 27 required and 109 pending**. Of these,
v2 contributes 12 accepted and 101 pending works; the other eight pending are
older exploratory entries. Pending sources do not produce playable ZIP content.
Do not count the 12-song AOM or five-song collection as extracted songs yet.

## Remaining batches

| Sources | Count | Work still required |
| --- | --- | --- |
| CW probes | 24 | Review exact engine hashes; recapture to fresh directories. v18.9.3/v22.5.2 function declarations need an older adapter variant. Syntax capture-point recognition is insufficient. |
| Other single/collection SWFs | 18 | Native title/selection/clock/audio observation and family classification; runtime-built sidecar URLs need their actual observed resource bindings. |
| Other レク generations | 57 | gen0: 6, gen1: 25, gen3: 11, gen4: 15. Prove each family against runtime schedules; use the live Saigetsu/ESS and reviewed HTML5 ports for independent comparisons. |
| Wayback works | 2 | Native runs, capture-date consistency, sidecar completeness and audio assembly. |

Next priority is the gen1/gen3/gen4 schedule families and the Saigetsu/ESS
cross-checks, alongside CW hash review. Gen0 needs its own contract. Any missing
required source or unproven chart remains pending rather than borrowing another
work's data or rating.

## Validation

The independent game-parser verifier checks all 49 charts, original frame
references, holds, speed events, musical-grid error budgets, native baseline
compensation at x1/x2/x3, preview bounds and published ratings. The 73 Python
unit tests and the restricted worker integration suite pass, including extended
graph limits and optional-colour rejection cases. Fresh-profile Chrome checks
the two imports, all 31 decoded banners, supported layouts and source meters.
The previous 19 songs' simfiles, audio and banners remain byte-identical; the
Kouhaku ZIP remains byte-identical. `just check`, REUSE lint and both standalone
Rust tool clippy checks pass.
