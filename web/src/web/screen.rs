//! The app's one canvas and GPU device (`docs/plans/ui-core.md`, P10),
//! made at boot and kept across routes.
//!
//! The canvas is a single element moved to whoever draws: the play screen
//! and the skin previews put it in their own place on the page
//! ([`attach`]), the 2D pass shows it behind the page ([`attach_backdrop`]),
//! and between them it is parked, hidden. Moving a canvas keeps its
//! context, so both backends keep one device: a WebGL2 context belongs to
//! its canvas, and a second canvas would need a second device.
//!
//! The backend is chosen once, at boot: `gfx=gl` in the address (the
//! route's parameters included) or the `graphics` setting forces WebGL2.
//! A route asking for WebGL2 later ([`require_gl`]) has the device made
//! again on it. A lost device or WebGL context is made again on a fresh
//! canvas in the old one's place; [`Gfx::is_lost`] tells a holder to stop
//! drawing, and [`on_lost`] listeners rebuild once the new one is ready.
//! The [`Painter`] (the 2D pass and its texture and text caches) survives
//! that: texture ids stay, textures are loaded again from their sources.

use std::cell::RefCell;
use std::rc::Rc;

use ddi_render::Painter;
use ddi_render::texture_cache::TextureId;
use gloo::events::EventListener;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Element, HtmlCanvasElement, HtmlElement, ImageBitmap};

use super::gfx::{BackendPreference, Gfx, replace_canvas};

enum State {
    /// The device is being made (at boot, after a loss, for another
    /// backend).
    Starting,
    Ready(Rc<Gfx>),
    /// No usable GPU: the reason.
    Failed(String),
}

struct Screen {
    canvas: HtmlCanvasElement,
    /// Where the canvas is kept between holders, and where it is shown
    /// behind the page (fixed, under everything).
    parking: HtmlElement,
    state: State,
    preference: BackendPreference,
    /// Devices made so far.
    generation: u64,
    /// Devices lost and made again.
    losses: u32,
    /// A device was lost and the one being made replaces it: the
    /// listeners hear of it once it is ready.
    replacing_lost: bool,
    /// The holder of the canvas ([`Attachment`]), 0 when parked.
    owner: u64,
    next_token: u64,
    /// Promises' `resolve` functions waiting for a device ([`gfx`]).
    waiting: Vec<js_sys::Function>,
    /// Called when a lost device has been made again.
    listeners: Vec<(u64, Rc<dyn Fn()>)>,
    /// The 2D pass, and the device it was made for.
    painter: Option<(u64, Painter)>,
    /// Images decoded for the painter's textures, not on the device yet.
    decoded: Vec<(TextureId, ImageBitmap)>,
    failed: Vec<TextureId>,
    /// Images copied to the device, closed after the next submit (WebGL2
    /// copies then).
    to_close: Vec<ImageBitmap>,
    /// `webglcontextlost` on the canvas.
    loss_hook: Option<EventListener>,
}

thread_local! {
    static SCREEN: RefCell<Option<Screen>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Screen) -> R) -> Option<R> {
    SCREEN.with(|s| s.borrow_mut().as_mut().map(f))
}

/// Makes the canvas and starts making the device. Call once, before the
/// first screen.
pub(crate) fn boot() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let (Ok(parking), Ok(canvas)) = (
        document.create_element("div"),
        document.create_element("canvas"),
    ) else {
        return;
    };
    let (Ok(parking), Ok(canvas)) = (
        parking.dyn_into::<HtmlElement>(),
        canvas.dyn_into::<HtmlCanvasElement>(),
    ) else {
        return;
    };
    parking.set_id("ddi-screen");
    let _ = parking.set_attribute("aria-hidden", "true");
    canvas.set_class_name("screen-canvas");
    let _ = parking.append_child(&canvas);
    if let Some(body) = document.body() {
        let _ = body.insert_before(&parking, body.first_child().as_ref());
    }
    super::text::load_bundled_font();
    SCREEN.with(|s| {
        *s.borrow_mut() = Some(Screen {
            canvas,
            parking,
            state: State::Starting,
            preference: boot_preference(),
            generation: 0,
            losses: 0,
            replacing_lost: false,
            owner: 0,
            next_token: 0,
            waiting: Vec::new(),
            listeners: Vec::new(),
            painter: None,
            decoded: Vec::new(),
            failed: Vec::new(),
            to_close: Vec::new(),
            loss_hook: None,
        })
    });
    install_test_hook();
    wasm_bindgen_futures::spawn_local(create());
}

