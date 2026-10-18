//! H.264 for decoders that are not the FFmpeg module (WebCodecs;
//! `docs/plans/video-backgrounds.md`, step 6): the sequence parameter set
//! (size, codec string, color tags, reorder delay), the NAL units of a
//! chunk, and the module's frame times and exact seeks, so a movie plays
//! frame for frame the same through either decoder.
//!
//! Frame times follow the module (StepMania's rule: a frame is stamped
//! with the decode time of the packet that releases it). [`OutputOrder`]
//! replays FFmpeg's choice of which picture a packet releases
//! (`h264_select_output_frame`, 9.0) from each chunk's picture order count
//! ([`PictureOrder`]), so a frame is known by its own chunk whatever order
//! another decoder gives it out in, and gets the module's time. With the
//! SPS's bitstream restriction FFmpeg's delay is `max_num_reorder_frames`
//! from the start; streams without it grow the delay as they go and are
//! left to the module ([`Sps::reorder_delay`] is `None`).

use crate::avi::Chunk;

/// A sequence parameter set, as far as playing the stream needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sps {
    pub profile_idc: u8,
    pub constraints: u8,
    pub level_idc: u8,
    pub chroma_format_idc: u32,
    pub bit_depth: u32,
    pub frame_mbs_only: bool,
    /// The picture size after cropping.
    pub width: u32,
    pub height: u32,
    /// VUI `matrix_coefficients`, when given.
    pub matrix: Option<u8>,
    pub full_range: bool,
    /// VUI `max_num_reorder_frames`, when the bitstream restriction is
    /// given.
    pub reorder: Option<u32>,
    pub separate_planes: bool,
    pub log2_max_frame_num: u32,
    pub poc_type: u32,
    pub log2_max_poc_lsb: u32,
}

/// An RBSP bit reader (emulation prevention removed first).
struct Bits {
    data: Vec<u8>,
    at: usize,
}

impl Bits {
    fn new(nal: &[u8]) -> Bits {
        let mut data = Vec::with_capacity(nal.len());
        let mut zeros = 0;
        for &b in nal {
            if zeros >= 2 && b == 3 {
                zeros = 0;
                continue;
            }
            zeros = if b == 0 { zeros + 1 } else { 0 };
            data.push(b);
        }
        Bits { data, at: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.at / 8)?;
        let b = (byte >> (7 - self.at % 8)) & 1;
        self.at += 1;
        Some(u32::from(b))
    }

    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Some(v)
    }

    fn flag(&mut self) -> Option<bool> {
        Some(self.bit()? == 1)
    }

    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1u32 << zeros) - 1 + self.bits(zeros)?)
    }

    fn se(&mut self) -> Option<i32> {
        let k = self.ue()?;
        let magnitude = k.div_ceil(2) as i32;
        Some(if k % 2 == 1 { magnitude } else { -magnitude })
    }
}

fn skip_hrd(b: &mut Bits) -> Option<()> {
    let count = b.ue()? + 1;
    b.bits(8)?;
    for _ in 0..count.min(32) {
        b.ue()?;
        b.ue()?;
        b.bit()?;
    }
    b.bits(20)?;
    Some(())
}

