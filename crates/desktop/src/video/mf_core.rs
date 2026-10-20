//! Windows decoder's portable checks. No COM, device or clock here.
use std::collections::{BTreeMap, VecDeque};
const LIMIT: usize = 32;
const BYTE_LIMIT: usize = 128 * 1024 * 1024;
#[derive(Default)]
pub(super) struct Flow {
    pub inputs: usize,
    pub outputs: usize,
    pub draining: bool,
    pub complete: bool,
}
pub(super) enum Signal {
    Input,
    Output,
    Complete,
}
impl Flow {
    pub fn event(&mut self, e: Signal) -> Result<(), String> {
        match e {
            Signal::Input => {
                if self.inputs == LIMIT {
                    return Err("MF input credit overflow".into());
                }
                self.inputs += 1;
            }
            Signal::Output => {
                if self.complete || self.outputs == LIMIT {
                    return Err("MF output credit overflow".into());
                }
                self.outputs += 1;
            }
            Signal::Complete => {
                if !self.draining || self.complete {
                    return Err("unexpected MF drain completion".into());
                }
                self.complete = true;
            }
        }
        Ok(())
    }
    pub fn input(&mut self) -> Result<(), String> {
        if self.draining || self.inputs == 0 {
            return Err("MF input without a credit".into());
        }
        self.inputs -= 1;
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct Ids {
    pending: BTreeMap<i64, usize>,
    recent: VecDeque<i64>,
    floor: Option<i64>,
}
impl Ids {
    pub fn insert(&mut self, seconds: Option<f64>, id: usize) -> Result<i64, String> {
        let n = seconds.ok_or("MF requires packet PTS for output identity")? * 10_000_000.0;
        let n = n.round();
        if !n.is_finite() || n < i64::MIN as f64 || n >= i64::MAX as f64 {
            return Err("MF packet PTS outside 100 ns range".into());
        }
        let stamp = n as i64;
        if self.pending.len() >= LIMIT
            || self.floor.is_some_and(|f| stamp <= f)
            || self.pending.contains_key(&stamp)
            || self.recent.contains(&stamp)
        {
            return Err("MF ambiguous PTS or pending identity bound".into());
        }
        self.pending.insert(stamp, id);
        Ok(stamp)
    }
    pub fn output(&mut self, stamp: i64) -> Result<usize, String> {
        let id = self
            .pending
            .remove(&stamp)
            .ok_or("MF output has an unknown or duplicate sample time")?;
        self.recent.push_back(stamp);
        if self.recent.len() > 64 {
            let old = self.recent.pop_front().unwrap();
            self.floor = Some(self.floor.map_or(old, |n| n.max(old)));
        }
        Ok(id)
    }
    pub fn empty(&self) -> bool {
        self.pending.is_empty()
    }
}
/// Full NV12 texture span. The UV plane starts after all allocated luma
/// rows, including padding, not after the visible picture's last row.
pub(super) fn span(stride: usize, rows: usize) -> Result<usize, String> {
    if stride == 0 || stride > 32768 || rows == 0 || rows > 4096 || !rows.is_multiple_of(2) {
        return Err("MF unsupported NV12 allocation".into());
    }
    stride
        .checked_mul(rows + rows / 2)
        .filter(|n| *n <= BYTE_LIMIT)
        .ok_or("MF NV12 byte bound".into())
}
pub(super) fn nv12(
    bytes: &[u8],
    stride: usize,
    rows: usize,
    width: usize,
    height: usize,
) -> Result<Vec<u8>, String> {
    let len = span(stride, rows)?;
    if width == 0
        || width > 4096
        || height == 0
        || height > 2160
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
        || width > stride
        || height > rows
        || bytes.len() < len
    {
        return Err("MF invalid NV12 plane layout".into());
    }
    let y = width * height;
    let c = y / 4;
    let mut out = vec![0; y + 2 * c];
    for row in 0..height {
        out[row * width..(row + 1) * width]
            .copy_from_slice(&bytes[row * stride..row * stride + width]);
    }
    // The interleaved chroma rows (`span` holds `rows / 2` of them).
    let (u, v) = out[y..].split_at_mut(c);
    let cw = width / 2;
    for ((src, u), v) in bytes[stride * rows..]
        .chunks(stride)
        .zip(u.chunks_exact_mut(cw))
        .zip(v.chunks_exact_mut(cw))
    {
        for ((&[cb, cr], u), v) in src[..width].as_chunks::<2>().0.iter().zip(u).zip(v) {
            (*u, *v) = (cb, cr);
        }
    }
    Ok(out)
}
/// The shared codec builder supplies four-byte NAL lengths. MFT H.264/HEVC
/// input uses Annex B, with the checked sets repeated at each fresh session.
pub(super) fn annex_b(bytes: &[u8], sets: &[Vec<u8>], first: bool) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut push = |n: &[u8]| -> Result<(), String> {
        if n.is_empty() || out.len().saturating_add(4 + n.len()) > 8 * 1024 * 1024 {
            return Err("MF compressed sample bound".into());
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(n);
        Ok(())
    };
    if first {
        for set in sets {
            push(set)?;
        }
    }
    let mut at = 0;
    while at < bytes.len() {
        let n = u32::from_be_bytes(
            bytes
                .get(at..at + 4)
                .ok_or("MF truncated NAL length")?
                .try_into()
                .unwrap(),
        ) as usize;
        at += 4;
        let end = at.checked_add(n).ok_or("MF NAL length overflow")?;
        push(bytes.get(at..end).ok_or("MF truncated NAL")?)?;
        at = end;
    }
    if out.is_empty() {
        return Err("MF empty sample".into());
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn async_credits_and_drain_are_bounded() {
        let mut f = Flow::default();
        assert!(f.input().is_err());
        assert!(f.event(Signal::Complete).is_err());
        f.event(Signal::Input).unwrap();
        f.input().unwrap();
        f.event(Signal::Output).unwrap();
        assert_eq!(f.outputs, 1);
        f.draining = true;
        assert!(f.input().is_err());
        f.event(Signal::Complete).unwrap();
        assert!(f.event(Signal::Output).is_err());
        let mut f = Flow::default();
        for _ in 0..LIMIT {
            f.event(Signal::Input).unwrap();
        }
        assert!(f.event(Signal::Input).is_err());
    }
    #[test]
    fn pts_correlates_reordered_pictures_and_refuses_ambiguity() {
        let mut ids = Ids::default();
        let a = ids.insert(Some(-0.1), 0).unwrap();
        let b = ids.insert(Some(0.1), 1).unwrap();
        let c = ids.insert(Some(0.0), 2).unwrap();
        assert_eq!(ids.output(a).unwrap(), 0);
        assert_eq!(ids.output(c).unwrap(), 2);
        assert_eq!(ids.output(b).unwrap(), 1);
        assert!(ids.empty());
        assert!(ids.insert(Some(0.1), 3).is_err());
        assert!(ids.output(c).is_err());
        for value in [None, Some(f64::NAN), Some(f64::INFINITY), Some(1e30)] {
            assert!(ids.insert(value, 3).is_err());
        }
        for id in 0..LIMIT {
            ids.insert(Some(id as f64 + 1.0), id).unwrap();
        }
        assert!(ids.insert(Some(100.0), 100).is_err());
    }
    #[test]
    fn pts_quantization_and_evicted_history_cannot_reuse_an_identity() {
        let mut ids = Ids::default();
        let stamp = ids.insert(Some(0.00000011), 0).unwrap();
        assert!(ids.insert(Some(0.00000014), 1).is_err());
        ids.output(stamp).unwrap();
        for id in 1..100 {
            let stamp = ids.insert(Some(id as f64), id).unwrap();
            assert_eq!(ids.output(stamp).unwrap(), id);
        }
        assert!(ids.empty());
        assert!(ids.insert(Some(0.00000011), 100).is_err());
        assert!(ids.insert(Some(10.0), 100).is_err());
        // A new session/seek owns a fresh identity namespace.
        assert!(Ids::default().insert(Some(0.00000011), 0).is_ok());
    }
    #[test]
    fn nv12_honours_row_pitch_and_allocated_luma_rows() {
        let mut bytes = vec![99; 24];
        bytes[0..2].copy_from_slice(&[1, 2]);
        bytes[4..6].copy_from_slice(&[3, 4]);
        bytes[16..18].copy_from_slice(&[5, 6]);
        assert_eq!(nv12(&bytes, 4, 4, 2, 2).unwrap(), [1, 2, 3, 4, 5, 6]);
        for args in [(1, 4, 2, 2), (4, 2, 4, 4), (4, 4, 3, 2), (32769, 4, 2, 2)] {
            assert!(nv12(&bytes, args.0, args.1, args.2, args.3).is_err());
        }
        assert!(nv12(&bytes[..23], 4, 4, 2, 2).is_err());
        assert!(span(32768, 4096).is_err());
    }
    #[test]
    fn checked_sets_and_packet_lengths_become_annex_b() {
        assert_eq!(
            annex_b(&[0, 0, 0, 1, 0x65], &[vec![0x67], vec![0x68]], true).unwrap(),
            [0, 0, 0, 1, 0x67, 0, 0, 0, 1, 0x68, 0, 0, 0, 1, 0x65]
        );
        for bad in [&[][..], &[0, 0], &[0, 0, 0, 0], &[0, 0, 0, 10, 0x65]] {
            assert!(annex_b(bad, &[], false).is_err());
        }
    }
}