/// WebGL2 when the address says `gfx=gl` (in its query or its route's)
/// or the setting does.
fn boot_preference() -> BackendPreference {
    let href = web_sys::window()
        .and_then(|w| w.location().href().ok())
        .unwrap_or_default();
    let asked = href.split(['?', '&', '#']).any(|p| p == "gfx=gl");
    if asked || crate::settings::Settings::load().graphics == crate::settings::Graphics::WebGl2 {
        BackendPreference::WebGl2
    } else {
        BackendPreference::Auto
    }
}

/// Makes the device on the canvas, with the backend preferred now.
async fn create() {
    let Some((canvas, preference)) = with(|s| {
        s.state = State::Starting;
        (s.canvas.clone(), s.preference)
    }) else {
        return;
    };
    let result = Gfx::new(canvas, preference).await;
    let done = with(|s| {
        match result {
            Ok((mut gfx, canvas)) => {
                s.generation += 1;
                gfx.generation = s.generation;
                // A failed WebGPU attempt swapped the canvas (in place).
                s.canvas = canvas;
                // WebGL2 asked for while WebGPU was being made: made again,
                // and whoever waits keeps waiting for that one.
                if s.preference == BackendPreference::WebGl2 && !gfx.is_gl() {
                    s.state = State::Ready(Rc::new(gfx));
                    return None;
                }
                watch(s, &gfx);
                s.state = State::Ready(Rc::new(gfx));
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("no graphics: {e}").into());
                s.state = State::Failed(e);
            }
        }
        let listeners: Vec<Rc<dyn Fn()>> = if std::mem::take(&mut s.replacing_lost) {
            s.listeners.iter().map(|(_, l)| l.clone()).collect()
        } else {
            Vec::new()
        };
        Some((std::mem::take(&mut s.waiting), listeners))
    });
    let Some(done) = done else {
        return;
    };
    let Some((waiting, listeners)) = done else {
        switch();
        return;
    };
    for resolve in waiting {
        let _ = resolve.call0(&JsValue::NULL);
    }
    for l in listeners {
        l();
    }
}

/// Watches for the device or context of `gfx` being lost.
fn watch(s: &mut Screen, gfx: &Gfx) {
    let generation = gfx.generation;
    // wgpu's default handler panics, and a lost WebGL2 context fails
    // calls (a surface configured, a texture made) before its event
    // arrives. Logged instead (the first few); a lost context found then
    // is handled later, outside whatever call failed.
    gfx.device
        .on_uncaptured_error(std::sync::Arc::new(move |error: wgpu::Error| {
            use std::sync::atomic::{AtomicU32, Ordering};
            static LOGGED: AtomicU32 = AtomicU32::new(0);
            if LOGGED.fetch_add(1, Ordering::Relaxed) < 20 {
                web_sys::console::error_1(&format!("wgpu: {error}").into());
            }
            wasm_bindgen_futures::spawn_local(async move { check_context(generation) });
        }));
    s.loss_hook = None;
    if gfx.is_gl() {
        s.loss_hook = Some(EventListener::new(
            &s.canvas,
            "webglcontextlost",
            move |_| {
                lost(generation, "the WebGL context was lost");
            },
        ));
    } else {
        gfx.device.set_device_lost_callback(move |reason, message| {
            lost(
                generation,
                &format!("the GPU device was lost ({reason:?}): {message}"),
            );
        });
    }
}

/// After a device error: a WebGL2 context found lost is a lost device
/// (its event may come later, or not at all).
fn check_context(generation: u64) {
    let context_lost = with(|s| {
        matches!(&s.state, State::Ready(g) if g.is_gl() && g.generation == generation)
            && s.canvas
                .get_context("webgl2")
                .ok()
                .flatten()
                .and_then(|c| c.dyn_into::<web_sys::WebGl2RenderingContext>().ok())
                .is_some_and(|c| c.is_context_lost())
    });
    if context_lost == Some(true) {
        lost(generation, "the WebGL context was lost");
    }
}

