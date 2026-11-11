//! A play drawn (`docs/plans/desktop.md`, section 3): the play field's
//! renderer over the window's surface, through its sRGB view with skins
//! blending through the plain one (D6, the web's WebGPU arrangement), in
//! the frame body both shells share (`ddi_shell::play_view`) over
//! desktop's parts: RGBA rows, the movies (feature `video`), the scripts'
//! text and sounds, the lyrics' fonts.

use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use ddi_library::backgrounds::{self, BgImage, BgSegment, LAYERS};
use ddi_platform::HostTime;
use ddi_render::cosmic::CosmicRasterizer;
use ddi_render::noteskin::NoteskinPlay;
use ddi_render::{RenderOptions, Renderer, SkinBlending};
use ddi_session::backgrounds::{MAX_BACKGROUND_SIDE, MAX_CHANGE_SIDE};
use ddi_shell::play_view::{Moment, PlayHost, PlayView, Screen, Target};
use ddi_shell::scripted::ScriptView;
use ddi_ui::state::settings::Settings;
#[cfg(feature = "video")]
use ddi_ui::state::settings::VideoMode;

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
    /// Drawn for a plain texture the 2D pass samples, in this float
    /// format ([`SkinBlending::Encoded`]).
    pub encoded: Option<wgpu::TextureFormat>,
}

impl Formats {
    /// For a surface configured `plain` with `srgb_view` among its view
    /// formats (or without one).
    pub fn of(plain: wgpu::TextureFormat, srgb_view: bool) -> Formats {
        if srgb_view {
            Formats {
                view: plain.add_srgb_suffix(),
                plain: Some(plain),
                encoded: None,
            }
        } else {
            Formats {
                view: plain,
                plain: None,
                encoded: None,
            }
        }
    }

    /// For a texture of encoded values the 2D pass shows: the announcer
    /// studio's staged plays, in their frame.
    pub fn picture() -> Formats {
        Formats {
            view: ddi_render::field_preview::FORMAT,
            plain: None,
            encoded: Some(wgpu::TextureFormat::Rgba16Float),
        }
    }

    fn blending(&self) -> SkinBlending {
        match (self.encoded, self.plain) {
            (Some(f), _) => SkinBlending::Encoded(f),
            (None, Some(p)) => SkinBlending::PlainView(p),
            (None, None) => SkinBlending::Scene,
        }
    }
}

/// What desktop brings to a play's drawing (`ddi_shell::play_view`):
/// RGBA rows, the movies through the decoder helper or the OS's decoders
/// (feature `video`), the scripts' text drawn here and the lyrics' fonts
/// read on a thread.
pub struct Desktop {
    /// The lyrics' fonts (cosmic-text on the system's fonts, read on a
    /// thread: a first read of the system's fonts is slow) while read,
    /// once read, and whether they were asked for.
    fonts: Option<Receiver<CosmicRasterizer>>,
    font: Option<CosmicRasterizer>,
    fonts_asked: bool,
    /// What the hand's budget is measured from.
    origin: Instant,
}

impl Desktop {
    fn new() -> Desktop {
        Desktop {
            fonts: None,
            font: None,
            fonts_asked: false,
            origin: Instant::now(),
        }
    }

    /// The lyrics' fonts read on a thread, once.
    fn read_fonts(&mut self) {
        if std::mem::replace(&mut self.fonts_asked, true) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("ddi-lyrics-fonts".into())
            .spawn(move || {
                let _ = tx.send(CosmicRasterizer::with_families(&crate::gpu::faces(), true));
            });
        self.fonts = Some(rx);
    }
}

impl PlayHost for Desktop {
    type Image = Rgba;
    #[cfg(feature = "video")]
    type Deck = super::movies::Movies;
    #[cfg(not(feature = "video"))]
    type Deck = ddi_shell::play_view::NoMovies;

    fn warn(&self, line: &str) {
        eprintln!("{line}");
    }

    fn lua_stopped(&mut self, key: &str, why: &str) {
        crate::library::noteskins::lua_stopped(key, why);
    }

    /// Drawn now (white, as a work's layers' text is drawn).
    fn draw_texts(&mut self, scripts: &mut ScriptView<Rgba>) {
        for text in scripts.measure() {
            if let Some((image, size)) = super::scripts::text_image(&text) {
                scripts.add_text(text, image, size);
            }
        }
    }

