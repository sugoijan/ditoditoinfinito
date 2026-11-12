//! Per-frame driver owned by the gameplay canvas: sizes the canvas, ticks the
//! play session (when one is running) and renders it, through the frame
//! body both shells share (`ddi_shell::play_view`) over the browser's
//! parts.

use std::cell::Cell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::component::Callback;
use crate::perf::{self, Lap};
use ddi_library::backgrounds::{self, BgImage};
use ddi_platform::HostTime;
use ddi_platform::video::StopReason;
use ddi_render::Renderer;
use ddi_shell::play_view::{Moment, PlayHost, PlayView, Screen, Target};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlCanvasElement, HtmlElement, ResizeObserver, ResizeObserverEntry};

use crate::movies::MovieDeck;
use crate::play::{PlaySession, SessionEvent};
use crate::scripts::ScriptView;
use crate::settings::{BackgroundFrame, VideoMode};
use crate::stage::Bitmap;
use crate::web::gfx::Gfx;
use serde_json::{Value, json};

/// The observer and the closure it calls, kept alive together and
/// disconnected before the closure is dropped (see `Drop`).
type ResizeHook = (ResizeObserver, Closure<dyn FnMut(js_sys::Array)>);

/// Texts the browser drew for a song's scripts: `(text, drawing, size in
/// StepMania's pixels)`.
type ScriptTexts = Rc<std::cell::RefCell<Vec<(String, Bitmap, [f32; 2])>>>;

/// What the browser brings to a play's drawing (`ddi_shell::play_view`):
/// its decoded bitmaps, its movie decoders, the scripts' text drawn by
/// its canvas, the lyrics' text, the frame recorder (`crate::perf`).
pub(crate) struct Browser {
    /// The scripts' texts the browser drew, waiting for the next frame.
    texts: ScriptTexts,
    /// Bitmaps copied to the GPU, closed after the next submit: the
    /// WebGL2 backend performs the copy then, not when it is issued.
    uploaded: Vec<Bitmap>,
    /// WebGL2: the lyrics' text is read back from its canvas.
    gl: bool,
    performance: Option<web_sys::Performance>,
}

impl PlayHost for Browser {
    type Image = Bitmap;
    type Deck = MovieDeck;

    fn warn(&self, line: &str) {
        web_sys::console::warn_1(&line.into());
    }

    fn lua_stopped(&mut self, key: &str, why: &str) {
        let (key, why) = (key.to_string(), why.to_string());
        wasm_bindgen_futures::spawn_local(async move {
            crate::noteskins::lua_stopped(&key, &why).await;
        });
    }

    /// The texts drawn so far go to the scripts; those they show that are
    /// not drawn yet are drawn, arriving in a later frame.
    fn draw_texts(&mut self, scripts: &mut ScriptView) {
        for (text, bitmap, size) in self.texts.borrow_mut().drain(..) {
            scripts.add_text(text, bitmap, size);
        }
        for text in scripts.measure() {
            let inbox = self.texts.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let size = ddi_session::scripted::TEXT_SIZE;
                let scale = f64::from(ddi_session::stage::TEXT_SCALE);
                match crate::web::image::text_bitmap(&text, size, "#ffffff", scale).await {
                    Ok((bitmap, (w, h))) => inbox.borrow_mut().push((text, Bitmap(bitmap), [w, h])),
                    Err(e) => web_sys::console::warn_1(&e.into()),
                }
            });
        }
    }

    fn uploaded(&mut self, image: Bitmap) {
        self.uploaded.push(image);
    }

    fn text(&mut self) -> Option<Box<dyn ddi_render::RunRasterizer>> {
        match crate::web::text::CanvasRasterizer::new(self.gl) {
            Ok(r) => Some(Box::new(r)),
            Err(e) => {
                web_sys::console::warn_1(&format!("lyrics: no text: {e}").into());
                None
            }
        }
    }

    fn seconds(&self) -> f64 {
        self.performance.as_ref().map_or(0.0, |p| p.now()) / 1000.0
    }

    fn snap_refresh(&self, hz: f64) -> f64 {
        crate::web::display::snap_refresh_rate(hz)
    }

    fn lap(&mut self, lap: ddi_shell::play_view::Lap) {
        use ddi_shell::play_view::Lap as Shared;
        perf::lap(match lap {
            Shared::Upload => Lap::Upload,
            Shared::Movies => Lap::Movies,
            Shared::Scene => Lap::Scene,
            Shared::Hand => Lap::Hand,
            Shared::Render => Lap::Render,
        });
    }
}

/// The play's canvas: the shared view (through `Deref`) with the page's
/// session, surface and debug state.
pub(crate) struct GameLoop {
    view: PlayView<Browser>,
    /// The app's device (`web::screen`), shared.
    gfx: Rc<Gfx>,
    canvas: HtmlCanvasElement,
    session: Option<PlaySession>,
    on_event: Callback<SessionEvent>,
    done: bool,
    frames: u64,
    /// CSS size of the canvas, kept current by a `ResizeObserver` so the
    /// frame loop never forces a layout.
    css_size: Rc<Cell<(f64, f64)>>,
    _resize: Option<ResizeHook>,
    debug_el: Option<HtmlElement>,
    /// The scripts' sounds decoded until their buffers are made (on the
    /// play's sound, the first time each plays), the buffers, and how
    /// many played so far.
    script_sounds: std::collections::HashMap<String, ddi_platform::decode::DecodedAudio>,
    sound_buffers: std::collections::HashMap<String, web_sys::AudioBuffer>,
    sounds_played: usize,
    /// Testing aid: each frame takes 40 ms longer while a movie is drawn.
    slow: bool,
    /// Driven by a test, or with the debug overlay asked for: the debug
    /// state kept fresh, not once a second.
    automated: bool,
    /// When the debug state was last made (milliseconds).
    debug_at: f64,
    /// The play ended this frame: its recording ends with it.
    end_perf_after: bool,
    /// A noteskin readied while a play was on: used from the next play.
    next_noteskin: Option<ddi_shell::play_view::Skin<Bitmap>>,
    /// The announcer studio's scene in play: the play drawn into a
    /// picture shown in its frame (`crate::studio_scene`).
    pub(crate) framed: Option<crate::studio_scene::Framed>,
}

