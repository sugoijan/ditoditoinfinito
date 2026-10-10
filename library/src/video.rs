//! Movies named by background changes: which files are movies (by
//! extension, as StepMania decides: `ActorUtil::InitFileTypeLists`) and what
//! they are, from their first bytes ([`sniff`]), so a song's manifest entry
//! says without opening the file again whether its movies play and which
//! decoder they need (`docs/plans/video-backgrounds.md`, step 3).
//!
//! The sniffer reads only what the first [`SNIFF_LEN`] bytes hold: the
//! container always, the video codec where the header says it (an AVI's
//! stream format, an MP4's sample entry when its index comes first, a
//! Matroska track's `CodecID`, a transport stream's program map). The FFmpeg
//! module probes the file itself when it plays, so a format the sniffer
//! does not know is still tried.

use ddi_platform::video::{VideoCodec, VideoContainer, VideoFormat};

use crate::pack::extension;

/// File extensions StepMania treats as movies (`ActorUtil.cpp`, 5_1-new).
pub const MOVIE_EXTENSIONS: &[&str] = &[
    "avi", "f4v", "flv", "mkv", "mp4", "mpeg", "mpg", "mov", "ogv", "webm", "wmv",
];

/// Bytes [`sniff`] looks at.
pub const SNIFF_LEN: usize = 64 * 1024;

/// Bytes read from [`mp4_index_at`] for [`mp4_index_codec`]: the video
/// track's sample description comes early in its track, after any track
/// before it (an audio track's tables are tens of kilobytes a minute).
pub const MP4_INDEX_LEN: usize = 4 << 20;

/// Whether StepMania would load `path` as a movie.
pub fn is_movie(path: &str) -> bool {
    extension(path).is_some_and(|e| MOVIE_EXTENSIONS.iter().any(|m| e.eq_ignore_ascii_case(m)))
}

/// What the movie whose first bytes are `head` is. Never fails: bytes it
/// does not recognise give [`VideoContainer::Unknown`].
pub fn sniff(head: &[u8]) -> VideoFormat {
    let (container, codec) = if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"AVI ") {
        (VideoContainer::Avi, avi(head))
    } else if head.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        (VideoContainer::Matroska, matroska(head))
    } else if is_iso_media(head) {
        (VideoContainer::Mp4, iso_media(head))
    } else if head.starts_with(b"OggS") {
        (VideoContainer::Ogg, ogg(head))
    } else if head.starts_with(&ASF_HEADER) {
        (VideoContainer::Asf, asf(head))
    } else if head.starts_with(b"FLV\x01") {
        (VideoContainer::Flv, flv(head))
    } else if let Some(packet) = transport_packet_size(head) {
        (VideoContainer::MpegTs, transport(head, packet))
    } else if head.starts_with(&[0, 0, 1, 0xba]) {
        (VideoContainer::MpegPs, mpeg_video(head))
    } else if head.starts_with(&[0, 0, 1, 0xb3]) {
        (VideoContainer::MpegVideo, mpeg_video(head))
    } else {
        (VideoContainer::Unknown, VideoCodec::Unknown)
    };
    VideoFormat { container, codec }
}

