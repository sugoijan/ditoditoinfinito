//! Image decoding by the browser (`createImageBitmap`), so no image decoder
//! is compiled into the wasm.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, ImageBitmap, ImageBitmapOptions, ResizeQuality};

use super::js_err;

/// Decodes `blob` and scales it down, keeping the aspect ratio, so neither
/// side exceeds `max_side` (GPU texture limits and memory).
pub(crate) async fn decode(blob: &Blob, max_side: u32) -> Result<ImageBitmap, String> {
    let window = web_sys::window().ok_or("no window")?;
    let promise = window
        .create_image_bitmap_with_blob(blob)
        .map_err(|e| js_err("createImageBitmap", e))?;
    let full: ImageBitmap = JsFuture::from(promise)
        .await
        .map_err(|e| js_err("decode image", e))?
        .dyn_into()
        .map_err(|e| js_err("decode image", e))?;
    let (w, h) = (full.width(), full.height());
    if w == 0 || h == 0 {
        return Err("empty image".into());
    }
    if w.max(h) <= max_side {
        return Ok(full);
    }
    let scale = f64::from(max_side) / f64::from(w.max(h));
    let options = ImageBitmapOptions::new();
    options.set_resize_width(((f64::from(w) * scale).round() as u32).max(1));
    options.set_resize_height(((f64::from(h) * scale).round() as u32).max(1));
    options.set_resize_quality(ResizeQuality::High);
    let promise = window
        .create_image_bitmap_with_image_bitmap_and_image_bitmap_options(&full, &options)
        .map_err(|e| js_err("createImageBitmap", e))?;
    let scaled = JsFuture::from(promise)
        .await
        .map_err(|e| js_err("resize image", e))?
        .dyn_into()
        .map_err(|e| js_err("resize image", e));
    full.close();
    scaled
}