impl Deref for GameLoop {
    type Target = PlayView<Browser>;

    fn deref(&self) -> &Self::Target {
        &self.view
    }
}

impl DerefMut for GameLoop {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.view
    }
}

impl GameLoop {
    /// `framed`: the announcer studio's scene in play, drawn in its frame
    /// (`crate::studio_scene`): the play into a picture of encoded values
    /// as Options' field preview draws (`ddi_render::field_preview`).
    pub(crate) fn new(
        gfx: Rc<Gfx>,
        canvas: HtmlCanvasElement,
        on_event: Callback<SessionEvent>,
        framed: Option<crate::studio_scene::FrameSpec>,
    ) -> GameLoop {
        let renderer = match &framed {
            Some(_) => Renderer::new(
                &gfx.device,
                &gfx.queue,
                ddi_render::field_preview::FORMAT,
                match gfx.float_format() {
                    Some(f) => ddi_render::SkinBlending::Encoded(f),
                    None => ddi_render::SkinBlending::Scene,
                },
                &[ddi_render::HUD_FONT],
            ),
            None => Renderer::new(
                &gfx.device,
                &gfx.queue,
                gfx.view_format(),
                gfx.skin_blending(),
                &[ddi_render::HUD_FONT],
            ),
        };
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
        let browser = Browser {
            texts: Default::default(),
            uploaded: Vec::new(),
            gl: gfx.is_gl(),
            performance: web_sys::window().and_then(|w| w.performance()),
        };
        GameLoop {
            view: PlayView::new(browser, renderer),
            gfx,
            canvas,
            session: None,
            on_event,
            done: false,
            frames: 0,
            css_size,
            _resize: resize,
            debug_el: None,
            script_sounds: Default::default(),
            sound_buffers: Default::default(),
            sounds_played: 0,
            slow: false,
            automated: web_sys::window().is_some_and(|w| {
                js_sys::Reflect::get(&w.navigator(), &"webdriver".into())
                    .is_ok_and(|v| v.is_truthy())
            }),
            debug_at: f64::NEG_INFINITY,
            end_perf_after: false,
            next_noteskin: None,
            framed: framed.map(crate::studio_scene::Framed::new),
        }
    }