/// Parses an SPS NAL unit (with its header byte).
pub fn parse_sps(nal: &[u8]) -> Option<Sps> {
    if nal.first()? & 0x1f != 7 {
        return None;
    }
    let mut b = Bits::new(&nal[1..]);
    let mut sps = Sps {
        profile_idc: b.bits(8)? as u8,
        constraints: b.bits(8)? as u8,
        level_idc: b.bits(8)? as u8,
        chroma_format_idc: 1,
        bit_depth: 8,
        ..Sps::default()
    };
    b.ue()?;
    let mut separate_planes = false;
    // FFmpeg's list (`ff_h264_decode_seq_parameter_set`, 9.0).
    if matches!(
        sps.profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 144
    ) {
        sps.chroma_format_idc = b.ue()?;
        if sps.chroma_format_idc > 3 {
            return None;
        }
        if sps.chroma_format_idc == 3 {
            separate_planes = b.flag()?;
        }
        sps.bit_depth = b.ue()?.checked_add(8).filter(|d| *d <= 14)?;
        b.ue()?;
        b.bit()?;
        if b.flag()? {
            let lists = if sps.chroma_format_idc == 3 { 12 } else { 8 };
            for i in 0..lists {
                if b.flag()? {
                    let size = if i < 6 { 16 } else { 64 };
                    let (mut last, mut next) = (8i64, 8i64);
                    for _ in 0..size {
                        if next != 0 {
                            next = (last + i64::from(b.se()?)).rem_euclid(256);
                        }
                        last = if next == 0 { last } else { next };
                    }
                }
            }
        }
    }
    sps.separate_planes = separate_planes;
    sps.log2_max_frame_num = (b.ue()? + 4).min(16);
    sps.poc_type = b.ue()?;
    match sps.poc_type {
        0 => {
            sps.log2_max_poc_lsb = (b.ue()? + 4).min(16);
        }
        1 => {
            b.bit()?;
            b.se()?;
            b.se()?;
            for _ in 0..b.ue()?.min(256) {
                b.se()?;
            }
        }
        _ => {}
    }
    b.ue()?;
    b.bit()?;
    let width_mbs = b.ue()?.checked_add(1)?;
    let height_units = b.ue()?.checked_add(1)?;
    sps.frame_mbs_only = b.flag()?;
    if !sps.frame_mbs_only {
        b.bit()?;
    }
    b.bit()?;
    let mut crop = [0u32; 4];
    if b.flag()? {
        for c in &mut crop {
            *c = b.ue()?;
        }
    }
    let (sub_w, sub_h): (u32, u32) = match (sps.chroma_format_idc, separate_planes) {
        (0, _) | (3, true) => (1, 1),
        (1, _) => (2, 2),
        (2, _) => (2, 1),
        _ => (1, 1),
    };
    let field: u32 = if sps.frame_mbs_only { 1 } else { 2 };
    let (unit_x, unit_y) = if sps.chroma_format_idc == 0 || separate_planes {
        (1, field)
    } else {
        (sub_w, sub_h * field)
    };
    // FFmpeg rejects a crop as large as the picture.
    let full_w = u64::from(width_mbs) * 16;
    let full_h = u64::from(height_units) * 16 * u64::from(field);
    let crop_w = u64::from(unit_x) * (u64::from(crop[0]) + u64::from(crop[1]));
    let crop_h = u64::from(unit_y) * (u64::from(crop[2]) + u64::from(crop[3]));
    if crop_w >= full_w || crop_h >= full_h || full_w > 1 << 16 || full_h > 1 << 16 {
        return None;
    }
    sps.width = (full_w - crop_w) as u32;
    sps.height = (full_h - crop_h) as u32;
    if !b.flag()? {
        return Some(sps);
    }
    // VUI; a stream cut short inside it keeps what was read.
    let vui = |b: &mut Bits, sps: &mut Sps| -> Option<()> {
        if b.flag()? && b.bits(8)? == 255 {
            b.bits(32)?;
        }
        if b.flag()? {
            b.bit()?;
        }
        if b.flag()? {
            b.bits(3)?;
            sps.full_range = b.flag()?;
            if b.flag()? {
                b.bits(16)?;
                sps.matrix = Some(b.bits(8)? as u8);
            }
        }
        if b.flag()? {
            b.ue()?;
            b.ue()?;
        }
        if b.flag()? {
            b.bits(32)?;
            b.bits(32)?;
            b.bit()?;
        }
        let nal_hrd = b.flag()?;
        if nal_hrd {
            skip_hrd(b)?;
        }
        let vcl_hrd = b.flag()?;
        if vcl_hrd {
            skip_hrd(b)?;
        }
        if nal_hrd || vcl_hrd {
            b.bit()?;
        }
        b.bit()?;
        if b.flag()? {
            b.bit()?;
            for _ in 0..4 {
                b.ue()?;
            }
            let reorder = b.ue()?;
            b.ue()?;
            // FFmpeg refuses more than 16 (and the module then decides as
            // it goes): left to the module.
            sps.reorder = (reorder <= 16).then_some(reorder);
        }
        Some(())
    };
    let _ = vui(&mut b, &mut sps);
    Some(sps)
}

impl Sps {
    /// The WebCodecs codec string, `avc1.PPCCLL`.
    pub fn codec(&self) -> String {
        format!(
            "avc1.{:02X}{:02X}{:02X}",
            self.profile_idc, self.constraints, self.level_idc
        )
    }

    /// How many frames the module's decoder holds back, when it is fixed
    /// from the start (see the module docs).
    pub fn reorder_delay(&self) -> Option<u32> {
        self.reorder
    }

    /// The module's color matrix: the stream's tag, else BT.601 below
    /// 720 lines and BT.709 from there (`guess_colorspace` in the shim).
    pub fn bt709(&self) -> bool {
        match self.matrix {
            Some(1) => true,
            Some(5) | Some(6) => false,
            _ => self.height >= 720,
        }
    }
}

/// How a stream's NAL units are framed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Framing {
    /// Start codes (`00 00 01`).
    AnnexB,
    /// Each unit after its length in this many bytes (`avcC` streams).
    Length(u8),
}

