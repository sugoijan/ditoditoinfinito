//! A noteskin's elements alone on a canvas, as a play draws them: the
//! page `#/skin-preview` (checking a skin against the renders its author
//! published) and the preview under the Arrows choice in Options, both a
//! [`PreviewCanvas`].
//!
//! The page's parameters: `skin` (the setting's key, `bundled:dance/cf_chrome`
//! or an import's `dance/<name>`; `builtin` for the built-in arrows),
//! `size` (one arrow's pixels, default 128), `bg` (hex colour, default
//! `000000`), `bpm` (default 60), `beat` (held still at that beat;
//! otherwise the beat runs from 0), `notes=4` (every lane's tap note rather
//! than Down's), `receptors=1`, `gfx=gl`, `demo=1` (a looping autoplayed
//! chart scrolling into the receptors: taps hit in every judgement's
//! window and a miss, holds and rolls kept and dropped, a mine; `rows` arrows tall, default 5, `speed` its x-mod,
//! default 1.5), `layout` (a built-in layout's id, default
//! `dance-single`; a skin draws Dancing☆Onigiri's onigiri with its own
//! onigiri or centre elements, else its Down's: `docs/plans/danoni-skins.md`). A test may set `window.__DDI_PREVIEW_BEAT`; after drawing it the
//! page sets `window.__DDI_PREVIEW_DRAWN` to that beat.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_chart::{Layout, Quantization};
use ddi_engine::appearance::Appearance;
use ddi_engine::demo::Demo;
use ddi_engine::frame::{Frame, NoteSprite, ReceptorState, SpriteKind};
use ddi_engine::rules::JudgeNames;
use ddi_render::noteskin::NoteskinPlay;
use ddi_render::{ColorScheme, FieldGeometry, Preview, RenderOptions, Renderer, field_span};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use yew::prelude::*;

use crate::web::gfx::{Gfx, SurfaceMode};

/// What a preview shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreviewOptions {
    /// A `noteskin` setting's key; [`crate::noteskins::BUILT_IN`] for the
    /// built-in arrows.
    pub(crate) skin: String,
    /// A built-in layout's id.
    pub(crate) layout: String,
    /// One arrow's pixels.
    pub(crate) size: u32,
    pub(crate) background: [f64; 3],
    pub(crate) bpm: f64,
    /// Held still at this beat.
    pub(crate) beat: Option<f64>,
    /// Every lane, not only Down's (the first's outside dance).
    pub(crate) all_lanes: bool,
    pub(crate) receptors: bool,
    pub(crate) force_gl: bool,
    /// The demo chart scrolling into the receptors.
    pub(crate) demo: bool,
    /// Arrows tall, with the demo.
    pub(crate) rows: u32,
    /// The demo's x-mod.
    pub(crate) speed: f64,
    /// The built-in arrows' colours.
    pub(crate) colors: ColorScheme,
    /// Canvas pixels per CSS pixel: the canvas is shown `1 / pixel_ratio`
    /// as large (sharp on a high-density screen).
    pub(crate) pixel_ratio: f64,
}

impl Default for PreviewOptions {
    fn default() -> PreviewOptions {
        PreviewOptions {
            skin: crate::noteskins::BUILT_IN.into(),
            layout: "dance-single".into(),
            size: 128,
            background: [0.0; 3],
            bpm: 60.0,
            beat: None,
            all_lanes: false,
            receptors: false,
            force_gl: false,
            demo: false,
            rows: 5,
            speed: 1.5,
            colors: ColorScheme::default(),
            pixel_ratio: 1.0,
        }
    }
}