    pub(crate) fn backend_name(&self) -> &'static str {
        self.gfx.backend_name()
    }

    /// The stage just played was drawn by a final attack's knockout
    /// (`ddi_room::attacks::SETTLEMENT_V0`): its results say so.
    pub(crate) fn drawn_by_final_attack(&self) -> bool {
        self.session
            .as_ref()
            .and_then(|s| s.battle())
            .is_some_and(|b| b.drawn_by_final_attack())
    }

    pub(crate) fn set_session(&mut self, session: PlaySession) {
        self.end_perf();
        perf::begin_play();
        let (device, queue) = (&self.gfx.device, &self.gfx.queue);
        if let Some(skin) = self.next_noteskin.take() {
            self.view.set_noteskin(device, queue, Some(skin));
        }
        // Everything decoded so far goes up before the music starts, the
        // layers' textures with the skin's, after them; without a skin
        // (another layout, the chart's own colors) or layers drawn as
        // skins are, frames need not be drawn offscreen for them (WebGL2).
        self.view.prepare(device, queue);
        // A retry plays the noteskin's commands from the start again,
        // and the song's scripts.
        self.view.start();
        self.session = Some(session);
        self.done = false;
    }

    /// Queues a background image the browser decoded, showing `what` (the
    /// song's background or a background change's file): see
    /// [`PlayView::add_background`].
    pub(crate) fn add_background(&mut self, what: BgImage, image: web_sys::ImageBitmap) {
        self.view.add_background(what, Bitmap(image));
    }

    /// A background movie of the song (a stored or linked file), played
    /// when its changes come; `probe`: the song's first movie, whose
    /// picture sets the automatic background frame.
    pub(crate) fn add_movie(
        &mut self,
        name: String,
        blob: crate::web::movie_source::MovieSource,
        format: ddi_platform::video::VideoFormat,
        probe: bool,
    ) {
        self.view.movies.add_source(name, blob, format, probe);
    }

    /// The video settings of the plays to come; `slow` is the testing aid
    /// that forces the automatic rule.
    pub(crate) fn set_video(&mut self, mode: VideoMode, frame: BackgroundFrame, slow: bool) {
        self.view.set_video(mode, frame);
        self.slow = slow;
    }

    /// Why the automatic setting turned the movies off this play.
    pub(crate) fn video_stopped(&self) -> Option<StopReason> {
        self.view.movies.off()
    }

    /// What kept movies from playing this play, for the results.
    pub(crate) fn video_trouble(&self) -> Option<ddi_platform::video::VideoTrouble> {
        self.view.movies.trouble()
    }

    /// Where the video decoder stands, once a movie asked for it.
    pub(crate) fn video_status(&self) -> Option<crate::web::video::ModuleStatus> {
        self.view.movies.module_status()
    }

    /// The song's scripts, loaded (`None`: none, or they failed).
    pub(crate) fn set_scripts(&mut self, loaded: Option<crate::scripts::Loaded>) {
        self.sound_buffers.clear();
        self.view.host.texts.borrow_mut().clear();
        let mut scripts = ScriptView::default();
        let movies = match loaded {
            Some(l) => {
                self.script_sounds = l.sounds.into_iter().collect();
                scripts.set(Some(l.scripts), l.screen, l.images);
                l.movies
            }
            None => {
                self.script_sounds.clear();
                Vec::new()
            }
        };
        self.view.set_scripts(scripts);
        let unknown = ddi_platform::video::VideoFormat {
            container: ddi_platform::video::VideoContainer::Unknown,
            codec: ddi_platform::video::VideoCodec::Unknown,
        };
        for (name, blob) in movies {
            self.view.script_movies.add_source(
                name,
                crate::web::movie_source::MovieSource::Blob(blob),
                unknown.clone(),
                false,
            );
        }
    }

    /// The scripts' sounds played this frame, at once on the play's
    /// sound at its volume (a buffer made the first time each plays).
    fn play_script_sounds(&mut self) {
        use ddi_platform::AudioBackend;
        let due = self.view.take_script_sounds();
        let Some(session) = self.session.as_mut() else {
            return;
        };
        for f in due {
            if !self.sound_buffers.contains_key(&f)
                && let Some(audio) = self.script_sounds.remove(&f)
            {
                match session
                    .audio()
                    .buffer_from_pcm(audio.channels, f64::from(audio.sample_rate))
                {
                    Ok(b) => {
                        self.sound_buffers.insert(f.clone(), b);
                    }
                    Err(e) => web_sys::console::warn_1(&format!("{f}: {e}").into()),
                }
            }
            let Some(buffer) = self.sound_buffers.get(&f) else {
                continue;
            };
            match session.play_now(buffer) {
                Ok(_) => self.sounds_played += 1,
                Err(e) => web_sys::console::warn_1(&format!("{f}: {e}").into()),
            }
        }
    }

    /// A content of the song's layers decoded.
    pub(crate) fn add_stage_content(
        &mut self,
        source: ddi_engine::sprites::SpriteSource,
        bitmap: web_sys::ImageBitmap,
        size: (f32, f32),
    ) {
        self.view.add_stage_content(source, Bitmap(bitmap), size);
    }

    /// Uses `play` for the plays to come, with its skin images decoded
    /// (`images[i]` for texture `i`, `None` for built-in art); `None` draws
    /// the built-in skin.
    pub(crate) fn set_noteskin(
        &mut self,
        play: Option<ddi_render::noteskin::NoteskinPlay>,
        images: Vec<Option<web_sys::ImageBitmap>>,
        key: String,
    ) {
        let images = images.into_iter().map(|i| i.map(Bitmap)).collect();
        // Mid-play the arrows do not change: from the next play on.
        if self.session.is_some() {
            if let Some(play) = play {
                self.next_noteskin = Some((play, images, key));
            }
            return;
        }
        let (device, queue) = (&self.gfx.device, &self.gfx.queue);
        self.view
            .set_noteskin(device, queue, play.map(|p| (p, images, key)));
    }

    /// The canvas back on the play's drawing (after a screen drew on it
    /// with the 2D pass alone).
    pub(crate) fn scene_mode(&self) {
        self.gfx.set_mode(crate::web::gfx::SurfaceMode::Scene);
    }

    /// The format the play's hand and lyrics are drawn in on the canvas.
    fn play_format(&self) -> wgpu::TextureFormat {
        self.gfx
            .plain_format()
            .unwrap_or_else(|| self.gfx.view_format())
    }

    /// A frame while the start prompt (`ddi_ui::screens::prompt`, the
    /// page's) draws on the canvas: nothing drawn here, but the refresh
    /// rate is measured (the devices' fingerprint and the video rule read
    /// it), decoded backgrounds go up to the GPU, what the play's first
    /// frames would stop to make is made, and the debug state is kept.
    pub(crate) fn idle(&mut self, time_ms: f64) {
        if self.gfx.is_lost() {
            return;
        }
        let _ = self.view.tick(time_ms);
        let (device, queue) = (&self.gfx.device, &self.gfx.queue);
        self.view.prepare(device, queue);
        let format = self.play_format();
        self.view.warm(device, queue, format);
        let (w, h) = self.gfx.size();
        self.view.dance.preload_work(
            device,
            queue,
            format,
            [w, h],
            &crate::web::clock::PerformanceClock::seconds(),
        );
        self.frames += 1;
        if self.debug_due(time_ms) {
            self.publish_debug(HostTime(time_ms / 1000.0));
        }
    }

    /// Whether the debug state is due: it makes a large object for JavaScript
    /// to collect, every 10 frames only where something reads it that often.
    fn debug_due(&mut self, time_ms: f64) -> bool {
        if self.automated || self.debug_el.is_some() {
            return self.frames.is_multiple_of(10);
        }
        let due = time_ms - self.debug_at >= 1000.0;
        if due {
            self.debug_at = time_ms;
        }
        due
    }

    /// The lines the play's announcer said so far, by folder.
    pub(crate) fn spoken(&self) -> Vec<String> {
        self.session.as_ref().map_or(Vec::new(), |s| {
            s.announcer_report()
                .spoken
                .iter()
                .map(|(f, _)| f.clone())
                .collect()
        })
    }

    /// The play taken back (a restart's sound and music).
    pub(crate) fn take_session(&mut self) -> Option<PlaySession> {
        self.session.take()
    }

    pub(crate) fn clear_session(&mut self) {
        self.view.dance.reset_preload();
        self.session = None;
        self.view.clear();
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
        }
        // Another holder of the canvas may have left the surface at its
        // own size.
        self.gfx.resize(w, h);
    }

    /// One animation frame. `time_ms` is the rAF timestamp (performance timeline).
    pub(crate) fn frame(&mut self, time_ms: f64) {
        // The device is gone: the route starts over on the new one.
        if self.gfx.is_lost() {
            return;
        }
        perf::frame_start(time_ms);
        self.fit_canvas();
        let second = self.view.tick(time_ms);
        // As a movie that costs frames would: not while it is paused.
        if self.slow
            && !self.view.movies.paused()
            && self.view.movies.drawing(self.view.schedule()).is_some()
        {
            let now = crate::web::clock::PerformanceClock::now_ms;
            let until = now() + 40.0;
            while now() < until {}
        }
        let host_now = HostTime(time_ms / 1000.0);

        let mut outcome = None;
        let mut song_second = f64::NAN;
        if let Some(session) = self.session.as_mut()
            && !self.done
        {
            let (_events, event) = session.tick(host_now);
            // A fail is told at once (the play goes on to its end).
            if let Some(failed) = session.take_fail_report() {
                self.on_event.emit(failed);
            }
            // A practice stage's outcome, to the room as the play ends
            // (before its results ask the room how it went).
            // Each attack's commitment, to the console (M8b, development).
            for line in session.take_battle_log() {
                web_sys::console::debug_1(&line.into());
            }
            if let Some(o) = session.take_stage_outcome() {
                let ending = crate::room::report(o);
                // The results' battle line, now the room has confirmed it.
                session.stage_reported(ending);
            }
            outcome = event;
        }
        perf::lap(Lap::Tick);

        // A scene of the announcer studio's: the canvas drawn by the 2D
        // pass alone (the frame), the play into its picture.
        if self.framed.is_some() {
            self.gfx.set_mode(crate::web::gfx::SurfaceMode::Plain);
        }
        let Some(surface) = self.gfx.frame() else {
            perf::frame_end(f64::NAN, 0.0);
            return;
        };
        perf::lap(Lap::Surface);
        let (surface_w, surface_h) = self.gfx.size();
        let (view, plain_view, play_format) = match self.framed.as_mut() {
            Some(f) => (
                f.target(&self.gfx.device, [surface_w, surface_h]).clone(),
                None,
                ddi_render::field_preview::FORMAT,
            ),
            None => {
                let view = surface.texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(self.gfx.view_format()),
                    ..Default::default()
                });
                // Skins' quads blend as StepMania's do, through the plain
                // view.
                let plain_view = self.gfx.plain_format().map(|format| {
                    surface.texture.create_view(&wgpu::TextureViewDescriptor {
                        format: Some(format),
                        ..Default::default()
                    })
                });
                (view, plain_view, self.play_format())
            }
        };
        let mut encoder = self
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        match self.session.as_ref() {
            Some(session) => {
                // The field's pixels to a CSS pixel, and the touch
                // screens' quit button, clear of the safe area.
                let css = self.css_size.get();
                let ratio = surface_w as f32 / (css.0 as f32).max(1.0);
                let now_ms = crate::web::clock::PerformanceClock::now_ms();
                let quit = crate::web::touch::quit_shown(now_ms)
                    .then(|| crate::web::touch::safe_insets(css));
                let target = Target {
                    view: &view,
                    plain: plain_view.as_ref(),
                    size: [surface_w, surface_h],
                    format: play_format,
                };
                let drawn = self.view.draw(
                    (&self.gfx.device, &self.gfx.queue, &mut encoder),
                    &target,
                    session,
                    session.render,
                    Moment {
                        now: host_now,
                        present: None,
                        second,
                    },
                    // A scene's picture has no lyrics.
                    Screen {
                        ratio,
                        quit,
                        lyrics: self.framed.is_none(),
                    },
                );
                // Where the quit button is (nowhere while it is not: no
                // press lands on it).
                crate::web::touch::set_quit_at(drawn.quit_at);
                // And where a stage's attack policy label takes a press.
                crate::web::touch::set_policy_at(drawn.policy_at);
                // The song time of each drawn frame, for tests that read
                // the screen (the debug state is refreshed every 10 frames).
                if self.automated
                    && let Some(window) = web_sys::window()
                {
                    let _ = js_sys::Reflect::set(
                        &window,
                        &"__DDI_SONG_TIME".into(),
                        &drawn.song_time.into(),
                    );
                }
                song_second = drawn.song_time;
            }
            None => {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.view.renderer.clear_color()),
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
        self.play_script_sounds();
        // Over the play: a scene's frame, the play's picture in it, over
        // the canvas.
        if let Some(f) = self.framed.as_ref() {
            f.set_spots(self.view.renderer.hud_spots());
        }
        let framed = match self.framed.as_mut() {
            Some(f) => {
                if let Some(session) = self.session.as_ref() {
                    use ddi_platform::AudioBackend;
                    let now = session.audio().now().context_time;
                    f.said(&session.announcer_report().spoken, now);
                }
                let target = self.gfx.menu_format().map(|format| {
                    surface.texture.create_view(&wgpu::TextureViewDescriptor {
                        format: Some(format),
                        ..Default::default()
                    })
                });
                if let Some(target) = target {
                    let css = self.css_size.get();
                    f.draw(
                        &self.gfx,
                        &mut encoder,
                        &target,
                        [surface_w, surface_h],
                        css,
                        f64::from(surface_w) / css.0.max(1.0),
                        host_now.0,
                    );
                }
                true
            }
            None => false,
        };
        self.gfx.queue.submit(Some(encoder.finish()));
        perf::submitted(&self.gfx.queue, !self.gfx.is_gl());
        perf::lap(Lap::Submit);
        if framed && let Some(f) = self.framed.as_ref() {
            f.submitted(&self.gfx);
        }
        // Their copies are done: the bitmaps close.
        self.view.host.uploaded.clear();
        self.gfx.queue.present(surface);

        if let Some(event) = outcome {
            self.done = true;
            self.end_perf_after = true;
            // The announcer's last word (the results' line) for the tests,
            // the frames stopping now.
            if let (Some(s), Some(d)) = (self.session.as_ref(), crate::web::debug_object()) {
                let report = crate::web::json(&announcer_debug(s.announcer_report()));
                let _ = js_sys::Reflect::set(&d, &"announcer".into(), &report);
            }
            self.on_event.emit(event);
        }
        self.frames += 1;
        if self.debug_due(time_ms) || self.end_perf_after {
            self.publish_debug(host_now);
        }
        perf::lap(Lap::Present);
        perf::frame_end(song_second, self.view.dance.prepare_ms());
        if std::mem::take(&mut self.end_perf_after) {
            self.end_perf();
        }
    }

    /// The frames recorded so far kept (`crate::perf`), the play over.
    fn end_perf(&self) {
        perf::end_play(|| {
            let (w, h) = self.gfx.size();
            let dpr = web_sys::window().map_or(0.0, |w| w.device_pixel_ratio());
            let (address, agent) = web_sys::window().map_or_else(Default::default, |w| {
                (
                    w.location().hash().unwrap_or_default(),
                    w.navigator().user_agent().unwrap_or_default(),
                )
            });
            let refresh = self.refresh_hz();
            let meta = serde_json::json!({
                "address": address,
                "agent": agent,
                "backend": self.backend_name(),
                "refresh_hz": refresh,
                "device_pixel_ratio": dpr,
                "canvas": [w, h],
                "skin": self.view.noteskin_key(),
                "hand_preparing": self.dance.prepare_ms() > 0.0 || self.dance.preload_pending(),
            })
            .to_string();
            // The object's members, for the file's own.
            let members = meta
                .trim_start_matches('{')
                .trim_end_matches('}')
                .to_owned();
            (members, 1.0 / refresh.max(1.0))
        });
    }

    /// The last frame's background quads, each layer's back to front:
    /// `{layer, file, add, place, alpha}` (`file` as the schedule names it).
    fn backdrop_debug(&self) -> Vec<Value> {
        let drawn = self.view.backdrop_drawn();
        let layers = [&drawn.first, &drawn.second];
        let mut out = Vec::new();
        for (layer, draws) in layers.into_iter().enumerate() {
            for d in draws.iter().flatten() {
                // A picture's name, else a movie's (its texture).
                let movie = self.view.movies.shown_ids().find(|(_, id)| *id == d.id);
                let file = self
                    .view
                    .pictures()
                    .iter()
                    .find(|(_, id)| *id == d.id)
                    .cloned()
                    .or(movie)
                    .map(|(what, _)| match &what {
                        BgImage::File(f) | BgImage::Movie(f) => f.clone(),
                        other => format!("{other:?}"),
                    })
                    .unwrap_or_else(|| d.id.to_string());
                let place = match d.place {
                    backgrounds::Place::Cover { .. } => "cover",
                    backgrounds::Place::CropTo { .. } => "cropto",
                    backgrounds::Place::Fill { .. } => "fill",
                    backgrounds::Place::Native { .. } => "native",
                };
                out.push(json!({
                    "layer": layer,
                    "file": file,
                    "add": d.add,
                    "place": place,
                    "alpha": f64::from(d.color[3]),
                    "crop_top": f64::from(d.crop[1]),
                }));
            }
        }
        out
    }

    /// `__DDI_DEBUG.noteskin` for the skin drawn.
    fn noteskin_debug(&self, n: &ddi_render::noteskin::NoteskinPlay) -> Value {
        // What its quads cost the last frame (`noteskin_cost` measures
        // the same on desktop).
        let stats = self.view.renderer.quad_stats();
        let (fuel, max_fuel) = n.lua_fuel();
        json!({
            "name": n.name,
            "notes": n.notes,
            "problems": n.problems(),
            "quads": n.last_quads(),
            "gpu": {
                "instances": stats.quads,
                "runs": stats.runs,
                "stencil_clears": stats.stencil_clears,
                "material_quads": stats.material_quads,
                "pipelines_made": stats.pipelines_made,
            },
            // The parameters' values its Lua reads.
            "params": n.parameters(),
            "lua": {
                "runs": n.runs_lua(),
                "fuel": fuel,
                "max_fuel": max_fuel,
                "stopped": n.lua_failure(),
            },
        })
    }

    /// Everything `window.__DDI_DEBUG` says of the play that can be made
    /// as JSON.
    fn debug_json(&self, host_now: HostTime) -> Value {
        let mut debug = json!({
            "frames": self.frames,
            "dance": self.dance.debug(),
            // The lyrics drawn over the play (their texts).
            "lyrics": self.view.lyrics_shown(),
            "backdrop": self.backdrop_debug(),
            // The song's scripts: whether they run, why they stopped, the
            // fuel of the last frame, the quads drawn (background,
            // foreground).
            "scripts": {
                "running": self.view.scripts().running(),
                "stopped": self.view.scripts_stopped().map(ToString::to_string),
                "fuel": self.view.scripts().frame_fuel(),
                "back": self.view.scripts_drawn().0,
                "sounds": self.sounds_played,
                "front": self.view.scripts_drawn().1,
            },
            "noteskin": self.view.noteskin().map(|n| self.noteskin_debug(n)),
            "backend": self.backend_name(),
            "srgb_view": self.gfx.view_format().is_srgb(),
            "skins_encoded": self.view.renderer.skins_blend_encoded(),
            "offscreen": self.view.renderer.offscreen(),
            "fps": self.view.fps.fps,
            // HUD runs shaped so far (a new string or size each).
            "text_shaped": self.view.renderer.text_shaped(),
            // The song's opening still is loaded (what the start prompt
            // shows is the page's, `prompt_backdrop`).
            "still": self.view.pictures().iter().any(|(w, _)| *w == BgImage::Still),
        });
        let mut video = self.view.movies.debug();
        let (update_ms, update_ms_max) = self.view.video_ms();
        video["update_ms"] = json!(update_ms);
        video["update_ms_max"] = json!(update_ms_max);
        video["guard"] = json!(self.view.guard().guard.state());
        debug["video"] = video;
        if let Some(d) = self.view.generated() {
            debug["generated"] = json!({
                "look": d.look,
                "palette": d.palette,
                "over": d.over,
                "analyzed": d.analyzed,
                "drawn": d.drawn,
                "width": d.size[0],
                "height": d.size[1],
            });
        }
        if let Some(s) = &self.session {
            let p = &s.player;
            let clock = p.clock();
            let r = p.results_brief();
            // A replay's edges fed so far (`__DDI_DEBUG.replay`).
            if let Some((fed, edges)) = s.replay_progress() {
                crate::replay::progress(fed, edges);
            }
            // The assist claps scheduled and the mine sounds played.
            let (claps, mines) = s.event_sounds();
            extend(
                &mut debug,
                json!({
                    "song_time": clock.heard_now(host_now),
                    "combo": p.combo(),
                    "max_combo": p.max_combo(),
                    "taps": r.tally.taps,
                    // Stray presses counted (DDI's Boos, FFR's), and the
                    // judgment word the HUD shows now (null: none).
                    "boos": r.tally.boo,
                    "judgement": p.judgement_flash(clock.heard_now(host_now)).map(|f| json!({
                        "word": ddi_render::scene::judgement_word(&p.ruleset().names, &f).0,
                        "boo": f.boo,
                        "age": f.age,
                    })),
                    "held": r.tally.held,
                    "let_go": r.tally.let_go,
                    "stage_back": self.view.stage_drawn().0,
                    "stage_mask": self.view.stage_drawn().1,
                    "ticks_hit": r.tally.checkpoint_hit,
                    "ticks_missed": r.tally.checkpoint_miss,
                    "holds_ticked": p.ruleset().judge.hold.is_ticked(),
                    "pressed": s.held(),
                    // The quit button's place while it shows (CSS pixels:
                    // x, y, radius), else null.
                    "quitButton": crate::web::touch::quit_at(),
                    "finished": p.finished(),
                    "failed": p.failed(),
                    // The lane covers now ("sudden+ 30%"; null when off).
                    "cover": p.cover().label(),
                    "claps": claps,
                    "mine_sounds": mines,
                    // A practice stage's CPUs as the play shows them:
                    // `score` a fraction of the best its notes give (the
                    // percent), `sum` DDI's score on the chart's scale
                    // (`CPU_SCORE_V0`), `score_shown` as its miniature
                    // says it, `locked` its lanes a shock locked (DDI).
                    "opponents": s.opponents().iter().enumerate().map(|(i, o)| json!({
                        "id": o.participant.0,
                        "name": o.name,
                        "level": o.level,
                        "life": o.life,
                        "out": o.out,
                        "score": o.score,
                        "sum": o.sum,
                        "score_shown": self.view.opponent_scores().map(|show| show.text(o)),
                        "locked": s.battle().and_then(|b| {
                            let lanes = b.track().lanes();
                            b.feeds().nth(i).map(|f| {
                                f.locks(lanes, clock.heard_now(host_now))
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, l)| l.is_some())
                                    .map(|(lane, _)| lane)
                                    .collect::<Vec<_>>()
                            })
                        }),
                        "combo": o.combo,
                    })).collect::<Vec<_>>(),
                    // The player's life (`0..=1`) and, under DDI, the
                    // score as the HUD's display has it (the sum, what a
                    // room shows and ranks by) with its parts.
                    "life": p.ruleset().gauge.life(),
                    "ddi_score": p.ruleset().score.view().ddi.map(|d| json!({
                        "score": d.score,
                        "ratio": d.ratio,
                        "percent": d.percent,
                        "points": d.points,
                    })),
                    "mines_hit": r.tally.mine_hit,
                    // The lanes a mine or a shock locked in the player's
                    // own play (DDI), and the play's rules' options.
                    "locked": (0..p.lanes()).filter(|l| {
                        p.judge().is_locked(*l, clock.heard_now(host_now) - ddi_engine::player::JUDGE_LAG)
                    }).collect::<Vec<_>>(),
                    "ddi": p.ruleset().ddi.map(|o| json!({
                        "difficulty": format!("{:?}", o.difficulty).to_lowercase(),
                        "boos": o.boos,
                        "practice": o.practice,
                    })),
                    // Their miniature fields: each slot (target pixels:
                    // x, y, w, h), its side and the seat it shows, the
                    // miniatures' lane (CSS pixels) and height in lanes,
                    // whether the opponents take turns in the slots.
                    "opponent_fields": self.view.opponent_fields().map(|f| json!({
                        "lane": f.lane,
                        "rows": f.rows,
                        "rotating": f.rotating,
                        "status": f.status,
                        "slots": f.slots.iter().map(|s| json!({
                            "rect": s.rect,
                            "left": s.left,
                            "seat": s.seat,
                        })).collect::<Vec<_>>(),
                    })),
                    "stage_won": s.battle().is_some_and(|b| b.won()),
                    // A stage's battle moments as told (said by a line, or
                    // shown beside the field), and the one showing now.
                    "battle": s.battle_policy().map(|policy| json!({
                        "policy": policy.id,
                        "told": s.battle_told().iter().map(|t| json!({
                            "event": ddi_library::announcer::Line::Battle(t.event).folder(),
                            "said": t.said.map(|l| l.folder()),
                            "shown": t.shown(&policy),
                        })).collect::<Vec<_>>(),
                        "cue": s.battle_cue(host_now.0).map(|(e, _)| {
                            ddi_library::announcer::Line::Battle(e).folder()
                        }),
                        "cue_drawn": self.view.battle_cue_drawn(),
                        // Attack power (M8a): each competitor's, the
                        // checkpoints run of the stage's, the last
                        // events.
                        // M8b: each seat's attacks (launched, received,
                        // checkpoints waited by reason), what is incoming
                        // and outgoing, the warning and waiting line as
                        // drawn, a final attack's draw.
                        "attacks": attacks_debug(
                            s,
                            self.view.attack_drawn(),
                            self.view.links_drawn(),
                        ),
                    })),
                    // A battle's shocks in the play (M8a): their window,
                    // each one's beat, lane and state, the counts.
                    "hazards": s.player.hazards().map(|h| {
                        let (hit, avoided) = h.counts();
                        json!({
                            "window": h.window(),
                            "hit": hit,
                            "avoided": avoided,
                            "mines": h.mines().iter().map(|m| json!([
                                m.tick.beat(), m.lane, format!("{:?}", m.state), m.seconds,
                            ])).collect::<Vec<_>>(),
                        })
                    }),
                    "announcer": announcer_debug(s.announcer_report()),
                    "drift": clock.drift().unwrap_or(0.0),
                    "output_latency": clock.output_latency(),
                    "mean_delta": r.mean_delta,
                    "stddev_delta": r.stddev_delta,
                    "audio": s.audio_status(),
                }),
            );
            // The best play's ghost: how far ahead of it (percentage
            // points, or points).
            if let Some(d) = s.ghost_delta() {
                let (ddi_ui::state::history::GhostDelta::Percent(v)
                | ddi_ui::state::history::GhostDelta::Points(v)) = d;
                debug["ghost"] = json!(v);
            }
        }
        if let Some(g) = self.session.as_ref().and_then(|s| s.gamepads()) {
            let pads: Vec<Value> = g
                .pads()
                .iter()
                .map(|p| {
                    json!({
                        "id": p.id,
                        "standard": p.standard,
                        "stamped_by_pad": p.stats.stamped_by_pad,
                        "stamped_by_poll": p.stats.stamped_by_poll,
                        "report_interval": p.stats.report_interval,
                    })
                })
                .collect();
            debug["pads"] = pads.into();
            debug["pad_polls"] = json!(g.polls_per_second());
            debug["pad_fast"] = json!(g.fast_running());
        }
        debug
    }

    /// The debug overlay's text.
    fn debug_text(&self, host_now: HostTime) -> String {
        let mut text = format!(
            "{} · {:.0} fps · worst {:.1} ms · {}×{}",
            self.backend_name(),
            self.view.fps.fps,
            self.view.fps.worst_last,
            self.gfx.size().0,
            self.gfx.size().1
        );
        if let Some(s) = &self.session {
            let p = &s.player;
            let clock = p.clock();
            let r = p.results_brief();
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
        let movies = &self.view.movies;
        if movies.module_status().is_some() || movies.stats().decoded > 0 {
            let v = movies.stats();
            let (mean, worst) = self.view.video_ms();
            text.push_str(&format!(
                "\nvideo: decoded {} · shown {} · dropped {} · late {} · {} ms a frame (worst {:.1}){}",
                v.decoded,
                v.shown,
                v.dropped,
                v.late,
                format_args!("{mean:.2}"),
                worst,
                match movies.off() {
                    Some(r) => format!(" · turned off: {r:?}"),
                    None => String::new(),
                }
            ));
        }
        if let Some(s) = &self.session {
            text.push('\n');
            text.push_str(&s.audio_status());
        }
        if let Some(g) = self.session.as_ref().and_then(|s| s.gamepads()) {
            let pads = g.pads();
            for p in &pads {
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
            }
            if !pads.is_empty() {
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
        }
        if let Some(d) = self.session.as_ref().and_then(|s| s.devices.as_ref()) {
            text.push_str(&format!(
                "\naudio: {} · display: {}",
                d.audio.label(),
                d.display.label()
            ));
        }
        text
    }

    /// The overlay element's text, and `window.__DDI_DEBUG`.
    fn publish_debug(&self, host_now: HostTime) {
        let Some(window) = web_sys::window() else {
            return;
        };
        if let Some(el) = &self.debug_el {
            el.set_text_content(Some(&self.debug_text(host_now)));
        }
        let debug = crate::web::json(&self.debug_json(host_now));
        let obj: &js_sys::Object = debug.unchecked_ref();
        let set = |k: &str, v: wasm_bindgen::JsValue| {
            let _ = js_sys::Reflect::set(obj, &wasm_bindgen::JsValue::from_str(k), &v);
        };
        // The announcer studio's scene in play: its frame drawn.
        if let Some(f) = &self.framed {
            set("stage", f.debug());
        }
        crate::web::screen::debug_into(obj);
        // What the page's screens on the canvas (the prompt, the results)
        // added stays.
        if let Some(old) = js_sys::Reflect::get(&window, &"__DDI_DEBUG".into())
            .ok()
            .filter(|v| v.is_object())
        {
            for key in [
                "ui",
                "results",
                "prompt",
                "prompt_backdrop",
                "replay",
                "music_seconds",
            ] {
                if let Ok(v) = js_sys::Reflect::get(&old, &key.into())
                    && !v.is_undefined()
                {
                    set(key, v);
                }
            }
        }
        let _ = js_sys::Reflect::set(
            &window,
            &wasm_bindgen::JsValue::from_str("__DDI_DEBUG"),
            &debug,
        );
    }
}

impl Drop for GameLoop {
    fn drop(&mut self) {
        self.end_perf();
        // A pending resize callback must not run into a dropped closure.
        if let Some((observer, _)) = &self._resize {
            observer.disconnect();
        }
        // A scene's picture leaves the 2D pass.
        if let Some(f) = self.framed.as_mut() {
            f.release(&self.gfx);
        }
    }
}

/// Gives the renderer `play`'s textures: the built-in art, and the decoded
/// `images` (by texture index) copied to the GPU, a model's with mipmaps.
/// The images are to be closed after the next submit.
pub(crate) fn upload_noteskin(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    play: &mut ddi_render::noteskin::NoteskinPlay,
    images: &[Option<web_sys::ImageBitmap>],
) {
    let images = images.iter().map(|i| i.as_ref().map(crate::stage::Image));
    ddi_shell::stage::upload_noteskin(renderer, device, queue, play, images);
}

/// `__DDI_DEBUG.announcer`: the lines spoken (folders) and when (context
/// seconds), how many its gate dropped, those due but not decoded yet,
/// and those not in the pack (`docs/plans/announcers.md`).
/// A stage's attacks for `__DDI_DEBUG.battle.attacks` (M8a, M8b): each
/// seat's power and attacks (launched, received, checkpoints waited by
/// reason, incoming and outgoing), the checkpoints run, the warning and
/// waiting line as drawn (`drawn`), a final attack's draw, the last
/// events.
fn attacks_debug(
    s: &PlaySession,
    drawn: (Option<ddi_shell::battle_cue::Cue>, bool),
    links: &[ddi_shell::attack_links::Shown],
) -> Option<Value> {
    let b = s.battle()?;
    let a = b.attacks()?;
    let (run, of) = a.checkpoints();
    let me = b.setup().me;
    let ledger = a.ledger();
    let seat = |p: ddi_room::ParticipantId| {
        let st = a.stats(p);
        json!({
            "participant": p.0,
            "power": a.power(p),
            "launched": st.launched,
            "received": st.received,
            "waited": [st.waited_no_eligible, st.waited_no_safe_section],
            "incoming": ledger.incoming(p).map(|x| x.sender.0),
            "outgoing": ledger.outgoing(p).map(|x| x.recipient.0).collect::<Vec<_>>(),
        })
    };
    let incoming = b.incoming(&s.player).map(|i| {
        json!({
            "shocks": i.shocks,
            "committed": i.committed,
            "first": i.first,
            "clear": i.clear,
            "more": i.more,
        })
    });
    let cue = drawn.0.map(|c| {
        json!({
            "shocks": c.shocks,
            "more": c.more,
            "dim": c.dim,
            "scale": c.scale,
        })
    });
    Some(json!({
        "mine": a.power(me),
        "shown": b.my_power(&s.player).map(|(p, c)| json!([p, c])),
        "me": seat(me),
        "power": b.setup().cpus.iter().map(|c| seat(c.participant)).collect::<Vec<_>>(),
        // Each CPU's shocks stepped on, as its field was told (in seat
        // order).
        "cpu_hits": b.feeds().map(|f| f.moments().iter().filter(|m| {
            m.kind == ddi_session::battle::feed::MomentKind::ShockHit
        }).count()).collect::<Vec<_>>(),
        "checkpoints": [run, of],
        "attacks": ledger.attacks().len(),
        "incoming": incoming,
        "cue": cue,
        "waiting_drawn": drawn.1,
        "waiting": b.waiting(&s.player).map(|w| format!("{:?} {:?}", w.targeting, w.reason)),
        // Who attacks whom as drawn for the player's own attacks: each
        // link's attack, `from` and `to` (`"me"` or an opponent's seat),
        // its `phase` (`pulse`, `fade`, `still`, `outline`), whether it
        // is `chained`, its line's and outline's opacity and the line's
        // corners; and the seats kept in view where the opponents take
        // turns.
        "links": ddi_shell::attack_links::debug(links),
        "kept_seats": b.kept_seats(&s.player, f64::from(ddi_shell::opponents::ROTATION_V0.every)),
        // The player's attack policy as its label says it (none once
        // out or after a win), where a press on it lands (CSS pixels),
        // the policy the ledger launches by, and the changes the play's
        // log keeps.
        "policy": b.policy(&s.player).map(|p| format!("{p:?}").to_lowercase()),
        "policy_at": crate::web::touch::policy_at(),
        "policy_ledger": format!("{:?}", ledger.targeting(me)).to_lowercase(),
        "policy_changes": s.player.log().policies.iter().map(|c| json!([c.time, c.policy])).collect::<Vec<_>>(),
        "settling": b.settling().map(|x| x.survivor.0),
        "drawn_final": b.drawn_by_final_attack(),
        "refused": b.refused(),
        "events": a.recent().map(|e| format!("{e:?}")).collect::<Vec<_>>(),
    }))
}

fn announcer_debug(a: &ddi_session::announcer::Report) -> Value {
    json!({
        "spoken": a.spoken.iter().map(|(f, _)| f).collect::<Vec<_>>(),
        "at": a.spoken.iter().map(|(_, t)| t).collect::<Vec<_>>(),
        "missing": a.missing,
        "absent": a.absent,
        "dropped": a.dropped,
    })
}

/// Adds the members of `more` to the object `debug`.
fn extend(debug: &mut Value, more: Value) {
    if let (Value::Object(to), Value::Object(from)) = (debug, more) {
        to.extend(from);
    }
}