/// The NAL units of `data`.
pub fn nal_units(data: &[u8], framing: Framing) -> Vec<&[u8]> {
    let mut out = Vec::new();
    match framing {
        Framing::AnnexB => {
            let mut starts = Vec::new();
            let mut i = 0;
            while i + 3 <= data.len() {
                if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
                    starts.push(i + 3);
                    i += 3;
                } else {
                    i += 1;
                }
            }
            for (n, &s) in starts.iter().enumerate() {
                let mut end = starts.get(n + 1).map_or(data.len(), |&next| next - 3);
                // Trailing zeros belong to the next start code.
                while end > s && data[end - 1] == 0 {
                    end -= 1;
                }
                if end > s {
                    out.push(&data[s..end]);
                }
            }
        }
        Framing::Length(n) => {
            let n = usize::from(n);
            let mut i = 0;
            while i + n <= data.len() {
                let len = data[i..i + n]
                    .iter()
                    .fold(0usize, |v, &b| (v << 8) | usize::from(b));
                i += n;
                let Some(unit) = i.checked_add(len).and_then(|end| data.get(i..end)) else {
                    break;
                };
                if !unit.is_empty() {
                    out.push(unit);
                }
                i += len;
            }
        }
    }
    out
}

/// What the stream's header says: its SPS, framing, the `avcC` record for
/// a decoder's description (length framing), or the parameter sets in
/// Annex B to put before keyframes (start-code framing).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stream {
    pub sps: Sps,
    pub framing: Framing,
    pub avcc: Option<Vec<u8>>,
    pub parameter_sets: Vec<u8>,
}

/// The stream's parameters from a container's extradata (`avcC`, or SPS
/// and PPS in Annex B), else from its first chunk.
pub fn stream(extradata: &[u8], first_chunk: &[u8]) -> Option<Stream> {
    if extradata.first() == Some(&1) && extradata.len() >= 7 {
        let length = (extradata[4] & 3) + 1;
        let count = usize::from(extradata[5] & 0x1f);
        let mut at = 6;
        let mut sps = None;
        for _ in 0..count {
            let len = usize::from(u16::from_be_bytes(
                extradata.get(at..at + 2)?.try_into().ok()?,
            ));
            let unit = extradata.get(at + 2..at + 2 + len)?;
            sps = sps.or_else(|| parse_sps(unit));
            at += 2 + len;
        }
        return Some(Stream {
            sps: sps?,
            framing: Framing::Length(length),
            avcc: Some(extradata.to_vec()),
            parameter_sets: Vec::new(),
        });
    }
    let sets: Vec<&[u8]> = nal_units(extradata, Framing::AnnexB)
        .into_iter()
        .filter(|n| matches!(n[0] & 0x1f, 7 | 8))
        .collect();
    let sps = sets.iter().find_map(|n| parse_sps(n)).or_else(|| {
        nal_units(first_chunk, Framing::AnnexB)
            .into_iter()
            .find_map(parse_sps)
    })?;
    let mut parameter_sets = Vec::new();
    for unit in sets {
        parameter_sets.extend([0, 0, 0, 1]);
        parameter_sets.extend(unit);
    }
    Some(Stream {
        sps,
        framing: Framing::AnnexB,
        avcc: None,
        parameter_sets,
    })
}

/// Whether a chunk's first picture is an IDR (a point a decoder can start
/// from; WebCodecs needs one after each reset).
pub fn is_idr(chunk: &[u8], framing: Framing) -> bool {
    nal_units(chunk, framing)
        .iter()
        .find(|n| matches!(n[0] & 0x1f, 1 | 5))
        .is_some_and(|n| n[0] & 0x1f == 5)
}

/// A PPS's id and whether its slices carry a bottom field's POC delta.
fn parse_pps(nal: &[u8]) -> Option<(u32, bool)> {
    let mut b = Bits::new(nal.get(1..)?);
    let id = b.ue()?;
    b.ue()?;
    b.bit()?;
    Some((id, b.flag()?))
}

/// A picture's place in display order, from its slice header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PictureInfo {
    pub idr: bool,
    pub poc: i64,
    /// A B slice (FFmpeg's probe holds a frame back for one when the SPS
    /// says none).
    pub b: bool,
}

/// Picture order counts of a stream's chunks in decode order, computed as
/// FFmpeg computes them (`ff_h264_init_poc`, POC types 0 and 2; a
/// memory-management reset is not modelled). Start a new one at each
/// decoder start (a keyframe).
#[derive(Clone, Debug)]
pub struct PictureOrder {
    pps: Vec<(u32, bool)>,
    prev_msb: i64,
    prev_lsb: i64,
    prev_frame_num: u32,
    frame_num_offset: i64,
}

