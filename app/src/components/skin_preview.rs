//! A noteskin's elements alone on a canvas (`#/skin-preview`): its Down tap
//! note (or a row of the four), at a size and over a colour of one's
//! choosing, as a play draws them. Its first uses are checking a skin
//! against the renders its author published and, later, previews in the
//! skin chooser.
//!
//! Parameters: `skin` (the setting's key, `bundled:dance/cf_chrome` or an
//! import's `dance/<name>`), `size` (one arrow's pixels, default 128),
//! `bg` (hex colour, default `000000`), `bpm` (default 60), `beat` (held
//! still at that beat; otherwise the beat runs from 0), `notes=4` (the four
//! dance arrows rather than Down), `receptors=1`, `gfx=gl`. A test may set
//! `window.__DDI_PREVIEW_BEAT`; after drawing it the page sets
//! `window.__DDI_PREVIEW_DRAWN` to that beat.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ddi_chart::{Layout, Quantization};
use ddi_engine::appearance::Appearance;
use ddi_engine::frame::{Frame, NoteSprite, ReceptorState, SpriteKind};
use ddi_engine::rules::JudgeNames;
use ddi_render::noteskin::NoteskinPlay;
use ddi_render::{FieldGeometry, Preview, RenderOptions, Renderer};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use yew::prelude::*;

use crate::web::gfx::{BackendPreference, Gfx};

#[derive(Properties, PartialEq)]
pub(crate) struct Props {
    pub(crate) params: Vec<(String, String)>,
}

/// What the URL asks for.
#[derive(Clone, Debug)]
struct Options {
    skin: String,
    size: u32,
    background: [f64; 3],
    bpm: f64,
    beat: Option<f64>,
    four: bool,
    receptors: bool,
    force_gl: bool,
}

impl Options {
    fn parse(params: &[(String, String)]) -> Options {
        let get = |k: &str| params.iter().find(|(p, _)| p == k).map(|(_, v)| v.as_str());
        let hex = get("bg").unwrap_or("000000").trim_start_matches('#');
        let channel = |i: usize| {
            hex.get(i..i + 2)
                .and_then(|c| u8::from_str_radix(c, 16).ok())
                .map_or(0.0, |c| f64::from(c) / 255.0)
        };
        Options {
            skin: get("skin").unwrap_or_default().to_string(),
            size: get("size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(128u32)
                .clamp(16, 512),
            background: [channel(0), channel(2), channel(4)],
            bpm: get("bpm")
                .and_then(|s| s.parse().ok())
                .filter(|b: &f64| *b > 0.0)
                .unwrap_or(60.0),
            beat: get("beat").and_then(|s| s.parse().ok()),
            four: get("notes") == Some("4"),
            receptors: get("receptors") == Some("1"),
            force_gl: get("gfx") == Some("gl"),
        }
    }

    /// The canvas's pixels.
    fn canvas_size(&self) -> (u32, u32) {
        let lanes = if self.four { 4 } else { 1 };
        (self.size * lanes, self.size)
    }

    /// One arrow a lane of `size` pixels; the notes on their receptors in
    /// the middle row.
    fn geometry(&self) -> FieldGeometry {
        let a = self.size as f32;
        let (w, h) = self.canvas_size();
        FieldGeometry {
            arrow: a,
            left: if self.four { 0.0 } else { -a },
            receptor_y: a / 2.0,
            direction: 1.0,
            width: w as f32,
            height: h as f32,
        }
    }
}

struct Running {
    gfx: Gfx,
    renderer: Renderer,
    play: NoteskinPlay,
    images: Vec<web_sys::ImageBitmap>,
    options: Options,
    start_ms: Option<f64>,
}

pub(crate) struct SkinPreview {
    canvas: NodeRef,
    status: Rc<RefCell<String>>,
    running: Rc<RefCell<Option<Running>>>,
    /// The animation frame callback, kept alive.
    tick: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>,
    /// The pending animation frame request, cancelled before the callback
    /// is dropped.
    frame: Rc<Cell<i32>>,
    /// The page was closed: a setup still running stops there.
    closed: Rc<Cell<bool>>,
}

pub(crate) enum Msg {
    Status,
}

impl Component for SkinPreview {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        SkinPreview {
            canvas: NodeRef::default(),
            status: Rc::new(RefCell::new("loading…".into())),
            running: Rc::new(RefCell::new(None)),
            tick: Rc::new(RefCell::new(None)),
            frame: Rc::new(Cell::new(0)),
            closed: Rc::new(Cell::new(false)),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Msg) -> bool {
        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            return;
        }
        let options = Options::parse(&ctx.props().params);
        let Some(canvas) = self.canvas.cast::<HtmlCanvasElement>() else {
            return;
        };
        let (w, h) = options.canvas_size();
        canvas.set_width(w);
        canvas.set_height(h);
        let status = self.status.clone();
        let running = self.running.clone();
        let tick = self.tick.clone();
        let frame = self.frame.clone();
        let closed = self.closed.clone();
        let link = ctx.link().clone();
        wasm_bindgen_futures::spawn_local(async move {
            let set = |s: String| {
                *status.borrow_mut() = s;
                link.send_message(Msg::Status);
            };
            let preference = if options.force_gl {
                BackendPreference::WebGl2
            } else {
                BackendPreference::Auto
            };
            let (gfx, _canvas) = match Gfx::new(canvas, preference).await {
                Ok(g) => g,
                Err(e) => return set(format!("no graphics: {e}")),
            };
            let layout = Layout::dance_single();
            let Some(prepared) =
                crate::noteskins::prepare(&options.skin, &layout, Default::default()).await
            else {
                return set(format!("no noteskin {:?}", options.skin));
            };
            let mut renderer = Renderer::new(
                &gfx.device,
                &gfx.queue,
                gfx.view_format(),
                gfx.skin_blending(true),
                &[crate::game_loop::FONT],
            );
            renderer.set_skins_drawn(&gfx.device, &gfx.queue, true);
            let [r, g, b] = options.background;
            renderer.skin.background = wgpu::Color { r, g, b, a: 1.0 };
            let mut play = prepared.play;
            play.set_srgb(renderer.srgb() && !renderer.skins_blend_encoded());
            let images = crate::game_loop::upload_noteskin(
                &mut renderer,
                &gfx.device,
                &gfx.queue,
                &mut play,
                &prepared.images,
            );
            if closed.get() {
                for image in images {
                    image.close();
                }
                return;
            }
            set(format!("{} on {}", play.name, gfx.backend_name()));
            *running.borrow_mut() = Some(Running {
                gfx,
                renderer,
                play,
                images,
                options,
                start_ms: None,
            });
            start(running, tick, frame);
        });
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.closed.set(true);
        if let Some(w) = web_sys::window() {
            let _ = w.cancel_animation_frame(self.frame.get());
        }
        self.tick.borrow_mut().take();
        if let Some(r) = self.running.borrow_mut().take() {
            for image in r.images {
                image.close();
            }
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <main class="skin-preview">
                <canvas ref={self.canvas.clone()} />
                <p class="skin-preview-status">{ self.status.borrow().clone() }</p>
            </main>
        }
    }
}

