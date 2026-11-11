//! A play drawn, for both shells (the web's `game_loop.rs`, desktop's
//! `play/view.rs`): the field's renderer, the noteskin and its Lua, the
//! song's pictures and background changes, its movies and its scripts'
//! (`docs/plans/phase-8-gaps.md`, step 3), a work's layers, the song's
//! scripted backgrounds and foregrounds, the generated background, the
//! dancing hand and the lyrics, all drawn in one frame body
//! ([`PlayView::draw`]).
//!
//! Each shell brings its platform's parts through [`PlayHost`]: the
//! decoded pictures ([`StageContent`]), the movie decks ([`Deck`]), the
//! log, the text it draws and the lyrics' rasterizer. It keeps its own
//! surface and what it reads of the frame.

use std::rc::Rc;

use ddi_actor::background::{Entrance, Reveal};
use ddi_engine::lyrics::LyricTrack;
use ddi_engine::sprites::{SpriteSource, SpriteTrack};
use ddi_library::backgrounds::{self, BgImage, BgSegment, Foreground, LAYERS};
use ddi_platform::video::{GuardAction, Second};
use ddi_platform::{AudioBackend, HostTime};
use ddi_render::noteskin::{NoteLayer, NoteskinPlay};
use ddi_render::{
    Backdrop, BackgroundPipeline, DrawList, Painter, RenderOptions, Renderer, RunRasterizer,
};
use ddi_session::backgrounds::{FpsMeter, FrameShape, MovieGuard};
use ddi_session::movies::{BackdropTextures, Clip};
use ddi_session::play::{Inputs, PlaySession};
use ddi_session::scripted::Failure;
use ddi_ui::state::settings::{BackgroundFrame, VideoMode};

use crate::generative::{Generated, Want};
use crate::scripted::{ScriptView, draw_layers};
use crate::stage::{StageContent, StageView, upload_noteskin};

/// A shell's movies (the song's, or its scripts'): its decoders under the
/// rules both shells share (`ddi_session::movies::MovieSlots`), the
/// textures it makes and the frames it shows.
pub trait Deck: Default {
    /// Opens, feeds and closes decoders for song second `t` on the
    /// background `layers`, and shows the frames due on `background`.
    fn update(
        &mut self,
        layers: &[Vec<BgSegment>],
        t: f64,
        background: &mut BackgroundPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    );

    /// Opens, feeds and closes decoders for a song's scripts' movies drawn
    /// now (`clips`) and puts the frames due on `background`: the movies
    /// shown, `(file, its id there)`.
    fn update_clips(
        &mut self,
        clips: &[Clip],
        background: &mut BackgroundPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<(String, usize)>;

    /// The probed movie's picture shape: `Err(true)` while the probe runs.
    fn picture_aspect(&mut self) -> Result<f32, bool>;

    /// Closes every decoder (the song ended).
    fn stop(&mut self);

    /// Stops for good (the session is over).
    fn reset(&mut self);

    /// What a movie draws on `layers` now, for the automatic setting.
    fn drawing(&self, layers: &[Vec<BgSegment>]) -> Option<usize>;

    /// A frame of a movie has been shown this session.
    fn shown_any(&self) -> bool;

    /// Changes whenever [`Deck::shown_ids`] would.
    fn version(&self) -> u64;

    /// The movie textures that hold a frame of this run.
    fn shown_ids(&self) -> Vec<(BgImage, usize)>;

    /// Whether the song's opening movie is expected to play.
    fn opening_plays(&self, first: &[BgSegment]) -> bool;

    /// Frames that came late, so far.
    fn late(&self) -> u64;

    /// Does what the automatic setting's rule says.
    fn obey(&mut self, action: GuardAction);
}

/// No movies: a shell built without them.
#[derive(Default)]
pub struct NoMovies;

impl Deck for NoMovies {
    fn update(
        &mut self,
        _: &[Vec<BgSegment>],
        _: f64,
        _: &mut BackgroundPipeline,
        _: &wgpu::Device,
        _: &wgpu::Queue,
    ) {
    }

    fn update_clips(
        &mut self,
        _: &[Clip],
        _: &mut BackgroundPipeline,
        _: &wgpu::Device,
        _: &wgpu::Queue,
    ) -> Vec<(String, usize)> {
        Vec::new()
    }

    fn picture_aspect(&mut self) -> Result<f32, bool> {
        Err(false)
    }

    fn stop(&mut self) {}

    fn reset(&mut self) {}

    fn drawing(&self, _: &[Vec<BgSegment>]) -> Option<usize> {
        None
    }

    fn shown_any(&self) -> bool {
        false
    }

    fn version(&self) -> u64 {
        0
    }

    fn shown_ids(&self) -> Vec<(BgImage, usize)> {
        Vec::new()
    }

    fn opening_plays(&self, _: &[BgSegment]) -> bool {
        false
    }

    fn late(&self) -> u64 {
        0
    }

    fn obey(&mut self, _: GuardAction) {}
}

/// A stretch of the frame body, for a shell that times them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lap {
    /// Decoded pictures going up to the GPU.
    Upload,
    /// Background movies.
    Movies,
    /// What is built before the field: generated background, skin,
    /// layers and scripts.
    Scene,
    /// The dancing hand: its preparation and its picture.
    Hand,
    /// The field's passes recorded.
    Render,
}

