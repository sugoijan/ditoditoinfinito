//! The video stream of an AVI file, sans-IO: its header and where its
//! chunks are, from the file's index, for decoders that take one coded
//! frame at a time (WebCodecs; `docs/plans/video-backgrounds.md`, step 6).
//!
//! Frames are numbered as FFmpeg's AVI demuxer numbers them
//! (`libavformat/avidec.c`, 9.0): every chunk of the stream counts, but an
//! empty one (a dropped frame) is no packet, so the frame after it is
//! stamped one period later. Keyframes are the index's `AVIIF_KEYFRAME`
//! entries (the first chunk when none is flagged). The legacy `idx1` index
//! is read when present, else the OpenDML one (`indx` → `ix##`); a file
//! with neither is left to the FFmpeg module.
//!
//! Callers read the file: [`parse_header`] takes its first bytes
//! ([`HEADER_LEN`] is enough for every file seen), [`Avi::idx1_at`] says
//! where the legacy index would be, [`Avi::odml`] where the OpenDML ones are.

/// Bytes to read from the start of the file for [`parse_header`].
pub const HEADER_LEN: usize = 256 * 1024;

/// `idx1` and `ix##` entries flagging a keyframe.
const AVIIF_KEYFRAME: u32 = 0x10;

/// The first video stream of an AVI.
#[derive(Clone, Debug, PartialEq)]
pub struct Avi {
    /// Its stream number (chunk ids `##dc`, `##db`).
    pub stream: u32,
    /// Seconds per frame: `scale / rate`.
    pub scale: u32,
    pub rate: u32,
    /// `strh`'s start: the first chunk's frame number (FFmpeg counts from
    /// it, and plays from time 0, so the movie starts that late).
    pub start: u32,
    pub width: u32,
    pub height: u32,
    /// `strf`'s compression code (`H264`, `XVID`, …).
    pub compression: [u8; 4],
    /// Bytes after `strf`'s `BITMAPINFOHEADER` (for H.264, the parameter
    /// sets, Annex B or `avcC`).
    pub extradata: Vec<u8>,
    /// File offset of the `movi` list's fourcc, and of its end.
    pub movi: (u64, u64),
    /// The OpenDML super index: `(offset, size)` of each `ix##` chunk.
    pub odml: Vec<(u64, u32)>,
}

/// One chunk of the video stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// File offset of the chunk's data.
    pub offset: u64,
    pub size: u32,
    pub key: bool,
    /// Its frame number: its time is `frame × scale / rate`.
    pub frame: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AviError {
    NotAvi,
    /// The header goes on past the bytes given (or is cut short).
    Truncated,
    NoVideo,
}

fn u16_le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_le(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn fourcc(b: &[u8], at: usize) -> Option<[u8; 4]> {
    b.get(at..at + 4)?.try_into().ok()
}

/// RIFF chunks of `b[at..end]`: `(id, data start, declared data end)`;
/// the end may lie past `b` (a list read only in part).
fn chunks(b: &[u8], mut at: usize, end: usize) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    while at + 8 <= end.min(b.len()) && out.len() < 4096 {
        let (Some(id), Some(size)) = (fourcc(b, at), u32_le(b, at + 4)) else {
            break;
        };
        let data = at + 8;
        let Some(data_end) = data.checked_add(size as usize) else {
            break;
        };
        out.push((id, data, data_end));
        // Chunks are padded to an even size.
        let Some(next) = data_end.checked_add(size as usize & 1) else {
            break;
        };
        at = next;
    }
    out
}

/// The stream number in a chunk id `##xx`.
fn stream_of(id: [u8; 4]) -> Option<u32> {
    let digit = |c: u8| c.is_ascii_digit().then(|| u32::from(c - b'0'));
    Some(digit(id[0])? * 10 + digit(id[1])?)
}

/// Whether `id` is a video chunk of `stream` (`##dc` or `##db`).
fn is_video_chunk(id: [u8; 4], stream: u32) -> bool {
    stream_of(id) == Some(stream) && id[2] == b'd' && (id[3] == b'c' || id[3] == b'b')
}

