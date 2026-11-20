use super::*;

/// A RIFF/WEBP file of `chunks` (FourCC and data).
fn webp_of(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut body = b"WEBP".to_vec();
    for (kind, data) in chunks {
        body.extend(*kind);
        body.extend((data.len() as u32).to_le_bytes());
        body.extend(data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

fn u24(v: u32) -> [u8; 3] {
    let b = v.to_le_bytes();
    [b[0], b[1], b[2]]
}

/// An animated WebP's chunks: a canvas of `size` and `frames` frames of
/// `ms` each (their image data a stub: the probe reads no pixel).
fn animated_webp(size: [u32; 2], frames: usize, ms: u32) -> Vec<u8> {
    let mut vp8x = vec![0x02, 0, 0, 0];
    vp8x.extend(u24(size[0] - 1));
    vp8x.extend(u24(size[1] - 1));
    let mut chunks: Vec<(&[u8; 4], Vec<u8>)> = vec![(b"VP8X", vp8x), (b"ANIM", vec![0; 6])];
    for _ in 0..frames {
        let mut f = Vec::new();
        f.extend(u24(0));
        f.extend(u24(0));
        f.extend(u24(size[0] - 1));
        f.extend(u24(size[1] - 1));
        f.extend(u24(ms));
        f.push(0);
        f.extend(b"VP8L");
        f.extend(5u32.to_le_bytes());
        f.extend([0x2f, 0, 0, 0, 0, 0]);
        chunks.push((b"ANMF", f));
    }
    webp_of(&chunks)
}

fn png_of(size: [u32; 2], fctl: &[(u32, u32, u16, u16)]) -> Vec<u8> {
    let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8; 4], data: Vec<u8>| {
        p.extend((data.len() as u32).to_be_bytes());
        p.extend(kind);
        p.extend(&data);
        p.extend([0; 4]);
    };
    let mut ihdr = Vec::new();
    ihdr.extend(size[0].to_be_bytes());
    ihdr.extend(size[1].to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(b"IHDR", ihdr);
    if !fctl.is_empty() {
        let mut a = (fctl.len() as u32).to_be_bytes().to_vec();
        a.extend(0u32.to_be_bytes());
        chunk(b"acTL", a);
    }
    for (i, (w, h, num, den)) in fctl.iter().enumerate() {
        let mut f = (i as u32).to_be_bytes().to_vec();
        f.extend(w.to_be_bytes());
        f.extend(h.to_be_bytes());
        f.extend([0; 8]);
        f.extend(num.to_be_bytes());
        f.extend(den.to_be_bytes());
        f.extend([0, 0]);
        chunk(b"fcTL", f);
    }
    chunk(b"IDAT", vec![0; 8]);
    chunk(b"IEND", Vec::new());
    p
}

#[test]
fn a_gif_is_read_without_decoding_its_frames() {
    let p = probe(&test_gif([56, 56], 12, 5)).unwrap();
    assert_eq!(p.format, Format::Gif);
    assert_eq!(p.size, [56, 56]);
    assert_eq!(p.frames, 12);
    assert_eq!(p.durations, vec![50; 12]);
    let Verdict::Animated(kept) = p.verdict() else {
        panic!()
    };
    assert_eq!(kept.len(), 12);
    // A frame of 10 ms or less lasts 100, as the browsers have it.
    let fast = probe(&test_gif([56, 56], 3, 1)).unwrap();
    assert_eq!(fast.durations, [100, 100, 100]);
    // One frame: still.
    let one = probe(&test_gif([28, 28], 1, 0)).unwrap();
    assert!(!one.animated());
    assert_eq!(one.verdict(), Verdict::Still);
    // Cut off: the frames whole so far.
    let mut cut = test_gif([56, 56], 5, 5);
    cut.truncate(cut.len() - 6);
    assert_eq!(probe(&cut).unwrap().frames, 4);
}

#[test]
fn webp_png_and_jpeg_are_read_too() {
    let p = probe(&animated_webp([112, 56], 30, 40)).unwrap();
    assert_eq!((p.format, p.size, p.frames), (Format::WebP, [112, 56], 30));
    assert_eq!(p.durations[0], 40);
    // A still WebP (lossless): its size from the bitstream's header.
    let bits: u32 = 55 | (27 << 14);
    let mut vp8l = vec![0x2f];
    vp8l.extend(bits.to_le_bytes());
    let still = probe(&webp_of(&[(b"VP8L", vp8l)])).unwrap();
    assert_eq!((still.size, still.frames), ([56, 28], 1));
    // APNG: its frames from fcTL; a plain PNG still.
    let apng = probe(&png_of([64, 64], &[(64, 64, 1, 20), (64, 64, 3, 0), (64, 64, 0, 0)])).unwrap();
    assert_eq!(apng.durations, [50, 30, 100], "3/100 s where den is 0, as APNG says");
    let plain = probe(&png_of([64, 32], &[])).unwrap();
    assert_eq!((plain.format, plain.size, plain.frames), (Format::Png, [64, 32], 1));
    let jpeg = [
        0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8, 0, 40, 0, 50, 3, 0, 0, 0,
    ];
    assert_eq!(probe(&jpeg).unwrap().size, [50, 40]);
}

/// The hostile and the huge: refused, or still, from the headers alone
/// (nothing here is decoded, and nothing is allocated for pixels).
#[test]
fn hostile_images_are_refused_before_they_are_decoded() {
    // Enormous sides: a GIF's canvas, a frame inside a small canvas, a
    // PNG decompression bomb (30000 × 30000 in a few bytes), a WebP.
    assert_eq!(probe(&test_gif([65535, 65535], 1, 0)), Err(Refusal::TooWide));
    let mut inner = test_gif([16, 16], 1, 0);
    // The frame's width (its descriptor after the table and the GCE).
    let at = 13 + 6 + 8 + 5;
    inner[at..at + 2].copy_from_slice(&5000u16.to_le_bytes());
    assert_eq!(probe(&inner), Err(Refusal::TooWide));
    assert_eq!(probe(&png_of([30000, 30000], &[])), Err(Refusal::TooWide));
    assert_eq!(probe(&animated_webp([4000, 10], 2, 50)), Err(Refusal::TooWide));
    assert_eq!(
        probe(&png_of([64, 64], &[(64, 64, 1, 10), (9000, 64, 1, 10)])),
        Err(Refusal::TooWide)
    );
    // More bytes than an emote's.
    assert_eq!(probe(&vec![0u8; MOST_BYTES + 1]), Err(Refusal::TooLarge));
    // Not an image, or broken.
    assert_eq!(probe(b"<html>"), Err(Refusal::Unknown));
    assert_eq!(probe(b"GIF89a\x10\x00\x10\x00\x00\x00\x00;"), Err(Refusal::Broken));
    assert_eq!(probe(&test_gif([0, 16], 1, 0)), Err(Refusal::Broken));
    assert_eq!(probe(&webp_of(&[(b"ANIM", vec![0; 6])])), Err(Refusal::Broken));
    // A huge WebP chunk length pointing past the end: not followed.
    let mut liar = animated_webp([16, 16], 2, 50);
    let at = 12 + 4;
    liar[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(probe(&liar).is_err());
    // Thousands of frames: read (cheaply), the animation refused: still.
    let many = probe(&test_gif([16, 16], 5000, 4)).unwrap();
    assert_eq!(many.frames, 5000);
    assert!(many.durations.len() <= MOST_FRAMES + 1);
    assert_eq!(many.verdict(), Verdict::Still);
    // Within the frames but past the work: 200 frames of 1024 × 1024.
    let work = probe(&animated_webp([1024, 1024], 200, 40)).unwrap();
    assert_eq!(work.verdict(), Verdict::Still);
    assert!(matches!(
        probe(&animated_webp([256, 256], 200, 40)).unwrap().verdict(),
        Verdict::Animated(_)
    ));
}

#[test]
fn long_animations_are_thinned_to_what_the_atlas_keeps() {
    // 150 frames of 20 ms: every third kept, 60 ms each, the loop as long.
    let k = plan(&[20; 150], KEPT_FRAMES);
    assert_eq!(k.len(), 50);
    assert_eq!(k[1], Kept { index: 3, ms: 60 });
    assert_eq!(k.iter().map(|k| k.ms).sum::<u32>(), 3000);
    // An uneven tail keeps its own sum.
    let k = plan(&[10, 20, 30, 40, 50], 2);
    assert_eq!(k, [Kept { index: 0, ms: 60 }, Kept { index: 3, ms: 90 }]);
    // A wide emote keeps fewer frames: 4 units a frame, 32 frames.
    assert_eq!(most_kept([56, 56]), 64);
    assert_eq!(most_kept([224, 56]), 32);
    let Verdict::Animated(kept) = probe(&test_gif([224, 56], 100, 4)).unwrap().verdict() else {
        panic!()
    };
    assert!(kept.len() <= 32);
}

/// Frames advance on the host's clock, the same frame for every copy at
/// one moment, held while the frames run late, standing still under
/// reduced motion.
#[test]
fn frames_advance_on_the_clock_held_under_load_and_still_when_reduced() {
    let d = [100, 50, 250];
    assert_eq!(frame_at(&d, 0), 0);
    assert_eq!(frame_at(&d, 99), 0);
    assert_eq!(frame_at(&d, 100), 1);
    assert_eq!(frame_at(&d, 150), 2);
    assert_eq!(frame_at(&d, 400), 0, "it loops");
    assert_eq!(frame_at(&[0], 5000), 0);
    assert_eq!(frame_at(&[0, 0], 5000), 0);
    let mut c = Clock::default();
    c.advance(10.0, true);
    assert_eq!(c.ms(), 0, "from the first frame");
    c.advance(10.12, true);
    assert_eq!(c.ms(), 120);
    // A late stretch: held on the frame it had.
    c.advance(10.5, false);
    c.advance(11.0, false);
    assert_eq!(c.ms(), 120);
    // On again from where it held.
    c.advance(11.1, true);
    assert_eq!(c.ms(), 220);
    // Reduced motion: never runs.
    let mut still = Clock::default();
    for t in 0..100 {
        still.advance(f64::from(t) * 0.1, false);
    }
    assert_eq!(still.ms(), 0);
}
