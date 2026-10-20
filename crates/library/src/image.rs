//! Image dimensions from the first bytes of a file, without decoding it.
//!
//! [`crate::pack::classify_images`] needs the pixel size of every unnamed
//! image in a song folder; decoding them all would be slow and memory hungry
//! in the browser, while the size sits in the first few bytes of every
//! format a simfile pack uses. Callers read [`IMAGE_HEADER_LEN`] leading
//! bytes (or the whole file if shorter) and pass them to [`image_size`].

/// How many leading bytes to read for [`image_size`]. PNG, GIF, BMP and WebP
/// need under 32; JPEG puts its frame header (SOFn) after the APPn segments,
/// and an EXIF segment with an embedded thumbnail alone can be 64 KiB, so
/// this leaves room for one maximal segment plus ICC profile and JFIF ones.
pub const IMAGE_HEADER_LEN: usize = 128 * 1024;

/// `(width, height)` in pixels of a PNG, GIF, BMP, WebP or JPEG, read from
/// its leading bytes. `None` for other formats, truncated or malformed
/// headers, zero sizes, or a JPEG whose frame header lies beyond `header`.
pub fn image_size(header: &[u8]) -> Option<(u32, u32)> {
    let size = if header.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(header)
    } else if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        Some((u32::from(le16(header, 6)?), u32::from(le16(header, 8)?)))
    } else if header.starts_with(b"BM") {
        bmp(header)
    } else if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP") {
        webp(header)
    } else if header.starts_with(b"\xFF\xD8") {
        jpeg(header)
    } else {
        None
    }?;
    (size.0 > 0 && size.1 > 0).then_some(size)
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn le24(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 3)?;
    Some(u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16)
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// IHDR must be the first chunk: signature (8), length (4), type (4), then
/// width and height as big-endian `u32`.
fn png(b: &[u8]) -> Option<(u32, u32)> {
    if b.get(12..16) != Some(b"IHDR") {
        return None;
    }
    Some((be32(b, 16)?, be32(b, 20)?))
}

/// File header (14), then the DIB header, whose size tells the version: the
/// old OS/2 `BITMAPCOREHEADER` (12) has 16-bit sizes, every later one 32-bit
/// signed sizes, with a negative height for top-down bitmaps.
fn bmp(b: &[u8]) -> Option<(u32, u32)> {
    match le32(b, 14)? {
        12 => Some((u32::from(le16(b, 18)?), u32::from(le16(b, 20)?))),
        n if n >= 40 => {
            let w = le32(b, 18)? as i32;
            let h = le32(b, 22)? as i32;
            Some((w.unsigned_abs(), h.unsigned_abs()))
        }
        _ => None,
    }
}

