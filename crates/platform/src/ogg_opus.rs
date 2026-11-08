//! Ogg Opus files written from their packets (RFC 7845, the pages of RFC
//! 3533), as `.opus`: what the announcer studio's packs hold where a
//! take is tidied (`docs/plans/announcer-studio.md`, step 6). The packets
//! are the browser's encoder's on the web and `rusty-opus`'s elsewhere
//! ([`crate::opus::encode`]), so this is built on every target and takes
//! no codec.
//!
//! The same packets give the same bytes: the stream's serial number is
//! fixed (a file holds one stream) and the vendor string says only the
//! game, so an export keeps saying what its draft says.

/// The rate Opus runs at here, which granule positions always count in.
pub const RATE: u32 = 48_000;

/// A packet's frames at most (RFC 6716, section 3.2.5): 120 ms.
const MAX_PACKET_FRAMES: u32 = 5_760;
/// A page's frames at most: RFC 7845 asks for a second or less of them.
const PAGE_FRAMES: u64 = RATE as u64;
/// A page's lacing values at most.
const MAX_SEGMENTS: usize = 255;
/// `OpusTags`'s vendor string.
const VENDOR: &str = "Dito Dito Infinito";
const SERIAL: u32 = 0x4444_4921;

const BEGIN_OF_STREAM: u8 = 0x02;
const END_OF_STREAM: u8 = 0x04;

/// The bit rate encoded at, in bits a second: a tenth of a WAV's size or
/// less, and about as near the original as libopus gets at the same rate.
pub const fn bitrate(channels: usize) -> u32 {
    if channels < 2 { 64_000 } else { 96_000 }
}

/// About the bytes `seconds` of `channels` channels take as a file.
pub fn len(seconds: f64, channels: usize) -> u64 {
    // The bit rate, and the headers' and pages' bytes.
    (seconds * f64::from(bitrate(channels)) / 8.0 * 1.05) as u64 + 128
}

/// What the file's packets are and how to play them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stream {
    /// 1 or 2.
    pub channels: u8,
    /// The decoder's first frames, which are not the sound's: the
    /// encoder's delay.
    pub pre_skip: u16,
    /// The sound's length, in frames at [`RATE`]: what is decoded past
    /// `pre_skip + frames` is dropped.
    pub frames: u64,
}

/// A packet's length in frames at [`RATE`], from its first bytes
/// (RFC 6716, section 3.1); `None` for one that cannot be a packet.
pub fn packet_frames(packet: &[u8]) -> Option<u32> {
    let toc = *packet.first()?;
    let config = toc >> 3;
    let frame: u32 = match config {
        0..=11 => [480, 960, 1_920, 2_880][usize::from(config & 3)],
        12..=15 => [480, 960][usize::from(config & 1)],
        _ => 120 << (config & 3),
    };
    let count = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => u32::from(*packet.get(1)? & 0x3F),
    };
    Some(frame * count).filter(|&n| (1..=MAX_PACKET_FRAMES).contains(&n))
}

/// The CRC of a page (RFC 3533: polynomial 0x04C11DB7, not reflected,
/// starting from zero, no final XOR).
const CRC: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut r = (i as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            r = if r & 0x8000_0000 != 0 {
                (r << 1) ^ 0x04C1_1DB7
            } else {
                r << 1
            };
            bit += 1;
        }
        table[i] = r;
        i += 1;
    }
    table
};

fn crc(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0, |c, &b| (c << 8) ^ CRC[usize::from((c >> 24) as u8 ^ b)])
}

/// The lacing values `len` bytes take: 255s, then what is left (0 after a
/// whole number of 255s, which is how a packet is seen to end).
fn lacing(len: usize, out: &mut Vec<u8>) {
    out.extend(std::iter::repeat_n(255, len / 255));
    out.push((len % 255) as u8);
}

/// The file, page by page.
#[derive(Default)]
struct Pages {
    out: Vec<u8>,
    sequence: u32,
}

