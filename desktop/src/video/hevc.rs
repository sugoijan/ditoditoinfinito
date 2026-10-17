//! Single-layer HEVC Main, one slice per access unit. POC and short-term
//! references reproduce the pinned module's DPB output decisions, including
//! the DPB-capacity bump (not just sorting PTS with a fixed delay).
//! Syntax: H.265 7.3.2/7.3.6/7.3.7, POC 8.3.1; behaviour checked against
//! FFmpeg 9.0.2 hevc/{ps,hevcdec,refs}.c. No FFmpeg code is linked natively.
use super::{bits::Bits, h264_format::MAX_PACKET};

#[derive(Clone)]
struct Rps {
    deltas: Vec<i32>,
}
#[derive(Clone)]
pub(super) struct Format {
    pub sets: Vec<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub delay: u32,
    length: usize,
    dpb: usize,
    poc_bits: u32,
    pps_id: u32,
    rps: Vec<Rps>,
    pub full_range: bool,
    pub matrix: Option<u8>,
}
fn header(n: &[u8]) -> Result<u8, String> {
    if n.len() < 3 || n[0] & 0x81 != 0 || n[1] != 1 {
        return Err("HEVC requires layer zero and temporal id zero".into());
    }
    Ok((n[0] >> 1) & 63)
}
fn units(mut bytes: &[u8], length: usize) -> Result<Vec<&[u8]>, String> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        if out.len() >= 256 {
            return Err("too many HEVC NAL units".into());
        }
        let prefix = bytes.get(..length).ok_or("truncated HEVC NAL length")?;
        let n = prefix
            .iter()
            .fold(0usize, |a, b| (a << 8) | usize::from(*b));
        bytes = &bytes[length..];
        let nal = bytes.get(..n).ok_or("invalid HEVC NAL length")?;
        header(nal)?;
        out.push(nal);
        bytes = &bytes[n..];
    }
    Ok(out)
}
fn rps(b: &mut Bits, previous: &[Rps], inline: bool) -> Result<Rps, String> {
    let mut deltas = Vec::new();
    if !previous.is_empty() && b.flag()? {
        let back = if inline {
            b.ue(previous.len() as u32 - 1)? as usize + 1
        } else {
            1
        };
        let reference = &previous[previous.len() - back];
        let sign = if b.flag()? { -1 } else { 1 };
        let delta = sign * (b.ue(32767)? as i32 + 1);
        for old in reference.deltas.iter().copied().chain(std::iter::once(0)) {
            let used = b.flag()?;
            if used || b.flag()? {
                let n = old.checked_add(delta).ok_or("HEVC RPS overflow")?;
                if n != 0 {
                    deltas.push(n);
                }
            }
        }
    } else {
        let negative = b.ue(16)?;
        let positive = b.ue(16 - negative)?;
        let mut delta = 0i32;
        for _ in 0..negative {
            delta -= b.ue(32767)? as i32 + 1;
            b.flag()?;
            deltas.push(delta);
        }
        delta = 0;
        for _ in 0..positive {
            delta += b.ue(32767)? as i32 + 1;
            b.flag()?;
            deltas.push(delta);
        }
    }
    if deltas.len() > 16 {
        return Err("HEVC reference set exceeds bound".into());
    }
    deltas.sort_by_key(|n| (u8::from(*n > 0), if *n < 0 { -*n } else { *n }));
    if deltas.windows(2).any(|w| w[0] == w[1]) {
        return Err("duplicate HEVC reference".into());
    }
    Ok(Rps { deltas })
}
impl Format {
    pub fn new(config: &[u8]) -> Result<Self, String> {
        if config.len() < 23
            || config[0] != 1
            || config[1] & 31 != 1
            || config[16] & 3 != 1
            || config[17] & 7 != 0
            || config[18] & 7 != 0
        {
            return Err("HEVC requires hvcC Main 8-bit 4:2:0".into());
        }
        let length = usize::from((config[21] & 3) + 1);
        let mut at = 23;
        let mut sets = Vec::new();
        for _ in 0..config[22] {
            let kind = *config.get(at).ok_or("short hvcC")? & 63;
            at += 1;
            let count = read_u16(config, &mut at)?;
            if !matches!(kind, 32..=34 | 39 | 40) || count != 1 {
                return Err("HEVC requires one VPS/SPS/PPS".into());
            }
            let size = read_u16(config, &mut at)?;
            let n = config
                .get(at..at + size)
                .ok_or("short hvcC parameter set")?;
            at += size;
            if header(n)? != kind || sets.iter().any(|s: &Vec<u8>| (s[0] >> 1) & 63 == kind) {
                return Err("invalid HEVC parameter sets".into());
            }
            if matches!(kind, 39 | 40) {
                check_sei(&n[2..])?;
            } else {
                sets.push(n.to_vec());
            }
        }
        if at != config.len() || sets.len() != 3 {
            return Err("HEVC requires exactly three parameter sets".into());
        }
        sets.sort_by_key(|s| s[0]);
        let mut b = Bits::rbsp(&sets[1][2..]);
        let vps_id = b.bits(4)?;
        if b.bits(3)? != 0 || !b.flag()? {
            return Err("HEVC requires one temporal layer".into());
        }
        // profile_tier_level, no sublayers. Require progressive Main.
        if b.bits(2)? != 0 {
            return Err("unsupported HEVC profile space".into());
        }
        b.flag()?;
        if b.bits(5)? != 1 {
            return Err("HEVC requires Main profile".into());
        }
        b.bits(32)?;
        if !b.flag()? || b.flag()? {
            return Err("HEVC requires progressive pictures".into());
        }
        b.bits(32)?;
        b.bits(14)?;
        b.bits(8)?;
        let sps_id = b.ue(15)?;
        if b.ue(3)? != 1 {
            return Err("HEVC requires 4:2:0".into());
        }
        let mut width = b.ue(4096)?;
        let mut height = b.ue(2160)?;
        if b.flag()? {
            let crop_x = (b.ue(2048)? + b.ue(2048)?) * 2;
            let crop_y = (b.ue(1080)? + b.ue(1080)?) * 2;
            width = width.checked_sub(crop_x).ok_or("HEVC crop overflow")?;
            height = height.checked_sub(crop_y).ok_or("HEVC crop overflow")?;
        }
        if width == 0 || height == 0 || b.ue(8)? != 0 || b.ue(8)? != 0 {
            return Err("HEVC requires bounded 8-bit geometry".into());
        }
        let poc_bits = b.ue(12)? + 4;
        b.flag()?; // ordering_info_present: one layer either way.
        let dpb = b.ue(15)? as usize + 1;
        let delay = b.ue((dpb - 1) as u32)?;
        b.ue(u32::MAX)?;
        for _ in 0..6 {
            b.ue(16)?;
        }
        if b.flag()? && b.flag()? {
            return Err("HEVC custom scaling lists unsupported".into());
        }
        b.flag()?;
        b.flag()?;
        if b.flag()? {
            return Err("HEVC PCM unsupported".into());
        }
        let count = b.ue(64)?;
        let mut references = Vec::new();
        for _ in 0..count {
            references.push(rps(&mut b, &references, false)?);
        }
        if b.flag()? {
            return Err("HEVC long-term references unsupported".into());
        }
        b.flag()?;
        b.flag()?;
        let (mut full_range, mut matrix) = (false, None);
        if b.flag()? {
            if b.flag()? {
                let aspect = b.bits(8)?;
                if aspect == 255 {
                    b.bits(32)?;
                }
            }
            if b.flag()? {
                b.flag()?;
            }
            if b.flag()? {
                b.bits(3)?;
                full_range = b.flag()?;
                if b.flag()? {
                    b.bits(8)?;
                    b.bits(8)?;
                    matrix = Some(b.bits(8)? as u8);
                }
            }
            if b.flag()? {
                b.ue(5)?;
                b.ue(5)?;
            }
            b.flag()?;
            if b.flag()? || b.flag()? {
                return Err("HEVC field pictures unsupported".into());
            }
            if b.flag()? {
                return Err("HEVC default display window unsupported".into());
            }
        }
        let mut p = Bits::rbsp(&sets[2][2..]);
        let pps_id = p.ue(63)?;
        if p.ue(15)? != sps_id || p.flag()? || p.flag()? || p.bits(3)? != 0 {
            return Err("HEVC requires a single independent output slice".into());
        }
        let mut v = Bits::rbsp(&sets[0][2..]);
        if v.bits(4)? != vps_id
            || !v.flag()?
            || !v.flag()?
            || v.bits(6)? != 0
            || v.bits(3)? != 0
            || !v.flag()?
        {
            return Err("HEVC VPS/SPS mismatch".into());
        }
        Ok(Self {
            sets,
            width,
            height,
            delay,
            length,
            dpb,
            poc_bits,
            pps_id,
            rps: references,
            full_range,
            matrix,
        })
    }
    pub fn sample(&self, bytes: &[u8]) -> Result<(Vec<u8>, Picture), String> {
        let mut sample = Vec::new();
        let mut picture = None;
        for n in units(bytes, self.length)? {
            let kind = header(n)?;
            match kind {
                32..=34 => {
                    if !self.sets.iter().any(|s| s == n) {
                        return Err("HEVC parameter sets changed".into());
                    }
                }
                35 => continue,
                39 | 40 => check_sei(&n[2..])?,
                0 | 1 | 6..=9 | 19..=21 => {
                    if picture.is_some() {
                        return Err("HEVC requires one slice per packet".into());
                    }
                    let mut b = Bits::rbsp(&n[2..]);
                    if !b.flag()? {
                        return Err("HEVC dependent/additional slice unsupported".into());
                    }
                    let idr = matches!(kind, 19 | 20);
                    let no_prior = if kind >= 16 { b.flag()? } else { false };
                    if b.ue(63)? != self.pps_id {
                        return Err("HEVC slice selected another PPS".into());
                    }
                    let slice_type = b.ue(2)?;
                    if idr && slice_type != 2 {
                        return Err("invalid HEVC IDR slice".into());
                    }
                    let lsb = if idr {
                        0
                    } else {
                        b.bits(self.poc_bits)? as i32
                    };
                    let refs = if idr {
                        Rps { deltas: Vec::new() }
                    } else if !b.flag()? {
                        rps(&mut b, &self.rps, true)?
                    } else {
                        let bits = usize::BITS - (self.rps.len().saturating_sub(1)).leading_zeros();
                        let index = b.bits(bits)? as usize;
                        self.rps.get(index).ok_or("invalid HEVC RPS index")?.clone()
                    };
                    picture = Some(Picture {
                        kind,
                        idr,
                        no_prior,
                        lsb,
                        refs,
                    });
                }
                _ => return Err("unsupported HEVC NAL unit".into()),
            }
            if sample.len().saturating_add(n.len() + 4) > MAX_PACKET {
                return Err("oversized HEVC sample".into());
            }
            sample.extend_from_slice(&(n.len() as u32).to_be_bytes());
            sample.extend_from_slice(n);
        }
        Ok((sample, picture.ok_or("HEVC packet contains no picture")?))
    }
}
fn read_u16(b: &[u8], at: &mut usize) -> Result<usize, String> {
    let n = u16::from_be_bytes(b.get(*at..*at + 2).ok_or("short hvcC")?.try_into().unwrap());
    *at += 2;
    Ok(usize::from(n))
}
fn check_sei(n: &[u8]) -> Result<(), String> {
    // User-data-unregistered (encoder labels) only; recovery, timing and
    // film-grain messages could change the module's output model/pixels.
    let mut data = Vec::new();
    let mut zeros = 0;
    for &v in n {
        if zeros >= 2 && v == 3 {
            zeros = 0;
            continue;
        }
        zeros = if v == 0 { zeros + 1 } else { 0 };
        data.push(v);
    }
    let mut at = 0;
    while at < data.len() {
        if data[at] == 128 && data[at + 1..].iter().all(|n| *n == 0) {
            return Ok(());
        }
        let mut value = || -> Result<usize, String> {
            let mut x = 0;
            loop {
                let n = *data.get(at).ok_or("short HEVC SEI")?;
                at += 1;
                x += usize::from(n);
                if n != 255 {
                    return Ok(x);
                }
            }
        };
        let kind = value()?;
        let size = value()?;
        if kind != 5 || size < 16 {
            return Err("unsupported HEVC SEI".into());
        }
        at = at
            .checked_add(size)
            .filter(|a| *a <= data.len())
            .ok_or("short HEVC SEI payload")?;
    }
    Err("HEVC SEI lacks trailing bits".into())
}
pub(super) struct Picture {
    kind: u8,
    idr: bool,
    no_prior: bool,
    lsb: i32,
    refs: Rps,
}
struct Entry {
    id: usize,
    poc: i32,
    reference: bool,
    pending: bool,
}
#[derive(Default)]
pub(super) struct Order {
    previous: i32,
    frames: Vec<Entry>,
}
impl Order {
    pub fn push(&mut self, f: &Format, p: Picture, id: usize) -> Result<Vec<usize>, String> {
        if id == 0 && !p.idr {
            return Err("HEVC seek did not land on an IDR".into());
        }
        let mut out = Vec::new();
        let max = 1i32 << f.poc_bits;
        let previous_lsb = self.previous % max;
        let mut msb = self.previous - previous_lsb;
        if p.lsb < previous_lsb && previous_lsb - p.lsb >= max / 2 {
            msb = msb.checked_add(max).ok_or("HEVC POC overflow")?;
        } else if p.lsb > previous_lsb && p.lsb - previous_lsb > max / 2 {
            msb = msb.checked_sub(max).ok_or("HEVC POC overflow")?;
        }
        let poc = if p.idr {
            0
        } else {
            msb.checked_add(p.lsb).ok_or("HEVC POC overflow")?
        };
        if p.idr {
            if p.no_prior && self.frames.iter().any(|e| e.pending) {
                return Err("HEVC discards prior output".into());
            }
            out.extend(self.drain());
            self.frames.clear();
        }
        let refs: Vec<i32> = p
            .refs
            .deltas
            .iter()
            .map(|d| poc.checked_add(*d).ok_or("HEVC reference POC overflow"))
            .collect::<Result<_, _>>()?;
        if refs.len() + 1 > f.dpb {
            return Err("HEVC references exceed SPS DPB capacity".into());
        }
        if refs
            .iter()
            .any(|n| !self.frames.iter().any(|e| e.poc == *n && e.reference))
        {
            return Err("HEVC missing reference picture".into());
        }
        for e in &mut self.frames {
            e.reference = refs.contains(&e.poc);
        }
        self.frames.retain(|e| e.pending || e.reference);
        if self.frames.iter().any(|e| e.poc == poc) {
            return Err("duplicate HEVC POC".into());
        }
        self.frames.push(Entry {
            id,
            poc,
            reference: true,
            pending: true,
        });
        while self.frames.iter().filter(|e| e.pending).count() > f.delay as usize
            || self.frames.len() > f.dpb
        {
            let Some(i) = self
                .frames
                .iter()
                .enumerate()
                .filter(|(_, e)| e.pending)
                .min_by_key(|(_, e)| e.poc)
                .map(|(i, _)| i)
            else {
                return Err("HEVC DPB cannot bump".into());
            };
            self.frames[i].pending = false;
            out.push(self.frames[i].id);
            self.frames.retain(|e| e.pending || e.reference);
        }
        if !matches!(p.kind, 0 | 6..=9) {
            self.previous = poc;
        }
        Ok(out)
    }
    pub fn drain(&mut self) -> Vec<usize> {
        let mut out: Vec<_> = self
            .frames
            .iter_mut()
            .filter(|e| e.pending)
            .map(|e| {
                e.pending = false;
                (e.poc, e.id)
            })
            .collect();
        out.sort_by_key(|e| e.0);
        out.into_iter().map(|e| e.1).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn format(delay: u32, dpb: usize) -> Format {
        Format {
            sets: vec![],
            width: 16,
            height: 16,
            delay,
            length: 4,
            dpb,
            poc_bits: 8,
            pps_id: 0,
            rps: vec![],
            full_range: false,
            matrix: None,
        }
    }
    fn picture(poc: i32, idr: bool, refs: &[i32]) -> Picture {
        Picture {
            kind: if idr { 20 } else { 1 },
            idr,
            no_prior: false,
            lsb: poc,
            refs: Rps {
                deltas: refs.to_vec(),
            },
        }
    }
    #[test]
    fn dpb_capacity_can_release_before_the_fixed_reorder_delay() {
        let f = format(2, 3);
        let mut order = Order::default();
        assert!(order.push(&f, picture(0, true, &[]), 0).unwrap().is_empty());
        assert!(
            order
                .push(&f, picture(3, false, &[-3]), 1)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            order.push(&f, picture(1, false, &[-1, 2]), 2).unwrap(),
            vec![0]
        );
        // Picture 0 remains a reference; picture 1 is output-only. Adding
        // a fourth picture bumps 1 with only two outputs still pending.
        assert_eq!(
            order.push(&f, picture(2, false, &[-2, 1]), 3).unwrap(),
            vec![2]
        );
        assert_eq!(order.drain(), vec![3, 1]);
    }
    #[test]
    fn idr_drains_prior_sequence_and_resets_picture_order() {
        let f = format(2, 4);
        let mut order = Order::default();
        order.push(&f, picture(0, true, &[]), 0).unwrap();
        order.push(&f, picture(3, false, &[-3]), 1).unwrap();
        assert_eq!(
            order.push(&f, picture(0, true, &[]), 2).unwrap(),
            vec![0, 1]
        );
        assert_eq!(order.drain(), vec![2]);
    }
    #[test]
    fn malformed_headers_cannot_escape_the_packet_limits() {
        for bad in [&[][..], &[0], &[0, 0, 0, 0], &[0, 0, 0, 20, 2, 1, 128]] {
            assert!(units(bad, 4).is_err() || bad.is_empty());
        }
        assert!(
            Order::default()
                .push(&format(0, 2), picture(2, false, &[]), 0)
                .is_err()
        );
        assert!(check_sei(&[6, 1, 0, 128]).is_err()); // Recovery point.
        assert!(check_sei(&[5, 255, 255]).is_err());
        for n in 0..128 {
            assert!(Format::new(&vec![255; n]).is_err());
        }
    }
}

#[cfg(test)]
mod ordering_edges {
    use super::*;
    #[test]
    fn predicted_reference_sets_keep_unused_following_references() {
        let previous = [Rps {
            deltas: vec![-1, -3, 2],
        }];
        // Prediction from set 0, delta=-1; all retained, with -4 unused.
        let mut bits = Bits::raw(&[0b11111011, 0b10000000]);
        assert_eq!(
            rps(&mut bits, &previous, true).unwrap().deltas,
            [-1, -2, -4, 1]
        );
    }
}
