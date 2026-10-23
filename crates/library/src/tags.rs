//! A music file's title and artist from its tags, for a song the chart
//! editor makes from it (`docs/plans/editor.md`, "New songs"): ID3v2
//! (MP3), Vorbis comments (Ogg Vorbis and Opus, FLAC) and a WAV's
//! `LIST`/`INFO`. Only the file's start is read ([`HEAD`] bytes); what
//! is cut off or cannot be read is left out, never an error.

/// The names a music file's tags give.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// How much of a file's start [`read`] looks at: a cover picture may sit
/// before the names.
pub const HEAD: usize = 1 << 20;

/// The tags at the start of a music file.
pub fn read(head: &[u8]) -> Tags {
    let mut tags = if head.starts_with(b"ID3") {
        id3(head)
    } else if head.starts_with(b"OggS") {
        ogg(head)
    } else if let Some(blocks) = head.strip_prefix(b"fLaC") {
        flac(blocks)
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WAVE") {
        wav(head)
    } else {
        Tags::default()
    };
    for t in [&mut tags.title, &mut tags.artist] {
        *t = t
            .take()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
    }
    tags
}

fn syncsafe(b: &[u8]) -> usize {
    b.iter().fold(0, |n, &x| (n << 7) | (x & 0x7f) as usize)
}

fn be(b: &[u8]) -> usize {
    b.iter().fold(0, |n, &x| (n << 8) | x as usize)
}

/// Bytes with ID3's unsynchronisation (`FF 00` for `FF`) taken out.
fn resync(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut last = 0;
    for &x in b {
        if !(last == 0xff && x == 0) {
            out.push(x);
        }
        last = x;
    }
    out
}

fn id3(head: &[u8]) -> Tags {
    let mut tags = Tags::default();
    let Some(h) = head.get(..10) else {
        return tags;
    };
    let (major, flags) = (h[3], h[5]);
    if !(2..=4).contains(&major) {
        return tags;
    }
    let end = (10 + syncsafe(&h[6..10])).min(head.len());
    let mut body = head[10..end].to_vec();
    if flags & 0x80 != 0 && major < 4 {
        body = resync(&body);
    }
    let mut at = 0;
    if flags & 0x40 != 0 && major > 2 {
        // The extended header: its size, then itself.
        let Some(s) = body.get(..4) else {
            return tags;
        };
        at = if major == 4 { syncsafe(s) } else { 4 + be(s) };
    }
    let (id_len, header) = if major == 2 { (3, 6) } else { (4, 10) };
    while let Some(fh) = body.get(at..at + header) {
        let id = &fh[..id_len];
        if id[0] == 0 {
            break;
        }
        let size = match major {
            2 => be(&fh[3..6]),
            3 => be(&fh[4..8]),
            _ => syncsafe(&fh[4..8]),
        };
        let start = at + header;
        at = start + size;
        let Some(data) = body.get(start..at.min(body.len())) else {
            break;
        };
        let slot = match id {
            b"TIT2" | b"TT2" => &mut tags.title,
            b"TPE1" | b"TP1" => &mut tags.artist,
            _ => continue,
        };
        let mut data = data.to_vec();
        if major == 3 && fh[9] & 0xc0 != 0 {
            // Compressed or encrypted.
            continue;
        }
        if major == 4 {
            let f = fh[9];
            if f & 0x0c != 0 {
                continue;
            }
            if f & 0x01 != 0 {
                data.drain(..4.min(data.len()));
            }
            if f & 0x02 != 0 || flags & 0x80 != 0 {
                data = resync(&data);
            }
        }
        if slot.is_none() {
            *slot = id3_text(&data);
        }
    }
    tags
}

/// A text frame's first string.
fn id3_text(data: &[u8]) -> Option<String> {
    let (&encoding, text) = data.split_first()?;
    let s = match encoding {
        0 => text.iter().map(|&b| b as char).collect(),
        1 | 2 => {
            let mut text = text;
            let mut big = encoding == 2;
            if let Some(rest) = text.strip_prefix(&[0xff, 0xfe]) {
                (text, big) = (rest, false);
            } else if let Some(rest) = text.strip_prefix(&[0xfe, 0xff]) {
                (text, big) = (rest, true);
            }
            let units: Vec<u16> = text
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&c| {
                    if big {
                        u16::from_be_bytes(c)
                    } else {
                        u16::from_le_bytes(c)
                    }
                })
                .take_while(|&u| u != 0)
                .collect();
            String::from_utf16_lossy(&units)
        }
        3 => String::from_utf8_lossy(text).into_owned(),
        _ => return None,
    };
    s.split('\0').next().map(str::to_string)
}

