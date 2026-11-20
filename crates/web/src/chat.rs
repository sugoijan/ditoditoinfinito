//! A room host's chat on the web (`docs/plans/multiplayer-twitch.md`,
//! part C): its emote images fetched and decoded off the frame by the
//! browser (`fetch`, `createImageBitmap`), drawn to the atlas's row on a
//! canvas and read back as RGBA (an image of 64 pixels at most).
//!
//! An image is read before it is decoded (`ddi_chat::anim::probe`, round
//! TW4): refused where it is larger than an emote or broken; an animation
//! within its limits decoded by the browser's `ImageDecoder` (WebCodecs,
//! asynchronous on the browser's own decoders), only the frames the atlas
//! keeps, each scaled by `createImageBitmap` and drawn into one small
//! canvas read back once. Without `ImageDecoder`, or where it refuses
//! the type, the first frame, still.
//!
//! No request reaches Twitch or an emote provider in this round: the
//! scripted chat's images are its own shapes (drawn by `ddi_shell::chat`),
//! or, with `mpfakechatimg=<path>`, files beside the page that a test
//! serves.

use std::cell::RefCell;

use ddi_chat::anim::{self, Format, Kept, Verdict};
use ddi_chat::cache::Fetch;
use ddi_shell::chat::{Decoded, decode_size};
use js_sys::{Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = ImageDecoder)]
    type JsImageDecoder;
    #[wasm_bindgen(constructor, js_class = "ImageDecoder", catch)]
    fn new(init: &Object) -> Result<JsImageDecoder, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn decode(this: &JsImageDecoder, options: &Object) -> Result<Promise, JsValue>;
    #[wasm_bindgen(method, getter)]
    fn completed(this: &JsImageDecoder) -> Promise;
    #[wasm_bindgen(method)]
    fn close(this: &JsImageDecoder);
    #[wasm_bindgen(static_method_of = JsImageDecoder, js_class = "ImageDecoder", js_name = isTypeSupported)]
    fn is_type_supported(kind: &str) -> Promise;
}

thread_local! {
    static BACK: RefCell<Vec<(Fetch, Option<Decoded>)>> = const { RefCell::new(Vec::new()) };
}

/// Fetches `f` and decodes it, later; its answer is in [`take`].
pub(crate) fn fetch(f: &Fetch) -> bool {
    let f = f.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let decoded = match decode(&f.url).await {
            Ok(d) => Some(d),
            Err(e) => {
                web_sys::console::warn_1(&format!("chat emote {}: {e}", f.url).into());
                None
            }
        };
        BACK.with(|b| b.borrow_mut().push((f, decoded)));
    });
    true
}

/// The fetches that ended since.
pub(crate) fn take() -> Vec<(Fetch, Option<Decoded>)> {
    BACK.with(|b| std::mem::take(&mut *b.borrow_mut()))
}

async fn decode(url: &str) -> Result<Decoded, String> {
    let bytes = read(url).await?;
    let buffer = Uint8Array::from(&bytes[..]).buffer();
    let probe = anim::probe(&bytes).map_err(|e| e.to_string())?;
    let size = decode_size(probe.size);
    if let Verdict::Animated(kept) = probe.verdict() {
        match animation(&buffer, probe.format, &kept, size).await {
            Ok(Some(d)) => return Ok(d),
            Ok(None) => {}
            Err(e) => web_sys::console::warn_1(&format!("chat emote {url}: {e}; its first frame").into()),
        }
    }
    let parts = js_sys::Array::of1(&buffer);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(probe.format.mime());
    let blob = web_sys::Blob::new_with_buffer_source_sequence_and_options(&parts, &options)
        .map_err(|e| crate::web::js_err("Blob", e))?;
    still(&blob, size).await
}

