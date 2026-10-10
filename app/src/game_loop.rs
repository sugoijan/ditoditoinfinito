//! Per-frame driver owned by the gameplay canvas: sizes the canvas, ticks the
//! play session (when one is running) and renders it.

use std::cell::Cell;
use std::rc::Rc;

use ddi_library::backgrounds::{self, BgImage, BgSegment};
use ddi_platform::HostTime;
use ddi_platform::video::{StopReason, VideoGuard};
use ddi_render::{Backdrop, Renderer};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlCanvasElement, HtmlElement, ResizeObserver, ResizeObserverEntry};
use yew::Callback;

use crate::lyrics::LyricsView;
use crate::play::{PlaySession, SessionEvent};
use crate::settings::{BackgroundFrame, VideoMode};
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
    /// Background images copied to the GPU, closed after the next submit:
    /// the WebGL2 backend performs the copy then, not when it is issued.
    uploaded_images: Vec<web_sys::ImageBitmap>,
    /// Background textures by what they show (renderer ids).
    background_ids: Vec<(BgImage, usize)>,
    /// Decoded background images waiting to be copied to the GPU.
    pending_backgrounds: Vec<(BgImage, web_sys::ImageBitmap)>,
    /// Song time of the last frame drawn during play, `None` outside play.
    playing_at: Option<f64>,
    /// The song's background changes; empty outside a song.
    background_schedule: Vec<BgSegment>,
    /// The chart's lyrics, if it has any and they are shown.
    lyrics: Option<LyricsView>,
    /// The song's background movies.
    movies: crate::movies::MovieDeck,
    /// Image and movie textures for the backdrop, rebuilt when either
    /// changes (`(images, movie version)`).
    shown_cache: (Vec<(BgImage, usize)>, (usize, u64)),
    /// The movies were stopped at the end of the song.
    movies_stopped: bool,
    video_mode: VideoMode,
    /// The automatic setting's rule.
    guard: VideoGuard,
    /// The refresh rate measured before the play started.
    refresh_at_start: f64,
    /// Testing aid: each frame takes 40 ms longer while a movie is drawn.
    slow: bool,
    frame_setting: BackgroundFrame,
    /// The automatic frame, once settled: it never changes while a movie
    /// shows (that would be a jump of its own).
    frozen_frame: Option<Option<f32>>,
    /// Width over height of the song's background image.
    song_bg_aspect: Option<f32>,
}

/// Width over height of the screens pack art was made for.
const FOUR_THREE: f32 = 4.0 / 3.0;
const SIXTEEN_NINE: f32 = 16.0 / 9.0;

/// The cabinet screen shape nearest to `aspect` (by ratio): a square
/// jacket fills a 4:3 frame, an ultrawide picture a 16:9 one.
fn cabinet_shape(aspect: f32) -> f32 {
    if aspect < (FOUR_THREE * SIXTEEN_NINE).sqrt() {
        FOUR_THREE
    } else {
        SIXTEEN_NINE
    }
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

    /// Counts a frame; the rate of a second that ended with it, if one did.
    fn tick(&mut self, time_ms: f64) -> Option<f64> {
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
            return Some(self.fps);
        }
        None
    }
}

impl GameLoop {
    pub(crate) fn new(
        gfx: Gfx,
        canvas: HtmlCanvasElement,
        on_event: Callback<SessionEvent>,
    ) -> GameLoop {
        let renderer = Renderer::new(&gfx.device, &gfx.queue, gfx.view_format(), &[FONT]);
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
            uploaded_images: Vec::new(),
            background_ids: Vec::new(),
            pending_backgrounds: Vec::new(),
            playing_at: None,
            background_schedule: Vec::new(),
            lyrics: None,
            movies: crate::movies::MovieDeck::default(),
            shown_cache: (Vec::new(), (0, u64::MAX)),
            movies_stopped: false,
            video_mode: VideoMode::Auto,
            guard: VideoGuard::default(),
            refresh_at_start: 60.0,
            slow: false,
            frame_setting: BackgroundFrame::Auto,
            frozen_frame: None,
            song_bg_aspect: None,
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
        // Everything decoded so far goes up before the music starts.
        self.playing_at = None;
        self.guard = VideoGuard::default();
        self.refresh_at_start = self.refresh_hz();
        self.upload_backgrounds();
        self.session = Some(session);
        self.done = false;
    }

    /// Queues a background image the browser decoded, showing `what` (the
    /// song's background or a background change's file). It is copied to
    /// the GPU at the next frame outside play; during play only when no
    /// change is due within a second, one per frame, so an upload never
    /// costs a frame at a change.
    pub(crate) fn add_background(&mut self, what: BgImage, image: web_sys::ImageBitmap) {
        if self.background_ids.iter().any(|(w, _)| *w == what) {
            image.close();
            return;
        }
        self.pending_backgrounds.push((what, image));
    }

