//! Per-frame driver owned by the gameplay canvas: sizes the canvas, ticks the
//! play session (when one is running) and renders it.

use std::cell::Cell;
use std::rc::Rc;

use ddi_platform::HostTime;
use ddi_render::Renderer;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlCanvasElement, HtmlElement, ResizeObserver, ResizeObserverEntry};
use yew::Callback;

use crate::play::{PlaySession, SessionEvent};
use crate::web::gfx::Gfx;

/// The observer and the closure it calls, kept alive together and
/// disconnected before the closure is dropped (see `Drop`).
type ResizeHook = (ResizeObserver, Closure<dyn FnMut(js_sys::Array)>);

const FONT: &[u8] = include_bytes!("../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");

pub(crate) struct GameLoop {
    gfx: Gfx,
    canvas: HtmlCanvasElement,
    renderer: Renderer,
    session: Option<PlaySession>,
    on_event: Callback<SessionEvent>,
    done: bool,
    frames: u64,
    /// CSS size of the canvas, kept current by a `ResizeObserver` so the
    /// frame loop never forces a layout.
    css_size: Rc<Cell<(f64, f64)>>,
    _resize: Option<ResizeHook>,
    fps: FpsMeter,
    debug_el: Option<HtmlElement>,
}

/// Frames per second over the last second, from rAF timestamps.
struct FpsMeter {
    window_start: f64,
    count: u32,
    fps: f64,
    last: Option<f64>,
    /// Longest frame interval in the current window, ms.
    worst_ms: f64,
    worst_last: f64,
    /// Recent per-second rates; frames can be dropped but never exceed the
    /// refresh rate, so the maximum is the robust estimate.
    history: Vec<f64>,
    /// Shortest frame interval seen in the current window, ms.
    best_ms: f64,
    best_last: f64,
}

impl FpsMeter {
    fn new() -> FpsMeter {
        FpsMeter {
            window_start: 0.0,
            count: 0,
            fps: 0.0,
            last: None,
            worst_ms: 0.0,
            worst_last: 0.0,
            history: Vec::new(),
            best_ms: f64::INFINITY,
            best_last: 0.0,
        }
    }

    /// Best estimate of the display refresh rate, or `None` before two full
    /// seconds have been measured.
    fn refresh_estimate(&self) -> Option<f64> {
        if self.history.len() < 2 {
            return None;
        }
        let from_rate = self.history.iter().copied().fold(f64::MIN, f64::max);
        // The shortest interval is the firmest clue (drops only lengthen
        // intervals); rates above it are impossible.
        let from_interval = if self.best_last > 0.0 {
            1000.0 / self.best_last
        } else {
            from_rate
        };
        Some(from_rate.max(from_interval.min(from_rate * 1.5)))
    }

    fn tick(&mut self, time_ms: f64) {
        if let Some(last) = self.last {
            let dt = time_ms - last;
            self.worst_ms = self.worst_ms.max(dt);
            if dt > 1.0 {
                self.best_ms = self.best_ms.min(dt);
            }
        }
        self.last = Some(time_ms);
        self.count += 1;
        if time_ms - self.window_start >= 1000.0 {
            self.fps = self.count as f64 * 1000.0 / (time_ms - self.window_start);
            self.count = 0;
            self.window_start = time_ms;
            self.worst_last = self.worst_ms;
            self.worst_ms = 0.0;
            self.best_last = self.best_ms;
            self.best_ms = f64::INFINITY;
            self.history.push(self.fps);
            if self.history.len() > 4 {
                self.history.remove(0);
            }
        }
    }
}

