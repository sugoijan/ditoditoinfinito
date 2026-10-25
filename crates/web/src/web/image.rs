//! Image decoding by the browser (`createImageBitmap`), so no image decoder
//! is compiled into the wasm; skins' SVG images are drawn by the SVG worker
//! instead (`svg.rs`), as desktop draws them.
//!
//! Every bitmap here keeps its colors unmultiplied by its alpha
//! (`premultiplyAlpha: "none"`): the textures are filled with
//! `premultiplied_alpha: false`, which WebGPU honours whatever the bitmap,
//! but WebGL2 takes an `ImageBitmap` as it is, so a premultiplied one
//! darkened every half-transparent edge there (alpha arrows' by up to 31
//! levels). A browser that refuses the option gets the default.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, CanvasRenderingContext2d, HtmlCanvasElement, ImageBitmap, ImageBitmapOptions,
    PremultiplyAlpha, ResizeQuality,
};

use super::js_err;

/// A `width × height` canvas of `document`, and its 2D context.
pub(crate) fn canvas_2d(
    document: &web_sys::Document,
    width: u32,
    height: u32,
) -> Result<(HtmlCanvasElement, CanvasRenderingContext2d), String> {
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|e| js_err("canvas", e))?
        .dyn_into()
        .map_err(|_| "not a canvas")?;
    canvas.set_width(width);
    canvas.set_height(height);
    let context: CanvasRenderingContext2d = canvas
        .get_context("2d")
        .map_err(|e| js_err("canvas", e))?
        .ok_or("no 2D canvas")?
        .dyn_into()
        .map_err(|_| "no 2D canvas")?;
    Ok((canvas, context))
}

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
    let full = bitmap("decode image", |options| match options {
        Some(o) => window.create_image_bitmap_with_blob_and_image_bitmap_options(blob, o),
        None => window.create_image_bitmap_with_blob(blob),
    })
    .await?;
    let (w, h) = (full.width(), full.height());
    if w == 0 || h == 0 {
        return Err("empty image".into());
    }
    if w.max(h) <= max_side {
        return Ok((full, (w, h)));
    }
    let scale = f64::from(max_side) / f64::from(w.max(h));
    let resize = |o: &ImageBitmapOptions| {
        o.set_resize_width(((f64::from(w) * scale).round() as u32).max(1));
        o.set_resize_height(((f64::from(h) * scale).round() as u32).max(1));
        o.set_resize_quality(ResizeQuality::High);
    };
    let scaled = bitmap("resize image", |options| {
        let fallback = ImageBitmapOptions::new();
        let o = options.unwrap_or(&fallback);
        resize(o);
        window.create_image_bitmap_with_image_bitmap_and_image_bitmap_options(&full, o)
    })
    .await;
    full.close();
    scaled.map(|s| (s, (w, h)))
}

/// A bitmap from `make`, given options asking for unmultiplied colors
/// (`Some`), else, when the browser refuses that, the default (`None`).
pub(crate) async fn bitmap(
    what: &str,
    make: impl Fn(Option<&ImageBitmapOptions>) -> Result<js_sys::Promise, wasm_bindgen::JsValue>,
) -> Result<ImageBitmap, String> {
    let straight = ImageBitmapOptions::new();
    straight.set_premultiply_alpha(PremultiplyAlpha::None);
    let got = match make(Some(&straight)) {
        Ok(promise) => JsFuture::from(promise).await,
        Err(e) => Err(e),
    };
    let got = match got {
        Ok(b) => Ok(b),
        Err(e) => {
            web_sys::console::warn_1(
                &format!(
                    "{what}: {}; decoded with the defaults",
                    js_err("createImageBitmap", e)
                )
                .into(),
            );
            JsFuture::from(make(None).map_err(|e| js_err("createImageBitmap", e))?).await
        }
    };
    got.map_err(|e| js_err(what, e))?
        .dyn_into()
        .map_err(|e| js_err(what, e))
}