/// The first video stream's header, from the file's first bytes (through
/// the start of `movi`).
pub fn parse_header(head: &[u8]) -> Result<Avi, AviError> {
    if !head.starts_with(b"RIFF") || head.get(8..12) != Some(b"AVI ") {
        return Err(AviError::NotAvi);
    }
    let mut video = None;
    let mut movi = None;
    for (id, start, end) in chunks(head, 12, usize::MAX) {
        if &id != b"LIST" {
            continue;
        }
        match head.get(start..start + 4) {
            Some(b"hdrl") if end <= head.len() => {
                video = video.or_else(|| hdrl(head, start + 4, end));
            }
            Some(b"hdrl") => return Err(AviError::Truncated),
            Some(b"movi") => {
                movi = Some((start as u64, end as u64));
                break;
            }
            _ => {}
        }
    }
    let (Some(mut video), Some(movi)) = (video, movi) else {
        return Err(if movi.is_none() {
            AviError::Truncated
        } else {
            AviError::NoVideo
        });
    };
    video.movi = movi;
    Ok(video)
}

/// The first video stream in a `hdrl` list.
fn hdrl(b: &[u8], start: usize, end: usize) -> Option<Avi> {
    let mut stream = 0;
    for (id, s, e) in chunks(b, start, end) {
        if &id != b"LIST" || b.get(s..s + 4) != Some(b"strl") {
            continue;
        }
        let parts = chunks(b, s + 4, e);
        let find = |name: &[u8; 4]| parts.iter().find(|(id, ..)| id == name).copied();
        let this = stream;
        stream += 1;
        let Some((_, h, he)) = find(b"strh") else {
            continue;
        };
        if b.get(h..h + 4) != Some(b"vids") || he < h + 36 {
            continue;
        }
        let (_, f, fe) = find(b"strf")?;
        // A `BITMAPINFOHEADER` is 40 bytes; FFmpeg takes whatever follows
        // as the extradata (`avidec.c`), whatever `biSize` says.
        if fe < f + 40 {
            continue;
        }
        let extradata = b
            .get(f + 40..fe.min(b.len()))
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        let mut odml = Vec::new();
        if let Some((_, x, xe)) = find(b"indx") {
            // An index of indexes (`bIndexType` 0) of 4-long entries.
            let entries = u32_le(b, x + 4)? as usize;
            if u16_le(b, x)? == 4 && b.get(x + 3) == Some(&0) {
                for i in 0..entries {
                    let at = x + 24 + 16 * i;
                    if at + 16 > xe {
                        break;
                    }
                    odml.push((u64_le(b, at)?, u32_le(b, at + 8)?));
                }
            }
        }
        let (scale, rate) = (u32_le(b, h + 20)?, u32_le(b, h + 24)?);
        let start = u32_le(b, h + 28)?;
        // FFmpeg ignores a start past an hour.
        let start = if scale > 0 && u64::from(start) * u64::from(scale) > 3600 * u64::from(rate) {
            0
        } else {
            start
        };
        return Some(Avi {
            stream: this,
            scale,
            rate,
            start,
            width: u32_le(b, f + 4)?,
            height: (u32_le(b, f + 8)? as i32).unsigned_abs(),
            compression: fourcc(b, f + 16)?,
            extradata,
            movi: (0, 0),
            odml,
        });
    }
    None
}

impl Avi {
    /// Seconds per frame.
    pub fn frame_time(&self) -> f64 {
        if self.scale > 0 && self.rate > 0 {
            f64::from(self.scale) / f64::from(self.rate)
        } else {
            1.0 / 30.0
        }
    }

    /// Where the legacy index's chunk header would be: right after `movi`.
    pub fn idx1_at(&self) -> u64 {
        self.movi.1 + (self.movi.1 & 1)
    }

    /// Looking for the legacy index after `movi`: given the 8-byte chunk
    /// header read at `at`, where to look next (past `JUNK` and other
    /// chunks, as FFmpeg does), or `None` when `at` holds it or the file
    /// has no more chunks.
    pub fn after(&self, at: u64, header: &[u8]) -> Option<u64> {
        let id = fourcc(header, 0)?;
        if &id == b"idx1" || !id.iter().all(|c| c.is_ascii_graphic() || *c == b' ') {
            return None;
        }
        let size = u64::from(u32_le(header, 4)?);
        Some(at + 8 + size + (size & 1))
    }