impl GameLoop {
    pub(crate) fn new(
        gfx: Gfx,
        canvas: HtmlCanvasElement,
        on_event: Callback<SessionEvent>,
    ) -> GameLoop {
        let renderer = Renderer::new(&gfx.device, &gfx.queue, gfx.config.format, &[FONT]);
        let rect = canvas.get_bounding_client_rect();
        let css_size = Rc::new(Cell::new((rect.width(), rect.height())));
        let resize = {
            let size = css_size.clone();
            let cb = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
                for entry in entries.iter() {
                    if let Ok(entry) = entry.dyn_into::<ResizeObserverEntry>() {
                        let r = entry.content_rect();
                        size.set((r.width(), r.height()));
                    }
                }
            });
            ResizeObserver::new(cb.as_ref().unchecked_ref())
                .ok()
                .map(|o| {
                    o.observe(&canvas);
                    (o, cb)
                })
        };
        GameLoop {
            gfx,
            canvas,
            renderer,
            session: None,
            on_event,
            done: false,
            frames: 0,
            css_size,
            _resize: resize,
            fps: FpsMeter::new(),
            debug_el: None,
        }
    }

    pub(crate) fn backend_name(&self) -> &'static str {
        self.gfx.backend_name()
    }

    /// Measured display refresh rate, snapped to a common panel rate
    /// (60 Hz until two full seconds have been measured).
    pub(crate) fn refresh_hz(&self) -> f64 {
        match self.fps.refresh_estimate() {
            Some(hz) => crate::web::display::snap_refresh_rate(hz),
            None => 60.0,
        }
    }

    pub(crate) fn set_session(&mut self, session: PlaySession) {
        self.session = Some(session);
        self.done = false;
    }

    pub(crate) fn clear_session(&mut self) {
        self.session = None;
    }

    /// Element that receives the debug text, or `None` to disable.
    pub(crate) fn set_debug_element(&mut self, el: Option<HtmlElement>) {
        self.debug_el = el;
    }

    /// Match the backing store to the CSS size × devicePixelRatio.
    fn fit_canvas(&mut self) {
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        let (cw, ch) = self.css_size.get();
        let w = (cw * dpr).round().max(1.0) as u32;
        let h = (ch * dpr).round().max(1.0) as u32;
        if w != self.canvas.width() || h != self.canvas.height() {
            self.canvas.set_width(w);
            self.canvas.set_height(h);
            self.gfx.resize(w, h);
        }
    }

    /// One animation frame. `time_ms` is the rAF timestamp (performance timeline).
    pub(crate) fn frame(&mut self, time_ms: f64) {
        self.fit_canvas();
        self.fps.tick(time_ms);
        let host_now = HostTime(time_ms / 1000.0);

        let mut outcome = None;
        if let Some(session) = self.session.as_mut()
            && !self.done
        {
            let (_events, event) = session.tick(host_now);
            outcome = event;
        }

        let surface = match self.gfx.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(s) => s,
            wgpu::CurrentSurfaceTexture::Suboptimal(s) => {
                self.gfx
                    .surface
                    .configure(&self.gfx.device, &self.gfx.config);
                s
            }
            _ => {
                self.gfx
                    .surface
                    .configure(&self.gfx.device, &self.gfx.config);
                return;
            }
        };
        let view = surface
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        match self.session.as_ref() {
            Some(session) => {
                let frame = session.player.frame(session.predicted_present(host_now));
                self.renderer.render(
                    &self.gfx.device,
                    &self.gfx.queue,
                    &mut encoder,
                    &view,
                    self.gfx.config.width,
                    self.gfx.config.height,
                    &frame,
                    &session.layout,
                    &session.names,
                    session.render,
                );
            }
            None => {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.renderer.skin.background),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }
        }
        self.gfx.queue.submit(Some(encoder.finish()));
        self.gfx.queue.present(surface);

        if let Some(event) = outcome {
            self.done = true;
            self.on_event.emit(event);
        }
        self.frames += 1;
        if self.frames.is_multiple_of(10) {
            self.publish_debug(host_now);
        }
    }

    /// Debug text for the overlay element and `window.__DDI_DEBUG`.
    fn publish_debug(&self, host_now: HostTime) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let obj = js_sys::Object::new();
        let set = |k: &str, v: wasm_bindgen::JsValue| {
            let _ = js_sys::Reflect::set(&obj, &wasm_bindgen::JsValue::from_str(k), &v);
        };
        set("frames", (self.frames as f64).into());
        set("backend", self.backend_name().into());
        set("fps", self.fps.fps.into());
        let mut text = format!(
            "{} · {:.0} fps · worst {:.1} ms · {}×{}",
            self.backend_name(),
            self.fps.fps,
            self.fps.worst_last,
            self.gfx.config.width,
            self.gfx.config.height
        );
        if let Some(s) = &self.session {
            let p = &s.player;
            let clock = p.clock();
            let r = p.results();
            set("song_time", clock.heard_now(host_now).into());
            set("combo", (p.combo() as f64).into());
            set("max_combo", (p.max_combo() as f64).into());
            let taps = js_sys::Array::new();
            for n in r.tally.taps {
                taps.push(&(n as f64).into());
            }
            set("taps", taps.into());
            set("held", (r.tally.held as f64).into());
            set("let_go", (r.tally.let_go as f64).into());
            set("finished", p.finished().into());
            set("failed", p.failed().into());
            set("drift", clock.drift().unwrap_or(0.0).into());
            set("output_latency", clock.output_latency().into());
            set("mean_delta", r.mean_delta.into());
            set("stddev_delta", r.stddev_delta.into());
            let last = s
                .last_delta
                .map(|d| format!("{:+.1}", d * 1000.0))
                .unwrap_or_else(|| "—".into());
            text.push_str(&format!(
                "\nsong {:.2} s · frame {:.1} ms · out-latency {:.1} ms · drift {:+.1} ms\nerror: last {} ms · mean {:+.1} ms · σ {:.1} ms · fast/slow {}/{}",
                clock.heard_now(host_now),
                s.frame_interval() * 1000.0,
                clock.output_latency() * 1000.0,
                clock.drift().unwrap_or(0.0) * 1000.0,
                last,
                r.mean_delta * 1000.0,
                r.stddev_delta * 1000.0,
                r.fast,
                r.slow
            ));
        }
        if let Some(d) = self.session.as_ref().and_then(|s| s.devices.as_ref()) {
            text.push_str(&format!(
                "\naudio: {} · display: {}",
                d.audio.label, d.display.label
            ));
        }
        if let Some(el) = &self.debug_el {
            el.set_text_content(Some(&text));
        }
        let _ = js_sys::Reflect::set(
            &window,
            &wasm_bindgen::JsValue::from_str("__DDI_DEBUG"),
            &obj,
        );
    }
}

impl Drop for GameLoop {
    fn drop(&mut self) {
        // A pending resize callback must not run into a dropped closure.
        if let Some((observer, _)) = &self._resize {
            observer.disconnect();
        }
    }
}