/// Rasterises an SVG (already checked, `ddi_noteskin::svg`) with the
/// browser, at `at` pixels: where the SVG worker
/// (`svg.rs`) does not draw, and for tests comparing the two. The browser
/// draws it through an image element, which runs no script and loads
/// nothing, onto a canvas of the final size, so the vector art is drawn at
/// that size rather than scaled up.
pub(crate) async fn rasterise_svg(text: &str, at: (u32, u32)) -> Result<ImageBitmap, String> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let (w, h) = at;
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
    let (canvas, context) = canvas_2d(&document, w, h)?;
    context
        .draw_image_with_html_image_element_and_dw_and_dh(
            &image,
            0.0,
            0.0,
            f64::from(w),
            f64::from(h),
        )
        .map_err(|e| js_err("draw SVG", e))?;
    bitmap("rasterise SVG", |options| match options {
        Some(o) => {
            window.create_image_bitmap_with_html_canvas_element_and_image_bitmap_options(&canvas, o)
        }
        None => window.create_image_bitmap_with_html_canvas_element(&canvas),
    })
    .await
}

/// Draws text as a back or mask layer shows it (danoniplus's `<span>`:
/// its base font, `"Meiryo UI", sans-serif`, at `size` px, 16 when 0, in
/// `color`, else the color danoniplus's page templates give text,
/// `#cccccc`, as CSS keeps it for a color it does not read; lines at the
/// font's height) at `scale` times that size. Returns the bitmap and its
/// size in the stage's px.
pub(crate) async fn text_bitmap(
    text: &str,
    size: u32,
    color: &str,
    scale: f64,
) -> Result<(ImageBitmap, (f32, f32)), String> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let size = if size == 0 {
        16.0
    } else {
        f64::from(size.min(400))
    };
    let px = size * scale;
    let font = format!("{px:.1}px \"Meiryo UI\", sans-serif");
    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .map_err(|e| js_err("canvas", e))?
        .dyn_into()
        .map_err(|_| "not a canvas")?;
    let context =
        |canvas: &web_sys::HtmlCanvasElement| -> Result<web_sys::CanvasRenderingContext2d, String> {
            canvas
                .get_context("2d")
                .map_err(|e| js_err("canvas", e))?
                .ok_or("no 2D canvas")?
                .dyn_into()
                .map_err(|_| "no 2D canvas".to_string())
        };
    let lines: Vec<&str> = text.split('\n').take(64).collect();
    let ctx = context(&canvas)?;
    ctx.set_font(&font);
    let width = lines
        .iter()
        .map(|l| ctx.measure_text(l).map(|m| m.width()).unwrap_or(0.0))
        .fold(0.0f64, f64::max);
    let line = px * 1.2;
    let (w, h) = (
        (width.ceil() as u32).clamp(1, 2048),
        ((line * lines.len() as f64).ceil() as u32).clamp(1, 2048),
    );
    canvas.set_width(w);
    canvas.set_height(h);
    // Resizing resets the context's state.
    let ctx = context(&canvas)?;
    ctx.set_font(&font);
    ctx.set_text_baseline("top");
    // A color the canvas does not read leaves the one before.
    ctx.set_fill_style_str("#cccccc");
    if !color.is_empty() {
        ctx.set_fill_style_str(color);
    }
    for (i, l) in lines.iter().enumerate() {
        let _ = ctx.fill_text(l, 0.0, line * i as f64 + (line - px) / 2.0);
    }
    let bitmap = bitmap("draw text", |options| match options {
        Some(o) => {
            window.create_image_bitmap_with_html_canvas_element_and_image_bitmap_options(&canvas, o)
        }
        None => window.create_image_bitmap_with_html_canvas_element(&canvas),
    })
    .await?;
    Ok((
        bitmap,
        ((f64::from(w) / scale) as f32, (f64::from(h) / scale) as f32),
    ))
}
