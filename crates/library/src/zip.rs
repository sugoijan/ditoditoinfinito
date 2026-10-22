//! Sans-IO ZIP reader.
//!
//! Song packs arrive as zips of a few hundred megabytes, and the browser shell
//! reads a `File` by slicing byte ranges (`Blob.slice`) rather than loading the
//! whole archive into wasm memory. So nothing here takes the archive: each
//! function parses one piece the caller has fetched and says which range to
//! fetch next.
//!
//! Reading an archive:
//!
//! 1. Read the last [`tail_len`]`(file_len)` bytes and call
//!    [`locate_central_directory`]. It returns either the central directory's
//!    location ([`Locate::Found`]) or, for a ZIP64 archive whose ZIP64 end
//!    record is not inside the tail, the range to read next
//!    ([`Locate::NeedZip64Record`]); pass those bytes to
//!    [`parse_zip64_end_record`].
//! 2. Read `CentralDirectory::offset .. offset + size` and call
//!    [`parse_central_directory`] for the entry list.
//! 3. For each wanted entry, read [`LOCAL_HEADER_LEN`] bytes at
//!    `ZipEntry::local_header_offset` and call [`local_data_offset`]; the
//!    local header's name and extra lengths can differ from the central
//!    directory's, so the data start is only known after this read.
//! 4. Read `compressed_size` bytes at that offset and call [`extract`]
//!    (size and CRC-32 checked), or hand them over as a reader to
//!    [`extract_to`] (an entry too large to hold), or read only the start
//!    of them for [`extract_prefix`] (a movie's header).
//!
//! Supported: stored, deflated, Deflate64, bzip2, LZMA and Zstandard
//! entries (every decoder pure Rust, for the wasm build; no entry
//! produces more than the central directory says), ZIP64 sizes and offsets, UTF-8
//! names (flag bit 11 or the Info-ZIP Unicode Path extra field) and CP437
//! names otherwise, with the raw bytes of such legacy names kept so the
//! caller can read them as UTF-8 or Shift-JIS instead
//! ([`crate::pack::decode_legacy_name`]). Not supported: encryption
//! (reported, never read), the rarer compression methods (XZ, PPMd and
//! older ones: reported with their number), multi-disk archives.

use std::io::{BufRead, Read};

use thiserror::Error;

use crate::bytes::{le16, le32, le64};

/// End Of Central Directory record, without its comment.
const EOCD_LEN: usize = 22;
/// The comment length is a `u16`.
const MAX_COMMENT_LEN: usize = 0xFFFF;
/// ZIP64 End Of Central Directory locator, which sits right before the EOCD.
const ZIP64_LOCATOR_LEN: usize = 20;

/// How many bytes from the end of the archive can hold the End Of Central
/// Directory record: the record (22) plus the longest comment (65535) plus
/// the ZIP64 locator in front of it (20). Read `min(TAIL_LEN, file_len)`
/// bytes; [`tail_len`] does that.
pub const TAIL_LEN: u64 = (EOCD_LEN + MAX_COMMENT_LEN + ZIP64_LOCATOR_LEN) as u64;

/// Fixed size of the ZIP64 End Of Central Directory record (without its
/// extensible data, which we never need).
pub const ZIP64_END_RECORD_LEN: u64 = 56;

/// Fixed part of a local file header; the name and extra field follow.
pub const LOCAL_HEADER_LEN: u64 = 30;

const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_ZIP64_EOCD: u32 = 0x0606_4b50;
const SIG_ZIP64_LOCATOR: u32 = 0x0706_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_LOCAL: u32 = 0x0403_4b50;

const CENTRAL_HEADER_LEN: usize = 46;

const FLAG_ENCRYPTED: u16 = 1 << 0;
/// For LZMA, an end-of-stream marker ends the data (the size need not be
/// known while it is written).
const FLAG_LZMA_EOS: u16 = 1 << 1;
const FLAG_UTF8: u16 = 1 << 11;

const EXTRA_ZIP64: u16 = 0x0001;
const EXTRA_UNICODE_PATH: u16 = 0x7075;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ZipError {
    #[error("not a zip archive (no end of central directory record)")]
    NotAZip,
    #[error("corrupt zip archive: {0}")]
    Corrupt(&'static str),
    #[error("multi-disk zip archives are not supported")]
    MultiDisk,
    #[error("`{name}` is encrypted; encrypted zip entries are not supported")]
    Encrypted { name: String },
    #[error(
        "`{name}` uses compression method {method}; only stored, deflate, Deflate64, bzip2, LZMA and Zstandard are supported"
    )]
    UnsupportedMethod { name: String, method: u16 },
    #[error("`{name}`: expected {expected} bytes, got {actual}")]
    SizeMismatch {
        name: String,
        expected: u64,
        actual: u64,
    },
    #[error("`{name}`: CRC-32 mismatch (expected {expected:08x}, got {actual:08x})")]
    CrcMismatch {
        name: String,
        expected: u32,
        actual: u32,
    },
    #[error("deflate stream is invalid: {0}")]
    Inflate(String),
    #[error("`{name}`: {method} data is invalid: {message}")]
    Decode {
        name: String,
        method: &'static str,
        message: String,
    },
}

/// Where the central directory lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CentralDirectory {
    /// Absolute offset of the first central directory header in the file.
    pub offset: u64,
    /// Size of the central directory in bytes.
    pub size: u64,
    /// Number of entries the end record announces.
    pub entries: u64,
    /// Bytes of foreign data in front of the archive (self-extractor stubs):
    /// recorded offsets are relative to the archive start, so this is added
    /// to every local header offset. Always 0 for ZIP64 archives.
    pub prefix: u64,
}

/// Result of [`locate_central_directory`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locate {
    Found(CentralDirectory),
    /// A ZIP64 archive whose ZIP64 end record lies outside the tail: read
    /// `len` bytes at `offset` and pass them to [`parse_zip64_end_record`].
    NeedZip64Record {
        offset: u64,
        len: u64,
    },
}

/// Compression method of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Stored,
    Deflated,
    /// Method 9, PKWARE's Enhanced Deflating: deflate with a 64 KB window
    /// and longer matches (7-Zip writes it on request).
    Deflate64,
    /// Method 12.
    Bzip2,
    /// Method 14: a raw LZMA stream behind a header of its own.
    Lzma,
    /// Zstandard frames: method 93, or 20, which writers used for it
    /// before the spec moved it to 93 (the frames' magic number tells
    /// them apart from anything else).
    Zstd,
    Other(u16),
}

impl Method {
    fn from_u16(m: u16) -> Method {
        match m {
            0 => Method::Stored,
            8 => Method::Deflated,
            9 => Method::Deflate64,
            12 => Method::Bzip2,
            14 => Method::Lzma,
            20 | 93 => Method::Zstd,
            other => Method::Other(other),
        }
    }

    /// The method's name, for errors.
    fn name(self) -> &'static str {
        match self {
            Method::Stored => "stored",
            Method::Deflated => "deflate",
            Method::Deflate64 => "Deflate64",
            Method::Bzip2 => "bzip2",
            Method::Lzma => "LZMA",
            Method::Zstd => "Zstandard",
            Method::Other(_) => "compressed",
        }
    }
}

/// One central directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipEntry {
    /// Path inside the archive, decoded (UTF-8 when flagged, else CP437), with
    /// backslashes turned into `/`: the spec mandates forward slashes, and
    /// the backslashes that do occur come from Windows tools writing native
    /// separators, never from file names.
    pub name: String,
    pub method: Method,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub crc32: u32,
    /// Absolute offset of the local file header (prefix already applied).
    pub local_header_offset: u64,
    /// Directory entry (name ends in `/`). Directories need not be listed in
    /// an archive at all, so callers should derive directories from paths.
    pub is_dir: bool,
    /// General purpose flag bit 0. Such entries cannot be read.
    pub encrypted: bool,
    /// The general purpose flags (the method's options among them).
    pub flags: u16,
    /// The raw name when it is in a legacy code page (neither flagged UTF-8
    /// nor given a Unicode Path), for a caller that can decode more code
    /// pages than CP437 ([`crate::pack::decode_legacy_name`]).
    pub legacy_name: Option<Vec<u8>>,
}