impl PreviewOptions {
    /// The page's URL parameters.
    pub(crate) fn parse(params: &[(String, String)]) -> PreviewOptions {
        let get = |k: &str| params.iter().find(|(p, _)| p == k).map(|(_, v)| v.as_str());
        let hex = get("bg").unwrap_or("000000").trim_start_matches('#');
        let channel = |i: usize| {
            hex.get(i..i + 2)
                .and_then(|c| u8::from_str_radix(c, 16).ok())
                .map_or(0.0, |c| f64::from(c) / 255.0)
        };
        let demo = get("demo") == Some("1");
        let d = PreviewOptions::default();
        PreviewOptions {
            skin: get("skin").unwrap_or(&d.skin).to_string(),
            layout: get("layout")
                .filter(|id| Layout::builtin(id).is_some())
                .unwrap_or(&d.layout)
                .to_string(),
            size: get("size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(d.size)
                .clamp(16, 512),
            background: [channel(0), channel(2), channel(4)],
            bpm: get("bpm")
                .and_then(|s| s.parse().ok())
                .filter(|b: &f64| *b > 0.0 && *b <= 1000.0)
                .unwrap_or(d.bpm),
            beat: get("beat").and_then(|s| s.parse().ok()),
            all_lanes: get("notes") == Some("4") || demo,
            receptors: get("receptors") == Some("1") || demo,
            force_gl: get("gfx") == Some("gl"),
            demo,
            rows: get("rows")
                .and_then(|s| s.parse().ok())
                .unwrap_or(d.rows)
                .clamp(2, 16),
            speed: get("speed")
                .and_then(|s| s.parse().ok())
                .filter(|s: &f64| *s > 0.0 && *s <= 8.0)
                .unwrap_or(d.speed),
            colors: d.colors,
            pixel_ratio: d.pixel_ratio,
        }
    }

    fn layout(&self) -> Layout {
        Layout::builtin(&self.layout).unwrap_or_else(Layout::dance_single)
    }

    /// The lane drawn alone: Down in dance, else the first.
    fn single_lane(layout: &Layout) -> usize {
        layout
            .lanes
            .iter()
            .position(|l| l.name.eq_ignore_ascii_case("down"))
            .unwrap_or(0)
    }

    /// The canvas's pixels.
    fn canvas_size(&self, layout: &Layout) -> (u32, u32) {
        let lanes = if self.all_lanes {
            field_span(layout).ceil().max(1.0) as u32
        } else {
            1
        };
        let rows = if self.demo { self.rows } else { 1 };
        (self.size * lanes, self.size * rows)
    }

    /// One arrow a lane of `size` pixels; still notes on their receptors in
    /// the middle row, the demo's coming up from below.
    fn geometry(&self, layout: &Layout) -> FieldGeometry {
        let a = self.size as f32;
        let (w, h) = self.canvas_size(layout);
        let left = if self.all_lanes {
            0.0
        } else {
            let lane = Self::single_lane(layout);
            -a * layout.lanes.get(lane).map_or(0.0, |l| l.column)
        };
        FieldGeometry {
            arrow: a,
            left,
            receptor_y: if self.demo { a * 0.75 } else { a / 2.0 },
            direction: 1.0,
            width: w as f32,
            height: h as f32,
        }
    }
}

/// The page.
#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) params: Vec<(String, String)>,
}

#[function_component(SkinPreview)]
pub(crate) fn skin_preview(props: &Props) -> Html {
    let options = PreviewOptions::parse(&props.params);
    html! {
        <main class="skin-preview">
            <PreviewCanvas {options} />
        </main>
    }
}

#[derive(Properties, PartialEq)]
pub(crate) struct CanvasProps {
    pub(crate) options: PreviewOptions,
}

/// The app's device (`web::screen`) and a renderer on it, kept while the
/// skin shown changes; made again when the device is.
struct Device {
    gfx: Rc<Gfx>,
    renderer: Renderer,
}

/// What is shown.
struct Shown {
    play: Option<NoteskinPlay>,
    images: Vec<web_sys::ImageBitmap>,
    options: PreviewOptions,
    layout: Layout,
    /// When the animation's beat 0 was, and the last frame's time (a
    /// longer gap, a hidden page, counts as a pause).
    start_ms: Option<f64>,
    last_ms: f64,
    /// With the demo: the chart playing, and the last time it was
    /// stepped to.
    demo: Option<(Demo, f64)>,
}

impl Shown {
    fn close(self) {
        for image in self.images {
            image.close();
        }
    }
}

#[derive(Default)]
struct State {
    canvas: Option<HtmlCanvasElement>,
    /// The app's canvas, while this preview holds it.
    attachment: Option<crate::web::screen::Attachment>,
    device: Option<Device>,
    /// A new device is being taken up (the last one was lost).
    rebuilding: bool,
    /// None could be: the preview stops.
    failed: bool,
    shown: Option<Shown>,
    /// The options last asked for, and their number: a load of older
    /// options stops when it sees a newer one.
    wanted: Option<PreviewOptions>,
    generation: u64,
    closed: bool,
    status: String,
}

pub(crate) struct PreviewCanvas {
    /// Where the app's canvas is put.
    slot: NodeRef,
    state: Rc<RefCell<State>>,
    /// The animation frame callback, kept alive.
    tick: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>,
    /// The pending animation frame request, cancelled before the callback
    /// is dropped.
    frame: Rc<Cell<i32>>,
}

pub(crate) enum Msg {
    Status,
}

impl Component for PreviewCanvas {
    type Message = Msg;
    type Properties = CanvasProps;