impl Pages {
    fn page(&mut self, flags: u8, granule: u64, lacing: &[u8], body: &[u8]) {
        let start = self.out.len();
        self.out.extend_from_slice(b"OggS\0");
        self.out.push(flags);
        self.out.extend_from_slice(&granule.to_le_bytes());
        self.out.extend_from_slice(&SERIAL.to_le_bytes());
        self.out.extend_from_slice(&self.sequence.to_le_bytes());
        self.out.extend_from_slice(&[0; 4]);
        self.out.push(lacing.len() as u8);
        self.out.extend_from_slice(lacing);
        self.out.extend_from_slice(body);
        let sum = crc(&self.out[start..]);
        self.out[start + 22..start + 26].copy_from_slice(&sum.to_le_bytes());
        self.sequence += 1;
    }

    /// A header packet on a page of its own.
    fn header(&mut self, flags: u8, packet: &[u8]) {
        let mut values = Vec::new();
        lacing(packet.len(), &mut values);
        self.page(flags, 0, &values, packet);
    }
}

/// The file `packets` make for `stream`. Packets wholly past its end are
/// left out, so the last page's granule (which trims the last packet)
/// never falls before the page before it; one packet at least is written.
pub fn write<P: AsRef<[u8]>>(stream: Stream, packets: &[P]) -> Result<Vec<u8>, String> {
    if !(1..=2).contains(&stream.channels) {
        return Err(format!("Opus of {} channels", stream.channels));
    }
    if packets.is_empty() {
        return Err("Opus without packets".into());
    }
    let mut pages = Pages::default();

    let mut head = Vec::with_capacity(19);
    head.extend_from_slice(b"OpusHead");
    head.push(1);
    head.push(stream.channels);
    head.extend_from_slice(&stream.pre_skip.to_le_bytes());
    head.extend_from_slice(&RATE.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    pages.header(BEGIN_OF_STREAM, &head);

    let mut tags = Vec::with_capacity(16 + VENDOR.len());
    tags.extend_from_slice(b"OpusTags");
    tags.extend_from_slice(&(VENDOR.len() as u32).to_le_bytes());
    tags.extend_from_slice(VENDOR.as_bytes());
    tags.extend_from_slice(&0u32.to_le_bytes());
    pages.header(0, &tags);

    let end = u64::from(stream.pre_skip) + stream.frames;
    let mut decoded = 0u64;
    let mut page_start = 0u64;
    let mut values = Vec::new();
    let mut body = Vec::new();
    let mut needed = Vec::new();
    for packet in packets {
        let packet = packet.as_ref();
        let frames = packet_frames(packet).ok_or("not an Opus packet")?;
        needed.clear();
        lacing(packet.len(), &mut needed);
        if needed.len() > MAX_SEGMENTS {
            return Err("an Opus packet too long for a page".into());
        }
        if values.len() + needed.len() > MAX_SEGMENTS || decoded - page_start >= PAGE_FRAMES {
            pages.page(0, decoded, &values, &body);
            values.clear();
            body.clear();
            page_start = decoded;
        }
        values.extend_from_slice(&needed);
        body.extend_from_slice(packet);
        decoded += u64::from(frames);
        if decoded >= end {
            break;
        }
    }
    pages.page(END_OF_STREAM, decoded.min(end), &values, &body);
    Ok(pages.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 20 ms CELT packet of `len` bytes.
    fn packet(len: usize) -> Vec<u8> {
        let mut p = vec![0xAB; len];
        p[0] = 31 << 3;
        p
    }

    struct Page<'a> {
        flags: u8,
        granule: u64,
        sequence: u32,
        lacing: &'a [u8],
        body: &'a [u8],
    }

    /// The file's pages, each checked against its CRC.
    fn pages(file: &[u8]) -> Vec<Page<'_>> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < file.len() {
            assert_eq!(&file[at..at + 5], b"OggS\0");
            let segments = usize::from(file[at + 26]);
            let lacing = &file[at + 27..at + 27 + segments];
            let len: usize = lacing.iter().map(|&v| usize::from(v)).sum();
            let end = at + 27 + segments + len;
            let mut blank = file[at..end].to_vec();
            blank[22..26].fill(0);
            let stored = u32::from_le_bytes(file[at + 22..at + 26].try_into().unwrap());
            assert_eq!(crc(&blank), stored, "the CRC of the page at {at}");
            out.push(Page {
                flags: file[at + 5],
                granule: u64::from_le_bytes(file[at + 6..at + 14].try_into().unwrap()),
                sequence: u32::from_le_bytes(file[at + 18..at + 22].try_into().unwrap()),
                lacing,
                body: &file[at + 27 + segments..end],
            });
            at = end;
        }
        out
    }

    #[test]
    fn the_crc_is_ogg_s() {
        // The check value of this CRC (CRC-32/MPEG-2 without its initial
        // and final inversions: what a page's sum is).
        assert_eq!(crc(b"123456789"), 0x89A1_897F);
    }

    #[test]
    fn a_packet_says_how_long_it_is() {
        // SILK narrowband 60 ms, one frame.
        assert_eq!(packet_frames(&[3 << 3]), Some(2_880));
        // Hybrid full band 20 ms.
        assert_eq!(packet_frames(&[15 << 3]), Some(960));
        // CELT full band 2.5 and 20 ms.
        assert_eq!(packet_frames(&[28 << 3]), Some(120));
        assert_eq!(packet_frames(&[31 << 3]), Some(960));
        // Two frames, of both kinds of code.
        assert_eq!(packet_frames(&[(31 << 3) | 1]), Some(1_920));
        assert_eq!(packet_frames(&[(31 << 3) | 2]), Some(1_920));
        // Code 3 counts its frames in its next byte.
        assert_eq!(packet_frames(&[(31 << 3) | 3, 6]), Some(5_760));
        // Past 120 ms, none and nothing to read: not packets.
        assert_eq!(packet_frames(&[(31 << 3) | 3, 7]), None);
        assert_eq!(packet_frames(&[(31 << 3) | 3, 0]), None);
        assert_eq!(packet_frames(&[(31 << 3) | 3]), None);
        assert_eq!(packet_frames(&[]), None);
    }

    #[test]
    fn the_headers_are_on_pages_of_their_own() {
        let file = write(
            Stream {
                channels: 2,
                pre_skip: 312,
                frames: 100,
            },
            &[packet(40)],
        )
        .unwrap();
        let p = pages(&file);
        assert_eq!(p.len(), 3);
        assert_eq!((p[0].flags, p[0].granule, p[0].sequence), (2, 0, 0));
        assert_eq!(
            p[0].body,
            [
                b"OpusHead".as_slice(),
                &[1, 2],
                &312u16.to_le_bytes(),
                &48_000u32.to_le_bytes(),
                &[0, 0, 0],
            ]
            .concat()
        );
        assert_eq!((p[1].flags, p[1].granule, p[1].sequence), (0, 0, 1));
        assert!(p[1].body.starts_with(b"OpusTags"));
        assert_eq!(&p[1].body[p[1].body.len() - 4..], &[0; 4]);
        // The end's granule trims the one packet.
        assert_eq!((p[2].flags, p[2].granule, p[2].sequence), (4, 412, 2));
        assert_eq!(p[2].body, packet(40));
    }

    #[test]
    fn packets_are_laced_across_their_lengths() {
        // Whole 255s end with a zero, or the next packet would run on.
        let packets = [packet(254), packet(255), packet(510), packet(1_000)];
        let file = write(
            Stream {
                channels: 1,
                pre_skip: 0,
                frames: 3_840,
            },
            &packets,
        )
        .unwrap();
        let p = pages(&file);
        assert_eq!(p.len(), 3);
        assert_eq!(p[2].body, packets.concat());
        let mut want = vec![254, 255, 0, 255, 255, 0];
        want.extend([255, 255, 255, 235]);
        assert_eq!(p[2].lacing, want);
    }

    #[test]
    fn pages_hold_a_second_at_most_and_count_what_ended_on_them() {
        let packets = vec![packet(30); 120];
        let file = write(
            Stream {
                channels: 1,
                pre_skip: 312,
                frames: 120 * 960 - 312,
            },
            &packets,
        )
        .unwrap();
        let p = pages(&file);
        let audio = &p[2..];
        assert_eq!(audio.len(), 3);
        for (i, page) in audio.iter().enumerate() {
            assert_eq!(page.sequence, 2 + i as u32);
        }
        assert_eq!(
            audio.iter().map(|p| p.granule).collect::<Vec<_>>(),
            [48_000, 96_000, 115_200]
        );
        assert_eq!(
            audio.iter().map(|p| p.flags).collect::<Vec<_>>(),
            [0, 0, END_OF_STREAM]
        );
    }

    #[test]
    fn a_page_of_long_packets_ends_at_its_last_lacing_value() {
        // 800 bytes take four values: 63 fit in 255, the 64th a new page
        // (packets of 2.5 ms, so that a second is not what ends it).
        let mut short = packet(800);
        short[0] = 28 << 3;
        let packets = vec![short; 64];
        let file = write(
            Stream {
                channels: 1,
                pre_skip: 0,
                frames: 64 * 120,
            },
            &packets,
        )
        .unwrap();
        let p = pages(&file);
        assert_eq!(p.len(), 4);
        assert_eq!(p[2].lacing.len(), 63 * 4);
        assert_eq!((p[2].flags, p[2].granule), (0, 63 * 120));
        assert_eq!((p[3].flags, p[3].granule), (END_OF_STREAM, 64 * 120));
    }

    #[test]
    fn packets_past_the_end_are_left_out() {
        // A browser's encoder runs on after its input, by however much:
        // the end's granule must not fall before the page before it.
        let packets = vec![packet(30); 200];
        let file = write(
            Stream {
                channels: 1,
                pre_skip: 312,
                frames: 51 * 960,
            },
            &packets,
        )
        .unwrap();
        let p = pages(&file);
        assert_eq!(p.len(), 4);
        assert_eq!((p[2].flags, p[2].granule), (0, 48_000));
        assert_eq!((p[3].flags, p[3].granule), (END_OF_STREAM, 312 + 51 * 960));
        assert!(p[3].granule > p[2].granule);
        assert_eq!(p[3].lacing.len(), 2);
    }

    #[test]
    fn an_end_past_the_packets_is_where_they_stop() {
        let file = write(
            Stream {
                channels: 1,
                pre_skip: 312,
                frames: 960,
            },
            &[packet(30)],
        )
        .unwrap();
        assert_eq!(pages(&file).last().unwrap().granule, 960);
    }

    #[test]
    fn nothing_to_say_is_one_packet_ended_at_once() {
        let file = write(
            Stream {
                channels: 1,
                pre_skip: 312,
                frames: 0,
            },
            &[packet(30), packet(30)],
        )
        .unwrap();
        let p = pages(&file);
        assert_eq!(p.len(), 3);
        assert_eq!((p[2].flags, p[2].granule), (END_OF_STREAM, 312));
        assert_eq!(p[2].lacing.len(), 1);
    }

    #[test]
    fn the_same_packets_give_the_same_bytes() {
        let s = Stream {
            channels: 1,
            pre_skip: 312,
            frames: 960,
        };
        let packets = [packet(30), packet(31)];
        assert_eq!(write(s, &packets), write(s, &packets));
    }

    #[test]
    fn what_is_not_a_stream_is_refused() {
        let s = Stream {
            channels: 1,
            pre_skip: 0,
            frames: 0,
        };
        let none: [Vec<u8>; 0] = [];
        assert!(write(s, &none).is_err());
        assert!(write(s, &[Vec::new()]).is_err());
        assert!(write(Stream { channels: 3, ..s }, &[packet(30)]).is_err());
        assert!(write(s, &[packet(255 * 255)]).is_err());
    }
}