/// Number of tail bytes to read for [`locate_central_directory`].
pub fn tail_len(file_len: u64) -> u64 {
    TAIL_LEN.min(file_len)
}

/// Finds the End Of Central Directory record in the last bytes of the
/// archive (`tail`, the final `tail.len()` bytes of a `file_len`-byte file;
/// see [`tail_len`]).
///
/// The record is found by scanning backwards for its signature. A comment can
/// itself contain the signature, so a candidate whose comment length reaches
/// exactly the end of the file wins; failing that (archives with trailing
/// junk) the last candidate whose comment fits is taken.
pub fn locate_central_directory(tail: &[u8], file_len: u64) -> Result<Locate, ZipError> {
    if (tail.len() as u64) > file_len {
        return Err(ZipError::Corrupt("tail longer than the file"));
    }
    let tail_start = file_len - tail.len() as u64;
    let candidates = || {
        (0..=tail.len().saturating_sub(EOCD_LEN))
            .rev()
            .filter(|&p| le32(tail, p) == Some(SIG_EOCD))
    };
    let comment_end = |p: usize| p + EOCD_LEN + le16(tail, p + 20).unwrap_or(0) as usize;
    let pos = candidates()
        .find(|&p| comment_end(p) == tail.len())
        .or_else(|| candidates().find(|&p| comment_end(p) <= tail.len()))
        .ok_or(ZipError::NotAZip)?;
    let eocd = &tail[pos..];
    let read16 = |at| le16(eocd, at).ok_or(ZipError::Corrupt("truncated end record"));
    let read32 = |at| le32(eocd, at).ok_or(ZipError::Corrupt("truncated end record"));
    let disk = read16(4)?;
    let cd_disk = read16(6)?;
    let entries_here = read16(8)?;
    let entries = read16(10)?;
    let size = read32(12)?;
    let offset = read32(16)?;

    // ZIP64: the locator sits right in front of the EOCD.
    if pos >= ZIP64_LOCATOR_LEN && le32(tail, pos - ZIP64_LOCATOR_LEN) == Some(SIG_ZIP64_LOCATOR) {
        let loc = &tail[pos - ZIP64_LOCATOR_LEN..pos];
        let corrupt = ZipError::Corrupt("truncated ZIP64 locator");
        let record_disk = le32(loc, 4).ok_or(corrupt.clone())?;
        let record_offset = le64(loc, 8).ok_or(corrupt.clone())?;
        let total_disks = le32(loc, 16).ok_or(corrupt)?;
        if record_disk != 0 || total_disks > 1 {
            return Err(ZipError::MultiDisk);
        }
        // Usually the ZIP64 end record directly precedes the locator and is
        // already in the tail.
        if record_offset >= tail_start
            && let Ok(start) = usize::try_from(record_offset - tail_start)
            && let Some(record) = tail.get(start..)
            && record.len() >= ZIP64_END_RECORD_LEN as usize
        {
            return parse_zip64_end_record(record).map(Locate::Found);
        }
        if record_offset
            .checked_add(ZIP64_END_RECORD_LEN)
            .is_none_or(|end| end > file_len)
        {
            return Err(ZipError::Corrupt(
                "ZIP64 end record past the end of the file",
            ));
        }
        return Ok(Locate::NeedZip64Record {
            offset: record_offset,
            len: ZIP64_END_RECORD_LEN,
        });
    }

    if disk != 0 || cd_disk != 0 || entries_here != entries {
        return Err(ZipError::MultiDisk);
    }
    let eocd_abs = tail_start + pos as u64;
    let (offset, size) = (u64::from(offset), u64::from(size));
    let cd_end = offset
        .checked_add(size)
        .ok_or(ZipError::Corrupt("central directory overflows"))?;
    if cd_end > eocd_abs {
        return Err(ZipError::Corrupt(
            "central directory overlaps its end record",
        ));
    }
    // The central directory ends where the EOCD starts; any gap is data
    // prepended to the archive, which shifts every recorded offset.
    let prefix = eocd_abs - cd_end;
    Ok(Locate::Found(CentralDirectory {
        offset: offset + prefix,
        size,
        entries: u64::from(entries),
        prefix,
    }))
}

/// Parses the ZIP64 End Of Central Directory record (at least
/// [`ZIP64_END_RECORD_LEN`] bytes starting at its signature).
pub fn parse_zip64_end_record(bytes: &[u8]) -> Result<CentralDirectory, ZipError> {
    let corrupt = || ZipError::Corrupt("truncated ZIP64 end record");
    if le32(bytes, 0) != Some(SIG_ZIP64_EOCD) {
        return Err(ZipError::Corrupt("bad ZIP64 end record signature"));
    }
    let disk = le32(bytes, 16).ok_or_else(corrupt)?;
    let cd_disk = le32(bytes, 20).ok_or_else(corrupt)?;
    let entries_here = le64(bytes, 24).ok_or_else(corrupt)?;
    let entries = le64(bytes, 32).ok_or_else(corrupt)?;
    let size = le64(bytes, 40).ok_or_else(corrupt)?;
    let offset = le64(bytes, 48).ok_or_else(corrupt)?;
    if disk != 0 || cd_disk != 0 || entries_here != entries {
        return Err(ZipError::MultiDisk);
    }
    offset
        .checked_add(size)
        .ok_or(ZipError::Corrupt("central directory overflows"))?;
    Ok(CentralDirectory {
        offset,
        size,
        entries,
        prefix: 0,
    })
}

