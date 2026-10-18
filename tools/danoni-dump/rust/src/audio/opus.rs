//! RFC 7845 mapping family 0. CELT-only music encoding fixes the algorithmic
//! delay at its 120-frame overlap; independent decoder tests bind this policy.
use super::*;
use ogg::{PacketReader, PacketWriter, writing::PacketWriteEndInfo};
use rusty_opus::{Application, OpusDecoder, OpusEncoder};

pub(super) const RATE: u32 = 48000;
const FRAME: usize = 960;
const PRE_SKIP: usize = 120;
const SERIAL: u32 = 0x444449;

pub fn encode(
    pcm: &Pcm16,
    quality: f32,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<(Vec<u8>, Value)> {
    pcm.validate()?;
    if pcm.rate != RATE || !quality.is_finite() || !(0.0..=1.0).contains(&quality) {
        return Err(invalid(
            "Opus encoding needs 48 kHz PCM and quality in 0..=1",
        ));
    }
    checkpoint()?;
    let mut encoder = OpusEncoder::new(RATE as i32, pcm.channels, Application::RestrictedLowDelay)
        .map_err(|e| invalid(e.to_string()))?;
    // Explicitly music-oriented CELT, with one stable delay throughout a song.
    let bitrate = ((64000.0 + quality * 64000.0) as i32) * pcm.channels as i32;
    encoder.bitrate_bps = bitrate;
    encoder.lsb_depth = 16;
    let mut writer = PacketWriter::new(Sink(Vec::new()));
    let mut head = b"OpusHead".to_vec();
    head.extend([1, pcm.channels as u8]);
    head.extend((PRE_SKIP as u16).to_le_bytes());
    head.extend(pcm.rate.to_le_bytes());
    head.extend([0, 0, 0]); // zero gain, family 0
    writer
        .write_packet(head, SERIAL, PacketWriteEndInfo::EndPage, 0)
        .map_err(|e| invalid(e.to_string()))?;
    let vendor = b"DDI native Opus / rusty-opus 1.0.1";
    let mut tags = b"OpusTags".to_vec();
    tags.extend((vendor.len() as u32).to_le_bytes());
    tags.extend(vendor);
    tags.extend(0u32.to_le_bytes());
    writer
        .write_packet(tags, SERIAL, PacketWriteEndInfo::EndPage, 0)
        .map_err(|e| invalid(e.to_string()))?;
    let end = pcm.frames() + PRE_SKIP;
    let packets = end.div_ceil(FRAME);
    let mut input = vec![0f32; FRAME * pcm.channels];
    let mut output = vec![0u8; 4000];
    for packet in 0..packets {
        checkpoint()?;
        input.fill(0.0);
        let start = packet * FRAME * pcm.channels;
        if start < pcm.samples.len() {
            for (out, sample) in input.iter_mut().zip(&pcm.samples[start..]) {
                *out = *sample as f32 / 32768.0;
            }
        }
        let n = encoder
            .encode(&input, FRAME, &mut output)
            .map_err(|e| invalid(e.to_string()))?;
        // The crate exposes experimental environment overrides. Do not emit
        // a stream with a different latency if an override changes its mode.
        if n == 0 || output[0] & 0x80 == 0 {
            return Err(invalid(
                "Opus encoder departed from the CELT-only timing policy",
            ));
        }
        let last = packet + 1 == packets;
        let info = if last {
            PacketWriteEndInfo::EndStream
        } else if (packet + 1).is_multiple_of(50) {
            PacketWriteEndInfo::EndPage
        } else {
            PacketWriteEndInfo::NormalPacket
        };
        let granule = if last { end } else { (packet + 1) * FRAME };
        writer
            .write_packet(output[..n].to_vec(), SERIAL, info, granule as u64)
            .map_err(|e| invalid(e.to_string()))?;
    }
    checkpoint()?;
    Ok((
        writer.into_inner().0,
        json!({"encoder":"rusty-opus 1.0.1","container":"ogg 0.9.2","application":"restricted-low-delay","mode":"CELT","bitrateBps":bitrate,"quality":quality,"frameSamples":FRAME,"preSkipFrames":PRE_SKIP,"lastGranule":end,"endTrimFrames":packets*FRAME-end,"oggStreamSerial":SERIAL}),
    ))
}

/// Reject missing pages/EOS and trailing data before the Ogg reader's tolerant
/// resynchronization can hide them. The library separately verifies page CRCs.
fn inventory(bytes: &[u8], checkpoint: &mut impl FnMut() -> Result<()>) -> Result<u32> {
    let mut at = 0usize;
    let mut serial = None;
    let mut sequence = 0u32;
    let mut ended = false;
    while at < bytes.len() {
        checkpoint()?;
        let page = &bytes[at..];
        if ended || page.len() < 27 || &page[..5] != b"OggS\0" || page[5] & !7 != 0 {
            return Err(invalid("Incomplete or unexpected Ogg page"));
        }
        let id = u32::from_le_bytes(page[14..18].try_into().unwrap());
        let seq = u32::from_le_bytes(page[18..22].try_into().unwrap());
        if serial.is_some_and(|s| s != id)
            || seq != sequence
            || (sequence == 0) != (page[5] & 2 != 0)
        {
            return Err(invalid("Opus needs one complete, sequential Ogg stream"));
        }
        serial = Some(id);
        sequence += 1;
        let table_end = 27 + page[26] as usize;
        let table = page
            .get(27..table_end)
            .ok_or_else(|| invalid("Truncated Ogg lacing"))?;
        let size = table_end + table.iter().map(|v| *v as usize).sum::<usize>();
        if size > page.len() {
            return Err(invalid("Truncated Ogg body"));
        }
        ended = page[5] & 4 != 0;
        at += size;
    }
    if !ended {
        return Err(invalid("Opus stream has no end page"));
    }
    serial.ok_or_else(|| invalid("Empty Opus stream"))
}

pub(super) fn decode(bytes: &[u8], checkpoint: &mut impl FnMut() -> Result<()>) -> Result<Decoded> {
    let serial = inventory(bytes, checkpoint)?;
    let mut reader = PacketReader::new(Cursor::new(bytes));
    let mut next = || {
        reader
            .read_packet_expected()
            .map_err(|e| invalid(e.to_string()))
    };
    let head = next()?;
    let h = &head.data;
    if h.len() != 19
        || &h[..8] != b"OpusHead"
        || h[8] != 1
        || !(1..=2).contains(&h[9])
        || h[18] != 0
        || !head.first_in_stream()
        || !head.last_in_page()
        || head.absgp_page() != 0
        || head.last_in_stream()
    {
        return Err(invalid(
            "Unsupported Opus header; need mono/stereo family 0",
        ));
    }
    let channels = h[9] as usize;
    let skip = u16::from_le_bytes(h[10..12].try_into().unwrap()) as usize;
    let gain = 10f32.powf(i16::from_le_bytes(h[16..18].try_into().unwrap()) as f32 / 5120.0);
    let tags = next()?;
    if !tags.data.starts_with(b"OpusTags")
        || tags.data.len() < 16
        || !tags.last_in_page()
        || tags.absgp_page() != 0
        || tags.last_in_stream()
    {
        return Err(invalid("Invalid Opus comment header"));
    }
    let mut decoder =
        OpusDecoder::new(RATE as i32, channels).map_err(|e| invalid(e.to_string()))?;
    let mut pcm = Pcm16 {
        rate: RATE,
        channels,
        samples: Vec::new(),
    };
    let mut buffer = vec![0f32; 5760 * channels];
    let mut frames = 0usize;
    let mut packets = 0usize;
    let mut previous_granule = 0usize;
    let end;
    loop {
        checkpoint()?;
        let packet = next()?;
        packets += 1;
        let n = decoder
            .decode(&packet.data, 5760, &mut buffer)
            .map_err(|e| invalid(e.to_string()))?;
        if n == 0
            || frames + n > RATE as usize * 600 + skip + 5760
            || (frames + n) * channels > MEDIA_LIMIT / 2
        {
            return Err(invalid("Opus PCM exceeds bounds"));
        }
        let drop = skip.saturating_sub(frames).min(n);
        for &v in &buffer[drop * channels..n * channels] {
            pcm.samples.push(quantize(v * gain)?);
        }
        frames += n;
        if packet.last_in_page() {
            let granule = usize::try_from(packet.absgp_page())
                .map_err(|_| invalid("Invalid Opus granule"))?;
            if packet.last_in_stream() {
                if granule < skip
                    || granule <= previous_granule
                    || granule > frames
                    || frames - granule >= n
                {
                    return Err(invalid("Invalid Opus final granule"));
                }
                end = granule - skip;
                break;
            }
            if granule != frames {
                return Err(invalid("Opus granules disagree with decoded frames"));
            }
            previous_granule = granule;
        }
    }
    if reader
        .read_packet()
        .map_err(|e| invalid(e.to_string()))?
        .is_some()
    {
        return Err(invalid("Unexpected packet after Opus end"));
    }
    pcm.samples.truncate(end * channels);
    pcm.validate()?;
    Ok(Decoded {
        report: json!({"schema":"ddi-danoni-audio-decode/1","decoder":"rusty-opus 1.0.1","container":"ogg","codec":"opus","trackId":serial,"gapless":true,"encoderDelayFrames":skip,"sampleRate":RATE,"channels":channels,"sampleFrames":pcm.frames(),"packets":packets,"skippedPackets":0}),
        pcm,
    })
}
