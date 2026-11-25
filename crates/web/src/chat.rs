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
//! The scripted chat's images are its own shapes (drawn by
//! `ddi_shell::chat`), or, with `mpfakechatimg=<path>`, files beside the
//! page that a test serves. One script reaches the providers: the
//! global-emotes demo (`mpfakechat=global`, round TW5, TESTING ONLY,
//! `ddi_chat::global`), which the maintainer runs: their global sets read
//! here (directly, then through our server's proxy, Twitch's through our
//! server alone), their images as a real chat's.
//!
//! The live chat (round TW6, [`WebLive`]): the play's
//! `ddi_session::chat::LiveIo` on the web: each socket to Twitch's EventSub
//! the browser's `WebSocket` (Twitch's takes no credentials; the browser
//! answers its pings), our server's subscribe route through `fetch` with
//! the page's cookie, the providers' sets as the images are read, and the
//! run's lines on the console (`ddi_chat::live::LOG_PREFIX`), its report in
//! `__DDI_DEBUG.chat.live`. `mpfakechat=live` runs it in solo plays and
//! CPU practice's stages for the signed-in person's own channel (a
//! player's are multiplayer stages alone); `mpfakechaturl=<ws address>`
//! points the socket at a local fake (TESTING ONLY).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ddi_session::chat::{LiveInput, LiveIo, LiveSetup, LiveWant};

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
    /// The global-emotes demo's reads (round TW5).
    static GLOBAL: RefCell<Option<ddi_chat::global::Demo>> = const { RefCell::new(None) };
}

/// What each provider answered the global-emotes demo (null without it):
/// `__DDI_DEBUG.chat.global`.
pub(crate) fn global_report() -> Option<serde_json::Value> {
    GLOBAL.with(|g| g.borrow().as_ref().map(ddi_chat::global::Demo::report))
}

/// TESTING ONLY (round TW5): the global-emotes demo. Its chat plays at
/// once (its words alone) and again with the emotes of each set as it is
/// read; every provider's address served from `at` instead where given
/// (`mpfakechatimg`: a local folder of fixtures, never a provider).
fn global(at: Option<&str>) {
    let settings = crate::settings::Settings::load();
    // Round HY (2026-11-25): only where this device shows Twitch chat
    // (Options → Display), as the privacy note says the providers are
    // asked; else a link carrying `mpfakechat=global` would have any
    // visitor's browser ask them at once.
    if settings.twitch_chat == ddi_chat::Level::Off {
        web_sys::console::warn_1(
            &"mpfakechat=global: Options → Display → Twitch chat in rooms is Off; not taken".into(),
        );
        return;
    }
    let from = settings.chat_emotes_from;
    let demo = ddi_chat::global::Demo::new(from, Some(crate::share::base()), at.map(str::to_string));
    ddi_session::chat::set_scripted(Some(demo.script()));
    GLOBAL.with(|g| *g.borrow_mut() = Some(demo));
    wasm_bindgen_futures::spawn_local(async {
        loop {
            let reads = GLOBAL.with(|g| g.borrow_mut().as_mut().map(ddi_chat::global::Demo::start));
            let Some(reads) = reads.filter(|r| !r.is_empty()) else {
                break;
            };
            for r in reads {
                let answer = read(&r.url)
                    .await
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
                let line = ddi_chat::global::answer_line(&r, &answer);
                web_sys::console::info_1(&line.into());
                GLOBAL.with(|g| {
                    if let Some(d) = g.borrow_mut().as_mut() {
                        d.answered(r.provider, r.via, answer);
                    }
                });
            }
            if let Some(script) = GLOBAL.with(|g| g.borrow().as_ref().map(ddi_chat::global::Demo::script)) {
                ddi_session::chat::set_scripted(Some(script));
            }
        }
    });
}

/// The live chat's side on this page (round TW6), from boot: Twitch's
/// EventSub socket, or `socket` (TESTING ONLY, `mpfakechaturl`); the
/// providers' addresses from `at` where a test says (`mpfakechatimg`).
/// Whether a play shows it is the settings' and the room's: for a player
/// only an online room's stage (round TW7).
pub(crate) fn install_live(socket: Option<&str>, at: Option<&str>) {
    let api = crate::share::base();
    ddi_session::chat::set_live(Some(LiveSetup {
        url: socket.map_or_else(|| ddi_chat::live::EVENTSUB_URL.to_string(), str::to_string),
        api,
        at: at.map(str::to_string),
        make: Box::new(|| Box::new(WebLive::default())),
    }));
}