/// Parses the central directory (the `cd.size` bytes at `cd.offset`).
///
/// Fails on any malformed header rather than returning a partial list: a
/// truncated directory means a truncated download, and importing half a pack
/// silently would be worse than an error.
pub fn parse_central_directory(
    bytes: &[u8],
    cd: &CentralDirectory,
) -> Result<Vec<ZipEntry>, ZipError> {
    let truncated = || ZipError::Corrupt("truncated central directory");
    // Each header is at least 46 bytes, so this bounds the allocation even
    // when the entry count is garbage.
    let cap = (cd.entries as usize).min(bytes.len() / CENTRAL_HEADER_LEN);
    let mut out = Vec::with_capacity(cap);
    let mut p = 0usize;
    while p < bytes.len() {
        if le32(bytes, p) != Some(SIG_CENTRAL) {
            // Some writers pad or put the (ZIP64) end records inside the
            // announced size; stop at the first non-header once all
            // announced entries are read.
            if out.len() as u64 >= cd.entries {
                break;
            }
            return Err(ZipError::Corrupt("bad central directory header signature"));
        }
        let h = bytes.get(p..p + CENTRAL_HEADER_LEN).ok_or_else(truncated)?;
        let r16 = |at| le16(h, at).ok_or_else(truncated);
        let r32 = |at| le32(h, at).ok_or_else(truncated);
        let flags = r16(8)?;
        let method = r16(10)?;
        let crc = r32(16)?;
        let mut compressed_size = u64::from(r32(20)?);
        let mut uncompressed_size = u64::from(r32(24)?);
        let name_len = r16(28)? as usize;
        let extra_len = r16(30)? as usize;
        let comment_len = r16(32)? as usize;
        let mut disk = u32::from(r16(34)?);
        let external_attrs = r32(38)?;
        let mut local_header_offset = u64::from(r32(42)?);
        let made_by_host = r16(4)? >> 8;

        let name_start = p + CENTRAL_HEADER_LEN;
        let extra_start = name_start + name_len;
        let next = extra_start + extra_len + comment_len;
        let raw_name = bytes.get(name_start..extra_start).ok_or_else(truncated)?;
        let extra = bytes
            .get(extra_start..extra_start + extra_len)
            .ok_or_else(truncated)?;
        if next > bytes.len() {
            return Err(truncated());
        }

        let mut legacy = flags & FLAG_UTF8 == 0;
        let mut name = if legacy {
            decode_cp437(raw_name)
        } else {
            String::from_utf8_lossy(raw_name).into_owned()
        };

        for (id, data) in extra_fields(extra) {
            match id {
                EXTRA_ZIP64 => {
                    // Only the saturated fields are present, in this order.
                    let mut q = 0;
                    let mut take = |v: &mut u64| -> Result<(), ZipError> {
                        *v = le64(data, q)
                            .ok_or(ZipError::Corrupt("truncated ZIP64 extra field"))?;
                        q += 8;
                        Ok(())
                    };
                    if uncompressed_size == 0xFFFF_FFFF {
                        take(&mut uncompressed_size)?;
                    }
                    if compressed_size == 0xFFFF_FFFF {
                        take(&mut compressed_size)?;
                    }
                    if local_header_offset == 0xFFFF_FFFF {
                        take(&mut local_header_offset)?;
                    }
                    if disk == 0xFFFF {
                        disk = le32(data, q)
                            .ok_or(ZipError::Corrupt("truncated ZIP64 extra field"))?;
                    }
                }
                // Info-ZIP Unicode Path: a UTF-8 copy of the name, valid only
                // while the CRC of the legacy name still matches (a tool that
                // renamed the entry without knowing the field leaves it stale).
                EXTRA_UNICODE_PATH if flags & FLAG_UTF8 == 0 => {
                    if data.first() == Some(&1)
                        && le32(data, 1) == Some(crc32(raw_name))
                        && let Some(utf8) = data.get(5..)
                        && let Ok(s) = core::str::from_utf8(utf8)
                    {
                        name = s.to_string();
                        legacy = false;
                    }
                }
                _ => {}
            }
        }
        if disk != 0 {
            return Err(ZipError::MultiDisk);
        }

        let name = name.replace('\\', "/");
        // MS-DOS directory attribute, for writers that omit the trailing
        // slash (host 0 = FAT, 10 = NTFS, 14 = VFAT).
        let dos_dir = matches!(made_by_host, 0 | 10 | 14) && external_attrs & 0x10 != 0;
        let is_dir = name.ends_with('/') || dos_dir;
        out.push(ZipEntry {
            name,
            method: Method::from_u16(method),
            compressed_size,
            uncompressed_size,
            crc32: crc,
            local_header_offset: local_header_offset
                .checked_add(cd.prefix)
                .ok_or(ZipError::Corrupt("local header offset overflows"))?,
            is_dir,
            encrypted: flags & FLAG_ENCRYPTED != 0,
            flags,
            legacy_name: legacy.then(|| raw_name.to_vec()),
        });
        p = next;
    }
    if (out.len() as u64) < cd.entries {
        return Err(ZipError::Corrupt(
            "fewer central directory entries than announced",
        ));
    }
    Ok(out)
}

/// Iterates `(header id, data)` pairs of an extra field block, stopping at
/// the first malformed one (some writers pad the block with zeros).
fn extra_fields(mut extra: &[u8]) -> impl Iterator<Item = (u16, &[u8])> {
    core::iter::from_fn(move || {
        let id = le16(extra, 0)?;
        let len = le16(extra, 2)? as usize;
        let data = extra.get(4..4 + len)?;
        extra = &extra[4 + len..];
        Some((id, data))
    })
}

/// Given the [`LOCAL_HEADER_LEN`] bytes at `entry.local_header_offset`,
/// returns the absolute offset of the entry's data.
pub fn local_data_offset(header: &[u8], entry: &ZipEntry) -> Result<u64, ZipError> {
    if le32(header, 0) != Some(SIG_LOCAL) {
        return Err(ZipError::Corrupt("bad local file header signature"));
    }
    let truncated = || ZipError::Corrupt("truncated local file header");
    let name_len = u64::from(le16(header, 26).ok_or_else(truncated)?);
    let extra_len = u64::from(le16(header, 28).ok_or_else(truncated)?);
    entry
        .local_header_offset
        .checked_add(LOCAL_HEADER_LEN + name_len + extra_len)
        .ok_or(ZipError::Corrupt("data offset overflows"))
}

/// Decompresses (or copies) an entry's `compressed_size` data bytes and
/// checks the size and CRC-32 against the central directory.
pub fn extract(entry: &ZipEntry, data: &[u8]) -> Result<Vec<u8>, ZipError> {
    if entry.encrypted {
        return Err(encrypted(entry));
    }
    if data.len() as u64 != entry.compressed_size {
        return Err(ZipError::SizeMismatch {
            name: entry.name.clone(),
            expected: entry.compressed_size,
            actual: data.len() as u64,
        });
    }
    let out = match entry.method {
        Method::Stored => data.to_vec(),
        Method::Deflated => inflate(data, entry.uncompressed_size)?,
        Method::Other(method) => return Err(unsupported(entry, method)),
        _ => {
            // Grown as it comes, not reserved from the size a header
            // claims.
            let reserve = usize::try_from(entry.uncompressed_size).map_or(0, |n| n.min(16 << 20));
            let mut out = Vec::with_capacity(reserve);
            decode_with(entry, data, u64::MAX, |bytes| {
                out.extend_from_slice(bytes);
                Ok(())
            })?;
            out
        }
    };
    check_output(entry, out.len() as u64, || crc32(&out))?;
    Ok(out)
}

/// The error for an encrypted entry.
fn encrypted(entry: &ZipEntry) -> ZipError {
    ZipError::Encrypted {
        name: entry.name.clone(),
    }
}

/// The error for an entry compressed with `method`, which is not read.
fn unsupported(entry: &ZipEntry, method: u16) -> ZipError {
    ZipError::UnsupportedMethod {
        name: entry.name.clone(),
        method,
    }
}

/// Checks an entry's output, `written` bytes, against the central
/// directory: its size first, then its CRC-32 (`crc`, only computed when
/// the size matches).
fn check_output(entry: &ZipEntry, written: u64, crc: impl FnOnce() -> u32) -> Result<(), ZipError> {
    if written != entry.uncompressed_size {
        return Err(ZipError::SizeMismatch {
            name: entry.name.clone(),
            expected: entry.uncompressed_size,
            actual: written,
        });
    }
    let actual = crc();
    if actual != entry.crc32 {
        return Err(ZipError::CrcMismatch {
            name: entry.name.clone(),
            expected: entry.crc32,
            actual,
        });
    }
    Ok(())
}

/// [`extract`] in pieces: an entry's `compressed_size` data bytes read
/// from `input` (at the entry's data), decompressed (or copied) into `out`
/// as they come, its size and CRC-32 checked at the end. For an entry
/// larger than is worth holding (a movie). `out` may hold part of it when
/// it fails.
pub fn extract_to(
    entry: &ZipEntry,
    input: &mut impl std::io::Read,
    out: &mut impl std::io::Write,
) -> Result<(), ZipError> {
    use miniz_oxide::inflate::stream::{InflateState, inflate};
    use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};
    if entry.encrypted {
        return Err(encrypted(entry));
    }
    let io = |e: std::io::Error| ZipError::Inflate(e.to_string());
    let mut left = entry.compressed_size;
    let mut crc = !0u32;
    let mut written = 0u64;
    let mut emit = |bytes: &[u8], out: &mut dyn std::io::Write| -> Result<(), ZipError> {
        written += bytes.len() as u64;
        if written > entry.uncompressed_size {
            return Err(ZipError::SizeMismatch {
                name: entry.name.clone(),
                expected: entry.uncompressed_size,
                actual: written,
            });
        }
        crc = crc32_update(crc, bytes);
        out.write_all(bytes)
            .map_err(|e| ZipError::Inflate(e.to_string()))
    };
    let inbuf = || vec![0u8; 256 << 10];
    match entry.method {
        Method::Stored => {
            let mut inbuf = inbuf();
            while left > 0 {
                let n = (left.min(inbuf.len() as u64)) as usize;
                input.read_exact(&mut inbuf[..n]).map_err(io)?;
                emit(&inbuf[..n], out)?;
                left -= n as u64;
            }
        }
        Method::Deflated => {
            let mut inbuf = inbuf();
            let mut state = InflateState::new_boxed(DataFormat::Raw);
            let mut outbuf = vec![0u8; 512 << 10];
            let (mut have, mut at) = (0usize, 0usize);
            loop {
                if at == have && left > 0 {
                    let n = (left.min(inbuf.len() as u64)) as usize;
                    input.read_exact(&mut inbuf[..n]).map_err(io)?;
                    left -= n as u64;
                    (have, at) = (n, 0);
                }
                // Never `Finish`: with all the input given, miniz would
                // want the rest of the output to fit at once.
                let r = inflate(&mut state, &inbuf[at..have], &mut outbuf, MZFlush::None);
                at += r.bytes_consumed;
                emit(&outbuf[..r.bytes_written], out)?;
                let stuck = r.bytes_consumed == 0 && r.bytes_written == 0;
                match r.status {
                    Ok(MZStatus::StreamEnd) => break,
                    Ok(_) if !(stuck && left == 0 && at == have) => {}
                    Err(MZError::Buf) if !(left == 0 && at == have) => {}
                    // All the input given and nothing more comes out: the
                    // stream is cut short.
                    Ok(_) | Err(MZError::Buf) => {
                        return Err(ZipError::Inflate("the stream ends early".into()));
                    }
                    Err(e) => return Err(ZipError::Inflate(format!("{e:?}"))),
                }
            }
        }
        Method::Other(method) => return Err(unsupported(entry, method)),
        _ => {
            // Only the entry's data; the decoders read ahead.
            let cap = entry.compressed_size.clamp(1, 64 << 10) as usize;
            let src = std::io::BufReader::with_capacity(cap, input.take(entry.compressed_size));
            decode_with(entry, src, u64::MAX, |bytes| emit(bytes, out))?;
        }
    }
    check_output(entry, written, || !crc)
}

