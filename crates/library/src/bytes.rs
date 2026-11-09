//! Fixed-size integers read from a byte slice at an offset, `None` when
//! the slice ends first, and the chunks of a RIFF file.

pub(crate) fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

pub(crate) fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

pub(crate) fn le24(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 3)?;
    Some(u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16)
}

pub(crate) fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

pub(crate) fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

pub(crate) fn le64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

pub(crate) fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// RIFF chunks of `b` from `at` to `end`, at most `max` of them: `(id,
/// data start, declared data end)`, the end past `b` for a chunk that
/// goes on beyond it.
pub(crate) fn riff_chunks(
    b: &[u8],
    mut at: usize,
    end: usize,
    max: usize,
) -> Vec<([u8; 4], usize, usize)> {
    let end = end.min(b.len());
    let mut out = Vec::new();
    while end.checked_sub(at).is_some_and(|left| left >= 8) && out.len() < max {
        let (Some(id), Some(size)) = (
            b.get(at..at + 4).and_then(|s| <[u8; 4]>::try_from(s).ok()),
            le32(b, at + 4),
        ) else {
            break;
        };
        let data = at + 8;
        let Some(data_end) = data.checked_add(size as usize) else {
            break;
        };
        out.push((id, data, data_end));
        // Chunks are padded to an even size.
        let Some(next) = data_end.checked_add(size as usize & 1) else {
            break;
        };
        at = next;
    }
    out
}
