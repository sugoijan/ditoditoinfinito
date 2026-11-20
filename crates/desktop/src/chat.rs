//! A room host's chat on desktop (`docs/plans/multiplayer-twitch.md`,
//! part C): its emote images fetched and decoded off the frame, each on a
//! thread of its own (four at most at once, the chat's cache says), and,
//! TESTING ONLY, the scripted chat `--mp-dev-chat` plays
//! (`ddi_chat::script`).
//!
//! An image is read before it is decoded (`ddi_chat::anim::probe`,
//! round TW4): refused where it is larger than an emote or broken; an
//! animation within its limits decoded frame by frame with the `image`
//! crate (GIF, WebP, APNG), only the frames the atlas keeps scaled and
//! kept, under the crate's own limits and a deadline, else its first
//! frame, still.
//!
//! No request reaches Twitch or an emote provider in this round: the
//! scripted chat's images are its own shapes (drawn by `ddi_shell::chat`)
//! or files and addresses its script names (a local test server's).

use std::io::Cursor;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use ddi_chat::anim::{self, Format, Kept, Verdict};
use ddi_chat::cache::Fetch;
use ddi_chat::script::Script;
use ddi_shell::chat::{Decoded, decode_size};

/// Images a fetch reads at most (an emote's still image is a few
/// kilobytes, an animated one rarely a megabyte).
const MOST_BYTES: u64 = anim::MOST_BYTES as u64;
/// How long an animation's frames may take to decode on their thread:
/// past it, its first frame, still.
const DEADLINE: Duration = Duration::from_millis(1500);
/// Bytes the decoder may allocate at once (its canvas and buffers).
const MOST_ALLOC: u64 = 64 << 20;

/// The fetches in flight and what came back.
pub struct Fetcher {
    tx: Sender<(Fetch, Option<Decoded>)>,
    rx: Receiver<(Fetch, Option<Decoded>)>,
}

impl Default for Fetcher {
    fn default() -> Fetcher {
        let (tx, rx) = channel();
        Fetcher { tx, rx }
    }
}

impl Fetcher {
    /// Fetches `f` on a thread: an `http(s)` address through the shared
    /// client, else a file's path.
    pub fn fetch(&mut self, f: &Fetch) -> bool {
        let tx = self.tx.clone();
        let f = f.clone();
        std::thread::Builder::new()
            .name("ddi-chat-emote".into())
            .spawn(move || {
                let decoded = read(&f.url).and_then(|bytes| decode(&bytes));
                let _ = tx.send((f, decoded));
            })
            .is_ok()
    }

    /// The fetches that ended since.
    pub fn take(&mut self) -> Vec<(Fetch, Option<Decoded>)> {
        self.rx.try_iter().collect()
    }
}

fn read(url: &str) -> Option<Vec<u8>> {
    if url.starts_with("http://") || url.starts_with("https://") {
        let mut answer = crate::share::agent().get(url).call().ok()?;
        if !answer.status().is_success() {
            return None;
        }
        answer
            .body_mut()
            .with_config()
            .limit(MOST_BYTES)
            .read_to_vec()
            .ok()
    } else {
        let path = url.strip_prefix("file://").unwrap_or(url);
        let meta = std::fs::metadata(path).ok()?;
        (meta.len() <= MOST_BYTES).then(|| std::fs::read(path).ok())?
    }
}

/// An image read ([`anim::probe`]) and decoded to the atlas's row
/// ([`ddi_shell::chat::decode_size`]): every kept frame of an animation
/// within the limits, else its first frame; `None` where it is refused.
fn decode(bytes: &[u8]) -> Option<Decoded> {
    decode_by(bytes, Instant::now() + DEADLINE)
}

fn decode_by(bytes: &[u8], deadline: Instant) -> Option<Decoded> {
    let probe = anim::probe(bytes).ok()?;
    let size = decode_size(probe.size);
    if let Verdict::Animated(kept) = probe.verdict()
        && let Some(d) = animation(bytes, probe.format, &kept, size, deadline)
    {
        return Some(d);
    }
    still(bytes, probe.format, size)
}

