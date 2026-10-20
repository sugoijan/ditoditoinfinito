//! Compressed input for the native video decoder. AVI uses the checked
//! range reader; MP4/Matroska use a private sandboxed demuxer. No movie-wide
//! packet/decoded-frame index is built. Seeks mirror the module's key-packet
//! backoff, with bounded retries and a reopen from the start as the last step.
use super::{
    MovieFile, avi,
    codec::Format,
    h264_format, hevc,
    packets::{Packet, PacketReader},
    vp9,
};
use ddi_library::h264::{self, Framing};
use ddi_platform::video::{VideoCodec, VideoContainer, VideoInfo};
use std::{
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::Duration,
};

pub(super) struct Compressed {
    pub bytes: Vec<u8>,
    /// Normalized DTS, including negative values before frame clamping.
    pub stamp: Option<f64>,
    pub duration: Option<f64>,
    /// Container PTS is only a token for matching MF outputs to input ids.
    pub presentation: Option<f64>,
}
enum Input {
    Avi {
        media: avi::Media,
        cursor: usize,
        start: usize,
    },
    Packets(Box<Packets>),
}
pub(super) struct Media {
    pub format: Format,
    input: Input,
    info: VideoInfo,
    pub period: f64,
    retries: u32,
    pub may_retry: bool,
}
impl Media {
    pub fn open(file: &MovieFile, name: &str, helper: Option<PathBuf>) -> Result<Self, String> {
        let mut handle = std::fs::File::open(&file.path).map_err(|e| e.to_string())?;
        let length = handle.metadata().map_err(|e| e.to_string())?.len();
        let (base, size) = file.range.unwrap_or((0, length));
        if base.checked_add(size).is_none_or(|end| end > length) {
            return Err("movie range is outside its file".into());
        }
        let mut head = vec![0; size.min(4096) as usize];
        handle
            .seek(SeekFrom::Start(base))
            .and_then(|_| handle.read_exact(&mut head))
            .map_err(|e| e.to_string())?;
        let sniffed = ddi_library::video::sniff(&head);
        let container = sniffed.container;
        if !matches!(
            sniffed.codec,
            VideoCodec::H264 | VideoCodec::Hevc | VideoCodec::Vp9 | VideoCodec::Unknown
        ) {
            return Err("not a supported native video stream".into());
        }
        if !matches!(
            container,
            VideoContainer::Avi | VideoContainer::Mp4 | VideoContainer::Matroska
        ) {
            return Err("native video requires AVI, MP4 or Matroska".into());
        }
        if container == VideoContainer::Avi {
            let (media, format) = avi::Media::open(file)?;
            let period = f64::from(media.avi.scale) / f64::from(media.avi.rate);
            let info = VideoInfo {
                width: format.stream.sps.width,
                height: format.stream.sps.height,
                duration: media
                    .chunks
                    .last()
                    .map(|c| (u64::from(c.frame) + 1 - u64::from(media.avi.start)) as f64 * period),
                codec: "h264".into(),
            };
            return Ok(Self {
                format: Format::H264(format),
                input: Input::Avi {
                    media,
                    cursor: 0,
                    start: 0,
                },
                info,
                period,
                retries: 0,
                may_retry: false,
            });
        }
        let (packets, format, info, period) = Packets::open(helper, file.clone(), name)?;
        Ok(Self {
            format,
            input: Input::Packets(Box::new(packets)),
            info,
            period,
            retries: 0,
            may_retry: false,
        })
    }
    pub fn packet_path(&self) -> bool {
        matches!(&self.input, Input::Packets(_))
    }
    pub fn info(&self) -> VideoInfo {
        self.info.clone()
    }
    pub fn next(&mut self) -> Result<Option<Compressed>, String> {
        match &mut self.input {
            Input::Avi { media, cursor, .. } => {
                let Some(c) = media.chunks.get(*cursor) else {
                    return Ok(None);
                };
                let stamp = Some(f64::from(c.frame) * self.period);
                let bytes = media.packet(*cursor)?;
                *cursor += 1;
                Ok(Some(Compressed {
                    bytes,
                    stamp,
                    duration: None,
                    presentation: None,
                }))
            }
            Input::Packets(p) => p.next(),
        }
    }
    pub fn seek(&mut self, t: f64) -> Result<(), String> {
        let t = if t.is_finite() { t.max(0.0) } else { 0.0 };
        self.retries = 0;
        match &mut self.input {
            Input::Avi {
                media,
                cursor,
                start,
            } => {
                let chosen = h264::seek_start(&media.chunks, t / self.period, self.format.delay());
                *start = chosen.unwrap_or(0);
                *cursor = *start;
                self.may_retry = chosen.is_some();
            }
            Input::Packets(p) => {
                let back = f64::from(self.format.delay() + 1) * self.period;
                let ts = p
                    .start
                    .checked_add(ticks((t - back) / p.time_base, false)?)
                    .ok_or("native seek timestamp overflow")?
                    .max(p.start);
                self.may_retry = p.position(Some(ts))?;
            }
        }
        Ok(())
    }
    pub fn retry(&mut self) -> Result<(), String> {
        match &mut self.input {
            Input::Avi {
                media,
                cursor,
                start,
            } => {
                let chosen = h264::seek_back(
                    &media.chunks,
                    media.chunks[*start].frame,
                    self.retries,
                    1.0 / self.period,
                );
                *start = chosen.unwrap_or(0);
                *cursor = *start;
                self.may_retry = chosen.is_some();
            }
            Input::Packets(p) => {
                let landed = p.landed.unwrap_or(p.start);
                let ts = if self.retries >= 4 || landed <= p.start {
                    None
                } else {
                    Some(
                        landed
                            .checked_sub(ticks(
                                0.5 * f64::from(1u32 << self.retries) / p.time_base,
                                true,
                            )?)
                            .ok_or("native seek backoff overflow")?
                            .max(p.start),
                    )
                };
                self.may_retry = p.position(ts)?;
            }
        }
        self.retries += 1;
        Ok(())
    }
    /// A seek landed at EOF without emitting anything: reopen as the module does.
    pub fn reopen(&mut self) -> Result<(), String> {
        match &mut self.input {
            Input::Avi { cursor, start, .. } => {
                *cursor = 0;
                *start = 0;
            }
            Input::Packets(p) => {
                p.position(None)?;
            }
        }
        self.may_retry = false;
        Ok(())
    }
}
fn ticks(value: f64, ceil: bool) -> Result<i64, String> {
    let n = if ceil { value.ceil() } else { value.floor() };
    if !n.is_finite() || n < i64::MIN as f64 || n >= i64::MAX as f64 {
        return Err("native seek timestamp outside i64".into());
    }
    Ok(n as i64)
}
struct Packets {
    reader: Option<PacketReader>,
    helper: Option<PathBuf>,
    file: MovieFile,
    name: String,
    initial: super::packets::StreamInfo,
    configuration: Vec<u8>,
    head: Option<Packet>,
    landed: Option<i64>,
    start: i64,
    time_base: f64,
}
impl Packets {
    fn open(
        helper: Option<PathBuf>,
        file: MovieFile,
        name: &str,
    ) -> Result<(Self, Format, VideoInfo, f64), String> {
        let mut reader = demux(helper.clone(), file.clone(), name)?;
        let initial = reader.info().clone();
        if !matches!(initial.codec.as_str(), "h264" | "hevc" | "vp9")
            || !matches!(
                initial.container.as_str(),
                "mov,mp4,m4a,3gp,3g2,mj2" | "matroska,webm"
            )
        {
            return Err("native packet path requires supported MP4/Matroska video".into());
        }
        if initial.container_duration.is_none_or(|t| t <= 0) {
            return Err("native packet path requires reliable container duration metadata".into());
        }
        let configuration = reader.configuration()?;
        let first = reader.next_packet()?.ok_or("empty native video stream")?;
        checked_packet(&first, None)?;
        let format = match initial.codec.as_str() {
            "h264" => {
                if configuration.len() < 7 || configuration[0] != 1 {
                    return Err("native packet path requires avcC".into());
                }
                checked_packet(&first, Some(Framing::Length((configuration[4] & 3) + 1)))?;
                Format::H264(h264_format::Format::new(&configuration, &first.data)?)
            }
            "hevc" => Format::Hevc(hevc::Format::new(&configuration)?),
            "vp9" => Format::Vp9(vp9::Format {
                width: initial.width,
                height: initial.height,
                full_range: initial.color_range == 2,
                bt709: initial.color_space == 1,
            }),
            _ => unreachable!(),
        };
        if initial.width != format.width()
            || initial.height != format.height()
            || initial.width == 0
            || initial.height == 0
            || initial.width > 4096
            || initial.height > 2160
            || initial.bit_depth != 8
            || !matches!(initial.field_order, 0 | 1)
            || initial.video_delay < 0
            || initial.video_delay as u32 > format.delay()
            || !matches!(initial.color_range, 0..=2)
            || (initial.color_range != 0 && (initial.color_range == 2) != format.full_range())
            || !matches!(initial.color_space, 1 | 2 | 5 | 6)
            || (initial.color_space != 2
                && (initial.color_space == 1)
                    != (format.matrix() == ddi_platform::video::YuvMatrix::Bt709))
            || (!matches!(&format, Format::H264(_))
                && (!matches!(initial.color_transfer, 1 | 2 | 6)
                    || !matches!(initial.color_primaries, 1 | 2 | 5 | 6)))
        {
            return Err("native container parameters disagree with the checked codec".into());
        }
        // Refuse unsupported first-packet syntax before creating an OS session.
        format.order().submit(&format, &first.data, 0, true)?;
        let time_base = f64::from(initial.time_base[0]) / f64::from(initial.time_base[1]);
        let period = if initial.frame_rate.iter().all(|v| *v > 0) {
            f64::from(initial.frame_rate[1]) / f64::from(initial.frame_rate[0])
        } else {
            1.0 / 30.0
        };
        let duration = initial
            .duration
            .filter(|d| *d > 0)
            .map(|d| d as f64 * time_base)
            .or_else(|| {
                initial
                    .container_duration
                    .filter(|d| *d > 0)
                    .map(|d| d as f64 / 1_000_000.0)
            });
        let info = VideoInfo {
            width: format.width(),
            height: format.height(),
            duration,
            codec: format.codec().into(),
        };
        let start = initial.start.unwrap_or(0);
        Ok((
            Self {
                reader: Some(reader),
                helper,
                file,
                name: name.into(),
                initial,
                configuration,
                head: Some(first),
                landed: None,
                start,
                time_base,
            },
            format,
            info,
            period,
        ))
    }
    fn position(&mut self, timestamp: Option<i64>) -> Result<bool, String> {
        self.head = None;
        self.landed = None;
        if let Some(ts) = timestamp
            && ts >= 0
            && self.reader.as_mut().expect("open reader").seek(ts).is_ok()
        {
            return Ok(true);
        }
        // Dispose of the old heap before allocating the replacement, including
        // a normal reopen. Four workers must not temporarily own eight heaps.
        self.reader = None;
        let mut reader = demux(self.helper.clone(), self.file.clone(), &self.name)?;
        if reader.info() != &self.initial || reader.configuration()? != self.configuration {
            return Err("native container changed while reopening".into());
        }
        self.reader = Some(reader);
        Ok(false)
    }
    fn next(&mut self) -> Result<Option<Compressed>, String> {
        let packet = match self.head.take() {
            Some(p) => Some(p),
            None => self.reader.as_mut().expect("open reader").next_packet()?,
        };
        let Some(packet) = packet else {
            return Ok(None);
        };
        // Framing is validated by each codec before submission; flags and
        // side data cannot silently alter the native decoder's configuration.
        checked_packet(&packet, None)?;
        self.landed.get_or_insert(packet.dts.unwrap_or(self.start));
        let stamp = packet
            .dts
            .map(|d| {
                d.checked_sub(self.start)
                    .ok_or("native DTS overflow")
                    .map(|d| d as f64 * self.time_base)
            })
            .transpose()?;
        let duration = (packet.duration > 0).then_some(packet.duration as f64 * self.time_base);
        Ok(Some(Compressed {
            bytes: packet.data,
            stamp,
            duration,
            presentation: packet
                .pts
                .map(|p| {
                    p.checked_sub(self.start)
                        .ok_or("native PTS overflow")
                        .map(|p| p as f64 * self.time_base)
                })
                .transpose()?,
        }))
    }
}
// A demuxing helper of this movie's own, apart from the shared software
// decoder. The outer worker's four-second deadline also covers OS calls.
fn demux(helper: Option<PathBuf>, file: MovieFile, name: &str) -> Result<PacketReader, String> {
    PacketReader::open_bounded(helper, file, name, Duration::from_secs(2))
}
/// This first streaming class has one picture/slice per packet and no
/// discard/corrupt/encrypted/configuration side data. Refusal uses the module.
fn checked_packet(p: &Packet, framing: Option<Framing>) -> Result<(), String> {
    if p.flags & !1 != 0 || p.configuration != 0 || !p.side_data.is_empty() || p.data.is_empty() {
        return Err("native packet has unsupported flags or side data".into());
    }
    if let Some(framing) = framing {
        checked_picture(&p.data, framing)?;
    }
    Ok(())
}
pub(super) fn checked_picture(data: &[u8], framing: Framing) -> Result<(), String> {
    if let Framing::Length(n) = framing {
        if !(1..=4).contains(&n) {
            return Err("invalid H.264 NAL length width".into());
        }
        let n = usize::from(n);
        let mut at = 0;
        let mut count = 0;
        while at < data.len() {
            count += 1;
            if count > 256 {
                return Err("too many H.264 NAL units in a packet".into());
            }
            let prefix = data.get(at..at + n).ok_or("truncated H.264 NAL length")?;
            let length = prefix
                .iter()
                .fold(0usize, |a, b| (a << 8) | usize::from(*b));
            at += n;
            if length == 0 || length > data.len() - at {
                return Err("invalid H.264 NAL length".into());
            }
            at += length;
        }
    } else if !data.starts_with(&[0, 0, 1]) && !data.starts_with(&[0, 0, 0, 1]) {
        return Err("invalid H.264 Annex B packet".into());
    }
    if matches!(framing, Framing::AnnexB)
        && data
            .windows(3)
            .filter(|w| *w == [0, 0, 1])
            .take(257)
            .count()
            > 256
    {
        return Err("too many H.264 NAL units in a packet".into());
    }
    let units = h264::nal_units(data, framing);
    if units
        .iter()
        .any(|n| n[0] & 128 != 0 || !matches!(n[0] & 31, 1 | 5..=12))
        || units.iter().filter(|n| matches!(n[0] & 31, 1 | 5)).count() != 1
    {
        return Err("native packet path requires one supported H.264 slice per packet".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_packet_framing_is_bounded_and_complete() {
        for bad in [
            &[][..],
            &[0],
            &[0, 0, 0, 4, 0x65],
            &[0, 0, 0, 0],
            &[0, 0, 0, 1, 0x65, 0],
            &[0, 0, 0, 1, 0x85],
            &[0, 0, 0, 1, 0x62],
            &[0, 0, 0, 1, 0x65, 0, 0, 0, 1, 0x61],
        ] {
            assert!(checked_picture(bad, Framing::Length(4)).is_err(), "{bad:?}");
        }
        assert!(checked_picture(&[1, 0x65], Framing::Length(0)).is_err());
        assert!(checked_picture(&[1, 0x65], Framing::Length(1)).is_ok());
        let mut many = [1, 6].repeat(256);
        many.extend_from_slice(&[1, 0x65]);
        assert!(checked_picture(&many, Framing::Length(1)).is_err());
        assert!(checked_picture(&[0, 0, 1, 0x65], Framing::AnnexB).is_ok());
        let mut many = [0, 0, 1, 6].repeat(256);
        many.extend_from_slice(&[0, 0, 1, 0x65]);
        assert!(checked_picture(&many, Framing::AnnexB).is_err());
    }
    #[test]
    fn timestamp_conversion_never_saturates_a_seek() {
        assert_eq!(ticks(-1.2, false).unwrap(), -2);
        assert_eq!(ticks(1.2, true).unwrap(), 2);
        for value in [
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            i64::MAX as f64,
            2.0 * i64::MIN as f64,
        ] {
            assert!(ticks(value, false).is_err());
        }
    }
}