impl PictureOrder {
    /// For a stream whose header held `parameter_sets` (Annex B; the PPSs
    /// of an `avcC` record are read from it too).
    pub fn new(stream: &Stream) -> PictureOrder {
        let mut order = PictureOrder {
            pps: Vec::new(),
            prev_msb: 1 << 16,
            prev_lsb: -1,
            prev_frame_num: 0,
            frame_num_offset: 0,
        };
        for unit in nal_units(&stream.parameter_sets, Framing::AnnexB) {
            order.learn(unit);
        }
        if let Some(avcc) = &stream.avcc {
            for unit in avcc_units(avcc) {
                order.learn(unit);
            }
        }
        order
    }

    fn learn(&mut self, unit: &[u8]) {
        if unit[0] & 0x1f == 8
            && let Some((id, bottom)) = parse_pps(unit)
        {
            self.pps.retain(|(i, _)| *i != id);
            self.pps.push((id, bottom));
        }
    }

    /// The picture in `chunk` (decode order), or `None` when it has no
    /// slice this can read.
    pub fn picture(&mut self, chunk: &[u8], framing: Framing, sps: &Sps) -> Option<PictureInfo> {
        let units = nal_units(chunk, framing);
        for unit in &units {
            self.learn(unit);
        }
        let slice = units.iter().find(|n| matches!(n[0] & 0x1f, 1 | 5))?;
        let idr = slice[0] & 0x1f == 5;
        let reference = slice[0] & 0x60 != 0;
        let mut b = Bits::new(&slice[1..]);
        b.ue()?;
        let is_b = b.ue()? % 5 == 1;
        let pps = b.ue()?;
        let bottom = self
            .pps
            .iter()
            .find(|(i, _)| *i == pps)
            .is_some_and(|p| p.1);
        if sps.separate_planes {
            b.bits(2)?;
        }
        let frame_num = b.bits(sps.log2_max_frame_num)?;
        if !sps.frame_mbs_only && b.flag()? {
            return None;
        }
        if idr {
            b.ue()?;
            self.prev_msb = 1 << 16;
            self.prev_lsb = -1;
            self.prev_frame_num = 0;
            self.frame_num_offset = 0;
        }
        if frame_num < self.prev_frame_num {
            self.frame_num_offset += 1i64 << sps.log2_max_frame_num;
        }
        let poc = match sps.poc_type {
            0 => {
                let lsb = i64::from(b.bits(sps.log2_max_poc_lsb)?);
                let max = 1i64 << sps.log2_max_poc_lsb;
                if self.prev_lsb < 0 {
                    self.prev_lsb = lsb;
                }
                let msb = if lsb < self.prev_lsb && self.prev_lsb - lsb >= max / 2 {
                    self.prev_msb + max
                } else if lsb > self.prev_lsb && self.prev_lsb - lsb < -max / 2 {
                    self.prev_msb - max
                } else {
                    self.prev_msb
                };
                let top = msb + lsb;
                let delta = if bottom { i64::from(b.se()?) } else { 0 };
                if reference {
                    self.prev_msb = msb;
                    self.prev_lsb = lsb;
                }
                top.min(top + delta)
            }
            2 => {
                let n = self.frame_num_offset + i64::from(frame_num);
                if idr {
                    0
                } else if reference {
                    2 * n
                } else {
                    2 * n - 1
                }
            }
            _ => return None,
        };
        self.prev_frame_num = frame_num;
        Some(PictureInfo { idr, poc, b: is_b })
    }
}

/// The NAL units of an `avcC` record's parameter sets.
fn avcc_units(avcc: &[u8]) -> Vec<&[u8]> {
    fn take<'a>(
        avcc: &'a [u8],
        count: usize,
        at: &mut usize,
        out: &mut Vec<&'a [u8]>,
    ) -> Option<()> {
        for _ in 0..count {
            let len = usize::from(u16::from_be_bytes(avcc.get(*at..*at + 2)?.try_into().ok()?));
            out.push(avcc.get(*at + 2..*at + 2 + len)?);
            *at += 2 + len;
        }
        Some(())
    }
    let mut out = Vec::new();
    let mut at = 6;
    let sps = usize::from(avcc.get(5).map_or(0, |c| c & 0x1f));
    if take(avcc, sps, &mut at, &mut out).is_some()
        && let Some(&pps) = avcc.get(at)
    {
        at += 1;
        let _ = take(avcc, usize::from(pps), &mut at, &mut out);
    }
    out.retain(|u| !u.is_empty());
    out
}

