# Native DanOni pilot: recovered Higan chart and export contract

Status, 2026-10-09: one real five-key Normal chart has a repeatable native
projection and reconstructed CW dos. All **562 taps, 9 holds, 12 speed changes
and 342 expanded arrow-colour changes** round-trip through cached CW v48.4.1
with zero timing error. Original Flash playback latency and full gameplay/visual
fidelity remain unmeasured. This is a pinned-work pilot, not general support for
the レク engine family. The [work plan](danoni-legacy-plan.md) and existing
[browser dump contract](danoni-corpus.md) remain the broader scope.

## Work and capture identity

Work: [彼岸帰航 ～ Titanic of Stygian～](https://autumnleaves.yukishigure.com/higan/higan.html),
chart レク, music どぶウサギ. SWF SHA-256
`6ce53b126e90b25a9f307595bf226ed4e6ce8f70c74d816a1d372c7f20de6e85`.
The page says source 2.15; the rendered title says Ver 2.16. Original page, SWF,
Shift_JIS `dos.txt`, separate `iro.txt`, HTTP evidence and credits remain local.
The native `tuning` value is the placeholder `Name`; exported credit uses the
page's chart-author credit and retains both sources.

Ruffle v0.7.1 at `89a7049965f3090e706265670df309a5128233ac` runs in the existing
restricted macOS worker. The bundled `default_font` feature fixes the title
button's native hit testing: the same click that previously failed now reaches
settings. Static inspection finds autosized text with zero declared RECT in the
button's hit state; text metrics are operational data here. Display-tree
inspection now reads child MovieClip storage directly, retaining the exclusion
of script MovieClipReference resolution, getters and prototypes. Original tests
verify that this traversal does not invoke a hostile getter.

Recorded viewport actions: click `(160,200)` at iteration 40; click `(50,230)`
at iteration 50. Release occurs on the next iteration. The original stage is
400×350; viewport coordinates are 320×240 with letterboxing. Movie frame 3 stays
active while `_root.top` advances from title frame 2 to settings frame 3.

## Initialization and timing evidence

The real engine loads dos again when play begins. Iteration 52 contains split
`timeline`/`ftimeline` inputs; iteration 53 contains transformed numeric tap and
hold-start arrays plus sparse compiled schedules; iteration 55 contains the
final schedules after cleanup. The engine deletes the source arrays during
cleanup, then consumes schedules during gameplay. Capturing only a later game
frame would miss or shorten the chart.

Native lane order is `left,leftdia,down,space,up,rightdia,right`. Five active
lanes become CW `left,down,up,right,space`, retaining native indices `0,2,4,6,3`.
Unused diagonal lanes retain nonnumeric placeholder strings in the raw snapshot;
the adapter excludes them only after confirming the compiled schedules contain
no notes there. The onigiri hold array is undefined in this capture and has no
compiled hold entries. All other flat hold pairs agree with native duration
storage, indexed by the original even endpoint index rather than hold ordinal.

At default settings, `set_cnt=100`, `correction=0`, `startFrame=0`, Motion OFF.
The observed transforms are:

| Data | Native logical coordinate |
|---|---|
| Tap spawn | Original note frame − 100 |
| Hold spawn | Original head frame − 100 |
| Hold duration | Original end − original head; checked against `frzLength` |
| Nominal judgement | Spawn + 100 |
| Speed application | Stored transformed `speed_dac` frame, already shifted by −100 |
| Colour application | `iro_dac` frame − 100; the runtime compares `frame_num + set_cnt` |

Root gameplay increments `frame_num` once per movie tick and displays time by
dividing by 60/3600. These are source/observed reasons for the logical 60 Hz
model, beyond the SWF's 60 Hz header. The mixer records stream initialization at
iteration 201, output sample-frame 147000, `_root.music_act` movie frame 2. The
post-frame root counter is **145**. For this projection, audio-relative seconds
are `(logical judgement/application frame − 145) / 60`. The first tap therefore
becomes frame 33, not 178, in a reconstructed zero-offset CW dos.

This is an observed Ruffle mapping. The host still uses wall-clock `getTimer`
and manual frame steps; it does not provide a general virtual Flash clock.
Audio-synchronized custom initialization, random generators, seeks, clock busy
loops and other works need independent adapters/tests. The generic snapshot
continues to declare `clock.timebaseVerified=false`; the narrow projection
records its own audio-origin evidence and scope.

## Embedded music

Sprite 1602 contains one stereo 44.1 kHz MP3 stream: 8,781 SWF blocks, including
3,180 empty blocks, 5,601 MPEG frames, 2,335,617 MP3 bytes and 6,452,352 declared
sample frames. The head and first-block latency are zero. Every nonempty block's
seek value matches the ideal timeline sample position minus samples already
written. All block headers and payload hashes are retained.

Adobe's SWF19 specification describes concatenating MP3 frames for continuous
playback; block `SeekSamples` aligns a restart after timeline seeking. Head/first
block latency additionally affects the origin. Nonzero block seek values alone
do not require skipping those samples repeatedly in continuous playback.
[SWF19, printed pp.182–186](https://open-flash.github.io/mirrors/swf-spec-19.pdf).
The demuxer permits the demonstrated continuous, zero-latency pattern and reports
other patterns as unverified. It removes only the four-byte SWF block headers,
without decoding, trimming or transcoding the MP3 payloads.

The emitted MP3 is SHA-256
`01f5e60b190aa012471813018a76d249d319845a108848d90fb5ad012c0f75a2`.
A separate local FFmpeg decode produced exactly 6,452,352 stereo sample frames;
duration estimates from probing VBR headers were less accurate. The native mixer
also reproduces its PCM prefix exactly across fresh restricted runs. Original
PCM impulse fixtures verify mixer sample positions and delayed startup. These
checks do not measure original Flash device/decoder latency.

Ruffle's pinned stream reader discards block seek offsets and has explicit
uncertainty around gaps. Consequently its output alone is insufficient to
validate seeking or arbitrary MP3 origins. Preserve the source metadata and
apply the narrower continuous-playback gate.
[Pinned decoder](https://github.com/ruffle-rs/ruffle/blob/89a7049965f3090e706265670df309a5128233ac/core/src/backend/audio/decoders.rs).

## Files and experimental JSON contract

The native observer's graph/config contract is in the
[tool README](../../tools/danoni-dump/ruffle/README.md). Real export files are:

| Path | Contents |
|---|---|
| `manifest.json` | `ddi-legacy-export/experimental-1`; reconstructed flag, adapter/source hashes, page URL, capture UTC, evidence paths, music descriptor, selected chart descriptors, separate assessments and limitations. |
| `charts/normal/chart.dos.txt` | UTF-8 reconstructed CW fields. Notes/effect frames are audio-relative at 60 Hz; blankFrame/adjustment/startFrame 0, playbackRate 1. |
| `charts/normal/chart.html` | HTML-escaped dos in `textarea#dos`; a data carrier. |
| `charts/normal/keys.json` | Mode `5`, five `{name,nativeLaneIndex}` entries in CW order, observed `nativeKeyCodes` in that same order. Original unused bindings remain in raw state. |
| `media/music.mp3` | Original MP3 payload sequence, demuxed as above. |
| `source/inputs/` | Byte-preserved original input directory, including provenance and resource/config files. |
| `native/{snapshot,run}.json` | Observed graph and successful restricted-parent report; both required for real export. |
| `native/reference.json` | `ddi-danoni-avm1-reference/experimental-1`, described below. |
| `native/audio.json` | `ddi-swf-audio/experimental-1`: selected sprite path, SWF identity, head settings, MP3 hash/length, continuity eligibility and all block metadata. |
| `native/{audio-prefix.pcm,reproducibility.json,independent-audio.json}` | Optional retained pilot evidence: native PCM including startup silence, fresh-run projection/PCM comparison with hashes, independent decoder command/version and sample count. These are supplementary to the exporter's required files. |
| `validation/cw/{capture,reference,conversion}.json` | Actual cached-engine browser observation, existing CW `/1` reference and comparison report. The producer assessment remains pending until this evidence exists and its hashes match. |

Native reference required fields: `adapter:string`, `sourceSwfSha256:string`,
`ruffle:BuildIdentity`, `fps:60`, `stages:{split,prepared,cleanup}` (integer worker
iterations), `audioOrigin:{logicalFrame,iteration,streamMovieFrame,outputSampleFrame,scope}`,
`metadata:{keyMode,label,authorSpeed,initialPixelSpeed,musicTitle,tuning}`,
`nativeKeyCodes:integer[5]`, `lanes:Lane[5]`, `speed:Speed[]`, `colors:Color[]`,
`nativeColors:NativeColor[]`, `boost:[]`, `stops:[]`, `scheduleAgreement:true`,
`limits:string[]`. Native references are not CW `/1` references and do not invent
an original `difData`; the exported row is explicitly reconstructed.

- Lane: `{name:string,nativeLaneIndex:integer,judgementFrames:number[],spawnFrames:number[],holdFrames:[number,number][],tapsSeconds:number[],holdsSeconds:[number,number][]}`.
- Speed: `{nativeFrame:number,seconds:number,multiplier:number}`.
- Color: `{seconds:number,target:integer,color:string,all:true}`; targets are CW
  lane indices. Native `al` expands to five arrow targets; `oni` becomes target 4.
- NativeColor: `{frame:number,target:string,color:string,seconds:number}`,
  preserving the original `0xRRGGBB` code and target.
- Audio blocks: `{movieFrame,tagOffset,sampleCount,seekSamples,mp3Frames,mp3Offset,mp3Bytes,sha256,sourceSampleOffset,seekResidualSamples}`. All numeric fields
  are integers here; offsets refer to decompressed SWF bytes or concatenated MP3
  bytes as named. Sample counts/offsets count stereo sample pairs, not channels.

The schema selects one chart per directory. Preserve every lane for 7/9-key
adapters; never map them to four lanes. Original key definitions and arbitrary
custom engine behavior need their own evidence before extending the adapter.
The importer session can consume the carrier/dos using its existing CW path;
recognizing this experimental manifest or applying native bindings is a separate
integration decision. Resolve dos music paths relative to the chart carrier.

## Reproduce locally

Build and run the original synthetic tests as described in the tool README.
Use fresh output directories; the existing acquisitions need no new downloads:

```sh
python3 tools/danoni-dump/ruffle/run.py \
  /private/tmp/ddi-ruffle/build/target/debug/ddi-ruffle-probe \
  "$HOME/Music/DanOni/legacy-pilot/inputs/autumn-higan/audio-origin.json" \
  /private/tmp/higan-capture-new
python3 tools/danoni-dump/ruffle/higan.py /private/tmp/higan-capture-new/snapshot.json \
  "$HOME/Music/DanOni/legacy-pilot/inputs/autumn-higan" /private/tmp/higan-export-new
mkdir -p /private/tmp/higan-export-new/validation
node tools/danoni-dump/ruffle/cw-roundtrip.mjs \
  "$HOME/Music/DanOni/corpus/jam-with-pancake" /private/tmp/higan-export-new \
  /private/tmp/higan-export-new/validation/cw
```

CW adds an observed 201-frame automatic lead-in for the early first note. The
comparison removes that observed `intAdjustment` only after checking it agrees
with `preblankFrame`, with zero author/user adjustment and playbackRate 1. It
compares every tap, both hold endpoints, speed value and expanded colour value,
including duplicate events. It never changes expected data to fit CW. Cosmetic
translation and judgement/gauges remain outside the validated projection.

The [single-song family adapters](danoni-native-adapters.md) now cover eight
further works, including seven-key examples and two RE:RAVE difficulties. The
original Straight Soul is excluded from the active corpus because its startup
requires server-backed registration; its HTML5 port is separately captured. Legacy nine-key adapters
remain future work. Chart Reverser remains useful for source-known
BPM/first-number cases; no authoring-grid equivalence is claimed for this pilot.