/// The device of `generation` is gone: a new one on a fresh canvas.
fn lost(generation: u64, why: &str) {
    let restart = with(|s| {
        let State::Ready(gfx) = &s.state else {
            return false;
        };
        if gfx.generation != generation {
            return false;
        }
        web_sys::console::warn_1(&format!("{why}; making it again").into());
        gfx.mark_lost();
        s.losses += 1;
        s.replacing_lost = true;
        fresh_canvas(s);
        true
    });
    if restart == Some(true) {
        wasm_bindgen_futures::spawn_local(create());
    }
}

/// A new canvas in the old one's place (a lost context may never come
/// back, and a canvas keeps the kind of context it had).
fn fresh_canvas(s: &mut Screen) {
    s.loss_hook = None;
    // wgpu never destroys a WebGPU device by itself (its drop does
    // nothing); its lost callback is ignored now (`lost`'s generation).
    if let State::Ready(gfx) = &s.state {
        gfx.mark_lost();
        if !gfx.is_gl() {
            gfx.device.destroy();
        }
    }
    if let Ok(fresh) = replace_canvas(&s.canvas) {
        s.canvas = fresh;
    }
    s.state = State::Starting;
}

/// Makes the device again on WebGL2.
fn switch() {
    let restart = with(|s| {
        fresh_canvas(s);
        true
    });
    if restart == Some(true) {
        wasm_bindgen_futures::spawn_local(create());
    }
}

/// The device on WebGL2 from now on (a route's `gfx=gl`). Call before
/// [`gfx`]; a device in use on WebGPU is made again.
pub(crate) fn require_gl() {
    let switching = with(|s| {
        if s.preference == BackendPreference::WebGl2 {
            return false;
        }
        s.preference = BackendPreference::WebGl2;
        // While starting, `create` sees the new preference when done.
        matches!(&s.state, State::Ready(gfx) if !gfx.is_gl())
    });
    if switching == Some(true) {
        switch();
    }
}

