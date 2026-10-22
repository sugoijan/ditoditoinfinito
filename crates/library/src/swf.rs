//! An FFR level as a player has it: a Flash SWF holding the chart, as
//! ActionScript 2 that assigns `_root.beatBox`, and the music, as MP3
//! (`docs/research/formats-ffr-danoni.md`, A1; `docs/plans/phase-10.md`,
//! step 2). Read from its bytes, sans IO; nothing is fetched.
//!
//! As R^3 reads a level (rCubed `Beatbox.parseBeatbox` and
//! `MP3Extraction.extractSound`, read for their behavior; the SWF format
//! from Adobe's published SWF File Format Specification, version 19):
//!
//! - **The chart.** Each `DoAction` tag's actions run on a small machine
//!   that knows only what builds an array: the constant pool, pushes,
//!   pop, duplicate, registers, variables and members got and set, and
//!   array literals; every other action is passed over, and a tag whose
//!   actions go wrong is left. The `beatBox` member of `_root` once they
//!   have run is the chart.
//! - **The music.** The MP3 frames of the level's sound stream
//!   (`SoundStreamHead` naming MP3, then each `SoundStreamBlock` less its
//!   four-byte header), or else of its first MP3 `DefineSound` (less its
//!   nine bytes; nothing after it is music), joined as they are: an MP3
//!   file. Where it starts in the game ([`Level::music_frame`]): R^3
//!   counts the `ShowFrame` tags before the stream's first block.
//!
//! Compressed levels (`CWS`, zlib) are inflated; LZMA (`ZWS`) ones are
//! refused. Everything is bounded: the inflated size, the actions run, the
//! stack, the arrays.

use std::collections::HashMap;

use ddi_chart::formats::ffr::Value;

/// The largest level read, inflated (a level is its music: a few MB).
pub const MAX_SIZE: usize = 256 << 20;
/// The most actions run in one level.
const MAX_ACTIONS: usize = 20_000_000;
/// The deepest stack.
const MAX_STACK: usize = 4_000_000;
/// The most values made by array literals in one level.
const MAX_VALUES: usize = 8_000_000;

const TAG_END: u16 = 0;
const TAG_SHOW_FRAME: u16 = 1;
const TAG_DO_ACTION: u16 = 12;
const TAG_DEFINE_SOUND: u16 = 14;
const TAG_SOUND_STREAM_HEAD: u16 = 18;
const TAG_SOUND_STREAM_BLOCK: u16 = 19;
const TAG_SOUND_STREAM_HEAD2: u16 = 45;
const TAG_DO_INIT_ACTION: u16 = 59;

/// `SoundFormat` 2: MP3.
const MP3: u8 = 2;

/// What a level holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Level {
    /// `_root.beatBox`, if the actions set one.
    pub beatbox: Option<Value>,
    /// The music, an MP3 file (empty: none).
    pub mp3: Vec<u8>,
    /// The SWF frame (from 0) of the sound stream's first block with
    /// music; none when the music is a `DefineSound`'s, or there is none.
    pub stream_frame: Option<u32>,
    /// The music's sample rate as the SWF declares it, Hz (0: unknown).
    pub sample_rate: u32,
    /// The SWF's own frame rate, frames a second.
    pub frame_rate: f64,
    /// The SWF version.
    pub version: u8,
}

impl Level {
    /// R^3's `mp3Frame` (`ddi_chart::formats::ffr::LevelInfo::music_frame`):
    /// the game frame its music starts at. `MP3Extraction.extractSound`
    /// gives the frame of the stream's first block (−1 without a stream),
    /// and `Song` takes 2 from it, so a stream from the SWF's first frame
    /// starts two frames before the game's frame 0, and a `DefineSound`'s
    /// music three.
    pub fn music_frame(&self) -> i64 {
        self.stream_frame.map_or(-1, i64::from) - 2
    }
}

/// Whether `bytes` start as a SWF does.
pub fn is_swf(bytes: &[u8]) -> bool {
    matches!(bytes.get(..3), Some(b"FWS" | b"CWS" | b"ZWS"))
}

