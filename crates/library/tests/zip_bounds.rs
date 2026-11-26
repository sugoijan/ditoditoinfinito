//! Hostile zip entries: a few dozen bytes that decompress to a megabyte,
//! in an entry that says 16 KiB. However an entry is read (whole, in
//! pieces or only its start), the reader must stop it while it
//! decompresses: what this thread allocates meanwhile is counted, and
//! what it writes where it comes in pieces. Small numbers only: the
//! most any of these ever makes is about 1 MiB.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use ddi_library::zip::{self, Method, ZipEntry, ZipError};

/// The system's allocator, counting what each thread holds.
struct Counting;

thread_local! {
    static HELD: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

fn held(change: isize) {
    let _ = HELD.try_with(|h| {
        let now = h.get() + change;
        h.set(now);
        let _ = PEAK.try_with(|p| p.set(p.get().max(now)));
    });
}

// SAFETY: every call is the system allocator's; the counting touches
// constant-initialized thread-locals only, which never allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            held(layout.size() as isize);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            held(layout.size() as isize);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        held(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            held(new_size as isize - layout.size() as isize);
        }
        p
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// `f`'s result and the most this thread held at once while it ran,
/// beyond what it held before.
fn peak_of<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let before = HELD.with(Cell::get);
    PEAK.with(|p| p.set(before));
    let out = f();
    (out, (PEAK.with(Cell::get) - before).max(0) as usize)
}

/// What each entry says it holds.
const DECLARED: u64 = 16 << 10;
/// What the data really decompresses to.
const EXPANDS: usize = 1 << 20;
/// The most a refusal may hold at once: the declared size, the
/// decoder's own state (bzip2's block of 100 000 symbols, 400 KB, is the
/// largest) and its buffers; never the expansion.
const HELD_AT_MOST: usize = 768 << 10;

fn entry(method: u16, flags: u16, data: &[u8]) -> ZipEntry {
    ZipEntry {
        name: "bomb.ogg".into(),
        method: match method {
            9 => Method::Deflate64,
            12 => Method::Bzip2,
            14 => Method::Lzma,
            93 => Method::Zstd,
            other => panic!("method {other}"),
        },
        compressed_size: data.len() as u64,
        uncompressed_size: DECLARED,
        crc32: 0,
        local_header_offset: 0,
        is_dir: false,
        encrypted: false,
        flags,
        legacy_name: None,
    }
}

/// Deflate64 (method 9), written bit by bit: a zero, then sixteen
/// copies of it 65 538 bytes long (length code 285 is Deflate64's long
/// match, 3 + 16 extra bits), in one block of the fixed codes.
fn deflate64_bomb() -> Vec<u8> {
    struct Bits {
        out: Vec<u8>,
        acc: u64,
        n: u32,
    }
    impl Bits {
        /// `count` bits of `value`, least significant first (extra bits,
        /// the header).
        fn put(&mut self, value: u64, count: u32) {
            self.acc |= value << self.n;
            self.n += count;
            while self.n >= 8 {
                self.out.push(self.acc as u8);
                self.acc >>= 8;
                self.n -= 8;
            }
        }
        /// A Huffman code, most significant bit first.
        fn code(&mut self, code: u64, len: u32) {
            let reversed = (0..len).fold(0, |r, i| r | ((code >> i) & 1) << (len - 1 - i));
            self.put(reversed, len);
        }
    }
    let mut b = Bits {
        out: Vec::new(),
        acc: 0,
        n: 0,
    };
    b.put(1, 1); // the last block
    b.put(1, 2); // fixed codes
    b.code(0x30, 8); // the literal 0
    for _ in 0..16 {
        b.code(0xC5, 8); // length code 285
        b.put(65_535, 16); // 3 + 65 535
        b.code(0, 5); // distance 1
    }
    b.code(0, 7); // end of block
    b.put(0, 7);
    b.out
}

/// bzip2 (method 12): 1 MiB of zeros as `bzip2 -1` (1.0.8) writes it,
/// `head -c 1048576 /dev/zero | bzip2 -1`.
const BZIP2_BOMB: &[u8] = &[
    0x42, 0x5a, 0x68, 0x31, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x38, 0x57, 0x1c, 0xe5, 0x00, 0x08,
    0x08, 0x40, 0x00, 0xc0, 0x04, 0x00, 0x08, 0x20, 0x00, 0x30, 0xcc, 0x05, 0x29, 0xa6, 0x08, 0x06,
    0xc4, 0x20, 0x1e, 0x2e, 0xe4, 0x8a, 0x70, 0xa1, 0x20, 0x70, 0xae, 0x39, 0xca,
];

/// LZMA (method 14): 1 MiB of zeros, with its end marker (flag bit 1:
/// the size is the marker's, not the entry's) or without.
fn lzma_bomb(end_marker: bool) -> Vec<u8> {
    let options = lzma_rust2::LzmaOptions::with_preset(1);
    let mut packed = vec![16, 2, 5, 0, options.get_props()];
    packed.extend_from_slice(&options.dict_size.to_le_bytes());
    let mut w = lzma_rust2::LzmaWriter::new_no_header(packed, &options, end_marker).unwrap();
    std::io::Write::write_all(&mut w, &vec![0u8; EXPANDS]).unwrap();
    w.finish().unwrap()
}

