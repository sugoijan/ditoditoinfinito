//! Bounded demux-only interface to the sandboxed module (`video/packets.md`).
//! Synchronous: construct and use on a decoder worker, never the UI thread.
//! An instance owns one reader; a trap or invalid export poisons it. No FFmpeg
//! pointers or borrowed wasm memory escape. Playback does not use this yet.

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

struct PacketExports {
    close: TypedFunc<i32, ()>,
    info: TypedFunc<i32, i32>,
    error: TypedFunc<i32, i32>,
    config: TypedFunc<i32, i32>,
    config_size: TypedFunc<i32, i32>,
    next: TypedFunc<i32, i32>,
    packet_info: TypedFunc<i32, i32>,
    data: TypedFunc<i32, i32>,
    side_data: TypedFunc<i32, i32>,
    seek: TypedFunc<(i32, i64), i32>,
    index_count: TypedFunc<i32, i32>,
    index: TypedFunc<(i32, i32, i32), i32>,
    index_data: TypedFunc<i32, i32>,
}
impl PacketExports {
    fn load(r: &mut Running) -> Result<(Self, TypedFunc<i32, i32>), String> {
        let version = r
            .instance
            .get_typed_func::<(), i32>(&mut r.store, "ddi_demux_abi")
            .map_err(|_| "the video module has no packet ABI; rebuild it".to_string())?;
        if call(&mut r.store, &version, ()).map_err(|e| e.0)? != 1 {
            return Err("unsupported video packet ABI".into());
        }
        macro_rules! get {
            ($name:ident) => {
                r.instance
                    .get_typed_func(&mut r.store, concat!("ddi_demux_", stringify!($name)))
                    .map_err(|e| format!("invalid packet export: {e}"))?
            };
        }
        Ok((
            Self {
                close: get!(close),
                info: get!(info),
                error: get!(error),
                config: get!(config),
                config_size: get!(config_size),
                next: get!(next),
                packet_info: get!(packet_info),
                data: get!(data),
                side_data: get!(side_data),
                seek: get!(seek),
                index_count: get!(index_count),
                index: get!(index),
                index_data: get!(index_data),
            },
            get!(open),
        ))
    }
}