/// The level in `bytes`.
pub fn read(bytes: &[u8]) -> Result<Level, String> {
    if bytes.len() < 8 || !is_swf(bytes) {
        return Err("not a SWF file".into());
    }
    let version = bytes[3];
    let declared = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
    let inflated;
    let body: &[u8] = match bytes[0] {
        b'F' => &bytes[8..],
        b'C' => {
            let limit = declared.saturating_sub(8).min(MAX_SIZE);
            inflated = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[8..], limit)
                .map_err(|e| format!("the SWF does not inflate: {:?}", e.status))?;
            &inflated
        }
        _ => return Err("an LZMA-compressed SWF (ZWS) is not read".into()),
    };
    let mut r = Bytes::new(body);
    // The frame's rectangle: 5 bits of size, then four fields of it.
    let bits = usize::from(r.peek_u8()? >> 3);
    r.skip((5 + 4 * bits).div_ceil(8))?;
    let frame_rate = f64::from(r.u16()?) / 256.0;
    let _frames = r.u16()?;
    let mut level = Level {
        frame_rate,
        version,
        ..Level::default()
    };
    let mut machine = Machine::default();
    // As `extractSound`: a stream once named MP3 stays one, and a
    // `DefineSound` taken ends the music.
    let mut streaming = false;
    let mut defined = false;
    let mut frames = 0u32;
    while !r.is_empty() {
        let head = r.u16()?;
        let code = head >> 6;
        let mut len = usize::from(head & 0x3f);
        if len == 0x3f {
            len = r.u32()? as usize;
        }
        let tag = r.take(len)?;
        match code {
            TAG_END => break,
            TAG_SHOW_FRAME => frames = frames.saturating_add(1),
            TAG_DO_ACTION => machine.run(tag),
            // A sprite's id, then its actions.
            TAG_DO_INIT_ACTION if tag.len() >= 2 => machine.run(&tag[2..]),
            TAG_SOUND_STREAM_HEAD | TAG_SOUND_STREAM_HEAD2
                if !defined && tag.len() >= 2 && tag[1] >> 4 == MP3 =>
            {
                streaming = true;
                level.sample_rate = rate(tag[1]);
            }
            TAG_SOUND_STREAM_BLOCK if streaming && !defined && tag.len() > 4 => {
                level.stream_frame.get_or_insert(frames);
                level.mp3.extend_from_slice(&tag[4..]);
            }
            TAG_DEFINE_SOUND if !streaming && !defined && tag.len() > 9 && tag[2] >> 4 == MP3 => {
                level.sample_rate = rate(tag[2]);
                level.mp3.extend_from_slice(&tag[9..]);
                defined = true;
            }
            _ => {}
        }
        if level.mp3.len() > MAX_SIZE {
            return Err("the music is too long".into());
        }
    }
    level.beatbox = machine.beatbox();
    Ok(level)
}

/// A level SWF of our own, as Flash compiles one (tests and fixtures:
/// no FFR level is ever needed to check the game): `notes` as
/// `_root.beatBox = [[frame, direction, color], …]` in one `DoAction`,
/// `mp3` as a 44.1 kHz MP3 sound stream in blocks of a few kilobytes,
/// zlib-compressed (`CWS`) or not; the stream from the SWF's first frame.
pub fn write_level(notes: &[(f64, &str, &str)], mp3: &[u8], compressed: bool) -> Vec<u8> {
    write_level_at(notes, mp3, compressed, 0)
}

/// [`write_level`], the stream's first block in SWF frame `stream_frame`
/// (the frames before it empty but for the chart's actions in the first).
pub fn write_level_at(
    notes: &[(f64, &str, &str)],
    mp3: &[u8],
    compressed: bool,
    stream_frame: u16,
) -> Vec<u8> {
    fn push(code: &mut Vec<u8>, values: &[&[u8]]) {
        let data: Vec<u8> = values.concat();
        code.push(ACTION_PUSH);
        code.extend_from_slice(&(data.len() as u16).to_le_bytes());
        code.extend_from_slice(&data);
    }
    let text = |s: &str| [&[0u8][..], s.as_bytes(), &[0]].concat();
    let double = |x: f64| {
        let b = x.to_le_bytes();
        [&[6u8][..], &b[4..], &b[..4]].concat()
    };
    let int = |i: i32| [&[7u8][..], &i.to_le_bytes()].concat();
    let mut code = Vec::new();
    push(&mut code, &[&text("_root")]);
    code.push(ACTION_GET_VARIABLE);
    push(&mut code, &[&text("beatBox")]);
    for (frame, direction, color) in notes.iter().rev() {
        push(
            &mut code,
            &[&text(color), &text(direction), &double(*frame), &int(3)],
        );
        code.push(ACTION_INIT_ARRAY);
    }
    push(&mut code, &[&int(notes.len() as i32)]);
    code.push(ACTION_INIT_ARRAY);
    code.push(ACTION_SET_MEMBER);
    code.push(ACTION_END);
    // Playback and stream: MP3, 44.1 kHz, 16-bit stereo; 1,470 samples a
    // frame; no latency. A frame a block.
    let mut swf = Swf::default()
        .tag(TAG_SOUND_STREAM_HEAD2, &[0x0f, 0x2f, 0xbe, 0x05, 0, 0])
        .tag(TAG_DO_ACTION, &code);
    for _ in 0..stream_frame {
        swf = swf.tag(TAG_SHOW_FRAME, &[]);
    }
    for block in mp3.chunks(4096) {
        let mut b = vec![0xbe, 0x05, 0, 0];
        b.extend_from_slice(block);
        swf = swf.tag(TAG_SOUND_STREAM_BLOCK, &b).tag(TAG_SHOW_FRAME, &[]);
    }
    swf.bytes(compressed)
}

/// A SWF writer, for [`write_level`] and the tests' own levels.
#[derive(Default)]
pub(crate) struct Swf {
    tags: Vec<u8>,
    frames: u16,
}