/// What a shell brings to its plays' drawing.
pub trait PlayHost {
    /// A picture it decoded: the song's backgrounds, a skin's images, a
    /// work's layers' contents, the scripts' images and texts. A
    /// picture's last owner may close it.
    type Image: StageContent;
    /// Its movie decoders.
    type Deck: Deck;

    /// A line for the log.
    fn warn(&self, line: &str);

    /// The skin `key`'s Lua stopped, by its own fault (`why`): held
    /// against the skin.
    fn lua_stopped(&mut self, key: &str, why: &str);

    /// Draws the texts the scripts show and the shell has not drawn yet
    /// (white, as a work's layers' text is), for
    /// [`ScriptView::add_text`]: now, or in a later frame.
    fn draw_texts(&mut self, scripts: &mut ScriptView<Self::Image>);

    /// A picture copied to the GPU in this frame's commands, kept until
    /// they are submitted where the copy happens then.
    fn uploaded(&mut self, _image: Self::Image) {}

    /// The text the lyrics are drawn with, once there is one.
    fn text(&mut self) -> Option<Box<dyn RunRasterizer>>;

    /// Host seconds now (the hand's budget).
    fn seconds(&self) -> f64;

    /// A measured refresh rate as the shell has it.
    fn snap_refresh(&self, hz: f64) -> f64 {
        hz
    }

    /// The frame body finished `lap`.
    fn lap(&mut self, _lap: Lap) {}

    /// The field's passes are about to be recorded on `encoder`.
    fn before_render(&mut self, _device: &wgpu::Device, _encoder: &wgpu::CommandEncoder) {}
}

/// What a frame is drawn into.
pub struct Target<'a> {
    /// The view the scene draws through.
    pub view: &'a wgpu::TextureView,
    /// The plain view skins blend through, where there is one.
    pub plain: Option<&'a wgpu::TextureView>,
    /// Its pixels.
    pub size: [u32; 2],
    /// The format the hand and the lyrics draw in: the plain view's,
    /// else the view's.
    pub format: wgpu::TextureFormat,
}

/// When a frame is drawn.
#[derive(Clone, Copy, Debug)]
pub struct Moment {
    /// Host time now.
    pub now: HostTime,
    /// The refresh the frame is for, when the shell knows it; else the
    /// session predicts it.
    pub present: Option<HostTime>,
    /// A second that ended with this frame ([`PlayView::tick`]).
    pub second: Option<Second>,
}

/// How a frame sits on the screen.
#[derive(Clone, Copy, Debug)]
pub struct Screen {
    /// Target pixels to a logical pixel.
    pub ratio: f32,
    /// The quit button shows, clear of these insets (logical pixels:
    /// top, right, bottom, left).
    pub quit: Option<[f32; 4]>,
    /// The lyrics are drawn over the play.
    pub lyrics: bool,
}

/// What a frame drew that the shell reads.
#[derive(Clone, Copy, Debug)]
pub struct Drawn {
    /// The song second drawn.
    pub song_time: f64,
    /// Where the quit button is (logical pixels: middle and radius),
    /// while it shows.
    pub quit_at: Option<[f32; 3]>,
}

/// A skin readied for a play: the skin, its images by texture index
/// (`None` for built-in art) and the key its Lua's stops count against.
pub type Skin<I> = (NoteskinPlay, Vec<Option<I>>, String);

/// The play field's drawing (module doc).
pub struct PlayView<H: PlayHost> {
    /// The shell's own parts.
    pub host: H,
    pub renderer: Renderer,
    pub dance: crate::dance::Preview,
    /// The song's background movies.
    pub movies: H::Deck,
    /// Its scripts' movies, decoded while drawn, their frames on the
    /// scripts' converter.
    pub script_movies: H::Deck,
    pub fps: FpsMeter,
    /// The display's refresh rate as the system says it, until frames
    /// have measured it.
    pub refresh_hint: Option<f64>,
    noteskin: Option<NoteskinPlay>,
    /// What the skin's Lua stops count against.
    noteskin_key: String,
    /// What a play's first frames would stop to make is made (for the
    /// skin set last).
    warmed: bool,
    /// The song's pictures on the GPU by what they show (renderer ids),
    /// and those decoded waiting for it.
    pictures: Vec<(BgImage, usize)>,
    pending: Vec<(BgImage, H::Image)>,
    /// The background changes, a schedule per layer.
    schedule: [Vec<BgSegment>; LAYERS],
    /// Image and movie textures for the backdrop, rebuilt when either
    /// changes, and what the last frame drew.
    backdrop_textures: BackdropTextures,
    backdrop_drawn: Backdrop,
    frame_shape: FrameShape,
    video: VideoMode,
    /// The automatic setting's rule, and the refresh rate measured
    /// before the play started.
    guard: MovieGuard,
    /// Time the movies took this second (sum, longest, frames they ran
    /// in), ms, and the last full second's (mean, longest).
    video_ms: (f64, f64, u32),
    video_ms_last: (f64, f64),
    /// A work's back and mask layers, and their own quads in the last
    /// frame.
    stage: StageView<H::Image>,
    stage_drawn: (usize, usize),
    /// What is drawn as skins' quads changed since the renderer was told.
    skins_changed: bool,
    /// The song's scripts, why they stopped, the quads they drew in the
    /// last frame they ran, and the sounds they played since the shell
    /// last took them.
    scripts: ScriptView<H::Image>,
    scripts_stopped: Option<Failure>,
    scripts_drawn: (usize, usize),
    sounds_due: Vec<String>,
    /// The chart's lyrics (none when it has none or they are not shown),
    /// the lines the last frame drew, and the 2D pass they are drawn by.
    lyrics: Option<LyricTrack>,
    lyrics_shown: Vec<String>,
    overlay: Option<(Painter, DrawList)>,
    /// A stage's opponents' column's words (`crate::opponents`), once
    /// worded, and where the last frame put it.
    column_words: Option<[String; 2]>,
    column_drawn: Option<crate::opponents::Placed>,
    /// Where the last frame put the opponents' miniatures, if it did,
    /// and whether a battle's moment was shown above them.
    fields_drawn: Option<crate::opponents::Fields>,
    fields_cue: bool,
    /// The song's generated background (`docs/plans/phase-8-gaps.md`,
    /// step 5).
    generated: Option<Generated>,
    entrance: Option<Entrance>,
    /// The song second of the last frame drawn during play.
    playing_at: Option<f64>,
}