/// The bytes at `url`, at most
/// [`anim::MOST_BYTES`]: refused from the answer's length where it says
/// more, else read piece by piece and stopped once past it, so an answer
/// larger than an emote is never held whole.
async fn read(url: &str) -> Result<Vec<u8>, String> {
    let too_large = || anim::Refusal::TooLarge.to_string();
    let resp = gloo::net::http::Request::get(url)
        .send()
        .await
        .map_err(|e| format!("GET: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let raw: web_sys::Response = resp.into();
    let headers = raw.headers();
    let header = |name: &str| headers.get(name).ok().flatten();
    if header("content-length")
        .and_then(|l| l.trim().parse::<u64>().ok())
        .is_some_and(|l| l > anim::MOST_BYTES as u64)
    {
        return Err(too_large());
    }
    let Some(body) = raw.body() else {
        return Ok(Vec::new());
    };
    let reader: web_sys::ReadableStreamDefaultReader =
        body.get_reader().dyn_into().map_err(|_| "no reader")?;
    let mut out = Vec::new();
    loop {
        let chunk = JsFuture::from(reader.read())
            .await
            .map_err(|e| format!("{e:?}"))?;
        if Reflect::get(&chunk, &"done".into())
            .ok()
            .and_then(|d| d.as_bool())
            .unwrap_or(true)
        {
            return Ok(out);
        }
        if let Ok(value) = Reflect::get(&chunk, &"value".into())
            && let Ok(piece) = value.dyn_into::<Uint8Array>()
        {
            if out.len() + piece.length() as usize > anim::MOST_BYTES {
                let _ = reader.cancel();
                return Err(too_large());
            }
            out.extend(piece.to_vec());
        }
    }
}

/// The first frame (a still image's only one), scaled to `size`.
async fn still(blob: &web_sys::Blob, [fw, fh]: [u32; 2]) -> Result<Decoded, String> {
    let (bitmap, _) = crate::web::image::decode_sized(blob, anim::MOST_SIDE).await?;
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let (_canvas, context) = crate::web::image::canvas_2d(&document, fw, fh)?;
    let drawn = context.draw_image_with_image_bitmap_and_dw_and_dh(
        &bitmap,
        0.0,
        0.0,
        f64::from(fw),
        f64::from(fh),
    );
    bitmap.close();
    drawn.map_err(|e| format!("{e:?}"))?;
    let data = context
        .get_image_data(0.0, 0.0, f64::from(fw), f64::from(fh))
        .map_err(|e| format!("{e:?}"))?;
    Ok(Decoded::still([fw, fh], data.data().0))
}

/// The `kept` frames of an animation, decoded by the browser's
/// `ImageDecoder` and scaled to `size`: drawn one under another into one
/// canvas, read back once. `None` where the browser has no
/// `ImageDecoder` or will not decode the type.
async fn animation(
    data: &js_sys::ArrayBuffer,
    format: Format,
    kept: &[Kept],
    [w, h]: [u32; 2],
) -> Result<Option<Decoded>, String> {
    let global = js_sys::global();
    if !Reflect::get(&global, &"ImageDecoder".into()).is_ok_and(|v| v.is_function()) {
        return Ok(None);
    }
    let supported = JsFuture::from(JsImageDecoder::is_type_supported(format.mime()))
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !supported {
        return Ok(None);
    }
    let set = |o: &Object, k: &str, v: &JsValue| {
        Reflect::set(o, &k.into(), v).map_err(|e| crate::web::js_err("ImageDecoder", e))
    };
    let init = Object::new();
    set(&init, "data", data)?;
    set(&init, "type", &format.mime().into())?;
    let decoder = JsImageDecoder::new(&init).map_err(|e| crate::web::js_err("ImageDecoder", e))?;
    let frames = frames(&decoder, kept, [w, h]).await;
    decoder.close();
    frames
}

async fn frames(
    decoder: &JsImageDecoder,
    kept: &[Kept],
    [w, h]: [u32; 2],
) -> Result<Option<Decoded>, String> {
    let err = |e: JsValue| crate::web::js_err("ImageDecoder", e);
    JsFuture::from(decoder.completed()).await.map_err(err)?;
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let n = kept.len() as u32;
    let (_canvas, context) = crate::web::image::canvas_2d(&document, w, h * n)?;
    let create = Reflect::get(&window, &"createImageBitmap".into())
        .map_err(err)?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "no createImageBitmap")?;
    let mut durations = Vec::with_capacity(kept.len());
    for (k, frame) in kept.iter().enumerate() {
        let options = Object::new();
        Reflect::set(&options, &"frameIndex".into(), &(frame.index as u32).into()).map_err(err)?;
        let Ok(promise) = decoder.decode(&options) else {
            break;
        };
        // A frame the browser cannot decode ends the animation there.
        let Ok(result) = JsFuture::from(promise).await else {
            break;
        };
        let image = Reflect::get(&result, &"image".into()).map_err(err)?;
        let bitmap = crate::web::image::bitmap("emote frame", |o| {
            let fallback = web_sys::ImageBitmapOptions::new();
            let o = o.unwrap_or(&fallback);
            o.set_resize_width(w);
            o.set_resize_height(h);
            o.set_resize_quality(web_sys::ResizeQuality::High);
            create
                .call2(&window, &image, o)
                .and_then(|p| p.dyn_into::<Promise>())
        })
        .await;
        if let Ok(close) = Reflect::get(&image, &"close".into())
            .and_then(|f| f.dyn_into::<js_sys::Function>())
        {
            let _ = close.call0(&image);
        }
        let bitmap = bitmap?;
        let drawn = context.draw_image_with_image_bitmap(&bitmap, 0.0, f64::from(h * k as u32));
        bitmap.close();
        drawn.map_err(err)?;
        durations.push(frame.ms);
    }
    if durations.len() < 2 {
        return Ok(None);
    }
    let rows = h * durations.len() as u32;
    let data = context
        .get_image_data(0.0, 0.0, f64::from(w), f64::from(rows))
        .map_err(err)?;
    Ok(Some(Decoded {
        size: [w, h],
        rgba: data.data().0,
        durations,
    }))
}

/// TESTING ONLY (round TW5): `mpfakechatlook=<name>`, the chat drawn in
/// one of the looks shown to the maintainer
/// ([`ddi_chat::overlay::look_named`]); none, `chat-overlay-v1`'s own.
pub(crate) fn look(name: Option<&str>) {
    let look = name.and_then(ddi_chat::overlay::look_named);
    if let (Some(name), None) = (name, look) {
        web_sys::console::warn_1(&format!("mpfakechatlook={name}: as-built, larger or scattered").into());
    }
    ddi_shell::chat::set_look(look);
}

/// TESTING ONLY: `mpfakechat=<name or rate>` (`0` stops it), the scripted
/// chat beside every play (`ddi_session::chat::set_scripted`), its images
/// from `mpfakechatimg=<path>` (`<path><emote's id>.png`, an animated
/// one's `.gif`, [`ddi_chat::script::Script::images_at`]) where given.
pub(crate) fn script(what: &str, images: Option<&str>) {
    if what == "0" {
        ddi_session::chat::set_scripted(None);
        return;
    }
    let script = ddi_chat::script::Script::builtin(what).map(|mut s| {
        if let Some(base) = images {
            s.images_at(base);
        }
        s
    });
    if script.is_none() {
        web_sys::console::warn_1(&format!("mpfakechat={what}: no such script or rate").into());
    }
    ddi_session::chat::set_scripted(script);
}
