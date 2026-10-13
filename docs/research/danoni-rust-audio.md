# Rust audio preparation and decoder origins

Implemented 2026-10-13 as the codec/origin portion of [milestone 3](danoni-rust-plan.md).
The Rust library now decodes supplied normalized audio, writes canonical PCM16
WAV, optionally encodes Ogg Vorbis in process, and produces the neutral media
receipt consumed by `prepare_work`. The decoder-origin/resampling math also runs
in Rust and is independently checked against existing restricted Ruffle captures.
No URL acquisition, SWF demux, worker supervision or new BPM fitting is claimed.
Existing accepted exports are unchanged; all corpus outputs stay local.

## API and units

`audio::Pcm16 {rate,channels,samples}` holds signed interleaved PCM16.
`frames()` counts **sample frames**, not interleaved values. Accepted dimensions
are mono/stereo, 5512–192000 Hz, nonempty, at most 600 seconds and a WAV output
within 128 MiB. `wav()` writes a simple complete 44-byte-header RIFF/WAVE.
`shifted(frames)` trims positive decoder lead-in or prepends silence for a
negative correction, bounded to half a second. No note or musical phase enters
this operation.

`audio::decode(bytes,&DecodeOptions,checkpoint)` returns `Decoded {pcm,report}`.
The only option is `trackId` (default null); one audio track must be selected
unambiguously. WAV, MPEG Layer III MP3 and Ogg Vorbis are enabled in Symphonia.
Bytes and decoded frames are bounded; malformed packets, channel/rate changes,
chained/reset streams and inconsistent exact frame counts reject. Packets are
never intentionally skipped to shorten the song. Finite samples become PCM16 by
nearest ties-even rounding and saturation. Byte-identical samples are expected
for PCM16 input; different MP3 implementations can differ by one least significant
sample value without changing sample placement.

The report `ddi-danoni-audio-decode/1` records decoder/container/codec/track,
`gapless`, `reportedFrames`, `exactExpectedFrames`, encoder delay/padding,
`mp3Inventory`, output dimensions, packet count, zero skipped packets and the
PCM conversion policy. Canonical WAV **container bytes** need not equal FFmpeg's
WAV, which may carry ancillary chunks; sample identity/alignment is checked
separately, and the original source container remains preserved.

### MP3 payload length and metadata

An independent bounded inventory counts complete MPEG Layer III frames, checks
frame boundaries and constant rate/channel dimensions, and identifies the first
Xing/Info/VBRI metadata frame separately from playable payload. Leading ID3v2
and trailing ID3v1 are supported. Free-format MP3, other MPEG layers, unexplained
bytes, incomplete frames and metadata frames inside the audio payload reject;
this is a declared subset, not a general-purpose MP3 repairer.

`mp3Inventory` records `mpegFrames`, `payloadSampleFrames`, `metadataFrame`,
`sampleRate`, `channels` and its length basis. Xing/Info identification accounts
for both fixed side-information offsets and CRC-marked frames; the original
synthetic regression includes the fixed-offset CRC case. MPEG frame samples are
1152 for MPEG-1 Layer III, 576 for MPEG-2/2.5.

Symphonia's reported MP3 length can come from metadata or an estimate. For an
untagged stream, this tool disables gapless trimming and requires the exact
counted payload frames, avoiding an estimate silently clipping the tail. For a
tagged stream, it retains decoder delay/padding semantics and requires the actual
complete payload minus leading delay, capped by the declared playable length.
This permits a whole-frame SWF stream slice whose inherited tag describes a
longer original song, without claiming the declaration describes the extracted
payload. It does not bypass malformed/truncated frame checks.