/// Draws a frame on every animation frame while the page is open.
fn start(
    running: Rc<RefCell<Option<Running>>>,
    tick: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>,
    frame: Rc<Cell<i32>>,
) {
    let again = tick.clone();
    let next = frame.clone();
    *tick.borrow_mut() = Some(Closure::new(move |time_ms: f64| {
        if let Some(r) = running.borrow_mut().as_mut() {
            draw(r, time_ms);
        } else {
            return;
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

fn draw(r: &mut Running, time_ms: f64) {
    let start = *r.start_ms.get_or_insert(time_ms);
    let o = &r.options;
    let beat = asked_beat()
        .or(o.beat)
        .unwrap_or((time_ms - start) / 1000.0 * o.bpm / 60.0);
    let lanes: Vec<u8> = if o.four { vec![0, 1, 2, 3] } else { vec![1] };
    let notes = lanes
        .iter()
        .map(|&lane| NoteSprite {
            lane,
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
    let frame = Frame {
        song_time: beat * 60.0 / o.bpm,
        beat,
        lanes: 4,
        receptors: vec![
            ReceptorState {
                pressed: false,
                flash: None,
                holding: None,
            };
            4
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
    };
    r.play.update(&frame, std::iter::empty(), false);
    let gfx = &r.gfx;
    let surface = match gfx.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(s) | wgpu::CurrentSurfaceTexture::Suboptimal(s) => s,
        _ => {
            gfx.surface.configure(&gfx.device, &gfx.config);
            return;
        }
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
            geometry: o.geometry(),
            receptors: o.receptors,
        }),
        ..Default::default()
    };
    r.renderer.render(
        &gfx.device,
        &gfx.queue,
        &mut encoder,
        &view,
        plain_view.as_ref(),
        gfx.config.width,
        gfx.config.height,
        &frame,
        &Layout::dance_single(),
        // Unused: a preview draws no judgements.
        &JudgeNames::new(["", "", "", "", "", ""], "", ""),
        opts,
        Some(&mut r.play),
    );
    gfx.queue.submit(Some(encoder.finish()));
    gfx.queue.present(surface);
    if let Some(w) = web_sys::window() {
        let _ = js_sys::Reflect::set(&w, &"__DDI_PREVIEW_DRAWN".into(), &beat.into());
    }
}