/// The first `max_out` bytes an entry's data decompresses to, from
/// `compressed`, the start of that data (or all of it): for sniffing a
/// large entry without decompressing it whole. Empty when it is
/// encrypted, of an unknown method or corrupt, and when `compressed` ran
/// out before `max_out` bytes came out (what came out of a cut-off
/// stream is not trusted); shorter only when the whole entry is.
pub fn extract_prefix(entry: &ZipEntry, compressed: &[u8], max_out: usize) -> Vec<u8> {
    if entry.encrypted {
        return Vec::new();
    }
    match entry.method {
        Method::Stored => compressed[..compressed.len().min(max_out)].to_vec(),
        Method::Deflated => inflate_prefix(compressed, max_out),
        Method::Other(_) => Vec::new(),
        _ => {
            let whole = compressed.len() as u64 >= entry.compressed_size;
            let mut out = Vec::new();
            let decoded = decode_with(entry, compressed, max_out as u64, |bytes| {
                out.extend_from_slice(bytes);
                Ok(())
            });
            if decoded.is_err() || (out.len() < max_out && !whole) {
                out.clear();
            }
            out
        }
    }
}

/// Decodes an entry of one of the methods read through a decoder (not
/// stored, deflated or unknown) from `src`, its data, handing what comes
/// out to `sink` in pieces until the stream ends or `limit` bytes came
/// out. More output than the central directory's size fails as soon as
/// it comes; the caller checks the size and CRC-32 at the end. Returns
/// the bytes given to `sink`.
fn decode_with<'a>(
    entry: &ZipEntry,
    src: impl BufRead + 'a,
    limit: u64,
    mut sink: impl FnMut(&[u8]) -> Result<(), ZipError>,
) -> Result<u64, ZipError> {
    let invalid = |e: std::io::Error| ZipError::Decode {
        name: entry.name.clone(),
        method: entry.method.name(),
        message: e.to_string(),
    };
    let mut decoder: Box<dyn Read + 'a> = match entry.method {
        Method::Deflate64 => Box::new(deflate64::Deflate64Decoder::with_buffer(src)),
        Method::Bzip2 => Box::new(bzip2_rs::DecoderReader::new(src)),
        Method::Lzma => Box::new(lzma_reader(entry, src).map_err(invalid)?),
        Method::Zstd => Box::new(Zstd::new(src)),
        Method::Other(method) => return Err(unsupported(entry, method)),
        Method::Stored | Method::Deflated => {
            return Err(ZipError::Corrupt("not a decoder's method"));
        }
    };
    let len = entry
        .uncompressed_size
        .saturating_add(1)
        .min(limit)
        .clamp(1, 256 << 10);
    let mut buf = vec![0u8; len as usize];
    let mut written = 0u64;
    while written < limit {
        let n = match decoder.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(invalid(e)),
        };
        if written + n as u64 > entry.uncompressed_size {
            return Err(ZipError::SizeMismatch {
                name: entry.name.clone(),
                expected: entry.uncompressed_size,
                actual: written + n as u64,
            });
        }
        let n = (n as u64).min(limit - written) as usize;
        sink(&buf[..n])?;
        written += n as u64;
    }
    Ok(written)
}

/// An LZMA entry's decoder. The data starts with LZMA SDK's version (2
/// bytes) and the properties' size (2 bytes, 5), then the properties (the
/// `lc`/`lp`/`pb` byte and the dictionary's size); the stream ends at the
/// entry's size, or at its end marker where flag bit 1 says there is one.
/// The dictionary grows with the output, whatever size it declares.
fn lzma_reader<R: Read>(
    entry: &ZipEntry,
    mut src: R,
) -> std::io::Result<lzma_rust2::LzmaReader<R>> {
    let mut head = [0u8; 9];
    src.read_exact(&mut head)?;
    if u16::from_le_bytes([head[2], head[3]]) != 5 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "LZMA properties of an unknown size",
        ));
    }
    let dict_size = u32::from_le_bytes([head[5], head[6], head[7], head[8]]);
    let size = if entry.flags & FLAG_LZMA_EOS != 0 {
        u64::MAX
    } else {
        entry.uncompressed_size
    };
    lzma_rust2::LzmaReader::new_with_props(src, size, head[4], dict_size, None)
}

/// Zstandard frames one after the other (skippable frames skipped) as one
/// reader: `ruzstd`'s streaming decoder reads a single frame. A frame's
/// window is reserved as it starts, limited to `ruzstd`'s default
/// (128 MB), past which the frame fails.
struct Zstd<R> {
    src: R,
    frame: ruzstd::decoding::FrameDecoder,
    /// Inside a frame (its header read).
    open: bool,
}

impl<R: BufRead> Zstd<R> {
    fn new(src: R) -> Self {
        Zstd {
            src,
            frame: ruzstd::decoding::FrameDecoder::new(),
            open: false,
        }
    }
}

impl<R: BufRead> Read for Zstd<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use ruzstd::decoding::BlockDecodingStrategy;
        use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.open {
                while self.frame.can_collect() == 0 && !self.frame.is_finished() {
                    self.frame
                        .decode_blocks(&mut self.src, BlockDecodingStrategy::UptoBlocks(1))
                        .map_err(std::io::Error::other)?;
                }
                let n = self.frame.read(buf)?;
                if n > 0 {
                    return Ok(n);
                }
                self.open = false;
            }
            if self.src.fill_buf()?.is_empty() {
                return Ok(0);
            }
            match self.frame.reset(&mut self.src) {
                Ok(()) => self.open = true,
                Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame {
                    length,
                    ..
                })) => {
                    let skipped = std::io::copy(
                        &mut (&mut self.src).take(u64::from(length)),
                        &mut std::io::sink(),
                    )?;
                    if skipped != u64::from(length) {
                        return Err(std::io::ErrorKind::UnexpectedEof.into());
                    }
                }
                Err(e) => return Err(std::io::Error::other(e)),
            }
        }
    }
}

/// Inflates a raw deflate stream (no zlib header), refusing to produce more
/// than `expected_len` bytes so a lying header cannot exhaust memory. The
/// output may be shorter; [`extract`] checks the exact length.
pub fn inflate(compressed: &[u8], expected_len: u64) -> Result<Vec<u8>, ZipError> {
    let limit = usize::try_from(expected_len)
        .map_err(|_| ZipError::Corrupt("entry too large for this platform"))?;
    miniz_oxide::inflate::decompress_to_vec_with_limit(compressed, limit)
        .map_err(|e| ZipError::Inflate(e.to_string()))
}