/// TESTING ONLY (round TW6), `mpfakechat=live`: the live chat in solo
/// plays and CPU practice's stages for the signed-in person's own
/// channel, once `/me` says who that is
/// (`ddi_ui::state::twitch::set_dev_live`).
fn live(at: Option<&str>, socket: Option<&str>) {
    ddi_session::chat::set_scripted(None);
    install_live(socket, at);
    wasm_bindgen_futures::spawn_local(async {
        let p = ddi_chat::live::LOG_PREFIX;
        let me = crate::online::send(&ddi_ui::state::online::api::me()).await;
        let parsed = (me.status == 200)
            .then(|| serde_json::from_str::<ddi_ui::state::online::api::Me>(&me.body).ok())
            .flatten();
        let line = match parsed {
            Some(m) => {
                match m.own_channel() {
                    Some((id, login)) => {
                        let scope = m.twitch.has("chat");
                        ddi_ui::state::twitch::set_dev_live(Some(ddi_ui::state::twitch::DevLive::Own {
                            channel_id: id,
                            login: login.clone(),
                        }));
                        format!(
                            "{p} live chat: {login}'s own channel in solo plays and CPU practice's stages; turn on Options → Display → Twitch chat in rooms{}",
                            if scope {
                                ""
                            } else {
                                " (your Twitch access lacks the chat's scope, user:read:chat: turning it on asks for it)"
                            }
                        )
                    }
                    None => format!("{p} live chat: signed in without a Twitch account"),
                }
            }
            None => format!(
                "{p} live chat: not signed in (HTTP {}): sign in first (Multiplayer)",
                me.status
            ),
        };
        web_sys::console::info_1(&line.into());
    });
}

/// TESTING ONLY (round TW7), `mpfakechat=live:<login>`: the live chat of
/// the channel `login` names, for the maintainer to test with a busy
/// channel: in solo plays, CPU practice's stages and, in the room's
/// channel's place on this device alone, an online room's stages. Our
/// server reads it with the signed-in person's own Twitch access, for
/// administrators only.
fn named(login: &str, at: Option<&str>, socket: Option<&str>) {
    let p = ddi_chat::live::LOG_PREFIX;
    ddi_session::chat::set_scripted(None);
    install_live(socket, at);
    let line = match ddi_ui::state::twitch::named_login(login) {
        Some(login) => {
            let line = format!(
                "{p} live chat: {login}'s channel (the development override; our server allows it for administrators only) in solo plays, CPU practice's stages and an online room's stages on this device; turn on Options → Display → Twitch chat in rooms"
            );
            ddi_ui::state::twitch::set_dev_live(Some(ddi_ui::state::twitch::DevLive::Channel { login }));
            line
        }
        None => format!("{p} mpfakechat=live:<login> takes a Twitch login (letters, digits, underscores)"),
    };
    web_sys::console::info_1(&line.into());
}

/// A live socket: the browser's `WebSocket` and its handlers.
struct LiveSocket {
    ws: web_sys::WebSocket,
    _open: Closure<dyn FnMut(web_sys::Event)>,
    _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _close: Closure<dyn FnMut(web_sys::CloseEvent)>,
}

impl Drop for LiveSocket {
    fn drop(&mut self) {
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

/// The live chat's side on the web (see the module's notes): it sends
/// through the page's own requests (`crate::online::send`, the cookie).
#[derive(Default)]
pub(crate) struct WebLive {
    queue: Rc<RefCell<Vec<LiveInput>>>,
    sockets: HashMap<u32, LiveSocket>,
}

impl LiveIo for WebLive {
    fn open(&mut self, socket: u32, url: &str) {
        let ws = match web_sys::WebSocket::new(url) {
            Ok(ws) => ws,
            Err(_) => {
                self.queue.borrow_mut().push(LiveInput::Closed(socket, 1006));
                return;
            }
        };
        let q = self.queue.clone();
        let open = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            q.borrow_mut().push(LiveInput::Opened(socket));
        });
        let q = self.queue.clone();
        let message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            if let Some(text) = e.data().as_string() {
                q.borrow_mut().push(LiveInput::Text(socket, text));
            }
        });
        let q = self.queue.clone();
        let close = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |e: web_sys::CloseEvent| {
            q.borrow_mut().push(LiveInput::Closed(socket, e.code()));
        });
        ws.set_onopen(Some(open.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
        ws.set_onclose(Some(close.as_ref().unchecked_ref()));
        self.sockets.insert(
            socket,
            LiveSocket {
                ws,
                _open: open,
                _message: message,
                _close: close,
            },
        );
    }

    fn close(&mut self, socket: u32) {
        self.sockets.remove(&socket);
    }

    fn subscribe(&mut self, socket: u32, session_id: &str, want: &LiveWant, messages: bool) {
        let q = self.queue.clone();
        let request = ddi_ui::state::online::api::ApiRequest {
            method: ddi_ui::state::online::api::Method::Post,
            path: want.route(),
            body: Some(want.body(session_id, messages)),
            bearer: None,
            poll: None,
        };
        wasm_bindgen_futures::spawn_local(async move {
            let r = crate::online::send(&request).await;
            let answer = ddi_chat::live::read_subscribe_answer(r.status, &r.body);
            q.borrow_mut().push(LiveInput::Subscribed(socket, answer));
        });
    }

    fn read_set(&mut self, key: usize, url: &str, via: ddi_chat::cache::Via) {
        let q = self.queue.clone();
        let url = url.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let answer = read(&url)
                .await
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
            q.borrow_mut().push(LiveInput::SetRead(key, via, answer));
        });
    }

    fn poll(&mut self) -> Vec<LiveInput> {
        let out = std::mem::take(&mut *self.queue.borrow_mut());
        for i in &out {
            if let LiveInput::Closed(n, _) = i {
                self.sockets.remove(n);
            }
        }
        out
    }

    fn report(&mut self, line: &str) {
        web_sys::console::info_1(&format!("{} {line}", ddi_chat::live::LOG_PREFIX).into());
    }
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

