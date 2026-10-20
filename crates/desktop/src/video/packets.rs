//! Bounded demux-only interface to the sandboxed helper (`video/packets.md`):
//! a helper process of its own that only demuxes, its packets copied out of
//! the shared arena after every length is checked. Synchronous: construct
//! and use on a decoder worker, never the UI thread. An instance owns one
//! reader; any failure poisons it (and kills its helper).

use super::*;
use serde::Deserialize;

const PACKET_LIMIT: usize = 8 * 1024 * 1024;
const CONFIG_LIMIT: usize = 1024 * 1024;
const SIDE_LIMIT: usize = 1024 * 1024;
const INDEX_LIMIT: u32 = 1_000_000;
pub const INDEX_PAGE: u32 = 256;

/// Initial stream parameters, not a promise that every packet has this format.
/// Color, pixel-format and field-order values use the pinned FFmpeg 9 enums.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct StreamInfo {
    pub abi: u32,
    pub codec: String,
    pub container: String,
    pub stream: u32,
    pub codec_tag: u32,
    pub width: u32,
    pub height: u32,
    pub profile: i32,
    pub level: i32,
    pub pixel_format: i32,
    pub bit_depth: i32,
    pub field_order: i32,
    pub video_delay: i32,
    pub time_base: [i32; 2],
    pub frame_rate: [i32; 2],
    #[serde(deserialize_with = "ticks")]
    pub start: Option<i64>,
    #[serde(deserialize_with = "ticks")]
    pub duration: Option<i64>,
    #[serde(deserialize_with = "ticks")]
    pub frames: Option<i64>,
    /// Container duration in AV_TIME_BASE microseconds, excluding bitrate guesses.
    #[serde(default, deserialize_with = "ticks")]
    pub container_duration: Option<i64>,
    pub aspect_ratio: [i32; 2],
    pub color_range: i32,
    pub color_space: i32,
    pub color_primaries: i32,
    pub color_transfer: i32,
    pub chroma_location: i32,
    pub configuration_size: u32,
}
fn ticks<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Option::<String>::deserialize(d)?
        .map(|s| s.parse().map_err(serde::de::Error::custom))
        .transpose()
}

