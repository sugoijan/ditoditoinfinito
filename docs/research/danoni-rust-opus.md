# Native Ogg Opus exports

Implemented 2026-10-17, following the [consolidation](danoni-consolidation.md).
The fresh native URL/import preparation default is now `codec: "opus"`.
This changes playback encoding, without changing source chart times, SSC grids,
scroll factors, preview selection, or source-runtime audio-origin corrections.
Previously generated packs are not rewritten automatically.

## Dependencies and encoding policy

The default SDK features are `native` and `opus`. The optional `vorbis` feature
retains the existing `codec: "ogg"` route; `codec: "wav"` needs neither encoder.
A missing requested encoder returns `encoder-unavailable` without substitution.

Latest stable releases checked on 2026-10-17:
[rusty-opus 1.0.1](https://docs.rs/crate/rusty-opus/1.0.1) (BSD-3-Clause) and
[ogg 0.9.2](https://docs.rs/ogg/0.9.2/ogg/) (BSD-3-Clause). Both are Rust; no
external codec process or C codec library is needed for Opus. Optional Vorbis and
TLS retain their existing native dependencies; this is not a claim about every
SDK dependency. No upstream implementation was copied into our MIT sources.
Cargo lockfiles and runtime notice collection record the added dependencies.

Opus accepts mono or stereo PCM at 48 kHz. The existing delay-compensated Rubato
resampler supplies `ceil(source_frames * 48000 / source_rate)` frames. Channels
are preserved, including mono. No samples of source leading silence are cropped.
An impulse regression exposed a Rubato odd-input-FFT center mismatch at
5512 → 48000 Hz (four output frames, about 0.08 ms). The adapter doubles only
odd input FFT lengths, making the sinc center agree with the reported filter
delay. The original even-FFT paths, including every corpus source rate, keep
their existing filter parameters. The receipt records the actual input chunk size.

`quality` keeps the existing finite 0..1 request field. Opus maps this to
`(64000 + quality * 64000) * channels` bits/second, with integer truncation before
channel multiplication. The default 0.5 selects 96 kbit/s mono / 192 kbit/s stereo.
The encoder uses 20 ms packets, VBR, complexity 9, PCM16 depth and the public
`RestrictedLowDelay` application: music-oriented CELT with one stable 120-frame
(2.5 ms at 48 kHz) overlap throughout a song. This intentionally avoids switching
between codec modes with different delays. The crate's public API has no
lookahead query. The fixed overlap is an explicit, independently tested encoding
policy, not the 312-frame delay assumed by the crate's general example. A packet
that departs from CELT rejects rather than silently using the wrong delay.

The Ogg writer uses a fixed serial (`0x444449`), separate OpusHead and OpusTags
pages, up to one second of packets per data page, pre-skip 120, zero output gain,
and channel mapping family 0. The encoder is fed sufficient zero padding to flush
the overlap. Intermediate granules count all decoded frames; the final granule
is exactly `120 + resampled_frames`, removing only the last packet's padding.
See [RFC 7845 §§4.2–4.5](https://datatracker.ietf.org/doc/html/rfc7845#section-4.2)
for pre-skip, sample positions and final trimming, and the
[encoder API](https://docs.rs/rusty-opus/1.0.1/rusty_opus/struct.OpusEncoder.html)
for application and bitrate controls.

## Files, receipts and units

- `music.opus`: standard Ogg Opus playback; SSC `#MUSIC` names this file.
- `pcm.wav`: **original decoded source-rate PCM16**, retained as analysis and
  timing evidence. Tempo fitting and previews read it, as before.
- `source.bin`, `source-manifest.json`: original input and source binding.
- Media receipt: existing `ddi-danoni-prepared-media/1`, `codec: "opus"`.
  `sampleRate`, `channels`, `sampleFrames`, `pcmSha256` describe `pcm.wav`.
  `playbackSha256` binds `music.opus`.
- `producer.audioPolicy: "native-opus-media/1"` distinguishes the new policy.
  `producer.playbackSampleRate` is 48000; `playbackSampleFrames` is the resampled
  count. `producer.resampling` records source/output units and removed filter
  delay. `producer.encoding` records bitrate, quality, application, packet size,
  pre-skip, final granule, end trimming and serial.
- `validation` compares decoded playback with the **resampled** analysis PCM.
  Rate, channels and exact frame count must match. Signal matching must have at
  most one frame of measured lag where informative; ambiguous/quiet material is
  marked unproved, preserving the existing explicit diagnostic policy.
- Complete song receipts retain source-rate `pcmSampleRate` / `pcmFrames`;
  their copied media receipt supplies playback units. Source/SSC offsets are
  never changed to compensate for codec delay.

The bounded Rust Opus decoder verifies physical page continuity, one serial,
sequence numbers, CRCs, EOS and exact final granules. It rejects truncated,
chained/multiplexed or unsupported streams, applies pre-skip and header gain,
and checks cancellation throughout. This is a strict family-0 source/output
adapter, not a replacement for the desktop's broader player decoder.

The installed host identity now includes enabled Opus/Vorbis features and its
updated SDK lock/source hashes. Rebuild/reinstall the host prefix before using
the new URL defaults. Existing Ruffle worker identities/captures are unchanged.

## Verification and local reproduction

Synthetic gates include deterministic output, exact lengths, signed phase,
malformed/truncated/CRC-damaged input, cancellation, and a fresh-preparation
comparison that keeps source PCM, chart diagnostics and preview metadata exact.
The independent FFmpeg/libopus gate covers mono/stereo, one-sample clips, clips
shorter than pre-skip, exact/adjacent packet boundaries, odd endings, leading
silence, and source rates 5512, 8000, 22050, 44100, 48000 and 96000 Hz.

```sh
DDI_DANONI_FFMPEG=/path/to/ffmpeg cargo test --locked \
  --manifest-path tools/danoni-dump/rust/Cargo.toml --test opus -- --nocapture
```

The existing complete-source codec gate now supports both Opus and Vorbis. Its
fresh output also keeps prepared works for the desktop integration probe:

```sh
DDI_DANONI_AUDIO_GENERATION=/outside/git/saved-source-generation \
DDI_DANONI_AUDIO_OUTPUT=/outside/git/new-opus-media \
DDI_DANONI_AUDIO_CODEC=opus cargo test --locked \
  --manifest-path tools/danoni-dump/rust/Cargo.toml --test local_corpus \
  native_codec_receipts_prepare_the_complete_corpus -- --nocapture
DDI_DANONI_AUDIO_MEDIA=/outside/git/new-opus-media \
DDI_DANONI_FFMPEG=/path/to/ffmpeg cargo test --locked \
  --manifest-path tools/danoni-dump/rust/Cargo.toml --test local_corpus \
  native_codec_outputs_match_independent_ffmpeg_timing -- --nocapture
```

`DDI_DANONI_AUDIO_CODEC=ogg` with `--features vorbis` selects the retained path.
All third-party inputs, generated songs and review logs stay outside Git.
FFmpeg is an independent development oracle, never an import-time dependency.

## Measured results and main-side follow-ups

Local evidence: `~/Music/DanOni/reviews/opus-20261017/` (never committed).

- **93 audio files** freshly encoded from the saved 89-work source generation.
  Source-rate PCM and all existing chart diagnostics, ratings, artwork and preview
  starting times match the baseline. All 93 Rust round trips have exact dimensions
  and measured **zero-frame lag**. These historical comparison records include
  retired 7i charts; this does not restore that playable target.
- Independent FFmpeg/libopus decoding of **all 93 files** has exact output frame
  counts and measured **zero-frame lag**, with no song-specific settings.
- **28 synthetic FFmpeg cases** cover both channel counts, tiny streams, packet
  boundaries and six source rates. Decoder PCM differs by at most one PCM16 level.
  Absolute impulse checks also verify the filter-delay correction, including the
  odd-FFT 5512 Hz case. A joined fresh-preparation regression preserves source PCM,
  chart/scroll diagnostics and preview metadata when changing only the codec.
- Real desktop scanner, decoder, banner/preview lookup and silent fake-clock
  autoplay pass for **91 songs / 131 charts / 262 plays**, at 60 and 144 Hz, over
  layouts `danoni-5`, `danoni-7`, `danoni-9A`, `danoni-9B`. Two saved records contain
  retired 7i charts (`vh-canaria-7i`, mixed-mode `vh-433-needs`) and are omitted
  from that playback gate. Exact prepared song bytes are hard-linked under unique
  work/song folder names in `playable/` for ordinary folder scanning. This gate
  tests folder playback, not managed-store admission or new live URL extraction.
- The required 2 GiB RSS watchdog stops an all-songs-in-one-process playback audit.
  Four-song batches pass under the same watchdog. The underlying long-run memory
  behavior is not diagnosed here; a future desktop memory investigation can use
  the saved whole-run failure and per-batch reports.
- Desktop's decoder keeps one extra **tail** sample for five of the 91 songs
  (Brilliant Blue, 彼岸帰航, Cynthia, 君恋し, つるぺったん). All five match
  `ceil((frames / 48000.0) * 48000.0) == frames + 1`: about **0.0208 ms** at the end.
  The producer and FFmpeg lengths remain exact. Main-side follow-up:
  `platform/src/decode_all.rs::decode_opus` should preserve the integer final
  granule limit through `Kept`, instead of expressing that exact bound as an
  `f64` time and rounding it back in `limit`. Add a regression using a 7,513,235
  frame stream and a caller-supplied range. This changes only the end bound;
  it is not a chart-offset correction. Platform code is outside this branch's
  agreed implementation boundary.

`desktop-summary.json` records the 86 exact desktop counts and the five +1
counts explicitly. Independent timing receipts, the complete media corpus,
selection/omissions, per-batch desktop reports and final test logs stay local.
No GPU, sound output device, browser or fresh work downloads were needed.

Final checks: **285 default-feature SDK tests**, **98 minimal-feature SDK
tests**, 14 focused audio/preparation/Opus tests with the independent decoder,
and eight audio tests with both optional encoders enabled pass. Strict Clippy
passes with all features and with no default features. `just check`, including
REUSE **1136/1136**, formatting and diff checks pass on macOS arm64 / Rust 1.99.
The full source codec gate is additionally recorded as one 93-song local test.
Linux/Windows audio and production desktop UI validation remain follow-ups.