impl<H: PlayHost> PlayView<H> {
    pub fn new(host: H, renderer: Renderer) -> PlayView<H> {
        PlayView {
            host,
            renderer,
            dance: Default::default(),
            movies: Default::default(),
            script_movies: Default::default(),
            fps: FpsMeter::new(),
            refresh_hint: None,
            noteskin: None,
            noteskin_key: String::new(),
            warmed: false,
            pictures: Vec::new(),
            pending: Vec::new(),
            schedule: Default::default(),
            backdrop_textures: Default::default(),
            backdrop_drawn: Backdrop::default(),
            frame_shape: FrameShape::default(),
            video: VideoMode::Auto,
            guard: MovieGuard::default(),
            video_ms: (0.0, 0.0, 0),
            video_ms_last: (0.0, 0.0),
            stage: Default::default(),
            stage_drawn: (0, 0),
            skins_changed: false,
            scripts: Default::default(),
            scripts_stopped: None,
            scripts_drawn: (0, 0),
            sounds_due: Vec::new(),
            lyrics: None,
            lyrics_shown: Vec::new(),
            overlay: None,
            column_words: None,
            column_drawn: None,
            fields_drawn: None,
            fields_cue: false,
            generated: None,
            entrance: None,
            playing_at: None,
        }
    }

    /// The display's refresh rate: measured, else the system's, else
    /// 60 Hz.
    pub fn refresh_hz(&self) -> f64 {
        self.fps
            .refresh_estimate()
            .map(|hz| self.host.snap_refresh(hz))
            .or(self.refresh_hint)
            .unwrap_or(60.0)
    }

    /// Counts a frame at `now_ms` (host milliseconds): the second that
    /// ended with it, if one did.
    pub fn tick(&mut self, now_ms: f64) -> Option<Second> {
        let second = self.fps.tick(now_ms).map(|fps| self.fps.second(fps));
        if second.is_some() {
            let (sum, max, n) = std::mem::take(&mut self.video_ms);
            self.video_ms_last = if n > 0 {
                (sum / f64::from(n), max)
            } else {
                (0.0, 0.0)
            };
        }
        second
    }

    /// The video settings of the plays to come.
    pub fn set_video(&mut self, mode: VideoMode, frame: BackgroundFrame) {
        self.video = mode;
        self.frame_shape.setting = frame;
    }

    /// The background changes of the song about to play, a schedule per
    /// layer (empty: none).
    pub fn set_schedule(&mut self, layers: [Vec<BgSegment>; LAYERS]) {
        self.schedule = layers;
    }

    /// The background changes, a schedule per layer.
    pub fn schedule(&self) -> &[Vec<BgSegment>; LAYERS] {
        &self.schedule
    }

    /// A picture decoded for the song, showing `what`: on the GPU at the
    /// next frame outside play; during play only when no change is due
    /// within a second, one a frame, so an upload never costs a frame at
    /// a change.
    pub fn add_background(&mut self, what: BgImage, image: H::Image) {
        if self.pictures.iter().any(|(w, _)| *w == what) {
            return;
        }
        self.pending.push((what, image));
    }

    /// The song's pictures on the GPU.
    pub fn pictures(&self) -> &[(BgImage, usize)] {
        &self.pictures
    }

    /// The pictures decoded go up as [`PlayView::add_background`] says,
    /// `at` the song second in play.
    fn upload_backgrounds(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, at: Option<f64>) {
        let count = backgrounds::uploads_due(&self.schedule, at, self.pending.len());
        let limit = device.limits().max_texture_dimension_2d;
        for (what, image) in self.pending.drain(..count).collect::<Vec<_>>() {
            let [w, h] = image.size();
            if w.max(h) > limit {
                self.host.warn(&format!(
                    "background {w}×{h} exceeds the GPU's {limit} px limit; not shown"
                ));
                continue;
            }
            if w == 0 || h == 0 {
                continue;
            }
            let id = self
                .renderer
                .add_background_image(device, [w, h], |texture| {
                    image.write(queue, texture, [w, h])
                });
            let aspect = Some(w as f32 / h as f32);
            match what {
                BgImage::Song => self.frame_shape.song_bg_aspect = aspect,
                BgImage::Still => self.frame_shape.still_aspect = aspect,
                _ => {}
            }
            self.pictures.push((what, id));
            self.host.uploaded(image);
        }
    }