    fn create(ctx: &Context<Self>) -> Self {
        let state = State {
            wanted: Some(ctx.props().options.clone()),
            status: "loading…".into(),
            ..State::default()
        };
        PreviewCanvas {
            slot: NodeRef::default(),
            state: Rc::new(RefCell::new(state)),
            tick: Rc::new(RefCell::new(None)),
            frame: Rc::new(Cell::new(0)),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Msg) -> bool {
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old: &CanvasProps) -> bool {
        let options = ctx.props().options.clone();
        if options == old.options {
            return false;
        }
        let generation = {
            let mut st = self.state.borrow_mut();
            st.wanted = Some(options.clone());
            st.generation += 1;
            st.status = "loading…".into();
            // Without a device yet, the setup loads the wanted options.
            st.device.is_some().then_some(st.generation)
        };
        if let Some(generation) = generation {
            let state = self.state.clone();
            let link = ctx.link().clone();
            wasm_bindgen_futures::spawn_local(async move {
                load(&state, options, generation).await;
                link.send_message(Msg::Status);
            });
        }
        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            return;
        }
        let options = ctx.props().options.clone();
        let Some(attachment) = self
            .slot
            .cast::<web_sys::Element>()
            .and_then(|slot| crate::web::screen::attach(&slot, "skin-preview-gpu"))
        else {
            return;
        };
        let Some(canvas) = crate::web::screen::canvas() else {
            return;
        };
        size_canvas(&canvas, &options);
        {
            let mut st = self.state.borrow_mut();
            st.canvas = Some(canvas);
            st.attachment = Some(attachment);
        }
        if options.force_gl {
            crate::web::screen::require_gl();
        }
        let state = self.state.clone();
        let tick = self.tick.clone();
        let frame = self.frame.clone();
        let link = ctx.link().clone();
        wasm_bindgen_futures::spawn_local(async move {
            if !take_device(&state, &link).await {
                return;
            }
            let (options, generation) = {
                let st = state.borrow();
                (st.wanted.clone().unwrap_or(options), st.generation)
            };
            load(&state, options, generation).await;
            // Closed meanwhile: no loop to start (it would keep the device).
            if state.borrow().closed {
                return;
            }
            link.send_message(Msg::Status);
            start(state, tick, frame, link);
        });
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        let mut st = self.state.borrow_mut();
        st.closed = true;
        if let Some(w) = web_sys::window() {
            let _ = w.cancel_animation_frame(self.frame.get());
        }
        self.tick.borrow_mut().take();
        if let Some(s) = st.shown.take() {
            s.close();
        }
        // The renderer goes now, not when a pending setup ends; the canvas
        // is parked again.
        st.device = None;
        st.canvas = None;
        st.attachment = None;
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        let status = self.state.borrow().status.clone();
        html! {
            <div class="skin-preview-canvas">
                <div class="skin-preview-slot" ref={self.slot.clone()} role="img" aria-label={format!("noteskin preview: {status}")}></div>
                <p class="skin-preview-status" aria-hidden="true">{ status }</p>
            </div>
        }
    }
}

/// Takes up the app's device (at the start, and after a loss): a renderer
/// on it, the canvas made with it. `false` when there is none or the
/// preview closed.
async fn take_device(state: &Rc<RefCell<State>>, link: &html::Scope<PreviewCanvas>) -> bool {
    let gfx = match crate::web::screen::gfx().await {
        Ok(g) => g,
        Err(e) => {
            state.borrow_mut().status = format!("no graphics: {e}");
            link.send_message(Msg::Status);
            return false;
        }
    };
    let mut st = state.borrow_mut();
    if st.closed {
        return false;
    }
    gfx.set_mode(SurfaceMode::Scene);
    let renderer = Renderer::new(
        &gfx.device,
        &gfx.queue,
        gfx.view_format(),
        gfx.skin_blending(true),
        &[crate::game_loop::FONT],
    );
    // A device made again comes with a new canvas, in the old one's place.
    if let Some(canvas) = crate::web::screen::canvas() {
        if let Some(options) = st
            .shown
            .as_ref()
            .map(|s| s.options.clone())
            .or(st.wanted.clone())
        {
            size_canvas(&canvas, &options);
            let (w, h) = options.canvas_size(&options.layout());
            gfx.resize(w, h);
        }
        st.canvas = Some(canvas);
    }
    st.device = Some(Device { gfx, renderer });
    true
}

/// Sizes `canvas` for `options`: its pixels, and its size on the page.
fn size_canvas(canvas: &HtmlCanvasElement, options: &PreviewOptions) {
    let (w, h) = options.canvas_size(&options.layout());
    canvas.set_width(w);
    canvas.set_height(h);
    let ratio = options.pixel_ratio.max(0.25);
    let style = canvas.style();
    let _ = style.set_property("width", &format!("{}px", f64::from(w) / ratio));
    let _ = style.set_property("height", &format!("{}px", f64::from(h) / ratio));
}