/// Which picture each packet releases, as FFmpeg's H.264 decoder decides
/// with a fixed reorder delay: pictures wait in a list; once more than
/// `delay` wait, the one first in display order (not looking past a
/// keyframe) goes out, stamped with the packet's frame; a picture that
/// comes after one already out is dropped. At the end the rest drain in
/// display order, each a frame after the one before (or after the last
/// packet).
#[derive(Clone, Debug)]
pub struct OutputOrder {
    delay: usize,
    /// `(id, poc, idr)`.
    waiting: Vec<(usize, i64, bool)>,
    next_poc: i64,
    last_stamp: Option<u64>,
    last_frame: Option<u64>,
}

/// What a packet did: the picture it released, `(id, frame)`, and the one
/// it dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Released {
    pub out: Option<(usize, u64)>,
    pub dropped: Option<usize>,
}

impl OutputOrder {
    pub fn new(delay: u32) -> OutputOrder {
        OutputOrder {
            delay: delay as usize,
            waiting: Vec::new(),
            next_poc: i64::MIN,
            last_stamp: None,
            last_frame: None,
        }
    }

    /// Picture `id` (decoded from the packet of frame `frame`).
    pub fn push(&mut self, id: usize, picture: PictureInfo, frame: u32) -> Released {
        let frame = u64::from(frame);
        self.last_frame = Some(frame);
        self.waiting.push((id, picture.poc, picture.idr));
        let pics = self.waiting.len();
        let out = self.first_shown();
        if self.delay == 0 && self.waiting[0].2 {
            self.next_poc = i64::MIN;
        }
        let late = self.waiting[out].1 < self.next_poc;
        let mut released = Released::default();
        if late || pics > self.delay {
            let (id, poc, _) = self.waiting.remove(out);
            if late {
                released.dropped = Some(id);
            } else {
                self.next_poc = if out == 0 && self.waiting.first().is_some_and(|w| w.2) {
                    i64::MIN
                } else {
                    poc
                };
                let stamp = frame.max(self.last_stamp.map_or(0, |s| s + 1).min(frame));
                self.last_stamp = Some(stamp);
                released.out = Some((id, stamp));
            }
        }
        released
    }

    /// The waiting picture shown first: lowest POC, not past a keyframe.
    fn first_shown(&self) -> usize {
        let mut out = 0;
        for (i, w) in self.waiting.iter().enumerate().skip(1) {
            if w.2 {
                break;
            }
            if w.1 < self.waiting[out].1 {
                out = i;
            }
        }
        out
    }

    /// The end of the stream: the pictures still waiting, in display
    /// order, with their frames.
    pub fn drain(&mut self) -> Vec<(usize, u64)> {
        let mut out = Vec::new();
        while !self.waiting.is_empty() {
            let i = self.first_shown();
            let (id, _, _) = self.waiting.remove(i);
            let before = self.last_stamp.max(self.last_frame);
            let stamp = before.map_or(0, |b| b + 1);
            self.last_stamp = Some(stamp);
            out.push((id, stamp));
        }
        out
    }
}

/// Where the module starts decoding for an exact seek to frame time
/// `target` (in frames, fractional): the last keyframe at or before
/// `target − (delay + 1)` (it backs off by the reorder delay so the frame
/// released late is not skipped); `None` when there is none (it then
/// decodes from the start).
pub fn seek_start(chunks: &[Chunk], target: f64, delay: u32) -> Option<usize> {
    let ts = (target - f64::from(delay + 1)).floor();
    chunks
        .iter()
        .rposition(|c| c.key && f64::from(c.frame) <= ts)
}

/// Where to go back to when an exact seek's first frame came after its
/// target (the module's retry: from where it landed, 0.5 s further back
/// each time, doubling; after four, or at the start, from the start):
/// `landed` is the frame the decoder started at, `retries` how many times
/// this seek went back already, `rate` frames per second. `None`: decode
/// from the start, never going back again (the module reopens the file).
pub fn seek_back(chunks: &[Chunk], landed: u32, retries: u32, rate: f64) -> Option<usize> {
    if retries >= 4 || landed == 0 {
        return None;
    }
    let back = (0.5 * f64::from(1u32 << retries) * rate).ceil();
    let ts = f64::from(landed) - back;
    if ts < 0.0 {
        // Back to the start: what decoding from the start gives.
        return None;
    }
    chunks
        .iter()
        .rposition(|c| c.key && f64::from(c.frame) <= ts)
}

/// What [`ExactSeek::offer`] says to do with a decoded frame.
#[derive(Debug, PartialEq)]
pub enum Offer<T> {
    /// Hand these over, in order.
    Show(Vec<(T, f64)>),
    /// The first frame came after the target: go back further
    /// ([`seek_back`]) and decode again.
    Retry,
}