/// The first `max_out` bytes a raw deflate stream inflates to, from the
/// start of the stream: for sniffing a large entry without inflating it
/// whole. `compressed` may be cut off, but must hold enough of the stream
/// to produce `max_out` bytes: when the input runs out first, what was
/// inflated is not reliable (miniz_oxide's partial output can go wrong
/// before its end), so the result is empty. Shorter when the whole stream
/// is shorter; empty when it is corrupt.
pub fn inflate_prefix(compressed: &[u8], max_out: usize) -> Vec<u8> {
    use miniz_oxide::inflate::TINFLStatus;
    let mut out = match miniz_oxide::inflate::decompress_to_vec_with_limit(compressed, max_out) {
        Ok(out) => out,
        Err(e) if e.status == TINFLStatus::HasMoreOutput => e.output,
        Err(_) => Vec::new(),
    };
    out.truncate(max_out);
    out
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

/// CRC-32 (IEEE 802.3, reflected, as used by zip and PNG).
pub fn crc32(bytes: &[u8]) -> u32 {
    !crc32_update(!0, bytes)
}

/// The CRC-32 register after `bytes` (start from `!0`, finish with `!`).
fn crc32_update(crc: u32, bytes: &[u8]) -> u32 {
    bytes.iter().fold(crc, |c, &b| {
        CRC_TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8)
    })
}