/// Owns a private sandbox and one movie (including range-relative zip members).
/// Older modules still work for ordinary playback; only this API requires v1.
pub struct PacketReader {
    machine: Machine,
    x: PacketExports,
    handle: i32,
    info: StreamInfo,
    epoch: u32,
    sequence: u64,
    configuration: u32,
}
impl PacketReader {
    pub fn open(module: Option<PathBuf>, file: MovieFile, name: &str) -> Result<Self, String> {
        let shared = Arc::new(Mutex::new(Shared {
            status: ModuleStatus::Idle,
            movies: HashMap::new(),
            probes: HashMap::new(),
        }));
        let mut machine = Machine::new(module, shared, CALL_DEADLINE);
        machine.compiled()?;
        machine.instantiate()?;
        let r = machine.running.as_mut().expect("instance");
        let (x, open) = PacketExports::load(r)?;
        let file_name = format!("packets{}", extension_of(name));
        let handle = machine
            .open_file_using(&file_name, file, open)
            .map_err(|e| e.0)?;
        let r = machine.running.as_mut().expect("instance");
        if handle == 0 {
            return Err("cannot demux the movie within the packet limits".into());
        }
        let p = call(&mut r.store, &x.info, handle).map_err(|e| e.0)?;
        let json = string(r, p, 4096)?;
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
            machine,
            x,
            handle,
            info,
            epoch: 0,
            sequence: 0,
            configuration: 0,
        })
    }
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Any failure discards the instance. Never call close inside trapped memory.
    fn run<T>(
        &mut self,
        f: impl FnOnce(&mut Running, &PacketExports, i32) -> Result<T, String>,
    ) -> Result<T, String> {
        let r = self
            .machine
            .running
            .as_mut()
            .ok_or("packet reader is closed after an error")?;
        let result = f(r, &self.x, self.handle);
        if result.is_err() {
            self.machine.running = None;
        }
        result
    }
    pub fn configuration(&mut self) -> Result<Vec<u8>, String> {
        self.run(|r, x, h| {
            let n = call(&mut r.store, &x.config_size, h).map_err(|e| e.0)? as u32 as usize;
            let p = call(&mut r.store, &x.config, h).map_err(|e| e.0)?;
            bytes(r, p, n, CONFIG_LIMIT)
        })
    }
    pub fn next_packet(&mut self) -> Result<Option<Packet>, String> {
        let epoch = self.epoch;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or("packet sequence exhausted")?;
        let generation = self.configuration;
        let packet = self.run(|r, x, h| {
            match call(&mut r.store, &x.next, h).map_err(|e| e.0)? {
                0 => return Ok(None),
                -1 => return Err(module_error(r, x, h)?),
                1 => (),
                _ => return Err("invalid packet result".into()),
            }
            let p = call(&mut r.store, &x.packet_info, h).map_err(|e| e.0)?;
            let b = bytes(r, p, 72, 72)?;
            let flags = u32_at(&b, 4);
            let changed = u32::from(flags & 8 != 0);
            if u32_at(&b, 0) != 72
                || flags & !15 != 0
                || u32_at(&b, 68) != 0
                || u32_at(&b, 56) != epoch
                || u64_at(&b, 48) != sequence
                || generation.checked_add(changed) != Some(u32_at(&b, 60))
            {
                return Err("invalid packet record or generation".into());
            }
            let p = call(&mut r.store, &x.data, h).map_err(|e| e.0)?;
            let data = bytes(r, p, u32_at(&b, 8) as usize, PACKET_LIMIT)?;
            let p = call(&mut r.store, &x.side_data, h).map_err(|e| e.0)?;
            let side = bytes(r, p, u32_at(&b, 12) as usize, SIDE_LIMIT)?;
            let count = u32_at(&b, 64);
            if count > 32 {
                return Err("too many packet side-data entries".into());
            }
            let mut side_data = Vec::with_capacity(count as usize);
            let mut rest = side.as_slice();
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
                let n = u64_at(&b, at) as i64;
                (n != i64::MIN).then_some(n)
            };
            Ok(Some(Packet {
                flags,
                pts: timestamp(16),
                dts: timestamp(24),
                duration: u64_at(&b, 32) as i64,
                position: u64_at(&b, 40) as i64,
                sequence,
                epoch,
                configuration: u32_at(&b, 60),
                data,
                side_data,
            }))
        })?;
        if let Some(p) = &packet {
            self.sequence = p.sequence;
            self.configuration = p.configuration;
        }
        Ok(packet)
    }
    /// Backward key-packet seek in raw stream ticks, not an exact frame seek.
    /// Parameter-changing streams require reopen/preroll or module fallback;
    /// the header configuration is restored, not reconstructed at the target.
    pub fn seek(&mut self, timestamp: i64) -> Result<(), String> {
        let epoch = self.epoch.checked_add(1).ok_or("packet epoch exhausted")?;
        if timestamp == i64::MIN {
            return Err("unknown timestamp cannot be a seek target".into());
        }
        self.run(|r, x, h| {
            if call(&mut r.store, &x.seek, (h, timestamp)).map_err(|e| e.0)? != 0 {
                return Err(module_error(r, x, h)?);
            }
            Ok(())
        })?;
        self.epoch = epoch;
        self.sequence = 0;
        self.configuration = 0;
        Ok(())
    }
    pub fn index_count(&mut self) -> Result<u32, String> {
        self.run(|r, x, h| {
            let n = call(&mut r.store, &x.index_count, h).map_err(|e| e.0)? as u32;
            if n > INDEX_LIMIT {
                return Err("seek index exceeds the entry limit".into());
            }
            Ok(n)
        })
    }
    pub fn index(&mut self, first: u32, count: u32) -> Result<Vec<IndexEntry>, String> {
        if count > INDEX_PAGE || first > INDEX_LIMIT {
            return Err("invalid seek index page".into());
        }
        self.run(|r, x, h| {
            let n = call(&mut r.store, &x.index, (h, first as i32, count as i32))
                .map_err(|e| e.0)? as u32;
            if n > count {
                return Err("invalid seek index page length".into());
            }
            let p = call(&mut r.store, &x.index_data, h).map_err(|e| e.0)?;
            let b = bytes(r, p, n as usize * 32, INDEX_PAGE as usize * 32)?;
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
        })
    }
}
impl Drop for PacketReader {
    fn drop(&mut self) {
        if let Some(r) = &mut self.machine.running {
            let _ = call(&mut r.store, &self.x.close, self.handle);
        }
    }
}
fn bytes(r: &Running, pointer: i32, n: usize, cap: usize) -> Result<Vec<u8>, String> {
    if n > cap || (n != 0 && pointer == 0) {
        return Err("packet buffer exceeds the byte limit or is null".into());
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    let p = pointer as u32 as usize;
    let end = p.checked_add(n).ok_or("packet buffer range overflow")?;
    Ok(r.x
        .memory
        .data(&r.store)
        .get(p..end)
        .ok_or("packet buffer outside module memory")?
        .to_vec())
}
fn string(r: &Running, pointer: i32, cap: usize) -> Result<String, String> {
    let p = pointer as u32 as usize;
    let memory = r.x.memory.data(&r.store);
    let b = memory
        .get(p..)
        .ok_or("packet string outside module memory")?;
    let end = b
        .iter()
        .take(cap)
        .position(|c| *c == 0)
        .ok_or("unterminated packet string")?;
    if pointer == 0 {
        return Err("null packet string".into());
    }
    String::from_utf8(b[..end].to_vec()).map_err(|e| e.to_string())
}
fn module_error(r: &mut Running, x: &PacketExports, h: i32) -> Result<String, String> {
    let p = call(&mut r.store, &x.error, h).map_err(|e| e.0)?;
    string(r, p, 256)
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

    fn info() -> StreamInfo {
        serde_json::from_value(serde_json::json!({
            "abi":1,"codec":"h264","container":"mov","stream":0,"codec_tag":0,
            "width":128,"height":96,"profile":100,"level":10,"pixel_format":0,
            "bit_depth":8,"field_order":1,"video_delay":2,"time_base":[1,30000],
            "frame_rate":[30000,1001],"start":"9007199254740993","duration":null,
            "frames":null,"aspect_ratio":[1,1],"color_range":0,"color_space":2,
            "color_primaries":2,"color_transfer":2,"chroma_location":0,"configuration_size":0
        }))
        .unwrap()
    }
    fn reader(extra: &str) -> PacketReader {
        reader_with(extra, &[])
    }
    fn decode_stub() -> String {
        let mut text = String::from("(module (memory (export \"memory\") 1)");
        text.push_str("(func (export \"ddi_malloc\") (param i32) (result i32) (i32.const 4096)) (func (export \"ddi_free\") (param i32))");
        // Old playback exports remain independently loadable.
        for name in [
            "open",
            "width",
            "height",
            "codec",
            "colorspace",
            "full_range",
            "planes",
            "planes_size",
        ] {
            text.push_str(&format!(
                "(func (export \"ddi_{name}\") (param i32) (result i32) (i32.const 1))"
            ));
        }
        for name in ["last_time", "duration", "next"] {
            text.push_str(&format!(
                "(func (export \"ddi_{name}\") (param i32) (result f64) (f64.const 0))"
            ));
        }
        text.push_str("(func (export \"ddi_seek\") (param i32 f64) (result i32) (i32.const 0)) (func (export \"ddi_close\") (param i32))");
        text
    }
    fn reader_with(extra: &str, values: &[(&str, i32)]) -> PacketReader {
        let mut text = decode_stub();
        text.push_str("(func (export \"ddi_demux_abi\") (result i32) (i32.const 1))");
        for (name, value) in [
            ("info", 2048),
            ("error", 2000),
            ("config", 0),
            ("config_size", 0),
            ("packet_info", 128),
            ("data", 512),
            ("side_data", 1024),
            ("index_data", 1500),
            ("index_count", 0),
            ("open", 1),
        ] {
            let value = values
                .iter()
                .find(|(n, _)| *n == name)
                .map_or(value, |(_, v)| *v);
            text.push_str(&format!(
                "(func (export \"ddi_demux_{name}\") (param i32) (result i32) (i32.const {value}))"
            ));
        }
        text.push_str("(func (export \"ddi_demux_close\") (param i32)) (func (export \"ddi_demux_index\") (param i32 i32 i32) (result i32) (i32.const 0))");
        text.push_str(
            "(func (export \"ddi_demux_seek\") (param i32 i64) (result i32) (i32.const 0))",
        );
        text.push_str(&format!(
            r#"(func (export "ddi_demux_next") (param i32) (result i32)
            (i32.store (i32.const 128) (i32.const 72))
            (i64.store (i32.const 144) (i64.const -9223372036854775808))
            (i64.store (i32.const 152) (i64.const 9007199254740993))
            (i64.store (i32.const 176) (i64.const 1))
            {extra} (i32.const 1)))"#
        ));
        let engine = Engine::default();
        let module = Module::new(&engine, wat::parse_str(text).unwrap()).unwrap();
        let mut store = Store::new(&engine, Wasi::default());
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let x = exports(&instance, &mut store).unwrap();
        let mut r = Running { instance, store, x };
        let (x, _) = PacketExports::load(&mut r).unwrap();
        let shared = Arc::new(Mutex::new(Shared {
            status: ModuleStatus::Idle,
            movies: HashMap::new(),
            probes: HashMap::new(),
        }));
        let mut machine = Machine::new(None, shared, CALL_DEADLINE);
        machine.running = Some(r);
        PacketReader {
            machine,
            x,
            handle: 1,
            info: info(),
            epoch: 0,
            sequence: 0,
            configuration: 0,
        }
    }
    #[test]
    fn exact_ticks_unknown_pts_and_configuration_changes() {
        let mut r = reader(
            r#"
            (i32.store (i32.const 132) (i32.const 8))
            (i32.store (i32.const 188) (i32.const 1))
            (i32.store (i32.const 140) (i32.const 11))
            (i32.store (i32.const 192) (i32.const 1))
            (i32.store (i32.const 1024) (i32.const 1))
            (i32.store (i32.const 1028) (i32.const 3))
            (i32.store (i32.const 1032) (i32.const 197121))"#,
        );
        let p = r.next_packet().unwrap().unwrap();
        assert_eq!(r.info().start, Some(9007199254740993));
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
    fn invalid_records_and_traps_poison_the_reader() {
        for extra in [
            "unreachable", // no later call, including close, enters this instance
            "(i32.store (i32.const 128) (i32.const 71))",
            "(i32.store (i32.const 132) (i32.const 16))",
            "(i32.store (i32.const 136) (i32.const 8388609))", // too large
            "(i32.store (i32.const 136) (i32.const 65536))",   // range beyond memory
            "(i32.store (i32.const 140) (i32.const 1048577))",
            "(i32.store (i32.const 192) (i32.const 33))",
            "(i32.store (i32.const 192) (i32.const 1))", // missing TLV
            "(i32.store (i32.const 140) (i32.const 8))", // unclaimed TLV
            "(i64.store (i32.const 176) (i64.const 2))", // stale sequence
            "(i32.store (i32.const 184) (i32.const 1))", // stale epoch
            "(i32.store (i32.const 188) (i32.const 1))", // unannounced config
        ] {
            let mut r = reader(extra);
            assert!(r.next_packet().is_err(), "{extra}");
            assert!(r.machine.running.is_none());
            assert!(r.configuration().unwrap_err().contains("closed"));
        }
    }
    #[test]
    fn configuration_and_index_exports_are_bounded_independently() {
        for (name, value, config) in [
            ("config_size", 1048577, true),
            ("config", -1, true),
            ("index_count", 1000001, false),
            ("index_count", -1, false),
        ] {
            let fields = if name == "config" {
                vec![(name, value), ("config_size", 1)]
            } else {
                vec![(name, value)]
            };
            let mut r = reader_with("", &fields);
            let failed = if config {
                r.configuration().is_err()
            } else {
                r.index_count().is_err()
            };
            assert!(failed, "{name}");
            assert!(r.machine.running.is_none());
        }
    }
    #[test]
    fn missing_packet_exports_do_not_break_old_playback_exports() {
        let engine = Engine::default();
        let text = format!("{})", decode_stub());
        let module = Module::new(&engine, wat::parse_str(text).unwrap()).unwrap();
        let mut store = Store::new(&engine, Wasi::default());
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let x = exports(&instance, &mut store).unwrap();
        let mut running = Running { instance, store, x };
        let error = PacketExports::load(&mut running).err().unwrap();
        assert!(error.contains("no packet ABI"));
        assert_eq!(
            call(&mut running.store, &running.x.width, 1)
                .map_err(|e| e.0)
                .unwrap(),
            1
        );
    }
}