/// The module's exact seek (`ddi_next` after `ddi_seek`): frames up to the
/// target are held, the last one showing at it is handed over when a later
/// one shows it is the one, and that later one next.
pub struct ExactSeek<T> {
    target: Option<f64>,
    held: Option<(T, f64)>,
    first: bool,
    /// Retries are allowed (not when decoding from the start).
    may_retry: bool,
}

impl<T> ExactSeek<T> {
    /// A seek to `target` seconds (`None`: decoding straight on).
    pub fn new(target: Option<f64>, may_retry: bool) -> ExactSeek<T> {
        ExactSeek {
            target,
            held: None,
            first: true,
            may_retry,
        }
    }

    /// A decoded frame at `t` seconds.
    pub fn offer(&mut self, frame: T, t: f64) -> Offer<T> {
        let first = std::mem::replace(&mut self.first, false);
        let Some(target) = self.target else {
            return Offer::Show(vec![(frame, t)]);
        };
        if first && t > target + 1e-6 && self.may_retry {
            self.first = true;
            return Offer::Retry;
        }
        if t <= target + 1e-6 {
            self.held = Some((frame, t));
            return Offer::Show(Vec::new());
        }
        self.target = None;
        let mut out: Vec<_> = self.held.take().into_iter().collect();
        out.push((frame, t));
        Offer::Show(out)
    }