fn le32(b: &[u8], at: usize) -> Option<usize> {
    let b = b.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

/// A Vorbis comment block (after its packet's own header): its title
/// and artist, as far as `b` goes.
fn comments(b: &[u8]) -> Tags {
    let mut tags = Tags::default();
    let Some(vendor) = le32(b, 0) else {
        return tags;
    };
    let mut at = 4 + vendor;
    let Some(n) = le32(b, at) else {
        return tags;
    };
    at += 4;
    for _ in 0..n {
        let Some(len) = le32(b, at) else {
            break;
        };
        at += 4;
        let Some(c) = b.get(at..at + len) else {
            break;
        };
        at += len;
        let Some(eq) = c.iter().position(|&x| x == b'=') else {
            continue;
        };
        let slot = match c[..eq].to_ascii_uppercase().as_slice() {
            b"TITLE" => &mut tags.title,
            b"ARTIST" => &mut tags.artist,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(String::from_utf8_lossy(&c[eq + 1..]).into_owned());
        }
    }
    tags
}

/// An Ogg file's second packet (the comment header) of its first stream.
fn ogg(head: &[u8]) -> Tags {
    let mut packets: Vec<Vec<u8>> = vec![Vec::new()];
    let mut at = 0;
    let mut serial = None;
    'pages: while let Some(h) = head.get(at..at + 27) {
        if &h[..4] != b"OggS" {
            break;
        }
        let segments = h[26] as usize;
        let Some(table) = head.get(at + 27..at + 27 + segments) else {
            break;
        };
        let mut data = at + 27 + segments;
        at = data + table.iter().map(|&l| l as usize).sum::<usize>();
        let s = &h[14..18];
        if *serial.get_or_insert(s) != s {
            continue;
        }
        for &len in table {
            let end = (data + len as usize).min(head.len());
            if let Some(p) = packets.last_mut() {
                p.extend_from_slice(&head[data.min(end)..end]);
            }
            data += len as usize;
            if len < 255 {
                if packets.len() == 2 {
                    break 'pages;
                }
                packets.push(Vec::new());
            }
        }
    }
    let Some(p) = packets.get(1) else {
        return Tags::default();
    };
    if let Some(rest) = p.strip_prefix(b"\x03vorbis") {
        comments(rest)
    } else if let Some(rest) = p.strip_prefix(b"OpusTags") {
        comments(rest)
    } else {
        Tags::default()
    }
}

fn flac(mut b: &[u8]) -> Tags {
    while let Some(h) = b.get(..4) {
        let (last, kind, len) = (h[0] & 0x80 != 0, h[0] & 0x7f, be(&h[1..4]));
        let data = &b[4..];
        if kind == 4 {
            return comments(&data[..len.min(data.len())]);
        }
        if last || data.len() < len {
            break;
        }
        b = &data[len..];
    }
    Tags::default()
}