impl Swf {
    pub(crate) fn tag(mut self, code: u16, body: &[u8]) -> Self {
        if body.len() < 0x3f {
            self.tags
                .extend_from_slice(&((code << 6) | body.len() as u16).to_le_bytes());
        } else {
            self.tags
                .extend_from_slice(&((code << 6) | 0x3f).to_le_bytes());
            self.tags
                .extend_from_slice(&(body.len() as u32).to_le_bytes());
        }
        self.tags.extend_from_slice(body);
        if code == TAG_SHOW_FRAME {
            self.frames += 1;
        }
        self
    }

    /// The file, uncompressed (`FWS`) or zlib-compressed (`CWS`).
    pub(crate) fn bytes(&self, compressed: bool) -> Vec<u8> {
        // A 0-bit rectangle (one byte), 30 fps, its frames (at least one).
        let mut body = vec![0u8, 0, 30];
        body.extend_from_slice(&self.frames.max(1).to_le_bytes());
        body.extend_from_slice(&self.tags);
        body.extend_from_slice(&[0, 0]);
        let len = (body.len() + 8) as u32;
        let mut out = if compressed { b"CWS" } else { b"FWS" }.to_vec();
        out.push(8);
        out.extend_from_slice(&len.to_le_bytes());
        if compressed {
            out.extend(miniz_oxide::deflate::compress_to_vec_zlib(&body, 6));
        } else {
            out.extend(body);
        }
        out
    }
}

/// A sound format byte's rate (bits 2–3).
fn rate(format: u8) -> u32 {
    [5512, 11025, 22050, 44100][usize::from((format >> 2) & 3)]
}

