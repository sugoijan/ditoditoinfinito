//! A play drawn (`docs/plans/desktop.md`, section 3): the play field's
//! renderer over the window's surface, through its sRGB view with skins
//! blending through the plain one (D6, the web's WebGPU arrangement), the
//! noteskin and its Lua, the backgrounds and their changes, the movies
//! (feature `video`), the start prompt's picture. As the web's
//! `game_loop.rs` draws it, the same `ddi_render` code.

use ddi_library::backgrounds::{self, BgImage, BgSegment, LAYERS};
use ddi_platform::HostTime;
use ddi_render::noteskin::{NoteLayer, NoteskinPlay};
use ddi_render::{Backdrop, RenderOptions, Renderer, SkinBlending};
use ddi_session::backgrounds::{
    FpsMeter, FrameShape, MAX_BACKGROUND_SIDE, MAX_CHANGE_SIDE, MovieGuard,
};
use ddi_ui::state::settings::{Settings, VideoMode};

use super::Play;
use super::load::Loaded;

/// A decoded image for the renderer: RGBA rows.
#[derive(Clone)]
pub struct Rgba {
    pub size: [u32; 2],
    pub data: Vec<u8>,
}

/// The surface's formats: the view the scene draws through (sRGB, when
/// the surface has one) and the plain one skins blend through.
#[derive(Clone, Copy, Debug)]
pub struct Formats {
    pub view: wgpu::TextureFormat,
    pub plain: Option<wgpu::TextureFormat>,
}

impl Formats {
    /// For a surface configured `plain` with `srgb_view` among its view
    /// formats (or without one).
    pub fn of(plain: wgpu::TextureFormat, srgb_view: bool) -> Formats {
        if srgb_view {
            Formats {
                view: plain.add_srgb_suffix(),
                plain: Some(plain),
            }
        } else {
            Formats {
                view: plain,
                plain: None,
            }
        }
    }

    fn blending(&self) -> SkinBlending {
        match self.plain {
            Some(p) => SkinBlending::PlainView(p),
            None => SkinBlending::Scene,
        }
    }
}

/// The play field's drawing.
pub struct View {
    pub renderer: Renderer,
    formats: Formats,
    noteskin: Option<NoteskinPlay>,
    /// What the skin's Lua stops count against.
    noteskin_key: String,
    /// Background textures by what they show (renderer ids).
    backgrounds: Vec<(BgImage, usize)>,
    /// Image and movie textures for the backdrop, rebuilt when either
    /// changes.
    backdrop_textures: ddi_session::movies::BackdropTextures,
    /// Decoded images waiting for the GPU.
    pending: Vec<(BgImage, Rgba)>,
    /// The background changes, a schedule per layer.
    schedule: [Vec<BgSegment>; LAYERS],
    frame_shape: FrameShape,
    pub fps: FpsMeter,
    /// A work's back and mask layers.
    stage: super::stage::StageView,
    stage_drawn: (usize, usize),
    /// The song's scripted backgrounds and foregrounds
    /// (`docs/plans/scripted-backgrounds.md`), and why they stopped.
    scripts: ddi_shell::scripted::ScriptView<Rgba>,
    scripts_stopped: Option<String>,
    /// The scripts' sounds decoded (by file, relative to the song's
    /// folder), and those played since the host last took them.
    script_sounds: std::collections::HashMap<String, crate::audio::mixer::Sound>,
    sounds_due: Vec<crate::audio::mixer::Sound>,
    /// Sounds the scripts played so far.
    pub sounds_played: usize,
    /// The layers of the play about to start, set on its first frame.
    stage_track: Option<Option<ddi_engine::sprites::SpriteTrack>>,
    /// The chart's lyrics, when it has some and they are shown.
    lyrics: Option<ddi_engine::lyrics::LyricTrack>,
    /// The 2D pass over the play (the lyrics, cosmic-text on the system's
    /// fonts): its painter and draw list, once its fonts are read (on a
    /// thread: a first read of the system's fonts is slow).
    overlay: Option<(ddi_render::painter::Painter, ddi_render::DrawList)>,
    fonts: Option<std::sync::mpsc::Receiver<ddi_render::cosmic::CosmicRasterizer>>,
    pending_fonts: Option<ddi_render::cosmic::CosmicRasterizer>,
    /// The lyrics drawn in the last frame.
    lyrics_shown: Vec<String>,
    /// The display's refresh rate as the OS says it, until frames have
    /// measured it (the movies' guard's start).
    pub refresh_hint: Option<f64>,
    guard: MovieGuard,
    video: VideoMode,
    #[cfg(feature = "video")]
    pub movies: super::movies::Movies,
    /// Frames drawn.
    pub frames: u64,
    entrance: Option<ddi_actor::background::Entrance>,
    /// The window's scale factor (device pixels per point).
    pub scale: f32,
    /// The refresh the next frame is for, when the shell's pacing knows
    /// it (`crate::pacing`); else the session predicts it.
    pub present_at: Option<HostTime>,
}

