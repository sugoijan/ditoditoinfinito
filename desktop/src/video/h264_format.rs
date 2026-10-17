//! The checked H.264 stream class used by both native container paths.
use ddi_library::h264::{self, Framing, Stream};

pub(super) const MAX_PACKET: usize = 8 * 1024 * 1024;

pub(super) struct Format {
    pub stream: Stream,
    pub sets: Vec<Vec<u8>>,
    pub delay: u32,
}
impl Format {
    pub fn new(extradata: &[u8], first: &[u8]) -> Result<Self, String> {
        let stream = h264::stream(extradata, first).ok_or("no H.264 parameter sets")?;
        let s = &stream.sps;
        let delay = s
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
            for n in h264::nal_units(first, stream.framing)
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
        Ok(Self {
            stream,
            sets,
            delay,
        })
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