fn u16_be(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_be(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u32_le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_be(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// A fourcc as text: printable ASCII, trimmed of spaces and NULs, lower case.
fn fourcc_name(code: &[u8]) -> Option<String> {
    let text: String = code
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect();
    let text = text.trim();
    (!text.is_empty() && text.chars().all(|c| c.is_ascii_graphic()))
        .then(|| text.to_ascii_lowercase())
}

/// The codec of a Video for Windows fourcc (AVI `strf`, ASF, Matroska
/// `V_MS/VFW/FOURCC`), as FFmpeg's `ff_codec_bmp_tags` names them.
fn vfw_codec(code: &[u8]) -> VideoCodec {
    if code.iter().all(|&c| c == 0) {
        // BI_RGB: uncompressed.
        return VideoCodec::Other("rawvideo".into());
    }
    let Some(name) = fourcc_name(code) else {
        return VideoCodec::Unknown;
    };
    match name.as_str() {
        "h264" | "x264" | "avc1" | "davc" | "vssh" | "h26l" => VideoCodec::H264,
        "hevc" | "h265" | "hvc1" | "hev1" | "x265" => VideoCodec::Hevc,
        "xvid" | "divx" | "dx50" | "mp4v" | "fmp4" | "3iv2" | "m4s2" | "dxgm" | "blz0" | "xvix"
        | "rmp4" | "mp4s" => VideoCodec::Mpeg4,
        "mpg1" => VideoCodec::Mpeg1,
        "mpg2" | "mpeg" | "mp2v" | "mmes" | "em2v" | "slif" => VideoCodec::Mpeg2,
        "vp80" => VideoCodec::Vp8,
        "vp90" => VideoCodec::Vp9,
        "av01" => VideoCodec::Av1,
        "div3" | "mp43" | "div4" | "mpg3" | "ap41" | "col1" | "div5" | "div6" => {
            VideoCodec::Other("msmpeg4v3".into())
        }
        "mp42" | "div2" => VideoCodec::Other("msmpeg4v2".into()),
        "mjpg" | "avrn" | "jpgl" | "mjls" => VideoCodec::Other("mjpeg".into()),
        "cvid" => VideoCodec::Other("cinepak".into()),
        _ => VideoCodec::Other(name),
    }
}

/// RIFF chunks of `b` from `at` to `end`: `(id, data start, data end)`, the
/// data clipped to what `b` holds.
fn riff_chunks(b: &[u8], mut at: usize, end: usize) -> Vec<([u8; 4], usize, usize)> {
    let end = end.min(b.len());
    let mut out = Vec::new();
    while end.checked_sub(at).is_some_and(|left| left >= 8) && out.len() < 256 {
        let Some(id) = b.get(at..at + 4).and_then(|s| <[u8; 4]>::try_from(s).ok()) else {
            break;
        };
        let size = u32_le(b, at + 4).unwrap_or(0) as usize;
        let data = at + 8;
        out.push((id, data, data.saturating_add(size).min(end)));
        // Chunks are padded to an even size.
        at = match data.checked_add(size).and_then(|e| e.checked_add(size & 1)) {
            Some(next) => next,
            None => break,
        };
    }
    out
}

/// The codec of an AVI's first video stream: `strf`'s compression, else
/// `strh`'s handler.
fn avi(b: &[u8]) -> VideoCodec {
    for (id, start, end) in riff_chunks(b, 12, b.len()) {
        if &id != b"LIST" || b.get(start..start + 4) != Some(b"hdrl") {
            continue;
        }
        for (id, start, end) in riff_chunks(b, start + 4, end) {
            if &id != b"LIST" || b.get(start..start + 4) != Some(b"strl") {
                continue;
            }
            let chunks = riff_chunks(b, start + 4, end);
            let strh = chunks.iter().find(|(id, ..)| id == b"strh");
            let Some(&(_, h, _)) = strh else { continue };
            if b.get(h..h + 4) != Some(b"vids") {
                continue;
            }
            if let Some(&(_, f, fe)) = chunks.iter().find(|(id, ..)| id == b"strf")
                && let Some(code) = b.get(f + 16..f + 20).filter(|_| f + 20 <= fe)
            {
                return vfw_codec(code);
            }
            return b.get(h + 4..h + 8).map_or(VideoCodec::Unknown, vfw_codec);
        }
    }
    VideoCodec::Unknown
}

/// Where an MP4's index (`moov`) starts when the first bytes, `head`, do
/// not hold all of it: encoders write it after the media data unless asked
/// not to.
/// The top-level box sizes say where the next box is; read from there and
/// pass the bytes to [`mp4_index_codec`].
pub fn mp4_index_at(head: &[u8]) -> Option<u64> {
    if !is_iso_media(head) {
        return None;
    }
    let mut at: u64 = 0;
    for _ in 0..64 {
        let i = usize::try_from(at).ok()?;
        if i.checked_add(8)? > head.len() {
            // The next box's header is past what was read: likely the index.
            return Some(at);
        }
        let size = u64::from(u32_be(head, i)?);
        let kind = head.get(i + 4..i + 8)?;
        let total = match size {
            0 => return None,
            1 => u64_be(head, i + 8)?,
            n => n,
        };
        if total < 8 {
            return None;
        }
        let end = at.checked_add(total)?;
        if kind == b"moov" {
            // Whole in `head` (and read there), or cut off: read it whole.
            return (end > head.len() as u64).then_some(at);
        }
        at = end;
    }
    None
}

/// The codec of an MP4's video track from its index, `index` starting at
/// the `moov` box ([`mp4_index_at`]).
pub fn mp4_index_codec(index: &[u8]) -> VideoCodec {
    if index.get(4..8) == Some(b"moov") {
        iso_media(index)
    } else {
        VideoCodec::Unknown
    }
}

/// ISO media boxes of `b` from `at` to `end`: `(type, data start, data end)`.
fn boxes(b: &[u8], mut at: usize, end: usize) -> Vec<([u8; 4], usize, usize)> {
    let end = end.min(b.len());
    let mut out = Vec::new();
    while end.checked_sub(at).is_some_and(|left| left >= 8) && out.len() < 256 {
        let Some(size) = u32_be(b, at) else { break };
        let Some(kind) = b
            .get(at + 4..at + 8)
            .and_then(|s| <[u8; 4]>::try_from(s).ok())
        else {
            break;
        };
        let (data, total) = match size {
            0 => (at + 8, end - at),
            1 => match u64_be(b, at + 8) {
                Some(large) => (at + 16, usize::try_from(large).unwrap_or(usize::MAX)),
                None => break,
            },
            n => (at + 8, n as usize),
        };
        if total < data - at {
            break;
        }
        let box_end = at.saturating_add(total);
        out.push((kind, data, box_end.min(end)));
        if box_end <= at {
            break;
        }
        at = box_end;
    }
    out
}

fn is_iso_media(b: &[u8]) -> bool {
    const TOP: [&[u8; 4]; 7] = [
        b"ftyp", b"moov", b"mdat", b"free", b"skip", b"wide", b"pnot",
    ];
    u32_be(b, 0).is_some_and(|s| s == 1 || s >= 8)
        && b.get(4..8).is_some_and(|k| TOP.iter().any(|t| k == *t))
}

/// The codec of an ISO media file's first video track, when its `moov`
/// comes before the media data.
fn iso_media(b: &[u8]) -> VideoCodec {
    let child = |kind: &[u8; 4], start: usize, end: usize| {
        boxes(b, start, end)
            .into_iter()
            .find(|(k, ..)| k == kind)
            .map(|(_, s, e)| (s, e))
    };
    let Some((moov, moov_end)) = child(b"moov", 0, b.len()) else {
        return VideoCodec::Unknown;
    };
    for (kind, trak, trak_end) in boxes(b, moov, moov_end) {
        if &kind != b"trak" {
            continue;
        }
        let Some((mdia, mdia_end)) = child(b"mdia", trak, trak_end) else {
            continue;
        };
        // `hdlr`: version and flags, pre_defined, then the handler type.
        let video = child(b"hdlr", mdia, mdia_end)
            .is_some_and(|(h, _)| b.get(h + 8..h + 12) == Some(b"vide"));
        if !video {
            continue;
        }
        let entry = child(b"minf", mdia, mdia_end)
            .and_then(|(s, e)| child(b"stbl", s, e))
            .and_then(|(s, e)| child(b"stsd", s, e))
            // Version and flags, entry count, then the first entry's size
            // and type.
            .and_then(|(s, _)| b.get(s + 12..s + 16));
        return entry.map_or(VideoCodec::Unknown, iso_codec);
    }
    VideoCodec::Unknown
}

fn iso_codec(kind: &[u8]) -> VideoCodec {
    let Some(name) = fourcc_name(kind) else {
        return VideoCodec::Unknown;
    };
    match name.as_str() {
        "avc1" | "avc2" | "avc3" | "avc4" => VideoCodec::H264,
        "hvc1" | "hev1" => VideoCodec::Hevc,
        "mp4v" => VideoCodec::Mpeg4,
        "vp08" => VideoCodec::Vp8,
        "vp09" => VideoCodec::Vp9,
        "av01" => VideoCodec::Av1,
        "mp1v" | "m1v1" => VideoCodec::Mpeg1,
        "mp2v" | "m2v1" | "hdv1" | "hdv2" | "hdv3" | "xdv1" | "xdv2" | "xdv3" | "xdva" => {
            VideoCodec::Mpeg2
        }
        "jpeg" | "mjpa" | "mjpb" | "avdj" => VideoCodec::Other("mjpeg".into()),
        "s263" | "h263" => VideoCodec::Other("h263".into()),
        "apch" | "apcn" | "apcs" | "apco" | "ap4h" | "ap4x" => VideoCodec::Other("prores".into()),
        "rle" => VideoCodec::Other("qtrle".into()),
        _ => VideoCodec::Other(name),
    }
}

/// An EBML variable-size integer at `at`: `(value, length)`.
fn ebml_size(b: &[u8], at: usize) -> Option<(u64, usize)> {
    let first = *b.get(at)?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let mut value = u64::from(first) & (0xff >> len);
    for i in 1..len {
        value = (value << 8) | u64::from(*b.get(at + i)?);
    }
    Some((value, len))
}

/// The codec of a Matroska file's first video track (`CodecID`, element
/// `0x86`, `V_…`).
fn matroska(b: &[u8]) -> VideoCodec {
    let mut from = 4;
    while let Some(at) = b
        .get(from..)
        .and_then(|s| s.iter().position(|&c| c == 0x86))
    {
        let at = from + at;
        from = at + 1;
        let Some((len, size_len)) = ebml_size(b, at + 1) else {
            continue;
        };
        let start = at + 1 + size_len;
        let Some(id) = b
            .get(start..start + len as usize)
            .filter(|_| (2..=64).contains(&len))
        else {
            continue;
        };
        if !id.starts_with(b"V_") || !id.iter().all(|c| c.is_ascii_graphic()) {
            continue;
        }
        let id = String::from_utf8_lossy(id);
        return match id.as_ref() {
            "V_MPEG4/ISO/AVC" => VideoCodec::H264,
            "V_MPEGH/ISO/HEVC" => VideoCodec::Hevc,
            "V_MPEG4/ISO/SP" | "V_MPEG4/ISO/ASP" | "V_MPEG4/ISO/AP" => VideoCodec::Mpeg4,
            "V_MPEG1" => VideoCodec::Mpeg1,
            "V_MPEG2" => VideoCodec::Mpeg2,
            "V_VP8" => VideoCodec::Vp8,
            "V_VP9" => VideoCodec::Vp9,
            "V_AV1" => VideoCodec::Av1,
            "V_MS/VFW/FOURCC" => {
                // CodecPrivate (0x63A2) holds a BITMAPINFOHEADER.
                find(b, &[0x63, 0xa2], start)
                    .and_then(|p| {
                        let (_, n) = ebml_size(b, p + 2)?;
                        b.get(p + 2 + n + 16..p + 2 + n + 20)
                    })
                    .map_or(VideoCodec::Unknown, vfw_codec)
            }
            other => VideoCodec::Other(other.trim_start_matches("V_").to_ascii_lowercase()),
        };
    }
    VideoCodec::Unknown
}

/// The codec of an Ogg file's video stream, from its first header packet.
fn ogg(b: &[u8]) -> VideoCodec {
    if find(b, b"\x80theora", 0).is_some() {
        VideoCodec::Other("theora".into())
    } else if find(b, b"OVP80", 0).is_some() {
        VideoCodec::Vp8
    } else if find(b, b"BBCD", 0).is_some() {
        VideoCodec::Other("dirac".into())
    } else {
        VideoCodec::Unknown
    }
}

const ASF_HEADER: [u8; 16] = [
    0x30, 0x26, 0xb2, 0x75, 0x8e, 0x66, 0xcf, 0x11, 0xa6, 0xd9, 0x00, 0xaa, 0x00, 0x62, 0xce, 0x6c,
];
const ASF_STREAM_PROPERTIES: [u8; 16] = [
    0x91, 0x07, 0xdc, 0xb7, 0xb7, 0xa9, 0xcf, 0x11, 0x8e, 0xe6, 0x00, 0xc0, 0x0c, 0x20, 0x53, 0x65,
];
const ASF_VIDEO_MEDIA: [u8; 16] = [
    0xc0, 0xef, 0x19, 0xbc, 0x4d, 0x5b, 0xcf, 0x11, 0xa8, 0xfd, 0x00, 0x80, 0x5f, 0x5c, 0x44, 0x2b,
];

/// The codec of an ASF file's first video stream: its Stream Properties
/// Object's type-specific data (offset 78) holds the encoded size, flags,
/// format data size, then a BITMAPINFOHEADER whose compression is at 16.
fn asf(b: &[u8]) -> VideoCodec {
    let mut from = 16;
    while let Some(at) = find(b, &ASF_STREAM_PROPERTIES, from) {
        from = at + 16;
        if b.get(at + 24..at + 40) == Some(&ASF_VIDEO_MEDIA[..])
            && let Some(code) = b.get(at + 78 + 11 + 16..at + 78 + 11 + 20)
        {
            return vfw_codec(code);
        }
    }
    VideoCodec::Unknown
}

/// The codec of an FLV file's first video tag.
fn flv(b: &[u8]) -> VideoCodec {
    let Some(header) = u32_be(b, 5) else {
        return VideoCodec::Unknown;
    };
    // After the header, the first PreviousTagSize, then tags: type, data
    // size (3 bytes), timestamp (4), stream id (3), data, PreviousTagSize.
    let mut at = (header as usize).saturating_add(4);
    for _ in 0..64 {
        let (Some(&kind), Some(size)) =
            (b.get(at), u32_be(b, at).map(|v| (v & 0xff_ffff) as usize))
        else {
            break;
        };
        if kind & 0x1f == 9 {
            let Some(&first) = b.get(at + 11) else { break };
            if first & 0x80 != 0 {
                // Enhanced RTMP: a fourcc follows.
                return b
                    .get(at + 12..at + 16)
                    .map_or(VideoCodec::Unknown, iso_codec);
            }
            return match first & 0x0f {
                2 => VideoCodec::Other("flv1".into()),
                3 => VideoCodec::Other("flashsv".into()),
                4 => VideoCodec::Other("vp6f".into()),
                5 => VideoCodec::Other("vp6a".into()),
                6 => VideoCodec::Other("flashsv2".into()),
                7 => VideoCodec::H264,
                12 => VideoCodec::Hevc,
                n => VideoCodec::Other(format!("flv codec {n}")),
            };
        }
        at = match at.checked_add(11 + size + 4) {
            Some(next) => next,
            None => break,
        };
    }
    VideoCodec::Unknown
}

/// MPEG-1 or MPEG-2 video, from a sequence header (`00 00 01 B3`) followed
/// by a sequence extension (`00 00 01 B5`, MPEG-2 only) before the first
/// picture or GOP.
fn mpeg_video(b: &[u8]) -> VideoCodec {
    let Some(seq) = find(b, &[0, 0, 1, 0xb3], 0) else {
        return VideoCodec::Unknown;
    };
    let mut at = seq + 4;
    while let Some(next) = find(b, &[0, 0, 1], at) {
        match b.get(next + 3) {
            Some(0xb5) => return VideoCodec::Mpeg2,
            // A GOP or a picture: no extension came, so MPEG-1.
            Some(0xb8) | Some(0x00) => return VideoCodec::Mpeg1,
            Some(_) => at = next + 3,
            None => break,
        }
    }
    VideoCodec::Unknown
}

/// 188 for a transport stream, 192 for one with 4-byte timestamps (M2TS):
/// sync bytes at three packets in a row.
fn transport_packet_size(b: &[u8]) -> Option<usize> {
    [(188, 0), (192, 4)].into_iter().find_map(|(size, offset)| {
        (0..3)
            .all(|i| b.get(offset + i * size) == Some(&0x47))
            .then_some(size)
    })
}

/// The codec of a transport stream's first video stream, from the program
/// map table the program association table points to.
fn transport(b: &[u8], packet: usize) -> VideoCodec {
    let offset = packet - 188;
    // Section payloads by PID, from packets that start a section.
    let section = |pid: u16| -> Option<&[u8]> {
        (0..b.len() / packet).find_map(|i| {
            let p = b.get(i * packet + offset..i * packet + offset + 188)?;
            let this = (u16::from(p[1] & 0x1f) << 8) | u16::from(p[2]);
            if p[0] != 0x47 || this != pid || p[1] & 0x40 == 0 {
                return None;
            }
            let mut at = 4;
            if p[3] & 0x20 != 0 {
                at += 1 + usize::from(*p.get(4)?);
            }
            if p[3] & 0x10 == 0 {
                return None;
            }
            // Pointer field, then the section.
            let start = at + 1 + usize::from(*p.get(at)?);
            p.get(start..)
        })
    };
    let Some(pat) = section(0) else {
        return VideoCodec::Unknown;
    };
    // table id, section length (2), id (2), version, section, last section,
    // then programs: number (2), PID (2), until the CRC.
    let pat_end = 3 + usize::from(u16_be(pat, 1).unwrap_or(0) & 0x0fff);
    let pmt_pid = (8..pat_end.saturating_sub(4).min(pat.len()))
        .step_by(4)
        .find_map(|at| {
            let number = u16_be(pat, at)?;
            (number != 0).then(|| u16_be(pat, at + 2).map(|p| p & 0x1fff))?
        });
    let Some(pmt) = pmt_pid.and_then(section) else {
        return VideoCodec::Unknown;
    };
    // table id, length, program, version, sections, PCR PID (2), program
    // info length (2), descriptors, then streams: type, PID (2), info
    // length (2), descriptors.
    let pmt_end = (3 + usize::from(u16_be(pmt, 1).unwrap_or(0) & 0x0fff))
        .saturating_sub(4)
        .min(pmt.len());
    let mut at = 12 + usize::from(u16_be(pmt, 10).unwrap_or(0) & 0x0fff);
    while at + 5 <= pmt_end {
        let codec = match pmt[at] {
            0x01 => Some(VideoCodec::Mpeg1),
            0x02 => Some(VideoCodec::Mpeg2),
            0x10 => Some(VideoCodec::Mpeg4),
            0x1b => Some(VideoCodec::H264),
            0x24 => Some(VideoCodec::Hevc),
            0xea => Some(VideoCodec::Other("vc1".into())),
            0x42 => Some(VideoCodec::Other("cavs".into())),
            _ => None,
        };
        if let Some(codec) = codec {
            return codec;
        }
        at += 5 + usize::from(u16_be(pmt, at + 3).unwrap_or(0) & 0x0fff);
    }
    VideoCodec::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(container: VideoContainer, codec: VideoCodec) -> VideoFormat {
        VideoFormat { container, codec }
    }

    fn le32(n: usize) -> [u8; 4] {
        (n as u32).to_le_bytes()
    }

    fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend(le32(data.len()));
        out.extend(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn list(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut data = kind.to_vec();
        data.extend(body);
        chunk(b"LIST", &data)
    }

    /// An AVI header with an audio stream, then a video stream whose strh
    /// handler and strf compression are given.
    fn avi_file(handler: &[u8; 4], compression: &[u8; 4]) -> Vec<u8> {
        let mut audio_strh = b"auds".to_vec();
        audio_strh.extend([0; 52]);
        let mut video_strh = b"vids".to_vec();
        video_strh.extend(handler);
        video_strh.extend([0; 48]);
        let mut strf = vec![0u8; 40];
        strf[16..20].copy_from_slice(compression);
        let mut hdrl = chunk(b"avih", &[0; 56]);
        hdrl.extend(list(b"strl", &chunk(b"strh", &audio_strh)));
        let mut strl = chunk(b"strh", &video_strh);
        strl.extend(chunk(b"strf", &strf));
        hdrl.extend(list(b"strl", &strl));
        let mut body = b"AVI ".to_vec();
        body.extend(list(b"hdrl", &hdrl));
        body.extend(list(b"movi", &[0; 32]));
        chunk(b"RIFF", &body)
    }

    #[test]
    fn movies_by_extension_as_stepmania() {
        assert!(is_movie("bg/Intro.AVI"));
        assert!(is_movie("song.webm"));
        assert!(is_movie("x.mpeg"));
        assert!(!is_movie("x.png"));
        assert!(!is_movie("animation"));
        assert!(!is_movie("x.m2v"));
    }

    #[test]
    fn avi_codec_from_the_video_stream() {
        let h264 = avi_file(b"H264", b"H264");
        assert_eq!(sniff(&h264), format(VideoContainer::Avi, VideoCodec::H264));
        assert_eq!(sniff(&avi_file(b"xvid", b"XVID")).codec, VideoCodec::Mpeg4);
        assert_eq!(sniff(&avi_file(b"divx", b"DX50")).codec, VideoCodec::Mpeg4);
        assert_eq!(
            sniff(&avi_file(b"DIV3", b"DIV3")).codec,
            VideoCodec::Other("msmpeg4v3".into())
        );
        assert_eq!(sniff(&avi_file(b"MPG2", b"mpg2")).codec, VideoCodec::Mpeg2);
        assert_eq!(
            sniff(&avi_file(b"cvid", b"cvid")).codec,
            VideoCodec::Other("cinepak".into())
        );
        assert_eq!(
            sniff(&avi_file(b"\0\0\0\0", b"\0\0\0\0")).codec,
            VideoCodec::Other("rawvideo".into())
        );
        // strf wins over strh's handler, which encoders often leave generic.
        assert_eq!(sniff(&avi_file(b"divx", b"H264")).codec, VideoCodec::H264);
        assert_eq!(sniff(&avi_file(b"AV01", b"AV01")).codec, VideoCodec::Av1);
        assert!(!sniff(&avi_file(b"AV01", b"AV01")).playable());
    }

    fn iso_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend(kind);
        out.extend(body);
        out
    }

    /// An MP4 with an audio track and then a video track with this sample
    /// entry, the index before or after the media data.
    fn mp4_file(entry: &[u8; 4], moov_first: bool) -> Vec<u8> {
        let track = |handler: &[u8; 4], entry: &[u8; 4]| {
            let mut hdlr = vec![0u8; 8];
            hdlr.extend(handler);
            hdlr.extend([0; 12]);
            let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
            stsd.extend(iso_box(entry, &[0; 78]));
            let stbl = iso_box(b"stbl", &iso_box(b"stsd", &stsd));
            let minf = iso_box(b"minf", &stbl);
            let mut mdia = iso_box(b"mdhd", &[0; 24]);
            mdia.extend(iso_box(b"hdlr", &hdlr));
            mdia.extend(minf);
            iso_box(b"trak", &iso_box(b"mdia", &mdia))
        };
        let mut moov = iso_box(b"mvhd", &[0; 100]);
        moov.extend(track(b"soun", b"mp4a"));
        moov.extend(track(b"vide", entry));
        let moov = iso_box(b"moov", &moov);
        let mut out = iso_box(b"ftyp", b"isom\0\0\x02\0isomiso2avc1mp41");
        let mdat = iso_box(b"mdat", &[0; 64]);
        if moov_first {
            out.extend(moov);
            out.extend(mdat);
        } else {
            out.extend(mdat);
            out.extend(moov);
        }
        out
    }

    #[test]
    fn mp4_codec_from_the_video_track() {
        assert_eq!(
            sniff(&mp4_file(b"avc1", true)),
            format(VideoContainer::Mp4, VideoCodec::H264)
        );
        assert_eq!(sniff(&mp4_file(b"hvc1", true)).codec, VideoCodec::Hevc);
        assert_eq!(sniff(&mp4_file(b"vp09", true)).codec, VideoCodec::Vp9);
        assert_eq!(sniff(&mp4_file(b"av01", true)).codec, VideoCodec::Av1);
        assert_eq!(sniff(&mp4_file(b"mp4v", true)).codec, VideoCodec::Mpeg4);
        // The index after the media data is beyond the sniffed bytes in a
        // real file; here the whole file is there, so it is found too.
        assert_eq!(sniff(&mp4_file(b"avc1", false)).codec, VideoCodec::H264);
        // Only the start of the file: the container, not the codec.
        let late = mp4_file(b"avc1", false);
        let cut = late.len() - 200;
        assert_eq!(
            sniff(&late[..cut]),
            format(VideoContainer::Mp4, VideoCodec::Unknown)
        );
        // The index after the media data: found from the box sizes.
        let at = mp4_index_at(&late[..cut]).unwrap() as usize;
        assert_eq!(mp4_index_codec(&late[at..]), VideoCodec::H264);
        assert_eq!(mp4_index_at(&mp4_file(b"avc1", true)), None);
        assert_eq!(mp4_index_codec(&late[..64]), VideoCodec::Unknown);
        // QuickTime without `ftyp`.
        let quicktime = &mp4_file(b"jpeg", true)[32..];
        assert_eq!(
            sniff(quicktime),
            format(VideoContainer::Mp4, VideoCodec::Other("mjpeg".into()))
        );
    }

    #[test]
    fn matroska_codec_from_the_codec_id() {
        let mkv = |id: &[u8]| {
            let mut out = vec![0x1a, 0x45, 0xdf, 0xa3, 0x9f];
            out.extend([0x42, 0x82, 0x84]);
            out.extend(b"webm");
            // An audio track first, then the video one.
            out.extend([0x86, 0x80 | 6]);
            out.extend(b"A_OPUS");
            out.extend([0x86, 0x80 | id.len() as u8]);
            out.extend(id);
            out
        };
        assert_eq!(
            sniff(&mkv(b"V_VP9")),
            format(VideoContainer::Matroska, VideoCodec::Vp9)
        );
        assert_eq!(sniff(&mkv(b"V_MPEG4/ISO/AVC")).codec, VideoCodec::H264);
        assert_eq!(sniff(&mkv(b"V_AV1")).codec, VideoCodec::Av1);
        assert_eq!(
            sniff(&mkv(b"V_THEORA")).codec,
            VideoCodec::Other("theora".into())
        );
        let mut vfw = mkv(b"V_MS/VFW/FOURCC");
        vfw.extend([0x63, 0xa2, 0x80 | 40]);
        let mut bih = [0u8; 40];
        bih[16..20].copy_from_slice(b"XVID");
        vfw.extend(bih);
        assert_eq!(sniff(&vfw).codec, VideoCodec::Mpeg4);
    }

    #[test]
    fn mpeg_streams() {
        // Sequence header, then an extension (MPEG-2) or a GOP (MPEG-1).
        let mut mpeg2 = vec![
            0, 0, 1, 0xb3, 0x14, 0x00, 0xf0, 0x13, 0xff, 0xff, 0xe0, 0x18,
        ];
        mpeg2.extend([0, 0, 1, 0xb5, 0x14, 0x8a, 0x00, 0x01]);
        assert_eq!(
            sniff(&mpeg2),
            format(VideoContainer::MpegVideo, VideoCodec::Mpeg2)
        );
        let mut mpeg1 = vec![
            0, 0, 1, 0xb3, 0x14, 0x00, 0xf0, 0x13, 0xff, 0xff, 0xe0, 0x18,
        ];
        mpeg1.extend([0, 0, 1, 0xb8, 0, 0, 0, 0]);
        assert_eq!(sniff(&mpeg1).codec, VideoCodec::Mpeg1);
        let mut ps = vec![0, 0, 1, 0xba, 0x44, 0, 4, 0, 4, 1, 1, 0x89, 0xc3, 0xf8];
        ps.extend([0, 0, 1, 0xe0, 0, 20, 0x81, 0x80, 0x05, 0x21, 0, 1, 0, 1]);
        ps.extend(&mpeg2);
        assert_eq!(
            sniff(&ps),
            format(VideoContainer::MpegPs, VideoCodec::Mpeg2)
        );
    }

    #[test]
    fn transport_stream_codec_from_the_program_map() {
        let packet = |pid: u16, payload: &[u8]| {
            let mut p = vec![0x47, 0x40 | (pid >> 8) as u8, pid as u8, 0x10, 0];
            p.extend(payload);
            p.resize(188, 0xff);
            p
        };
        // PAT: program 1 on PID 0x1000.
        let pat = [
            0x00, 0xb0, 0x0d, 0, 1, 0xc1, 0, 0, 0, 1, 0xf0, 0x00, 0, 0, 0, 0,
        ];
        // PMT: AAC on 0x101, then H.264 on 0x100.
        let pmt = [
            0x02, 0xb0, 0x17, 0, 1, 0xc1, 0, 0, 0xe1, 0x00, 0xf0, 0x00, 0x0f, 0xe1, 0x01, 0xf0,
            0x00, 0x1b, 0xe1, 0x00, 0xf0, 0x00, 0, 0, 0, 0,
        ];
        let mut ts = packet(0, &pat);
        ts.extend(packet(0x1000, &pmt));
        ts.extend(packet(0x100, &[]));
        assert_eq!(sniff(&ts), format(VideoContainer::MpegTs, VideoCodec::H264));
    }

    #[test]
    fn other_containers() {
        let mut flv = b"FLV\x01\x05\0\0\0\x09\0\0\0\0".to_vec();
        // An audio tag, then a video tag with codec 7 (AVC).
        flv.extend([8, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0xaf, 0, 0, 0, 12]);
        flv.extend([9, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0x17]);
        assert_eq!(sniff(&flv), format(VideoContainer::Flv, VideoCodec::H264));
        let mut asf = ASF_HEADER.to_vec();
        asf.extend([0; 14]);
        let mut props = ASF_STREAM_PROPERTIES.to_vec();
        props.extend([0; 8]);
        props.extend(ASF_VIDEO_MEDIA);
        props.resize(78 + 11 + 40, 0);
        props[78 + 11 + 16..78 + 11 + 20].copy_from_slice(b"WMV3");
        asf.extend(props);
        assert_eq!(
            sniff(&asf),
            format(VideoContainer::Asf, VideoCodec::Other("wmv3".into()))
        );
        let ogg = b"OggS\0\x02\0\0\0\0\0\0\0\0\x01\0\0\0\0\0\0\0\0\0\0\0\x01\x2a\x80theora";
        assert_eq!(
            sniff(ogg),
            format(VideoContainer::Ogg, VideoCodec::Other("theora".into()))
        );
        assert_eq!(
            sniff(b"\x89PNG\r\n\x1a\n"),
            format(VideoContainer::Unknown, VideoCodec::Unknown)
        );
        assert_eq!(sniff(&[]).container, VideoContainer::Unknown);
    }

    /// Truncations and corruptions of every synthetic file never panic.
    #[test]
    fn hostile_headers_never_panic() {
        let mut samples = vec![
            avi_file(b"H264", b"H264"),
            mp4_file(b"avc1", true),
            mpeg_hostile_sizes(),
            mp4_file(b"avc1", false),
        ];
        let mut huge = avi_file(b"H264", b"H264");
        // Chunk sizes of 0xffffffff everywhere.
        for at in [4, 16, 24] {
            huge[at..at + 4].copy_from_slice(&[0xff; 4]);
        }
        samples.push(huge);
        let mut seed = 0x2545f491u32;
        for sample in &samples {
            for cut in 0..sample.len() {
                sniff(&sample[..cut]);
                mp4_index_at(&sample[..cut]);
                mp4_index_codec(&sample[cut..]);
            }
            for _ in 0..2000 {
                let mut s = sample.clone();
                for _ in 0..8 {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    let at = seed as usize % s.len();
                    s[at] = (seed >> 8) as u8;
                }
                sniff(&s);
                if let Some(at) = mp4_index_at(&s) {
                    mp4_index_codec(s.get(at as usize..).unwrap_or(&[]));
                }
            }
        }
    }

    /// An MP4 whose boxes claim huge and zero sizes.
    fn mpeg_hostile_sizes() -> Vec<u8> {
        let mut out = iso_box(b"ftyp", b"isom");
        out.extend([0, 0, 0, 1]);
        out.extend(b"moov");
        out.extend(u64::MAX.to_be_bytes());
        out.extend([0, 0, 0, 0]);
        out.extend(b"trak");
        out
    }

    /// `DDI_MOVIES=<dir>`: sniffs every movie under a local folder (packs
    /// stay local) and prints how many of each format, and the files whose
    /// format was not recognised.
    #[test]
    fn local_movies() {
        let Some(dir) = std::env::var_os("DDI_MOVIES") else {
            return;
        };
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if is_movie(&path.to_string_lossy()) {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(std::path::Path::new(&dir), &mut files);
        let mut counts = std::collections::BTreeMap::new();
        for path in &files {
            use std::io::{Read, Seek, SeekFrom};
            let Ok(mut file) = std::fs::File::open(path) else {
                continue;
            };
            let mut head = vec![0; SNIFF_LEN];
            let n = file.read(&mut head).unwrap_or(0);
            head.truncate(n);
            let mut got = sniff(&head);
            if let Some(at) = mp4_index_at(&head) {
                let mut index = vec![0; 4 << 20];
                if file.seek(SeekFrom::Start(at)).is_ok() {
                    let n = file.read(&mut index).unwrap_or(0);
                    index.truncate(n);
                    got.codec = mp4_index_codec(&index);
                }
            }
            if got.container == VideoContainer::Unknown || got.codec == VideoCodec::Unknown {
                eprintln!("not recognised: {}", path.display());
            }
            *counts.entry(got.describe()).or_insert(0) += 1;
        }
        for (format, n) in &counts {
            eprintln!("{n:5} {format}");
        }
        eprintln!("{} movies", files.len());
    }

    /// The clips `video/clips.sh` generates, when they are there (they are
    /// never committed): each must sniff as its name says.
    #[test]
    fn generated_clips() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/video/clips");
        let expected: &[(&str, VideoContainer, VideoCodec)] = &[
            ("xvid.avi", VideoContainer::Avi, VideoCodec::Mpeg4),
            ("h264.avi", VideoContainer::Avi, VideoCodec::H264),
            ("mpeg2.avi", VideoContainer::MpegVideo, VideoCodec::Mpeg2),
            ("h264.mp4", VideoContainer::Mp4, VideoCodec::H264),
            ("hevc.mp4", VideoContainer::Mp4, VideoCodec::Hevc),
            ("mpeg4.mov", VideoContainer::Mp4, VideoCodec::Mpeg4),
            ("h264.mkv", VideoContainer::Matroska, VideoCodec::H264),
            ("vp8.webm", VideoContainer::Matroska, VideoCodec::Vp8),
            ("vp9.webm", VideoContainer::Matroska, VideoCodec::Vp9),
            (
                "theora.ogv",
                VideoContainer::Ogg,
                VideoCodec::Other(String::new()),
            ),
            (
                "wmv2.wmv",
                VideoContainer::Asf,
                VideoCodec::Other(String::new()),
            ),
            (
                "flv1.flv",
                VideoContainer::Flv,
                VideoCodec::Other(String::new()),
            ),
            (
                "divx3.avi",
                VideoContainer::Avi,
                VideoCodec::Other(String::new()),
            ),
            ("mpeg1.mpg", VideoContainer::MpegPs, VideoCodec::Mpeg1),
            ("mpeg2.mpg", VideoContainer::MpegPs, VideoCodec::Mpeg2),
            ("h264.ts", VideoContainer::MpegTs, VideoCodec::H264),
            (
                "cinepak.avi",
                VideoContainer::Avi,
                VideoCodec::Other(String::new()),
            ),
        ];
        let mut checked = 0;
        for (name, container, codec) in expected {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                continue;
            };
            let head = &bytes[..bytes.len().min(SNIFF_LEN)];
            let mut got = sniff(head);
            if let Some(at) = mp4_index_at(head) {
                got.codec = mp4_index_codec(&bytes[at as usize..]);
            }
            assert_eq!(got.container, *container, "{name}");
            match (codec, &got.codec) {
                (VideoCodec::Other(_), VideoCodec::Other(_)) => {}
                _ => assert_eq!(&got.codec, codec, "{name}"),
            }
            checked += 1;
        }
        eprintln!("generated clips sniffed: {checked}");
    }
}
