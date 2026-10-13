# Native single-song family adapters

Implemented 2026-10-10. These adapters consume hash-bound captures from the
restricted native Ruffle worker, without Playwright or network access. Work
scripts execute only inside that worker. Projection and audio processing outside
it read data; they never evaluate acquired ActionScript or JavaScript.
The earlier [Higan](danoni-native.md) and [Kouhaku](danoni-kouhaku.md) adapters
remain separately pinned. The new contracts recognize structural engine families,
not a list of approved movie hashes. A recipe still binds one exact acquired work,
selected difficulty and observed initialization/movement/audio evidence.

## Validated local set

Original sources and captures remain under `~/Music/DanOni/`. No music, movies,
sidecars, screenshots or third-party chart data are committed.

| Work | Family / mode | Taps / holds | Additional coverage |
| --- | --- | --- | --- |
| [FTHosyu](http://eucaly.net/~iso50/onigiri3.html) | child timeline / 5 | 190 / 0 | Embedded data; root movie-frame clock; 2 px/frame |
| [party 4u](http://eucaly.net/~iso50/onigiri5.html) | child timeline / 5 | 289 / 0 | Separate menu stream excluded; initially silent song audio; 2 px/frame |
| [felys](http://eucaly.net/~iso50/10/onigiri10.html) | child timeline / 5 | 381 / 0 | External dos; 3 px/frame; final right-lane source ordering defect retained |
| [RE:RAVE](https://web.archive.org/web/20190314171455/http://www.geocities.jp/naotopage2/game/re_rave/re_rave.htm) | child timeline / 5 | LVUP 556 / 0; Hard 922 / 0 | Two original menu selections; prefixed and unprefixed data; shared music independently checked |
| [Happy Rainbow](https://cw7.sakura.ne.jp/event/7fes06/works/31hpr/main.html) | DAC arrays / 7 | 281 / 38 | Seven lanes, freezes and nine speed events; 2 px/frame |
| [Decipher](https://cw7.sakura.ne.jp/event/7fes06/works/15decipher/main.html) | DAC arrays / 7 | 609 / 27 | Six speed events; two visual stop toggles; stream restart excluded from audio comparison |
| [Destined Marionette](https://autumnleaves.yukishigure.com/_9/D_M.html) | scheduled counter / 5 | 645 / 7 | Independent counter, variable travel, zero speed, boost; original Motion OFF setting |
| [FAKE800](https://mfv2.sakura.ne.jp/do/__Flash/mfv2_005/danoni/DanOni/FAKE800/DO.FAKE800.html) | scheduled counter / 7 | 541 / 1 | Computed external dos, independent counter, compiled sparse spawn tables; two reviewed attachment diagnostics |

This adds **eight works, nine charts, 4,414 taps and 73 holds**. Acquisition
manifests retain exact live/archive URLs, HTTP response provenance, sidecar hashes
and retrieval dates. Human-readable labels are source labels, not numerical
ratings: unknown star counts remain `null`, and SSC uses `Edit` / meter 0 with
`#CHARTNAME` distinguishing difficulties.

The original [Straight Soul Flash work](https://cw7.sakura.ne.jp/danoni/2009/straights/main.html)
is excluded from the active corpus at the maintainer's request. Its first-time Play path checks its published URL and submits a
name registration to a server before initializing gameplay. Cached pre-game state
and static bytecode inspection establish the dependency; the tool does not submit
registration, simulate a server response, alter the URL guard or claim an
initialized chart reference. Its already captured HTML5 port is a separate work.

## Family contracts

`child-timeline-5` reads a unique child bank containing `left/down/up/right/space_timeline`
and requires all five native consumption indices to be zero. Repeated captures
must agree. When loaded source fields are present, they must match every parsed
frame. `sourcePrefix` selects either ordinary fields or the observed `n` prefix;
ambiguous difficulty suffix placements are rejected. An optional recipe
`sourceSuffix` selects a reviewed suffix when the runtime selector is outside
the captured root. It requires all five selected loaded fields and exact
agreement with every native lane; conflicting root selectors, missing lanes,
selected unsupported holds/effects and source speed without a native bank fail.
`projection.selectedSourceSuffix` preserves that string. This does not change
the clock: child storage does not imply a child timing counter. Data order is retained.

`dac-7` reads a unique child bank containing `le/led/do/oni/up/rid/ri_dac`,
zero root consumption indices, root `f<clip>_dac` freeze endpoints,
`speed_dac` and child `stop_dac`. Loaded tap/hold/speed/stop fields are checked
against those arrays. A terminal empty split token is a sentinel, not frame 0;
interior empty tokens and invalid/backwards hold pairs fail. The observed stop
array toggles a visual flag. It does not pause music or define a StepMania timing
stop; its omission from playable SSC is explicit.

`scheduled-counter` handles 5 or 7 lanes with independent `frame_num` or
`frameNum`. Five-key native indices are `[0,2,4,6,3]`; seven-key indices are
`[0,1,2,3,4,5,6]`. It reads raw `timeline`/`ftimeline` or retained selected loaded
fields, then independently recovers **every tap and hold head** from sparse
`mkArrow`/`mkFArrow` plus `judgeY`: in these observed engines `judgeY[spawn]`
is travel in frames, so `spawn + travel = raw head`. It does not use root movie
frames once that timeline stops. Raw tails and the full native duration caches
remain evidence; arbitrary custom freeze animation is outside the projection.

The [gen1 batch](danoni-gen1-admission.md) extends `scheduled-constant` to
engines that initialize `frame_num` after deleting `timeline`/`ftimeline`.
Selected loaded fields must match the raw split arrays; compiled heads and
cached durations must match those raw frames; a later zero-counter snapshot
must retain the schedules, keys, parsed speeds and initial pixel speed.
`compiledIteration` and optional `clockInitializationIteration` distinguish
those observations. Original metadata survives cleanup in the reference.

Each chart additionally needs a visibly moving native arrow, its receptor and
stable source geometry, with at least three linear observations. Its arrival
must agree with the raw first frame within 1.5 native ticks. Early art registration
can differ from the actual judgment target; a recipe's explicit `targetY` requires
separate inspection of the original judgment code. Measurements record the
remaining frame-phase difference rather than shifting the chart to hide it.
Normal arrow geometry is used when an onigiri receptor has a different width.

Halts, truncated state/audits, undeclared requests and runtime errors fail.
The optional `reviewMissingDecorativeSprites: true` exception accepts only the
exact observed missing-attachment diagnostic, after schedule compilation and
with every required seven-key receptor present. FAKE800's two errors remain in
`presentationDiagnostics`; unknown errors are rejected. This is an explicit,
limited presentation review, not permission to ignore runtime errors generally.

## Constant-travel family added for v2

`scheduled-constant` proves every split raw tap/hold head against its compiled
spawn at `raw - set_cnt`, checks mutated banks and native hold-duration caches,
and requires the pre-consumption `frame_num == 0` state. `initialPixelSpeed`
is the native `set_spi`, proved to equal `dif_spi * 2`; `constantTravelFrames`
records `set_cnt`. Original coded `iro_dac` values remain in `nativePresentation`.
See [the nine-work runtime evidence](danoni-corpus-v2-admission.md).

Motion measurements now record `observedAuthorMultiplier`. For this family,
base speed uses `initialPixelSpeed`, and the actual measured velocity must match
that speed times the already active author multiplier. The older contracts
retain their existing measurement behavior.

A chart recipe may bind `optionalColorReview: {path,sha256}` with schema
`ddi-native-optional-color-review/1`, `sourceSwfSha256`, and `missingResources`.
Each missing resource has contained `path`, `sha256`, `url`, `retrievedAt` and
`status: 404`. Only the exact requested `iro.txt` and matching denied-fetch/error
pair qualify. Original capture bytes stay intact and normalized effects retain
the review and diagnostics. No successful substitute response is served.

## Recipe and normalized layout

```text
native-adapters-v1/<work>/
  inputs/                         original cached inputs plus our run configs
  recipe-final.json
  <capture>-final/{snapshot.json,run.json,audio.pcm,...}
  normalized-final/               immutable producer output
normalized/<work>/                canonical copy used by export registry
  manifest.json
  source-manifest.json            byte-identical acquisition provenance
  recipe.json
  source/inputs/
  references/<chart>.json
  references/<chart>-motion.json
  references/<chart>-audio/       independent shared-music difficulty check, if any
  native/<chart>/{projection,motion,audio}/
  media/<compressed-sha256>/{source.mp3,source-decoded.wav,music.wav,
                            stream.json,audio-origin.json}
  native-scroll-review.json
reviews/<work>/                   relocated, hash-bound motion review
```

Recipe schema `ddi-native-family-recipe/experimental-1`:

- `root`: absolute external corpus directory; `source: {path,sha256}` binds an
  acquisition manifest; `inputs` is a contained root-relative directory.
- `metadata`: optional title/artist hints, with original credits retained in
  projected metadata and the acquisition record. Explicit runtime `ARTIST_NAME`
  can supply the artist; otherwise it remains unknown rather than guessed.
- `charts`: nonempty array with unique safe `id`, `family`, selected `mode`,
  optional `label`, `sourcePrefix`, `sourceSuffix`, and `reviewMissingDecorativeSprites`.
  `sourceSuffix` is a string of up to 32 letters/digits/underscores, including
  the empty string, and applies only to `child-timeline-5`.
- `projection`, `motionCapture`, `audioCapture`: each binds a snapshot with
  `{path,sha256,runSha256}`; audio additionally has `pcmSha256`. Paths are
  root-relative. The successful parent run, movie identity and preserved config
  bytes must match. Ruffle revision is pinned.
- `motion`: `{note,receptor,lane,iterations}` identifies native MovieClip names,
  canonical zero-based lane index and worker iterations; optional `targetY`
  and `geometryReference` specify separately reviewed target/art geometry.

The normalized manifest uses `ddi-normalized-work/experimental-1`, shared with
the existing preparer. It records adapter/family source hashes, recipe/provenance
hashes, page URL and capture date. Each chart has `id`, `mode`, `metadata`,
`difficultyStars: null`, `music: {path,sha256,mimeType}`, `audioOrigin`,
`nativeReference`, `lanes`, `speed`, `boost`, `colors`, optional `stops`, and
`effects` describing source cursor ordering problems. Sharing music is scoped to
one work and requires both difficulties' native origins to agree.

Reference schema `ddi-native-family-reference/experimental-1` contains:

- `family`, `sourceSwfSha256`, `sourceSnapshotSha256`, `preparedIteration`,
  `clock`, `audioOrigin`, `metadata`, `limits`, `motion` and `projection`.
- `projection.mode`; canonical `lanes` with `name`, `nativeFrames` (tap heads),
  `nativeHoldFrames` (head/tail pairs), and optional `sourceOrderIssues`.
- `projection.speed` / `boost`: `{nativeFrame,multiplier}`. `stops`, when
  present: `{nativeFrame,paused}` for native visual toggles. `colors` is empty
  for these contracts; full original palettes/scripts remain in saved state.
- Early DAC families additionally retain `motionSpeedPolicy`, `speedValuePolicy`,
  `spawnLookaheadFrames`, `nativeSpeedChanges`, and optional
  `independentOnigiriPixelSpeed`; see the [precise gen0 fields](danoni-gen0-admission.md#per-arrow-speed-is-different-from-global-scroll).
- Native key codes when observed; scheduled families additionally retain
  `nativeLaneIndices`, `clockField`, `scheduleAgreement`, `preparedIteration`,
  `nativeSchedules` (tap/hold `{frame,lane,count}`) and presentation diagnostics.
- `motion` records FPS, px/sec, source reference width in stage pixels,
  arrow widths/sec, raw/observed arrival and per-iteration registration points.

All source frames are **60 Hz** for these verified contracts; worker iterations
are separate integers. The exact relative mapping is
`seconds = (nativeFrame - audioOrigin.logicalFrame) / 60`, including hold tails
and author events. Normalized lanes add `tapsSeconds` and `holdsSeconds`;
normalized events add `seconds`. Coordinates are native stage pixels; no device
pixels or milliseconds are implied. These are native references, not CW `/1`
references: they do not fabricate CW's `difData` or recovered authoring BPM.

## Music origin and presentation

The actual native stream start identifies its root/sprite container and
compressed payload range. Ruffle slice offsets index decompressed tag data,
excluding the SWF header; static full-file offsets add `tagStart`. A matching
head must agree on sample rate, channels and latency. The pinned reader's buffer
continuity is checked through the final nonempty payload; arbitrary seeking or
an unresolved underrun is rejected. Menu streams are not concatenated into songs.

FFmpeg decodes the preserved MP3. The tool compares its PCM against energetic
windows from the native stereo 44.1 kHz prefix, using full-lag normalized FFT
correlation within ±250 ms, at least two matching windows, correlation ≥0.97 and
lag spread ≤2 samples. Silence/constant buffers cannot establish alignment.
Comparison ends before a second observed stream starts. The measured origin
shift is rounded to original-rate PCM samples and applied to `music.wav`:
positive lag trims decoder lead-in; negative lag prepends zeros. Source MP3,
unshifted WAV, commands, hashes, correlations and rejected windows remain local.
Decipher needs approximately 50 ms of prepended silence in this comparison.
This aligns **the observed restricted Ruffle decoder**, not unmeasured original
Flash/device latency, and it is independent of BPM fitting or calibration.

The shared preparer emits Ogg and SSC with exact synthetic 75 BPM timing, generic
pickup previews and unknown-rating policy. Each work's measured baseline and
source speed changes determine `#SCROLLS`, multiplying the user's speed setting.
No Kouhaku BPM or movement parameter is reused. Ogg waveform loss alone can
trigger the same bounded higher-quality retry policy for any work; Decipher
passes at quality 0.7 while the other additions pass at the default 0.5. Musical-grid inference is not
automatically accepted for these songs. Boosts, native visual stop toggles,
colours and bespoke presentation may be omitted from SSC; loss reports and
references keep that distinction explicit. felys exports the stored final
out-of-order note; its original one-way cursor cannot spawn it. That behavioral
difference is declared in the effect-loss report rather than hidden.

## Running it

Build/test the worker as in its [README](../../tools/danoni-dump/ruffle/README.md).
Use `run.py WORKER CONFIG NEW_CAPTURE` for each reviewed native selection;
never run an acquired movie directly. Then bind the resulting snapshot/run/PCM
hashes in a recipe and use a fresh output directory:

```sh
uv run --with-requirements tools/danoni-dump/ruffle/requirements-analysis.txt \
  python tools/danoni-dump/ruffle/extract_native.py \
  "$HOME/Music/DanOni/native-adapters-v1/iso-fthosyu/recipe-final.json" \
  /private/tmp/fthosyu-export-new
```

First-time native audio matching uses NumPy/SciPy from the existing pinned
analysis requirements and installed FFmpeg. No new dependency was added.
Rebuilding already normalized works remains standard-library Python and native
Rust, with no browser or scientific package requirement. Register canonical
manifests plus every evidence hash and their relocated motion reviews, then
[stage, validate and promote](danoni-exports.md). Never replace a reviewed
manifest hash without comparing its data, origin and movement evidence.

Original synthetic tests cover family selection/source agreement, hold/stop
semantics, schedule corruption, unknown diagnostics, slice-offset/container
selection, audio shift/silence rejection and labels versus ratings. The native
suite exercises read-only inspection, network/filesystem/process restrictions,
getter avoidance, offscreen geometry, overlapping PCM clipping and bounded text,
frame and execution inputs. Real local exports are additionally checked with the
game parsers, native scroll multiplication and independent Ogg audio decoding.


The [presentation follow-up](danoni-presentation.md) corrects the earlier lack of
page-rating extraction: some surrounding HTML pages contain genuine stars even
when runtime metadata only supplies a label. It also records generic BPM
analysis for every normalized work. Current immutable native references and
playable metadata are not rewritten by that investigation.

## ParaFla counter extension (2026-10-11)

`parafla-counter` additionally proves parsed `arrowSpd` pixel increments against
`setSpd == 2 * difSpd`, retains numeric/divided sidecar selection and records
unplayed key-code-zero schedules separately. It does not discard an active extra
lane. The initial pixel assignment is the baseline, not an author change.
[Exact fields, proofs and omissions](danoni-paired-ports.md#the-parafla-counter-contract)
describe the two admitted charts. Unknown artist placeholders do not override
source-bound artist hints; the original runtime credit strings remain intact.

## Gen3 scheduled-counter extension (2026-10-11)

The [gen3 admission](danoni-gen3-admission.md) proves 21 further 5/7-key charts
from eleven works. The existing schedule/raw-head cross-check is retained.
`set_spi == 2 * dif_spi > 0` supplies independently corroborated baseline units
when present; later dos fields can overwrite earlier option-file speed values.
Movement proofs now account for a single native twip (0.05 source-stage pixels)
of positional/increment quantization, while retaining the 1.5-frame arrival
bound. Stable samples near arrival avoid extrapolation across author changes.
One source's fifth selector halts in native conversion and remains pending.


## Remaining gen4 listed works (2026-10-12)

The [gen4 admission](danoni-gen4-admission.md) reuses `parafla-counter` for
13 further works / 16 five-key charts. Its sidecars use a computed `dos.txt`
filename, so static string inspection cannot establish embedded data. Original
acquisitions remain unchanged; supplemental resource/robots receipts bind the
actual observed requests. The worker, projection schema and execution limits
are unchanged.

Moon's fast opening demonstrates why an early linear flight cannot be extended
through a forthcoming speed change. Three stable first-arrival samples prove
its original baseline and author multiplier. Its unplayed default gauge also
freezes the counter at an ordinary game-over screen; a menu-selected Practice
run supplies enough audio while retaining identical banks, schedules, keys and
speed units. An in-game failure is distinct from an AVM halt, and neither
justifies inventing missing clock observations. Mono and 22.05 kHz sources pass
the existing independent audio-origin comparisons without canonical resampling.


## Early five-key DAC extension (2026-10-12)

The [gen0 admission](danoni-gen0-admission.md) adds six works and seven
charts through `dac-5` and the fixed-control `dac-fixed-5` variant, with repeated
unconsumed child banks, selected loaded-field agreement, root hold pairs and independently observed movie-frame/audio clocks. Optional
diagonal banks must remain empty. The fixed-five-key Owen variant preserves
its source controls without inventing a key-code array.

The configurable variant sets new-arrow speed to
`initialPixelSpeed + 2 * (M - 1)` at source frame `F - 150`, then stores that speed on each arrow. Native `spd`, measured
velocity and authored arrival must agree. References retain original speed
values/application frames and portable ratios separately. SSC global scroll at
the authored frames approximates these per-arrow changes; every chart records
that limitation explicitly. All seven arrival measurements are exact; one
source requires an independently reviewed judgment target rather than its art
registration. Original decoder origins remain measured separately from grids.

The fixed-control variant uses multiplication by M and retains a separate
onigiri baseline. A late source review distinguished these laws before
promotion; matching the first arrow is insufficient when the first speed event
occurs later. Unknown independent onigiri event fields fail. The reference's
`speedValuePolicy` distinguishes the two laws, and
`independentOnigiriPixelSpeed` preserves the fixed lane's units.


## Collaboration selectors and terminal DAC comments (2026-10-12)

The [legacy single-song admission](danoni-legacy-single-admission.md) adds
ten Yamiyo author charts, 歪んだ神's Hard and two What's Going On charts.
`dac-5` now accepts explicit integer key label 5 as well as the string form.
Its speed-bank reader checks the complete native split array against the
selected loaded field, retaining only terminal empty/comment tokens after
complete pairs as `inactiveSpeedTail`. Unknown/interior tokens and incomplete
pairs still fail. Normalized effects explicitly record the inactive native tail.

A valid source run does not by itself prove a reusable projection. NIGHTMARE
rewrites some heads before the first split snapshot, Luvin' You halts in its
converter, and one Yamiyo selector has malformed note/hold fields. These stay
pending. An observed mode 11 is an explicit scope exclusion. The [2005 admission](danoni-2005-admission.md) subsequently establishes that
Air and Cold Breath use the advancing root movie timeline. Their child `Cnt`
is a wrapping arrow-instance counter. Five selectors now have exact native
arrivals and independently matched nonzero-seek MP3 audio; the earlier
child-clock suspicion was incorrect.


## Rust migration checkpoint, 2026-10-13

The [passive Rust port](danoni-rust-extraction.md) reproduces current structural
projections and movement/counter evidence from preserved data. Its comparison
exposes two frozen-reference gaps that the current Python contracts reject too:
RE:RAVE Hard has loaded speed data without a native speed bank, and Destined
Marionette's original motion spans author segments under the later initial-unit
check. These dated admissions remain historical results; they are not evidence
that the newest strict extraction regenerates those two charts. Accepted exports
remain unchanged while those policies/source paths are reviewed. The Rust stage
does not replace native execution, SWF audio demux or decoded-origin proofs yet.


### RE:RAVE source/control review, 2026-10-14

The [Rust review](danoni-rust-extraction.md#rerave-selector-controlled-speed-proof-2026-10-14)
now establishes that the archived SWF creates `speed_timeline` only for global
selector `humen=2` (LVUP). Hard selects 1 and retains the same root `speed_data`
without executing it. A narrowly hash-bound Rust contract accepts that case
and records `inactiveSourceSpeed`; ordinary structural missing-bank checks
still reject. No third-party code is transcribed. Fresh Hard extraction passes
frozen references, movement, independent audio-origin/PCM and game-parser SSC
checks. This closes the remaining known source-bank gap; the older Python
adapter remains conservative until retired. Existing exports are unchanged.