    /// Before a play: every picture decoded so far and the layers'
    /// contents on the GPU, and the renderer told what draws as skins'
    /// quads.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.upload_backgrounds(device, queue, None);
        self.stage.upload(&mut self.renderer, device, queue);
        self.skins_changed = false;
        self.renderer
            .set_skins_drawn(device, queue, self.skins_drawn());
    }

    /// Draws with `play` (readied for this play's layout and direction)
    /// from now on; `None`: the built-in arrows.
    pub fn set_noteskin(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        play: Option<Skin<H::Image>>,
    ) {
        self.warmed = false;
        self.renderer.clear_noteskin_textures();
        // The layers' and the scripts' textures went with them.
        self.stage.forget_textures();
        self.scripts.forget_textures();
        // Colors as the target blends them: encoded when skins blend as
        // StepMania's do.
        let srgb = self.renderer.skin_colors_linear();
        self.noteskin_key.clear();
        self.noteskin = play.map(|(mut play, images, key)| {
            self.noteskin_key = key;
            play.set_srgb(srgb);
            upload_noteskin(
                &mut self.renderer,
                device,
                queue,
                &mut play,
                images.iter().map(Option::as_ref),
            );
            for image in images.into_iter().flatten() {
                self.host.uploaded(image);
            }
            play
        });
        self.renderer
            .set_skins_drawn(device, queue, self.skins_drawn());
    }

    /// The noteskin drawn, if any, and its key.
    pub fn noteskin(&self) -> Option<&NoteskinPlay> {
        self.noteskin.as_ref()
    }

    pub fn noteskin_key(&self) -> &str {
        &self.noteskin_key
    }

    /// Whether quads are drawn as skins' are: a noteskin's, a work's
    /// layers', the song's scripts'.
    fn skins_drawn(&self) -> bool {
        self.noteskin.is_some() || self.stage.shown() || self.scripts.running()
    }

    /// Continues the prompt's entrance from the first frame drawn.
    pub fn enter_background(&mut self, from: Reveal, reduced: bool) {
        self.entrance = Some(Entrance::resume(from, reduced));
    }

    /// A play starts (again): the skin's commands and the scripts from
    /// their start, the HUD anew, the movies' guard on the refresh rate
    /// now.
    pub fn start(&mut self) {
        self.playing_at = None;
        self.renderer.reset_hud();
        self.renderer.set_miniatures(Vec::new());
        if let Some(n) = self.noteskin.as_mut() {
            n.restart();
        }
        self.scripts.restart();
        self.script_movies.stop();
        self.guard.start(self.refresh_hz());
    }

    /// The play is over for good: no lyrics or layers, the movies reset.
    pub fn clear(&mut self) {
        self.playing_at = None;
        self.set_lyrics(None);
        self.set_stage_track(None);
        self.movies.reset();
    }

    /// The backgrounds' frame before the play starts, for the start
    /// prompt's picture (`None`: the whole screen).
    pub fn prompt_frame(&mut self) -> Option<f32> {
        self.frame_aspect(false)
    }

    /// The shape backgrounds are fitted into (`None`: the whole screen),
    /// by the setting: the automatic one follows the song's first movie,
    /// else its pictures, and settles once a movie frame shows, or when
    /// the song starts with nothing left to learn.
    fn frame_aspect(&mut self, started: bool) -> Option<f32> {
        let movies = &mut self.movies;
        let shown = movies.shown_any();
        self.frame_shape
            .aspect(|| movies.picture_aspect(), shown, started)
    }

    /// The generated background of the plays to come (`None`: none): the
    /// one there is kept while it is the same.
    pub fn set_generated(&mut self, want: Option<Want>) {
        if self.generated.as_ref().map(|g| g.want) != want {
            self.generated = want.map(Generated::new);
            self.backdrop_textures.set_generated(None);
        }
    }

    /// The music's signals for the generated background, analyzed.
    pub fn set_generated_series(&mut self, series: Rc<ddi_library::generative::Series>) {
        if let Some(g) = self.generated.as_mut() {
            g.set_series(series);
        }
    }

    /// What the generated background did, if the song has one.
    pub fn generated(&self) -> Option<crate::generative::Debug> {
        self.generated.as_ref().map(Generated::debug)
    }

    /// What the start prompt shows of the generated background, for a
    /// song without a picture: its frame at the play's first song second
    /// and beat (`first`), for a frame of shape `aspect`
    /// ([`Generated::still`]). `None` without one, or while the cover it
    /// folds is read but not up.
    pub fn generated_still(
        &self,
        first: (f64, f64),
        brightness: f32,
        reduced: bool,
        aspect: f32,
    ) -> Option<crate::generative::Still> {
        self.generated.as_ref()?.still(
            &self.renderer,
            &self.pictures,
            first,
            brightness,
            reduced,
            aspect,
        )
    }

    /// The song's scripts (`docs/plans/scripted-backgrounds.md`; a default
    /// view: none, or they did not load), their movies' deck anew for the
    /// shell to fill. Their text as Init left it is drawn now, while the
    /// prompt shows.
    pub fn set_scripts(&mut self, mut scripts: ScriptView<H::Image>) {
        scripts.forget_textures();
        self.scripts = scripts;
        self.scripts_stopped = None;
        self.scripts_drawn = (0, 0);
        self.sounds_due.clear();
        self.script_movies.reset();
        self.script_movies = Default::default();
        self.host.draw_texts(&mut self.scripts);
    }

    /// The song's scripts taken back (the view is let go: a lost device),
    /// for the next view to draw; `None` without.
    pub fn take_scripts(&mut self) -> Option<ScriptView<H::Image>> {
        if !self.scripts.loaded() {
            return None;
        }
        let mut scripts = std::mem::take(&mut self.scripts);
        scripts.forget_device();
        Some(scripts)
    }

    /// The foregrounds' times for the play about to start (the song
    /// offset moves them).
    pub fn retime_scripts(&mut self, foregrounds: &[Foreground]) {
        self.scripts.retime(foregrounds);
    }

    /// The song's scripts.
    pub fn scripts(&self) -> &ScriptView<H::Image> {
        &self.scripts
    }

    /// Why the song's scripts stopped during the play, if they did.
    pub fn scripts_stopped(&self) -> Option<&Failure> {
        self.scripts_stopped.as_ref()
    }

    /// The scripts' background and foreground quads in the last frame
    /// they ran.
    pub fn scripts_drawn(&self) -> (usize, usize) {
        self.scripts_drawn
    }

    /// The scripts' sounds played since last asked (files relative to
    /// the song's folder), for the shell to play now.
    pub fn take_script_sounds(&mut self) -> Vec<String> {
        std::mem::take(&mut self.sounds_due)
    }

    /// A content of the song's layers decoded, `size` in the stage's px.
    pub fn add_stage_content(&mut self, source: SpriteSource, content: H::Image, size: (f32, f32)) {
        self.stage.add(source, content, size);
    }

    /// The layers of the play about to start (`None`: none shown).
    pub fn set_stage_track(&mut self, track: Option<SpriteTrack>) {
        self.stage.set_track(track);
        self.renderer.set_stage_quads(Vec::new(), Vec::new());
        self.stage_drawn = (0, 0);
        self.skins_changed = true;
    }

    /// The layers' own back and mask quads in the last frame (none while
    /// they are not shown).
    pub fn stage_drawn(&self) -> (usize, usize) {
        self.stage_drawn
    }

    /// The lyrics of the play about to start (`None`, or none in it:
    /// none shown).
    pub fn set_lyrics(&mut self, lyrics: Option<LyricTrack>) {
        self.lyrics = lyrics.filter(|t| !t.is_empty());
        self.lyrics_shown.clear();
    }

    /// Whether the play has lyrics shown.
    pub fn has_lyrics(&self) -> bool {
        self.lyrics.is_some()
    }

    /// The lyrics drawn in the last frame.
    pub fn lyrics_shown(&self) -> &[String] {
        &self.lyrics_shown
    }

    /// Where the last frame drew a stage's opponents' column, if it did.
    pub fn opponents_drawn(&self) -> Option<crate::opponents::Placed> {
        self.column_drawn
    }

    /// Where the last frame drew a stage's opponents' miniature fields,
    /// if it did.
    pub fn opponent_fields(&self) -> Option<&crate::opponents::Fields> {
        self.fields_drawn.as_ref()
    }

    /// The last frame showed a battle's moment beside the field (in the
    /// column's heading or above the miniatures).
    pub fn battle_cue_drawn(&self) -> bool {
        self.column_drawn.is_some_and(|p| p.cue) || (self.fields_drawn.is_some() && self.fields_cue)
    }

    /// What the last frame drew behind the field.
    pub fn backdrop_drawn(&self) -> &Backdrop {
        &self.backdrop_drawn
    }

    /// The automatic video setting's rule.
    pub fn guard(&self) -> &MovieGuard {
        &self.guard
    }

    /// What the movies took in the last full second, ms: mean and longest.
    pub fn video_ms(&self) -> (f64, f64) {
        self.video_ms_last
    }

    /// The 2D pass the lyrics are drawn by, made for `format` once the
    /// shell has their text; whether there is one.
    fn ensure_overlay(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> bool {
        if self
            .overlay
            .as_ref()
            .is_some_and(|(p, _)| p.format() == format)
        {
            return true;
        }
        let Some(text) = self.host.text() else {
            return false;
        };
        let mut painter = Painter::new(device, queue, format);
        painter.set_rasterizer(text);
        self.overlay = Some((painter, DrawList::new()));
        true
    }

    /// Makes a little more of what the play's first frames would stop
    /// for, behind the prompt (on WebGL2 a pipeline compiles while the
    /// page waits, in the first frames of the song otherwise); the
    /// lyrics drawn in `format`.
    pub fn warm(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) {
        if self.warmed {
            return;
        }
        self.renderer
            .set_skins_drawn(device, queue, self.skins_drawn());
        if let Some(g) = self.generated.as_mut() {
            g.warm(device, queue);
        }
        if self.lyrics.is_some() {
            self.ensure_overlay(device, queue, format);
        }
        if self.video != VideoMode::Off {
            self.scripts.warm_movies(device);
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("warm"),
        });
        let layer = self.noteskin.as_ref().map(|n| n as &dyn NoteLayer);
        self.warmed = self.renderer.warm(device, &mut encoder, layer);
        queue.submit(Some(encoder.finish()));
    }

    /// What shows behind the field at song second `t`: both layers.
    fn backdrop(&mut self, t: f64) -> Backdrop {
        // Black under the scripts while they run.
        let black = self.scripts.running().then_some(ddi_render::BLACK);
        let movies = &self.movies;
        let all = self.backdrop_textures.get(
            &self.pictures,
            movies.version(),
            || movies.shown_ids(),
            ddi_render::PLAIN,
            black,
        );
        let opening = movies.opening_plays(&self.schedule[0]);
        let [first, second] = backgrounds::draws_at(&self.schedule, t, all, opening);
        Backdrop { first, second }
    }

    /// One frame of `session` into `target`: the pictures due up, the
    /// movies, the backdrop with the generated background, the skin and
    /// its Lua, a work's layers and the song's scripts, the hand, the
    /// field and the lyrics over it. The shell submits `encoder`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw<A: AudioBackend, I: Inputs>(
        &mut self,
        (device, queue, encoder): (&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder),
        target: &Target<'_>,
        session: &PlaySession<A, I>,
        render: RenderOptions,
        moment: Moment,
        screen: Screen,
    ) -> Drawn {
        let present = moment
            .present
            .unwrap_or_else(|| session.predicted_present(moment.now));
        let frame = session.frame(present);
        let (started, finished) = (session.player.started(), session.player.finished());
        self.playing_at = (started && !finished).then_some(frame.song_time);
        self.upload_backgrounds(device, queue, self.playing_at);
        if std::mem::take(&mut self.skins_changed) {
            self.renderer
                .set_skins_drawn(device, queue, self.skins_drawn());
        }
        self.host.lap(Lap::Upload);
        let frame_aspect = self.frame_aspect(started);
        // Movies follow the song time the notes are drawn at, until the
        // song is over (the results show the last frame).
        if finished {
            self.movies.stop();
        } else {
            let drawing = self.movies.drawing(&self.schedule);
            let before = self.host.seconds();
            self.movies.update(
                &self.schedule,
                frame.song_time,
                self.renderer.background_mut(),
                device,
                queue,
            );
            if drawing.is_some() {
                let spent = (self.host.seconds() - before) * 1000.0;
                let (sum, max, n) = &mut self.video_ms;
                *sum += spent;
                *max = max.max(spent);
                *n += 1;
            }
            if self.video == VideoMode::Auto {
                let refresh = self.refresh_hz();
                let late = self.movies.late();
                if let Some(action) =
                    self.guard
                        .step(moment.now.0 * 1000.0, drawing, moment.second, refresh, late)
                {
                    self.movies.obey(action);
                }
            }
        }
        self.host.lap(Lap::Movies);
        let mut render = render;
        render.hud_time = Some(moment.now.0);
        render.frame_aspect = frame_aspect;
        if let Some(entrance) = &mut self.entrance {
            let reveal = entrance.sample(moment.now.0);
            render.background_reveal = Some(reveal);
            if reveal == Reveal::default() {
                self.entrance = None;
            }
        }
        // A generated background: its texture at the frame's size, in
        // the backdrop's list, drawn when the backdrop shows it.
        if let Some(g) = self.generated.as_mut() {
            g.prepare(
                &mut self.renderer,
                device,
                &mut self.backdrop_textures,
                target.size,
                &render,
            );
        }
        render.backdrop = self.backdrop(frame.song_time);
        self.backdrop_drawn = render.backdrop;
        if let Some(g) = self.generated.as_mut() {
            g.draw_shown(
                &mut self.renderer,
                device,
                queue,
                encoder,
                &render,
                &self.pictures,
                (frame.song_time, frame.beat),
            );
        }
        // The field's place, for the quit button, a work's layers and
        // the lyrics.
        let [w, h] = target.size.map(|v| v as f32);
        let span = ddi_render::scene::field_span(&session.layout);
        let geo = ddi_render::FieldGeometry::placed(w, h, span, render.reverse, render.place);
        // The quit button, clear of the field's receptors and the insets.
        let quit = screen.quit.map(|insets| {
            ddi_render::scene::quit_button(&geo, &session.layout, screen.ratio, insets)
        });
        self.renderer.set_quit_button(quit);
        // The lyrics at this frame's time, over the field's middle, clear
        // of the HUD's gauge and score and of the quit button.
        let lyrics = self
            .lyrics
            .as_ref()
            .filter(|_| screen.lyrics)
            .zip(self.playing_at)
            .map(|(track, t)| {
                let place = ddi_render::lyrics::Place::for_play(
                    &geo,
                    &session.layout,
                    track.stage(),
                    quit,
                    screen.ratio,
                );
                (track.at(t), place)
            })
            .filter(|(lines, _)| !lines.is_empty());
        let (host, key) = (&mut self.host, &self.noteskin_key);
        let layer = self.noteskin.as_mut().map(|n| {
            // The skin's Lua stopped: the play goes on with the built-in
            // skin; the next one tries again, and the second stop turns
            // its Lua off (a stall is not the skin's: not held against
            // it).
            if let Some((why, on_clock)) =
                crate::update_noteskin(n, &frame, &session.player, render.reverse)
            {
                if on_clock {
                    host.warn(&format!("noteskin {key}: {why}"));
                } else {
                    host.lua_stopped(key, why);
                }
            }
            n as &mut dyn NoteLayer
        });
        // A work's back and mask layers, placed as the field is; the
        // song's scripts in the background frame (they load while the
        // play may have begun).
        let layers = draw_layers(
            &mut self.stage,
            &mut self.scripts,
            (&mut self.renderer, device, queue),
            (frame.song_time, frame.beat),
            &self.schedule,
            ([w, h], frame_aspect),
            geo.center_x(span),
            (session.danger_shown(), layer.is_some()),
            |s| host.draw_texts(s),
        );
        self.stage_drawn = layers.as_ref().and_then(|l| l.stage).unwrap_or_default();
        if let Some(drawn) = layers.and_then(|l| l.scripts) {
            if let Some(why) = drawn.stopped {
                host.warn(&format!("scripted backgrounds stopped: {why}"));
                self.scripts_stopped = Some(why);
            }
            self.sounds_due.extend(drawn.sounds);
            self.scripts_drawn = drawn.drawn;
            // Their movies drawn, decoded on their clocks and their new
            // frames drawn into their textures (shown from the next frame
            // on, the first time).
            if finished {
                self.script_movies.stop();
            } else if self.video != VideoMode::Off {
                let deck = &mut self.script_movies;
                self.scripts
                    .play_movies(&mut self.renderer, device, queue, |clips, r| {
                        deck.update_clips(clips, r, device, queue)
                    });
            }
        }
        // A stage's opponents' miniature fields in the side space, clear
        // of the HUD and of the quit button's place (shown or not: the
        // slots never move), drawn by the play's skin after its field
        // (`crate::opponents`); else their column, after.
        let mut keep_out = ddi_render::scene::hud_areas(&geo, span);
        let quit_place = quit.unwrap_or_else(|| {
            ddi_render::scene::quit_button(&geo, &session.layout, screen.ratio, [0.0; 4])
        });
        keep_out.push({
            let [x, y, r] = quit_place;
            [x - r, y - r, x + r, y + r]
        });
        let battle = session
            .battle()
            .filter(|_| crate::opponents::miniatures() && lyrics.is_none());
        // In a stage the dancing hand goes behind the field, out of the
        // miniatures' way (the play study's "Hand behind field"), as large
        // as the band allows clear of the receptors and the judgment; its
        // Off still turns it off. Elsewhere where the settings put it.
        let behind = session
            .battle()
            .map(|_| behind_field(&geo, span, render.reverse, screen.ratio));
        // Its arm cut at the frame's top, softly.
        self.renderer
            .set_field_underlay_fade(behind.map_or(0.0, |r| r[3] * BEHIND_FIELD_FADE));
        self.dance.set_behind_field(behind);
        let fields = battle.and_then(|b| {
            let n = b.feeds().count();
            let mut f =
                crate::opponents::arrange(n, &geo, span, render.reverse, screen.ratio, &keep_out)?;
            // Too few slots: the opponents take turns in them, those the
            // stage keeps in view staying.
            crate::opponents::rotate(
                &mut f,
                n,
                &crate::opponents::priority_seats(b),
                frame.song_time,
                crate::opponents::ROTATION_V0,
                render.reduced_motion,
            );
            Some(f)
        });
        // The local player's attack power above its life (a battle with
        // attacks: M8a's development switch), each opponent's above theirs.
        let bar = |p: Option<(f32, bool)>| {
            p.map(|(power, charged)| ddi_render::BattleBar { power, charged })
        };
        self.renderer.set_battle_bar(
            session
                .battle()
                .and_then(|b| bar(b.my_power(&session.player))),
        );
        let mut minis = self.renderer.take_miniatures();
        minis.clear();
        let mut flashes = crate::opponents::Flashes::new();
        if let (Some(b), Some(f)) = (battle, &fields) {
            let scroll = session.player.options().scroll;
            let hits = layer.as_ref().is_some_and(|l| l.miniature_hits()) || layer.is_none();
            let feeds: Vec<_> = b.feeds().collect();
            let views = b.views();
            for slot in &f.slots {
                let Some(feed) = feeds.get(slot.seat) else {
                    continue;
                };
                let frame = feed.frame(b.track(), scroll, frame.song_time, f.range);
                if !hits {
                    flashes.push(frame.receptors.iter().map(|r| r.flash).collect());
                }
                minis.push(ddi_render::scene::Miniature {
                    out: frame.failed,
                    frame,
                    geometry: slot.field,
                    panel: slot.panel,
                    battle: bar(views.get(slot.seat).and_then(|v| v.power)),
                });
            }
        }
        self.renderer.set_miniatures(minis);
        host.lap(Lap::Scene);
        host.before_render(device, encoder);
        self.dance.before_field(
            device,
            queue,
            encoder,
            &mut self.renderer,
            target.format,
            target.size,
            frame.song_time,
            &|| host.seconds(),
        );
        host.lap(Lap::Hand);
        self.renderer.render(
            device,
            queue,
            encoder,
            target.view,
            target.plain,
            target.size[0],
            target.size[1],
            &frame,
            &session.layout,
            &session.rules.names,
            render,
            layer,
        );
        host.lap(Lap::Render);
        self.dance.after_field(
            device,
            queue,
            encoder,
            target.plain.unwrap_or(target.view),
            target.format,
            target.size,
            frame.song_time,
        );
        self.host.lap(Lap::Hand);
        // A battle's moment no line said, in the miniatures' status line
        // or at the column's top (`ddi_session::announcer::battle`).
        let cue = session.battle_cue(moment.now.0).and_then(|(event, age)| {
            let policy = session.battle_policy()?;
            let number = session.battle().map_or(1, |b| b.setup().number);
            Some((
                cue_words(event, number)?,
                policy.cue_alpha(age, render.reduced_motion),
            ))
        });
        // A stage's opponents beside the field, clear of the HUD and the
        // quit button: around their miniatures, else as the column.
        let column = session.battle().map(|b| {
            let words = self.column_words.get_or_insert_with(|| {
                use ddi_l10n::msg::room as m;
                [m::play_out().into(), m::play_won().into()]
            });
            let mut keep_out = ddi_render::scene::hud_areas(&geo, span);
            keep_out.extend(quit.map(|[x, y, r]| [x - r, y - r, x + r, y + r]));
            let mut opponents = b.views();
            // In miniature, life and out as their streams show them.
            if fields.is_some() {
                for (o, feed) in opponents.iter_mut().zip(b.feeds()) {
                    let shown = feed.shown(frame.song_time);
                    o.life = shown.life;
                    o.out = shown.out;
                }
            }
            (
                crate::opponents::Column {
                    opponents,
                    out: words[0].clone(),
                    won: b.won().then(|| words[1].clone()),
                    cue,
                    more: Some(|n| ddi_l10n::msg::room::play_more(n).into()),
                },
                (geo, span, screen.ratio, keep_out),
            )
        });
        if lyrics.is_none() {
            self.lyrics_shown.clear();
        }
        let column = column.map(|(c, place)| (c, place, fields, flashes));
        if lyrics.is_some() || column.is_some() {
            self.draw_overlay((device, queue, encoder), target, lyrics, column);
        } else {
            self.column_drawn = None;
            self.fields_drawn = None;
        }
        Drawn {
            song_time: frame.song_time,
            quit_at: quit.map(|b| b.map(|v| v / screen.ratio)),
        }
    }

    /// Draws the lyrics' `lines` and a stage's opponents' column through
    /// the 2D pass over the frame just encoded: through the target's plain
    /// view where it has one (blended encoded, as the menus), else through
    /// the view itself.
    #[allow(clippy::type_complexity)]
    fn draw_overlay(
        &mut self,
        (device, queue, encoder): (&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder),
        target: &Target<'_>,
        lyrics: Option<(
            Vec<ddi_engine::lyrics::LyricLine>,
            ddi_render::lyrics::Place,
        )>,
        column: Option<(
            crate::opponents::Column,
            (ddi_render::FieldGeometry, f32, f32, Vec<[f32; 4]>),
            Option<crate::opponents::Fields>,
            crate::opponents::Flashes,
        )>,
    ) {
        if !self.ensure_overlay(device, queue, target.format) {
            return;
        }
        let Some((painter, list)) = self.overlay.as_mut() else {
            return;
        };
        // The last frame's draws were submitted by the shell.
        painter.submitted();
        list.clear();
        {
            let mut measure = painter.measure(1.0);
            if let Some((lines, place)) = &lyrics {
                ddi_render::lyrics::draw(list, lines, place, &mut measure);
            }
            self.column_drawn = None;
            self.fields_drawn = None;
            match &column {
                Some((c, (_, _, ratio, _), Some(fields), flashes)) => {
                    self.fields_cue = crate::opponents::draw_fields(
                        list,
                        c,
                        fields,
                        flashes,
                        *ratio,
                        &mut measure,
                    );
                    self.fields_drawn = Some(fields.clone());
                }
                Some((c, (geo, span, ratio, keep), None, _)) => {
                    self.column_drawn =
                        crate::opponents::draw(list, c, geo, *span, *ratio, keep, &mut measure);
                }
                None => {}
            }
        }
        painter.draw(
            device,
            queue,
            encoder,
            target.plain.unwrap_or(target.view),
            target.size,
            1.0,
            None,
            list,
            &mut |_, _| {},
        );
        let Some((lines, _)) = lyrics else {
            return;
        };
        let shown = lines.iter().filter(|l| l.opacity > 0.0).map(|l| &*l.text);
        if !shown
            .clone()
            .eq(self.lyrics_shown.iter().map(String::as_str))
        {
            self.lyrics_shown = shown.map(str::to_string).collect();
        }
    }
}