/// IBM PC code page 437, bytes 0x80..=0xFF: the zip spec's default name
/// encoding when the UTF-8 flag is clear.
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', //
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', //
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', //
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐', //
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', //
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀', //
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', //
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Decodes a CP437 name. Bytes below 0x80 are taken as ASCII (the control
/// glyphs of the original code page never appear in real file names).
pub(crate) fn decode_cp437(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                b as char
            } else {
                CP437_HIGH[(b - 0x80) as usize]
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn inflate_prefix_reads_the_start_of_a_cut_off_stream() {
        let data: Vec<u8> = (0..300_000u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        let packed = miniz_oxide::deflate::compress_to_vec(&data, 6);
        let head = inflate_prefix(&packed, 65536);
        let differs = head.iter().zip(&data).position(|(a, b)| a != b);
        assert_eq!((head.len(), differs), (65536, None));
        // Cut off before 64 KB came out: nothing rather than a wrong start.
        let small = 1000;
        assert!(inflate_prefix(&packed[..small], 65536).is_empty());
        // The whole stream, shorter than the limit.
        assert_eq!(inflate_prefix(&packed, usize::MAX), data);
        assert!(inflate_prefix(&[0xff; 64], 1024).is_empty());
    }

    /// One file for the test writer.
    pub(crate) struct TestFile<'a> {
        pub name: &'a [u8],
        pub data: &'a [u8],
        pub deflate: bool,
        pub utf8: bool,
        /// Data compressed elsewhere, with its method and the general
        /// purpose flags it wants, in place of stored or deflated `data`.
        pub packed: Option<(u16, u16, &'a [u8])>,
    }

    impl<'a> TestFile<'a> {
        pub fn stored(name: &'a str, data: &'a [u8]) -> Self {
            TestFile {
                name: name.as_bytes(),
                data,
                deflate: false,
                utf8: true,
                packed: None,
            }
        }
        pub fn deflated(name: &'a str, data: &'a [u8]) -> Self {
            TestFile {
                deflate: true,
                ..TestFile::stored(name, data)
            }
        }
    }

    #[derive(Default)]
    pub(crate) struct WriteOpts<'a> {
        pub comment: &'a [u8],
        /// Write ZIP64 extra fields, a ZIP64 end record and locator, and
        /// saturate the classic fields.
        pub zip64: bool,
        pub prefix: &'a [u8],
        /// Different extra length in local headers than in the central
        /// directory (allowed by the spec, done by some writers).
        pub local_extra: &'a [u8],
    }

    /// Minimal zip writer for the tests.
    pub(crate) fn write_zip(files: &[TestFile<'_>], opts: &WriteOpts<'_>) -> Vec<u8> {
        let mut out = opts.prefix.to_vec();
        let base = out.len();
        let mut central = Vec::new();
        for f in files {
            let offset = (out.len() - base) as u64;
            let payload = if let Some((_, _, packed)) = f.packed {
                packed.to_vec()
            } else if f.deflate {
                miniz_oxide::deflate::compress_to_vec(f.data, 6)
            } else {
                f.data.to_vec()
            };
            let method: u16 = match f.packed {
                Some((method, _, _)) => method,
                None if f.deflate => 8,
                None => 0,
            };
            let flags: u16 =
                if f.utf8 { FLAG_UTF8 } else { 0 } | f.packed.map_or(0, |(_, flags, _)| flags);
            let crc = crc32(f.data);
            // Local header.
            out.extend_from_slice(&SIG_LOCAL.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&flags.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 4]); // time, date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&(f.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(f.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&(opts.local_extra.len() as u16).to_le_bytes());
            out.extend_from_slice(f.name);
            out.extend_from_slice(opts.local_extra);
            out.extend_from_slice(&payload);
            // Central header.
            let mut extra = Vec::new();
            let sat = |v: u64| if opts.zip64 { 0xFFFF_FFFF } else { v as u32 };
            if opts.zip64 {
                extra.extend_from_slice(&EXTRA_ZIP64.to_le_bytes());
                extra.extend_from_slice(&24u16.to_le_bytes());
                extra.extend_from_slice(&(f.data.len() as u64).to_le_bytes());
                extra.extend_from_slice(&(payload.len() as u64).to_le_bytes());
                extra.extend_from_slice(&offset.to_le_bytes());
            }
            central.extend_from_slice(&SIG_CENTRAL.to_le_bytes());
            central.extend_from_slice(&((3u16 << 8) | 45).to_le_bytes()); // unix
            central.extend_from_slice(&45u16.to_le_bytes());
            central.extend_from_slice(&flags.to_le_bytes());
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&sat(payload.len() as u64).to_le_bytes());
            central.extend_from_slice(&sat(f.data.len() as u64).to_le_bytes());
            central.extend_from_slice(&(f.name.len() as u16).to_le_bytes());
            central.extend_from_slice(&(extra.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // comment
            central.extend_from_slice(&0u16.to_le_bytes()); // disk
            central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            central.extend_from_slice(&sat(offset).to_le_bytes());
            central.extend_from_slice(f.name);
            central.extend_from_slice(&extra);
        }
        let cd_offset = (out.len() - base) as u64;
        out.extend_from_slice(&central);
        let n = files.len() as u64;
        if opts.zip64 {
            let record_offset = (out.len() - base) as u64;
            out.extend_from_slice(&SIG_ZIP64_EOCD.to_le_bytes());
            out.extend_from_slice(&44u64.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&n.to_le_bytes());
            out.extend_from_slice(&n.to_le_bytes());
            out.extend_from_slice(&(central.len() as u64).to_le_bytes());
            out.extend_from_slice(&cd_offset.to_le_bytes());
            out.extend_from_slice(&SIG_ZIP64_LOCATOR.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&record_offset.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
        }
        out.extend_from_slice(&SIG_EOCD.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        let n16 = if opts.zip64 { 0xFFFF } else { n as u16 };
        out.extend_from_slice(&n16.to_le_bytes());
        out.extend_from_slice(&n16.to_le_bytes());
        let sat32 = |v: u64| if opts.zip64 { 0xFFFF_FFFF } else { v as u32 };
        out.extend_from_slice(&sat32(central.len() as u64).to_le_bytes());
        out.extend_from_slice(&sat32(cd_offset).to_le_bytes());
        out.extend_from_slice(&(opts.comment.len() as u16).to_le_bytes());
        out.extend_from_slice(opts.comment);
        out
    }

    /// Reads a whole archive the way the web shell does, by ranges only.
    pub(crate) fn read_all(zip: &[u8]) -> Result<Vec<(ZipEntry, Vec<u8>)>, ZipError> {
        let len = zip.len() as u64;
        let range = |off: u64, n: u64| -> Result<&[u8], ZipError> {
            let outside = ZipError::Corrupt("range outside the test file");
            let end = off.checked_add(n).ok_or(outside.clone())?;
            zip.get(usize::try_from(off).map_err(|_| outside.clone())?..)
                .and_then(|z| z.get(..usize::try_from(end - off).ok()?))
                .ok_or(outside)
        };
        let tail = range(len - tail_len(len), tail_len(len))?;
        let cd = match locate_central_directory(tail, len)? {
            Locate::Found(cd) => cd,
            Locate::NeedZip64Record { offset, len } => parse_zip64_end_record(range(offset, len)?)?,
        };
        let entries = parse_central_directory(range(cd.offset, cd.size)?, &cd)?;
        entries
            .into_iter()
            .filter(|e| !e.is_dir)
            .map(|e| {
                let start = local_data_offset(range(e.local_header_offset, LOCAL_HEADER_LEN)?, &e)?;
                let data = extract(&e, range(start, e.compressed_size)?)?;
                Ok((e, data))
            })
            .collect()
    }

    const SIMFILE: &[u8] = b"#TITLE:Test;\n#BPMS:0=120;\n#NOTES:\n     dance-single:\n     :\n     Easy:\n     1:\n     0,0,0,0,0:\n1000\n0100\n0010\n0001\n;\n";

    /// An entry inflated into a writer in pieces is what [`extract`]
    /// gives; a wrong CRC or a short stream fails.
    #[test]
    fn extract_to_streams_large_entries() {
        // 3 MB that compress unevenly, over several input and output
        // buffers.
        let big: Vec<u8> = (0..3_000_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> (i % 23)) as u8)
            .collect();
        // And 20 MB of near silence: it inflates to far more than the
        // output buffer from the last of its input.
        let flat: Vec<u8> = (0..20_000_000u32)
            .map(|i| u8::from(i % 65_536 == 0))
            .collect();
        let zip = write_zip(
            &[
                TestFile::deflated("a/movie.avi", &big),
                TestFile::stored("a/clip.mp4", &big[..700_000]),
                TestFile::deflated("a/flat.avi", &flat),
            ],
            &WriteOpts::default(),
        );
        let entries = read_all(&zip).unwrap();
        for (e, whole) in &entries {
            let at = local_data_offset(
                &zip[e.local_header_offset as usize..][..LOCAL_HEADER_LEN as usize],
                e,
            )
            .unwrap() as usize;
            let mut out = Vec::new();
            extract_to(e, &mut &zip[at..], &mut out).unwrap();
            assert_eq!(&out, whole, "{}", e.name);
            let mut bad = e.clone();
            bad.crc32 ^= 1;
            assert!(matches!(
                extract_to(&bad, &mut &zip[at..], &mut Vec::new()),
                Err(ZipError::CrcMismatch { .. })
            ));
            let mut short = e.clone();
            short.uncompressed_size -= 1;
            assert!(extract_to(&short, &mut &zip[at..], &mut Vec::new()).is_err());
            // A stream cut short ends with an error, not a hang.
            let mut cut = e.clone();
            cut.compressed_size /= 2;
            assert!(extract_to(&cut, &mut &zip[at..], &mut Vec::new()).is_err());
        }
    }

    #[test]
    fn crc32_known_values() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
    }

    #[test]
    fn tail_len_never_exceeds_file() {
        assert_eq!(tail_len(10), 10);
        assert_eq!(TAIL_LEN, 65577);
        assert_eq!(tail_len(1 << 30), TAIL_LEN);
    }

    #[test]
    fn stored_and_deflated() {
        let big = SIMFILE.repeat(50);
        let zip = write_zip(
            &[
                TestFile::stored("Pack/Song/song.sm", SIMFILE),
                TestFile::deflated("Pack/Song/big.ssc", &big),
                TestFile::stored("Pack/Song/", b""),
            ],
            &WriteOpts::default(),
        );
        let files = read_all(&zip).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0.name, "Pack/Song/song.sm");
        assert_eq!(files[0].0.method, Method::Stored);
        assert_eq!(files[0].1, SIMFILE);
        assert_eq!(files[1].0.method, Method::Deflated);
        assert!(files[1].0.compressed_size < big.len() as u64);
        assert_eq!(files[1].1, big);
    }

    #[test]
    fn directory_entries_are_flagged() {
        let zip = write_zip(&[TestFile::stored("Pack/", b"")], &WriteOpts::default());
        let len = zip.len() as u64;
        let Locate::Found(cd) = locate_central_directory(&zip, len).unwrap() else {
            panic!("expected classic end record");
        };
        let entries = parse_central_directory(
            &zip[cd.offset as usize..(cd.offset + cd.size) as usize],
            &cd,
        )
        .unwrap();
        assert!(entries[0].is_dir);
    }

    #[test]
    fn comment_at_the_end() {
        // A maximal comment that itself contains an end-record signature.
        let mut comment = vec![b'x'; MAX_COMMENT_LEN];
        comment[100..104].copy_from_slice(&SIG_EOCD.to_le_bytes());
        let zip = write_zip(
            &[TestFile::deflated("a.sm", SIMFILE)],
            &WriteOpts {
                comment: &comment,
                ..Default::default()
            },
        );
        let files = read_all(&zip).unwrap();
        assert_eq!(files[0].1, SIMFILE);
    }

    #[test]
    fn zip64() {
        let zip = write_zip(
            &[
                TestFile::stored("one.sm", SIMFILE),
                TestFile::deflated("two.ssc", SIMFILE),
            ],
            &WriteOpts {
                zip64: true,
                ..Default::default()
            },
        );
        let files = read_all(&zip).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[1].0.uncompressed_size, SIMFILE.len() as u64);
        assert_eq!(files[1].1, SIMFILE);
    }

    #[test]
    fn zip64_record_outside_the_tail_asks_for_a_second_read() {
        let zip = write_zip(
            &[TestFile::stored("one.sm", SIMFILE)],
            &WriteOpts {
                zip64: true,
                ..Default::default()
            },
        );
        let len = zip.len() as u64;
        // Hand over only the locator and the end record.
        let tail = &zip[zip.len() - EOCD_LEN - ZIP64_LOCATOR_LEN..];
        let Locate::NeedZip64Record { offset, len: n } =
            locate_central_directory(tail, len).unwrap()
        else {
            panic!("expected a request for the ZIP64 record");
        };
        let cd = parse_zip64_end_record(&zip[offset as usize..(offset + n) as usize]).unwrap();
        assert_eq!(cd.entries, 1);
        // One stored entry: 30-byte local header, 6-byte name, payload.
        assert_eq!(cd.offset, (30 + 6 + SIMFILE.len()) as u64);
    }

    #[test]
    fn prepended_data_shifts_offsets() {
        let zip = write_zip(
            &[TestFile::deflated("a.sm", SIMFILE)],
            &WriteOpts {
                prefix: b"MZ self-extractor stub",
                ..Default::default()
            },
        );
        let files = read_all(&zip).unwrap();
        assert_eq!(files[0].1, SIMFILE);
    }

    #[test]
    fn local_extra_differs_from_central() {
        let zip = write_zip(
            &[TestFile::deflated("a.sm", SIMFILE)],
            &WriteOpts {
                local_extra: &[0x55, 0x54, 5, 0, 1, 0, 0, 0, 0],
                ..Default::default()
            },
        );
        assert_eq!(read_all(&zip).unwrap()[0].1, SIMFILE);
    }

    #[test]
    fn cp437_names_and_backslashes() {
        let zip = write_zip(
            &[TestFile {
                name: b"Pack\\Caf\x82\\song.sm",
                data: SIMFILE,
                deflate: false,
                utf8: false,
                packed: None,
            }],
            &WriteOpts::default(),
        );
        assert_eq!(read_all(&zip).unwrap()[0].0.name, "Pack/Café/song.sm");
    }

    #[test]
    fn utf8_names() {
        let zip = write_zip(
            &[TestFile::stored("曲/譜面.ssc", SIMFILE)],
            &WriteOpts::default(),
        );
        assert_eq!(read_all(&zip).unwrap()[0].0.name, "曲/譜面.ssc");
    }

    #[test]
    fn not_a_zip() {
        assert_eq!(
            locate_central_directory(b"definitely not a zip file at all", 32),
            Err(ZipError::NotAZip)
        );
        assert_eq!(locate_central_directory(b"", 0), Err(ZipError::NotAZip));
    }

    #[test]
    fn corrupt_archives_fail_cleanly() {
        let zip = write_zip(
            &[TestFile::deflated("a.sm", SIMFILE)],
            &WriteOpts::default(),
        );
        // Flipped payload byte: CRC or inflate error, never a panic.
        let mut bad = zip.clone();
        bad[40] ^= 0xFF;
        assert!(read_all(&bad).is_err());
        // Truncated archive: the end record is gone.
        assert!(read_all(&zip[..zip.len() - 10]).is_err());
        // Central directory offset pointing past the end record.
        let mut bad = zip.clone();
        let n = bad.len();
        bad[n - 6..n - 2].copy_from_slice(&0xFFFF_0000u32.to_le_bytes());
        assert!(read_all(&bad).is_err());
        // Every single-byte truncation and corruption must return, not panic.
        for cut in 0..zip.len() {
            let _ = read_all(&zip[..cut]);
            let mut b = zip.clone();
            b[cut] = b[cut].wrapping_add(1);
            let _ = read_all(&b);
        }
    }

    #[test]
    fn crc_mismatch_is_reported() {
        let zip = write_zip(&[TestFile::stored("a.sm", SIMFILE)], &WriteOpts::default());
        let mut bad = zip.clone();
        bad[30 + 4] ^= 0x01; // first data byte after the 30-byte header + "a.sm"
        assert!(matches!(read_all(&bad), Err(ZipError::CrcMismatch { .. })));
    }

    #[test]
    fn encrypted_entries_are_not_read() {
        let mut zip = write_zip(&[TestFile::stored("a.sm", SIMFILE)], &WriteOpts::default());
        // Set flag bit 0 in the central header.
        let cd = zip.len() - EOCD_LEN - (CENTRAL_HEADER_LEN + 4);
        zip[cd + 8] |= 1;
        assert!(matches!(read_all(&zip), Err(ZipError::Encrypted { .. })));
    }

    #[test]
    fn inflate_respects_the_limit() {
        let data = vec![7u8; 10_000];
        let packed = miniz_oxide::deflate::compress_to_vec(&data, 6);
        assert_eq!(inflate(&packed, 10_000).unwrap(), data);
        assert!(inflate(&packed, 100).is_err());
    }

    /// Zips of our own texts (below) made by real writers with the other
    /// methods (`tests/zips/make.py`), and the method each entry is in.
    const FIXTURES: &[(&str, &[u8], Method)] = &[
        (
            "7z-deflate64.zip",
            include_bytes!("../tests/zips/7z-deflate64.zip"),
            Method::Deflate64,
        ),
        (
            "7z-bzip2.zip",
            include_bytes!("../tests/zips/7z-bzip2.zip"),
            Method::Bzip2,
        ),
        (
            "python-bzip2.zip",
            include_bytes!("../tests/zips/python-bzip2.zip"),
            Method::Bzip2,
        ),
        (
            "7z-lzma.zip",
            include_bytes!("../tests/zips/7z-lzma.zip"),
            Method::Lzma,
        ),
        (
            "python-lzma.zip",
            include_bytes!("../tests/zips/python-lzma.zip"),
            Method::Lzma,
        ),
        (
            "zstd.zip",
            include_bytes!("../tests/zips/zstd.zip"),
            Method::Zstd,
        ),
    ];

    /// The fixtures' `small.txt` (`make.py` writes it the same way).
    fn fixture_small() -> Vec<u8> {
        (0..200u32)
            .map(|i| format!("#NOTE {i}: {:05}\n", i * 7919 % 10007))
            .collect::<String>()
            .into_bytes()
    }

    /// The fixtures' `big.txt` (and `big-frames.txt`): several bzip2
    /// blocks at 100 KB.
    fn fixture_big() -> Vec<u8> {
        let arrows = ["LEFT", "DOWN", "UP", "RIGHT"];
        (0..30000u32)
            .map(|i| {
                let arrow = arrows[(i * i % 7 % 4) as usize];
                format!("{:03} {arrow} {:02}\n", i / 4096, i % 97)
            })
            .collect::<String>()
            .into_bytes()
    }

    fn fixture_text(name: &str) -> Vec<u8> {
        if name == "small.txt" {
            fixture_small()
        } else {
            fixture_big()
        }
    }

    /// An archive's entries, read whole from memory.
    fn entries(zip: &[u8]) -> Vec<ZipEntry> {
        let len = zip.len() as u64;
        let Locate::Found(cd) =
            locate_central_directory(&zip[(len - tail_len(len)) as usize..], len).unwrap()
        else {
            panic!("expected a classic end record");
        };
        parse_central_directory(&zip[cd.offset as usize..][..cd.size as usize], &cd).unwrap()
    }

    /// An entry's compressed data.
    fn data_of<'z>(zip: &'z [u8], e: &ZipEntry) -> &'z [u8] {
        let header = &zip[e.local_header_offset as usize..][..LOCAL_HEADER_LEN as usize];
        let at = local_data_offset(header, e).unwrap() as usize;
        &zip[at..][..e.compressed_size as usize]
    }

    /// A writer that counts what it is given, to see how far a bomb gets.
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

    #[test]
    fn other_methods_from_real_writers() {
        for &(file, zip, method) in FIXTURES {
            let files = read_all(zip).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(files.len(), if file == "zstd.zip" { 3 } else { 2 });
            for (e, data) in &files {
                let want = fixture_text(&e.name);
                assert!(*data == want, "{file}: {}", e.name);
                assert_eq!(e.method, method, "{file}: {}", e.name);
                let packed = data_of(zip, e);
                let mut out = Vec::new();
                extract_to(e, &mut &packed[..], &mut out).unwrap();
                assert!(out == want, "{file}: {}", e.name);
                // The start alone, from the whole data or from a part.
                assert_eq!(extract_prefix(e, packed, 1000), want[..1000]);
                assert_eq!(extract_prefix(e, packed, usize::MAX), want);
                for cut in [0, 1, 9, 20, packed.len() / 3, packed.len() - 1] {
                    let head = extract_prefix(e, &packed[..cut], 2000);
                    assert!(
                        head.is_empty() || head == want[..2000],
                        "{file}: {} cut at {cut}",
                        e.name
                    );
                }
            }
        }
    }

    /// Cut off or damaged, an entry of each method fails or (where the
    /// damage changed nothing the CRC sees) reads right; never a panic or
    /// a hang.
    #[test]
    fn other_methods_fail_cleanly() {
        for &(file, zip, _) in FIXTURES {
            for e in entries(zip) {
                let want = fixture_text(&e.name);
                let packed = data_of(zip, &e).to_vec();
                // About 100 places in the small texts' data, 12 in the
                // big's (an ignored test below tries many more).
                let step = packed.len() / if want.len() < 10_000 { 100 } else { 12 };
                let step = step.max(1);
                let check = |e: &ZipEntry, data: &[u8], what: &str| {
                    if let Ok(out) = extract(e, data) {
                        assert!(out == want, "{file}: {} {what}", e.name);
                    }
                    let mut out = Vec::new();
                    if extract_to(e, &mut &data[..], &mut out).is_ok() {
                        assert!(out == want, "{file}: {} {what}", e.name);
                    }
                    let _ = extract_prefix(e, data, 4096);
                };
                for cut in (0..packed.len()).step_by(step) {
                    let mut short = e.clone();
                    short.compressed_size = cut as u64;
                    check(&short, &packed[..cut], &format!("cut at {cut}"));
                    // Cut off, but the size not told: a cut-off read.
                    assert!(extract_prefix(&e, &packed[..cut], usize::MAX).is_empty());
                }
                for at in (0..packed.len()).step_by(step) {
                    for flip in [0x01, 0xFF] {
                        let mut bad = packed.clone();
                        bad[at] ^= flip;
                        check(&e, &bad, &format!("byte {at} ^ {flip:#x}"));
                    }
                }
            }
        }
    }

    /// [`other_methods_fail_cleanly`] at random, a few bytes damaged at a
    /// time and the data cut anywhere: `DDI_ZIP_FUZZ` rounds (10 000), in
    /// release under the watchdog (`cargo test --release -p ddi-library
    /// --lib zip_fuzz -- --ignored`).
    #[test]
    #[ignore]
    fn zip_fuzz_other_methods() {
        let rounds: u64 = std::env::var("DDI_ZIP_FUZZ")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(10_000);
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let all: Vec<(ZipEntry, Vec<u8>, Vec<u8>)> = FIXTURES
            .iter()
            .flat_map(|&(_, zip, _)| {
                entries(zip).into_iter().map(move |e| {
                    let packed = data_of(zip, &e).to_vec();
                    let want = fixture_text(&e.name);
                    (e, packed, want)
                })
            })
            .collect();
        for round in 0..rounds {
            let (e, packed, want) = &all[(next() % all.len() as u64) as usize];
            let mut bad = packed.clone();
            for _ in 0..1 + next() % 4 {
                let at = (next() % bad.len() as u64) as usize;
                bad[at] = next() as u8;
            }
            if next() % 4 == 0 {
                bad.truncate((next() % bad.len() as u64) as usize);
            }
            let mut e = e.clone();
            e.compressed_size = bad.len() as u64;
            if let Ok(out) = extract(&e, &bad) {
                assert!(out == *want, "round {round}: {}", e.name);
            }
            let _ = extract_prefix(&e, &bad, 4096);
        }
    }

    /// Sizes, CRC-32 and flags hold for every method as for deflate.
    #[test]
    fn other_methods_keep_to_the_central_directory() {
        for &(file, zip, _) in FIXTURES {
            for e in entries(zip) {
                let packed = data_of(zip, &e);
                let mut small = e.clone();
                small.uncompressed_size = 100;
                assert!(
                    matches!(extract(&small, packed), Err(ZipError::SizeMismatch { .. })),
                    "{file}: {}",
                    e.name
                );
                // Stopped as soon as it passes the size.
                let mut count = Count(0);
                assert!(matches!(
                    extract_to(&small, &mut &packed[..], &mut count),
                    Err(ZipError::SizeMismatch { .. })
                ));
                assert!(count.0 <= 100, "{file}: {} wrote {}", e.name, count.0);
                for size in [e.uncompressed_size + 1, u64::MAX / 4] {
                    let mut large = e.clone();
                    large.uncompressed_size = size;
                    assert!(matches!(
                        extract(&large, packed),
                        Err(ZipError::SizeMismatch { .. })
                    ));
                }
                let mut bad = e.clone();
                bad.crc32 ^= 1;
                assert!(matches!(
                    extract(&bad, packed),
                    Err(ZipError::CrcMismatch { .. })
                ));
                let mut locked = e.clone();
                locked.encrypted = true;
                assert!(matches!(
                    extract(&locked, packed),
                    Err(ZipError::Encrypted { .. })
                ));
                assert!(extract_prefix(&locked, packed, 100).is_empty());
                // Data past the stream's end is not the entry's.
                let mut longer = packed.to_vec();
                longer.extend_from_slice(b"\x28\xb5\x2f\xfd junk after the stream");
                let mut more = e.clone();
                more.compressed_size = longer.len() as u64;
                if e.method == Method::Zstd {
                    assert!(extract(&more, &longer).is_err());
                }
            }
        }
    }

    #[test]
    fn rarer_methods_are_reported() {
        for (method, packed) in [(95u16, &b"\xfd7zXZ\0"[..]), (98, b"\x06\0\0\0"), (6, b"")] {
            let zip = write_zip(
                &[TestFile {
                    packed: Some((method, 0, packed)),
                    ..TestFile::stored("a.sm", SIMFILE)
                }],
                &WriteOpts::default(),
            );
            let e = &entries(&zip)[0];
            assert_eq!(e.method, Method::Other(method));
            let err = read_all(&zip).unwrap_err();
            assert_eq!(
                err,
                ZipError::UnsupportedMethod {
                    name: "a.sm".into(),
                    method
                }
            );
            assert!(err.to_string().contains(&format!("method {method}")));
            assert!(extract_prefix(e, data_of(&zip, e), 10).is_empty());
            assert!(extract_to(e, &mut &data_of(&zip, e)[..], &mut Vec::new()).is_err());
        }
    }

    /// A raw LZMA stream of `data` behind the zip's LZMA header.
    fn lzma_packed(data: &[u8], end_marker: bool) -> Vec<u8> {
        let options = lzma_rust2::LzmaOptions::with_preset(1);
        let mut packed = vec![16, 2, 5, 0, options.get_props()];
        packed.extend_from_slice(&options.dict_size.to_le_bytes());
        let mut w = lzma_rust2::LzmaWriter::new_no_header(packed, &options, end_marker).unwrap();
        std::io::Write::write_all(&mut w, data).unwrap();
        w.finish().unwrap()
    }

    /// LZMA ends at the entry's size without an end marker (flag bit 1
    /// clear), at its marker with one.
    #[test]
    fn lzma_with_and_without_its_end_marker() {
        let big = fixture_big();
        let (bare, marked) = (lzma_packed(&big, false), lzma_packed(&big, true));
        let zip = write_zip(
            &[
                TestFile {
                    packed: Some((14, 0, &bare)),
                    ..TestFile::stored("bare.txt", &big)
                },
                TestFile {
                    packed: Some((14, FLAG_LZMA_EOS, &marked)),
                    ..TestFile::stored("marked.txt", &big)
                },
            ],
            &WriteOpts::default(),
        );
        for (e, data) in read_all(&zip).unwrap() {
            assert!(data == big, "{}", e.name);
        }
        // An end marker the flags announce but the stream lacks fails.
        let zip = write_zip(
            &[TestFile {
                packed: Some((14, FLAG_LZMA_EOS, &bare)),
                ..TestFile::stored("a.txt", &big)
            }],
            &WriteOpts::default(),
        );
        assert!(read_all(&zip).is_err());
        // Properties of another size.
        let mut odd = bare.clone();
        odd[2] = 4;
        let zip = write_zip(
            &[TestFile {
                packed: Some((14, 0, &odd)),
                ..TestFile::stored("a.txt", &big)
            }],
            &WriteOpts::default(),
        );
        assert!(matches!(read_all(&zip), Err(ZipError::Decode { .. })));
    }

    /// 8 MB that pack into a few kilobytes, in an entry that says 64 KB:
    /// no method gives more than the size it was told.
    #[test]
    fn bombs_stop_at_the_size() {
        let zeros = vec![0u8; 8 << 20];
        let zstd = ruzstd::encoding::compress_to_vec(
            &zeros[..],
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        let lzma = lzma_packed(&zeros, true);
        for (method, flags, packed) in [(93, 0, &zstd), (14, FLAG_LZMA_EOS, &lzma)] {
            assert!(packed.len() < 64 << 10);
            let e = ZipEntry {
                name: "bomb".into(),
                method: Method::from_u16(method),
                compressed_size: packed.len() as u64,
                uncompressed_size: 64 << 10,
                crc32: 0,
                local_header_offset: 0,
                is_dir: false,
                encrypted: false,
                flags,
                legacy_name: None,
            };
            let mut count = Count(0);
            assert!(matches!(
                extract_to(&e, &mut &packed[..], &mut count),
                Err(ZipError::SizeMismatch { .. })
            ));
            assert!(count.0 <= 64 << 10);
            assert!(matches!(
                extract(&e, packed),
                Err(ZipError::SizeMismatch { .. })
            ));
        }
    }
}
