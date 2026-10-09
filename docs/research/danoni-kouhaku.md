# 2004 Kouhaku multi-song SWF extraction

Validated locally on 2026-10-09. The restricted Ruffle tool now exports the
maintainer's `~/Music/DanOni/2004kouhaku.swf`: **five songs, six five-key charts,
2,493 taps**, including the hidden 桃源郷 Hard chart. Source/compiled arrays agree;
each reconstructed CW chart passes the cached v48.4.1 browser comparison with
zero note-time differences. The generated simfiles also parse in the game.
This is a second pinned engine family, with explicitly limited audio timing
claims; the [Higan pilot](danoni-native.md) remains a separate adapter.

## Source and inventory

Original page: [第３回紅白FLASH合戦 / DANCING☆ONIGIRI](http://eucaly.net/~iso50/kouhaku.html).
The maintainer supplied the local SWF and this URL. The page's object/embed and
DL link name `2004kouhaku.swf`; the supplied SWF was not downloaded again. The
page was fetched once through the shared paced, robots-aware cache, preserving
its Shift_JIS HTML and response evidence. Original SWF acquisition date is
unknown; local file modification time and inspection/capture dates are retained
separately. Public availability does not license redistribution; all work assets
and extracted music remain in the local corpus.

The SWF is CWS / Flash 6 / AVM1, 2,512,142 original bytes, 3,079,135 declared
uncompressed bytes, header 60 Hz. Pinned SHA-256:
`e14e472911b6603ddcc58bd40049d0b05cfa986a2240209dd848a516cf40fed2`.

| Chart id | Native title | Music credit | Taps | Stream sprite |
|---|---|---|---:|---:|
| `tougenkyo` | 桃源郷 | 公 | 333 | 229 |
| `tougenkyo-hard` | 桃源郷 hard | 公 | 605 | 229 |
| `run` | RUN | tio | 401 | 232 |
| `kitakaze` | 北風のメロディー | 八乙女葦菜 | 433 | 234 |
| `korokke` | ころっけわっしょい | ゆうかり | 407 | 236 |
| `emy` | emy | onoken | 314 | 238 |

These six charts contain taps only; default movement is constant. No holds or
chart speed/boost/colour events are claimed. Native difficulty labels are 3/8/6/5/4/5
stars, retained as strings. Simfiles use `Edit`, meter 1 as a placeholder, with
native stars in the standalone pack's chart descriptions. Synthetic 75 BPM
encodes frame timing; it is not a recovered musical BPM or original editor grid.

The page spells the fourth music name “ころっけわしょーい”; the runtime spells it
“ころっけわっしょい”. The title graphic of the first song includes “-Remix-”. Keep
original page/runtime evidence; the exporter uses native stored title strings.
Chart authorship is not established by the page or captured fields and is left
unidentified, rather than inferred from the hosting account.

## Reaching and cross-checking the chart data

The original song menu uses arrow keys and Enter. The hidden chart is reached
through the original **FMV** button in Options: Space opens Options, clicking
FMV changes the game's selection state, Space returns to Music Select, and
Enter starts the selected chart. The extractor records these inputs; it does
not patch movie scripts or write chart-selection variables.

Root `*_data` strings agree byte-for-decoded-string with the selected menu
state and with the child controller's split `*_timeline` arrays. The controller
starts with `Cnt=1`, and the lane cursors have not consumed notes. Lane order
is left/down/up/right/space, native keys 37/40/38/39/32. Song title and default
movement settings live in the SWF6 case-insensitive `_global` environment.
The new opt-in `inspectGlobals` snapshot exposes stored enumerable global data,
excluding function values and non-enumerable properties; getters are never run.
Inspecting built-in constructor graphs exhausted the initial bounds, so those
incomplete attempts were rejected and retained only as scratch diagnostics.

The pinned controller spawns a note when its parent timeline frame equals the
source frame minus 150. Source frame arrays are preserved in the native reference;
the portable nominal grid subtracts **2**, the observed parent timeline frame
at the Ruffle stream start, then divides by 60. Two immediately following
captures verify successive timeline frames 3 and 4. This is a recorded nominal
frame convention: the old engine judges through moving hit-box geometry,
whose centres need not coincide exactly with an integer author frame. Screenshots
and display bounds confirm movement/receptor crossing; full legacy judging,
visual fidelity and physical centre timing are outside the conversion claim.

## MP3 source and WAV playback

All five songs are embedded **MPEG Layer III, mono, 22,050 Hz** streams.
The demux preserves each MP3 frame byte-for-byte after stripping the SWF block's
four-byte sample-count/seek header, and retains every block's timing metadata.
There is one music file for the two 桃源郷 difficulties. UI sounds are separate
DefineSound resources and are not exported as songs.

Playable exports use **untrimmed signed 16-bit PCM WAV**, decoded by the installed
FFmpeg without resampling. Decoded sample count must equal the sum of validated
MP3 block counts. MP3 originals remain alongside the WAVs, and the manifest
records hashes, decoder version and exact command. WAV avoids further lossy
encoding and makes the chosen sample origin explicit across browser decoders;
there is no conversion to Ogg Vorbis or another MP3 encoding.

Every stream declares `latencySeek=1633` samples (about 74.06 ms) and starts
with a nonzero block seek. Later seek metadata also has nonzero residuals under
the existing continuous-zero-latency criterion. Therefore the Higan audio
eligibility check **does not pass** for this collection. The pinned Ruffle
stream reader strips block seek fields and does not apply that latency.
[Source: Ruffle v0.7.1 stream reader](https://github.com/ruffle-rs/ruffle/blob/89a7049965f3090e706265670df309a5128233ac/core/src/backend/audio/decoders.rs).

The exporter retains the untrimmed sample origin observed in that Ruffle path;
it neither silently discards 1,633 samples nor calls the original Flash timing
verified. CW round trips validate chart-to-export timing at this chosen origin.
They do not establish original Flash compensation, arbitrary seeks, output-device
latency or perceived musical synchronisation. An Ogg transcode alone cannot
resolve those source/runtime uncertainties. A later audio correction must carry
its own evidence and recorded transformation.

## Local layout and reproduction

Permanent export:
`~/Music/DanOni/legacy-pilot/exports/2004-kouhaku/`.
Import **its `playable.zip` or `playable/` folder** into the game: five song folders containing
`music.wav` and `song.sm`, with both 桃源郷 charts in its one simfile. The other
files are preservation/research evidence. `danoni-5` is this repository's five-key
simfile layout id; other simfile players may not recognise it. CW dos carriers
provide a community-compatible alternative.

| Path | Contract |
|---|---|
| `manifest.json` | `ddi-legacy-export/experimental-1`; six `charts[]` with string `id`, dos/page/simfile/keys/nativeReference paths, snapshot SHA, lanes/effects, per-chart `music` descriptor and validation path. `playablePack` points to `playable`. |
| `source/inputs/` | Original hashed SWF, source provenance, and six exact restricted-run configs. Original acquisition date remains null. |
| `native/<id>/` | Parent `run.json`, full `snapshot.json`, `reference.json`, audit stderr and observed PCM prefix. |
| `native/audio-<sprite>.json` | MP3 stream/block metadata, original/demux hashes, counts, latency/seek evidence and explicit WAV transformation. |
| `media/<sprite>.{mp3,wav}` | Preserved demux MP3 and untrimmed decoded WAV. Five shared music descriptors, each referenced by its chart(s). |
| `charts/<id>/` | UTF-8 `chart.dos.txt`, inert `chart.html` carrier, `keys.json`, and one-chart `chart.sm`. |
| `playable.zip` | Optional local ZIP of the standalone pack only; DEFLATE compression preserves WAV samples exactly. `validation/pack.json` records its hash and integrity result. |
| `playable/<song>/` | Standalone five-song pack: music and simfiles only. Music copies are byte-identical to exported WAVs. |
| `validation/<id>/` | Cached CW capture/reference/conversion reports; verify their chart id and source snapshot/dos hashes. |
| `native/reproducibility.json` | Optional retained six fresh-run comparisons of projected references and PCM prefixes. |
| `validation/simfiles.json` | Optional local game-parser comparison of every note lane/time and standalone pack/music resolution. |
| `source/page/` | Optional retained original page/robots acquisition evidence. |

`native/reference.json` uses the experimental AVM1 reference type described in
[danoni-native.md](danoni-native.md), with collection-specific metadata and
`audioOrigin` fields. `judgementFrames` preserves the nominal author integers,
`spawnMovieFrames` records source minus 150, and `tapsSeconds` records source
minus the observed origin 2, divided by 60. Holds/effects are empty here.
All `/1` CW schemas remain unchanged. Multi-chart experimental exports put
`music` and `sourceSnapshotSha256` on each chart; the Higan single-chart top-level
fields remain accepted by the comparison tool. Producer conversion assessment
starts unverified; acceptance uses the matching per-chart validation reports.

```sh
python3 tools/danoni-dump/ruffle/kouhaku.py \
  "$HOME/Music/DanOni/2004kouhaku.swf" /private/tmp/kouhaku-export-new \
  --worker /private/tmp/ddi-ruffle/build/target/debug/ddi-ruffle-probe

# Repeat for each of the six chart ids in the inventory.
node tools/danoni-dump/ruffle/cw-roundtrip.mjs \
  "$HOME/Music/DanOni/corpus/jam-with-pancake" \
  /private/tmp/kouhaku-export-new /private/tmp/kouhaku-cw-new tougenkyo

DDI_EXTRA_SIMFILES=/private/tmp/kouhaku-export-new/playable \
  cargo test -p ddi-chart --test simfiles extra_simfiles_from_env -- --nocapture

python3 tools/danoni-dump/ruffle/test_kouhaku.py
```

Requires the isolated worker build and installed FFmpeg. Outputs must be fresh
and outside Git. The adapter accepts only the pinned SWF hash, uses the restricted
runner for every capture, and performs no network acquisition. Original synthetic
tests cover selection controls, global getter exclusion, held-key bounds, changed
state rejection and exact five-key simfile rows. All six CW conversions and
fresh native projections/PCM prefixes reproduce locally. The standalone game
parser finds five songs, six charts and all music, and every generated note time
matches the chosen nominal native reference within 1e-12 seconds.

## Numeric ratings and musical-grid follow-up (2026-10-09)

The original pack above is retained as produced by adapter `/1`. Adapter `/2`
now records integer `difficultyStars`, null classification and a separate
`rawDifficulty` provenance string; simfiles use that count as meter. A separate
local Ogg pack uses the tested sample origin and keeps every note time unchanged.
The maintainer reports good sync, with a possible uncertain ~20 ms discrepancy
on some songs; no automatic correction is made.

[danoni-beat-grid.md](danoni-beat-grid.md) gives the revised metadata, codec
validation, measured musical tempo hypotheses and the importer/gameplay work
needed to use them for colours while keeping the exact 75 BPM frame clock.