fn wav(head: &[u8]) -> Tags {
    let mut tags = Tags::default();
    let mut at = 12;
    while let (Some(id), Some(len)) = (head.get(at..at + 4), le32(head, at + 4)) {
        let start = at + 8;
        at = start + len + (len & 1);
        if id != b"LIST" || head.get(start..start + 4) != Some(b"INFO") {
            continue;
        }
        let end = (start + len).min(head.len());
        let mut i = start + 4;
        while let (Some(sub), Some(n)) = (head.get(i..i + 4), le32(head, i + 4)) {
            let text = head.get(i + 8..(i + 8 + n).min(end)).unwrap_or_default();
            i += 8 + n + (n & 1);
            let slot = match sub {
                b"INAM" => &mut tags.title,
                b"IART" => &mut tags.artist,
                _ => continue,
            };
            let text = text.split(|&b| b == 0).next().unwrap_or_default();
            *slot = Some(String::from_utf8_lossy(text).into_owned());
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: &[u8], text: &[u8], syncsafe: bool) -> Vec<u8> {
        let n = text.len() as u32;
        let size = if syncsafe {
            [
                (n >> 21) as u8 & 0x7f,
                (n >> 14) as u8 & 0x7f,
                (n >> 7) as u8 & 0x7f,
                n as u8 & 0x7f,
            ]
        } else {
            n.to_be_bytes()
        };
        [id, &size, &[0, 0], text].concat()
    }

    fn id3(major: u8, frames: &[Vec<u8>]) -> Vec<u8> {
        let body = frames.concat();
        let n = body.len() as u32 + 64;
        let size = [
            (n >> 21) as u8 & 0x7f,
            (n >> 14) as u8 & 0x7f,
            (n >> 7) as u8 & 0x7f,
            n as u8 & 0x7f,
        ];
        [
            b"ID3",
            &[major, 0, 0][..],
            &size,
            &body,
            &[0; 64],
            &[0xff, 0xfb, 0x90],
        ]
        .concat()
    }

    #[test]
    fn id3_titles_and_artists_in_each_encoding() {
        let utf16: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain("夜の歌".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let v3 = id3(
            3,
            &[
                frame(b"APIC", &[0; 300], false),
                frame(b"TIT2", &[&[1][..], &utf16].concat(), false),
                frame(b"TPE1", b"\0Caf\xe9", false),
            ],
        );
        assert_eq!(
            read(&v3),
            Tags {
                title: Some("夜の歌".into()),
                artist: Some("Café".into())
            }
        );
        let v4 = id3(
            4,
            &[
                frame(b"TIT2", b"\x03 Song \0Other", true),
                frame(b"TPE1", b"\x03", true),
            ],
        );
        assert_eq!(
            read(&v4),
            Tags {
                title: Some("Song".into()),
                artist: None
            }
        );
        // Cut off: what was read.
        assert_eq!(read(&v4[..18]), Tags::default());
    }

    fn ogg_page(serial: u32, seq: u32, packets: &[&[u8]]) -> Vec<u8> {
        let mut table = Vec::new();
        for p in packets {
            table.extend(std::iter::repeat_n(255u8, p.len() / 255));
            table.push((p.len() % 255) as u8);
        }
        let mut page = b"OggS\0\0".to_vec();
        page.extend(0u64.to_le_bytes());
        page.extend(serial.to_le_bytes());
        page.extend(seq.to_le_bytes());
        page.extend([0; 4]);
        page.push(table.len() as u8);
        page.extend(table);
        for p in packets {
            page.extend_from_slice(p);
        }
        page
    }

    fn vorbis_comments(prefix: &[u8], list: &[&str]) -> Vec<u8> {
        let mut c = prefix.to_vec();
        c.extend(3u32.to_le_bytes());
        c.extend(b"ddi");
        c.extend((list.len() as u32).to_le_bytes());
        for s in list {
            c.extend((s.len() as u32).to_le_bytes());
            c.extend(s.as_bytes());
        }
        c
    }

    #[test]
    fn ogg_vorbis_and_opus_comments() {
        let tags = vorbis_comments(
            b"\x03vorbis",
            &[
                "encoder=x",
                &format!("PICTURE={}", "a".repeat(600)),
                "title=Tune",
                "ARTIST=Band",
            ],
        );
        let mut file = ogg_page(7, 0, &[b"\x01vorbis-id"]);
        // Another stream's page between.
        file.extend(ogg_page(9, 0, &[b"other"]));
        file.extend(ogg_page(7, 1, &[&tags, b"\x05vorbis-setup"]));
        let want = Tags {
            title: Some("Tune".into()),
            artist: Some("Band".into()),
        };
        assert_eq!(read(&file), want);
        let opus = [
            ogg_page(1, 0, &[b"OpusHead"]),
            ogg_page(
                1,
                1,
                &[&vorbis_comments(
                    b"OpusTags",
                    &["TITLE=Tune", "ARTIST=Band"],
                )],
            ),
        ]
        .concat();
        assert_eq!(read(&opus), want);
        assert_eq!(read(&file[..file.len() / 2]), Tags::default());
    }

    #[test]
    fn flac_and_wav() {
        let c = vorbis_comments(b"", &["TITLE=Flac song"]);
        let mut flac = b"fLaC".to_vec();
        flac.extend([0, 0, 0, 34]);
        flac.extend([0; 34]);
        flac.extend([0x84, 0, 0, c.len() as u8]);
        flac.extend(&c);
        assert_eq!(read(&flac).title.as_deref(), Some("Flac song"));

        let info: Vec<u8> = [
            &b"INFO"[..],
            b"INAM",
            &5u32.to_le_bytes(),
            b"Wave\0\0",
            b"IART",
            &3u32.to_le_bytes(),
            b"Me\0\0",
        ]
        .concat();
        let mut wav = b"RIFF\0\0\0\0WAVE".to_vec();
        wav.extend(b"fmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend([0; 16]);
        wav.extend(b"LIST");
        wav.extend((info.len() as u32).to_le_bytes());
        wav.extend(&info);
        assert_eq!(
            read(&wav),
            Tags {
                title: Some("Wave".into()),
                artist: Some("Me".into())
            }
        );
        assert_eq!(read(b"not music"), Tags::default());
    }
}