/// How much of the hand's frame behind the field fades in from its top
/// (its arm cut there).
const BEHIND_FIELD_FADE: f32 = 0.25;

/// Where the dancing hand goes behind the field `geo` (`span` lanes wide)
/// in a stage, `[x, y, width, height]` in the target's pixels (`ratio` a
/// logical pixel): the field's band, from the judgment's place to the end
/// away from the receptors (the bottom; the top under Reverse), so the
/// receptors and the judgment never stand over it. The combo may.
pub fn behind_field(
    geo: &ddi_render::FieldGeometry,
    span: f32,
    reverse: bool,
    ratio: f32,
) -> [f32; 4] {
    let [mid, band] = ddi_render::scene::filter_band(geo, span);
    let (margin, gap) = (12.0 * ratio, 8.0 * ratio);
    let [_, top, _, bottom] = ddi_render::scene::judgement_area(geo, span);
    let (y0, y1) = if reverse {
        (margin, top - gap)
    } else {
        (bottom + gap, geo.height - margin)
    };
    [mid - band / 2.0, y0, band, (y1 - y0).max(0.0)]
}

/// A battle's moment as the opponents' column words it (`None`: one the
/// play does not show), stage `number` being played.
fn cue_words(event: ddi_library::announcer::Battle, number: u16) -> Option<String> {
    use ddi_l10n::msg::room as m;
    use ddi_library::announcer::Battle;
    Some(
        match event {
            Battle::MatchBegin => m::play_cue_match_begin(),
            Battle::StageBegin => m::play_cue_stage_begin(number),
            Battle::FinalStage => m::play_cue_final_stage(),
            Battle::LifeCritical => m::play_cue_life_critical(),
            Battle::Survivors(n) => m::play_cue_survivors(n),
            Battle::Eliminated => m::play_cue_eliminated(),
            Battle::StageDrawn => m::play_cue_stage_drawn(),
            _ => return None,
        }
        .into(),
    )
}
