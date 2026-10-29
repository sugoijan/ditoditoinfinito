//! Per-frame driver owned by the gameplay canvas: sizes the canvas, ticks the
//! play session (when one is running) and renders it.

use std::cell::Cell;
use std::rc::Rc;

use crate::component::Callback;
use ddi_library::backgrounds::{self, BgImage, BgSegment, LAYERS};
use ddi_platform::HostTime;
use ddi_platform::video::{GuardAction, StopReason};
use ddi_render::{Backdrop, Renderer};
use ddi_session::backgrounds::{FpsMeter, FrameShape, MovieGuard};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlCanvasElement, HtmlElement, ResizeObserver, ResizeObserverEntry};

use crate::play::{PlaySession, SessionEvent};
use crate::settings::{BackgroundFrame, VideoMode};
use crate::web::gfx::Gfx;
use ddi_engine::lyrics::{LyricLine, LyricTrack};

/// The lyrics' lines of a frame, and where they go.
type Lyrics = (Vec<LyricLine>, ddi_render::lyrics::Place);

/// The 2D pass over a play: its painter (in the surface's format) and
/// what it draws.
struct Overlay {
    painter: ddi_render::painter::Painter,
    list: ddi_render::DrawList,
}

/// The observer and the closure it calls, kept alive together and
/// disconnected before the closure is dropped (see `Drop`).
type ResizeHook = (ResizeObserver, Closure<dyn FnMut(js_sys::Array)>);

pub(crate) const FONT: &[u8] =
    include_bytes!("../../../assets/fonts/IosevkaCustom-ExtraBold-latin.ttf");

pub(crate) struct GameLoop {
    /// The app's device (`web::screen`), shared.
    gfx: Rc<Gfx>,
    canvas: HtmlCanvasElement,
    renderer: Renderer,
    session: Option<PlaySession>,
    entrance: Option<ddi_actor::background::Entrance>,
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
    /// The song's background changes, a schedule per layer; empty outside
    /// a song.
    background_schedule: [Vec<BgSegment>; LAYERS],
    /// What the last frame drew behind the field (for the debug state).
    backdrop_drawn: Backdrop,
    /// The song's scripted backgrounds and foregrounds
    /// (`docs/plans/scripted-backgrounds.md`), why they stopped, and
    /// the quads they drew in the last frame.
    scripts: crate::scripts::ScriptView,
    /// The scripts' texts the browser drew, waiting for the next frame.
    script_texts: ScriptTexts,
    /// The scripts' sounds decoded until their buffers are made (on the
    /// play's sound, the first time each plays), the buffers, those played
    /// this frame, and how many played so far.
    script_sounds: std::collections::HashMap<String, ddi_platform::decode::DecodedAudio>,
    sound_buffers: std::collections::HashMap<String, web_sys::AudioBuffer>,
    sounds_due: Vec<String>,
    sounds_played: usize,
    scripts_stopped: Option<String>,
    scripts_drawn: (usize, usize),
    /// The chart's lyrics, if it has any and they are shown.
    lyrics: Option<LyricTrack>,
    /// The 2D pass over the play (the lyrics), made when first needed, and
    /// the lines it drew last.
    overlay: Option<Overlay>,
    lyrics_shown: Vec<String>,
    /// A work's back and mask layers: the song's decoded contents, the
    /// play's timeline.
    stage: crate::stage::StageView,
    /// Back and mask quads of the last frame (for the debug state).
    stage_drawn: (usize, usize),
    /// The song's background movies.
    movies: crate::movies::MovieDeck,
    /// Image and movie textures for the backdrop, rebuilt when either
    /// changes.
    shown_cache: ddi_session::movies::BackdropTextures,
    /// The movies were stopped at the end of the song.
    movies_stopped: bool,
    video_mode: VideoMode,
    /// The automatic setting's rule, and the refresh rate measured
    /// before the play started (`ddi_session::backgrounds`).
    guard: MovieGuard,
    /// Testing aid: each frame takes 40 ms longer while a movie is drawn.
    slow: bool,
    /// The shape backgrounds are fitted into (`ddi_session::backgrounds`).
    frame_shape: FrameShape,
    /// Main-thread time the movies took this second (sum, longest, frames
    /// they ran in), ms, and the last full second's (mean, longest): what
    /// the debug overlay shows of the movie's cost.
    video_ms: (f64, f64, u32),
    video_ms_last: (f64, f64),
    /// The noteskin of this play, if one is chosen and fits the layout.
    noteskin: Option<ddi_render::noteskin::NoteskinPlay>,
    /// A noteskin readied while a play was on: used from the next play.
    next_noteskin: Option<(
        ddi_render::noteskin::NoteskinPlay,
        Vec<Option<web_sys::ImageBitmap>>,
        String,
    )>,
    /// The noteskin's store key.
    noteskin_key: String,
}