    /// The video chunks of a legacy index, `data` being the `idx1` chunk
    /// with its header.
    pub fn parse_idx1(&self, data: &[u8]) -> Vec<Chunk> {
        if data.get(..4) != Some(b"idx1") {
            return Vec::new();
        }
        let size = u32_le(data, 4).unwrap_or(0) as usize;
        let entries = data
            .get(8..size.saturating_add(8).min(data.len()))
            .unwrap_or_default();
        let mut out = Vec::new();
        let mut frame = self.start;
        let mut any_key = false;
        let mut last_pos = None;
        // Offsets are from the `movi` fourcc, or (some muxers) from the
        // start of the file: the first entry tells (FFmpeg's `data_offset`
        // compares it with the first chunk the same way).
        let mut base = None;
        for e in entries.as_chunks::<16>().0 {
            let (Some(id), Some(flags), Some(pos), Some(len)) =
                (fourcc(e, 0), u32_le(e, 4), u32_le(e, 8), u32_le(e, 12))
            else {
                break;
            };
            let base = *base.get_or_insert(if u64::from(pos) < self.movi.0 {
                self.movi.0
            } else {
                0
            });
            // Keyframes of any stream count for FFmpeg's "none flagged".
            any_key |= flags & AVIIF_KEYFRAME != 0;
            if !is_video_chunk(id, self.stream) {
                continue;
            }
            let offset = base + u64::from(pos) + 8;
            // An entry repeating the one before is no packet (FFmpeg).
            if len > 0 && last_pos != Some(offset) {
                out.push(Chunk {
                    offset,
                    size: len,
                    key: flags & AVIIF_KEYFRAME != 0,
                    frame,
                });
                last_pos = Some(offset);
            }
            frame = frame.saturating_add(1);
        }
        if !any_key {
            mark_first_key(&mut out);
        }
        out
    }

    /// The video chunks of one OpenDML standard index, `data` being an
    /// `ix##` chunk with its header, and the frame number after them;
    /// `frame` is the number of the first (`start`, then what the index
    /// before returned).
    pub fn parse_ix(&self, data: &[u8], frame: u32) -> (Vec<Chunk>, u32) {
        let mut out = Vec::new();
        let (Some(id), Some(longs), Some(kind), Some(n), Some(base)) = (
            fourcc(data, 0),
            u16_le(data, 8),
            data.get(11),
            u32_le(data, 12),
            u64_le(data, 20),
        ) else {
            return (out, frame);
        };
        if &id[..2] != b"ix" || longs != 2 || *kind != 1 {
            return (out, frame);
        }
        let mut frame = frame;
        for i in 0..n as usize {
            let at = 32 + 8 * i;
            let (Some(offset), Some(size)) = (u32_le(data, at), u32_le(data, at + 4)) else {
                break;
            };
            // Bit 31 marks a frame that is not a keyframe. An offset that
            // does not leave room for the chunk's header is no packet.
            let len = size & 0x7fff_ffff;
            let at = base.checked_add(u64::from(offset)).filter(|a| *a >= 8);
            if let Some(at) = at.filter(|_| len > 0) {
                out.push(Chunk {
                    offset: at,
                    size: len,
                    key: size & 0x8000_0000 == 0,
                    frame,
                });
            }
            frame = frame.saturating_add(1);
        }
        (out, frame)
    }

    /// Whether `header` (8 bytes read at `chunk.offset - 8`) is that
    /// chunk's header: the index pointed at the stream's data.
    pub fn check_chunk(&self, header: &[u8], chunk: &Chunk) -> bool {
        fourcc(header, 0).is_some_and(|id| is_video_chunk(id, self.stream))
            && u32_le(header, 4) == Some(chunk.size)
    }
}