/// How long an emote's image or a provider's set may take, answer and
/// body, milliseconds (review 2026-11-22, chat-touch 4: a request that
/// hangs on a network that drops a provider's traffic held one of the
/// cache's four places for the browser's own timeout, minutes; desktop's
/// reads have their own bounds). An emote of 64 pixels or a set's list
/// takes well under a second on a working link.
const READ_WAIT_MS: u32 = 8_000;

/// The bytes at `url`, at most
/// [`anim::MOST_BYTES`]: refused from the answer's length where it says
/// more, else read piece by piece and stopped once past it, so an answer
/// larger than an emote is never held whole; given up after
/// [`READ_WAIT_MS`] (a failure: tried through the proxy where it may be).
async fn read(url: &str) -> Result<Vec<u8>, String> {
    let too_large = || anim::Refusal::TooLarge.to_string();
    let abort = web_sys::AbortController::new().map_err(|e| format!("{e:?}"))?;
    let signal = abort.signal();
    // (Dropped with this read: canceled once it ends.)
    let _deadline = gloo::timers::callback::Timeout::new(READ_WAIT_MS, move || abort.abort());
    let resp = gloo::net::http::Request::get(url)
        .abort_signal(Some(&signal))
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
    let reader = crate::web::files::body_reader(&body)?;
    let mut out = Vec::new();
    let read = crate::web::files::each_chunk(&reader, |piece| {
        if out.len() + piece.length() as usize > anim::MOST_BYTES {
            return std::ops::ControlFlow::Break(());
        }
        out.extend(piece.to_vec());
        std::ops::ControlFlow::Continue(())
    })
    .await?;
    if read.is_break() {
        let _ = reader.cancel();
        return Err(too_large());
    }
    Ok(out)
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
/// ([`ddi_chat::overlay::look_named`]); none, `chat-overlay-v2`'s own.
pub(crate) fn look(name: Option<&str>) {
    let look = name.and_then(ddi_chat::overlay::look_named);
    if let (Some(name), None) = (name, look) {
        web_sys::console::warn_1(&format!("mpfakechatlook={name}: as-built, larger or scattered").into());
    }
    ddi_shell::chat::set_look(look);
}

/// TESTING ONLY (round DE): `mpfakechatrules=v1`, the chat laid by the
/// retired `chat-overlay-v1` for the frame-cost comparison
/// ([`ddi_shell::chat::rules_named`]); none or `v2`, `chat-overlay-v2`.
pub(crate) fn rules(name: Option<&str>) {
    let rules = name.and_then(ddi_shell::chat::rules_named);
    if let (Some(name), None) = (name, rules) {
        web_sys::console::warn_1(&format!("mpfakechatrules={name}: v1 or v2").into());
    }
    ddi_shell::chat::set_rules(rules);
}

/// TESTING ONLY: `mpfakechat=<name or rate>` (`0` stops it; `global` the
/// global-emotes demo, its addresses from `mpfakechatimg` where given), the scripted
/// chat beside every play (`ddi_session::chat::set_scripted`), its images
/// from `mpfakechatimg=<path>` (`<path><emote's id>.png`, an animated
/// one's `.gif`, [`ddi_chat::script::Script::images_at`]) where given.
pub(crate) fn script(what: &str, images: Option<&str>, socket: Option<&str>) {
    // Only this machine's addresses, or the page's own for pictures: a
    // link can never point a player's chat at another server (review
    // 2026-11-22, chat-touch 3), whatever route or session carries it.
    let origin = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default();
    let images = images.filter(|base| {
        let ok = crate::route::local_base(base, &origin);
        if !ok {
            web_sys::console::warn_1(&format!("mpfakechatimg={base}: not beside the page nor on this machine; not taken").into());
        }
        ok
    });
    let socket = socket.filter(|url| {
        let ok = crate::route::local_socket(url);
        if !ok {
            web_sys::console::warn_1(&format!("mpfakechaturl={url}: not a socket on this machine; not taken").into());
        }
        ok
    });
    if what == "0" {
        // No script: a player's live chat, its socket and the providers'
        // addresses where a test points them at local fakes (round TW7:
        // `mpfakechat=0&mpfakechaturl=…`, a player's room walked without
        // reaching Twitch).
        ddi_session::chat::set_scripted(None);
        ddi_ui::state::twitch::set_dev_live(None);
        install_live(socket, images);
        return;
    }
    if what == "live" {
        live(images, socket);
        return;
    }
    if let Some(login) = what.strip_prefix("live:") {
        named(login, images, socket);
        return;
    }
    if what == "global" {
        global(images);
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