/// Texts the browser drew for a song's scripts: `(text, drawing, size in
/// StepMania's pixels)`.
type ScriptTexts = std::rc::Rc<std::cell::RefCell<Vec<(String, crate::stage::Bitmap, [f32; 2])>>>;

impl GameLoop {
    /// `skins`: the player draws layouts with a skin (not the
    /// built-in arrows, chosen to be lighter).
    pub(crate) fn new(
        gfx: Rc<Gfx>,
        canvas: HtmlCanvasElement,
        on_event: Callback<SessionEvent>,
        skins: bool,
    ) -> GameLoop {
        let renderer = Renderer::new(
            &gfx.device,
            &gfx.queue,
            gfx.view_format(),
            gfx.skin_blending(skins),
            &[FONT],
        );
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
            entrance: None,
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
            background_schedule: Default::default(),
            backdrop_drawn: Backdrop::default(),
            scripts: Default::default(),
            script_texts: Default::default(),
            script_sounds: Default::default(),
            sound_buffers: Default::default(),
            sounds_due: Vec::new(),
            sounds_played: 0,
            scripts_stopped: None,
            scripts_drawn: (0, 0),
            lyrics: None,
            overlay: None,
            lyrics_shown: Vec::new(),
            stage: crate::stage::StageView::default(),
            stage_drawn: (0, 0),
            movies: crate::movies::MovieDeck::default(),
            shown_cache: Default::default(),
            movies_stopped: false,
            video_mode: VideoMode::Auto,
            guard: MovieGuard::default(),
            slow: false,
            frame_shape: FrameShape::default(),
            video_ms: (0.0, 0.0, 0),
            video_ms_last: (0.0, 0.0),
            noteskin: None,
            next_noteskin: None,
            noteskin_key: String::new(),
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

    pub(crate) fn enter_background(&mut self, from: ddi_actor::background::Reveal, reduced: bool) {
        self.entrance = Some(ddi_actor::background::Entrance::resume(from, reduced));
    }

    pub(crate) fn set_session(&mut self, session: PlaySession) {
        // Everything decoded so far goes up before the music starts.
        self.playing_at = None;
        self.guard.start(self.refresh_hz());
        self.upload_backgrounds();
        if let Some((play, images, key)) = self.next_noteskin.take() {
            self.install_noteskin(Some(play), images, key);
        }
        // The layers' textures go up with the skin's, after them.
        self.stage
            .upload(&mut self.renderer, &self.gfx.device, &self.gfx.queue);
        // Without a skin (another layout, the chart's own colors) or
        // layers drawn as skins are, frames need not be drawn offscreen
        // for them (WebGL2).
        self.renderer.set_skins_drawn(
            &self.gfx.device,
            &self.gfx.queue,
            self.noteskin.is_some() || self.stage.shown() || self.scripts.running(),
        );
        // A retry plays the noteskin's commands from the start again,
        // and the song's scripts.
        if let Some(n) = self.noteskin.as_mut() {
            n.restart();
        }
        self.scripts.restart();
        self.renderer.reset_hud();
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
        let count = backgrounds::uploads_due(
            &self.background_schedule,
            self.playing_at,
            self.pending_backgrounds.len(),
        );
        let batch: Vec<_> = self.pending_backgrounds.drain(..count).collect();
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
        let queue = &self.gfx.queue;
        let id = self
            .renderer
            .add_background_image(&self.gfx.device, [width, height], |texture| {
                crate::web::gfx::copy_bitmap(queue, &image, texture, width, height);
            });
        match what {
            BgImage::Song => self.frame_shape.song_bg_aspect = Some(width as f32 / height as f32),
            BgImage::Still => self.frame_shape.still_aspect = Some(width as f32 / height as f32),
            _ => {}
        }
        self.background_ids.push((what, id));
        self.uploaded_images.push(image);
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
        self.movies.add_source(name, blob, format, probe);
    }

    /// The video settings of the plays to come; `slow` is the testing aid
    /// that forces the automatic rule.
    pub(crate) fn set_video(&mut self, mode: VideoMode, frame: BackgroundFrame, slow: bool) {
        self.video_mode = mode;
        self.frame_shape.setting = frame;
        self.slow = slow;
    }

    /// Why the automatic setting turned the movies off this play.
    pub(crate) fn video_stopped(&self) -> Option<StopReason> {
        self.movies.off()
    }

    /// What kept movies from playing this play, for the results.
    pub(crate) fn video_trouble(&self) -> Option<String> {
        self.movies.trouble()
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
        let movies = &mut self.movies;
        let shown_any = movies.shown_any();
        self.frame_shape
            .aspect(|| movies.picture_aspect(), shown_any, started)
    }

    /// The background changes of the song about to play, a schedule per
    /// layer (empty: none).
    pub(crate) fn set_background_schedule(&mut self, layers: [Vec<BgSegment>; LAYERS]) {
        self.background_schedule = layers;
    }

    /// What to show behind the field at song second `t` (fields passed
    /// one by one, so a session can stay borrowed).
    fn backdrop(
        cache: &mut ddi_session::movies::BackdropTextures,
        images: &[(BgImage, usize)],
        movies: &crate::movies::MovieDeck,
        layers: &[Vec<BgSegment>; LAYERS],
        t: f64,
        black: Option<usize>,
    ) -> Backdrop {
        let all = cache.get(
            images,
            movies.version(),
            || movies.shown_ids(),
            ddi_render::PLAIN,
            black,
        );
        let opening = movies.opening_plays(&layers[0]);
        let [first, second] = backgrounds::draws_at(layers, t, all, opening);
        Backdrop { first, second }
    }

    /// The back and mask layers of the play about to start (`None`: none
    /// shown).
    pub(crate) fn set_stage_track(&mut self, track: Option<ddi_engine::sprites::SpriteTrack>) {
        self.stage.set_track(track);
        self.renderer.set_stage_quads(Vec::new(), Vec::new());
    }

    /// The song's scripts, loaded (`None`: none, or they failed).
    pub(crate) fn set_scripts(&mut self, loaded: Option<crate::scripts::Loaded>) {
        self.scripts_stopped = None;
        self.sound_buffers.clear();
        self.sounds_due.clear();
        match loaded {
            Some(l) => {
                self.script_sounds = l.sounds.into_iter().collect();
                self.scripts.set(Some(l.scripts), l.screen, l.images);
            }
            None => {
                self.script_sounds.clear();
                self.scripts.set(None, [854.0, 480.0], Vec::new());
            }
        }
        self.script_texts.borrow_mut().clear();
        // Their text as Init left it, drawn while the prompt shows.
        Self::draw_script_texts(&mut self.scripts, &self.script_texts);
    }

    /// The scripts' sounds played this frame, at once on the play's
    /// sound at its volume (a buffer made the first time each plays).
    fn play_script_sounds(&mut self) {
        use ddi_platform::AudioBackend;
        let Some(session) = self.session.as_mut() else {
            self.sounds_due.clear();
            return;
        };
        for f in self.sounds_due.drain(..) {
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

    /// The scripts' texts the browser drew so far go to them; the texts
    /// they show that are not drawn yet are drawn (white, as a work's
    /// layers' text is), arriving in a later frame.
    fn draw_script_texts(scripts: &mut crate::scripts::ScriptView, inbox: &ScriptTexts) {
        for (text, bitmap, size) in inbox.borrow_mut().drain(..) {
            scripts.add_text(text, bitmap, size);
        }
        for text in scripts.measure() {
            let inbox = inbox.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let size = ddi_session::scripted::TEXT_SIZE;
                let scale = f64::from(ddi_session::stage::TEXT_SCALE);
                match crate::web::image::text_bitmap(&text, size, "#ffffff", scale).await {
                    Ok((bitmap, (w, h))) => {
                        inbox
                            .borrow_mut()
                            .push((text, crate::stage::Bitmap(bitmap), [w, h]))
                    }
                    Err(e) => web_sys::console::warn_1(&e.into()),
                }
            });
        }
    }

    /// The foregrounds' times for the play about to start (the song
    /// offset moves them).
    pub(crate) fn retime_scripts(&mut self, foregrounds: &[ddi_library::backgrounds::Foreground]) {
        self.scripts.retime(foregrounds);
    }

    /// Why the song's scripts stopped during the play, if they did.
    pub(crate) fn scripts_stopped(&self) -> Option<&str> {
        self.scripts_stopped.as_deref()
    }

    /// A content of the song's layers decoded.
    pub(crate) fn add_stage_content(
        &mut self,
        source: ddi_engine::sprites::SpriteSource,
        bitmap: web_sys::ImageBitmap,
        size: (f32, f32),
    ) {
        self.stage.add(source, crate::stage::Bitmap(bitmap), size);
    }

    /// The lyrics of the song about to play (`None`: none shown).
    pub(crate) fn set_lyrics(&mut self, lyrics: Option<LyricTrack>) {
        self.lyrics = lyrics;
        self.lyrics_shown.clear();
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
        // Mid-play the arrows do not change: from the next play on.
        if self.session.is_some() {
            if let Some(play) = play
                && let Some((_, old, _)) = self.next_noteskin.replace((play, images, key))
            {
                old.into_iter().flatten().for_each(|b| b.close());
            }
            return;
        }
        self.install_noteskin(play, images, key);
    }

    fn install_noteskin(
        &mut self,
        play: Option<ddi_render::noteskin::NoteskinPlay>,
        images: Vec<Option<web_sys::ImageBitmap>>,
        key: String,
    ) {
        self.noteskin_key = key;
        self.renderer.clear_noteskin_textures();
        self.stage.forget_textures();
        self.scripts.forget_textures();
        // Colors as the target blends them: encoded when skins blend as
        // StepMania's do.
        let srgb = self.renderer.skin_colors_linear();
        self.noteskin = play.map(|mut play| {
            play.set_srgb(srgb);
            self.uploaded_images.extend(upload_noteskin(
                &mut self.renderer,
                &self.gfx.device,
                &self.gfx.queue,
                &mut play,
                &images,
            ));
            play
        });
    }

    /// The canvas back on the play's drawing (after a screen drew on it
    /// with the 2D pass alone).
    pub(crate) fn scene_mode(&self) {
        self.gfx.set_mode(crate::web::gfx::SurfaceMode::Scene);
    }

    /// A frame while the start prompt (`ddi_ui::screens::prompt`, the
    /// page's) draws on the canvas: nothing drawn here, but the refresh
    /// rate is measured (the devices' fingerprint and the video rule read
    /// it), decoded backgrounds go up to the GPU, and the debug state is
    /// kept.
    pub(crate) fn idle(&mut self, time_ms: f64) {
        if self.gfx.is_lost() {
            return;
        }
        let _ = self.fps.tick(time_ms);
        self.upload_backgrounds();
        self.frames += 1;
        if self.frames.is_multiple_of(10) {
            self.publish_debug(HostTime(time_ms / 1000.0));
        }
    }

    /// The background frame's shape as the play would start with it
    /// (`None`: the whole screen), for the start prompt's picture.
    pub(crate) fn prompt_frame(&mut self) -> Option<f32> {
        self.frame_aspect(false)
    }

    pub(crate) fn clear_session(&mut self) {
        self.session = None;
        self.lyrics = None;
        self.stage.set_track(None);
        self.renderer.set_stage_quads(Vec::new(), Vec::new());
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
        self.fit_canvas();
        let second = self.fps.tick(time_ms).map(|fps| self.fps.second(fps));
        if second.is_some() {
            let (sum, max, n) = std::mem::take(&mut self.video_ms);
            self.video_ms_last = if n > 0 {
                (sum / f64::from(n), max)
            } else {
                (0.0, 0.0)
            };
        }
        self.upload_backgrounds();
        let drawing = self.movies.drawing(&self.background_schedule);
        // As a movie that costs frames would: not while it is paused.
        if self.slow && drawing.is_some() && !self.movies.paused() {
            let now = crate::web::clock::PerformanceClock::now_ms;
            let until = now() + 40.0;
            while now() < until {}
        }
        let started = self.session.as_ref().is_some_and(|s| s.player.started());
        let frame_aspect = self.frame_aspect(started);
        let host_now = HostTime(time_ms / 1000.0);

        let mut outcome = None;
        let mut lyrics: Option<Lyrics> = None;
        if let Some(session) = self.session.as_mut()
            && !self.done
        {
            let (_events, event) = session.tick(host_now);
            // A fail is told at once (the play goes on to its end).
            if let Some(failed) = session.take_fail_report() {
                self.on_event.emit(failed);
            }
            outcome = event;
        }

        let Some(surface) = self.gfx.frame() else {
            return;
        };
        let view = surface.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.gfx.view_format()),
            ..Default::default()
        });
        // Skins' quads blend as StepMania's do, through the plain view.
        let plain_view = self.gfx.plain_format().map(|format| {
            surface.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(format),
                ..Default::default()
            })
        });
        let mut encoder = self
            .gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        match self.session.as_ref() {
            Some(session) => {
                let frame = session.frame(session.predicted_present(host_now));
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
                    let before = crate::web::clock::PerformanceClock::now_ms();
                    self.movies.update(
                        &self.background_schedule,
                        frame.song_time,
                        &mut self.renderer,
                        &self.gfx.device,
                        &self.gfx.queue,
                    );
                    if drawing.is_some() {
                        let spent = crate::web::clock::PerformanceClock::now_ms() - before;
                        let (sum, max, n) = &mut self.video_ms;
                        *sum += spent;
                        *max = max.max(spent);
                        *n += 1;
                    }
                    if self.video_mode == VideoMode::Auto {
                        let refresh = self.refresh_hz();
                        let late = self.movies.stats().late;
                        match self.guard.step(time_ms, drawing, second, refresh, late) {
                            Some(GuardAction::Pause) => self.movies.pause(),
                            Some(GuardAction::Resume) => self.movies.resume(),
                            Some(GuardAction::Off(reason)) => self.movies.turn_off(reason),
                            None => {}
                        }
                    }
                }
                // The field's place on the canvas this frame, and its
                // pixels to a CSS pixel.
                let css = self.css_size.get();
                let (width, height) = self.gfx.size();
                let ratio = width as f32 / (css.0 as f32).max(1.0);
                let span = ddi_render::scene::field_span(&session.layout);
                let geo = ddi_render::FieldGeometry::new(
                    width as f32,
                    height as f32,
                    span,
                    session.render.reverse,
                );
                // The touch screens' quit button, at the canvas's pixels,
                // clear of the field's receptors and the safe area.
                let quit =
                    crate::web::touch::quit_shown(crate::web::clock::PerformanceClock::now_ms())
                        .then(|| {
                            let insets = crate::web::touch::safe_insets(css);
                            ddi_render::scene::quit_button(&geo, &session.layout, ratio, insets)
                        });
                // Where it is (nowhere while it is not: no press lands on
                // it).
                crate::web::touch::set_quit_at(quit.map(|b| b.map(|v| v / ratio)));
                self.renderer.set_quit_button(quit);
                let mut render = session.render;
                render.hud_time = Some(host_now.0);
                render.frame_aspect = frame_aspect;
                if let Some(entrance) = &mut self.entrance {
                    let reveal = entrance.sample(host_now.0);
                    render.background_reveal = Some(reveal);
                    if reveal == ddi_actor::background::Reveal::default() {
                        self.entrance = None;
                    }
                }
                render.backdrop = Self::backdrop(
                    &mut self.shown_cache,
                    &self.background_ids,
                    &self.movies,
                    &self.background_schedule,
                    frame.song_time,
                    // Black under the scripts while they run.
                    self.scripts.running().then_some(ddi_render::BLACK),
                );
                self.backdrop_drawn = render.backdrop;
                self.playing_at = (session.player.started() && !session.player.finished())
                    .then_some(frame.song_time);
                // The lyrics at this frame's time, over the field's middle.
                if let (Some(track), Some(t)) = (self.lyrics.as_ref(), self.playing_at) {
                    let lines = track.at(t);
                    if !lines.is_empty() {
                        let place = ddi_render::lyrics::Place::for_play(
                            &geo,
                            &session.layout,
                            track.stage(),
                            quit,
                            ratio,
                        );
                        lyrics = Some((lines, place));
                    }
                }
                let layer = match self.noteskin.as_mut() {
                    Some(play) => {
                        let from = play.next_seq();
                        play.update(
                            &frame,
                            session.player.skin_events_since(from),
                            render.reverse,
                        );
                        // The skin's Lua stopped: the play goes on with
                        // the built-in skin; the next one tries again.
                        if let Some((why, on_clock)) = play.take_lua_stop() {
                            let (key, why) = (self.noteskin_key.clone(), why.to_string());
                            if on_clock {
                                // A stall, not the skin: not held against it.
                                web_sys::console::warn_1(&format!("noteskin {key}: {why}").into());
                            } else {
                                wasm_bindgen_futures::spawn_local(async move {
                                    crate::noteskins::lua_stopped(&key, &why).await;
                                });
                            }
                        }
                        Some(play as &mut dyn ddi_render::noteskin::NoteLayer)
                    }
                    None => None,
                };
                let scripts = self.scripts.running();
                if self.stage.shown() || scripts {
                    let srgb = self.renderer.skin_colors_linear();
                    let (mut back, mut mask) = (Vec::new(), Vec::new());
                    if self.stage.shown() {
                        let stage = &mut self.stage;
                        // Images decoded late go up as they come.
                        if stage.pending() {
                            stage.upload(&mut self.renderer, &self.gfx.device, &self.gfx.queue);
                        }
                        (back, mask) =
                            stage.quads(frame.song_time, geo.center_x(span), height as f32, srgb);
                        self.stage_drawn = (back.len(), mask.len());
                    }
                    // The song's scripts, in the background frame (they
                    // load while the play may have begun).
                    if scripts {
                        let texts = &self.script_texts;
                        let drawn = self.scripts.draw(
                            (&mut self.renderer, &self.gfx.device, &self.gfx.queue),
                            (frame.song_time, frame.beat),
                            &self.background_schedule,
                            ([width as f32, height as f32], frame_aspect),
                            layer.is_some() || self.stage.shown(),
                            |s| Self::draw_script_texts(s, texts),
                            (&mut back, &mut mask),
                        );
                        if let Some(why) = drawn.stopped {
                            web_sys::console::warn_1(
                                &format!("scripted backgrounds stopped: {why}").into(),
                            );
                            self.scripts_stopped = Some(why);
                        }
                        self.sounds_due.extend(drawn.sounds);
                        self.scripts_drawn = drawn.drawn;
                    }
                    self.renderer.set_stage_quads(back, mask);
                }
                self.renderer.render(
                    &self.gfx.device,
                    &self.gfx.queue,
                    &mut encoder,
                    &view,
                    plain_view.as_ref(),
                    width,
                    height,
                    &frame,
                    &session.layout,
                    &session.names,
                    render,
                    layer,
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
        self.play_script_sounds();
        // Over the play: the 2D pass (the lyrics, with the browser's text).
        let overlaid = match lyrics {
            Some(l) => self.draw_overlay(&mut encoder, &view, plain_view.as_ref(), l),
            None => {
                self.lyrics_shown.clear();
                false
            }
        };
        self.gfx.queue.submit(Some(encoder.finish()));
        if overlaid && let Some(o) = self.overlay.as_mut() {
            o.painter.submitted();
        }
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

    /// Draws the lyrics' lines through the 2D pass over the frame just
    /// encoded: through the surface's plain view where it has one
    /// (WebGPU: blended encoded, as the menus), else through the sRGB
    /// one (WebGL2, `Painter`'s linear target). Whether it drew.
    fn draw_overlay(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        plain_view: Option<&wgpu::TextureView>,
        (lines, place): Lyrics,
    ) -> bool {
        let format = self
            .gfx
            .plain_format()
            .unwrap_or_else(|| self.gfx.view_format());
        let target = plain_view.unwrap_or(view);
        if self
            .overlay
            .as_ref()
            .is_none_or(|o| o.painter.format() != format)
        {
            let mut painter =
                ddi_render::painter::Painter::new(&self.gfx.device, &self.gfx.queue, format);
            match crate::web::text::CanvasRasterizer::new(self.gfx.is_gl()) {
                Ok(r) => painter.set_rasterizer(Box::new(r)),
                Err(e) => {
                    web_sys::console::warn_1(&format!("lyrics: no text: {e}").into());
                    return false;
                }
            }
            self.overlay = Some(Overlay {
                painter,
                list: ddi_render::DrawList::new(),
            });
        }
        let Some(o) = self.overlay.as_mut() else {
            return false;
        };
        o.list.clear();
        {
            let mut measure = o.painter.measure(1.0);
            ddi_render::lyrics::draw(&mut o.list, &lines, &place, &mut measure);
        }
        let (w, h) = self.gfx.size();
        o.painter.draw(
            &self.gfx.device,
            &self.gfx.queue,
            encoder,
            target,
            [w, h],
            1.0,
            None,
            &o.list,
            &mut |_, _| {},
        );
        self.lyrics_shown = lines
            .iter()
            .filter(|l| l.opacity > 0.0)
            .map(|l| l.text.to_string())
            .collect();
        true
    }

    /// The last frame's background quads, each layer's back to front:
    /// `{layer, file, add, place, alpha}` (`file` as the schedule names it).
    fn backdrop_debug(&self) -> js_sys::Array {
        let out = js_sys::Array::new();
        let layers = [&self.backdrop_drawn.first, &self.backdrop_drawn.second];
        for (layer, draws) in layers.into_iter().enumerate() {
            for d in draws.iter().flatten() {
                let o = js_sys::Object::new();
                let file = self
                    .background_ids
                    .iter()
                    .find(|(_, id)| *id == d.id)
                    .map(|(what, _)| match what {
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
                for (k, v) in [
                    ("layer", wasm_bindgen::JsValue::from(layer as u32)),
                    ("file", file.as_str().into()),
                    ("add", d.add.into()),
                    ("place", place.into()),
                    ("alpha", f64::from(d.color[3]).into()),
                    ("crop_top", f64::from(d.crop[1]).into()),
                ] {
                    let _ = js_sys::Reflect::set(&o, &k.into(), &v);
                }
                out.push(&o);
            }
        }
        out
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
        // The lyrics drawn over the play (their texts).
        let lyrics = js_sys::Array::new();
        for l in &self.lyrics_shown {
            lyrics.push(&l.as_str().into());
        }
        set("lyrics", lyrics.into());
        set("backdrop", self.backdrop_debug().into());
        // The song's scripts: whether they run, why they stopped, the fuel
        // of the last frame, the quads drawn (background, foreground).
        let scripts = js_sys::Object::new();
        for (k, v) in [
            (
                "running",
                wasm_bindgen::JsValue::from(self.scripts.running()),
            ),
            (
                "stopped",
                self.scripts_stopped
                    .as_deref()
                    .map_or(wasm_bindgen::JsValue::NULL, |s| s.into()),
            ),
            ("fuel", (self.scripts.frame_fuel() as f64).into()),
            ("back", (self.scripts_drawn.0 as u32).into()),
            ("sounds", (self.sounds_played as u32).into()),
            ("front", (self.scripts_drawn.1 as u32).into()),
        ] {
            let _ = js_sys::Reflect::set(&scripts, &k.into(), &v);
        }
        set("scripts", scripts.into());
        let video = self.movies.debug();
        let _ = js_sys::Reflect::set(&video, &"update_ms".into(), &self.video_ms_last.0.into());
        let _ = js_sys::Reflect::set(
            &video,
            &"update_ms_max".into(),
            &self.video_ms_last.1.into(),
        );
        let _ = js_sys::Reflect::set(&video, &"guard".into(), &self.guard.guard.state().into());
        set("video", video.into());
        set(
            "noteskin",
            match &self.noteskin {
                Some(n) => {
                    let o = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&o, &"name".into(), &n.name.as_str().into());
                    let notes: js_sys::Array = n
                        .notes
                        .iter()
                        .map(|s| wasm_bindgen::JsValue::from_str(s))
                        .collect();
                    let _ = js_sys::Reflect::set(&o, &"notes".into(), &notes);
                    let problems: js_sys::Array = n
                        .problems()
                        .iter()
                        .map(|s| wasm_bindgen::JsValue::from_str(s))
                        .collect();
                    let _ = js_sys::Reflect::set(&o, &"problems".into(), &problems);
                    let _ =
                        js_sys::Reflect::set(&o, &"quads".into(), &(n.last_quads() as u32).into());
                    // What its quads cost the last frame (`noteskin_cost`
                    // measures the same on desktop).
                    let stats = self.renderer.quad_stats();
                    let gpu = js_sys::Object::new();
                    for (key, value) in [
                        ("instances", stats.quads),
                        ("runs", stats.runs),
                        ("stencil_clears", stats.stencil_clears),
                        ("material_quads", stats.material_quads),
                        ("pipelines_made", stats.pipelines_made),
                    ] {
                        let _ = js_sys::Reflect::set(&gpu, &key.into(), &value.into());
                    }
                    let _ = js_sys::Reflect::set(&o, &"gpu".into(), &gpu);
                    // The parameters' values its Lua reads.
                    let _ = js_sys::Reflect::set(
                        &o,
                        &"params".into(),
                        &crate::web::json(&n.parameters()),
                    );
                    let (fuel, max_fuel) = n.lua_fuel();
                    let lua = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&lua, &"runs".into(), &n.runs_lua().into());
                    let _ = js_sys::Reflect::set(&lua, &"fuel".into(), &(fuel as f64).into());
                    let _ =
                        js_sys::Reflect::set(&lua, &"max_fuel".into(), &(max_fuel as f64).into());
                    let _ = js_sys::Reflect::set(
                        &lua,
                        &"stopped".into(),
                        &n.lua_failure()
                            .map_or(wasm_bindgen::JsValue::NULL, |f| f.into()),
                    );
                    let _ = js_sys::Reflect::set(&o, &"lua".into(), &lua);
                    o.into()
                }
                None => wasm_bindgen::JsValue::NULL,
            },
        );
        set("backend", self.backend_name().into());
        set("srgb_view", self.gfx.view_format().is_srgb().into());
        set("skins_encoded", self.renderer.skins_blend_encoded().into());
        set("offscreen", self.renderer.offscreen().into());
        set("fps", self.fps.fps.into());
        // HUD runs shaped so far (a new string or size each).
        set("text_shaped", (self.renderer.text_shaped() as f64).into());
        // The song's opening still is loaded (what the start prompt shows
        // is the page's, `prompt_backdrop`).
        set(
            "still",
            self.background_ids
                .iter()
                .any(|(w, _)| *w == BgImage::Still)
                .into(),
        );
        let mut text = format!(
            "{} · {:.0} fps · worst {:.1} ms · {}×{}",
            self.backend_name(),
            self.fps.fps,
            self.fps.worst_last,
            self.gfx.size().0,
            self.gfx.size().1
        );
        if let Some(s) = &self.session {
            let p = &s.player;
            let clock = p.clock();
            let r = p.results_brief();
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
            set("stage_back", (self.stage_drawn.0 as f64).into());
            set("stage_mask", (self.stage_drawn.1 as f64).into());
            set("ticks_hit", (r.tally.checkpoint_hit as f64).into());
            set("ticks_missed", (r.tally.checkpoint_miss as f64).into());
            set("holds_ticked", p.ruleset().judge.hold.is_ticked().into());
            let pressed = js_sys::Array::new();
            for &h in s.held() {
                pressed.push(&h.into());
            }
            set("pressed", pressed.into());
            // The quit button's place while it shows (CSS pixels: x, y,
            // radius), else null.
            set(
                "quitButton",
                match crate::web::touch::quit_at() {
                    Some(b) => {
                        js_sys::Array::from_iter(b.iter().map(|&v| wasm_bindgen::JsValue::from(v)))
                            .into()
                    }
                    None => wasm_bindgen::JsValue::NULL,
                },
            );
            set("finished", p.finished().into());
            set("failed", p.failed().into());
            // The lane covers now ("sudden+ 30%"; null when off).
            set(
                "cover",
                p.cover()
                    .label()
                    .map_or(wasm_bindgen::JsValue::NULL, |l| l.into()),
            );
            // The assist claps scheduled and the mine sounds played.
            let (claps, mines) = s.event_sounds();
            set("claps", f64::from(claps).into());
            set("mine_sounds", f64::from(mines).into());
            // A replay's edges fed so far (`__DDI_DEBUG.replay`).
            if let Some((fed, edges)) = s.replay_progress() {
                crate::replay::progress(fed, edges);
            }
            // The best play's ghost: how far ahead of it (percentage
            // points, or points).
            if let Some(d) = s.ghost_delta() {
                let v = match d {
                    ddi_ui::state::history::GhostDelta::Percent(p)
                    | ddi_ui::state::history::GhostDelta::Points(p) => p,
                };
                set("ghost", v.into());
            }
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
        if self.movies.module_status().is_some() || self.movies.stats().decoded > 0 {
            let v = self.movies.stats();
            text.push_str(&format!(
                "\nvideo: decoded {} · shown {} · dropped {} · late {} · {} ms a frame (worst {:.1}){}",
                v.decoded,
                v.shown,
                v.dropped,
                v.late,
                format_args!("{:.2}", self.video_ms_last.0),
                self.video_ms_last.1,
                match self.movies.off() {
                    Some(r) => format!(" · turned off: {r:?}"),
                    None => String::new(),
                }
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
        crate::web::screen::debug_into(&obj);
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

/// Gives the renderer `play`'s textures: the built-in art, and the decoded
/// `images` (by texture index) copied to the GPU, a model's with mipmaps.
/// Returns the images copied, to be closed when the play ends.
pub(crate) fn upload_noteskin(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    play: &mut ddi_render::noteskin::NoteskinPlay,
    images: &[Option<web_sys::ImageBitmap>],
) -> Vec<web_sys::ImageBitmap> {
    let mut uploaded = Vec::new();
    let keys = play.texture_keys().to_vec();
    for (i, key) in keys.iter().enumerate() {
        let id = match (key, images.get(i).cloned().flatten()) {
            (ddi_noteskin::build::TextureKey::BuiltIn(name), _) => {
                renderer.add_builtin_art(device, queue, name)
            }
            (_, Some(image)) => {
                let (w, h) = (image.width().max(1), image.height().max(1));
                // A model's texture is mipmapped, as StepMania's are.
                let mipmaps = play.texture_mipmaps(i);
                let id = renderer.add_noteskin_image(device, queue, [w, h], mipmaps, |texture| {
                    crate::web::gfx::copy_bitmap(queue, &image, texture, w, h);
                });
                uploaded.push(image);
                id
            }
            (_, None) => continue,
        };
        play.set_texture(i, id);
    }
    uploaded
}
