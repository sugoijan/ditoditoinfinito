# Saigetsu and ESS: native extraction and port comparisons

Updated 2026-10-11. This second v2 admission adds **two five-key charts**:
[Saigetsu](http://autumnleaves.yukishigure.com/Saigetsu/saigetsu.html) and the
レク/Normal chart of
[Earthquake Super Shock](https://mfv2.sakura.ne.jp/do/__Flash/danoniwinterfes/No200/DO.ESS.html).
The accepted local packs now contain **33 songs / 51 charts, 40,977 taps and
1,230 holds**. The previous 31 songs' 93 playable files and the Kouhaku ZIP
remain byte-identical. All source files, music, artwork, captures and generated
packs remain local.

| Work | Native reference | Published meter | Presentation |
| --- | --- | --- | --- |
| Saigetsu | 679 taps, one hold; source version 4.00 drawn on its title | 11 / Challenge | Default fitted BPM rounds to 140; pickup preview starts 13.403 s |
| Earthquake Super Shock / レク[Normal] | 1,277 taps, 53 holds; MFV2 ParaFla! source 5.051 | 25 / Challenge | Default fitted BPM rounds to 175; pickup preview starts 12.920 s |

Ratings come from the exact linked rows in
[レク's catalogue](https://cw7.sakura.ne.jp/lst/ds0065/), not selector counters.
Both charts use the existing tier bounds, Ogg encoder and plain-title banner
renderer. There are now 34 rated single-song charts out of 45. The six Kouhaku
charts retain their earlier ratings. These are defaults and estimates; the
preparer has no source-name or pack-specific BPM/preview thresholds.

## Static generation hints needed correction

The survey's gen4 classification was a structural hint, not an engine identity.
These movies use ParaFla families and **external chart files**, not wholly
embedded chart data. Actual native requests revealed:

- Saigetsu requests `dos.txt` before displaying its title and again at Play.
- ESS first requests `no200_.txt` for its 55-chart menu, then
  `no200_33.txt` when the レク chart is selected. Native `diffin` is the number
  33, while the selected label's zero-based menu index is 32. The root
  `difficulty` remains 55; it is not a star count or this chart's index.

The required files were acquired through the existing robots-aware, paced cache
only after their exact runtime URLs were observed. Original acquisition
manifests stay intact; supplemental receipts and body hashes live in
`native-adapters-v3/<id>/inputs/provenance-complete.json` and, for ESS,
`provenance-final.json`. No absent required file was replaced with an empty one.
ESS's other 54 native chart choices are not claimed as normalized exports yet.

## The `parafla-counter` contract

This family extends the independent `mkArrow`/`mkFArrow` plus `judgeY` proof.
Every **played** tap and hold head must match `spawn + travel`; raw tails and
all original snapshots remain evidence. It requires `frameNum`, a seven-slot
native key-code bank, and `setSpd == 2 * difSpd > 0`.

Divided sidecars load their selected data into unsuffixed fields. The observed
`DOS_DIVIDE: true` selects those fields; a complete compiled schedule must still
agree. Numeric suffixes accept only finite integers 0–999 and use their decimal
string form. Other suffix values fail rather than being guessed.

ESS illustrates a source-state carryover problem: the menu's initial seven-key
chart leaves 312 taps and four holds in diagonal slots when its five-key chart
loads. Those slots have key code 0 and no played receptor. The adapter allows
only explicitly disabled slots outside the selected built-in layout; their full
schedules are retained in `projection.inactiveSchedules` and the normalized
`inactive-native-lane-schedules` effect. An extra slot with an active key code
still fails. Thus the five-key projection does not donate another creator's
retained diagonal notes to the レク chart. This is not a claim to reproduce the
original engine's carryover behavior or its gauge consequences.

The native `arrowSpd` bank contains **pixel increments**, not bare multipliers.
The first pair initializes `[0, setSpd]`; it supplies the baseline, not another
author speed change. Later increments are divided by that baseline to form
normalized multipliers. Their engine-computed frames are retained exactly. ESS
has 16 such changes; its loaded `speed_change` still contains an older chart's
values and must not override the actual parsed `arrowSpd`. `parsedPixelSpeeds`
retains the complete bank. Parsed `boostDac` remains evidence and is not applied
as an SSC per-note boost.

Both sources produce two exact missing-attachment diagnostics after conversion
for unused diagonal art. Explicit recipes use the existing narrow attachment
review, now requiring the selected mode's five receptors. All five are present;
unknown diagnostics, failures before compilation, incomplete graphs and VM
halts still fail. Saigetsu's later bitmap background needs a real renderer;
the movement proof uses the offscreen Metal renderer rather than suppressing
`BitmapData.draw` failures from the null renderer.

ESS's converter exceeds the default 500 ms AVM budget. Its recipe uses
`avmExecutionMs: 10000` with the unchanged 20 s CPU, 30 s wall and 2 GiB parent
limits. Neither movie's variables, scripts or judgment routines are modified.
Menu actions are ordinary clicks/keys, with source stage bounds recorded.

The verified audio stream starts are worker iteration 561 / logical frame 202
for Saigetsu, and iteration 392 / logical frame 202 for ESS. These are separately
observed clocks, not inferred from `blankFrame` or a copied port offset. Original
MP3 data and native PCM remain intact; the existing waveform alignment checks
produce canonical WAV for the shared Ogg preparation. Both movement proofs
observe 5 source pixels per tick and a stable 47.55-pixel reference width:
**6.309148265 arrow widths/second**. Arrival differs from the raw judgment frame
by one native tick, recorded explicitly. SSC base compensation preserves this
measured rate, with player x1/x2/x3 applied on top.

## What the HTML5 comparisons actually establish

MFV2's exact browser-decoded CW v50.0.0 engine hash is now reviewed:
`54bf2c5c7833d2fcb9971cc55178aa15b59d41d39e23fcc558b4ac12008d16dd`.
The four scopes, six-argument converter, full-return arrays, frame adjustment
and event layouts were inspected. The custom helper's separately reviewed
browser-decoded hash is
`2d0df288cd600c72852effe9c359ff02a9efb8e5e06d06943dc2bba298ab859a`.

That helper loads chart and music/artist CSV data using sole timestamp queries
**before engine startup**. Its exact hash activates declarative page-relative
URL templates via `kind: "loader"` and
`timestampResources: {pageQueryParam: "id", templates: ["{id}/dos.js",
"{id}/dos.txt", "../music/_artistDataList.csv", ...]}` in the reviewed registry. The observed frame URL supplies the work's `id` path using the
helper's documented decoding, without evaluating source code in Node. Aliases
are accepted only for those exact URLs and a sole 13-digit timestamp; meaningful
queries remain significant. Before-script debugger pauses must not evaluate
JavaScript or request DOM protocol data: those calls can deadlock before the
script resumes. The implementation uses Playwright's already observed frame URL;
a real Chrome test checks that execution resumes. Existing engine and loader
policies remain separate, and robots refusals are not bypassed.

[Saigetsu's CW port](https://mfv2.sakura.ne.jp/do/_rekuAL/?id=saigetsu)
produced genuine input, full-return, prepared and main-entry snapshots. Its
converter reference replays identically offline. Full work verification remains
false: captured page/helper diagnostics and an uncaptured shadow image remain
separate limitations. It is not admitted as a complete playable CW dump.

The port has **682 taps and one hold**, compared with **679 taps and one hold**
in the SWF. Even after removing the CW engine's integer adjustment, note frames
and the hold endpoints differ. Therefore it is **not an equality oracle for the
original chart**. The differences are retained; neither source is edited or
shifted to force agreement. The exact copied sidecar/input evidence and full
native schedule proof distinguish a chart difference from a converter-stage
misreading.

[ESS's CW port](https://mfv2.sakura.ne.jp/do/main/?id=ESS) acquired its 55 startup
sidecars through the cache, but the captured combination did not reach a usable
converter return or ready chart-selection state. It reports a duplicate
`createDivLabel` declaration and retains unavailable dependencies. Its reference
comparison remains pending. No source repair, missing-library substitution or
pretend successful verification was used. The native export stands on its own
schedule, movement, audio and game-parser proofs.

## Generic comparison utility and local layout

```sh
python3 tools/danoni-dump/ruffle/compare_ports.py \
  "$HOME/Music/DanOni/normalized/reku-103-saigetsu-saigetsu/manifest.json" \
  "$HOME/Music/DanOni/corpus-v2-pairs/mfv2-saigetsu-retry1/manifest.json" \
  --native-chart reku-normal --cw-chart 0 \
  --out "$HOME/Music/DanOni/reviews/corpus-v2-pairs-20261011/saigetsu-comparison.json"
```

The output schema is `ddi-danoni-port-comparison/1`. It binds both manifest
hashes and the CW reference hash; records chart IDs, mode, CW timing and native
audio origin; and reports per-lane tap/hold counts and exact frame-multiset
differences. Units are integer 60 Hz source frames before CW integer adjustment.
Only playback rate 1 is invertible here; rate-scaled data fail. It performs no
fitted shift or audio alignment. `cwOfflineReferenceMatched`, `cwOfflineWorkPassed`
and `cwCaptureStatus` are distinct. `equalRawFrames` is only a note/hold equality
result, not proof of equal music or event semantics. Comparison JSON remains
outside Git along with the source data.

```text
~/Music/DanOni/
  native-adapters-v3/<id>/         input/config/receipt files, failed probes,
                                  audio-proof-v1, motion-proof-v1, recipe-final.json
  normalized/<id>/               accepted normalization and original evidence
  corpus-v2-pairs/                partial CW captures and verification reports
  reviews/corpus-v2-pairs-20261011/
    acquisition/, ess-chart-acquisition/
    saigetsu-comparison.json
    <id>/{native-scroll-review.json,ratings/,plain-title-v1/}
    native-prototypes/, presentation-prototypes/  earlier versions preserved
    registration-candidate.json, registry-before.json, browser/
  exports/                       accepted packs; older generations in history
```

The registry remains 136 entries, now **29 required / 107 pending**. Fourteen
of the original 113 v2 works are admitted; 99 remain pending. Of the planned
51 gen1/gen3/gen4 candidates, 49 remain. The next useful batch is gen3's
scheduled-counter relatives, followed by gen1 and the remaining gen4 variants.
The reviewed MFV2 engine also unlocks fresh investigation of its earlier CW
probe, though an engine hash review alone does not complete a dump.

Validation includes all 51 charts through the independent game-parser verifier,
9 ms endpoint limits, native movement compensation at x1/x2/x3, source meters,
previews and banners; fresh-profile Chrome imports; 77 Python tests, 50 Node
tests (including real Chrome before-engine policy coverage), and `just check` /
REUSE lint. The pinned native worker is unchanged in this batch.