struct Bytes<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Bytes { bytes, at: 0 }
    }

    fn is_empty(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&e| e <= self.bytes.len())
            .ok_or("the SWF is cut short")?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.take(n).map(|_| ())
    }

    fn peek_u8(&self) -> Result<u8, String> {
        self.bytes
            .get(self.at)
            .copied()
            .ok_or_else(|| "the SWF is cut short".into())
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A null-terminated string (UTF-8 from SWF 6; older ones' bytes
    /// taken as they come).
    fn string(&mut self) -> Result<String, String> {
        let rest = &self.bytes[self.at.min(self.bytes.len())..];
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or("a string not ended")?;
        let s = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.at += end + 1;
        Ok(s)
    }
}

/// A value on the machine: as the chart has them, or an object (by its
/// place among the machine's objects).
#[derive(Clone, Debug, PartialEq)]
enum Slot {
    Value(Value),
    Object(usize),
}

/// Just enough of ActionScript 2's machine to build an array.
#[derive(Default)]
struct Machine {
    /// `_root`'s members are object 0's.
    objects: Vec<HashMap<String, Slot>>,
    /// Actions run so far.
    actions: usize,
    /// Values made so far.
    values: usize,
}

const ACTION_END: u8 = 0x00;
const ACTION_POP: u8 = 0x17;
const ACTION_GET_VARIABLE: u8 = 0x1c;
const ACTION_SET_VARIABLE: u8 = 0x1d;
const ACTION_INIT_ARRAY: u8 = 0x42;
const ACTION_PUSH_DUPLICATE: u8 = 0x4c;
const ACTION_GET_MEMBER: u8 = 0x4e;
const ACTION_SET_MEMBER: u8 = 0x4f;
const ACTION_STORE_REGISTER: u8 = 0x87;
const ACTION_CONSTANT_POOL: u8 = 0x88;
const ACTION_PUSH: u8 = 0x96;

/// The names the main timeline goes by.
fn is_root(name: &str) -> bool {
    matches!(name, "_root" | "_level0" | "this" | "/")
}

impl Machine {
    fn root(&mut self) -> usize {
        if self.objects.is_empty() {
            self.objects.push(HashMap::new());
        }
        0
    }

    fn new_object(&mut self) -> Slot {
        self.objects.push(HashMap::new());
        Slot::Object(self.objects.len() - 1)
    }

    /// Runs a tag's actions; one that goes wrong ends the tag (what it set
    /// before stays, as in R^3).
    fn run(&mut self, actions: &[u8]) {
        let _ = self.run_actions(actions);
    }

    fn run_actions(&mut self, actions: &[u8]) -> Result<(), String> {
        let root = self.root();
        let mut r = Bytes::new(actions);
        let mut pool: Vec<String> = Vec::new();
        let mut stack: Vec<Slot> = Vec::new();
        let mut registers: [Option<Slot>; 256] = std::array::from_fn(|_| None);
        let pop = |stack: &mut Vec<Slot>| stack.pop().unwrap_or(Slot::Value(Value::Null));
        while !r.is_empty() {
            self.actions += 1;
            if self.actions > MAX_ACTIONS {
                return Err("too many actions".into());
            }
            let code = r.u8()?;
            let data = if code >= 0x80 {
                let len = usize::from(r.u16()?);
                r.take(len)?
            } else {
                &[][..]
            };
            let mut d = Bytes::new(data);
            match code {
                ACTION_END => break,
                ACTION_CONSTANT_POOL => {
                    let n = d.u16()?;
                    pool = (0..n).map(|_| d.string()).collect::<Result<_, _>>()?;
                }
                ACTION_PUSH => {
                    while !d.is_empty() {
                        let slot = match d.u8()? {
                            0 => Slot::Value(Value::Text(d.string()?)),
                            1 => {
                                let b = d.take(4)?;
                                Slot::Value(Value::Number(f64::from(f32::from_le_bytes([
                                    b[0], b[1], b[2], b[3],
                                ]))))
                            }
                            2 | 3 => Slot::Value(Value::Null),
                            4 => registers[usize::from(d.u8()?)]
                                .clone()
                                .unwrap_or(Slot::Value(Value::Null)),
                            5 => Slot::Value(Value::Bool(d.u8()? != 0)),
                            6 => {
                                // The high half first, each half little-endian.
                                let b = d.take(8)?;
                                Slot::Value(Value::Number(f64::from_le_bytes([
                                    b[4], b[5], b[6], b[7], b[0], b[1], b[2], b[3],
                                ])))
                            }
                            7 => {
                                let b = d.take(4)?;
                                Slot::Value(Value::Number(f64::from(i32::from_le_bytes([
                                    b[0], b[1], b[2], b[3],
                                ]))))
                            }
                            8 => constant(&pool, usize::from(d.u8()?))?,
                            9 => constant(&pool, usize::from(d.u16()?))?,
                            other => return Err(format!("a push of type {other}")),
                        };
                        if stack.len() >= MAX_STACK {
                            return Err("the stack is too deep".into());
                        }
                        stack.push(slot);
                    }
                }
                ACTION_POP => {
                    pop(&mut stack);
                }
                ACTION_PUSH_DUPLICATE => {
                    let top = stack.last().cloned().unwrap_or(Slot::Value(Value::Null));
                    if stack.len() >= MAX_STACK {
                        return Err("the stack is too deep".into());
                    }
                    stack.push(top);
                }
                ACTION_STORE_REGISTER => {
                    registers[usize::from(d.u8()?)] = stack.last().cloned();
                }
                ACTION_GET_VARIABLE => {
                    let name = text(&pop(&mut stack));
                    let slot = if is_root(&name) {
                        Slot::Object(root)
                    } else {
                        let (object, member) = self.path(root, &name);
                        self.member(object, &member)
                    };
                    stack.push(slot);
                }
                ACTION_SET_VARIABLE => {
                    let value = pop(&mut stack);
                    let name = text(&pop(&mut stack));
                    let (object, member) = self.path(root, &name);
                    self.objects[object].insert(member, value);
                }
                ACTION_GET_MEMBER => {
                    let name = text(&pop(&mut stack));
                    let slot = match pop(&mut stack) {
                        Slot::Object(o) => self.member(o, &name),
                        _ => Slot::Value(Value::Null),
                    };
                    stack.push(slot);
                }
                ACTION_SET_MEMBER => {
                    let value = pop(&mut stack);
                    let name = text(&pop(&mut stack));
                    if let Slot::Object(o) = pop(&mut stack) {
                        self.objects[o].insert(name, value);
                    }
                }
                ACTION_INIT_ARRAY => {
                    let n = match pop(&mut stack) {
                        Slot::Value(Value::Number(n)) if n >= 0.0 => n as usize,
                        _ => return Err("an array of no length".into()),
                    };
                    if n > stack.len() {
                        return Err("an array longer than the stack".into());
                    }
                    self.values += n;
                    if self.values > MAX_VALUES {
                        return Err("too many values".into());
                    }
                    // The first element is on top.
                    let items: Vec<Value> = stack
                        .drain(stack.len() - n..)
                        .rev()
                        .map(|s| self.value(&s))
                        .collect();
                    stack.push(Slot::Value(Value::List(items)));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// `a.b.c`, `/a:b` or `a:b` as an object and a member of it (made
    /// where missing).
    fn path(&mut self, root: usize, name: &str) -> (usize, String) {
        let parts: Vec<&str> = name
            .split(['.', ':', '/'])
            .filter(|p| !p.is_empty())
            .collect();
        let Some((last, objects)) = parts.split_last() else {
            return (root, name.to_string());
        };
        let mut object = root;
        for p in objects {
            if is_root(p) {
                object = root;
                continue;
            }
            object = match self.member(object, p) {
                Slot::Object(o) => o,
                _ => {
                    let Slot::Object(o) = self.new_object() else {
                        unreachable!()
                    };
                    self.objects[object].insert(p.to_string(), Slot::Object(o));
                    o
                }
            };
        }
        (object, last.to_string())
    }

    /// A member, an object of its own where missing (as R^3 makes them:
    /// `_root.a.b` reads without `a`).
    fn member(&mut self, object: usize, name: &str) -> Slot {
        if let Some(slot) = self.objects.get(object).and_then(|o| o.get(name)) {
            return slot.clone();
        }
        let made = self.new_object();
        self.objects[object].insert(name.to_string(), made.clone());
        made
    }

    /// A slot as the chart's value (objects are none of its).
    fn value(&self, slot: &Slot) -> Value {
        match slot {
            Slot::Value(v) => v.clone(),
            Slot::Object(_) => Value::Other,
        }
    }

    /// `_root.beatBox`, when it is an array.
    fn beatbox(&self) -> Option<Value> {
        match self.objects.first()?.get("beatBox")? {
            Slot::Value(v @ Value::List(_)) => Some(v.clone()),
            _ => None,
        }
    }
}

fn constant(pool: &[String], i: usize) -> Result<Slot, String> {
    pool.get(i)
        .map(|s| Slot::Value(Value::Text(s.clone())))
        .ok_or_else(|| format!("no constant {i}"))
}

/// A name as text.
fn text(slot: &Slot) -> String {
    match slot {
        Slot::Value(Value::Text(t)) => t.clone(),
        Slot::Value(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// ActionScript 2 bytecode, as a compiler writes it.
    #[derive(Default)]
    pub(crate) struct As2 {
        pub(crate) code: Vec<u8>,
    }

    pub(crate) enum P<'a> {
        S(&'a str),
        I(i32),
        D(f64),
        F(f32),
        C(u8),
        Null,
        Reg(u8),
    }

    impl As2 {
        fn action(mut self, code: u8, data: &[u8]) -> Self {
            self.code.push(code);
            if code >= 0x80 {
                self.code
                    .extend_from_slice(&(data.len() as u16).to_le_bytes());
                self.code.extend_from_slice(data);
            }
            self
        }

        pub(crate) fn pool(self, names: &[&str]) -> Self {
            let mut d = (names.len() as u16).to_le_bytes().to_vec();
            for n in names {
                d.extend_from_slice(n.as_bytes());
                d.push(0);
            }
            self.action(ACTION_CONSTANT_POOL, &d)
        }

        pub(crate) fn push(self, values: &[P]) -> Self {
            let mut d = Vec::new();
            for v in values {
                match v {
                    P::S(s) => {
                        d.push(0);
                        d.extend_from_slice(s.as_bytes());
                        d.push(0);
                    }
                    P::F(f) => {
                        d.push(1);
                        d.extend_from_slice(&f.to_le_bytes());
                    }
                    P::Null => d.push(2),
                    P::Reg(r) => d.extend_from_slice(&[4, *r]),
                    P::D(x) => {
                        d.push(6);
                        let b = x.to_le_bytes();
                        d.extend_from_slice(&b[4..]);
                        d.extend_from_slice(&b[..4]);
                    }
                    P::I(i) => {
                        d.push(7);
                        d.extend_from_slice(&i.to_le_bytes());
                    }
                    P::C(c) => d.extend_from_slice(&[8, *c]),
                }
            }
            self.action(ACTION_PUSH, &d)
        }

        pub(crate) fn op(self, code: u8) -> Self {
            self.action(code, &[])
        }

        pub(crate) fn store(self, r: u8) -> Self {
            self.action(ACTION_STORE_REGISTER, &[r])
        }

        pub(crate) fn end(self) -> Vec<u8> {
            let mut c = self.code;
            c.push(ACTION_END);
            c
        }
    }

    /// `_root.beatBox = [[f, d, c], …]` as Flash compiles it: `_root`
    /// got, `beatBox` pushed, each note's array (its values last first),
    /// the outer array, set as a member.
    pub(crate) fn beatbox_actions(notes: &[(i32, &str, &str)]) -> Vec<u8> {
        let mut a = As2::default()
            .pool(&["_root", "beatBox", "L", "D", "U", "R", "red", "blue"])
            .push(&[P::C(0)])
            .op(ACTION_GET_VARIABLE)
            .push(&[P::C(1)]);
        for (frame, dir, color) in notes.iter().rev() {
            let c = |s: &str| {
                ["_root", "beatBox", "L", "D", "U", "R", "red", "blue"]
                    .iter()
                    .position(|x| *x == s)
                    .unwrap() as u8
            };
            a = a
                .push(&[P::C(c(color)), P::C(c(dir)), P::I(*frame), P::I(3)])
                .op(ACTION_INIT_ARRAY);
        }
        a.push(&[P::I(notes.len() as i32)])
            .op(ACTION_INIT_ARRAY)
            .op(ACTION_SET_MEMBER)
            .end()
    }

    /// A short MP3 stream's frames (headers only: what matters is that
    /// they are carried over byte for byte).
    fn mp3_frames(n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..n {
            out.extend_from_slice(&[0xff, 0xfb, 0x90, 0x64]);
            out.extend(std::iter::repeat_n(i as u8, 413));
        }
        out
    }

    fn stream_head() -> Vec<u8> {
        // Playback 44.1 kHz 16-bit stereo; stream MP3 44.1 kHz; 1470
        // samples a frame; latency seek 0.
        vec![0x0f, 0x2f, 0xbe, 0x05, 0, 0]
    }

    fn block(frames: &[u8]) -> Vec<u8> {
        let mut b = vec![0x80, 0x04, 0, 0];
        b.extend_from_slice(frames);
        b
    }

    #[test]
    fn a_level_gives_its_beatbox_and_its_music() {
        let notes = [(30, "L", "red"), (45, "D", "blue"), (60, "R", "red")];
        let frames = mp3_frames(4);
        for compressed in [false, true] {
            let swf = Swf::default()
                .tag(TAG_SOUND_STREAM_HEAD2, &stream_head())
                .tag(TAG_DO_ACTION, &beatbox_actions(&notes))
                .tag(TAG_SOUND_STREAM_BLOCK, &block(&frames[..834]))
                .tag(TAG_SHOW_FRAME, &[])
                .tag(TAG_SOUND_STREAM_BLOCK, &block(&frames[834..]))
                .tag(TAG_SHOW_FRAME, &[])
                .bytes(compressed);
            assert!(is_swf(&swf));
            let level = read(&swf).unwrap();
            assert_eq!(level.mp3, frames, "compressed {compressed}");
            assert_eq!(level.sample_rate, 44100);
            assert_eq!(level.frame_rate, 30.0);
            assert_eq!((level.stream_frame, level.music_frame()), (Some(0), -2));
            let Some(Value::List(entries)) = &level.beatbox else {
                panic!("{:?}", level.beatbox)
            };
            assert_eq!(entries.len(), 3);
            assert_eq!(
                entries[1],
                Value::List(vec![
                    Value::Number(45.0),
                    Value::Text("D".into()),
                    Value::Text("blue".into())
                ])
            );
        }
    }

    #[test]
    fn a_level_is_a_song_and_its_music() {
        let frames = mp3_frames(2);
        let swf = Swf::default()
            .tag(TAG_SOUND_STREAM_HEAD, &stream_head())
            .tag(
                TAG_DO_ACTION,
                &beatbox_actions(&[(30, "L", "red"), (31, "R", "blue")]),
            )
            .tag(TAG_SOUND_STREAM_BLOCK, &block(&frames))
            .bytes(true);
        let c = &crate::ffr::find_levels(&["Levels/level_42.swf"], &[], "x")[0];
        let level = crate::ffr::read_level(c, &swf).unwrap().unwrap();
        assert_eq!(level.mp3, Some(frames.clone()));
        assert_eq!(level.song.title, "level_42");
        // The stream from frame 0: frame 30 heard 32.5 frames in.
        let notes = &level.song.charts[0].notes;
        assert_eq!((notes[0].tick.0, notes[0].lane), (1300, 0));
        assert_eq!((notes[1].tick.0, notes[1].lane), (1340, 3));
        // No music: refused; no chart: not a level.
        let silent = Swf::default()
            .tag(TAG_DO_ACTION, &beatbox_actions(&[(30, "L", "red")]))
            .bytes(false);
        assert!(crate::ffr::read_level(c, &silent).is_err());
        let chartless = Swf::default()
            .tag(TAG_SOUND_STREAM_HEAD, &stream_head())
            .tag(TAG_SOUND_STREAM_BLOCK, &block(&frames))
            .bytes(false);
        assert_eq!(crate::ffr::read_level(c, &chartless).unwrap(), None);
    }

    #[test]
    fn a_level_written_reads_back() {
        let mp3 = mp3_frames(30);
        let notes = [(30.0, "L", "red"), (31.5, "R", "blue")];
        for compressed in [false, true] {
            let level = read(&write_level(&notes, &mp3, compressed)).unwrap();
            assert_eq!(level.mp3, mp3);
            assert_eq!(level.sample_rate, 44100);
            assert_eq!(level.stream_frame, Some(0));
            let later = read(&write_level_at(&notes, &mp3, compressed, 7)).unwrap();
            assert_eq!((later.stream_frame, later.music_frame()), (Some(7), 5));
            assert_eq!(
                (later.mp3, later.beatbox),
                (mp3.clone(), level.beatbox.clone())
            );
            assert_eq!(
                level.beatbox,
                Some(Value::List(vec![
                    Value::List(vec![
                        Value::Number(30.0),
                        Value::Text("L".into()),
                        Value::Text("red".into())
                    ]),
                    Value::List(vec![
                        Value::Number(31.5),
                        Value::Text("R".into()),
                        Value::Text("blue".into())
                    ]),
                ]))
            );
        }
    }

    /// An FFR level's chart plays: autoplayed on time, every note the best
    /// judgement, at the times R^3 has its frames in the music (the
    /// stream from the SWF's first frame: two and a half frames later).
    #[test]
    fn a_level_plays() {
        use ddi_engine::player::{PlayOptions, Player};
        let notes: Vec<(i32, &str, &str)> = (0..64)
            .map(|i| (30 + 3 * i, ["L", "D", "U", "R"][i as usize % 4], "red"))
            .collect();
        let swf = Swf::default()
            .tag(TAG_SOUND_STREAM_HEAD, &stream_head())
            .tag(TAG_DO_ACTION, &beatbox_actions(&notes))
            .tag(TAG_SOUND_STREAM_BLOCK, &block(&mp3_frames(1)))
            .bytes(false);
        let song =
            crate::ffr::chart_song(crate::path::RelPath::new("level.swf"), &swf, "level").unwrap();
        let rules = ddi_engine::rules::presets::itg().key();
        let player = Player::new(
            &song,
            0,
            rules.build(&song.charts[0]),
            PlayOptions::default(),
        );
        let judged: Vec<f64> = player.judge().notes().iter().map(|n| n.seconds).collect();
        for (t, (frame, _, _)) in judged.iter().zip(&notes) {
            assert!(
                (t - (f64::from(*frame) + 2.5) / 30.0).abs() < 1e-9,
                "{t} {frame}"
            );
        }
        let mut inputs = Vec::new();
        for p in ddi_engine::autoplay::script(player.judge().notes(), 0.0) {
            for (time, pressed) in [(p.press, true), (p.release, false)] {
                inputs.push(ddi_engine::history::LoggedInput {
                    time,
                    lane: p.lane,
                    pressed,
                });
            }
        }
        inputs.sort_by(|a, b| a.time.total_cmp(&b.time));
        let log = ddi_engine::history::PlayLog {
            start: 0.0,
            inputs,
            ..Default::default()
        };
        let results = ddi_engine::history::replay(&song, 0, &rules, &PlayOptions::default(), &log);
        assert_eq!(results.tally.taps[0], 64, "{:?}", results.tally);
        assert!(!results.failed);
    }

    #[test]
    fn the_first_mp3_sound_is_the_music_without_a_stream() {
        let frames = mp3_frames(2);
        let mut adpcm = vec![1, 0, 0x1e];
        adpcm.extend_from_slice(&[0; 20]);
        let mut define = vec![2, 0, 0x2e, 0, 0, 0, 0, 0, 0];
        define.extend_from_slice(&frames);
        let mut second = define.clone();
        second.push(9);
        let swf = Swf::default()
            .tag(TAG_DEFINE_SOUND, &adpcm)
            .tag(TAG_DEFINE_SOUND, &define)
            .tag(TAG_DEFINE_SOUND, &second)
            .bytes(false);
        let level = read(&swf).unwrap();
        assert_eq!(level.mp3, frames);
        assert_eq!(level.sample_rate, 44100);
        assert_eq!(level.beatbox, None);
        // No stream: R^3's frame −1, less 2.
        assert_eq!((level.stream_frame, level.music_frame()), (None, -3));
    }

    #[test]
    fn the_music_starts_where_the_stream_does() {
        let frames = mp3_frames(4);
        let empty = vec![0x80, 0x04, 0, 0];
        // Frames 0–2 without music (an empty block in frame 1, a stream
        // head of another codec after the MP3 one), the first block in
        // frame 3.
        let mut adpcm_head = stream_head();
        adpcm_head[1] = 0x1f;
        let swf = Swf::default()
            .tag(TAG_SOUND_STREAM_HEAD2, &stream_head())
            .tag(TAG_DO_ACTION, &beatbox_actions(&[(30, "L", "red")]))
            .tag(TAG_SHOW_FRAME, &[])
            .tag(TAG_SOUND_STREAM_BLOCK, &empty)
            .tag(TAG_SHOW_FRAME, &[])
            .tag(TAG_SOUND_STREAM_HEAD, &adpcm_head)
            .tag(TAG_SHOW_FRAME, &[])
            .tag(TAG_SOUND_STREAM_BLOCK, &block(&frames[..834]))
            .tag(TAG_SHOW_FRAME, &[])
            .tag(TAG_SOUND_STREAM_BLOCK, &block(&frames[834..]))
            .bytes(false);
        let level = read(&swf).unwrap();
        assert_eq!(level.mp3, frames);
        assert_eq!((level.stream_frame, level.music_frame()), (Some(3), 1));
        // A `DefineSound` taken first: what streams after it is not music.
        let mut define = vec![2, 0, 0x2e, 0, 0, 0, 0, 0, 0];
        define.extend_from_slice(&frames[..834]);
        let swf = Swf::default()
            .tag(TAG_DEFINE_SOUND, &define)
            .tag(TAG_SOUND_STREAM_HEAD2, &stream_head())
            .tag(TAG_SOUND_STREAM_BLOCK, &block(&frames[834..]))
            .tag(TAG_SHOW_FRAME, &[])
            .bytes(false);
        let level = read(&swf).unwrap();
        assert_eq!(level.mp3, frames[..834]);
        assert_eq!(level.music_frame(), -3);
    }

    #[test]
    fn other_ways_of_building_the_array_are_read() {
        // `beatBox = [[1.5, "U", "red"]]` set as a variable, its note
        // through a register, a double and a float, an action passed
        // over, and a sprite's init actions.
        let code = As2::default()
            .push(&[P::S("beatBox")])
            .push(&[P::S("red"), P::S("U"), P::D(1.5), P::I(3)])
            .op(ACTION_INIT_ARRAY)
            .store(1)
            .op(ACTION_POP)
            .op(0x0a) // Add: not this machine's.
            .push(&[P::Reg(1), P::F(2.0), P::Null, P::I(1)])
            .op(ACTION_POP)
            .op(ACTION_POP)
            .op(ACTION_POP)
            .push(&[P::I(1)])
            .op(ACTION_INIT_ARRAY)
            .op(ACTION_SET_VARIABLE)
            .end();
        let mut init = vec![7, 0];
        init.extend_from_slice(&code);
        let level = read(&Swf::default().tag(TAG_DO_INIT_ACTION, &init).bytes(false)).unwrap();
        assert_eq!(
            level.beatbox,
            Some(Value::List(vec![Value::List(vec![
                Value::Number(1.5),
                Value::Text("U".into()),
                Value::Text("red".into())
            ])]))
        );
        // `_root.beatBox` named as a path.
        let code = As2::default()
            .push(&[P::S("_root.beatBox"), P::I(0)])
            .op(ACTION_INIT_ARRAY)
            .op(ACTION_SET_VARIABLE)
            .end();
        let level = read(&Swf::default().tag(TAG_DO_ACTION, &code).bytes(true)).unwrap();
        assert_eq!(level.beatbox, Some(Value::List(Vec::new())));
    }

    #[test]
    fn broken_levels_are_refused_or_left_alone() {
        assert!(read(b"").is_err());
        assert!(read(b"PK\x03\x04....").is_err());
        assert!(read(b"ZWS\x0a\0\0\0\0rest").is_err());
        // Cut short, and a compressed body that is not zlib.
        let swf = Swf::default()
            .tag(TAG_DO_ACTION, &beatbox_actions(&[(1, "L", "red")]))
            .bytes(false);
        assert!(read(&swf[..swf.len() - 10]).is_err());
        let mut bad = swf.clone();
        bad[0] = b'C';
        assert!(read(&bad).is_err());
        // A compressed level that says it is larger than it is, or
        // inflates past what it says: bounded by what it says.
        let mut big = Swf::default()
            .tag(TAG_SOUND_STREAM_HEAD, &stream_head())
            .tag(TAG_SOUND_STREAM_BLOCK, &block(&vec![0; 60_000]))
            .bytes(true);
        big[4..8].copy_from_slice(&1000u32.to_le_bytes());
        assert!(read(&big).is_err());
        // Actions that go wrong end their tag; the level still reads.
        for code in [
            vec![ACTION_PUSH, 5, 0, 0, b'x'], // a string not ended
            vec![ACTION_PUSH, 1, 0, 42],      // an unknown push type
            vec![ACTION_PUSH, 2, 0, 8, 3],    // no such constant
            vec![ACTION_INIT_ARRAY],          // no length
            vec![ACTION_PUSH, 5, 0, 7, 9, 0, 0, 0, ACTION_INIT_ARRAY], // longer than the stack
            vec![ACTION_PUSH, 200, 0],        // cut short
        ] {
            let level = read(&Swf::default().tag(TAG_DO_ACTION, &code).bytes(false)).unwrap();
            assert_eq!(level.beatbox, None);
        }
        // A level whose beatBox is not an array has none.
        let code = As2::default()
            .push(&[P::S("beatBox"), P::I(5)])
            .op(ACTION_SET_VARIABLE)
            .end();
        let level = read(&Swf::default().tag(TAG_DO_ACTION, &code).bytes(false)).unwrap();
        assert_eq!(level.beatbox, None);
    }

    /// Levels damaged anywhere read or are refused; none panics.
    #[test]
    fn damaged_levels_never_panic() {
        let notes: Vec<(i32, &str, &str)> = (0..20).map(|i| (i, "L", "red")).collect();
        let swf = Swf::default()
            .tag(TAG_SOUND_STREAM_HEAD, &stream_head())
            .tag(TAG_DO_ACTION, &beatbox_actions(&notes))
            .tag(TAG_SOUND_STREAM_BLOCK, &block(&mp3_frames(2)))
            .bytes(false);
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..3000 {
            let mut bad = swf.clone();
            for _ in 0..1 + next() % 4 {
                let at = 8 + (next() as usize) % (bad.len() - 8);
                bad[at] = next() as u8;
            }
            if next() % 5 == 0 {
                bad.truncate(8 + (next() as usize) % (bad.len() - 8));
            }
            if let Ok(level) = read(&bad)
                && let Some(b) = level.beatbox
            {
                let _ = ddi_chart::formats::ffr::song_from_beatbox(&b, &Default::default());
            }
        }
    }

    #[test]
    fn endless_actions_are_bounded() {
        // A tag of pushes and pops long enough to pass the bound many
        // times over, in a few tags: it stops.
        let mut code = Vec::new();
        for _ in 0..20_000 {
            code.extend_from_slice(&[ACTION_PUSH, 5, 0, 7, 1, 0, 0, 0, ACTION_POP]);
        }
        let mut swf = Swf::default();
        for _ in 0..600 {
            swf = swf.tag(TAG_DO_ACTION, &code);
        }
        let level = read(&swf.bytes(true)).unwrap();
        assert_eq!(level.beatbox, None);
    }
}
