//! Bounded, range-relative AVI reads for the OS decoder. The sans-IO
//! parser and H.264 timing model are shared with WebCodecs; packets stay
//! on disk (including stored zip entries), never a whole movie in memory.

use super::MovieFile;
use ddi_library::{
    avi::{self, Avi, Chunk},
    h264::{self, Framing, Stream},
};
use std::io::{Read, Seek, SeekFrom};

const MAX_INDEX: usize = 32 * 1024 * 1024;
pub(super) const MAX_PACKET: usize = 8 * 1024 * 1024;
const MAX_CHUNKS: usize = 1_000_000;

pub(super) struct Media {
    reader: Reader,
    pub avi: Avi,
    pub chunks: Vec<Chunk>,
    pub stream: Stream,
    pub sets: Vec<Vec<u8>>,
    pub delay: u32,
}

impl Media {
    pub fn open(source: &MovieFile) -> Result<Self, String> {
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
        let stream = h264::stream(&avi.extradata, &first).ok_or("no H.264 parameter sets")?;
        let s = &stream.sps;
        let mut delay = s
            .reorder_delay()
            .ok_or("H.264 has no fixed reorder delay")?;
        if !s.frame_mbs_only
            || s.chroma_format_idc != 1
            || s.bit_depth != 8
            || !matches!(s.poc_type, 0 | 2)
            || s.width == 0
            || s.height == 0
            || s.width > 4096
            || s.height > 2160
            || delay > 16
        {
            return Err("not bounded progressive 8-bit 4:2:0 H.264 with POC 0/2".into());
        }
        // CM format descriptions take NAL units, independent of packet framing.
        let sets = if let Some(a) = &stream.avcc {
            avcc_sets(a)?
        } else {
            let mut sets: Vec<Vec<u8>> = h264::nal_units(&stream.parameter_sets, Framing::AnnexB)
                .into_iter()
                .map(<[u8]>::to_vec)
                .collect();
            for n in h264::nal_units(&first, stream.framing)
                .into_iter()
                .filter(|n| matches!(n[0] & 31, 7 | 8))
            {
                if !sets.iter().any(|s| s == n) {
                    sets.push(n.to_vec());
                }
            }
            sets
        };
        single_configuration(&sets)?;
        // Match FFmpeg's early B-slice probe even if the SPS declares no delay.
        if delay == 0 {
            let mut order = h264::PictureOrder::new(&stream);
            for i in 0..chunks.len().min(64) {
                let bytes = packet(&mut reader, &avi, &chunks, i)?;
                if order
                    .picture(&bytes, stream.framing, &stream.sps)
                    .is_some_and(|p| p.b)
                {
                    delay = 1;
                    break;
                }
            }
        }
        Ok(Self {
            reader,
            avi,
            chunks,
            stream,
            sets,
            delay,
        })
    }

    pub fn packet(&mut self, i: usize) -> Result<Vec<u8>, String> {
        packet(&mut self.reader, &self.avi, &self.chunks, i)
    }

    /// Four-byte length framing, refusing parameter-set changes instead of
    /// silently letting the OS's timing or colour interpretation change.
    pub fn sample(&self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(bytes.len() + 32);
        for n in h264::nal_units(bytes, self.stream.framing) {
            if matches!(n[0] & 31, 7 | 8) && !self.sets.iter().any(|s| s == n) {
                return Err("H.264 parameter sets changed".into());
            }
            if n[0] & 31 == 9 {
                continue;
            }
            if out.len().saturating_add(n.len() + 4) > MAX_PACKET {
                return Err("oversized H.264 sample".into());
            }
            out.extend_from_slice(&(n.len() as u32).to_be_bytes());
            out.extend_from_slice(n);
        }
        if out.is_empty() {
            return Err("empty H.264 sample".into());
        }
        Ok(out)
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

/// The first SPS is the geometry and timing we validated. Refuse another
/// configuration rather than allowing slices to select an unchecked SPS.
fn single_configuration(sets: &[Vec<u8>]) -> Result<(), String> {
    if sets.len() == 2
        && [7, 8].into_iter().all(|kind| {
            sets.iter()
                .filter(|s| s.first().is_some_and(|b| b & 31 == kind))
                .count()
                == 1
        })
    {
        Ok(())
    } else {
        Err("H.264 requires a single SPS/PPS configuration".into())
    }
}

fn avcc_sets(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    let mut at = 6;
    for group in 0..2 {
        let count = if group == 0 {
            usize::from(*bytes.get(5).ok_or("short avcC")? & 31)
        } else {
            let n = *bytes.get(at).ok_or("short avcC")?;
            at += 1;
            usize::from(n)
        };
        for _ in 0..count {
            let n = u16::from_be_bytes(
                bytes
                    .get(at..at + 2)
                    .ok_or("short avcC")?
                    .try_into()
                    .unwrap(),
            ) as usize;
            at += 2;
            let unit = bytes.get(at..at + n).ok_or("short avcC")?;
            if unit.is_empty() {
                return Err("empty avcC parameter set".into());
            }
            out.push(unit.to_vec());
            at += n;
        }
    }
    Ok(out)
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
    #[test]
    fn alternate_parameter_sets_cannot_select_unchecked_geometry() {
        assert!(single_configuration(&[vec![0x67, 1], vec![0x68, 2]]).is_ok());
        assert!(single_configuration(&[vec![0x67, 1], vec![0x68, 2], vec![0x67, 3]]).is_err());
        assert!(single_configuration(&[vec![0x67, 1], vec![0x67, 2]]).is_err());
        assert!(single_configuration(&[vec![], vec![0x68, 2]]).is_err());
    }
    #[test]
    fn short_or_empty_avcc_sets_are_refused() {
        for n in 0..10 {
            assert!(avcc_sets(&[1; 10][..n]).is_err());
        }
        assert_eq!(
            avcc_sets(&[1, 0, 0, 0, 255, 225, 0, 2, 103, 1, 1, 0, 2, 104, 2]).unwrap(),
            [vec![103, 1], vec![104, 2]]
        );
    }
}