    fn text(&mut self) -> Option<Box<dyn ddi_render::RunRasterizer>> {
        if let Some(rx) = &self.fonts
            && let Ok(r) = rx.try_recv()
        {
            self.fonts = None;
            self.font = Some(r);
        }
        let font = self.font.take()?;
        Some(Box::new(font))
    }

    fn seconds(&self) -> f64 {
        self.origin.elapsed().as_secs_f64()
    }

    fn before_render(&mut self, device: &wgpu::Device, encoder: &wgpu::CommandEncoder) {
        #[cfg(feature = "video")]
        crate::video::protect_external_frames(device, encoder);
        #[cfg(not(feature = "video"))]
        let _ = (device, encoder);
    }
}

/// The play field's drawing: the shared view (`ddi_shell::play_view`,
/// through `Deref`) with what desktop keeps of a song (its scripts'
/// sounds and movies, its music's analysis) and of the window.
pub struct View {
    view: PlayView<Desktop>,
    formats: Formats,
    /// The scripts' sounds decoded (by file, relative to the song's
    /// folder), and how many they played so far.
    script_sounds: std::collections::HashMap<String, crate::audio::mixer::Sound>,
    pub sounds_played: usize,
    /// The scripts' movies' files, kept for [`View::take_scripts`].
    script_movie_files: Vec<(String, Result<super::load::MovieAt, String>)>,
    /// The music's analysis for the generated background, while a
    /// thread makes it.
    analysis: Option<Receiver<ddi_library::generative::Series>>,
    /// The window's scale factor (device pixels per point).
    pub scale: f32,
    /// The quit button is drawn this frame (`Feed::pointer_at`), and
    /// where it was (logical pixels: middle and radius).
    pub quit_shown: bool,
    pub quit_at: Option<[f32; 3]>,
    /// The refresh the next frame is for, when the shell's pacing knows
    /// it (`crate::pacing`); else the session predicts it.
    pub present_at: Option<HostTime>,
}

impl Deref for View {
    type Target = PlayView<Desktop>;

    fn deref(&self) -> &Self::Target {
        &self.view
    }
}

impl DerefMut for View {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.view
    }
}

