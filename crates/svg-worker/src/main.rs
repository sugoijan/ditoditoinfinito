//! The web's SVG rasterizer for noteskins (`docs/plans/noteskin-svg.md`,
//! step 1): a worker drawing a skin's SVG images with the code desktop
//! draws them with (`ddi_noteskin::raster`), so they look the same on both
//! shells. The page starts a few while a skin loads and ends them when it
//! is done (`crates/web/src/web/svg.rs`).
//!
//! Protocol (the page's boot script loads this module from the compiled
//! `WebAssembly.Module` the page sends): the worker says `{ready: true}`
//! once it listens; the page sends `{id, text, width, height}`, one at a
//! time; the worker answers `{id, bitmap: ImageBitmap, hash, ms}`
//! (transferred; its colors not premultiplied, as every image of the web
//! shell's) or `{id, pixels: ArrayBuffer, hash, ms}` (the RGBA rows)
//! where it cannot make bitmaps or the page sent `raw: true`, or `{id,
//! error}`. `ms` is the time drawing took; `hash` the pixels'
//! (`ddi_noteskin::report::pixels_hash`, 16 hex digits), by which the
//! page keeps images of the same pixels once.

#[cfg(target_arch = "wasm32")]
fn main() {
    use js_sys::{Array, Object, Reflect, Uint8Array};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{
        DedicatedWorkerGlobalScope, ImageBitmapOptions, ImageData, MessageEvent, PremultiplyAlpha,
    };

    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let set = |o: &Object, k: &str, v: &JsValue| {
        let _ = Reflect::set(o, &JsValue::from_str(k), v);
    };
    let reply = scope.clone();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let data = event.data();
        let get = |k: &str| Reflect::get(&data, &JsValue::from_str(k)).ok();
        let id = get("id").unwrap_or(JsValue::UNDEFINED);
        let text = get("text").and_then(|t| t.as_string());
        let size = |k: &str| get(k).and_then(|v| v.as_f64()).map(|v| v as u32);
        let (width, height) = (size("width").unwrap_or(0), size("height").unwrap_or(0));
        let raw = get("raw").is_some_and(|v| v.is_truthy());
        let reply = reply.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let msg = Object::new();
            set(&msg, "id", &id);
            let transfer = Array::new();
            let began = js_sys::Date::now();
            let drawn = text
                .ok_or_else(|| "no SVG was sent".to_string())
                .and_then(|t| {
                    if width == 0 || height == 0 {
                        return Err("no size was sent".into());
                    }
                    ddi_noteskin::svg::check(&t)?;
                    ddi_noteskin::raster::rasterize(&t, width, height)
                });
            set(&msg, "ms", &JsValue::from(js_sys::Date::now() - began));
            match drawn {
                Ok(pixels) => {
                    let hash = ddi_noteskin::report::pixels_hash([width, height], &pixels);
                    set(&msg, "hash", &JsValue::from_str(&format!("{hash:016x}")));
                    let array = Uint8Array::from(&pixels[..]);
                    drop(pixels);
                    let bitmap = async {
                        if raw {
                            return None;
                        }
                        let clamped = js_sys::Uint8ClampedArray::new(&array.buffer());
                        let image =
                            ImageData::new_with_js_u8_clamped_array_and_sh(&clamped, width, height)
                                .ok()?;
                        let options = ImageBitmapOptions::new();
                        options.set_premultiply_alpha(PremultiplyAlpha::None);
                        let promise = reply
                            .create_image_bitmap_with_image_data_and_image_bitmap_options(
                                &image, &options,
                            )
                            .ok()?;
                        JsFuture::from(promise).await.ok()
                    };
                    match bitmap.await {
                        Some(bitmap) => {
                            set(&msg, "bitmap", &bitmap);
                            transfer.push(&bitmap);
                        }
                        None => {
                            let buffer = array.buffer();
                            set(&msg, "pixels", &buffer);
                            transfer.push(&buffer);
                        }
                    }
                }
                Err(e) => set(&msg, "error", &JsValue::from_str(&e)),
            }
            let _ = reply.post_message_with_transfer(&msg, &transfer);
        });
    });
    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    let ready = Object::new();
    set(&ready, "ready", &JsValue::TRUE);
    let _ = scope.post_message(&ready);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
