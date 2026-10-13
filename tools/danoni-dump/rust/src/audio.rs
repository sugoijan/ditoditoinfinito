//! Bounded native decoding and measured decoder-origin alignment. No musical
//! offset estimation, subprocess, network or work-code execution.
use crate::{Error, Result, assets::MEDIA_LIMIT};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Cursor;
use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};
fn invalid(s: impl Into<String>) -> Error {
    Error::new("invalid-audio", s)
}
#[derive(Clone, Debug, PartialEq)]
pub struct Pcm16 {
    pub rate: u32,
    pub channels: usize,
    pub samples: Vec<i16>,
}
impl Pcm16 {
    pub fn frames(&self) -> usize {
        self.samples.len().checked_div(self.channels).unwrap_or(0)
    }
    pub fn validate(&self) -> Result<()> {
        if !(5512..=192000).contains(&self.rate)
            || !(1..=2).contains(&self.channels)
            || self.samples.is_empty()
            || !self.samples.len().is_multiple_of(self.channels)
            || self.frames() > self.rate as usize * 600
            || self.samples.len() > (MEDIA_LIMIT - 44) / 2
        {
            return Err(invalid("Need bounded mono/stereo PCM16 sample frames"));
        }
        Ok(())
    }
    pub fn wav(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let size = self.samples.len() * 2;
        let mut b = Vec::with_capacity(size + 44);
        b.extend(b"RIFF");
        b.extend((size as u32 + 36).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend((self.channels as u16).to_le_bytes());
        b.extend(self.rate.to_le_bytes());
        b.extend((self.rate * self.channels as u32 * 2).to_le_bytes());
        b.extend((self.channels as u16 * 2).to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend((size as u32).to_le_bytes());
        for v in &self.samples {
            b.extend(v.to_le_bytes());
        }
        Ok(b)
    }
    pub fn shifted(&self, frames: i64) -> Result<Self> {
        self.validate()?;
        if frames.unsigned_abs() > self.rate as u64 / 2 {
            return Err(invalid("Origin correction exceeds half a second"));
        }
        let samples = if frames >= 0 {
            let n = frames as usize * self.channels;
            if n >= self.samples.len() {
                return Err(invalid("Origin trim consumes all audio"));
            }
            self.samples[n..].to_vec()
        } else {
            let mut v = vec![0; (-frames) as usize * self.channels];
            v.extend(&self.samples);
            v
        };
        let p = Self {
            rate: self.rate,
            channels: self.channels,
            samples,
        };
        p.validate()?;
        Ok(p)
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecodeOptions {
    pub track_id: Option<u32>,
}
pub struct Decoded {
    pub pcm: Pcm16,
    pub report: Value,
}
fn quantize(v: f32) -> Result<i16> {
    if !v.is_finite() {
        return Err(invalid("Nonfinite decoded sample"));
    }
    Ok((v as f64 * 32768.0)
        .round_ties_even()
        .clamp(-32768.0, 32767.0) as i16)
}

fn mp3_inventory(bytes: &[u8], checkpoint: &mut impl FnMut() -> Result<()>) -> Result<Value> {
    let mut at = 0;
    let mut end = bytes.len();
    if bytes.starts_with(b"ID3") {
        if bytes.len() < 10
            || !(2..=4).contains(&bytes[3])
            || bytes[6..10].iter().any(|b| b & 128 != 0)
        {
            return Err(invalid("Malformed ID3 header"));
        }
        let size = bytes[6..10]
            .iter()
            .fold(0usize, |n, b| (n << 7) | *b as usize);
        at = 10
            + size
            + if bytes[3] == 4 && bytes[5] & 16 != 0 {
                10
            } else {
                0
            };
        if at > end {
            return Err(invalid("Truncated ID3 data"));
        }
    }
    if end >= 128 && &bytes[end - 128..end - 125] == b"TAG" {
        end -= 128;
    }
    let (mut frames, mut samples, mut tagged, mut format) = (0usize, 0u64, false, None);
    while at < end {
        if frames.is_multiple_of(256) {
            checkpoint()?;
        }
        let frame = crate::mpeg::frame(&bytes[at..end])?;
        let (h, version, rate, channels, size) = (
            frame.header,
            frame.version,
            frame.rate,
            frame.channels,
            frame.bytes,
        );
        if format.is_some_and(|v| v != (rate, channels)) {
            return Err(invalid("MP3 rate/channels change within stream"));
        }
        format = Some((rate, channels));
        let side = if version == 3 {
            if channels == 1 { 17 } else { 32 }
        } else if channels == 1 {
            9
        } else {
            17
        };
        let info = at + 4 + if h >> 16 & 1 == 0 { 2 } else { 0 } + side;
        let nominal = at + 4 + side;
        let info_tag = |offset: usize| {
            offset + 4 <= at + size && matches!(&bytes[offset..offset + 4], b"Xing" | b"Info")
        };
        let tag = info_tag(info)
            || info_tag(nominal)
            || (size >= 40 && &bytes[at + 36..at + 40] == b"VBRI");
        if tag {
            if frames != 0 {
                return Err(invalid("Unexpected metadata frame inside MP3 payload"));
            }
            tagged = true;
        } else {
            samples += frame.samples as u64;
        }
        frames += 1;
        if frames > 1_000_000 {
            return Err(invalid("MP3 frame budget exceeded"));
        }
        at += size;
    }
    let (rate, channels) = format.ok_or_else(|| invalid("No complete MP3 frames"))?;
    Ok(
        json!({"mpegFrames":frames,"payloadSampleFrames":samples,"metadataFrame":tagged,"sampleRate":rate,"channels":channels,"lengthSource":"counted complete MPEG Layer III frames; metadata duration is separate"}),
    )
}
/// Strict packet decoding: errors/format changes are failures, never skipped
/// packets that silently shorten a chart's audio timeline.
pub fn decode(
    bytes: &[u8],
    options: &DecodeOptions,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Decoded> {
    if bytes.is_empty() || bytes.len() > MEDIA_LIMIT {
        return Err(invalid("Encoded input exceeds bounds"));
    }
    checkpoint()?;
    let kind = if bytes.starts_with(b"RIFF") {
        if bytes.len() < 12
            || &bytes[8..12] != b"WAVE"
            || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8 != bytes.len()
        {
            return Err(invalid("Incomplete WAV container"));
        }
        "wav"
    } else if bytes.starts_with(b"OggS") {
        "ogg"
    } else if bytes.starts_with(b"ID3")
        || (bytes.len() >= 2 && bytes[0] == 255 && bytes[1] & 224 == 224)
    {
        "mp3"
    } else {
        return Err(Error::new(
            "unsupported-audio",
            "Only WAV, MP3 and Ogg Vorbis are supported",
        ));
    };
    let mp3 = if kind == "mp3" {
        Some(mp3_inventory(bytes, checkpoint)?)
    } else {
        None
    };
    let stream = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(kind);
    let mut reader = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| invalid(e.to_string()))?;
    let tracks = reader
        .tracks()
        .iter()
        .filter(|t| t.codec_params.as_ref().and_then(|p| p.audio()).is_some())
        .collect::<Vec<_>>();
    let track = match options.track_id {
        Some(id) => tracks
            .iter()
            .find(|t| t.id == id)
            .copied()
            .ok_or_else(|| invalid("Selected audio track is absent"))?,
        None => {
            if tracks.len() == 1 {
                tracks[0]
            } else {
                return Err(invalid("Select exactly one audio track"));
            }
        }
    };
    let params = track.codec_params.as_ref().unwrap().audio().unwrap();
    let rate = params
        .sample_rate
        .ok_or_else(|| invalid("Missing sample rate"))?;
    let channels = params
        .channels
        .as_ref()
        .ok_or_else(|| invalid("Missing channels"))?
        .count();
    if !(5512..=192000).contains(&rate) || !(1..=2).contains(&channels) {
        return Err(invalid("Unsupported rate/channel dimensions"));
    }
    let id = track.id;
    let expected = track.num_frames;
    let delay = track.delay;
    let padding = track.padding;
    let codec = format!("{:?}", params.codec);
    let gapless = mp3.as_ref().is_none_or(|m| m["metadataFrame"] == true);
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default().gapless(gapless))
        .map_err(|e| invalid(e.to_string()))?;
    let mut pcm = Pcm16 {
        rate,
        channels,
        samples: Vec::new(),
    };
    let mut packets = 0usize;
    while let Some(packet) = reader
        .next_packet()
        .map_err(|e| invalid(format!("Container failed: {e}")))?
    {
        checkpoint()?;
        packets += 1;
        if packets > 1_000_000 {
            return Err(invalid("Packet limit exceeded"));
        }
        if packet.track_id != id {
            continue;
        }
        let buffer = decoder
            .decode(&packet)
            .map_err(|e| invalid(format!("Audio packet failed: {e}")))?;
        if buffer.num_planes() != channels || buffer.spec().rate() != rate {
            return Err(invalid("Channel dimensions changed"));
        }
        let frames = buffer.frames();
        if pcm
            .frames()
            .checked_add(frames)
            .is_none_or(|n| n > rate as usize * 600 || n * channels > MEDIA_LIMIT / 2)
        {
            return Err(invalid("Decoded PCM exceeds bounds"));
        }
        let mut values = vec![vec![0f32; frames]; channels];
        let mut slices = values
            .iter_mut()
            .map(|v| v.as_mut_slice())
            .collect::<Vec<_>>();
        buffer.copy_to_slice_planar(&mut slices);
        for frame in 0..frames {
            for ch in &values {
                pcm.samples.push(quantize(ch[frame])?);
            }
        }
    }
    pcm.validate()?;
    let exact_expected = if let Some(m) = &mp3 {
        let count = m["payloadSampleFrames"].as_u64().unwrap();
        if gapless {
            Some(
                count
                    .saturating_sub(delay.unwrap_or(0) as u64)
                    .min(expected.unwrap_or(u64::MAX)),
            )
        } else {
            Some(count)
        }
    } else {
        expected
    };
    if exact_expected.is_some_and(|n| n != pcm.frames() as u64) {
        return Err(invalid(format!(
            "Decoded frame count {} differs from exact expected {:?}",
            pcm.frames(),
            exact_expected
        )));
    }
    let report = json!({"schema":"ddi-danoni-audio-decode/1","decoder":"symphonia 0.6.1","container":kind,"codec":codec,"trackId":id,"gapless":gapless,"mp3Inventory":mp3,"exactExpectedFrames":exact_expected,"reportedFrames":expected,"encoderDelayFrames":delay,"encoderPaddingFrames":padding,"sampleRate":rate,"channels":channels,"sampleFrames":pcm.frames(),"packets":packets,"skippedPackets":0,"pcmQuantization":"signed PCM16; round to nearest ties-even; saturation"});
    Ok(Decoded { pcm, report })
}
/// FFT bandlimited resampling, with startup delay removed and ceil(N*out/in)
/// frames retained. Mono becomes two identical channels only when requested.
pub fn resample(
    pcm: &Pcm16,
    rate: u32,
    stereo: bool,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<(Pcm16, Value)> {
    use audioadapter_buffers::direct::InterleavedSlice;
    use rubato::{Fft, FixedSync, Indexing, Resampler};
    pcm.validate()?;
    if !(5512..=192000).contains(&rate) {
        return Err(invalid("Unsupported resample rate"));
    }
    checkpoint()?;
    let channels = if stereo { 2 } else { pcm.channels };
    let mut delay = 0;
    let frames = (pcm.frames() as u64 * rate as u64).div_ceil(pcm.rate as u64) as usize;
    if frames > rate as usize * 600 || frames * channels > MEDIA_LIMIT / 2 {
        return Err(invalid("Resampling exceeds output bounds"));
    }
    let samples = if rate == pcm.rate {
        pcm.samples.clone()
    } else {
        let input = pcm
            .samples
            .iter()
            .map(|v| *v as f32 / 32768.0)
            .collect::<Vec<_>>();
        let adapter = InterleavedSlice::new(&input, pcm.channels, pcm.frames())
            .map_err(|e| invalid(e.to_string()))?;
        let mut r = Fft::<f32>::new(
            pcm.rate as usize,
            rate as usize,
            1024,
            pcm.channels,
            FixedSync::Input,
        )
        .map_err(|e| invalid(e.to_string()))?;
        delay = r.output_delay();
        let mut out = Vec::with_capacity(frames * pcm.channels);
        let (mut at, mut skipped) = (0, 0);
        while out.len() < frames * pcm.channels {
            checkpoint()?;
            let available = (pcm.frames() - at).min(r.input_frames_next());
            let indexing = Indexing::new().input_offset(at).partial_len(available);
            let block = r
                .process(&adapter, Some(&indexing))
                .map_err(|e| invalid(e.to_string()))?
                .take_data();
            at += available;
            for frame in block.chunks_exact(pcm.channels) {
                if skipped < delay {
                    skipped += 1;
                    continue;
                }
                if out.len() == frames * pcm.channels {
                    break;
                }
                for v in frame {
                    out.push(quantize(*v)?);
                }
            }
        }
        out
    };
    let samples = if channels == 2 && pcm.channels == 1 {
        samples.into_iter().flat_map(|v| [v, v]).collect()
    } else {
        samples
    };
    let out = Pcm16 {
        rate,
        channels,
        samples,
    };
    out.validate()?;
    Ok((
        out,
        json!({"schema":"ddi-danoni-audio-resample/1","implementation":"rubato 5.0.1 FFT / BlackmanHarris2 / 1024 input frames","sourceRate":pcm.rate,"sourceFrames":pcm.frames(),"sampleRate":rate,"sampleFrames":frames,"channels":channels,"removedOutputDelayFrames":delay,"lengthPolicy":"ceil(sourceFrames * outputRate / sourceRate)","monoToStereo":"duplicate channels; no averaging"}),
    ))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginWindow {
    pub part: usize,
    pub start_seconds: f64,
    pub lag_samples: i64,
    pub correlation: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginMatch {
    pub schema: String,
    pub rate: u32,
    pub lag_samples: i64,
    pub maximum_spread_samples: i64,
    pub windows: Vec<OriginWindow>,
    pub rejected_windows: Vec<OriginWindow>,
}
/// Generic multi-window per-channel normalized FFT correlation, matching the
/// established evidence thresholds without work names or beat assumptions.
pub fn origin_match(
    native: &Pcm16,
    decoded: &Pcm16,
    start_frame: usize,
    end_frame: Option<usize>,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<OriginMatch> {
    use rustfft::{FftPlanner, num_complex::Complex};
    native.validate()?;
    decoded.validate()?;
    if native.rate != decoded.rate || native.channels != decoded.channels {
        return Err(invalid("Correlation dimensions differ"));
    }
    let end = end_frame.unwrap_or(native.frames());
    if start_frame >= end || end > native.frames() {
        return Err(invalid("Invalid native stream interval"));
    }
    let x = &native.samples[start_frame * native.channels..end * native.channels];
    let channels = native.channels;
    let count = (x.len() / channels).min(decoded.frames());
    let rate = native.rate as usize;
    if count < rate / 2 {
        return Err(Error::new(
            "insufficient-audio-origin",
            "Need half a second of observed PCM",
        ));
    }
    let length = (rate / 3).min(count / 3);
    let margin = rate / 4;
    if count < length + margin {
        return Err(Error::new(
            "insufficient-audio-origin",
            "Capture too short for search windows",
        ));
    }
    let mut candidates = Vec::new();
    for at in (margin..=count - length).step_by((length / 2).max(1)) {
        checkpoint()?;
        let energy = (0..length)
            .step_by(32)
            .flat_map(|i| (0..channels).map(move |ch| x[(at + i) * channels + ch] as f64))
            .map(|s| s * s)
            .sum::<f64>();
        candidates.push((at, energy));
    }
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut selected = Vec::<usize>::new();
    for (at, _) in candidates {
        if selected.iter().all(|v| at.abs_diff(*v) >= length / 2) {
            selected.push(at);
        }
        if selected.len() == 8 {
            break;
        }
    }
    selected.sort_unstable();
    let (mut windows, mut rejected) = (Vec::new(), Vec::new());
    let mut planner = FftPlanner::<f64>::new();
    for (part, at) in selected.into_iter().enumerate() {
        checkpoint()?;
        let ref_start = at - margin;
        let ref_end = (at + length + margin).min(decoded.frames());
        let reference = &decoded.samples[ref_start * channels..ref_end * channels];
        let rn = ref_end - ref_start;
        let positions = rn - length + 1;
        let size = (rn + length - 1).next_power_of_two();
        let fft = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let (mut xy, mut yy) = (vec![0f64; positions], vec![0f64; positions]);
        let mut xx = 0f64;
        for ch in 0..channels {
            checkpoint()?;
            let mean = (0..length)
                .map(|i| x[(at + i) * channels + ch] as f64)
                .sum::<f64>()
                / length as f64;
            let window = (0..length)
                .map(|i| x[(at + i) * channels + ch] as f64 - mean)
                .collect::<Vec<_>>();
            xx += window.iter().map(|v| v * v).sum::<f64>();
            let (mut a, mut b) = (
                vec![Complex::new(0.0, 0.0); size],
                vec![Complex::new(0.0, 0.0); size],
            );
            let (mut sums, mut squares) = (vec![0f64; rn + 1], vec![0f64; rn + 1]);
            for i in 0..rn {
                let v = reference[i * channels + ch] as f64;
                a[i].re = v;
                sums[i + 1] = sums[i] + v;
                squares[i + 1] = squares[i] + v * v;
            }
            for i in 0..length {
                b[i].re = window[length - 1 - i];
            }
            fft.process(&mut a);
            fft.process(&mut b);
            for (i, v) in a.iter_mut().enumerate() {
                *v *= b[i];
            }
            inverse.process(&mut a);
            for p in 0..positions {
                xy[p] += a[p + length - 1].re / size as f64;
                let sum = sums[p + length] - sums[p];
                yy[p] += squares[p + length] - squares[p] - sum * sum / length as f64;
            }
        }
        if (xx / (length * channels) as f64).sqrt() < 4.0 {
            continue;
        }
        let mut best = (-1f64, 0usize);
        for p in 0..positions {
            let den = (yy[p].max(0.0) * xx).sqrt();
            let score = if den > 0.0 { xy[p] / den } else { -1.0 };
            if score > best.0 {
                best = (score, p);
            }
        }
        let row = OriginWindow {
            part,
            start_seconds: at as f64 / rate as f64,
            lag_samples: best.1 as i64 - margin as i64,
            correlation: best.0.clamp(-1.0, 1.0),
        };
        if row.correlation >= 0.97 {
            windows.push(row);
        } else {
            rejected.push(row);
        }
    }
    if windows.len() < 2 {
        return Err(Error::new(
            "insufficient-audio-origin",
            format!("Need two correlated windows; found {}", windows.len()),
        ));
    }
    let min = windows.iter().map(|w| w.lag_samples).min().unwrap();
    let max = windows.iter().map(|w| w.lag_samples).max().unwrap();
    if max - min > 2 {
        return Err(Error::new(
            "inconsistent-audio-origin",
            "Correlated lags differ by more than two samples",
        ));
    }
    let lag = (windows.iter().map(|w| w.lag_samples as f64).sum::<f64>() / windows.len() as f64)
        .round_ties_even() as i64;
    Ok(OriginMatch {
        schema: "ddi-danoni-audio-origin-match/1".into(),
        rate: native.rate,
        lag_samples: lag,
        maximum_spread_samples: max - min,
        windows,
        rejected_windows: rejected,
    })
}

#[cfg(feature = "vorbis")]
pub fn encode_ogg(
    pcm: &Pcm16,
    quality: f32,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<Vec<u8>> {
    use std::{
        io::{self, Write},
        num::{NonZeroU8, NonZeroU32},
    };
    use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};
    pcm.validate()?;
    if !(8000..=96000).contains(&pcm.rate)
        || !quality.is_finite()
        || !(0.0..=1.0).contains(&quality)
    {
        return Err(invalid("Unsupported Vorbis rate/quality"));
    }
    struct Sink(Vec<u8>);
    impl Write for Sink {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self
                .0
                .len()
                .checked_add(b.len())
                .is_none_or(|n| n > MEDIA_LIMIT)
            {
                return Err(io::Error::other("Encoded output exceeds limit"));
            }
            self.0.extend(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut builder = VorbisEncoderBuilder::new_with_serial(
        NonZeroU32::new(pcm.rate).unwrap(),
        NonZeroU8::new(pcm.channels as u8).unwrap(),
        Sink(Vec::new()),
        0x444449,
    );
    builder.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
        target_quality: quality,
    });
    builder
        .comment_tag("ENCODER", "DDI native Vorbis / vorbis_rs 0.5.6")
        .map_err(|e| invalid(e.to_string()))?;
    let mut encoder = builder.build().map_err(|e| invalid(e.to_string()))?;
    for block in pcm.samples.chunks(1024 * pcm.channels) {
        checkpoint()?;
        let channels = (0..pcm.channels)
            .map(|ch| {
                block
                    .chunks_exact(pcm.channels)
                    .map(|f| f[ch] as f32 / 32768.0)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        encoder
            .encode_audio_block(&channels)
            .map_err(|e| invalid(e.to_string()))?;
    }
    checkpoint()?;
    Ok(encoder.finish().map_err(|e| invalid(e.to_string()))?.0)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaRequest {
    pub manifest: crate::assets::Binding,
    pub audio: crate::assets::Binding,
    pub codec: crate::work::Codec,
    #[serde(default)]
    pub decode: DecodeOptions,
    #[serde(default = "default_quality")]
    pub quality: f32,
}
fn default_quality() -> f32 {
    0.5
}
pub struct PreparedMedia {
    pub files: Vec<crate::assets::Artifact>,
    pub manifest: Value,
}
/// Codec stage for an already normalized audio source. SWF-origin alignment is
/// an earlier producer: it must use origin_match against captured runtime PCM.
pub fn prepare_media(
    request: &MediaRequest,
    source: &impl crate::assets::AssetSource,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<PreparedMedia> {
    use crate::assets::{self, Artifact, Inputs, JSON_LIMIT, sha256};
    use std::sync::Arc;
    if !request.quality.is_finite() || !(0.0..=1.0).contains(&request.quality) {
        return Err(invalid("Invalid encoder quality"));
    }
    let mut inputs = Inputs::new(source);
    let manifest = inputs.checked(&request.manifest, JSON_LIMIT)?;
    let doc: Value = assets::json(&manifest)?;
    let schema = doc["schema"].as_str().unwrap_or("");
    if ![
        "ddi-normalized-work/experimental-1",
        "ddi-legacy-export/experimental-1",
        "ddi-synthetic-export/experimental-1",
    ]
    .contains(&schema)
    {
        return Err(invalid("Need normalized source manifest"));
    }
    let expected = std::iter::once(&doc["music"])
        .chain(
            doc["charts"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| &c["music"]),
        )
        .any(|m| m["sha256"] == request.audio.sha256);
    if !expected {
        return Err(invalid("Audio hash is not bound by source music"));
    }
    let bytes = inputs.checked(&request.audio, MEDIA_LIMIT)?;
    let decoded = decode(&bytes, &request.decode, checkpoint)?;
    let pcm = decoded.pcm.wav()?;
    let mut files = vec![
        Artifact {
            path: "source.bin".into(),
            bytes,
        },
        Artifact {
            path: "source-manifest.json".into(),
            bytes: manifest,
        },
    ];
    let (music, codec, validation, producer) = match request.codec {
        crate::work::Codec::Wav => (
            pcm.clone(),
            "wav",
            json!({"pcmPlaybackIdentical":true}),
            json!({"name":"ddi-danoni-export","audioPolicy":"native-media/1","decoder":"symphonia 0.6.1"}),
        ),
        crate::work::Codec::Ogg => {
            #[cfg(feature = "vorbis")]
            {
                let ogg = encode_ogg(&decoded.pcm, request.quality, checkpoint)?;
                let check = decode(&ogg, &DecodeOptions::default(), checkpoint)?;
                if check.pcm.rate != decoded.pcm.rate
                    || check.pcm.channels != decoded.pcm.channels
                    || check.pcm.frames() != decoded.pcm.frames()
                {
                    return Err(invalid("Vorbis round-trip dimensions differ"));
                }
                // Count/granule agreement is mandatory. Lossy signal matching is
                // evidence with explicit scope, not a fabricated origin proof.
                let stop = (decoded.pcm.rate as usize * 12).min(decoded.pcm.frames());
                let correlation = origin_match(&decoded.pcm, &check.pcm, 0, Some(stop), checkpoint);
                let validation = match correlation {
                    Ok(m) => {
                        if m.lag_samples.abs() > 1 {
                            return Err(invalid("Vorbis alignment differs by more than one frame"));
                        }
                        json!({"rateChannelsFramesMatched":true,"originMatch":m,"signalAlignment":"measured"})
                    }
                    Err(e)
                        if ["insufficient-audio-origin", "inconsistent-audio-origin"]
                            .contains(&e.code) =>
                    {
                        json!({"rateChannelsFramesMatched":true,"signalAlignment":"unproved","originDiagnostic":e})
                    }
                    Err(e) => return Err(e),
                };
                (
                    ogg,
                    "ogg",
                    validation,
                    json!({"name":"ddi-danoni-export","audioPolicy":"native-media/1","decoder":"symphonia 0.6.1","encoder":"vorbis_rs 0.5.6 / aoTuV+Lancer bundled libvorbis","quality":request.quality,"oggStreamSerial":0x444449}),
                )
            }
            #[cfg(not(feature = "vorbis"))]
            {
                return Err(Error::new(
                    "encoder-unavailable",
                    "Enable the native vorbis feature",
                ));
            }
        }
    };
    let pcm_hash = sha256(&pcm);
    let playback_hash = sha256(&music);
    files.push(Artifact {
        path: "pcm.wav".into(),
        bytes: Arc::new(pcm),
    });
    files.push(Artifact {
        path: format!("music.{codec}"),
        bytes: Arc::new(music),
    });
    let report = json!({"schema":"ddi-danoni-prepared-media/1","sourceManifestSha256":request.manifest.sha256,"sourceAudioSha256":request.audio.sha256,"playbackSha256":playback_hash,"pcmSha256":pcm_hash,"sampleRate":decoded.pcm.rate,"channels":decoded.pcm.channels,"sampleFrames":decoded.pcm.frames(),"codec":codec,"producer":producer,"decode":decoded.report,"validation":validation,"sourceOrigin":"already normalized source; no new SWF origin proof or musical offset","request":request,"artifacts":files.iter().map(|f|json!({"path":f.path,"sha256":sha256(&f.bytes),"bytes":f.bytes.len()})).collect::<Vec<_>>()});
    Ok(PreparedMedia {
        files,
        manifest: report,
    })
}
