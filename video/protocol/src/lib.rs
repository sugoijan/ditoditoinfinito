//! The contract between the desktop shell and `ddi-video-helper`, the
//! sandboxed child that runs FFmpeg (`docs/plans/native-video.md`, section
//! 10). Platform-free and dependency-free; both sides share it.
//!
//! **Control** goes over the helper's stdin (requests) and stdout
//! (replies) as bounded frames: a little-endian `u32` payload length, a
//! `u32` sequence number, then the payload, whose first byte is a tag.
//! Payloads are at most [`MAX_PAYLOAD`] bytes; strings carry a `u16`
//! length. Messages are hand-encoded field by field: no Rust struct is
//! ever sent as its bytes, and every length and count is checked on the
//! way in. The parent sends one request at a time and waits for its
//! reply, whose sequence number must echo the request's; while serving a
//! request the helper may ask for file bytes ([`Reply::Read`]) and the
//! parent answers with [`Request::Data`] before the reply arrives.
//!
//! **Bulk bytes** never cross the pipes: frames, packets, codec
//! configuration and the movie bytes the helper reads go through one
//! shared memory arena the parent creates and the helper maps
//! ([`Layout`]). The parent treats the arena as untrusted: it copies a
//! frame out of a slot only after checking the reply's dimensions against
//! its own bounds, and the helper trusts the arena no further than the
//! lengths the parent names.
//!
//! The helper never opens a file: the parent serves `Read`s of the one
//! movie each handle names, at most [`MAX_READ`] bytes each, into the
//! arena's I/O region.

use std::fmt;
use std::io::{self, Read, Write};
use std::ops::Range;

/// The protocol's version; both sides say it in `Hello` and refuse another.
pub const VERSION: u32 = 1;
/// A payload's bytes, at most.
pub const MAX_PAYLOAD: usize = 64 * 1024;
/// A name's (an extension hint's) or a message's characters, at most.
pub const MAX_STRING: usize = 4096;
/// Movies (decode and demux handles together) open in one helper, at most.
pub const MAX_OPEN: usize = 32;
/// Bytes of one `Read`, at most: the arena's I/O region.
pub const MAX_READ: usize = 1024 * 1024;
/// Pixels of a frame the helper hands over, at most (the shim refuses
/// larger pictures before decoding them).
pub const MAX_PIXELS: usize = 3840 * 2160;
/// Index entries a `DemuxIndex` page carries, at most (`video/packets.md`).
pub const INDEX_PAGE: u32 = 256;
/// The demux ABI's record sizes (`video/packets.md`).
pub const PACKET_RECORD: usize = 72;
pub const INDEX_RECORD: usize = 32;

/// How the helper exits when it refuses to serve.
pub mod exit {
    /// The command line was wrong.
    pub const USAGE: i32 = 2;
    /// The sandbox could not be entered, or its self-test found it open.
    pub const SANDBOX: i32 = 3;
    /// The parent broke the protocol (a bad frame, an unknown handle).
    pub const PROTOCOL: i32 = 4;
    /// The arena could not be mapped.
    pub const ARENA: i32 = 5;
}

/// Where everything bulk lives in the arena. One region for the movie
/// bytes the parent serves, one each for a demuxed packet's data, its
/// side data and the stream's configuration, and two frame slots so the
/// parent can copy one frame while the helper decodes the next. Regions
/// are page-aligned; the arena is created at [`Layout::LEN`] bytes, which
/// `Hello` repeats so a helper mapping another size refuses to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub io: Range<usize>,
    pub packet: Range<usize>,
    pub side: Range<usize>,
    pub config: Range<usize>,
    pub frames: [Range<usize>; Layout::FRAME_SLOTS],
}

