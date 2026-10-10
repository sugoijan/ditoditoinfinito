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
ambiguous difficulty suffix placements are rejected. Data order is retained.

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
  optional `label`, `sourcePrefix`, and `reviewMissingDecorativeSprites`.
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