impl View {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, formats: Formats) -> View {
        View {
            renderer: Renderer::new(
                device,
                queue,
                formats.view,
                formats.blending(),
                &[ddi_render::HUD_FONT],
            ),
            formats,
            noteskin: None,
            noteskin_key: String::new(),
            backgrounds: Vec::new(),
            backdrop_textures: Default::default(),
            pending: Vec::new(),
            schedule: Default::default(),
            frame_shape: FrameShape::default(),
            fps: FpsMeter::new(),
            stage: Default::default(),
            stage_drawn: (0, 0),
            scripts: Default::default(),
            scripts_stopped: None,
            script_sounds: Default::default(),
            sounds_due: Vec::new(),
            sounds_played: 0,
            stage_track: None,
            pending_fonts: None,
            lyrics: None,
            overlay: None,
            fonts: None,
            lyrics_shown: Vec::new(),
            refresh_hint: None,
            guard: MovieGuard::default(),
            video: VideoMode::Auto,
            #[cfg(feature = "video")]
            movies: Default::default(),
            frames: 0,
            entrance: None,
            scale: 1.0,
            present_at: None,
        }
    }

    pub fn formats(&self) -> Formats {
        self.formats
    }

    /// The song to play (a view draws one song: a new one has a new
    /// view): its background changes (on the song as stored, until a
    /// play gives its own: [`View::set_played_song`]), the settings' frame
    /// and video mode, its movies. Its images are read on other threads
    /// ([`View::images_to_read`]) and come in through
    /// [`View::add_background`]; its scripts are readied by the host
    /// (`super::scripts`) and come in through [`View::set_scripts`].
    pub fn set_song(&mut self, loaded: &Loaded, settings: &Settings) {
        self.schedule = schedule(&loaded.song, loaded);
        self.frame_shape = FrameShape::new(settings.background_frame);
        self.video = settings.video;
        self.scripts = Default::default();
        self.scripts_stopped = None;
        self.script_sounds.clear();
        self.sounds_due.clear();
        #[cfg(feature = "video")]
        {
            self.movies.reset();
            if settings.bg_brightness > 0.0 && settings.video != VideoMode::Off {
                // The first layer's first movie first: it is the one probed
                // for the background frame.
                let first = self.schedule[0].iter().find_map(|s| match &s.image {
                    BgImage::Movie(name) => Some(name.as_str()),
                    _ => None,
                });
                let mut movies: Vec<_> = loaded.movies.iter().collect();
                movies.sort_by_key(|(name, _)| Some(name.as_str()) != first);
                for (name, at) in movies {
                    self.movies.add(name, at.clone());
                }
            }
        }
    }

    /// The play about to start plays `song`, `loaded`'s song as played:
    /// shifted by the song offset, which moves its background changes too
    /// (as the web schedules them).
    pub fn set_played_song(&mut self, song: &ddi_chart::Song, loaded: &Loaded) {
        self.schedule = schedule(song, loaded);
        let folders = backgrounds::script_folders_of(&loaded.entry.bg_scripts);
        self.scripts
            .retime(&backgrounds::foregrounds(song, &folders));
    }

    /// The song's scripts (`docs/plans/scripted-backgrounds.md`), loaded
    /// (`None`: none, or they did not load; the song background then
    /// shows in their place). Their textures go up, and skins' quads are
    /// drawn, from their first frame.
    pub fn set_scripts(&mut self, ready: Option<super::scripts::Ready>) {
        let ready = ready.unwrap_or_else(|| super::scripts::Ready {
            view: Default::default(),
            sounds: Default::default(),
        });
        self.scripts = ready.view;
        self.scripts.forget_textures();
        self.script_sounds = ready.sounds;
        self.scripts_stopped = None;
    }

    /// The song's scripts taken back (the view is let go: a lost device),
    /// for the next view to draw ([`View::set_scripts`]); `None` without.
    pub fn take_scripts(&mut self) -> Option<super::scripts::Ready> {
        if !self.scripts.loaded() {
            return None;
        }
        let mut view = std::mem::take(&mut self.scripts);
        view.forget_textures();
        Some(super::scripts::Ready {
            view,
            sounds: std::mem::take(&mut self.script_sounds),
        })
    }

    /// The scripts' texts not drawn yet, drawn (white, as a work's
    /// layers' text is drawn).
    fn draw_script_texts(scripts: &mut ddi_shell::scripted::ScriptView<Rgba>) {
        for text in scripts.measure() {
            if let Some((image, size)) = super::scripts::text_image(&text) {
                scripts.add_text(text, image, size);
            }
        }
    }

    /// The scripts' sounds played since last asked, for the host to play
    /// on the play's sound now.
    pub fn take_script_sounds(&mut self) -> Vec<crate::audio::mixer::Sound> {
        std::mem::take(&mut self.sounds_due)
    }

    /// Why the song's scripts stopped during the play, if they did.
    pub fn scripts_stopped(&self) -> Option<&str> {
        self.scripts_stopped.as_deref()
    }

    /// Whether the song's scripts draw (loaded, not stopped).
    pub fn scripts_running(&self) -> bool {
        self.scripts.running()
    }

    /// The images the song shows: what to read, `(what, source,
    /// longest side)`.
    pub fn images_to_read(loaded: &Loaded, settings: &Settings) -> Vec<(BgImage, String, u32)> {
        if settings.bg_brightness <= 0.0 {
            return Vec::new();
        }
        let e = &loaded.entry;
        let mut out = Vec::new();
        if let Some(bg) = &e.background {
            out.push((BgImage::Song, loaded.files.source(bg), MAX_BACKGROUND_SIDE));
        }
        for name in &e.bg_images {
            out.push((
                BgImage::File(name.to_string()),
                loaded.files.source(name),
                MAX_CHANGE_SIDE,
            ));
        }
        out
    }

    /// The song's pictures on the device.
    pub fn backgrounds(&self) -> usize {
        self.backgrounds.len()
    }

    /// An image read for the song.
    pub fn add_background(&mut self, what: BgImage, image: Rgba) {
        if self.backgrounds.iter().any(|(w, _)| *w == what) {
            return;
        }
        self.pending.push((what, image));
    }

    /// Images read go up: all of them outside play; in play one a frame,
    /// and only when no change is due within a second (an upload never
    /// costs a change its frame).
    fn upload_backgrounds(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, at: Option<f64>) {
        let count = backgrounds::uploads_due(&self.schedule, at, self.pending.len());
        let batch: Vec<_> = self.pending.drain(..count).collect();
        let limit = device.limits().max_texture_dimension_2d;
        for (what, image) in batch {
            let [w, h] = image.size;
            if w.max(h) > limit || w == 0 || h == 0 {
                continue;
            }
            let id = self
                .renderer
                .add_background_image(device, [w, h], |texture| write_rgba(queue, texture, &image));
            match what {
                BgImage::Song => self.frame_shape.song_bg_aspect = Some(w as f32 / h as f32),
                BgImage::Still => self.frame_shape.still_aspect = Some(w as f32 / h as f32),
                _ => {}
            }
            self.backgrounds.push((what, id));
        }
    }

    /// Draws with `play` (readied for this play's layout and direction,
    /// its images by texture index: `None` for built-in art) from now on;
    /// `None`: the built-in arrows.
    pub fn set_noteskin(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        play: Option<(NoteskinPlay, Vec<Option<Rgba>>, String)>,
    ) {
        self.renderer.clear_noteskin_textures();
        // The layers' and the scripts' textures went with them.
        self.stage.forget_textures();
        self.scripts.forget_textures();
        let srgb = self.renderer.skin_colors_linear();
        self.noteskin = play.map(|(mut play, images, key)| {
            self.noteskin_key = key;
            play.set_srgb(srgb);
            upload_noteskin(&mut self.renderer, device, queue, &mut play, &images);
            play
        });
        self.renderer
            .set_skins_drawn(device, queue, self.skins_drawn());
    }

    /// The noteskin drawn, if any.
    pub fn noteskin(&self) -> Option<&NoteskinPlay> {
        self.noteskin.as_ref()
    }

    /// Continue the prompt's visual entrance from the first drawn play frame.
    pub fn enter_background(&mut self, from: f32, reduced: bool) {
        self.entrance = Some(ddi_actor::background::Entrance::new(from, reduced));
    }

    /// Restart skin commands and the movies' guard for a new play.
    pub fn start(&mut self) {
        if let Some(n) = self.noteskin.as_mut() {
            n.restart();
        }
        self.scripts.restart();
        let refresh = self
            .fps
            .refresh_estimate()
            .or(self.refresh_hint)
            .unwrap_or(60.0);
        self.guard.start(refresh);
    }

    /// The pictures read so far on the device (before a play: all of
    /// them).
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.upload_backgrounds(device, queue, None);
    }

    /// The backgrounds' frame before the play starts, for the prompt's
    /// picture (`None`: the whole screen).
    pub fn prompt_frame(&mut self) -> Option<f32> {
        self.frame_aspect(false)
    }

    /// The frame's backgrounds' shape (`None`: the whole screen).
    fn frame_aspect(&mut self, started: bool) -> Option<f32> {
        #[cfg(feature = "video")]
        {
            let movies = &mut self.movies;
            let shown = movies.shown_any();
            self.frame_shape
                .aspect(|| movies.picture_aspect(), shown, started)
        }
        #[cfg(not(feature = "video"))]
        {
            self.frame_shape.aspect(|| Err(false), false, started)
        }
    }

    /// One frame of `play` at host time `now` into `view` (the plain
    /// view `plain`, for skins), `size` pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        plain: Option<&wgpu::TextureView>,
        size: [u32; 2],
        play: &Play,
        render: RenderOptions,
        now: HostTime,
    ) {
        let session = &play.session;
        let second = self
            .fps
            .tick(now.0 * 1000.0)
            .map(|fps| self.fps.second(fps));
        let present = self
            .present_at
            .take()
            .unwrap_or_else(|| session.predicted_present(now));
        let frame = session.frame(present);
        let v = session.view;
        let started = session.player.started();
        let playing = started && !session.player.finished();
        self.upload_backgrounds(device, queue, playing.then_some(frame.song_time));
        if let Some(track) = self.stage_track.take() {
            self.apply_stage_track(device, queue, track);
        }
        let frame_aspect = self.frame_aspect(started);
        #[cfg(feature = "video")]
        {
            if session.player.finished() {
                self.movies.stop();
            } else {
                let drawing = self.movies.drawing(&self.schedule);
                self.movies.update(
                    &self.schedule,
                    frame.song_time,
                    &mut self.renderer,
                    device,
                    queue,
                );
                if self.video == VideoMode::Auto {
                    let refresh = self
                        .fps
                        .refresh_estimate()
                        .or(self.refresh_hint)
                        .unwrap_or(60.0);
                    let late = self.movies.late();
                    match self
                        .guard
                        .step(now.0 * 1000.0, drawing, second, refresh, late)
                    {
                        Some(ddi_platform::video::GuardAction::Pause) => self.movies.pause(),
                        Some(ddi_platform::video::GuardAction::Resume) => self.movies.resume(),
                        Some(ddi_platform::video::GuardAction::Off(r)) => self.movies.turn_off(r),
                        None => {}
                    }
                }
            }
        }
        #[cfg(not(feature = "video"))]
        let _ = (second, &self.guard, self.video);
        let mut render = render;
        render.hide_notes = v.hide_notes;
        render.cancel_progress = v.cancel_progress;
        render.cancel_hint = v.cancel_hint;
        render.frame_aspect = frame_aspect;
        if let Some(entrance) = &mut self.entrance {
            let reveal = entrance.sample(now.0);
            render.background_reveal = Some(reveal);
            if reveal == ddi_actor::background::Reveal::default() {
                self.entrance = None;
            }
        }
        render.backdrop = self.backdrop(frame.song_time);
        let layer = match self.noteskin.as_mut() {
            Some(n) => {
                let from = n.next_seq();
                n.update(
                    &frame,
                    session.player.skin_events_since(from),
                    render.reverse,
                );
                // The skin's Lua stopped: the play goes on with the
                // built-in skin; the next one tries again, and the
                // second stop turns its Lua off (a stall is not the
                // skin's: not held against it).
                if let Some((why, on_clock)) = n.take_lua_stop() {
                    if on_clock {
                        eprintln!("noteskin {}: {why}", self.noteskin_key);
                    } else {
                        crate::library::noteskins::lua_stopped(&self.noteskin_key, why);
                    }
                }
                Some(n as &mut dyn NoteLayer)
            }
            None => None,
        };
        // The field's place, for a work's layers and the lyrics.
        let (w, h) = (size[0] as f32, size[1] as f32);
        let span = ddi_render::scene::field_span(&session.layout);
        let geo = ddi_render::FieldGeometry::new(w, h, span, render.reverse);
        // A work's back and mask layers, placed as the web places them;
        // the song's scripts in the background frame.
        let scripts = self.scripts.running();
        if self.stage.shown() || scripts {
            let srgb = self.renderer.skin_colors_linear();
            let (mut back, mut mask) = (Vec::new(), Vec::new());
            if self.stage.shown() {
                self.stage.upload(&mut self.renderer, device, queue);
                (back, mask) = self
                    .stage
                    .quads(frame.song_time, geo.center_x(span), h, srgb);
            }
            if scripts {
                let drawn = self.scripts.draw(
                    (&mut self.renderer, device, queue),
                    (frame.song_time, frame.beat),
                    &self.schedule,
                    ([w, h], frame_aspect),
                    layer.is_some() || self.stage.shown(),
                    Self::draw_script_texts,
                    (&mut back, &mut mask),
                );
                if let Some(why) = drawn.stopped {
                    eprintln!("scripted backgrounds stopped: {why}");
                    self.scripts_stopped = Some(why);
                }
                for f in drawn.sounds {
                    if let Some(s) = self.script_sounds.get(&f) {
                        self.sounds_due.push(s.clone());
                        self.sounds_played += 1;
                    }
                }
            }
            self.stage_drawn = (back.len(), mask.len());
            self.renderer.set_stage_quads(back, mask);
        }
        #[cfg(feature = "video")]
        crate::video::protect_external_frames(device, encoder);
        self.renderer.render(
            device,
            queue,
            encoder,
            view,
            plain,
            size[0],
            size[1],
            &frame,
            &session.layout,
            &session.names,
            render,
            layer,
        );
        // The lyrics at this frame's time, over the field's middle.
        let lyrics = self
            .lyrics
            .as_ref()
            .filter(|_| playing)
            .map(|track| {
                // Clear of the HUD's gauge and score, never over the
                // receptors (no quit button here); the smallest size in
                // points, as the web's is in CSS pixels.
                let place = ddi_render::lyrics::Place::for_play(
                    &geo,
                    &session.layout,
                    track.stage(),
                    None,
                    self.scale,
                );
                (track.at(frame.song_time), place)
            })
            .filter(|l| !l.0.is_empty());
        match lyrics {
            Some(l) => self.draw_lyrics(device, queue, encoder, view, plain, size, l),
            None => self.lyrics_shown.clear(),
        }
        self.frames += 1;
    }

    /// A work's layers for the play about to start (`None`: none shown),
    /// from its first frame.
    pub fn set_stage_track(&mut self, track: Option<ddi_engine::sprites::SpriteTrack>) {
        self.stage_track = Some(track);
    }

    fn apply_stage_track(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        track: Option<ddi_engine::sprites::SpriteTrack>,
    ) {
        self.stage.set_track(track);
        self.renderer.set_stage_quads(Vec::new(), Vec::new());
        self.stage_drawn = (0, 0);
        self.renderer
            .set_skins_drawn(device, queue, self.skins_drawn());
    }

    /// Whether quads are drawn as skins' are: a noteskin's, a work's
    /// layers', the song's scripts'.
    fn skins_drawn(&self) -> bool {
        self.noteskin.is_some() || self.stage.shown() || self.scripts.running()
    }

    /// The lyrics of the play about to start (`None`: none shown); the
    /// fonts they are drawn with are read meanwhile.
    pub fn set_lyrics(&mut self, lyrics: Option<ddi_engine::lyrics::LyricTrack>) {
        self.lyrics = lyrics.filter(|t| !t.is_empty());
        self.lyrics_shown.clear();
        if self.lyrics.is_some() && self.overlay.is_none() && self.fonts.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = std::thread::Builder::new()
                .name("ddi-lyrics-fonts".into())
                .spawn(move || {
                    let _ = tx.send(ddi_render::cosmic::CosmicRasterizer::with_families(
                        crate::gpu::FACES,
                        true,
                    ));
                });
            self.fonts = Some(rx);
        }
    }

    /// The lyrics drawn in the last frame.
    pub fn lyrics_shown(&self) -> &[String] {
        &self.lyrics_shown
    }

    /// Waits for the lyrics' fonts (tests).
    pub fn wait_for_fonts(&mut self) {
        if let Some(rx) = &self.fonts
            && let Ok(r) = rx.recv()
        {
            self.fonts = None;
            self.pending_fonts = Some(r);
        }
    }

    /// Draws `lines` through the 2D pass over the frame just encoded:
    /// through the surface's plain view where it has one (blended encoded,
    /// as the menus), else through the view itself.
    #[allow(clippy::too_many_arguments)]
    fn draw_lyrics(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        plain: Option<&wgpu::TextureView>,
        size: [u32; 2],
        (lines, place): (
            Vec<ddi_engine::lyrics::LyricLine>,
            ddi_render::lyrics::Place,
        ),
    ) {
        let format = self.formats.plain.unwrap_or(self.formats.view);
        let target = plain.unwrap_or(view);
        if let Some(rx) = &self.fonts
            && let Ok(r) = rx.try_recv()
        {
            self.fonts = None;
            self.pending_fonts = Some(r);
        }
        if self.overlay.is_none()
            && let Some(r) = self.pending_fonts.take()
        {
            let mut painter = ddi_render::painter::Painter::new(device, queue, format);
            painter.set_rasterizer(Box::new(r));
            self.overlay = Some((painter, ddi_render::DrawList::new()));
        }
        let Some((painter, list)) = self.overlay.as_mut() else {
            return;
        };
        // The last frame's draws were submitted by the shell.
        painter.submitted();
        list.clear();
        {
            let mut measure = painter.measure(1.0);
            ddi_render::lyrics::draw(list, &lines, &place, &mut measure);
        }
        painter.draw(
            device,
            queue,
            encoder,
            target,
            size,
            1.0,
            None,
            list,
            &mut |_, _| {},
        );
        let shown = lines.iter().filter(|l| l.opacity > 0.0).map(|l| &*l.text);
        if !shown
            .clone()
            .eq(self.lyrics_shown.iter().map(String::as_str))
        {
            self.lyrics_shown = shown.map(str::to_string).collect();
        }
    }

    /// A content of the song's layers, read.
    pub fn add_stage_content(&mut self, read: super::stage::Read) {
        let (content, image, size) = read;
        self.stage.add(content.source(), image, size);
    }

    /// The layers' quads drawn in the last frame, back and mask.
    pub fn stage_drawn(&self) -> (usize, usize) {
        self.stage_drawn
    }

    /// What shows behind the field at song second `t`: both layers.
    fn backdrop(&mut self, t: f64) -> Backdrop {
        // Black under the scripts while they run.
        let black = self.scripts.running().then_some(ddi_render::BLACK);
        #[cfg(feature = "video")]
        let first = &self.schedule[0];
        #[cfg(feature = "video")]
        let (all, opening) = {
            let movies = &self.movies;
            let all = self.backdrop_textures.get(
                &self.backgrounds,
                movies.version(),
                || movies.shown_ids(),
                ddi_render::PLAIN,
                black,
            );
            (all, movies.opening_plays(first))
        };
        #[cfg(not(feature = "video"))]
        let (all, opening) = (
            self.backdrop_textures
                .get(&self.backgrounds, 0, Vec::new, ddi_render::PLAIN, black),
            false,
        );
        let [first, second] = backgrounds::draws_at(&self.schedule, t, all, opening);
        Backdrop { first, second }
    }
}