/// Owned compressed packet. Timestamps are unshifted stream ticks; unknown
/// PTS/DTS are `None`. Duration 0 means unknown, position -1 means unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    /// Key 1, corrupt 2, discard 4, configuration changed 8.
    pub flags: u32,
    pub pts: Option<i64>,
    pub dts: Option<i64>,
    pub duration: i64,
    pub position: i64,
    pub sequence: u64,
    pub epoch: u32,
    pub configuration: u32,
    pub data: Vec<u8>,
    pub side_data: Vec<SideData>,
}
impl Packet {
    pub fn key(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn configuration_changed(&self) -> bool {
        self.flags & 8 != 0
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideData {
    /// AVPacketSideDataType from the pinned FFmpeg 9, including unknown types.
    pub kind: u32,
    pub data: Vec<u8>,
}
/// A demuxer's seek hint, not a complete frame/presentation index. The index
/// may grow as packets are read, and its timestamp convention is demuxer-specific.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    pub timestamp: i64,
    pub position: i64,
    pub size: i32,
    pub flags: u32,
    pub min_distance: i32,
}

/// Owns a private helper and one movie (including range-relative zip members).
pub struct PacketReader {
    helper: Option<helper::Helper>,
    id: u32,
    info: StreamInfo,
    epoch: u32,
    sequence: u64,
    configuration: u32,
}
impl PacketReader {
    pub fn open(helper: Option<PathBuf>, file: MovieFile, name: &str) -> Result<Self, String> {
        Self::open_bounded(helper, file, name, CALL_DEADLINE)
    }
    /// Native workers use a shorter deadline than software decode.
    pub(super) fn open_bounded(
        helper_path: Option<PathBuf>,
        file: MovieFile,
        name: &str,
        deadline: Duration,
    ) -> Result<Self, String> {
        let path = helper_path.ok_or_else(|| NOT_INCLUDED.to_string())?;
        // Demuxing decodes nothing: one thread.
        let mut helper = helper::Helper::spawn(&path, deadline, 1)?;
        let id = 1;
        let size = helper
            .register(id, &file)
            .map_err(|e| format!("cannot read the movie: {e}"))?;
        let reply = helper
            .request(&Request::DemuxOpen {
                id,
                size,
                name: format!("packets{}", extension_of(name)),
            })
            .map_err(|e| e.0)?;
        let json = match reply {
            Reply::DemuxOpened { id: got, info } if got == id => info,
            Reply::DemuxOpenFailed { id: got } if got == id => {
                return Err("cannot demux the movie within the packet limits".into());
            }
            other => return Err(unexpected(other).0),
        };
        let info: StreamInfo =
            serde_json::from_str(&json).map_err(|e| format!("invalid stream info: {e}"))?;
        if info.abi != 1
            || info.width == 0
            || info.height == 0
            || info.stream >= 32
            || info.time_base.iter().any(|n| *n <= 0)
            || info.configuration_size as usize > CONFIG_LIMIT
        {
            return Err("invalid stream parameters".into());
        }
        Ok(Self {
            helper: Some(helper),
            id,
            info,
            epoch: 0,
            sequence: 0,
            configuration: 0,
        })
    }
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Any failure discards the helper (killing it).
    fn run<T>(
        &mut self,
        f: impl FnOnce(&mut helper::Helper, u32) -> Result<T, String>,
    ) -> Result<T, String> {
        let h = self
            .helper
            .as_mut()
            .ok_or("packet reader is closed after an error")?;
        let result = f(h, self.id);
        if result.is_err() {
            self.helper = None;
        }
        result
    }
    pub fn configuration(&mut self) -> Result<Vec<u8>, String> {
        self.run(
            |h, id| match h.request(&Request::DemuxConfig { id }).map_err(|e| e.0)? {
                Reply::DemuxConfig { id: got, len } if got == id => region_bytes(
                    h,
                    &h.arena.layout.config.clone(),
                    len as usize,
                    CONFIG_LIMIT,
                ),
                Reply::DemuxError { id: got, message } if got == id => Err(message),
                other => Err(unexpected(other).0),
            },
        )
    }
    pub fn next_packet(&mut self) -> Result<Option<Packet>, String> {
        let epoch = self.epoch;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or("packet sequence exhausted")?;
        let generation = self.configuration;
        let packet = self.run(|h, id| {
            let record = match h.request(&Request::DemuxNext { id }).map_err(|e| e.0)? {
                Reply::DemuxEnd { id: got } if got == id => return Ok(None),
                Reply::DemuxError { id: got, message } if got == id => return Err(message),
                Reply::Packet { id: got, record } if got == id => record,
                other => return Err(unexpected(other).0),
            };
            let (data_size, side_size) = record_sizes(&record)?;
            let data = region_bytes(h, &h.arena.layout.packet.clone(), data_size, PACKET_LIMIT)?;
            let side = region_bytes(h, &h.arena.layout.side.clone(), side_size, SIDE_LIMIT)?;
            packet_from(&record, data, &side, epoch, sequence, generation).map(Some)
        })?;
        if let Some(p) = &packet {
            self.sequence = p.sequence;
            self.configuration = p.configuration;
        }
        Ok(packet)
    }
    /// Backward key-packet seek in raw stream ticks, not an exact frame seek.
    /// Parameter-changing streams require reopen/preroll or software fallback;
    /// the header configuration is restored, not reconstructed at the target.
    pub fn seek(&mut self, timestamp: i64) -> Result<(), String> {
        let epoch = self.epoch.checked_add(1).ok_or("packet epoch exhausted")?;
        if timestamp == i64::MIN {
            return Err("unknown timestamp cannot be a seek target".into());
        }
        self.run(|h, id| {
            match h
                .request(&Request::DemuxSeek { id, timestamp })
                .map_err(|e| e.0)?
            {
                Reply::DemuxSought { id: got } if got == id => Ok(()),
                Reply::DemuxError { id: got, message } if got == id => Err(message),
                other => Err(unexpected(other).0),
            }
        })?;
        self.epoch = epoch;
        self.sequence = 0;
        self.configuration = 0;
        Ok(())
    }
    pub fn index_count(&mut self) -> Result<u32, String> {
        self.run(|h, id| {
            match h
                .request(&Request::DemuxIndexCount { id })
                .map_err(|e| e.0)?
            {
                Reply::DemuxIndexCount { id: got, count } if got == id => {
                    if count < 0 || count as u32 > INDEX_LIMIT {
                        return Err("seek index exceeds the entry limit".into());
                    }
                    Ok(count as u32)
                }
                other => Err(unexpected(other).0),
            }
        })
    }
    pub fn index(&mut self, first: u32, count: u32) -> Result<Vec<IndexEntry>, String> {
        if count > INDEX_PAGE || first > INDEX_LIMIT {
            return Err("invalid seek index page".into());
        }
        self.run(|h, id| {
            match h
                .request(&Request::DemuxIndex { id, first, count })
                .map_err(|e| e.0)?
            {
                Reply::DemuxIndex { id: got, entries } if got == id => {
                    if entries.len() / 32 > count as usize {
                        return Err("invalid seek index page length".into());
                    }
                    index_from(&entries)
                }
                Reply::DemuxError { id: got, message } if got == id => Err(message),
                other => Err(unexpected(other).0),
            }
        })
    }
}

/// `n` bytes at the start of `region` of the helper's arena, copied out;
/// refused past `cap` or the region.
fn region_bytes(
    h: &helper::Helper,
    region: &std::ops::Range<usize>,
    n: usize,
    cap: usize,
) -> Result<Vec<u8>, String> {
    if n > cap || n > region.len() {
        return Err("packet buffer exceeds the byte limit".into());
    }
    Ok(h.arena.slice(&(region.start..region.start + n)).to_vec())
}

/// The data and side-data sizes a packet record names.
fn record_sizes(record: &[u8]) -> Result<(usize, usize), String> {
    if record.len() != 72 {
        return Err("invalid packet record".into());
    }
    Ok((u32_at(record, 8) as usize, u32_at(record, 12) as usize))
}

/// A packet from its record and buffers, checked against what this reader
/// expects of the next packet (`epoch`, `sequence`, the configuration
/// `generation` before it).
fn packet_from(
    b: &[u8],
    data: Vec<u8>,
    side: &[u8],
    epoch: u32,
    sequence: u64,
    generation: u32,
) -> Result<Packet, String> {
    if b.len() != 72 {
        return Err("invalid packet record".into());
    }
    let flags = u32_at(b, 4);
    let changed = u32::from(flags & 8 != 0);
    if u32_at(b, 0) != 72
        || flags & !15 != 0
        || u32_at(b, 68) != 0
        || u32_at(b, 56) != epoch
        || u64_at(b, 48) != sequence
        || generation.checked_add(changed) != Some(u32_at(b, 60))
    {
        return Err("invalid packet record or generation".into());
    }
    if data.len() != u32_at(b, 8) as usize || side.len() != u32_at(b, 12) as usize {
        return Err("packet buffers disagree with their record".into());
    }
    let count = u32_at(b, 64);
    if count > 32 {
        return Err("too many packet side-data entries".into());
    }
    let mut side_data = Vec::with_capacity(count as usize);
    let mut rest = side;
    for _ in 0..count {
        if rest.len() < 8 {
            return Err("truncated packet side data".into());
        }
        let kind = u32_at(rest, 0);
        let n = u32_at(rest, 4) as usize;
        rest = &rest[8..];
        let value = rest.get(..n).ok_or("truncated packet side data")?;
        side_data.push(SideData {
            kind,
            data: value.to_vec(),
        });
        rest = &rest[n..];
    }
    if !rest.is_empty() {
        return Err("trailing packet side data".into());
    }
    if side_data.iter().any(|s| matches!(s.kind, 1 | 2)) != (changed != 0) {
        return Err("configuration flag disagrees with packet side data".into());
    }
    let timestamp = |at| {
        let n = u64_at(b, at) as i64;
        (n != i64::MIN).then_some(n)
    };
    Ok(Packet {
        flags,
        pts: timestamp(16),
        dts: timestamp(24),
        duration: u64_at(b, 32) as i64,
        position: u64_at(b, 40) as i64,
        sequence,
        epoch,
        configuration: u32_at(b, 60),
        data,
        side_data,
    })
}

/// Index entries from their 32-byte records.
fn index_from(b: &[u8]) -> Result<Vec<IndexEntry>, String> {
    if !b.len().is_multiple_of(32) {
        return Err("invalid seek index page".into());
    }
    b.as_chunks::<32>()
        .0
        .iter()
        .map(|b| {
            if u32_at(b, 28) != 0 || u32_at(b, 20) & !3 != 0 || (u32_at(b, 16) as i32) < 0 {
                return Err("invalid seek index entry".into());
            }
            Ok(IndexEntry {
                timestamp: u64_at(b, 0) as i64,
                position: u64_at(b, 8) as i64,
                size: u32_at(b, 16) as i32,
                flags: u32_at(b, 20),
                min_distance: u32_at(b, 24) as i32,
            })
        })
        .collect()
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("fixed record"))
}
fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("fixed record"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record of sequence 1, epoch 0, DTS 2^53 + 1, no PTS.
    fn record() -> Vec<u8> {
        let mut b = vec![0u8; 72];
        b[0..4].copy_from_slice(&72u32.to_le_bytes());
        b[16..24].copy_from_slice(&i64::MIN.to_le_bytes());
        b[24..32].copy_from_slice(&9007199254740993i64.to_le_bytes());
        b[48..56].copy_from_slice(&1u64.to_le_bytes());
        b
    }
    fn set_u32(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn exact_ticks_unknown_pts_and_configuration_changes() {
        let mut b = record();
        set_u32(&mut b, 4, 8); // configuration changed
        set_u32(&mut b, 60, 1); // generation 1
        set_u32(&mut b, 12, 11); // side bytes
        set_u32(&mut b, 64, 1); // one entry
        let mut side = Vec::new();
        side.extend_from_slice(&1u32.to_le_bytes());
        side.extend_from_slice(&3u32.to_le_bytes());
        side.extend_from_slice(&[1, 2, 3]);
        let p = packet_from(&b, Vec::new(), &side, 0, 1, 0).unwrap();
        assert_eq!(p.pts, None);
        assert_eq!(p.dts, Some(9007199254740993));
        assert!(p.configuration_changed());
        assert_eq!(p.configuration, 1);
        assert_eq!(
            p.side_data,
            vec![SideData {
                kind: 1,
                data: vec![1, 2, 3]
            }]
        );
    }

    #[test]
    fn invalid_records_are_refused() {
        type Change = Box<dyn Fn(&mut Vec<u8>, &mut Vec<u8>, &mut Vec<u8>)>;
        let cases: Vec<(&str, Change)> = vec![
            ("size", Box::new(|b, _, _| set_u32(b, 0, 71))),
            ("flags", Box::new(|b, _, _| set_u32(b, 4, 16))),
            ("reserved", Box::new(|b, _, _| set_u32(b, 68, 1))),
            ("data length", Box::new(|b, _, _| set_u32(b, 8, 1))),
            ("side length", Box::new(|b, _, _| set_u32(b, 12, 1))),
            ("too many entries", Box::new(|b, _, _| set_u32(b, 64, 33))),
            ("missing TLV", Box::new(|b, _, _| set_u32(b, 64, 1))),
            (
                "unclaimed TLV",
                Box::new(|b, _, s| {
                    set_u32(b, 12, 8);
                    s.extend_from_slice(&[0; 8]);
                }),
            ),
            (
                "stale sequence",
                Box::new(|b, _, _| b[48..56].copy_from_slice(&2u64.to_le_bytes())),
            ),
            ("stale epoch", Box::new(|b, _, _| set_u32(b, 56, 1))),
            ("unannounced config", Box::new(|b, _, _| set_u32(b, 60, 1))),
            (
                "config flag without side data",
                Box::new(|b, _, _| {
                    set_u32(b, 4, 8);
                    set_u32(b, 60, 1);
                }),
            ),
        ];
        for (what, change) in cases {
            let (mut b, mut data, mut side) = (record(), Vec::new(), Vec::new());
            change(&mut b, &mut data, &mut side);
            assert!(packet_from(&b, data, &side, 0, 1, 0).is_err(), "{what}");
        }
        assert!(packet_from(&record(), Vec::new(), &[], 0, 1, 0).is_ok());
        assert!(record_sizes(&[0; 71]).is_err());
    }

    #[test]
    fn index_pages_are_checked_entry_by_entry() {
        let mut e = vec![0u8; 32];
        e[0..8].copy_from_slice(&(-3i64).to_le_bytes());
        e[16..20].copy_from_slice(&7u32.to_le_bytes());
        e[20..24].copy_from_slice(&1u32.to_le_bytes());
        let got = index_from(&e).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].timestamp, got[0].size, got[0].flags), (-3, 7, 1));
        assert!(index_from(&e[..31]).is_err());
        let mut bad = e.clone();
        bad[28] = 1;
        assert!(index_from(&bad).is_err());
        let mut bad = e.clone();
        bad[20] = 4;
        assert!(index_from(&bad).is_err());
        let mut bad = e;
        bad[19] = 0x80;
        assert!(index_from(&bad).is_err(), "negative size");
    }
}