impl Layout {
    pub const FRAME_SLOTS: usize = 2;
    pub const IO: usize = MAX_READ;
    pub const PACKET: usize = 8 * 1024 * 1024;
    pub const SIDE: usize = 1024 * 1024;
    pub const CONFIG: usize = 1024 * 1024;
    /// Room for a 4K 4:2:0 frame (12,441,600 bytes) and odd sizes'
    /// rounding; a frame whose planes do not fit is refused.
    pub const FRAME: usize = 12 * 1024 * 1024 + 64 * 1024;
    pub const LEN: usize =
        Self::IO + Self::PACKET + Self::SIDE + Self::CONFIG + Self::FRAME_SLOTS * Self::FRAME;

    pub fn new() -> Layout {
        let mut at = 0;
        let mut take = |n: usize| {
            let r = at..at + n;
            at += n;
            r
        };
        Layout {
            io: take(Self::IO),
            packet: take(Self::PACKET),
            side: take(Self::SIDE),
            config: take(Self::CONFIG),
            frames: [take(Self::FRAME), take(Self::FRAME)],
        }
    }

    /// The slot's range, or none for a slot number out of range.
    pub fn frame(&self, slot: u32) -> Option<&Range<usize>> {
        self.frames.get(slot as usize)
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

/// Bytes of a frame's planes: Y (`w × h`) and U, V (`⌈w/2⌉ × ⌈h/2⌉` each).
pub fn planes_len(width: u32, height: u32) -> Option<usize> {
    let (w, h) = (width as usize, height as usize);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    w.checked_mul(h)?
        .checked_add(cw.checked_mul(ch)?.checked_mul(2)?)
}

/// The parent's requests.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// First: the protocol version, the arena's length as the parent made
    /// it, and how many decoder threads FFmpeg may use.
    Hello {
        version: u32,
        arena_len: u64,
        threads: u32,
    },
    /// A movie to decode: its handle (the parent's choice, unused so
    /// far), its length in bytes and its file name (an extension hint).
    Open {
        id: u32,
        size: u64,
        name: String,
    },
    /// `ddi_seek`: the next frame is the one showing at `t`.
    Seek {
        id: u32,
        t: f64,
    },
    /// `ddi_seek_key`: the next frame is the first from a keyframe at or
    /// before `t` (the black-bar probe).
    SeekKey {
        id: u32,
        t: f64,
    },
    /// The next frame, its planes into frame slot `slot`.
    Next {
        id: u32,
        slot: u32,
    },
    Close {
        id: u32,
    },
    /// A movie to demux only (`video/packets.md`).
    DemuxOpen {
        id: u32,
        size: u64,
        name: String,
    },
    /// The stream's current configuration into the config region.
    DemuxConfig {
        id: u32,
    },
    /// The next packet: its record in the reply, its data and side data
    /// in the packet and side regions.
    DemuxNext {
        id: u32,
    },
    DemuxSeek {
        id: u32,
        timestamp: i64,
    },
    DemuxIndexCount {
        id: u32,
    },
    DemuxIndex {
        id: u32,
        first: u32,
        count: u32,
    },
    DemuxClose {
        id: u32,
    },
    /// The answer to a `Read`: `len` bytes are in the I/O region (fewer
    /// than asked at the end of the movie; 0 at or past it).
    Data {
        len: u32,
    },
    Shutdown,
}

/// The helper's replies (and its one request, `Read`).
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// The answer to `Hello`: what decodes, and how it is confined
    /// ("seatbelt", "seccomp+landlock", "windows-restricted").
    Hello {
        version: u32,
        decoder: String,
        sandbox: String,
        threads: u32,
    },
    /// Mid-request: `len` bytes of movie `id` from `offset`, into the I/O
    /// region, please (answered by `Request::Data`).
    Read {
        id: u32,
        offset: u64,
        len: u32,
    },
    Opened {
        id: u32,
        width: u32,
        height: u32,
        /// Seconds; negative when unknown.
        duration: f64,
        codec: String,
    },
    /// Not a movie the decoder knows (or the handle cap is reached).
    OpenFailed {
        id: u32,
    },
    /// A frame in slot `slot`; `colorspace` is 601 or 709.
    Frame {
        id: u32,
        slot: u32,
        pts: f64,
        width: u32,
        height: u32,
        colorspace: u32,
        full_range: bool,
        decode_us: u32,
    },
    /// The end of the movie; `last_time` the last frame decoded (handed
    /// over or not), negative when none since the last seek.
    End {
        id: u32,
        last_time: f64,
    },
    /// A frame failed to decode or cannot be handed over; the next
    /// `Next` goes on with the next one.
    Bad {
        id: u32,
    },
    /// A seek done; `ok` false when the file had to be reopened and could
    /// not be (the movie is unusable).
    Sought {
        id: u32,
        ok: bool,
    },
    Closed {
        id: u32,
    },
    /// The stream's description, the demux ABI's JSON (at most 4096 bytes).
    DemuxOpened {
        id: u32,
        info: String,
    },
    DemuxOpenFailed {
        id: u32,
    },
    /// `len` bytes of configuration are in the config region.
    DemuxConfig {
        id: u32,
        len: u32,
    },
    /// A packet: its 72-byte record; data and side data are in their
    /// regions, the lengths inside the record.
    Packet {
        id: u32,
        record: Vec<u8>,
    },
    DemuxEnd {
        id: u32,
    },
    DemuxError {
        id: u32,
        message: String,
    },
    DemuxSought {
        id: u32,
    },
    DemuxIndexCount {
        id: u32,
        count: i32,
    },
    /// Index entries, 32 bytes each.
    DemuxIndex {
        id: u32,
        entries: Vec<u8>,
    },
    /// The helper refuses the request (an unknown handle, a slot out of
    /// range): the parent should treat the helper as broken.
    Error {
        message: String,
    },
    Bye,
}