fn limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_image_width = Some(anim::MOST_SIDE);
    l.max_image_height = Some(anim::MOST_SIDE);
    l.max_alloc = Some(MOST_ALLOC);
    l
}

fn image_format(f: Format) -> image::ImageFormat {
    match f {
        Format::Png => image::ImageFormat::Png,
        Format::Gif => image::ImageFormat::Gif,
        Format::WebP => image::ImageFormat::WebP,
        Format::Jpeg => image::ImageFormat::Jpeg,
    }
}

/// The first frame (a still image's only one), scaled to `size`.
fn still(bytes: &[u8], format: Format, size: [u32; 2]) -> Option<Decoded> {
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image_format(format));
    reader.limits(limits());
    let image = reader.decode().ok()?.to_rgba8();
    let scaled = image::imageops::resize(&image, size[0], size[1], image::imageops::FilterType::Triangle);
    Some(Decoded::still(size, scaled.into_raw()))
}

/// The `kept` frames of an animation, each scaled to `size`, by
/// `deadline`; `None` where it cannot be (a decoder's refusal, fewer than
/// two frames, the deadline passed).
fn animation(
    bytes: &[u8],
    format: Format,
    kept: &[Kept],
    size: [u32; 2],
    deadline: Instant,
) -> Option<Decoded> {
    use image::AnimationDecoder;
    use image::ImageDecoder;
    let frames: image::Frames<'_> = match format {
        Format::Gif => {
            let mut d = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).ok()?;
            d.set_limits(limits()).ok()?;
            d.into_frames()
        }
        Format::WebP => {
            let mut d = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).ok()?;
            d.set_limits(limits()).ok()?;
            d.into_frames()
        }
        Format::Png => {
            let mut d = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).ok()?;
            d.set_limits(limits()).ok()?;
            d.apng().ok()?.into_frames()
        }
        Format::Jpeg => return None,
    };
    let last = kept.last()?.index;
    let mut want = kept.iter().peekable();
    let mut rgba = Vec::new();
    let mut durations = Vec::new();
    for (i, frame) in frames.enumerate() {
        if i > last || Instant::now() > deadline {
            break;
        }
        let Ok(frame) = frame else { break };
        if want.peek().is_some_and(|k| k.index == i) {
            let k = want.next()?;
            let scaled = image::imageops::resize(
                frame.buffer(),
                size[0],
                size[1],
                image::imageops::FilterType::Triangle,
            );
            rgba.extend_from_slice(scaled.as_raw());
            durations.push(k.ms);
        }
    }
    if Instant::now() > deadline || durations.len() < 2 {
        return None;
    }
    Some(Decoded {
        size,
        rgba,
        durations,
    })
}

/// TESTING ONLY: the script `--mp-dev-chat` names: a file of one, else a
/// built-in one by name (`demo`, `emotes`, `flood`) or a rate (messages
/// a second).
pub fn load_script(what: &str) -> Result<Script, String> {
    match std::fs::read_to_string(what) {
        Ok(text) => Script::parse(&text).map_err(|e| format!("{what}: {e}")),
        Err(_) => Script::builtin(what)
            .ok_or_else(|| format!("{what}: no such file, built-in script or rate")),
    }
}

/// TESTING ONLY: emote pictures made by code for the scripted chat's walks
/// and the decoders' tests (round TW4), never a provider's: a GIF and an
/// animated WebP of given frames (`image`'s encoders; the WebP's frames
/// its lossless ones put in an animation's chunks), and every picture a
/// script names written to a folder (`examples/chat_images.rs`).
pub mod made {
    use std::io::Cursor;
    use std::path::Path;