/// RIFF container; the first chunk (at 12) decides the layout.
fn webp(b: &[u8]) -> Option<(u32, u32)> {
    match b.get(12..16)? {
        // Lossy: 3-byte frame tag, start code 9D 01 2A, then 14-bit sizes
        // (the top two bits are a scale hint).
        b"VP8 " => {
            if b.get(23..26) != Some(&[0x9D, 0x01, 0x2A]) {
                return None;
            }
            Some((
                u32::from(le16(b, 26)? & 0x3FFF),
                u32::from(le16(b, 28)? & 0x3FFF),
            ))
        }
        // Lossless: signature 0x2F, then width − 1 and height − 1 as 14-bit
        // fields.
        b"VP8L" => {
            if b.get(20) != Some(&0x2F) {
                return None;
            }
            let bits = le32(b, 21)?;
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        // Extended: flags (4), then canvas width − 1 and height − 1 as
        // 24-bit fields.
        b"VP8X" => Some((le24(b, 24)? + 1, le24(b, 27)? + 1)),
        _ => None,
    }
}

/// Walks the marker segments to the first SOFn (start of frame), skipping
/// APPn, DQT, DHT, comments and the like by their length fields.
fn jpeg(b: &[u8]) -> Option<(u32, u32)> {
    let mut p = 2;
    loop {
        if *b.get(p)? != 0xFF {
            return None;
        }
        // Any number of 0xFF fill bytes may precede a marker.
        while *b.get(p + 1)? == 0xFF {
            p += 1;
        }
        let marker = *b.get(p + 1)?;
        match marker {
            // SOF0–SOF15 except DHT (C4), JPG (C8) and DAC (CC):
            // length (2), precision (1), height (2), width (2).
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                return Some((u32::from(be16(b, p + 7)?), u32::from(be16(b, p + 5)?)));
            }
            // Standalone markers without a length: TEM, RST0–7, SOI.
            0x01 | 0xD0..=0xD8 => p += 2,
            // End of image or start of scan before any frame header.
            0xD9 | 0xDA => return None,
            _ => p += 2 + usize::from(be16(b, p + 2)?),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b
    }

    fn bmp_header(dib: u32, w: i32, h: i32) -> Vec<u8> {
        let mut b = b"BM".to_vec();
        b.extend_from_slice(&[0; 12]);
        b.extend_from_slice(&dib.to_le_bytes());
        if dib == 12 {
            b.extend_from_slice(&(w as u16).to_le_bytes());
            b.extend_from_slice(&(h as u16).to_le_bytes());
        } else {
            b.extend_from_slice(&w.to_le_bytes());
            b.extend_from_slice(&h.to_le_bytes());
        }
        b.extend_from_slice(&[1, 0, 24, 0]);
        b
    }

    fn riff(chunk: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WEBP".to_vec();
        b.extend_from_slice(chunk);
        b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        b.extend_from_slice(payload);
        b
    }

    fn segment(marker: u8, body: &[u8]) -> Vec<u8> {
        let mut s = vec![0xFF, marker];
        s.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
        s.extend_from_slice(body);
        s
    }

    fn jpeg_header(w: u16, h: u16, exif_len: usize) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8];
        b.extend(segment(0xE0, b"JFIF\0\x01\x02\0\0\x01\0\x01\0\0"));
        // EXIF with a thumbnail that itself contains SOI/SOF bytes, which
        // must be skipped by length, not scanned.
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xC0, 0, 17, 8, 0, 1, 0, 1]);
        exif.resize(exif_len, 0);
        b.extend(segment(0xE1, &exif));
        b.extend(segment(0xDB, &[0; 65]));
        b.push(0xFF); // fill byte
        let mut sof = vec![8];
        sof.extend_from_slice(&h.to_be_bytes());
        sof.extend_from_slice(&w.to_be_bytes());
        sof.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        b.extend(segment(0xC2, &sof)); // progressive
        b.extend(segment(0xDA, &[0; 10]));
        b
    }

    #[test]
    fn png() {
        assert_eq!(image_size(&png_header(256, 80)), Some((256, 80)));
    }

    #[test]
    fn gif() {
        let mut b = b"GIF89a".to_vec();
        b.extend_from_slice(&640u16.to_le_bytes());
        b.extend_from_slice(&480u16.to_le_bytes());
        b.extend_from_slice(&[0xF7, 0, 0]);
        assert_eq!(image_size(&b), Some((640, 480)));
        b[..6].copy_from_slice(b"GIF87a");
        assert_eq!(image_size(&b), Some((640, 480)));
    }

    #[test]
    fn bmp() {
        assert_eq!(image_size(&bmp_header(40, 300, 200)), Some((300, 200)));
        assert_eq!(image_size(&bmp_header(124, 320, -240)), Some((320, 240)));
        assert_eq!(image_size(&bmp_header(12, 100, 50)), Some((100, 50)));
        assert_eq!(image_size(&bmp_header(7, 100, 50)), None);
    }

    #[test]
    fn webp() {
        let mut vp8 = vec![0x10, 0x02, 0x00, 0x9D, 0x01, 0x2A];
        vp8.extend_from_slice(&(512u16 | 0x4000).to_le_bytes()); // scale bits set
        vp8.extend_from_slice(&160u16.to_le_bytes());
        assert_eq!(image_size(&riff(b"VP8 ", &vp8)), Some((512, 160)));

        let bits: u32 = (256 - 1) | ((80 - 1) << 14) | (1 << 28);
        let mut vp8l = vec![0x2F];
        vp8l.extend_from_slice(&bits.to_le_bytes());
        assert_eq!(image_size(&riff(b"VP8L", &vp8l)), Some((256, 80)));

        let mut vp8x = vec![0x10, 0, 0, 0];
        vp8x.extend_from_slice(&(1920u32 - 1).to_le_bytes()[..3]);
        vp8x.extend_from_slice(&(1080u32 - 1).to_le_bytes()[..3]);
        assert_eq!(image_size(&riff(b"VP8X", &vp8x)), Some((1920, 1080)));

        assert_eq!(image_size(&riff(b"ALPH", &[0; 10])), None);
    }

    #[test]
    fn jpeg() {
        let small = jpeg_header(418, 164, 100);
        assert_eq!(image_size(&small), Some((418, 164)));
        // A maximal EXIF segment still fits the advised header length.
        let big = jpeg_header(1280, 720, 65_000);
        assert!(big.len() < IMAGE_HEADER_LEN);
        assert_eq!(image_size(&big), Some((1280, 720)));
        // Frame header beyond the bytes supplied.
        assert_eq!(image_size(&big[..40_000]), None);
        // Start of scan without a frame header.
        let mut no_sof = vec![0xFF, 0xD8];
        no_sof.extend(segment(0xDA, &[0; 4]));
        assert_eq!(image_size(&no_sof), None);
    }

    #[test]
    fn truncated_and_garbage() {
        let samples = [
            png_header(256, 80),
            bmp_header(40, 300, 200),
            riff(b"VP8X", &[0; 10]),
            jpeg_header(418, 164, 100),
        ];
        for s in &samples {
            for cut in 0..s.len() {
                // Must not panic; short prefixes never claim a size they
                // could not read (JPEG may still succeed once the SOF fits).
                let _ = image_size(&s[..cut]);
            }
        }
        assert_eq!(image_size(&png_header(256, 80)[..20]), None);
        assert_eq!(image_size(b""), None);
        assert_eq!(image_size(b"not an image at all"), None);
        assert_eq!(image_size(&[0xFF, 0xD8, 0x00, 0x01]), None);
        assert_eq!(image_size(&png_header(0, 80)), None);
        // A PNG whose first chunk is not IHDR.
        let mut b = png_header(1, 1);
        b[12..16].copy_from_slice(b"tEXt");
        assert_eq!(image_size(&b), None);
    }
}