/// With no keyframe flagged, the first chunk is one (FFmpeg does the same).
pub fn mark_first_key(chunks: &mut [Chunk]) {
    if !chunks.iter().any(|c| c.key)
        && let Some(first) = chunks.first_mut()
    {
        first.key = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn list(kind: &[u8; 4], parts: &[Vec<u8>]) -> Vec<u8> {
        let mut data = kind.to_vec();
        for p in parts {
            data.extend(p);
        }
        chunk(b"LIST", &data)
    }

    /// An AVI of an audio stream (0) and a video stream (1), with `frames`
    /// video chunks (`None`: an empty one) and audio between them.
    fn file(frames: &[Option<(&[u8], bool)>], absolute: bool) -> (Vec<u8>, Vec<u8>) {
        let mut strh = b"vids".to_vec();
        strh.extend(b"H264");
        strh.extend([0; 12]);
        strh.extend(1001u32.to_le_bytes());
        strh.extend(30000u32.to_le_bytes());
        strh.extend([0; 24]);
        let mut strf = 40u32.to_le_bytes().to_vec();
        strf.extend(64u32.to_le_bytes());
        strf.extend((-48i32).to_le_bytes());
        strf.extend([1, 0, 24, 0]);
        strf.extend(b"H264");
        strf.extend([0; 20]);
        strf.extend([0, 0, 0, 1, 0x67]);
        let mut auds = b"auds".to_vec();
        auds.extend([0; 52]);
        let hdrl = list(
            b"hdrl",
            &[
                chunk(b"avih", &[0; 56]),
                list(b"strl", &[chunk(b"strh", &auds), chunk(b"strf", &[0; 18])]),
                list(b"strl", &[chunk(b"strh", &strh), chunk(b"strf", &strf)]),
            ],
        );
        let mut movi_data = b"movi".to_vec();
        let mut index = Vec::new();
        let header_len = 12 + hdrl.len() + 8;
        for f in frames {
            let audio = chunk(b"00wb", &[1, 2, 3]);
            index.push((*b"00wb", 0u32, movi_data.len(), 3usize));
            movi_data.extend(&audio);
            let (data, key) = f.unwrap_or((&[], false));
            index.push((
                *b"01dc",
                if key { 0x10 } else { 0 },
                movi_data.len(),
                data.len(),
            ));
            movi_data.extend(chunk(b"01dc", data));
        }
        let movi = chunk(b"LIST", &movi_data);
        let mut idx1 = Vec::new();
        for (id, flags, at, len) in index {
            idx1.extend(id);
            idx1.extend(flags.to_le_bytes());
            let pos = if absolute { header_len + at } else { at };
            idx1.extend((pos as u32).to_le_bytes());
            idx1.extend((len as u32).to_le_bytes());
        }
        let mut body = b"AVI ".to_vec();
        body.extend(&hdrl);
        body.extend(&movi);
        let idx1 = chunk(b"idx1", &idx1);
        body.extend(&idx1);
        (chunk(b"RIFF", &body), idx1)
    }

    #[test]
    fn the_video_stream_and_its_index() {
        let frames: [Option<(&[u8], bool)>; 4] = [
            Some((b"key", true)),
            Some((b"p1", false)),
            None,
            Some((b"p3x", false)),
        ];
        for absolute in [false, true] {
            let (bytes, idx1) = file(&frames, absolute);
            let avi = parse_header(&bytes).unwrap();
            assert_eq!((avi.stream, avi.scale, avi.rate), (1, 1001, 30000));
            assert_eq!((avi.width, avi.height, &avi.compression), (64, 48, b"H264"));
            assert_eq!(avi.extradata, [0, 0, 0, 1, 0x67]);
            let at = avi.idx1_at() as usize;
            assert_eq!(&bytes[at..at + 4], b"idx1");
            let chunks = avi.parse_idx1(&idx1);
            // The empty chunk is no packet but takes a frame number.
            let got: Vec<_> = chunks
                .iter()
                .map(|c| {
                    let o = c.offset as usize;
                    let ok = avi.check_chunk(&bytes[o - 8..o], c);
                    (&bytes[o..o + c.size as usize], c.key, c.frame, ok)
                })
                .collect();
            assert_eq!(
                got,
                vec![
                    (&b"key"[..], true, 0, true),
                    (&b"p1"[..], false, 1, true),
                    (&b"p3x"[..], false, 3, true),
                ],
                "absolute {absolute}"
            );
        }
    }

    #[test]
    fn opendml_indexes_and_bad_input() {
        let (bytes, _) = file(&[Some((b"k", true))], false);
        let avi = parse_header(&bytes).unwrap();
        let mut ix = b"ix01".to_vec();
        ix.extend(0u32.to_le_bytes());
        ix.extend([2, 0, 0, 1]);
        ix.extend(3u32.to_le_bytes());
        ix.extend(b"01dc");
        ix.extend(1000u64.to_le_bytes());
        ix.extend(0u32.to_le_bytes());
        for (off, size) in [(8u32, 5u32), (20, 0x8000_0000), (30, 0x8000_0007)] {
            ix.extend(off.to_le_bytes());
            ix.extend(size.to_le_bytes());
        }
        let (chunks, next) = avi.parse_ix(&ix, 10);
        assert_eq!(next, 13);
        assert_eq!(
            chunks,
            vec![
                Chunk {
                    offset: 1008,
                    size: 5,
                    key: true,
                    frame: 10
                },
                Chunk {
                    offset: 1030,
                    size: 7,
                    key: false,
                    frame: 12
                },
            ]
        );
        assert_eq!(parse_header(b"RIFF\0\0\0\0WAVE"), Err(AviError::NotAvi));
        assert_eq!(parse_header(&bytes[..40]), Err(AviError::Truncated));
        // Cut anywhere, the header never panics.
        for n in 0..bytes.len() {
            let _ = parse_header(&bytes[..n]);
            let _ = avi.parse_idx1(&bytes[n..]);
            let _ = avi.parse_ix(&bytes[n..], 0);
            let _ = avi.after(0, &bytes[n..]);
        }
        // An index without keyframes: the first chunk is one.
        let mut c = [Chunk {
            offset: 0,
            size: 1,
            key: false,
            frame: 0,
        }; 2];
        mark_first_key(&mut c);
        assert!(c[0].key && !c[1].key);
    }

    /// `DDI_MOVIES=<dir> cargo test -p ddi-library local_avis -- --nocapture`
    /// prints each local AVI's video packets, keyframes and frames as this
    /// reader finds them (third-party files stay local), for comparing with
    /// `ffprobe -count_packets`.
    #[test]
    fn local_avis() {
        use std::io::{Read, Seek, SeekFrom};
        let Some(dir) = std::env::var_os("DDI_MOVIES") else {
            return;
        };
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("avi"))
                {
                    out.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(std::path::Path::new(&dir), &mut files);
        files.sort();
        let read = |file: &mut std::fs::File, at: u64, n: usize| {
            let mut b = vec![0; n];
            let got = file
                .seek(SeekFrom::Start(at))
                .and_then(|_| file.read(&mut b));
            b.truncate(got.unwrap_or(0));
            b
        };
        for path in &files {
            let Ok(mut file) = std::fs::File::open(path) else {
                continue;
            };
            let head = read(&mut file, 0, HEADER_LEN);
            let avi = match parse_header(&head) {
                Ok(a) => a,
                Err(e) => {
                    println!("{}\terror {e:?}", path.display());
                    continue;
                }
            };
            // FFmpeg reads the OpenDML index when there is one, else the
            // legacy one (looking past `JUNK` after `movi`).
            let (source, chunks) = if !avi.odml.is_empty() {
                let mut all: Vec<Chunk> = Vec::new();
                let mut frame = avi.start;
                for &(at, size) in &avi.odml {
                    let (part, next) = avi.parse_ix(&read(&mut file, at, size as usize), frame);
                    all.extend(part);
                    frame = next;
                }
                ("odml", all)
            } else {
                let mut at = avi.idx1_at();
                let mut header = read(&mut file, at, 8);
                for _ in 0..16 {
                    match avi.after(at, &header) {
                        Some(next) => {
                            at = next;
                            header = read(&mut file, at, 8);
                        }
                        None => break,
                    }
                }
                if header.starts_with(b"idx1") {
                    let size = u32_le(&header, 4).unwrap_or(0) as usize;
                    ("idx1", avi.parse_idx1(&read(&mut file, at, size + 8)))
                } else {
                    ("none", Vec::new())
                }
            };
            let bad = chunks
                .iter()
                .filter(|c| !avi.check_chunk(&read(&mut file, c.offset.saturating_sub(8), 8), c))
                .count();
            println!(
                "{}\t{source}\t{}\t{}\t{}\t{bad}",
                path.display(),
                chunks.len(),
                chunks.iter().filter(|c| c.key).count(),
                chunks.last().map_or(0, |c| c.frame + 1),
            );
        }
    }
}