    /// An animated GIF of `frames` (RGBA rows of `size`), each `ms` long,
    /// looping.
    pub fn gif(size: [u32; 2], frames: &[Vec<u8>], ms: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = image::codecs::gif::GifEncoder::new(&mut out);
            e.set_repeat(image::codecs::gif::Repeat::Infinite).expect("a GIF");
            for f in frames {
                let buffer = image::RgbaImage::from_raw(size[0], size[1], f.clone()).expect("a frame");
                let delay = image::Delay::from_numer_denom_ms(ms, 1);
                e.encode_frame(image::Frame::from_parts(buffer, 0, 0, delay))
                    .expect("a GIF frame");
            }
        }
        out
    }

    fn u24(v: u32) -> [u8; 3] {
        let b = v.to_le_bytes();
        [b[0], b[1], b[2]]
    }

    /// An animated WebP of `frames` (RGBA rows of `size`), each `ms` long.
    pub fn webp(size: [u32; 2], frames: &[Vec<u8>], ms: u32) -> Vec<u8> {
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = kind.to_vec();
            c.extend((data.len() as u32).to_le_bytes());
            c.extend(data);
            if data.len() % 2 == 1 {
                c.push(0);
            }
            c
        };
        let mut body = b"WEBP".to_vec();
        let mut vp8x = vec![0x02 | 0x10, 0, 0, 0];
        vp8x.extend(u24(size[0] - 1));
        vp8x.extend(u24(size[1] - 1));
        body.extend(chunk(b"VP8X", &vp8x));
        body.extend(chunk(b"ANIM", &[0, 0, 0, 0, 0, 0]));
        for f in frames {
            let mut one = Vec::new();
            image::codecs::webp::WebPEncoder::new_lossless(Cursor::new(&mut one))
                .encode(f, size[0], size[1], image::ExtendedColorType::Rgba8)
                .expect("a WebP frame");
            // Its VP8L chunk, whole.
            let mut at = 12;
            let mut vp8l = None;
            while at + 8 <= one.len() {
                let len = u32::from_le_bytes(one[at + 4..at + 8].try_into().unwrap()) as usize;
                if &one[at..at + 4] == b"VP8L" {
                    vp8l = Some(chunk(b"VP8L", &one[at + 8..at + 8 + len]));
                }
                at += 8 + len + (len & 1);
            }
            let mut anmf = Vec::new();
            anmf.extend(u24(0));
            anmf.extend(u24(0));
            anmf.extend(u24(size[0] - 1));
            anmf.extend(u24(size[1] - 1));
            anmf.extend(u24(ms));
            // Not blended over the frame before.
            anmf.push(0x02);
            anmf.extend(vp8l.expect("a lossless frame"));
            body.extend(chunk(b"ANMF", &anmf));
        }
        let mut out = b"RIFF".to_vec();
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        out
    }

    /// A PNG of `rgba` rows of `size`.
    pub fn png(size: [u32; 2], rgba: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbaImage::from_raw(size[0], size[1], rgba)
            .expect("an image")
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("a PNG");
        out
    }

    /// Writes every picture `script` names into `dir`: a still one's
    /// `<id>.png`, an animated one's `<id>.gif` and `<id>.webp` (the
    /// scripted chat's `images_at`, the web's `mpfakechatimg`). The files
    /// written.
    pub fn write(script: &ddi_chat::script::Script, dir: &Path) -> std::io::Result<Vec<String>> {
        std::fs::create_dir_all(dir)?;
        let mut out = Vec::new();
        for (id, animated, spec) in script.emote_specs() {
            if animated {
                let Some((size, rgba, durations)) = ddi_chat::script::anim_pixels(&spec) else {
                    continue;
                };
                let n = (size[0] * size[1] * 4) as usize;
                let frames: Vec<Vec<u8>> = rgba.chunks(n).map(<[u8]>::to_vec).collect();
                let ms = durations.first().copied().unwrap_or(100);
                for (kind, bytes) in [("gif", gif(size, &frames, ms)), ("webp", webp(size, &frames, ms))] {
                    let name = format!("{id}.{kind}");
                    std::fs::write(dir.join(&name), bytes)?;
                    out.push(name);
                }
            } else if let Some((size, rgba)) = ddi_chat::script::shape_pixels(&spec) {
                let name = format!("{id}.png");
                std::fs::write(dir.join(&name), png(size, rgba))?;
                out.push(name);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_is_decoded_to_the_atlas_row() {
        let mut png = Vec::new();
        let image = image::RgbaImage::from_pixel(28, 14, image::Rgba([255, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let d = decode(&png).unwrap();
        assert_eq!(d.size, [128, 64]);
        assert_eq!(d.rgba.len(), 128 * 64 * 4);
        assert_eq!(&d.rgba[..4], [255, 0, 0, 255]);
    }

    #[test]
    fn scripts_load_from_files_names_and_rates() {
        assert!(load_script("demo").is_ok());
        assert!(load_script("30").is_ok());
        assert!(load_script("/no/such/script").is_err());
    }

    /// Frames that differ: a color a frame.
    fn frames(size: [u32; 2], n: usize) -> Vec<Vec<u8>> {
        (0..n)
            .map(|k| {
                let c = [(40 * k % 256) as u8, (200 - 20 * (k % 10)) as u8, 90, 255];
                c.iter().copied().cycle().take((size[0] * size[1] * 4) as usize).collect()
            })
            .collect()
    }

    /// Round TW4: a GIF's and an animated WebP's frames decoded to the
    /// atlas's row, their durations the files' own.
    #[test]
    fn animated_gifs_and_webps_are_decoded_frame_by_frame() {
        for (name, bytes) in [
            ("gif", made::gif([28, 28], &frames([28, 28], 6), 80)),
            ("webp", made::webp([28, 28], &frames([28, 28], 6), 80)),
        ] {
            let d = decode(&bytes).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(d.size, [64, 64], "{name}");
            assert_eq!(d.durations, vec![80; 6], "{name}");
            assert_eq!(d.rgba.len(), 6 * 64 * 64 * 4, "{name}");
            assert_ne!(d.frame(0), d.frame(1), "{name}: frames differ");
            let px = d.frame(2).unwrap();
            assert!(px[0].abs_diff(80) < 8 && px[1].abs_diff(160) < 8, "{name}: {:?}", &px[..4]);
        }
        // A long one thinned to 64 frames, its loop as long.
        let long = made::gif([16, 16], &frames([16, 16], 130), 20);
        let d = decode(&long).unwrap();
        assert!(d.durations.len() <= 64, "{}", d.durations.len());
        assert_eq!(d.durations.iter().sum::<u32>(), 130 * 20);
    }

    /// The hostile and the huge: refused, or their first frame still,
    /// before their frames are decoded.
    #[test]
    fn hostile_images_are_refused_or_still() {
        use ddi_chat::anim::test_gif;
        // Enormous sides (a GIF's 65535 × 65535 in a few bytes, a PNG
        // whose header claims 30000 × 30000): never decoded.
        let t = Instant::now();
        assert!(decode(&test_gif([65535, 65535], 1, 0)).is_none());
        let mut png = made::png([4, 4], vec![255; 64]);
        png[16..20].copy_from_slice(&30000u32.to_be_bytes());
        png[20..24].copy_from_slice(&30000u32.to_be_bytes());
        assert!(decode(&png).is_none());
        // Thousands of frames: their first frame alone, still.
        let many = test_gif([16, 16], 5000, 4);
        if let Some(d) = decode(&many) {
            assert_eq!(d.durations.len(), 1);
        }
        // More bytes than an emote's, and not an image at all.
        assert!(decode(&vec![0x47; (2 << 20) + 1]).is_none());
        assert!(decode(b"<html>not an image</html>").is_none());
        assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
        // An animation past its deadline: its first frame, still.
        let gif = made::gif([28, 28], &frames([28, 28], 6), 80);
        let d = decode_by(&gif, Instant::now()).unwrap();
        assert_eq!(d.durations.len(), 1);
    }

    /// The scripted chat's pictures written as files (the walks' images).
    #[test]
    fn the_scripted_chats_pictures_are_written_as_files() {
        let dir = std::env::temp_dir().join(format!("ddi-chat-images-{}", std::process::id()));
        let names = made::write(&Script::builtin("demo").unwrap(), &dir).unwrap();
        assert!(names.iter().any(|n| n == "25.png"));
        assert!(names.iter().any(|n| n == "c2.gif") && names.iter().any(|n| n == "c2.webp"));
        let gif = std::fs::read(dir.join("c2.gif")).unwrap();
        assert_eq!(decode(&gif).unwrap().durations, vec![70; 12]);
        let webp = std::fs::read(dir.join("c2.webp")).unwrap();
        assert_eq!(decode(&webp).unwrap().durations, vec![70; 12]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