/// A message that does not decode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Malformed(pub &'static str);

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "malformed video protocol message: {}", self.0)
    }
}

impl std::error::Error for Malformed {}

impl From<Malformed> for io::Error {
    fn from(m: Malformed) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, m)
    }
}

struct Enc(Vec<u8>);

impl Enc {
    fn tag(t: u8) -> Enc {
        Enc(vec![t])
    }
    fn u8(mut self, v: u8) -> Enc {
        self.0.push(v);
        self
    }
    fn u32(mut self, v: u32) -> Enc {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i32(self, v: i32) -> Enc {
        self.u32(v as u32)
    }
    fn u64(mut self, v: u64) -> Enc {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i64(self, v: i64) -> Enc {
        self.u64(v as u64)
    }
    fn f64(mut self, v: f64) -> Enc {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn bytes(mut self, b: &[u8]) -> Enc {
        // Lengths are checked by `encode`.
        self.0.extend_from_slice(&(b.len() as u16).to_le_bytes());
        self.0.extend_from_slice(b);
        self
    }
    fn str(self, s: &str) -> Enc {
        self.bytes(s.as_bytes())
    }
}

struct Dec<'a>(&'a [u8]);

impl<'a> Dec<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Malformed> {
        if self.0.len() < n {
            return Err(Malformed("truncated"));
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8, Malformed> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, Malformed> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, Malformed> {
        Ok(self.u32()? as i32)
    }
    fn u64(&mut self) -> Result<u64, Malformed> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, Malformed> {
        Ok(self.u64()? as i64)
    }
    fn f64(&mut self) -> Result<f64, Malformed> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn bytes(&mut self) -> Result<&'a [u8], Malformed> {
        let n = u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as usize;
        if n > MAX_STRING {
            return Err(Malformed("string too long"));
        }
        self.take(n)
    }
    fn str(&mut self) -> Result<String, Malformed> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| Malformed("string not UTF-8"))
    }
    fn end(self) -> Result<(), Malformed> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Malformed("trailing bytes"))
        }
    }
}

fn check_str(s: &str) -> Result<(), Malformed> {
    if s.len() > MAX_STRING {
        Err(Malformed("string too long"))
    } else {
        Ok(())
    }
}