/// Readies `options`' skin and shows it, unless newer options were asked
/// for meanwhile.
async fn load(state: &Rc<RefCell<State>>, options: PreviewOptions, generation: u64) {
    let layout = options.layout();
    let prepared = if options.skin == crate::noteskins::BUILT_IN {
        None
    } else {
        crate::noteskins::prepare(&options.skin, &layout, Default::default()).await
    };
    let mut st = state.borrow_mut();
    if st.closed || st.generation != generation {
        if let Some(p) = prepared {
            p.close();
        }
        return;
    }
    let st = &mut *st;
    // A lost device: the skin is readied again on the new one (`start`).
    let Some(device) = st.device.as_mut().filter(|d| !d.gfx.is_lost()) else {
        if let Some(p) = prepared {
            p.close();
        }
        return;
    };
    if let Some(old) = st.shown.take() {
        old.close();
    }
    // The canvas takes the new size as the new skin shows.
    if let Some(canvas) = &st.canvas {
        size_canvas(canvas, &options);
    }
    let (w, h) = options.canvas_size(&layout);
    device.gfx.resize(w, h);
    let Device { gfx, renderer } = device;
    renderer.clear_noteskin_textures();
    renderer.set_skins_drawn(&gfx.device, &gfx.queue, prepared.is_some());
    let [r, g, b] = options.background;
    renderer.skin.background = wgpu::Color { r, g, b, a: 1.0 };
    let (play, images) = match prepared {
        Some(p) => {
            let mut play = p.play;
            play.set_srgb(renderer.srgb() && !renderer.skins_blend_encoded());
            let images = crate::game_loop::upload_noteskin(
                renderer,
                &gfx.device,
                &gfx.queue,
                &mut play,
                &p.images,
            );
            (Some(play), images)
        }
        None => (None, Vec::new()),
    };
    st.status = match &play {
        Some(p) => format!("{} on {}", p.name, gfx.backend_name()),
        None if options.skin != crate::noteskins::BUILT_IN => {
            format!("no noteskin {:?}: the built-in arrows", options.skin)
        }
        None => format!("built-in arrows on {}", gfx.backend_name()),
    };
    let demo = if options.demo {
        Demo::new(&options.layout, options.bpm, options.speed).map(|d| (d, 0.0))
    } else {
        None
    };
    st.shown = Some(Shown {
        play,
        images,
        options,
        layout,
        start_ms: None,
        last_ms: 0.0,
        demo,
    });
}

/// Draws a frame on every animation frame while the canvas is shown.
fn start(
    state: Rc<RefCell<State>>,
    tick: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>,
    frame: Rc<Cell<i32>>,
    link: html::Scope<PreviewCanvas>,
) {
    let again = tick.clone();
    let next = frame.clone();
    *tick.borrow_mut() = Some(Closure::new(move |time_ms: f64| {
        let lost = {
            let mut st = state.borrow_mut();
            if st.closed {
                return;
            }
            let st = &mut *st;
            if st.failed {
                return;
            }
            let holds = st.attachment.as_ref().is_some_and(|a| a.holds());
            match (st.device.as_mut(), st.shown.as_mut()) {
                (Some(device), _) if device.gfx.is_lost() => !st.rebuilding,
                (Some(device), Some(shown)) if holds => {
                    draw(device, shown, time_ms);
                    false
                }
                _ => false,
            }
        };
        // The device was lost: the skin again on the new one.
        if lost {
            state.borrow_mut().rebuilding = true;
            let (state, link) = (state.clone(), link.clone());
            wasm_bindgen_futures::spawn_local(async move {
                if take_device(&state, &link).await {
                    let (options, generation) = {
                        let mut st = state.borrow_mut();
                        st.generation += 1;
                        let options = st
                            .shown
                            .as_ref()
                            .map(|s| s.options.clone())
                            .or(st.wanted.clone());
                        (options, st.generation)
                    };
                    if let Some(options) = options {
                        load(&state, options, generation).await;
                    }
                    link.send_message(Msg::Status);
                    state.borrow_mut().rebuilding = false;
                } else {
                    // No device to be had: the preview stops (its status
                    // says why), rather than asking every frame.
                    state.borrow_mut().failed = true;
                }
            });
        }
        if let (Some(w), Some(cb)) = (web_sys::window(), again.borrow().as_ref())
            && let Ok(id) = w.request_animation_frame(cb.as_ref().unchecked_ref())
        {
            next.set(id);
        }
    }));
    if let (Some(w), Some(cb)) = (web_sys::window(), tick.borrow().as_ref())
        && let Ok(id) = w.request_animation_frame(cb.as_ref().unchecked_ref())
    {
        frame.set(id);
    }
}

