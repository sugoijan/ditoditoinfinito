//! Image decoding by the browser (`createImageBitmap`), so no image decoder
//! is compiled into the wasm.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, ImageBitmap, ImageBitmapOptions, ResizeQuality};

use super::js_err;

/// Decodes `blob` and scales it down, keeping the aspect ratio, so neither
/// side exceeds `max_side` (GPU texture limits and memory).
pub(crate) async fn decode(blob: &Blob, max_side: u32) -> Result<ImageBitmap, String> {
    Ok(decode_sized(blob, max_side).await?.0)
}

/// [`decode`], with the image's size before any scaling down.
pub(crate) async fn decode_sized(
    blob: &Blob,
    max_side: u32,
) -> Result<(ImageBitmap, (u32, u32)), String> {
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
        return Ok((full, (w, h)));
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
    scaled.map(|s| (s, (w, h)))
}

/// Rasterises an SVG (already checked, `ddi_noteskin::svg`) of `size`
/// pixels at `scale` times that, keeping neither side above `max_side`.
/// The browser draws it through an image element, which runs no script
/// and loads nothing, onto a canvas of the final size, so the vector art
/// is drawn at that size rather than scaled up.
pub(crate) async fn rasterise_svg(
    text: &str,
    size: (u32, u32),
    scale: f64,
    max_side: u32,
) -> Result<ImageBitmap, String> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let longest = f64::from(size.0.max(size.1));
    let scale = scale.min(f64::from(max_side) / longest);
    let (w, h) = (
        ((f64::from(size.0) * scale).round() as u32).max(1),
        ((f64::from(size.1) * scale).round() as u32).max(1),
    );
    let blob = super::files::bytes_blob(text.as_bytes(), "image/svg+xml")?;
    let url = super::files::object_url(&blob).ok_or("no object URL")?;
    let image: web_sys::HtmlImageElement = document
        .create_element("img")
        .map_err(|e| js_err("img", e))?
        .dyn_into()
        .map_err(|_| "not an image element")?;
    image.set_src(&url);
    let loaded = JsFuture::from(image.decode()).await;
    super::files::revoke_object_url(&url);
    loaded.map_err(|e| js_err("decode SVG", e))?;
    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|e| js_err("canvas", e))?
        .dyn_into()
        .map_err(|_| "not a canvas")?;
    canvas.set_width(w);
    canvas.set_height(h);
    let context: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .map_err(|e| js_err("canvas", e))?
        .ok_or("no 2D canvas")?
        .dyn_into()
        .map_err(|_| "no 2D canvas")?;
    context
        .draw_image_with_html_image_element_and_dw_and_dh(
            &image,
            0.0,
            0.0,
            f64::from(w),
            f64::from(h),
        )
        .map_err(|e| js_err("draw SVG", e))?;
    let promise = window
        .create_image_bitmap_with_html_canvas_element(&canvas)
        .map_err(|e| js_err("createImageBitmap", e))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| js_err("rasterise SVG", e))?
        .dyn_into()
        .map_err(|e| js_err("rasterise SVG", e))
}