/// The device, once made (`Err`: the browser has no usable GPU).
pub(crate) async fn gfx() -> Result<Rc<Gfx>, String> {
    loop {
        let ready = with(|s| match &s.state {
            State::Ready(g) => Some(Ok(g.clone())),
            State::Failed(e) => Some(Err(e.clone())),
            State::Starting => None,
        });
        match ready {
            None => return Err("no screen".into()),
            Some(Some(result)) => return result,
            Some(None) => {}
        }
        let promise = js_sys::Promise::new(&mut |resolve, _| {
            with(|s| s.waiting.push(resolve));
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

/// The device if it is ready now.
pub(crate) fn current() -> Option<Rc<Gfx>> {
    with(|s| match &s.state {
        State::Ready(g) => Some(g.clone()),
        _ => None,
    })
    .flatten()
}

/// The canvas now (it is replaced when a device is made again).
pub(crate) fn canvas() -> Option<HtmlCanvasElement> {
    with(|s| s.canvas.clone())
}

/// Devices lost so far: a route keyed by it starts over on a new one.
pub(crate) fn losses() -> u32 {
    with(|s| s.losses).unwrap_or(0)
}

/// Holds the canvas until dropped; then it is parked again, unless
/// someone else took it meanwhile.
pub(crate) struct Attachment {
    token: u64,
}

impl Attachment {
    /// Still holding the canvas.
    pub(crate) fn holds(&self) -> bool {
        with(|s| s.owner == self.token).unwrap_or(false)
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        with(|s| {
            if s.owner == self.token {
                park(s);
            }
        });
    }
}

/// Moves the canvas into `host` (appended), with `class` as its class.
pub(crate) fn attach(host: &Element, class: &str) -> Option<Attachment> {
    with(|s| {
        let token = take(s, class);
        let _ = host.append_child(&s.canvas);
        Attachment { token }
    })
}

/// Shows the canvas behind the page, filling the window (the 2D pass
/// while Yew's pages last).
pub(crate) fn attach_backdrop() -> Option<Attachment> {
    with(|s| {
        let token = take(s, "screen-canvas");
        let _ = s.parking.class_list().add_1("shown");
        set_backdrop_class(true);
        Attachment { token }
    })
}

fn take(s: &mut Screen, class: &str) -> u64 {
    if s.owner != 0 {
        park(s);
    }
    s.next_token += 1;
    s.owner = s.next_token;
    s.canvas.set_class_name(class);
    let _ = s.canvas.remove_attribute("style");
    s.owner
}

fn park(s: &mut Screen) {
    s.owner = 0;
    let _ = s.parking.class_list().remove_1("shown");
    set_backdrop_class(false);
    s.canvas.set_class_name("screen-canvas");
    let _ = s.canvas.remove_attribute("style");
    let _ = s.parking.append_child(&s.canvas);
}

/// The page lets the canvas behind it show (`styles.css`).
fn set_backdrop_class(on: bool) {
    if let Some(root) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        let list = root.class_list();
        let _ = if on {
            list.add_1("screen-backdrop")
        } else {
            list.remove_1("screen-backdrop")
        };
    }
}

/// Calls `f` when a lost device has been made again; until the handle
/// is dropped.
pub(crate) fn on_lost(f: impl Fn() + 'static) -> Listener {
    let id = with(|s| {
        s.next_token += 1;
        s.listeners.push((s.next_token, Rc::new(f)));
        s.next_token
    })
    .unwrap_or(0);
    Listener { id }
}

pub(crate) struct Listener {
    id: u64,
}

impl Drop for Listener {
    fn drop(&mut self) {
        with(|s| s.listeners.retain(|(id, _)| *id != self.id));
    }
}

/// Runs `f` with the 2D pass for `gfx` (made, or rebuilt for a new
/// device, as needed); `None` when the surface has no plain view now
/// (WebGL2 not in [`super::gfx::SurfaceMode::Plain`]). `f` must not call
/// into this module.
pub(crate) fn with_painter<R>(gfx: &Gfx, f: impl FnOnce(&mut Painter) -> R) -> Option<R> {
    if gfx.is_lost() {
        return None;
    }
    let format = gfx.menu_format()?;
    with(|s| {
        let painter = match &mut s.painter {
            Some((generation, p)) if *generation == gfx.generation && p.format() == format => p,
            Some((generation, p)) => {
                p.device_changed(&gfx.device, &gfx.queue, format);
                // The way runs reach the pages depends on the backend.
                if let Ok(r) = super::text::CanvasRasterizer::new(gfx.is_gl()) {
                    p.set_rasterizer(Box::new(r));
                }
                *generation = gfx.generation;
                p
            }
            None => {
                let mut p = Painter::new(&gfx.device, &gfx.queue, format);
                match super::text::CanvasRasterizer::new(gfx.is_gl()) {
                    Ok(r) => p.set_rasterizer(Box::new(r)),
                    Err(e) => web_sys::console::warn_1(&format!("no text: {e}").into()),
                }
                &mut s.painter.insert((gfx.generation, p)).1
            }
        };
        f(painter)
    })
}

/// Before a frame of the 2D pass: asks the platform for the textures
/// drawn and not loaded, and puts those decoded since on the device.
pub(crate) fn prepare_textures(gfx: &Gfx) {
    // Without the 2D pass now, what was decoded waits for a frame with it.
    if gfx.is_lost() || gfx.menu_format().is_none() {
        return;
    }
    let Some((decoded, failed)) = with(|s| {
        (
            std::mem::take(&mut s.decoded),
            std::mem::take(&mut s.failed),
        )
    }) else {
        return;
    };
    let copied = with_painter(gfx, |p| {
        for id in failed {
            p.fail_texture(id);
        }
        let limit = p.max_texture_side();
        let mut copied = Vec::new();
        for (id, image) in decoded {
            let (w, h) = (image.width().max(1), image.height().max(1));
            if w.max(h) > limit {
                p.fail_texture(id);
                image.close();
                continue;
            }
            p.insert_texture(&gfx.device, id, [w, h], |texture| {
                gfx.queue.copy_external_image_to_texture(
                    &wgpu::CopyExternalImageSourceInfo {
                        source: wgpu::ExternalImageSource::ImageBitmap(image.clone()),
                        origin: wgpu::Origin2d::ZERO,
                        flip_y: false,
                    },
                    wgpu::CopyExternalImageDestInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                        color_space: wgpu::PredefinedColorSpace::Srgb,
                        premultiplied_alpha: false,
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
            });
            copied.push(image);
        }
        (copied, p.take_texture_requests(), limit)
    });
    let Some((copied, requests, limit)) = copied else {
        return;
    };
    with(|s| s.to_close.extend(copied));
    for (id, key) in requests {
        super::textures::load(id, key, limit);
    }
}

/// After a frame's submit: closes the images copied in it, and tells the
/// 2D pass its frame is over.
pub(crate) fn after_submit() {
    if let Some(images) = with(|s| {
        if let Some((_, p)) = &mut s.painter {
            p.submitted();
        }
        std::mem::take(&mut s.to_close)
    }) {
        for image in images {
            image.close();
        }
    }
}

/// A texture's image was decoded (`web::textures`).
pub(crate) fn texture_decoded(id: TextureId, result: Result<ImageBitmap, String>) {
    with(|s| match result {
        Ok(image) => s.decoded.push((id, image)),
        Err(e) => {
            web_sys::console::warn_1(&format!("texture {}: {e}", id.0).into());
            s.failed.push(id);
        }
    });
}

/// The screen's part of `window.__DDI_DEBUG`: `screen` (backend,
/// generation, losses, surface mode), `textures` and `text` (the 2D
/// pass's caches).
pub(crate) fn debug_into(obj: &js_sys::Object) {
    let set = |o: &js_sys::Object, k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(o, &JsValue::from_str(k), &v);
    };
    with(|s| {
        let screen = js_sys::Object::new();
        let (backend, mode) = match &s.state {
            State::Ready(g) => (g.backend_name(), format!("{:?}", g.mode())),
            State::Starting => ("starting", String::new()),
            State::Failed(_) => ("none", String::new()),
        };
        set(&screen, "backend", backend.into());
        set(&screen, "mode", mode.into());
        set(&screen, "generation", (s.generation as f64).into());
        set(&screen, "losses", s.losses.into());
        set(&screen, "held", (s.owner != 0).into());
        set(obj, "screen", screen.into());
        if let Some((_, p)) = &s.painter {
            let stats = p.stats();
            let t = stats.textures;
            let textures = js_sys::Object::new();
            set(&textures, "bytes", (t.bytes as f64).into());
            set(&textures, "budget", (t.budget as f64).into());
            set(&textures, "entries", (t.entries as f64).into());
            set(&textures, "loaded", (t.loaded as f64).into());
            set(&textures, "loading", (t.loading as f64).into());
            set(&textures, "failed", (t.failed as f64).into());
            set(&textures, "evictions", (t.evictions as f64).into());
            set(&textures, "loads", (t.loads as f64).into());
            set(obj, "textures", textures.into());
            let x = stats.text;
            let text = js_sys::Object::new();
            set(&text, "pages", (x.pages as f64).into());
            set(&text, "bytes", (x.bytes as f64).into());
            set(&text, "budget", (x.budget as f64).into());
            set(&text, "runs", (x.runs as f64).into());
            set(&text, "drawn", (x.drawn as f64).into());
            set(&text, "evictions", (x.evictions as f64).into());
            set(&text, "uploads", (x.uploads as f64).into());
            set(obj, "text", text.into());
            let pass = js_sys::Object::new();
            set(&pass, "instances", (stats.instances as f64).into());
            set(&pass, "draws", (stats.draws as f64).into());
            set(&pass, "passes", (stats.passes as f64).into());
            set(obj, "pass2d", pass.into());
        }
    });
}

/// `window.__DDI_LOSE_DEVICE()`: destroys the device, as a lost one
/// (testing; on WebGL2 the context is lost through `WEBGL_lose_context`).
fn install_test_hook() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let hook = Closure::<dyn Fn()>::new(|| {
        let Some(gfx) = current() else {
            return;
        };
        if gfx.is_gl() {
            // WebGL2: the context lost as a browser loses it (wgpu's
            // destroy does not lose it).
            let ext = canvas()
                .and_then(|c| c.get_context("webgl2").ok().flatten())
                .and_then(|c| c.dyn_into::<web_sys::WebGl2RenderingContext>().ok())
                .and_then(|c| c.get_extension("WEBGL_lose_context").ok().flatten());
            if let Some(ext) = ext
                && let Ok(f) = js_sys::Reflect::get(&ext, &"loseContext".into())
                && let Ok(f) = f.dyn_into::<js_sys::Function>()
            {
                let _ = f.call0(&ext);
            }
        } else {
            gfx.device.destroy();
        }
    });
    let _ = js_sys::Reflect::set(&window, &"__DDI_LOSE_DEVICE".into(), hook.as_ref());
    hook.forget();
}
