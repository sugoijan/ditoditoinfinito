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
//!    (stored or deflated, CRC-32 checked).
//!
//! Supported: stored and deflated entries, ZIP64 sizes and offsets, UTF-8
//! names (flag bit 11 or the Info-ZIP Unicode Path extra field) and CP437
//! names otherwise. Not supported: encryption (reported, never read), other
//! compression methods, multi-disk archives. Names in a legacy code page other
//! than CP437 (Shift-JIS from Japanese Windows zippers) come out as CP437
//! mojibake; the bytes of the entry are unaffected.

use thiserror::Error;

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
    #[error("`{name}` uses compression method {method}; only stored and deflate are supported")]
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
    Other(u16),
}

impl Method {
    fn from_u16(m: u16) -> Method {
        match m {
            0 => Method::Stored,
            8 => Method::Deflated,
            other => Method::Other(other),
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
}

/// Number of tail bytes to read for [`locate_central_directory`].
pub fn tail_len(file_len: u64) -> u64 {
    TAIL_LEN.min(file_len)
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
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
            .filter(|&p| u32_at(tail, p) == Some(SIG_EOCD))
    };
    let comment_end = |p: usize| p + EOCD_LEN + u16_at(tail, p + 20).unwrap_or(0) as usize;
    let pos = candidates()
        .find(|&p| comment_end(p) == tail.len())
        .or_else(|| candidates().find(|&p| comment_end(p) <= tail.len()))
        .ok_or(ZipError::NotAZip)?;
    let eocd = &tail[pos..];
    let read16 = |at| u16_at(eocd, at).ok_or(ZipError::Corrupt("truncated end record"));
    let read32 = |at| u32_at(eocd, at).ok_or(ZipError::Corrupt("truncated end record"));
    let disk = read16(4)?;
    let cd_disk = read16(6)?;
    let entries_here = read16(8)?;
    let entries = read16(10)?;
    let size = read32(12)?;
    let offset = read32(16)?;

    // ZIP64: the locator sits right in front of the EOCD.
    if pos >= ZIP64_LOCATOR_LEN && u32_at(tail, pos - ZIP64_LOCATOR_LEN) == Some(SIG_ZIP64_LOCATOR)
    {
        let loc = &tail[pos - ZIP64_LOCATOR_LEN..pos];
        let corrupt = ZipError::Corrupt("truncated ZIP64 locator");
        let record_disk = u32_at(loc, 4).ok_or(corrupt.clone())?;
        let record_offset = u64_at(loc, 8).ok_or(corrupt.clone())?;
        let total_disks = u32_at(loc, 16).ok_or(corrupt)?;
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
    if u32_at(bytes, 0) != Some(SIG_ZIP64_EOCD) {
        return Err(ZipError::Corrupt("bad ZIP64 end record signature"));
    }
    let disk = u32_at(bytes, 16).ok_or_else(corrupt)?;
    let cd_disk = u32_at(bytes, 20).ok_or_else(corrupt)?;
    let entries_here = u64_at(bytes, 24).ok_or_else(corrupt)?;
    let entries = u64_at(bytes, 32).ok_or_else(corrupt)?;
    let size = u64_at(bytes, 40).ok_or_else(corrupt)?;
    let offset = u64_at(bytes, 48).ok_or_else(corrupt)?;
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
        if u32_at(bytes, p) != Some(SIG_CENTRAL) {
            // Some writers pad or put the (ZIP64) end records inside the
            // announced size; stop at the first non-header once all
            // announced entries are read.
            if out.len() as u64 >= cd.entries {
                break;
            }
            return Err(ZipError::Corrupt("bad central directory header signature"));
        }
        let h = bytes.get(p..p + CENTRAL_HEADER_LEN).ok_or_else(truncated)?;
        let r16 = |at| u16_at(h, at).ok_or_else(truncated);
        let r32 = |at| u32_at(h, at).ok_or_else(truncated);
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

        let mut name = if flags & FLAG_UTF8 != 0 {
            String::from_utf8_lossy(raw_name).into_owned()
        } else {
            decode_cp437(raw_name)
        };

        for (id, data) in extra_fields(extra) {
            match id {
                EXTRA_ZIP64 => {
                    // Only the saturated fields are present, in this order.
                    let mut q = 0;
                    let mut take = |v: &mut u64| -> Result<(), ZipError> {
                        *v = u64_at(data, q)
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
                        disk = u32_at(data, q)
                            .ok_or(ZipError::Corrupt("truncated ZIP64 extra field"))?;
                    }
                }
                // Info-ZIP Unicode Path: a UTF-8 copy of the name, valid only
                // while the CRC of the legacy name still matches (a tool that
                // renamed the entry without knowing the field leaves it stale).
                EXTRA_UNICODE_PATH if flags & FLAG_UTF8 == 0 => {
                    if data.first() == Some(&1)
                        && u32_at(data, 1) == Some(crc32(raw_name))
                        && let Some(utf8) = data.get(5..)
                        && let Ok(s) = core::str::from_utf8(utf8)
                    {
                        name = s.to_string();
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
        let id = u16_at(extra, 0)?;
        let len = u16_at(extra, 2)? as usize;
        let data = extra.get(4..4 + len)?;
        extra = &extra[4 + len..];
        Some((id, data))
    })
}

/// Given the [`LOCAL_HEADER_LEN`] bytes at `entry.local_header_offset`,
/// returns the absolute offset of the entry's data.
pub fn local_data_offset(header: &[u8], entry: &ZipEntry) -> Result<u64, ZipError> {
    if u32_at(header, 0) != Some(SIG_LOCAL) {
        return Err(ZipError::Corrupt("bad local file header signature"));
    }
    let truncated = || ZipError::Corrupt("truncated local file header");
    let name_len = u64::from(u16_at(header, 26).ok_or_else(truncated)?);
    let extra_len = u64::from(u16_at(header, 28).ok_or_else(truncated)?);
    entry
        .local_header_offset
        .checked_add(LOCAL_HEADER_LEN + name_len + extra_len)
        .ok_or(ZipError::Corrupt("data offset overflows"))
}

/// Decompresses (or copies) an entry's `compressed_size` data bytes and
/// checks the size and CRC-32 against the central directory.
pub fn extract(entry: &ZipEntry, data: &[u8]) -> Result<Vec<u8>, ZipError> {
    if entry.encrypted {
        return Err(ZipError::Encrypted {
            name: entry.name.clone(),
        });
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
        Method::Other(method) => {
            return Err(ZipError::UnsupportedMethod {
                name: entry.name.clone(),
                method,
            });
        }
    };
    if out.len() as u64 != entry.uncompressed_size {
        return Err(ZipError::SizeMismatch {
            name: entry.name.clone(),
            expected: entry.uncompressed_size,
            actual: out.len() as u64,
        });
    }
    let actual = crc32(&out);
    if actual != entry.crc32 {
        return Err(ZipError::CrcMismatch {
            name: entry.name.clone(),
            expected: entry.crc32,
            actual,
        });
    }
    Ok(out)
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
    !bytes.iter().fold(!0u32, |c, &b| {
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
fn decode_cp437(bytes: &[u8]) -> String {
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

    /// One file for the test writer.
    pub(crate) struct TestFile<'a> {
        pub name: &'a [u8],
        pub data: &'a [u8],
        pub deflate: bool,
        pub utf8: bool,
    }

    impl<'a> TestFile<'a> {
        pub fn stored(name: &'a str, data: &'a [u8]) -> Self {
            TestFile {
                name: name.as_bytes(),
                data,
                deflate: false,
                utf8: true,
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
            let payload = if f.deflate {
                miniz_oxide::deflate::compress_to_vec(f.data, 6)
            } else {
                f.data.to_vec()
            };
            let method: u16 = if f.deflate { 8 } else { 0 };
            let flags: u16 = if f.utf8 { FLAG_UTF8 } else { 0 };
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
}