/// Zstandard (method 93), written by hand: a frame whose window is
/// `window` (a window descriptor's byte) or, with `content`, a single
/// segment of that content size, then eight blocks of one byte
/// repeated 128 KiB times.
fn zstd_bomb(window: Option<u8>, content: Option<u32>) -> Vec<u8> {
    let mut out = vec![0x28, 0xB5, 0x2F, 0xFD];
    match (window, content) {
        (Some(w), None) => out.extend_from_slice(&[0x00, w]),
        (None, Some(size)) => {
            out.push(0xA0); // four bytes of content size, a single segment
            out.extend_from_slice(&size.to_le_bytes());
        }
        _ => panic!("a window or a content size"),
    }
    let blocks = EXPANDS / (128 << 10);
    for i in 0..blocks {
        let last = u32::from(i + 1 == blocks);
        let header = last | (1 << 1) | ((128 << 10) << 3);
        out.extend_from_slice(&header.to_le_bytes()[..3]);
        out.push(0x55);
    }
    out
}

/// A writer that counts what it is given.
struct Count(u64);

impl std::io::Write for Count {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn damaged(e: &ZipError) -> bool {
    matches!(
        e,
        ZipError::SizeMismatch { .. } | ZipError::CrcMismatch { .. } | ZipError::Decode { .. }
    )
}

/// Every way of reading `e` refuses it as damaged, writes at most its
/// size and holds at most [`HELD_AT_MOST`].
fn refused(what: &str, e: &ZipEntry, data: &[u8]) {
    let (whole, peak) = peak_of(|| zip::extract(e, data));
    let err = whole.expect_err(what);
    assert!(damaged(&err), "{what}: {err}");
    assert!(peak <= HELD_AT_MOST, "{what}: extract held {peak} bytes");

    let mut out = Count(0);
    let (pieces, peak) = peak_of(|| zip::extract_to(e, &mut &data[..], &mut out));
    let err = pieces.expect_err(what);
    assert!(damaged(&err), "{what}: {err}");
    assert!(out.0 <= DECLARED, "{what}: wrote {}", out.0);
    assert!(peak <= HELD_AT_MOST, "{what}: extract_to held {peak} bytes");

    let (head, peak) = peak_of(|| zip::extract_prefix(e, data, usize::MAX));
    assert!(head.is_empty(), "{what}: a start of {} bytes", head.len());
    assert!(
        peak <= HELD_AT_MOST,
        "{what}: extract_prefix held {peak} bytes"
    );
}

/// The same data, told its real size (`size` bytes of `byte`), reads:
/// the bombs are what they say.
fn reads_at_its_size(what: &str, e: &ZipEntry, data: &[u8], size: usize, byte: u8) {
    let mut honest = e.clone();
    honest.uncompressed_size = size as u64;
    honest.crc32 = zip::crc32(&vec![byte; size]);
    let mut out = Vec::new();
    zip::extract_to(&honest, &mut &data[..], &mut out).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert_eq!(out.len(), size, "{what}");
    assert!(out.iter().all(|&b| b == byte), "{what}");
}

#[test]
fn deflate64_stops_at_the_declared_size() {
    let data = deflate64_bomb();
    assert!(data.len() < 100);
    let e = entry(9, 0, &data);
    reads_at_its_size("deflate64", &e, &data, 1 + 16 * 65_538, 0);
    refused("deflate64", &e, &data);
}

#[test]
fn bzip2_stops_at_the_declared_size() {
    let e = entry(12, 0, BZIP2_BOMB);
    reads_at_its_size("bzip2", &e, BZIP2_BOMB, EXPANDS, 0);
    refused("bzip2", &e, BZIP2_BOMB);
}

#[test]
fn lzma_stops_at_the_declared_size() {
    for marker in [true, false] {
        let data = lzma_bomb(marker);
        assert!(data.len() < 1000);
        let e = entry(14, if marker { 1 << 1 } else { 0 }, &data);
        if marker {
            reads_at_its_size("lzma", &e, &data, EXPANDS, 0);
        }
        refused(&format!("lzma, marker {marker}"), &e, &data);
    }
}

/// A frame's window is the most it keeps back before it gives anything
/// out: a window larger than the entry is held to the entry's size, and
/// a frame that says it holds more than the entry is refused at once.
#[test]
fn zstd_stops_at_the_declared_size() {
    // Window descriptors: 128 MiB (2^27) and 1 MiB (2^20).
    for (what, window, content) in [
        ("zstd, a 128 MiB window", Some(17 << 3), None),
        ("zstd, a 1 MiB window", Some(10 << 3), None),
        ("zstd, a 1 MiB segment", None, Some(EXPANDS as u32)),
    ] {
        let data = zstd_bomb(window, content);
        assert!(data.len() < 100);
        let e = entry(93, 0, &data);
        reads_at_its_size(what, &e, &data, EXPANDS, 0x55);
        refused(what, &e, &data);
    }
}

/// An entry that says it is larger than [`zip::MAX_WHOLE`] is not read
/// whole: refused before any of it is decompressed (an honest deflate
/// bomb says its size, and a few megabytes say 4 GiB).
#[test]
fn whole_reads_have_a_most() {
    let data = miniz_oxide::deflate::compress_to_vec(&vec![0u8; EXPANDS], 6);
    let mut e = entry(93, 0, &data);
    e.method = Method::Deflated;
    e.uncompressed_size = zip::MAX_WHOLE + 1;
    let (whole, peak) = peak_of(|| zip::extract(&e, &data));
    assert!(
        matches!(whole, Err(ZipError::TooLarge { size, .. }) if size == zip::MAX_WHOLE + 1),
        "{whole:?}"
    );
    assert!(peak < 4096, "held {peak} bytes");
    // At the most, it is read (and found to be shorter than it says).
    e.uncompressed_size = zip::MAX_WHOLE;
    assert!(matches!(
        zip::extract(&e, &data),
        Err(ZipError::SizeMismatch { actual, .. }) if actual == EXPANDS as u64
    ));
}