Three existing raw streams (`7fes06-decipher`, `reku-019-mahou-mahou`,
`reku-102-turuturu-turupeta`) have inherited Xing/Info lengths longer than their
physical slice. Another (`reku-050-dosmf08-last-saigo`) carries fixed-offset Info
metadata on a CRC-marked frame. These are discoveries/examples, **not work-name
branches**: the inventory/decoder policy is identical for every input. See the
[Symphonia MP3 reader](https://docs.rs/symphonia-bundle-mp3/0.6.1/src/symphonia_bundle_mp3/demuxer.rs.html)
for the distinction between tag lengths, duration estimation and gapless packet
trimming. The existing local `stream.json`/origin receipts retain the complete
source-side frame/block/seek evidence; porting their SWF demux remains milestone 4.

## Resampling and measured origins

`audio::resample(pcm,rate,stereo,checkpoint)` uses Rubato's FFT resampler, 1024
input-frame chunks and BlackmanHarris2 window. It removes the reported output
filter delay, flushes the tail and returns exactly
`ceil(sourceFrames × outputRate / sourceRate)` frames. Mono duplication to stereo
is explicit; channels are not averaged. Same-rate conversion preserves samples.
The versioned report includes rates, frame counts, removed output delay and the
channel/length policy. Synthetic impulses at 22050→44100, 44100→48000 and
5512→44100 independently check placement within one output sample frame.

`audio::origin_match(native,decoded,startFrame,endFrame,checkpoint)` returns
`ddi-danoni-audio-origin-match/1`. Inputs must have equal rate/channel dimensions;
for captured Ruffle this is 44100 Hz stereo. The native interval starts at the
recorded stream-start output frame and optionally ends at the next stream start.
At least half a second of overlapping observed PCM is required.

The generic policy matches the earlier Python evidence gate: energy-ranked
windows (at most eight), at most one-third second each, a ±one-quarter-second
lag search, per-channel demeaning, FFT correlation normalized by local energy,
minimum RMS 4 PCM units, correlation ≥0.97, at least two retained windows and
lag spread ≤2 sample frames. Opposite-phase stereo is preserved. A mean lag uses
ties-even rounding. Quiet/unrelated/inconsistent data fails; no guessed origin
is substituted. Callbacks occur during packet decoding, inventory/resampling
chunks and correlation candidates/windows/channels.

The result records `rate`, `lagSamples`, `maximumSpreadSamples`, retained and
rejected `windows` with `{part,startSeconds,lagSamples,correlation}`. A positive
lag means the decoder has lead-in relative to native playback. Convert to source
rate using `round_ties_even(lag × sourceRate / nativeRate)` before `shifted`.
The extraction supervisor must bind exact original MP3, captured PCM, snapshot,
run receipt, stream selector and this proof when integrating the operation.
This API does not itself certify arbitrary caller-supplied capture provenance,
original Flash/device latency, seek behavior or unobserved later song portions.

## Native Ogg encoding

The separate `vorbis` feature, enabled by default for the standalone tool, uses
`vorbis_rs` with its statically built aoTuV/Lancer libvorbis and libogg dependencies.
It is an in-process encoder, with no system FFmpeg/oggenc subprocess. A C compiler
is required to **build** those bundled dependencies; it is not a desktop end-user
runtime requirement. The game workspace and its WASM dependency tree are unchanged.
`--no-default-features` removes both native filesystem and C encoder dependencies,
while decoding/resampling/origin math and WAV preparation remain available.
`--no-default-features --features native` gives a WAV-only filesystem CLI.

`audio::encode_ogg(pcm,quality,checkpoint)` accepts 8000–96000 Hz mono/stereo,
finite quality 0–1 (default **0.5**, matching the old default quality setting),
uses a fixed Ogg serial, explicit encoder comment and 1024-frame PCM chunks.
An output sink enforces 128 MiB. Samples are not resampled or musically shifted
by encoding. Repeated encoding with the pinned build gives identical output;
this patched encoder is not promised byte-identical to the previous installed
libvorbis. It is intentionally a lossy transform, as authorized by the maintainer.

Every encoded stream is decoded with Symphonia and checked for exact rate,
channel and sample-frame counts. The first up-to-12-second interval is also
correlated against the uncompressed input. A measurable displacement greater
than one sample frame rejects. Uninformative/insufficient correlation is recorded
as `signalAlignment:"unproved"` with a stable diagnostic; the exact dimensional
check remains mandatory. A cancellation/decode error is never swallowed as weak
evidence. The existing 93-song generation yields **measured zero-frame lag for
all 93**; this is stronger evidence than the length check alone. This does not
claim independent full-track waveform/phase equality for lossy audio.

## Portable media job and receipt

`audio::prepare_media(&MediaRequest,&AssetSource,checkpoint)` returns
`PreparedMedia {files,manifest}`. `native::prepare_audio` exposes it through:

```sh
target/danoni-rust/debug/ddi-danoni-export prepare-audio JOB.json NEW_OUTPUT
```

```text
schema: "ddi-danoni-audio-job/1"
roots: {namespace: directory, ...}
request:
  manifest: Binding                    exact normalized source document
  audio: Binding                       one music hash bound by that document
  codec: "wav" | "ogg"
  decode: {trackId?: integer | null}
  quality: number                      default 0.5
```

Bindings/relative directories use [the offline job rules](danoni-rust-offline.md).
The source manifest must be one of the supported normalized/legacy/synthetic
schemas and bind the selected music hash at work or chart level. This stage
consumes **already normalized audio**, which can already include a measured
SWF decoder-origin correction. It does not silently realign it a second time.
The raw-origin math above belongs to the earlier extraction producer.

```text
manifest.json          ddi-danoni-prepared-media/1 readiness receipt
source-manifest.json   original normalized manifest bytes
source.bin             original supplied audio container unchanged
pcm.wav                canonical decoded PCM16 WAV
music.wav|ogg          playback container
```

The neutral receipt's required fields remain
`sourceManifestSha256,sourceAudioSha256,playbackSha256,pcmSha256,sampleRate,
channels,sampleFrames,codec`. Additional fields record `producer` with the
**`native-media/1`** algorithm/dependency identities, decode/validation diagnostics,
exact request and `artifacts[] {path,sha256,bytes}`. WAV playback equals PCM bytes.
The native writer verifies that inventory and writes only to a fresh external
folder, checking cancellation while writing; an error removes its owned partial
folder, and the final manifest rename marks readiness. These prepared folders
are not resumable jobs or the library publication transaction.

The host creates `SuppliedMedia` by binding `music.<codec>`, `pcm.wav` and
`manifest.json` in its selected namespace. This plugs into the existing
`prepare_work` API with the original source audio hash unchanged. No Python receipt
is needed. Do not replace source metadata with the decoded PCM's new container
hash; both source and derived identity have their own fields.

## Local acceptance

Receipts and derived music stay under `~/Music/DanOni/reviews/rust-audio-20261013/`.
The baseline is the sealed 93-song/144-chart Rust trial and accepted source data.

- Source decoding: **93 songs**, 92 already normalized WAV sources with exact
  PCM samples; one MP3 source with maximum one-unit PCM difference. All have
  equal frame counts and measured **zero-sample** lag versus accepted reference
  PCM. This is not 93 independent raw-MP3 decodes.
- Raw decoder/origin validation: **79 demuxed MP3 streams**, 62 at 44100 Hz,
  13 at 22050 Hz and four at 11025 Hz, checked against hashed captured Ruffle
  stereo PCM and recorded stream intervals. **All source-rate origin corrections
  equal the previous Python/FFmpeg-derived correction exactly**. No exclusions.
- Native codec stage: freshly decode and encode **all 93 songs**, produce neutral
  receipts and feed them into complete-song preparation. All chart diagnostics,
  source ratings/artwork and preview starting seconds match; Ogg round-trip
  dimensions and first-interval measured zero-frame lag pass. Original music,
  prepared sources and accepted exports remain unchanged. A development-only
  independent FFmpeg decode of **all 93 new Ogg files** also has exact frame
  counts and measured **zero-frame lag**, with no external decoder in production.
- Original synthetic tests: sample extremes/channel identity, impulses/filter
  delay, signed origins, inconsistent/silent evidence, PCM bounds, malformed
  containers, complete MP3/ID3/Info frames, truncated frames, cancellation,
  reproducible Ogg/frame/phase checks and receipt→song integration.

The optional corpus gates are `DDI_DANONI_AUDIO_GENERATION` for a sealed Rust
source generation, `DDI_DANONI_AUDIO_ORIGIN_CASES` for a local hash-bound list of
recorded MP3/native/snapshot/run/origin inputs, and `DDI_DANONI_AUDIO_OUTPUT` for
fresh derived output. The origin-case list is research input reconstructed from
existing captures, not a production URL discovery or operator recipe requirement.
`DDI_DANONI_AUDIO_MEDIA` plus `DDI_DANONI_FFMPEG` enable the independent decoder
gate over already written codec outputs; it writes no music and invokes FFmpeg
only from a development test. The codec gate always requires a new output directory. Without inputs these
local gates report skips; the source content is never a repository fixture.

**44 synthetic tests and seven local corpus gates** pass, as do **22 audio/
artwork/offline tests without default native/encoder features**, Clippy on both
feature sets, formatting and `just check` (REUSE **687/687**). Local receipts
include explicit CLI job/output, both final codec and independent decode reports,
raw native origin comparisons and a hash-bound validation summary.

## Dependencies and next work

Latest stable versions were checked against crates.io on 2026-10-13:
[Symphonia 0.6.1](https://docs.rs/symphonia/0.6.1/symphonia/) (MPL-2.0),
[Rubato 5.0.1](https://crates.io/api/v1/crates/rubato) (MIT/Apache-2.0),
[RustFFT 6.4.1](https://docs.rs/rustfft/6.4.1/rustfft/) (MIT/Apache-2.0),
[audioadapter-buffers 5.2.0](https://crates.io/api/v1/crates/audioadapter-buffers) (MIT/Apache-2.0),
and [vorbis_rs 0.5.6](https://docs.rs/vorbis_rs/0.5.6/vorbis_rs/) (BSD-3-Clause).
The encoder's sys crates/bundled codec code retain BSD notices; a desktop binary
must ship applicable dependency notices. No upstream implementation was copied
into our MIT files. Audio math remains host-consumable; the C encoder is optional.

Next port onset/pulse analysis and robust note-lattice/shared-audio BPM selection,
with explicit algorithm/cache versioning and ambiguous/sparse/variable-tempo
outcomes. Preserve the current 9 ms note/hold budget, native authored base scroll
and every original reference. Reproducing librosa numerically is not assumed.
SWF stream demux/provenance wrappers and restricted worker execution remain
milestone 4, followed by URL acquisition and browser/desktop integration.