    /// Copies queued background images to the GPU, as
    /// [`GameLoop::add_background`] describes.
    fn upload_backgrounds(&mut self) {
        let count = match self.playing_at {
            None => self.pending_backgrounds.len(),
            Some(t) => {
                let next_change = self
                    .background_schedule
                    .iter()
                    .map(|s| s.seconds)
                    .find(|&s| s > t);
                usize::from(next_change.is_none_or(|s| s - t >= 1.0))
            }
        };
        let batch: Vec<_> = self
            .pending_backgrounds
            .drain(..count.min(self.pending_backgrounds.len()))
            .collect();
        for (what, image) in batch {
            self.upload_background(what, image);
        }
    }

    /// Copies one decoded image into a texture registered as showing `what`;
    /// the bitmap is closed once the copy is done.
    fn upload_background(&mut self, what: BgImage, image: web_sys::ImageBitmap) {
        let limit = self.gfx.device.limits().max_texture_dimension_2d;
        if image.width().max(image.height()) > limit {
            web_sys::console::warn_1(
                &format!(
                    "background {}×{} exceeds the GPU's {limit} px limit; not shown",
                    image.width(),
                    image.height()
                )
                .into(),
            );
            image.close();
            return;
        }
        let (width, height) = (image.width().max(1), image.height().max(1));
        let texture = self
            .renderer
            .create_background_texture(&self.gfx.device, width, height);
        self.gfx.queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::ImageBitmap(image.clone()),
                origin: wgpu::Origin2d::ZERO,
                flip_y: false,
            },
            wgpu::CopyExternalImageDestInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                color_space: wgpu::PredefinedColorSpace::Srgb,
                premultiplied_alpha: false,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        if what == BgImage::Song {
            self.song_bg_aspect = Some(width as f32 / height as f32);
        }
        let id = self.renderer.add_background(&self.gfx.device, &texture);
        self.background_ids.push((what, id));
        self.uploaded_images.push(image);
    }

    /// A background movie of the song (a stored or linked file), played
    /// when its changes come; `probe`: the song's first movie, whose
    /// picture sets the automatic background frame.
    pub(crate) fn add_movie(
        &mut self,
        name: String,
        blob: web_sys::Blob,
        format: ddi_platform::video::VideoFormat,
        probe: bool,
    ) {
        self.movies.add_source(name, blob, format, probe);
    }

    /// The video settings of the plays to come; `slow` is the testing aid
    /// that forces the automatic rule.
    pub(crate) fn set_video(&mut self, mode: VideoMode, frame: BackgroundFrame, slow: bool) {
        self.video_mode = mode;
        self.frame_setting = frame;
        self.slow = slow;
    }

    /// Why the automatic setting turned the movies off this play.
    pub(crate) fn video_stopped(&self) -> Option<StopReason> {
        self.movies.off()
    }

    /// Where the video decoder stands, once a movie asked for it.
    pub(crate) fn video_status(&self) -> Option<crate::web::video::ModuleStatus> {
        self.movies.module_status()
    }

    /// The shape backgrounds are fitted into (`None`: the whole screen).
    /// The automatic one follows the song's first movie, else its
    /// background image, and settles once a movie frame shows, or when the
    /// song starts with nothing left to learn.
    fn frame_aspect(&mut self, started: bool) -> Option<f32> {
        match self.frame_setting {
            BackgroundFrame::Screen => return None,
            BackgroundFrame::FourThree => return Some(FOUR_THREE),
            BackgroundFrame::SixteenNine => return Some(SIXTEEN_NINE),
            BackgroundFrame::Auto => {}
        }
        if let Some(frame) = self.frozen_frame {
            return frame;
        }
        let (movie, probing) = match self.movies.picture_aspect() {
            Ok(a) => (Some(a), false),
            Err(probing) => (None, probing),
        };
        let frame = movie.or(self.song_bg_aspect).map(cabinet_shape);
        if self.movies.shown_any() || (started && !probing) {
            self.frozen_frame = Some(frame);
        }
        frame
    }

    /// The background changes of the song about to play (empty: none).
    pub(crate) fn set_background_schedule(&mut self, schedule: Vec<BgSegment>) {
        self.background_schedule = schedule;
    }

    /// What to show behind the field at song second `t` (fields passed
    /// one by one, so a session can stay borrowed).
    fn backdrop(
        cache: &mut (Vec<(BgImage, usize)>, (usize, u64)),
        images: &[(BgImage, usize)],
        movies: &crate::movies::MovieDeck,
        schedule: &[BgSegment],
        t: f64,
    ) -> Backdrop {
        let key = (images.len(), movies.version());
        if cache.1 != key {
            *cache = (
                images.iter().cloned().chain(movies.shown_ids()).collect(),
                key,
            );
        }
        let shown = backgrounds::shown(schedule, t, &cache.0);
        Backdrop {
            current: shown.current,
            previous: shown.previous,
            mix: shown.mix,
        }
    }

    /// The lyrics of the song about to play (`None`: none shown).
    pub(crate) fn set_lyrics(&mut self, lyrics: Option<LyricsView>) {
        self.lyrics = lyrics;
    }

    pub(crate) fn clear_session(&mut self) {
        self.session = None;
        self.lyrics = None;
        self.movies.reset();
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
        let second = self.fps.tick(time_ms);
        self.upload_backgrounds();
        let drawing = self.movies.drawing(&self.background_schedule);
        if self.slow && drawing.is_some() {
            let now = || {
                web_sys::window()
                    .and_then(|w| w.performance())
                    .map_or(0.0, |p| p.now())
            };
            let until = now() + 40.0;
            while now() < until {}
        }
        let started = self.session.as_ref().is_some_and(|s| s.player.started());
        let frame_aspect = self.frame_aspect(started);
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
        let view = surface.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.gfx.view_format()),
            ..Default::default()
        });
        let mut encoder = self
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        match self.session.as_ref() {
            Some(session) => {
                let frame = session.player.frame(session.predicted_present(host_now));
                // The song time of each drawn frame, for tests that read
                // the screen (the debug state is refreshed every 10 frames).
                if let Some(window) = web_sys::window() {
                    let _ = js_sys::Reflect::set(
                        &window,
                        &"__DDI_SONG_TIME".into(),
                        &frame.song_time.into(),
                    );
                }
                // Movies follow the song time the notes are drawn at, until
                // the song is over (the results show the last frame).
                if session.player.finished() {
                    if !self.movies_stopped {
                        self.movies.stop();
                        self.movies_stopped = true;
                    }
                } else {
                    self.movies_stopped = false;
                    self.movies.update(
                        &self.background_schedule,
                        frame.song_time,
                        &mut self.renderer,
                        &self.gfx.device,
                        &self.gfx.queue,
                    );
                    if self.video_mode == VideoMode::Auto {
                        let refresh = self.refresh_at_start.max(self.refresh_hz());
                        let late = self.movies.stats().late;
                        if let Some(reason) =
                            self.guard.update(time_ms, drawing, second, refresh, late)
                        {
                            self.movies.turn_off(reason);
                        }
                    }
                }
                let mut render = session.render;
                render.frame_aspect = frame_aspect;
                render.backdrop = Self::backdrop(
                    &mut self.shown_cache,
                    &self.background_ids,
                    &self.movies,
                    &self.background_schedule,
                    frame.song_time,
                );
                self.playing_at = (session.player.started() && !session.player.finished())
                    .then_some(frame.song_time);
                if let Some(lyrics) = self.lyrics.as_mut() {
                    match self.playing_at {
                        Some(t) => {
                            let (w, h) = self.css_size.get();
                            let span = ddi_render::scene::field_span(&session.layout);
                            let geo =
                                ddi_render::FieldGeometry::new(w as f32, h as f32, span, false);
                            lyrics.update(t, f64::from(geo.arrow * span), h)
                        }
                        None => lyrics.hide(),
                    }
                }
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
                    render,
                );
            }
            None => {
                self.playing_at = None;
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.renderer.clear_color()),
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
        for image in self.uploaded_images.drain(..) {
            image.close();
        }
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
        set("video", self.movies.debug().into());
        set("backend", self.backend_name().into());
        set("srgb_view", self.gfx.view_format().is_srgb().into());
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
            let pressed = js_sys::Array::new();
            for &h in s.held() {
                pressed.push(&h.into());
            }
            set("pressed", pressed.into());
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
        if let Some(s) = &self.session {
            let status = s.audio_status();
            set("audio", status.as_str().into());
            text.push('\n');
            text.push_str(&status);
        }
        if let Some(g) = self.session.as_ref().and_then(|s| s.gamepads()) {
            let pads = js_sys::Array::new();
            for p in g.pads() {
                let total = p.stats.stamped_by_pad + p.stats.stamped_by_poll;
                let report = p
                    .stats
                    .report_interval
                    .map(|r| format!("{:.1} ms", r * 1000.0))
                    .unwrap_or_else(|| "—".into());
                text.push_str(&format!(
                    "\npad {}: {}{} · pad timestamps {}/{} · reports every {report}",
                    p.index,
                    p.id,
                    if p.standard { " (standard)" } else { "" },
                    p.stats.stamped_by_pad,
                    total,
                ));
                let o = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&o, &"id".into(), &p.id.as_str().into());
                let _ = js_sys::Reflect::set(&o, &"standard".into(), &p.standard.into());
                let _ = js_sys::Reflect::set(
                    &o,
                    &"stamped_by_pad".into(),
                    &(p.stats.stamped_by_pad as f64).into(),
                );
                let _ = js_sys::Reflect::set(
                    &o,
                    &"stamped_by_poll".into(),
                    &(p.stats.stamped_by_poll as f64).into(),
                );
                let _ = js_sys::Reflect::set(
                    &o,
                    &"report_interval".into(),
                    &p.stats.report_interval.unwrap_or(f64::NAN).into(),
                );
                pads.push(&o);
            }
            if pads.length() > 0 {
                text.push_str(&format!(
                    "\npad polling: {} /s ({})",
                    g.polls_per_second(),
                    if g.fast_running() {
                        "1 ms loop"
                    } else {
                        "per frame"
                    }
                ));
            }
            set("pads", pads.into());
            set("pad_polls", (g.polls_per_second() as f64).into());
            set("pad_fast", g.fast_running().into());
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
        for (_, image) in self.pending_backgrounds.drain(..) {
            image.close();
        }
    }
}
