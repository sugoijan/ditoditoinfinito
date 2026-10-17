//! Bounded, range-relative AVI reads for the OS decoder. The sans-IO
//! parser and H.264 timing model are shared with WebCodecs; packets stay
//! on disk (including stored zip entries), never a whole movie in memory.

use super::{
    MovieFile,
    h264_format::{Format, MAX_PACKET},
};
use ddi_library::{
    avi::{self, Avi, Chunk},
    h264,
};
use std::io::{Read, Seek, SeekFrom};

const MAX_INDEX: usize = 32 * 1024 * 1024;
const MAX_CHUNKS: usize = 1_000_000;

pub(super) struct Media {
    reader: Reader,
    pub avi: Avi,
    pub chunks: Vec<Chunk>,
}

impl Media {
    pub fn open(source: &MovieFile) -> Result<(Self, Format), String> {
        let file = std::fs::File::open(&source.path).map_err(|e| e.to_string())?;
        let file_size = file.metadata().map_err(|e| e.to_string())?.len();
        let (base, size) = match source.range {
            Some((base, size)) if base.checked_add(size).is_some_and(|end| end <= file_size) => {
                (base, size)
            }
            Some(_) => return Err("movie range is outside its file".into()),
            None => (0, file_size),
        };
        let mut reader = Reader { file, base, size };
        let avi = avi::parse_header(&reader.read(0, size.min(avi::HEADER_LEN as u64) as usize)?)
            .map_err(|e| format!("not an indexed AVI: {e:?}"))?;
        let mut chunks = Vec::new();
        if !matches!(
            &avi.compression,
            b"H264" | b"h264" | b"X264" | b"x264" | b"avc1" | b"AVC1"
        ) {
            return Err("not H.264 AVI".into());
        }
        if avi.rate == 0 || avi.scale == 0 {
            return Err("invalid AVI frame rate".into());
        }
        let mut budget = MAX_INDEX;
        if !avi.odml.is_empty() {
            let mut frame = avi.start;
            for (at, size) in avi.odml.clone() {
                let size = size as usize;
                budget = budget.checked_sub(size).ok_or("AVI index exceeds 32 MiB")?;
                let bytes = reader.read(at, size)?;
                let (part, next) = avi.parse_ix(&bytes, frame);
                chunks.extend(part);
                frame = next;
            }
            avi::mark_first_key(&mut chunks);
        } else {
            let mut at = avi.idx1_at();
            for _ in 0..16 {
                let header = reader.read(at, 8)?;
                if header.starts_with(b"idx1") {
                    let size = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
                    if size > MAX_INDEX - 8 {
                        return Err("AVI index exceeds 32 MiB".into());
                    }
                    let bytes = reader.read(at, size + 8)?;
                    chunks = avi.parse_idx1(&bytes);
                    break;
                }
                at = avi.after(at, &header).ok_or("AVI has no index")?;
            }
        }
        if chunks.is_empty() || chunks.len() > MAX_CHUNKS {
            return Err("empty or oversized AVI index".into());
        }
        if chunks.windows(2).any(|w| w[1].frame <= w[0].frame) {
            return Err("non-increasing AVI frame index".into());
        }
        for c in &chunks {
            if c.offset < 8
                || c.size as usize > MAX_PACKET
                || c.offset
                    .checked_add(u64::from(c.size))
                    .is_none_or(|end| end > reader.size)
            {
                return Err("AVI index points outside the movie or has an oversized packet".into());
            }
        }
        let first = packet(&mut reader, &avi, &chunks, 0)?;
        let mut format = Format::new(&avi.extradata, &first)?;
        // Match FFmpeg's early B-slice probe even if the SPS declares no delay.
        if format.delay == 0 {
            let mut order = h264::PictureOrder::new(&format.stream);
            for i in 0..chunks.len().min(64) {
                let bytes = packet(&mut reader, &avi, &chunks, i)?;
                if order
                    .picture(&bytes, format.stream.framing, &format.stream.sps)
                    .is_some_and(|p| p.b)
                {
                    format.delay = 1;
                    break;
                }
            }
        }
        Ok((
            Self {
                reader,
                avi,
                chunks,
            },
            format,
        ))
    }

    pub fn packet(&mut self, i: usize) -> Result<Vec<u8>, String> {
        packet(&mut self.reader, &self.avi, &self.chunks, i)
    }
}

struct Reader {
    file: std::fs::File,
    base: u64,
    size: u64,
}
impl Reader {
    fn read(&mut self, at: u64, size: usize) -> Result<Vec<u8>, String> {
        if at
            .checked_add(size as u64)
            .is_none_or(|end| end > self.size)
        {
            return Err("read outside the movie".into());
        }
        let mut bytes = vec![0; size];
        self.file
            .seek(SeekFrom::Start(self.base + at))
            .and_then(|_| self.file.read_exact(&mut bytes))
            .map_err(|e| e.to_string())?;
        Ok(bytes)
    }
}

fn packet(reader: &mut Reader, avi: &Avi, chunks: &[Chunk], i: usize) -> Result<Vec<u8>, String> {
    let c = *chunks.get(i).ok_or("packet outside index")?;
    let mut bytes = reader.read(
        c.offset.checked_sub(8).ok_or("packet has no header")?,
        c.size as usize + 8,
    )?;
    if !avi.check_chunk(&bytes, &c) {
        return Err("AVI index does not point at its packet".into());
    }
    bytes.drain(..8);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range_reads_refuse_overflow_and_reads_into_neighbouring_entries() {
        let path = std::env::temp_dir().join(format!("ddi-vt-range-{}", std::process::id()));
        std::fs::write(&path, [0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
        let file = std::fs::File::open(&path).unwrap();
        let mut reader = Reader {
            file,
            base: 2,
            size: 4,
        };
        assert_eq!(reader.read(0, 4).unwrap(), [2, 3, 4, 5]);
        assert!(reader.read(3, 2).is_err());
        assert!(reader.read(u64::MAX, 2).is_err());
        assert!(
            Media::open(&MovieFile {
                path: path.clone(),
                range: Some((u64::MAX, 2))
            })
            .is_err()
        );
        std::fs::remove_file(path).unwrap();
    }
}