    /// The stream ended: the frame held, if it still shows at the target
    /// (each frame shows for `period` seconds).
    pub fn end(&mut self, period: f64) -> Option<(T, f64)> {
        let target = self.target.take()?;
        self.held.take().filter(|(_, t)| t + period > target + 1e-6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An SPS written bit by bit.
    struct Writer {
        bits: Vec<bool>,
    }

    impl Writer {
        fn u(&mut self, n: u32, v: u32) -> &mut Self {
            for i in (0..n).rev() {
                self.bits.push(v >> i & 1 == 1);
            }
            self
        }
        fn ue(&mut self, v: u32) -> &mut Self {
            let x = v + 1;
            let n = 32 - x.leading_zeros();
            self.u(n - 1, 0).u(n, x)
        }
        fn bytes(&self) -> Vec<u8> {
            let mut out: Vec<u8> = self
                .bits
                .chunks(8)
                .map(|c| {
                    c.iter()
                        .enumerate()
                        .fold(0u8, |b, (i, &x)| b | (u8::from(x) << (7 - i)))
                })
                .collect();
            // RBSP stop bit.
            out.push(0x80);
            // Emulation prevention, as encoders write it.
            let mut nal = Vec::new();
            let mut zeros = 0;
            for b in out {
                if zeros >= 2 && b <= 3 {
                    nal.push(3);
                    zeros = 0;
                }
                zeros = if b == 0 { zeros + 1 } else { 0 };
                nal.push(b);
            }
            nal
        }
    }

    /// A Main profile SPS, 852×480 (crop 6 on the right), with a VUI
    /// carrying the color tags and the bitstream restriction.
    fn sps(reorder: Option<u32>, matrix: Option<u32>) -> Vec<u8> {
        let mut w = Writer { bits: Vec::new() };
        w.u(8, 77).u(8, 0x40).u(8, 31).ue(0);
        w.ue(5)
            .ue(0)
            .ue(6)
            .ue(1)
            .u(1, 0)
            .ue(53)
            .ue(29)
            .u(1, 1)
            .u(1, 1);
        w.u(1, 1).ue(0).ue(6).ue(0).ue(0);
        w.u(1, 1); // VUI
        w.u(1, 1).u(8, 1); // aspect ratio 1:1
        w.u(1, 0);
        match matrix {
            Some(m) => w.u(1, 1).u(3, 5).u(1, 1).u(1, 1).u(8, 1).u(8, 1).u(8, m),
            None => w.u(1, 0),
        };
        w.u(1, 0);
        w.u(1, 1).u(32, 1).u(32, 60).u(1, 1);
        w.u(1, 0).u(1, 0).u(1, 0);
        match reorder {
            Some(r) => w.u(1, 1).u(1, 1).ue(0).ue(0).ue(11).ue(11).ue(r).ue(r + 1),
            None => w.u(1, 0),
        };
        let mut nal = vec![0x67];
        nal.extend(w.bytes());
        nal
    }

    #[test]
    fn sps_fields_and_codec_string() {
        let s = parse_sps(&sps(Some(2), Some(1))).unwrap();
        assert_eq!((s.width, s.height), (852, 480));
        assert_eq!(s.codec(), "avc1.4D401F");
        assert_eq!(s.reorder_delay(), Some(2));
        assert!(s.full_range && s.bt709() && s.frame_mbs_only);
        let s = parse_sps(&sps(None, None)).unwrap();
        assert_eq!(s.reorder_delay(), None);
        // Untagged: by height, as the module guesses.
        assert!(!s.full_range && !s.bt709());
        assert!(!parse_sps(&sps(Some(0), Some(6))).unwrap().bt709());
        // Cut anywhere, parsing never panics.
        let whole = sps(Some(1), Some(5));
        for n in 0..whole.len() {
            let _ = parse_sps(&whole[..n]);
        }
        assert_eq!(parse_sps(&[0x68, 0xff]), None);
    }

    #[test]
    fn emulation_prevention_is_removed() {
        let mut b = Bits::new(&[0, 0, 3, 1, 0xff]);
        assert_eq!(b.bits(24), Some(1));
        assert_eq!(b.bits(8), Some(0xff));
    }

    #[test]
    fn nal_units_in_either_framing_and_parameter_sets() {
        let sps = sps(Some(1), None);
        let mut annexb = vec![0, 0, 0, 1];
        annexb.extend(&sps);
        annexb.extend([0, 0, 1, 0x68, 0xce, 0x38, 0x80, 0, 0, 0, 1, 0x65, 0x88]);
        let units = nal_units(&annexb, Framing::AnnexB);
        assert_eq!(units.len(), 3);
        assert_eq!(units[1], [0x68, 0xce, 0x38, 0x80]);
        assert!(is_idr(&annexb, Framing::AnnexB));
        assert!(!is_idr(&[0, 0, 1, 0x41, 0x9a], Framing::AnnexB));
        // Parameter sets from extradata in Annex B, for keyframes.
        let s = stream(&annexb[..annexb.len() - 6], &[]).unwrap();
        assert_eq!(s.framing, Framing::AnnexB);
        assert_eq!(s.sps.reorder_delay(), Some(1));
        assert_eq!(nal_units(&s.parameter_sets, Framing::AnnexB).len(), 2);
        // An avcC record: length framing, the record as the description.
        let mut avcc = vec![1, 77, 0x40, 31, 0xff, 0xe1];
        avcc.extend((sps.len() as u16).to_be_bytes());
        avcc.extend(&sps);
        avcc.extend([1, 0, 4, 0x68, 0xce, 0x38, 0x80]);
        let s = stream(&avcc, &[]).unwrap();
        assert_eq!(s.framing, Framing::Length(4));
        assert_eq!(s.avcc.as_deref(), Some(&avcc[..]));
        let mut chunk = vec![0, 0, 0, 2, 0x65, 0x88, 0, 0, 0, 1, 0x06];
        assert!(is_idr(&chunk, Framing::Length(4)));
        chunk.truncate(7);
        assert_eq!(nal_units(&chunk, Framing::Length(4)).len(), 1);
        // No parameter sets anywhere: no stream.
        assert_eq!(stream(&[], &[0, 0, 1, 0x65]), None);
    }

    fn chunks(frames: &[(u32, bool)]) -> Vec<Chunk> {
        frames
            .iter()
            .map(|&(frame, key)| Chunk {
                offset: 0,
                size: 1,
                key,
                frame,
            })
            .collect()
    }

    fn pic(poc: i64, idr: bool) -> PictureInfo {
        PictureInfo { idr, poc, b: false }
    }

    #[test]
    fn output_order_follows_ffmpeg() {
        // I0 P6 B2 B4 P12 B8 B10 in decode order, one frame held back.
        let pocs = [
            (0, true),
            (6, false),
            (2, false),
            (4, false),
            (12, false),
            (8, false),
            (10, false),
        ];
        let mut order = OutputOrder::new(1);
        let mut out = Vec::new();
        for (i, &(poc, idr)) in pocs.iter().enumerate() {
            let r = order.push(i, pic(poc, idr), i as u32);
            assert_eq!(r.dropped, None);
            out.extend(r.out);
        }
        out.extend(order.drain());
        // Display order, each stamped a packet late, the last drained.
        assert_eq!(
            out,
            [(0, 1), (2, 2), (3, 3), (1, 4), (5, 5), (6, 6), (4, 7)]
        );
        // No delay: released at once; a picture shown before one already
        // out is dropped (a stream that under-declares its reordering).
        let mut order = OutputOrder::new(0);
        assert_eq!(order.push(0, pic(0, true), 0).out, Some((0, 0)));
        assert_eq!(order.push(1, pic(6, false), 1).out, Some((1, 1)));
        let r = order.push(2, pic(2, false), 2);
        assert_eq!((r.out, r.dropped), (None, Some(2)));
        // A keyframe is not looked past: the pictures before it go first.
        let mut order = OutputOrder::new(2);
        let mut out = Vec::new();
        for (i, &(poc, idr)) in [(0, true), (4, false), (2, false), (0, true), (2, false)]
            .iter()
            .enumerate()
        {
            out.extend(order.push(i, pic(poc, idr), i as u32 + 10).out);
        }
        out.extend(order.drain());
        assert_eq!(out.iter().map(|o| o.0).collect::<Vec<_>>(), [0, 2, 1, 3, 4]);
        assert_eq!(
            out.iter().map(|o| o.1).collect::<Vec<_>>(),
            [12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn picture_order_counts_from_slice_headers() {
        let sps = parse_sps(&sps(Some(1), None)).unwrap();
        assert_eq!(
            (sps.log2_max_frame_num, sps.poc_type, sps.log2_max_poc_lsb),
            (9, 0, 10)
        );
        // A slice header: first_mb 0, type, pps 0, frame_num (9 bits),
        // [idr_pic_id], poc lsb (10 bits).
        let slice = |nal: u8, kind: u32, frame_num: u32, lsb: u32| {
            let mut w = Writer { bits: Vec::new() };
            w.ue(0).ue(kind).ue(0).u(9, frame_num);
            if nal & 0x1f == 5 {
                w.ue(0);
            }
            w.u(10, lsb).u(8, 0xff);
            let mut chunk = vec![0, 0, 0, 1, nal];
            chunk.extend(w.bytes());
            chunk
        };
        let stream = Stream {
            sps: sps.clone(),
            framing: Framing::AnnexB,
            avcc: None,
            parameter_sets: Vec::new(),
        };
        let mut order = PictureOrder::new(&stream);
        let mut get = |nal, kind, f, lsb| {
            order
                .picture(&slice(nal, kind, f, lsb), Framing::AnnexB, &sps)
                .unwrap()
        };
        let idr = get(0x65, 7, 0, 0);
        assert!(idr.idr && !idr.b);
        let p = get(0x41, 5, 1, 6);
        let b = get(0x01, 6, 2, 2);
        assert!(b.b && !p.b);
        assert!(idr.poc < b.poc && b.poc < p.poc);
        // The LSB wraps (1024): the count goes on past it.
        let late = get(0x41, 5, 3, 1020);
        let wrapped = get(0x41, 5, 4, 4);
        assert!(wrapped.poc > late.poc, "{late:?} {wrapped:?}");
    }

    #[test]
    fn seeks_start_where_the_module_starts() {
        let c = chunks(&[
            (0, true),
            (1, false),
            (2, false),
            (3, true),
            (4, false),
            (5, false),
            (6, true),
        ]);
        assert_eq!(seek_start(&c, 5.5, 0), Some(3));
        // Backed off by the delay: frame 4 is released by packet 5.
        assert_eq!(seek_start(&c, 4.0, 1), Some(0));
        assert_eq!(seek_start(&c, 7.0, 0), Some(6));
        assert_eq!(seek_start(&c, 0.5, 1), None);
        // Going back: 0.5 s at 4 fps is 2 frames.
        assert_eq!(seek_back(&c, 6, 0, 4.0), Some(3));
        assert_eq!(seek_back(&c, 6, 1, 4.0), Some(0));
        assert_eq!(seek_back(&c, 3, 4, 4.0), None);
        assert_eq!(seek_back(&c, 0, 0, 4.0), None);
    }

    #[test]
    fn an_exact_seek_holds_until_the_frame_showing() {
        let mut s = ExactSeek::new(Some(1.05), true);
        assert_eq!(s.offer('a', 0.9), Offer::Show(vec![]));
        assert_eq!(s.offer('b', 1.0), Offer::Show(vec![]));
        assert_eq!(s.offer('c', 1.1), Offer::Show(vec![('b', 1.0), ('c', 1.1)]));
        assert_eq!(s.offer('d', 1.2), Offer::Show(vec![('d', 1.2)]));
        // The first frame after the target: go back, then accept.
        let mut s = ExactSeek::new(Some(1.0), true);
        assert_eq!(s.offer('x', 1.5), Offer::Retry);
        let mut s = ExactSeek::<char>::new(Some(1.0), false);
        assert_eq!(s.offer('x', 1.5), Offer::Show(vec![('x', 1.5)]));
        // The end inside the last frame shows it; past it, nothing.
        let mut s = ExactSeek::new(Some(2.0), true);
        let _ = s.offer('z', 1.98);
        assert_eq!(s.end(1.0 / 30.0), Some(('z', 1.98)));
        let mut s = ExactSeek::new(Some(3.0), true);
        let _ = s.offer('z', 1.98);
        assert_eq!(s.end(1.0 / 30.0), None);
    }
}
