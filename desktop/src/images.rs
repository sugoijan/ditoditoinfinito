//! Images for the painter's texture cache on desktop: the bundled songs'
//! banners and backgrounds (`songs/<id>/<file>`, the web's paths) from
//! the repository's `assets/` (or `DDI_ASSETS`), and files by absolute
//! path, decoded (PNG, JPEG) and scaled down to the size asked for, off
//! the drawing thread ([`Loader`]), as the web's `createImageBitmap`
//! decodes off the page's.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use ddi_render::texture_cache::TextureId;

/// Threads decoding at once: a long flick's rows ask a screenful at a
/// time, and a decode is a few milliseconds.
const WORKERS: usize = 2;

/// An image decoded: its size and encoded RGBA rows (`None`: it could
/// not be).
pub type Decoded = (TextureId, Option<Image>);

/// An image's size and encoded RGBA rows.
pub type Image = ([u32; 2], Vec<u8>);

/// Decodes images on worker threads: [`Loader::request`] from a frame,
/// [`Loader::take`] the results at the next.
pub struct Loader {
    requests: Option<mpsc::Sender<(TextureId, String, u32)>>,
    results: mpsc::Receiver<Decoded>,
    ready: Vec<Decoded>,
    pending: usize,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl Loader {
    pub fn new() -> Loader {
        Loader::with_decoder(load)
    }

    /// A loader decoding with `decode` (tests).
    pub fn with_decoder(decode: fn(&str, u32) -> Option<Image>) -> Loader {
        let (requests, queue) = mpsc::channel::<(TextureId, String, u32)>();
        let (done, results) = mpsc::channel();
        let queue = Arc::new(Mutex::new(queue));
        let workers = (0..WORKERS)
            .map(|i| {
                let (queue, done) = (queue.clone(), done.clone());
                std::thread::Builder::new()
                    .name(format!("ddi-images-{i}"))
                    .spawn(move || {
                        loop {
                            // The lock only while taking the next one.
                            let next = queue.lock().ok().and_then(|q| q.recv().ok());
                            let Some((id, source, side)) = next else {
                                return;
                            };
                            // A decoder that panics fails the image (its
                            // placeholder stays) and the thread goes on.
                            let image = std::panic::catch_unwind(|| decode(&source, side))
                                .ok()
                                .flatten();
                            if done.send((id, image)).is_err() {
                                return;
                            }
                        }
                    })
                    .expect("an image thread")
            })
            .collect();
        Loader {
            requests: Some(requests),
            results,
            ready: Vec::new(),
            pending: 0,
            workers,
        }
    }

    /// Decodes `source` at most `side` pixels long (0: as it is).
    pub fn request(&mut self, id: TextureId, source: &str, side: u32) {
        if let Some(r) = &self.requests
            && r.send((id, source.to_string(), side)).is_ok()
        {
            self.pending += 1;
        }
    }

    /// Decodes asked for and not yet taken.
    pub fn pending(&self) -> usize {
        self.pending
    }

    /// Whether decoded images wait to be taken.
    pub fn has_ready(&mut self) -> bool {
        self.ready.extend(self.results.try_iter());
        !self.ready.is_empty()
    }

    /// The images decoded so far.
    pub fn take(&mut self) -> Vec<Decoded> {
        self.ready.extend(self.results.try_iter());
        self.pending -= self.ready.len();
        std::mem::take(&mut self.ready)
    }

    /// Waits for every decode asked for (tests), then takes them.
    pub fn wait(&mut self) -> Vec<Decoded> {
        while self.ready.len() < self.pending {
            match self.results.recv() {
                Ok(d) => self.ready.push(d),
                Err(_) => break,
            }
        }
        self.take()
    }
}

impl Default for Loader {
    fn default() -> Loader {
        Loader::new()
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        // The workers end once the queue is gone and the decode under way
        // is done.
        self.requests = None;
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

/// The largest image side decoded.
const MAX_SIDE: u32 = 8192;
/// The most a decode may allocate: an image of about 4,900 pixels a side
/// (a background is 1920 by 1080); with [`WORKERS`] decoding at once and
/// the RGBA copy, a few hundred megabytes at worst.
const MAX_ALLOC: u64 = 96 * 1024 * 1024;

/// Where `songs/…` and the rest are read from.
pub fn assets() -> PathBuf {
    std::env::var_os("DDI_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets"))
}

/// The file a texture source names.
pub fn path(source: &str) -> Option<PathBuf> {
    let p = Path::new(source);
    if p.is_absolute() {
        return Some(p.to_path_buf());
    }
    // Site paths only: plain names, nothing above the assets.
    if !p
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    Some(assets().join(p))
}

/// The image at `source`, its longest side at most `max_side` (0: as
/// it is): its size and encoded RGBA rows.
pub fn load(source: &str, max_side: u32) -> Option<([u32; 2], Vec<u8>)> {
    let path = path(source)?;
    let mut reader = image::ImageReader::open(&path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    // A hostile file cannot ask for more than a banner or a background
    // needs.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let img = reader.decode().ok()?;
    let (w, h) = (img.width(), img.height());
    let img = if max_side > 0 && w.max(h) > max_side {
        let s = max_side as f32 / w.max(h) as f32;
        let (nw, nh) = (
            ((w as f32 * s).round() as u32).max(1),
            ((h as f32 * s).round() as u32).max(1),
        );
        img.resize_exact(nw, nh, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    Some(([rgba.width(), rgba.height()], rgba.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_decode_off_the_calling_thread() {
        let mut l = Loader::new();
        l.request(TextureId(3), "songs/sweeteners/banner.jpg", 64);
        l.request(TextureId(4), "songs/nothing/here.png", 64);
        assert_eq!(l.pending(), 2);
        let mut got = l.wait();
        got.sort_by_key(|d| d.0);
        assert_eq!(l.pending(), 0);
        assert_eq!(got.len(), 2);
        let (size, _) = got[0].1.as_ref().expect("the banner");
        assert_eq!(size[0].max(size[1]), 64);
        assert!(got[1].1.is_none());
    }

    #[test]
    fn a_decoder_that_panics_fails_its_image_only() {
        let mut l = Loader::with_decoder(|source, _| {
            assert!(source != "boom", "a broken decoder");
            Some(([1, 1], vec![0; 4]))
        });
        l.request(TextureId(1), "boom", 64);
        l.request(TextureId(2), "fine", 64);
        let mut got = l.wait();
        got.sort_by_key(|d| d.0);
        assert_eq!(l.pending(), 0);
        assert!(got[0].1.is_none());
        assert!(got[1].1.is_some());
    }

    #[test]
    fn bundled_banners_load_scaled_down() {
        let (size, rgba) = load("songs/sweeteners/banner.jpg", 128).expect("banner");
        assert_eq!(size[0].max(size[1]), 128);
        assert_eq!(rgba.len(), (size[0] * size[1] * 4) as usize);
        assert!(load("songs/../Cargo.toml", 64).is_none());
        assert!(load("./songs/sweeteners/banner.jpg", 64).is_none());
        assert!(path("songs//sweeteners").is_some());
        assert!(load("songs/nothing/here.png", 64).is_none());
    }
}
