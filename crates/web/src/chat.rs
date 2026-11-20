//! A room host's chat on the web (`docs/plans/multiplayer-twitch.md`,
//! part C): its emote images fetched and decoded off the frame by the
//! browser (`fetch`, `createImageBitmap`), drawn to the atlas's row on a
//! canvas and read back as RGBA (an image of 64 pixels at most).
//!
//! No request reaches Twitch or an emote provider in this round: the
//! scripted chat's images are its own shapes (drawn by `ddi_shell::chat`),
//! or, with `mpfakechatimg=<path>`, files beside the page that a test
//! serves.

use std::cell::RefCell;

use ddi_chat::cache::Fetch;
use ddi_shell::chat::{Decoded, decode_size};
use wasm_bindgen::JsCast;

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
    let resp = gloo::net::http::Request::get(url)
        .send()
        .await
        .map_err(|e| format!("GET: {e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let raw: web_sys::Response = resp.into();
    let blob: web_sys::Blob = wasm_bindgen_futures::JsFuture::from(
        raw.blob().map_err(|e| format!("{e:?}"))?,
    )
    .await
    .map_err(|e| format!("{e:?}"))?
    .dyn_into()
    .map_err(|_| "not a blob")?;
    let (bitmap, (w, h)) = crate::web::image::decode_sized(&blob, 1024).await?;
    let [fw, fh] = decode_size([w, h]);
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
    Ok(Decoded {
        size: [fw, fh],
        rgba: data.data().0,
    })
}

/// TESTING ONLY: `mpfakechat=<name or rate>` (`0` stops it), the scripted
/// chat beside every play (`ddi_session::chat::set_scripted`), its images
/// from `mpfakechatimg=<path>` (`<path><emote's id>.png`) where given.
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
