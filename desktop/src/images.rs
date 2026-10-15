//! Images for the painter's texture cache on desktop: the bundled songs'
//! banners and backgrounds (`songs/<id>/<file>`, the web's paths) from
//! the repository's `assets/` (or `DDI_ASSETS`), and files by absolute
//! path, decoded (PNG, JPEG) and scaled down to the size asked for. On
//! the main thread, when a frame asks: enough for the developer screens;
//! the library on disk (phase 9) decodes off it.

use std::path::{Path, PathBuf};

/// The largest image side decoded.
const MAX_SIDE: u32 = 8192;
/// The most a decode may allocate.
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

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