/// The background changes of `song` (`loaded`'s, as played or not), a
/// schedule per layer.
pub(super) fn schedule(song: &ddi_chart::Song, loaded: &Loaded) -> [Vec<BgSegment>; LAYERS] {
    let e = &loaded.entry;
    let movies: Vec<String> = e
        .bg_videos
        .iter()
        .filter(|v| v.plays())
        .map(|v| v.name.to_string())
        .collect();
    // Movies found but that do not play: the second layer shows nothing
    // for them.
    let unshowable: Vec<&str> = e
        .bg_videos
        .iter()
        .filter(|v| !v.plays())
        .map(|v| v.name.as_str())
        .chain(e.bg_scripted.iter().map(|p| p.as_str()))
        .collect();
    let scripts = backgrounds::script_folders_of(&e.bg_scripts);
    backgrounds::layers(
        song,
        &e.bg_images,
        &movies,
        &scripts,
        &unshowable,
        e.background.is_some(),
    )
}

/// Fills a texture's first level with RGBA rows.
pub fn write_rgba(queue: &wgpu::Queue, texture: &wgpu::Texture, image: &Rgba) {
    ddi_render::write_rgba(queue, texture, image.size, &image.data);
}

/// Gives the renderer `play`'s textures: the built-in art, and the
/// decoded `images` (by texture index), a model's with mipmaps (the
/// web's `game_loop::upload_noteskin`).
pub fn upload_noteskin(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    play: &mut NoteskinPlay,
    images: &[Option<Rgba>],
) {
    let keys = play.texture_keys().to_vec();
    for (i, key) in keys.iter().enumerate() {
        let id = match (key, images.get(i).and_then(Option::as_ref)) {
            (ddi_noteskin::build::TextureKey::BuiltIn(name), _) => {
                renderer.add_builtin_art(device, queue, name)
            }
            (_, Some(image)) => {
                let mipmaps = play.texture_mipmaps(i);
                renderer.add_noteskin_image(device, queue, image.size, mipmaps, |texture| {
                    write_rgba(queue, texture, image);
                })
            }
            (_, None) => continue,
        };
        play.set_texture(i, id);
    }
}