impl Request {
    pub fn encode(&self) -> Result<Vec<u8>, Malformed> {
        let e = match self {
            Request::Hello {
                version,
                arena_len,
                threads,
            } => Enc::tag(1).u32(*version).u64(*arena_len).u32(*threads),
            Request::Open { id, size, name } => {
                check_str(name)?;
                Enc::tag(2).u32(*id).u64(*size).str(name)
            }
            Request::Seek { id, t } => Enc::tag(3).u32(*id).f64(*t),
            Request::SeekKey { id, t } => Enc::tag(4).u32(*id).f64(*t),
            Request::Next { id, slot } => Enc::tag(5).u32(*id).u32(*slot),
            Request::Close { id } => Enc::tag(6).u32(*id),
            Request::DemuxOpen { id, size, name } => {
                check_str(name)?;
                Enc::tag(7).u32(*id).u64(*size).str(name)
            }
            Request::DemuxConfig { id } => Enc::tag(8).u32(*id),
            Request::DemuxNext { id } => Enc::tag(9).u32(*id),
            Request::DemuxSeek { id, timestamp } => Enc::tag(10).u32(*id).i64(*timestamp),
            Request::DemuxIndexCount { id } => Enc::tag(11).u32(*id),
            Request::DemuxIndex { id, first, count } => {
                Enc::tag(12).u32(*id).u32(*first).u32(*count)
            }
            Request::DemuxClose { id } => Enc::tag(13).u32(*id),
            Request::Data { len } => Enc::tag(14).u32(*len),
            Request::Shutdown => Enc::tag(15),
        };
        Ok(e.0)
    }

    pub fn decode(bytes: &[u8]) -> Result<Request, Malformed> {
        let mut d = Dec(bytes);
        let r = match d.u8()? {
            1 => Request::Hello {
                version: d.u32()?,
                arena_len: d.u64()?,
                threads: d.u32()?,
            },
            2 => Request::Open {
                id: d.u32()?,
                size: d.u64()?,
                name: d.str()?,
            },
            3 => Request::Seek {
                id: d.u32()?,
                t: d.f64()?,
            },
            4 => Request::SeekKey {
                id: d.u32()?,
                t: d.f64()?,
            },
            5 => Request::Next {
                id: d.u32()?,
                slot: d.u32()?,
            },
            6 => Request::Close { id: d.u32()? },
            7 => Request::DemuxOpen {
                id: d.u32()?,
                size: d.u64()?,
                name: d.str()?,
            },
            8 => Request::DemuxConfig { id: d.u32()? },
            9 => Request::DemuxNext { id: d.u32()? },
            10 => Request::DemuxSeek {
                id: d.u32()?,
                timestamp: d.i64()?,
            },
            11 => Request::DemuxIndexCount { id: d.u32()? },
            12 => Request::DemuxIndex {
                id: d.u32()?,
                first: d.u32()?,
                count: d.u32()?,
            },
            13 => Request::DemuxClose { id: d.u32()? },
            14 => Request::Data { len: d.u32()? },
            15 => Request::Shutdown,
            _ => return Err(Malformed("unknown request tag")),
        };
        d.end()?;
        Ok(r)
    }
}