/// The beat a test asked for (`window.__DDI_PREVIEW_BEAT`).
fn asked_beat() -> Option<f64> {
    let w = web_sys::window()?;
    js_sys::Reflect::get(&w, &"__DDI_PREVIEW_BEAT".into())
        .ok()?
        .as_f64()
}

/// The tap notes of the lanes shown, on their receptors at `beat`.
fn still_frame(o: &PreviewOptions, layout: &Layout, beat: f64) -> Frame {
    let count = layout.lane_count();
    let lanes: Vec<usize> = if o.all_lanes {
        (0..count).collect()
    } else {
        vec![PreviewOptions::single_lane(layout)]
    };
    let notes = lanes
        .iter()
        .map(|&lane| NoteSprite {
            lane: lane as u8,
            y: 0.0,
            kind: SpriteKind::Tap,
            quantization: Quantization::N4,
            beat_frac: 0.0,
            color: None,
            alpha: 1.0,
            glow: 0.0,
            beat: 0.0,
            hold_life: 1.0,
        })
        .collect();
    Frame {
        song_time: beat * 60.0 / o.bpm,
        beat,
        lanes: count as u8,
        receptors: vec![
            ReceptorState {
                pressed: false,
                flash: None,
                holding: None,
            };
            count
        ],
        notes,
        judgement: None,
        combo: 0,
        score: Default::default(),
        life: 0.5,
        danger: false,
        failed: false,
        finished: false,
        visible_range: 1.0,
        appearance: Appearance::Visible,
    }
}

fn draw(device: &mut Device, shown: &mut Shown, time_ms: f64) {
    let start = shown.start_ms.get_or_insert(time_ms);
    // A hidden page's frames stop: the animation waits for it.
    let gap = time_ms - shown.last_ms;
    if shown.last_ms > 0.0 && gap > 250.0 {
        *start += gap - 1000.0 / 60.0;
    }
    shown.last_ms = time_ms;
    let start = *start;
    let o = &shown.options;
    let beat = asked_beat()
        .or(o.beat)
        .unwrap_or((time_ms - start) / 1000.0 * o.bpm / 60.0);
    let frame = match &mut shown.demo {
        Some((demo, last)) => {
            let seconds = beat * 60.0 / o.bpm;
            // A test going back in time: the demo again from the start.
            if seconds < *last {
                if let Some(d) = Demo::new(&o.layout, o.bpm, o.speed) {
                    *demo = d;
                }
                if let Some(p) = shown.play.as_mut() {
                    p.restart();
                }
            }
            *last = seconds;
            let step = demo.step(seconds);
            if let Some(p) = shown.play.as_mut() {
                p.update(&step.frame, step.events.iter(), false);
            }
            step.frame
        }
        None => {
            let frame = still_frame(o, &shown.layout, beat);
            if let Some(p) = shown.play.as_mut() {
                p.update(&frame, std::iter::empty(), false);
            }
            frame
        }
    };
    let Device { gfx, renderer } = device;
    let Some(surface) = gfx.frame() else {
        return;
    };
    let view = surface.texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(gfx.view_format()),
        ..Default::default()
    });
    let plain_view = gfx.plain_format().map(|format| {
        surface.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(format),
            ..Default::default()
        })
    });
    let mut encoder = gfx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("skin-preview"),
        });
    let opts = RenderOptions {
        preview: Some(Preview {
            geometry: o.geometry(&shown.layout),
            receptors: o.receptors,
        }),
        note_colors: o.colors,
        ..Default::default()
    };
    renderer.render(
        &gfx.device,
        &gfx.queue,
        &mut encoder,
        &view,
        plain_view.as_ref(),
        gfx.size().0,
        gfx.size().1,
        &frame,
        &shown.layout,
        // Unused: a preview draws no judgements.
        &JudgeNames::new(["", "", "", "", "", ""], "", ""),
        opts,
        shown
            .play
            .as_mut()
            .map(|p| p as &mut dyn ddi_render::noteskin::NoteLayer),
    );
    gfx.queue.submit(Some(encoder.finish()));
    gfx.queue.present(surface);
    if let Some(w) = web_sys::window() {
        let _ = js_sys::Reflect::set(&w, &"__DDI_PREVIEW_DRAWN".into(), &beat.into());
    }
}