impl View {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, formats: Formats) -> View {
        let renderer = Renderer::new(
            device,
            queue,
            formats.view,
            formats.blending(),
            &[ddi_render::HUD_FONT],
        );
        View {
            view: PlayView::new(Desktop::new(), renderer),
            formats,
            script_sounds: Default::default(),
            sounds_played: 0,
            script_movie_files: Vec::new(),
            analysis: None,
            scale: 1.0,
            quit_shown: false,
            quit_at: None,
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
    /// `add_background`; its scripts are readied by the host
    /// (`super::scripts`) and come in through [`View::set_scripts`].
    pub fn set_song(&mut self, loaded: &Loaded, settings: &Settings) {
        let layers = schedule(&loaded.song, loaded);
        let view = &mut self.view;
        #[cfg(feature = "video")]
        {
            view.movies.reset();
            if settings.bg_brightness > 0.0 && settings.video != VideoMode::Off {
                // The first layer's first movie first: it is the one probed
                // for the background frame.
                let first = layers[0].iter().find_map(|s| match &s.image {
                    BgImage::Movie(name) => Some(name.as_str()),
                    _ => None,
                });
                let mut movies: Vec<_> = loaded.movies.iter().collect();
                movies.sort_by_key(|(name, _)| Some(name.as_str()) != first);
                for (name, at) in movies {
                    view.movies.add(name, at.clone());
                }
            }
        }
        view.set_schedule(layers);
        view.set_video(settings.video, settings.background_frame);
        view.set_scripts(Default::default());
        self.script_sounds.clear();
    }

    /// The play about to start plays `song`, `loaded`'s song as played:
    /// shifted by the song offset, which moves its background changes too
    /// (as the web schedules them).
    pub fn set_played_song(&mut self, song: &ddi_chart::Song, loaded: &Loaded) {
        self.view.set_schedule(schedule(song, loaded));
        let folders = backgrounds::script_folders_of(&loaded.entry.bg_scripts);
        self.view
            .retime_scripts(&backgrounds::foregrounds(song, &folders));
    }

    /// The song's generated background under `settings`
    /// (`ddi_shell::generative::wanted`): none for a work, when turned
    /// off, with the backgrounds off or in `calibration`. Its music is
    /// analyzed on a thread where it may show (no picture, or over it);
    /// until then it runs on the beat alone.
    pub fn set_generated(&mut self, loaded: &Loaded, settings: &Settings, calibration: bool) {
        use ddi_shell::generative::{needs_music, wanted};
        let want = wanted(settings, &loaded.song, calibration);
        self.view.set_generated(want);
        self.analysis = None;
        if needs_music(want, loaded.entry.background.is_some()) {
            let music = loaded.music.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let made = std::thread::Builder::new()
                .name("ddi-generated-music".into())
                .spawn(move || {
                    use ddi_library::music::Analyzer;
                    let channels = usize::from(music.channels.max(1));
                    let mut a = Analyzer::new(f64::from(music.rate), music.frames(), 0.0);
                    // A second at a time: few samples kept.
                    for chunk in music.data.chunks(music.rate as usize * channels) {
                        a.add_interleaved(chunk, channels);
                        a.step(usize::MAX);
                    }
                    let _ = tx.send(ddi_library::generative::Series::new(&a.finish()));
                });
            self.analysis = made.is_ok().then_some(rx);
        }
    }

    /// Waits for the music's analysis, if it is being made (tests).
    pub fn wait_for_generated(&mut self) {
        if let Some(rx) = self.analysis.take()
            && let Ok(series) = rx.recv()
        {
            self.view.set_generated_series(Rc::new(series));
        }
    }

    /// Whether the song has a generated background.
    pub fn has_generated(&self) -> bool {
        self.view.generated().is_some()
    }

    /// The song's scripts (`docs/plans/scripted-backgrounds.md`), loaded
    /// (`None`: none, or they did not load; the song background then
    /// shows in their place). Their textures go up, and skins' quads are
    /// drawn, from their first frame.
    pub fn set_scripts(&mut self, ready: Option<super::scripts::Ready>) {
        let ready = ready.unwrap_or_else(|| super::scripts::Ready {
            view: Default::default(),
            sounds: Default::default(),
            movies: Vec::new(),
        });
        self.view.set_scripts(ready.view);
        self.script_sounds = ready.sounds;
        #[cfg(feature = "video")]
        for (name, at) in &ready.movies {
            self.view.script_movies.add(name, at.clone());
        }
        self.script_movie_files = ready.movies;
    }

    /// The song's scripts taken back (the view is let go: a lost device),
    /// for the next view to draw ([`View::set_scripts`]); `None` without.
    pub fn take_scripts(&mut self) -> Option<super::scripts::Ready> {
        let view = self.view.take_scripts()?;
        Some(super::scripts::Ready {
            view,
            sounds: std::mem::take(&mut self.script_sounds),
            movies: std::mem::take(&mut self.script_movie_files),
        })
    }

    /// The scripts' sounds played since last asked, for the host to play
    /// on the play's sound now.
    pub fn take_script_sounds(&mut self) -> Vec<crate::audio::mixer::Sound> {
        let files = self.view.take_script_sounds();
        let sounds: Vec<_> = files
            .iter()
            .filter_map(|f| self.script_sounds.get(f).cloned())
            .collect();
        self.sounds_played += sounds.len();
        sounds
    }

    /// Whether the song's scripts draw (loaded, not stopped).
    pub fn scripts_running(&self) -> bool {
        self.view.scripts().running()
    }

    /// The images the song shows (in `calibration`, without the cover a
    /// generated background folds: it has none): what to read, `(what,
    /// source, longest side)`.
    pub fn images_to_read(
        loaded: &Loaded,
        settings: &Settings,
        calibration: bool,
    ) -> Vec<(BgImage, String, u32)> {
        if settings.bg_brightness <= 0.0 {
            return Vec::new();
        }
        let e = &loaded.entry;
        let mut out = Vec::new();
        if let Some(bg) = &e.background {
            out.push((BgImage::Song, loaded.files.source(bg), MAX_BACKGROUND_SIDE));
        }
        // The cover the kaleidoscope folds standing in.
        let want = ddi_shell::generative::wanted(settings, &loaded.song, calibration);
        if ddi_shell::generative::needs_cover(want, e.background.is_some())
            && let Some(cover) = ddi_shell::generative::cover(e)
        {
            out.push((BgImage::Cover, loaded.files.source(cover), MAX_CHANGE_SIDE));
        }
        for name in &e.bg_images {
            out.push((
                BgImage::File(name.to_string()),
                loaded.files.source(name),
                MAX_CHANGE_SIDE,
            ));
        }
        // Those found in the shared folders, under the names written.
        for (name, file) in &loaded.shared {
            if !ddi_library::video::is_movie(name) {
                out.push((
                    BgImage::File(name.clone()),
                    file.display().to_string(),
                    MAX_CHANGE_SIDE,
                ));
            }
        }
        out
    }

    /// The song's pictures on the device.
    pub fn backgrounds(&self) -> usize {
        self.view.pictures().len()
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
        if let Some(rx) = &self.analysis
            && let Ok(series) = rx.try_recv()
        {
            self.view.set_generated_series(Rc::new(series));
            self.analysis = None;
        }
        let second = self.view.tick(now.0 * 1000.0);
        let target = Target {
            view,
            plain,
            size,
            format: self.formats.plain.unwrap_or(self.formats.view),
        };
        let moment = Moment {
            now,
            present: self.present_at.take(),
            second,
        };
        // The quit button while the pointer was moved lately (as the
        // web's touch screens have it).
        let screen = Screen {
            ratio: self.scale,
            quit: self.quit_shown.then_some([0.0; 4]),
            lyrics: true,
        };
        let drawn = self.view.draw(
            (device, queue, encoder),
            &target,
            &play.session,
            render,
            moment,
            screen,
        );
        self.quit_at = drawn.quit_at;
    }

    /// The lyrics of the play about to start (`None`: none shown); the
    /// fonts they are drawn with are read meanwhile.
    pub fn set_lyrics(&mut self, lyrics: Option<ddi_engine::lyrics::LyricTrack>) {
        self.view.set_lyrics(lyrics);
        if self.view.has_lyrics() {
            self.view.host.read_fonts();
        }
    }

    /// The fonts the play's text is drawn with (a stage's opponents'
    /// names), read meanwhile.
    pub fn read_text(&mut self) {
        self.view.host.read_fonts();
    }

    /// The lyrics the last frame drew.
    pub fn lyrics_shown(&self) -> &[String] {
        self.view.lyrics_shown()
    }

    /// Where the last frame drew a stage's opponents' miniature fields.
    pub fn opponent_fields(&self) -> Option<&ddi_shell::opponents::Fields> {
        self.view.opponent_fields()
    }

    /// Waits for the lyrics' fonts (tests).
    pub fn wait_for_fonts(&mut self) {
        let host = &mut self.view.host;
        if let Some(rx) = host.fonts.take()
            && let Ok(r) = rx.recv()
        {
            host.font = Some(r);
        }
    }

    /// A content of the song's layers, read.
    pub fn add_stage_content(&mut self, read: super::stage::Read) {
        let (content, image, size) = read;
        self.view.add_stage_content(content.source(), image, size);
    }
}

/// The background changes of `song` (`loaded`'s, as played or not), a
/// schedule per layer.
pub(crate) fn schedule(song: &ddi_chart::Song, loaded: &Loaded) -> [Vec<BgSegment>; LAYERS] {
    let (shared_movies, shared_images): (Vec<&str>, Vec<&str>) = loaded
        .shared
        .iter()
        .map(|(name, _)| name.as_str())
        .partition(|name| ddi_library::video::is_movie(name));
    let (images, movies) = backgrounds::files_shown(&loaded.entry, shared_images, shared_movies);
    backgrounds::layers_of(song, &loaded.entry, &images, &movies, &loaded.random)
}

/// Fills a texture's first level with RGBA rows.
pub fn write_rgba(queue: &wgpu::Queue, texture: &wgpu::Texture, image: &Rgba) {
    ddi_render::write_rgba(queue, texture, image.size, &image.data);
}

/// Gives the renderer `play`'s textures: the built-in art, and the
/// decoded `images` (by texture index), a model's with mipmaps.
pub fn upload_noteskin(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    play: &mut NoteskinPlay,
    images: &[Option<Rgba>],
) {
    let images = images.iter().map(Option::as_ref);
    ddi_shell::stage::upload_noteskin(renderer, device, queue, play, images);
}
