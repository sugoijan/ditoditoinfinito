//! Ogg Opus's packets through `rusty-opus` (pure Rust, BSD-3-Clause,
//! within float rounding of libopus: `docs/plans/desktop.md` 7.0), as
//! RFC 7845 has them played: the `OpusHead`'s pre-skip dropped, its
//! output gain applied. The stream's end (the last granule) is the
//! caller's ([`crate::decode::Decoder`], which both shells' Opus goes
//! through: the web's fallback and desktop's `decode_all`).

/// Opus always decodes at 48 kHz.
pub const RATE: u32 = 48_000;
/// The longest Opus packet: 120 ms at 48 kHz.
const MAX_FRAMES: usize = 5_760;

/// The fields of an `OpusHead` (RFC 7845, section 5.1) the decoder uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Head {
    pub channels: usize,
    pub pre_skip: usize,
    /// The output gain, as a factor.
    pub gain: f32,
    pub family: u8,
}

/// The `OpusHead` packet's fields (Symphonia's Ogg reader keeps the packet
/// as the track's extra data).
pub(crate) fn head(head: &[u8]) -> Option<Head> {
    if head.len() < 19 || &head[..8] != b"OpusHead" {
        return None;
    }
    let q8 = i16::from_le_bytes([head[16], head[17]]);
    Some(Head {
        channels: usize::from(head[9]),
        pre_skip: usize::from(u16::from_le_bytes([head[10], head[11]])),
        gain: 10f32.powf(f32::from(q8) / (20.0 * 256.0)),
        family: head[18],
    })
}

/// One stream's decoder: packets in, frames out with the pre-skip dropped
/// and the gain applied.
pub(crate) struct Stream {
    decoder: rusty_opus::OpusDecoder,
    channels: usize,
    /// Frames of the pre-skip still to drop.
    pre_skip: usize,
    gain: f32,
    /// One packet's frames, interleaved.
    buf: Vec<f32>,
}

impl Stream {
    /// A decoder for a stream of mapping family 0 (mono or stereo, which
    /// every simfile's music is); `None` for others.
    pub fn new(head: Head) -> Option<Stream> {
        if head.family != 0 || !(1..=2).contains(&head.channels) {
            return None;
        }
        Some(Stream {
            decoder: rusty_opus::OpusDecoder::new(RATE as i32, head.channels).ok()?,
            channels: head.channels,
            pre_skip: head.pre_skip,
            gain: head.gain,
            buf: vec![0.0; MAX_FRAMES * head.channels],
        })
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The frames of silence `frames` lost before a packet come to (a page
    /// the reader dropped), what of them the pre-skip covers left out.
    pub fn gap(&mut self, frames: usize) -> usize {
        let drop = self.pre_skip.min(frames);
        self.pre_skip -= drop;
        frames - drop
    }

    /// One packet's frames, interleaved, after the pre-skip; and whether
    /// the packet failed (then concealed, `dur` frames long, by libopus's
    /// PLC, as a browser would a lost packet).
    pub fn decode(&mut self, data: &[u8], dur: u64) -> (&[f32], bool) {
        let (n, failed) = match self.decoder.decode(data, MAX_FRAMES, &mut self.buf) {
            Ok(n) => (n, false),
            Err(_) => {
                let n = self
                    .decoder
                    .decode(&[], dur.min(MAX_FRAMES as u64) as usize, &mut self.buf)
                    .unwrap_or(0);
                (n, true)
            }
        };
        // The pre-skip is not the song's.
        let drop = self.pre_skip.min(n);
        self.pre_skip -= drop;
        let frames = &mut self.buf[drop * self.channels..n * self.channels];
        if self.gain != 1.0 {
            for s in frames.iter_mut() {
                *s *= self.gain;
            }
        }
        (frames, failed)
    }
}

/// An Ogg Opus file decoded whole, as a WAV of 32-bit floats at 48 kHz
/// (and the packets that failed): what the web's Opus worker hands the
/// page, whose `decodeAudioData` reads it at the context's rate with the
/// browser's own resampler, as it would have the Opus file.
pub fn decode_wav(bytes: &[u8]) -> Result<(Vec<u8>, usize), String> {
    let mut d = crate::decode::Decoder::ogg_opus(bytes).map_err(|e| e.to_string())?;
    while !d.step(64).map_err(|e| e.to_string())? {}
    let audio = d.finish().map_err(|e| e.to_string())?;
    Ok((wav(&audio)?, audio.skipped_packets))
}

/// `audio` as a WAV file of 32-bit floats (format 3), interleaved.
pub fn wav(audio: &crate::decode::DecodedAudio) -> Result<Vec<u8>, String> {
    let channels = audio.channels.len();
    let frames = audio.frames();
    let data = u32::try_from(frames * channels * 4)
        .ok()
        .filter(|&n| n <= u32::MAX - 36)
        .ok_or("the audio is too long for a WAV file")?;
    let block = (channels * 4) as u16;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&(channels as u16).to_le_bytes());
    out.extend_from_slice(&audio.sample_rate.to_le_bytes());
    out.extend_from_slice(&(audio.sample_rate * u32::from(block)).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for i in 0..frames {
        for c in &audio.channels {
            out.extend_from_slice(&c[i].to_le_bytes());
        }
    }
    Ok(out)
}