impl Reply {
    pub fn encode(&self) -> Result<Vec<u8>, Malformed> {
        let e = match self {
            Reply::Hello {
                version,
                decoder,
                sandbox,
                threads,
            } => {
                check_str(decoder)?;
                check_str(sandbox)?;
                Enc::tag(1)
                    .u32(*version)
                    .str(decoder)
                    .str(sandbox)
                    .u32(*threads)
            }
            Reply::Read { id, offset, len } => Enc::tag(2).u32(*id).u64(*offset).u32(*len),
            Reply::Opened {
                id,
                width,
                height,
                duration,
                codec,
            } => {
                check_str(codec)?;
                Enc::tag(3)
                    .u32(*id)
                    .u32(*width)
                    .u32(*height)
                    .f64(*duration)
                    .str(codec)
            }
            Reply::OpenFailed { id } => Enc::tag(4).u32(*id),
            Reply::Frame {
                id,
                slot,
                pts,
                width,
                height,
                colorspace,
                full_range,
                decode_us,
            } => Enc::tag(5)
                .u32(*id)
                .u32(*slot)
                .f64(*pts)
                .u32(*width)
                .u32(*height)
                .u32(*colorspace)
                .u8(u8::from(*full_range))
                .u32(*decode_us),
            Reply::End { id, last_time } => Enc::tag(6).u32(*id).f64(*last_time),
            Reply::Bad { id } => Enc::tag(7).u32(*id),
            Reply::Sought { id, ok } => Enc::tag(8).u32(*id).u8(u8::from(*ok)),
            Reply::Closed { id } => Enc::tag(9).u32(*id),
            Reply::DemuxOpened { id, info } => {
                check_str(info)?;
                Enc::tag(10).u32(*id).str(info)
            }
            Reply::DemuxOpenFailed { id } => Enc::tag(11).u32(*id),
            Reply::DemuxConfig { id, len } => Enc::tag(12).u32(*id).u32(*len),
            Reply::Packet { id, record } => {
                if record.len() != PACKET_RECORD {
                    return Err(Malformed("packet record size"));
                }
                Enc::tag(13).u32(*id).bytes(record)
            }
            Reply::DemuxEnd { id } => Enc::tag(14).u32(*id),
            Reply::DemuxError { id, message } => {
                check_str(message)?;
                Enc::tag(15).u32(*id).str(message)
            }
            Reply::DemuxSought { id } => Enc::tag(16).u32(*id),
            Reply::DemuxIndexCount { id, count } => Enc::tag(17).u32(*id).i32(*count),
            Reply::DemuxIndex { id, entries } => {
                if entries.len() % INDEX_RECORD != 0
                    || entries.len() > INDEX_PAGE as usize * INDEX_RECORD
                {
                    return Err(Malformed("index page size"));
                }
                // 8 KiB at most: `bytes` takes it.
                Enc::tag(18).u32(*id).bytes(entries)
            }
            Reply::Error { message } => {
                check_str(message)?;
                Enc::tag(19).str(message)
            }
            Reply::Bye => Enc::tag(20),
        };
        Ok(e.0)
    }

    pub fn decode(bytes: &[u8]) -> Result<Reply, Malformed> {
        let mut d = Dec(bytes);
        let r = match d.u8()? {
            1 => Reply::Hello {
                version: d.u32()?,
                decoder: d.str()?,
                sandbox: d.str()?,
                threads: d.u32()?,
            },
            2 => Reply::Read {
                id: d.u32()?,
                offset: d.u64()?,
                len: d.u32()?,
            },
            3 => Reply::Opened {
                id: d.u32()?,
                width: d.u32()?,
                height: d.u32()?,
                duration: d.f64()?,
                codec: d.str()?,
            },
            4 => Reply::OpenFailed { id: d.u32()? },
            5 => Reply::Frame {
                id: d.u32()?,
                slot: d.u32()?,
                pts: d.f64()?,
                width: d.u32()?,
                height: d.u32()?,
                colorspace: d.u32()?,
                full_range: d.u8()? != 0,
                decode_us: d.u32()?,
            },
            6 => Reply::End {
                id: d.u32()?,
                last_time: d.f64()?,
            },
            7 => Reply::Bad { id: d.u32()? },
            8 => Reply::Sought {
                id: d.u32()?,
                ok: d.u8()? != 0,
            },
            9 => Reply::Closed { id: d.u32()? },
            10 => Reply::DemuxOpened {
                id: d.u32()?,
                info: d.str()?,
            },
            11 => Reply::DemuxOpenFailed { id: d.u32()? },
            12 => Reply::DemuxConfig {
                id: d.u32()?,
                len: d.u32()?,
            },
            13 => {
                let id = d.u32()?;
                let record = d.bytes()?;
                if record.len() != PACKET_RECORD {
                    return Err(Malformed("packet record size"));
                }
                Reply::Packet {
                    id,
                    record: record.to_vec(),
                }
            }
            14 => Reply::DemuxEnd { id: d.u32()? },
            15 => Reply::DemuxError {
                id: d.u32()?,
                message: d.str()?,
            },
            16 => Reply::DemuxSought { id: d.u32()? },
            17 => Reply::DemuxIndexCount {
                id: d.u32()?,
                count: d.i32()?,
            },
            18 => {
                let id = d.u32()?;
                let entries = d.bytes()?;
                if entries.len() % INDEX_RECORD != 0
                    || entries.len() > INDEX_PAGE as usize * INDEX_RECORD
                {
                    return Err(Malformed("index page size"));
                }
                Reply::DemuxIndex {
                    id,
                    entries: entries.to_vec(),
                }
            }
            19 => Reply::Error { message: d.str()? },
            20 => Reply::Bye,
            _ => return Err(Malformed("unknown reply tag")),
        };
        d.end()?;
        Ok(r)
    }
}

/// Writes one frame: `seq` and `payload` (at most [`MAX_PAYLOAD`] bytes).
pub fn write_frame(w: &mut impl Write, seq: u32, payload: &[u8]) -> io::Result<()> {
    if payload.is_empty() || payload.len() > MAX_PAYLOAD {
        return Err(Malformed("payload length out of bounds").into());
    }
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&seq.to_le_bytes());
    frame.extend_from_slice(payload);
    w.write_all(&frame)?;
    w.flush()
}

/// Reads one frame: `(seq, payload)`, or `None` at a clean end of the
/// stream (between frames only: a cut frame is an error).
pub fn read_frame(r: &mut impl Read) -> io::Result<Option<(u32, Vec<u8>)>> {
    let mut header = [0u8; 8];
    let mut got = 0;
    while got < header.len() {
        match r.read(&mut header[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
    let seq = u32::from_le_bytes(header[4..].try_into().unwrap());
    if len == 0 || len > MAX_PAYLOAD {
        return Err(Malformed("payload length out of bounds").into());
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok(Some((seq, payload)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_replies_round_trip() {
        let requests = [
            Request::Hello {
                version: VERSION,
                arena_len: Layout::LEN as u64,
                threads: 4,
            },
            Request::Open {
                id: 7,
                size: 1 << 40,
                name: "movie.avi".into(),
            },
            Request::Seek { id: 7, t: 12.5 },
            Request::SeekKey { id: 7, t: 0.0 },
            Request::Next { id: 7, slot: 1 },
            Request::Close { id: 7 },
            Request::DemuxOpen {
                id: 9,
                size: 0,
                name: String::new(),
            },
            Request::DemuxConfig { id: 9 },
            Request::DemuxNext { id: 9 },
            Request::DemuxSeek {
                id: 9,
                timestamp: -5,
            },
            Request::DemuxIndexCount { id: 9 },
            Request::DemuxIndex {
                id: 9,
                first: 256,
                count: 256,
            },
            Request::DemuxClose { id: 9 },
            Request::Data { len: 65536 },
            Request::Shutdown,
        ];
        for r in requests {
            assert_eq!(Request::decode(&r.encode().unwrap()).unwrap(), r);
        }
        let replies = [
            Reply::Hello {
                version: 1,
                decoder: "FFmpeg 9.0.2".into(),
                sandbox: "seatbelt".into(),
                threads: 4,
            },
            Reply::Read {
                id: 7,
                offset: 1 << 33,
                len: 65536,
            },
            Reply::Opened {
                id: 7,
                width: 640,
                height: 480,
                duration: -1.0,
                codec: "mpeg4".into(),
            },
            Reply::OpenFailed { id: 7 },
            Reply::Frame {
                id: 7,
                slot: 1,
                pts: 0.033,
                width: 640,
                height: 480,
                colorspace: 601,
                full_range: true,
                decode_us: 1200,
            },
            Reply::End {
                id: 7,
                last_time: 9.9,
            },
            Reply::Bad { id: 7 },
            Reply::Sought { id: 7, ok: false },
            Reply::Closed { id: 7 },
            Reply::DemuxOpened {
                id: 9,
                info: "{\"abi\":1}".into(),
            },
            Reply::DemuxOpenFailed { id: 9 },
            Reply::DemuxConfig { id: 9, len: 33 },
            Reply::Packet {
                id: 9,
                record: vec![1; PACKET_RECORD],
            },
            Reply::DemuxEnd { id: 9 },
            Reply::DemuxError {
                id: 9,
                message: "cannot read the next packet".into(),
            },
            Reply::DemuxSought { id: 9 },
            Reply::DemuxIndexCount { id: 9, count: -1 },
            Reply::DemuxIndex {
                id: 9,
                entries: vec![2; INDEX_RECORD * 3],
            },
            Reply::Error {
                message: "unknown handle".into(),
            },
            Reply::Bye,
        ];
        for r in replies {
            assert_eq!(Reply::decode(&r.encode().unwrap()).unwrap(), r);
        }
    }

    #[test]
    fn malformed_messages_are_refused() {
        assert!(Request::decode(&[]).is_err());
        assert!(Request::decode(&[99]).is_err());
        assert!(Request::decode(&[15, 0]).is_err(), "trailing byte");
        assert!(Request::decode(&[6, 1, 2]).is_err(), "truncated");
        let long = Request::Open {
            id: 1,
            size: 0,
            name: "x".repeat(MAX_STRING + 1),
        };
        assert!(long.encode().is_err());
        assert!(
            Reply::Packet {
                id: 1,
                record: vec![0; 71]
            }
            .encode()
            .is_err()
        );
        assert!(
            Reply::DemuxIndex {
                id: 1,
                entries: vec![0; 31]
            }
            .encode()
            .is_err()
        );
        // A length field past the cap.
        let mut bytes = Request::Open {
            id: 1,
            size: 0,
            name: "a".into(),
        }
        .encode()
        .unwrap();
        let at = bytes.len() - 3;
        bytes[at..at + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(Request::decode(&bytes), Err(Malformed("string too long")));
    }

    #[test]
    fn frames_round_trip_and_eof_is_only_clean_between_frames() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, 1, &[1, 2, 3]).unwrap();
        write_frame(&mut bytes, 2, &[4]).unwrap();
        let mut r = bytes.as_slice();
        assert_eq!(read_frame(&mut r).unwrap(), Some((1, vec![1, 2, 3])));
        assert_eq!(read_frame(&mut r).unwrap(), Some((2, vec![4])));
        assert_eq!(read_frame(&mut r).unwrap(), None);
        for cut in [&bytes[..3], &bytes[..9]] {
            assert_eq!(
                read_frame(&mut &cut[..]).unwrap_err().kind(),
                io::ErrorKind::UnexpectedEof
            );
        }
        let mut huge = Vec::new();
        huge.extend_from_slice(&((MAX_PAYLOAD + 1) as u32).to_le_bytes());
        huge.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_frame(&mut huge.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let mut out = Vec::new();
        assert!(write_frame(&mut out, 0, &[]).is_err());
        assert!(write_frame(&mut out, 0, &vec![0; MAX_PAYLOAD + 1]).is_err());
        assert!(out.is_empty());
    }

    #[test]
    fn the_layout_tiles_the_arena() {
        let l = Layout::new();
        let mut ranges = vec![&l.io, &l.packet, &l.side, &l.config];
        ranges.extend(l.frames.iter());
        let mut at = 0;
        for r in ranges {
            assert_eq!(r.start, at);
            assert_eq!(r.start % 4096, 0);
            at = r.end;
        }
        assert_eq!(at, Layout::LEN);
        assert!(planes_len(3840, 2160).unwrap() <= Layout::FRAME);
        assert_eq!(planes_len(3, 3), Some(9 + 8));
        assert!(l.frame(2).is_none());
    }
}
