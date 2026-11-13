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
    /// What is drawn over the play (a stage's miniatures' words, the
    /// ladder, the lines, the warnings; the lyrics): worked out and
    /// recorded by the 2D pass.
    Overlay,
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
    /// A touch screen: what a key does is said as what a tap does.
    pub touch: bool,
}

/// What a frame drew that the shell reads.
#[derive(Clone, Debug)]
pub struct Drawn {
    /// The song second drawn.
    pub song_time: f64,
    /// Where the quit button is (logical pixels: middle and radius),
    /// while it shows.
    pub quit_at: Option<[f32; 3]>,
    /// Where a tap or a click cycles a stage's attack policy (logical
    /// pixels: left, top, right, bottom), while its label shows: the
    /// shell sends `ddi_session::play::TOUCH_POLICY_CONTROL` for a press
    /// there.
    pub policy_at: Option<[f32; 4]>,
    /// A watched stage's fields a tap or a click follows (logical
    /// pixels: left, top, right, bottom), each with the seat it follows
    /// (none: back to every field): the shell sends
    /// `ddi_session::play::TOUCH_WATCH_CONTROL` and the seat (or
    /// `TOUCH_WATCH_ALL`) for a press there, the first that holds it.
    pub watch_at: Vec<(Option<usize>, [f32; 4])>,
    /// Where a tap or a click skips a stage the player is out of to its
    /// results, while that shows: the shell sends
    /// `ddi_session::play::TOUCH_SKIP_CONTROL` for a press there.
    pub skip_at: Option<[f32; 4]>,
}

/// A watched stage as the last frame drew it
/// ([`PlayView::watch_drawn`]): for the shells' debug state and tests,
/// so that they need no pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct WatchDrawn {
    /// `grid` (every field over the whole screen), `focus` (one
    /// competitor's field large, the others beside), or a player's own
    /// field after they are out of the stage: `out` (their life ran out)
    /// or `left` (they left its play: a forfeit).
    pub view: &'static str,
    /// The seat followed.
    pub focus: Option<usize>,
    /// The fields' lane, logical pixels, and their slots (the seat each
    /// shows and its field's band, logical pixels: left, top, right,
    /// bottom), the large one's last.
    pub lane: f32,
    pub fields: Vec<(usize, [f32; 4])>,
    /// The status line's label and moment as worded.
    pub label: Option<String>,
    pub cue: Option<String>,
    /// The attack warnings drawn on the grid's fields: their seats.
    pub warned: Vec<usize>,
    /// The mark over the player's own field (Out, Left the stage) and
    /// the skip's words, while they watch a stage they are out of.
    pub mark: Option<String>,
    /// A paged grid's page (from 1) and pages.
    pub page: Option<(usize, usize)>,
    pub skip: Option<String>,
    /// The stage is decided: its results follow.
    pub over: bool,
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
    /// A stage's opponents' words (`crate::opponents`: "Out", "Stage
    /// won"), once worded.
    shown_words: Option<[String; 3]>,
    /// Where the last frame put the opponents' miniatures, if it did,
    /// and whether a battle's moment was shown in their status line.
    fields_drawn: Option<crate::opponents::Fields>,
    fields_cue: bool,
    /// "Last standing" or "Stage won" as the last frame's status line
    /// said it.
    standing_drawn: Option<String>,
    /// How the last frame's miniatures said their scores.
    fields_score: crate::opponents::ScoreShown,
    /// The miniatures' places as last arranged, and what they were
    /// arranged for (the slots never move: arranged again only when the
    /// field, the screen or what they keep clear of changes).
    fields_for: Option<(FieldsKey, Option<crate::opponents::Fields>)>,
    /// A large field's slots: the opponents that matter to the player
    /// now (`crate::rivals`), kept from frame to frame; and its ladder.
    rivals: crate::rivals::Rivals,
    ladder: Option<crate::rivals::Ladder>,
    /// A paged grid's page and pages, from 0.
    ladder_page: Option<(usize, usize)>,
    /// The ladder as drawn, with its words ([`PlayView::ladder_drawn`]).
    ladder_drawn: Option<(String, crate::rivals::Ladder)>,
    /// The last frame's attack warning and waiting line
    /// (`crate::battle_cue`), if it drew them.
    attack_drawn: Option<crate::battle_cue::Cue>,
    waiting_drawn: bool,
    /// The last frame's attack policy label: the policy and where a tap
    /// cycles it, target pixels (`crate::battle_cue::policy_area`).
    policy_drawn: Option<(ddi_session::battle::Targeting, [f32; 4])>,
    /// The last frame's lines and outlines of who attacks whom
    /// (`crate::attack_links`).
    links_drawn: Vec<crate::attack_links::Shown>,
    /// A watched stage as the last frame drew it.
    watch_drawn: Option<WatchDrawn>,
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
            shown_words: None,
            fields_for: None,
            rivals: crate::rivals::Rivals::default(),
            ladder: None,
            ladder_page: None,
            ladder_drawn: None,
            fields_drawn: None,
            fields_cue: false,
            standing_drawn: None,
            fields_score: Default::default(),
            attack_drawn: None,
            policy_drawn: None,
            links_drawn: Vec::new(),
            watch_drawn: None,
            waiting_drawn: false,
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

    /// Where the last frame drew a stage's opponents' miniature fields,
    /// if it did.
    /// What the 2D pass over the play holds and drew last (its text's
    /// runs drawn and uploaded so far, its draws), once it has drawn.
    pub fn overlay_stats(&self) -> Option<ddi_render::painter::PainterStats> {
        self.overlay.as_ref().map(|(p, _)| p.stats())
    }

    pub fn opponent_fields(&self) -> Option<&crate::opponents::Fields> {
        self.fields_drawn.as_ref()
    }

    /// A large field's ladder as the last frame drew it: its words and
    /// the field it stands for (`crate::rivals`), none in a room of nine.
    pub fn ladder_drawn(&self) -> Option<&(String, crate::rivals::Ladder)> {
        self.ladder_drawn.as_ref()
    }

    /// How the last frame's miniatures said their opponents' scores,
    /// where it drew any.
    pub fn opponent_scores(&self) -> Option<crate::opponents::ScoreShown> {
        self.fields_drawn.as_ref().map(|_| self.fields_score)
    }

    /// The last frame showed a battle's moment beside the field (in the
    /// miniatures' status line).
    pub fn battle_cue_drawn(&self) -> bool {
        self.fields_drawn.is_some() && self.fields_cue
    }

    /// "Last standing" or "Stage won" as the last frame's status line said
    /// it, if it did (`settlement-v1`).
    pub fn standing_drawn(&self) -> Option<&str> {
        self.fields_drawn.as_ref().and(self.standing_drawn.as_deref())
    }

    /// The attack warning the last frame drew beside the field, if any,
    /// and whether it drew the waiting line (`crate::battle_cue`).
    pub fn attack_drawn(&self) -> (Option<crate::battle_cue::Cue>, bool) {
        (self.attack_drawn, self.waiting_drawn)
    }

    /// Who attacks whom as the last frame drew it, for the player's own
    /// attacks: each link's ends, phase, line and outline
    /// (`crate::attack_links`).
    pub fn links_drawn(&self) -> &[crate::attack_links::Shown] {
        &self.links_drawn
    }

    /// A watched stage as the last frame drew it (none: the player
    /// plays, or no stage).
    pub fn watch_drawn(&self) -> Option<&WatchDrawn> {
        self.watch_drawn.as_ref()
    }

    /// The attack policy's label as the last frame drew it: the policy,
    /// and where a tap or a click cycles it (target pixels).
    pub fn policy_drawn(&self) -> Option<(ddi_session::battle::Targeting, [f32; 4])> {
        self.policy_drawn
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
        // (A stage watched on after the player's own play ended goes on.)
        let started = session.player.started();
        let finished = started && !session.song_going();
        self.playing_at = (started && !finished).then_some(frame.song_time);
        // A watched stage (`watch-v0`): a spectator has no field of
        // their own (every field over the whole screen, or the one they
        // follow in the play's field's place); a player out of the stage
        // keeps theirs, dimmed.
        let watch = session.watch(moment.now.0);
        let spectator = watch.filter(|w| w.why == ddi_session::play::Watching::Spectator);
        // A player out of the stage may follow a competitor large too,
        // their own field then a miniature among the others (its seat the
        // last: `own_seat`).
        let focus = watch.and_then(|w| w.focus);
        let following = focus.is_some() && spectator.is_none();
        let own_seat = following
            .then(|| session.battle().map(|b| b.feeds().count()))
            .flatten();
        let grid = spectator.is_some() && focus.is_none();
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
        render.no_field = spectator.is_some() || following;
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
        // (`crate::opponents`), with lyrics over the play too.
        // (A grid has no HUD of the play's own to keep clear of.)
        let mut keep_out = if grid {
            Vec::new()
        } else {
            ddi_render::scene::hud_areas(&geo, span)
        };
        let quit_place = quit.unwrap_or_else(|| {
            ddi_render::scene::quit_button(&geo, &session.layout, screen.ratio, [0.0; 4])
        });
        keep_out.push({
            let [x, y, r] = quit_place;
            [x - r, y - r, x + r, y + r]
        });
        // What a line of who attacks whom keeps clear of besides the band
        // and the miniatures (`crate::attack_links`): the HUD and the
        // quit button's place (the warning's is where one ends).
        let line_keep = keep_out.clone();
        // The attack warning's place, shown or not (the slots never move).
        let attacks = session.battle().is_some_and(|b| b.attacks().is_some());
        if attacks && !grid {
            keep_out.push(crate::battle_cue::place(&geo, span));
        }
        let battle = session.battle();
        // In a stage the dancing hand goes behind the field, out of the
        // miniatures' way (the play study's "Hand behind field"): the
        // whole of it as a solo play shows it, its arm's end included,
        // as large as fits the band past the receptors, with no fade
        // and no cut; its Off still turns it off. Elsewhere where the
        // settings put it.
        let behind = session
            .battle()
            .map(|_| behind_field(&geo, span, render.reverse, screen.ratio));
        self.dance.set_behind_field(behind);
        let fields = battle.and_then(|b| {
            // Everyone but the competitor followed, whose field is the
            // large one.
            let n = b.feeds().count() + usize::from(own_seat.is_some())
                - usize::from(focus.is_some());
            let key = FieldsKey {
                n,
                geo,
                span,
                reverse: render.reverse,
                ratio: screen.ratio,
                keep_out: keep_out.clone(),
                grid,
            };
            if self.fields_for.as_ref().is_none_or(|(k, _)| *k != key) {
                let arranged = if grid {
                    crate::opponents::arrange_grid(
                        n,
                        &geo,
                        render.reverse,
                        screen.ratio,
                        &keep_out,
                    )
                } else {
                    crate::opponents::arrange(
                        n,
                        &geo,
                        span,
                        render.reverse,
                        screen.ratio,
                        &keep_out,
                    )
                };
                self.fields_for = Some((key, arranged));
            }
            let mut f = self.fields_for.as_ref()?.1.clone()?;
            let pinned = crate::opponents::priority_seats(
                b,
                &session.player,
                crate::opponents::ROTATION_V0,
            );
            let paged = grid && n > ddi_session::play::GRID_PAGE;
            if paged {
                // A spectator's grid past a page: the page's fields by
                // standing, the ladder for the whole field.
                let (standings, _) = large_standings(b, &frame, None, frame.song_time);
                let ranked = crate::rivals::order(&standings);
                let page = spectator.map_or(0, |w| w.page);
                let from = page * ddi_session::play::GRID_PAGE;
                for (i, slot) in f.slots.iter_mut().enumerate() {
                    slot.seat = ranked.get(from + i).copied().unwrap_or(usize::MAX);
                    slot.dim = 0.0;
                }
                self.ladder = Some(crate::rivals::Ladder::of(&standings, None));
                self.ladder_page = Some((page, n.div_ceil(ddi_session::play::GRID_PAGE)));
            } else if n > crate::opponents::ROOM_OPPONENTS && !grid {
                // A large field: those that matter to the player now in
                // the slots (around the competitor followed, for a
                // spectator), the ladder for the whole field.
                let (standings, me) = large_standings(b, &frame, focus, frame.song_time);
                // The pinned seats as the slots count them (past a
                // followed competitor's).
                let pinned: Vec<usize> = pinned
                    .iter()
                    .filter(|&&p| Some(p) != focus)
                    .map(|&p| if focus.is_some_and(|f| p > f) { p - 1 } else { p })
                    .collect();
                self.rivals.update(
                    f.slots.len(),
                    &standings,
                    me,
                    &pinned,
                    frame.song_time,
                    crate::rivals::RIVALS_V0,
                );
                self.rivals.apply(
                    &mut f,
                    frame.song_time,
                    crate::rivals::RIVALS_V0,
                    render.reduced_motion,
                );
                self.ladder = f
                    .ladder
                    .is_some()
                    .then(|| crate::rivals::Ladder::of(&standings, me));
                self.ladder_page = None;
            } else {
                self.ladder = None;
                self.ladder_page = None;
                // Too few slots: the opponents take turns in them, those
                // the stage keeps in view staying.
                crate::opponents::rotate(
                    &mut f,
                    n,
                    &pinned,
                    frame.song_time,
                    crate::opponents::ROTATION_V0,
                    render.reduced_motion,
                );
            }
            // The slots beside a followed competitor's field show the
            // others, each in the place its order gives it.
            if let Some(focus) = focus {
                for s in &mut f.slots {
                    if s.seat >= focus && s.seat != usize::MAX {
                        s.seat += 1;
                    }
                }
            }
            Some(f)
        });
        // The followed competitor's field, in the play's field's place.
        let large = battle
            .zip(focus)
            .map(|(_, seat)| crate::opponents::large_slot(seat, &geo, span));
        // The local player's attack power above its life (a battle with
        // attacks: M8a's development switch), each opponent's above theirs.
        let bar = |p: Option<(f32, bool)>| {
            p.map(|(power, charged)| ddi_render::BattleBar { power, charged })
        };
        self.renderer.set_battle_bar(
            session
                .battle()
                .filter(|_| spectator.is_none() && !following)
                .and_then(|b| bar(b.my_power(&session.player))),
        );
        let mut minis = self.renderer.take_miniatures();
        minis.clear();
        let mut flashes = crate::opponents::Flashes::new();
        // Each slot's lanes a shock locked (DDI's lane lock), as its
        // stream says.
        let mut locks: Vec<Vec<Option<f32>>> = Vec::new();
        // The attack warnings on a grid's fields (each recipient's).
        let mut warnings: Vec<(usize, ddi_render::FieldGeometry, crate::battle_cue::Cue)> =
            Vec::new();
        if let Some(b) = battle {
            let scroll = session.player.options().scroll;
            let hits = layer.as_ref().is_some_and(|l| l.miniature_hits()) || layer.is_none();
            let mut feeds: Vec<_> = b.feeds().collect();
            let mut views = b.views();
            if own_seat.is_some() {
                feeds.push(b.own_feed());
                views.push(b.own_view(&session.player, ddi_l10n::msg::room::you().into()));
            }
            let t = frame.song_time;
            // Each slot's field, then the large one's (its notes as far
            // ahead as a play's own shows them).
            let slots = fields.iter().flat_map(|f| f.slots.iter().map(|s| (s, f.range)));
            for (slot, range) in slots.chain(large.iter().map(|s| (s, frame.visible_range))) {
                let Some(feed) = feeds.get(slot.seat) else {
                    continue;
                };
                locks.push(feed.locks(b.track_of(slot.seat).lanes(), t));
                let mini = feed.frame(b.track_of(slot.seat), scroll, t, range);
                if !hits {
                    flashes.push(mini.receptors.iter().map(|r| r.flash).collect());
                }
                if grid {
                    let in_sight = mini.notes.iter().any(|n| {
                        n.note_index >= ddi_engine::battle::HAZARD_INDEX_BASE && n.alpha > 0.0
                    });
                    if let Some(cue) = crate::battle_cue::cue(
                        b.incoming_for(slot.seat),
                        t,
                        in_sight,
                        render.reduced_motion,
                    ) {
                        warnings.push((slot.seat, slot.field, cue));
                    }
                }
                minis.push(ddi_render::scene::Miniature {
                    out: mini.failed,
                    frame: mini,
                    geometry: slot.field,
                    panel: slot.panel,
                    battle: bar(views.get(slot.seat).and_then(|v| v.power)),
                });
            }
        }
        self.renderer.set_miniatures(minis);
        host.lap(Lap::Scene);
        host.before_render(device, encoder);
        // Out of the stage or left it: the hand rests where the play
        // ended rather than dancing on behind the dimmed field.
        let hand_time = session
            .hand_rests_at()
            .map_or(frame.song_time, |at| frame.song_time.min(at));
        // A step down: the hand plays the new chart from its boundary.
        self.dance.follow_steps(&session.player);
        self.dance.before_field(
            device,
            queue,
            encoder,
            &mut self.renderer,
            target.format,
            target.size,
            hand_time,
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
            hand_time,
        );
        self.host.lap(Lap::Hand);
        // A battle's moment no line said, in the miniatures' status line
        // (`ddi_session::announcer::battle`).
        let cue = session.stage_cue(moment.now.0).and_then(|(cue, age)| {
            let policy = session.battle_policy()?;
            let number = session.battle().map_or(1, |b| b.setup().number);
            let rung = |r: u8| {
                session
                    .battle()?
                    .chart_name(r)
                    .map(str::to_string)
            };
            Some((
                cue_words(cue, number, rung)?,
                policy.cue_alpha(age, render.reduced_motion),
            ))
        });
        // Someone watching: the stage's moments, nobody's "you" (who
        // went out, who attacks whom, who won), before a battle's moment
        // no line said.
        let names: Vec<String> = session
            .battle()
            .map_or_else(Vec::new, |b| b.views().into_iter().map(|v| v.name).collect());
        let cue = watch
            .and_then(|w| w.cue)
            .and_then(|(m, age)| {
                let policy = session.battle_policy()?;
                // The decision stays to the end.
                let alpha = match m {
                    ddi_session::battle::WatchMoment::Won { .. }
                    | ddi_session::battle::WatchMoment::Drawn
                    | ddi_session::battle::WatchMoment::LastStanding { .. } => 1.0,
                    _ => policy.cue_alpha(age, render.reduced_motion),
                };
                Some((watch_words(m, &names)?, alpha))
            })
            .or(cue);
        // The status line's quiet word: who is followed, or, for a
        // stage's first seconds, how to follow someone.
        let label: Option<String> = watch
            .filter(|w| w.why == ddi_session::play::Watching::Spectator || w.focus.is_some())
            .and_then(|w| match w.focus {
                Some(seat) => names
                    .get(seat)
                    .map(|n| ddi_l10n::msg::room::watch_following(n.as_str()).into()),
                None if w.age < ddi_session::play::WATCH_HINT_SECONDS && !w.over => {
                    let seats = session.battle().map_or(0, |b| b.feeds().count());
                    Some(if seats > ddi_session::play::GRID_PAGE {
                        ddi_l10n::msg::room::watch_hint_pages().into()
                    } else {
                        ddi_l10n::msg::room::watch_hint().into()
                    })
                }
                None => None,
            })
            // A live stage the room lost (it restarted): the play goes on,
            // reported at its end.
            .or_else(|| {
                session
                    .battle()
                    .filter(|b| b.live_lost())
                    .map(|_| ddi_l10n::msg::room::live_lost().into())
            });
        // Who attacks whom: a line and the recipient's field outlined
        // (`crate::attack_links`). For the player's own attacks; for
        // someone watching, between the competitors' fields (a grid's,
        // or the miniatures beside a player's own dimmed field), or the
        // followed competitor's as a player's own.
        let band_box = {
            let [mid, band] = ddi_render::scene::filter_band(&geo, span);
            [mid - band / 2.0, 0.0, mid + band / 2.0, geo.height]
        };
        let mut links = match (session.battle(), &fields) {
            (Some(b), Some(f)) if watch.is_some() => {
                let seen = b.seen_links();
                match focus {
                    Some(seat) => crate::attack_links::shown(
                        &seen.iter().filter_map(|l| l.of(seat)).collect::<Vec<_>>(),
                        &crate::attack_links::Layout {
                            geo: &geo,
                            span,
                            fields: f,
                            ratio: screen.ratio,
                            keep_out: &line_keep,
                        },
                        frame.song_time,
                        render.reduced_motion,
                        crate::attack_links::ATTACK_LINKS_V0,
                    ),
                    None => {
                        // Around a player's own field, where there is one.
                        let mut keep = line_keep.clone();
                        if !grid {
                            keep.push(band_box);
                        }
                        crate::attack_links::shown_between(
                            &seen,
                            &crate::attack_links::GridLayout {
                                fields: f,
                                size: [w, h],
                                span,
                                ratio: screen.ratio,
                                keep_out: &keep,
                            },
                            frame.song_time,
                            render.reduced_motion,
                            crate::attack_links::ATTACK_LINKS_V0,
                        )
                    }
                }
            }
            (Some(b), Some(f)) => crate::attack_links::shown(
                &b.links(&session.player),
                &crate::attack_links::Layout {
                    geo: &geo,
                    span,
                    fields: f,
                    ratio: screen.ratio,
                    keep_out: &line_keep,
                },
                frame.song_time,
                render.reduced_motion,
                crate::attack_links::ATTACK_LINKS_V0,
            ),
            _ => Vec::new(),
        };
        self.links_drawn.clone_from(&links);
        // A stage's opponents beside the field: what their miniatures
        // show around them.
        // A stage watched after the player is out of it: the mark over
        // their own dimmed field and the skip's words.
        let own_out = watch
            .filter(|w| w.why != ddi_session::play::Watching::Spectator)
            .map(|w| {
                use ddi_l10n::msg::room as m;
                let mark: String = match w.why {
                    ddi_session::play::Watching::Left => m::play_forfeit().into(),
                    _ => m::play_out().into(),
                };
                let skip: Option<String> = (!w.over).then(|| {
                    if screen.touch {
                        m::play_skip_tap().into()
                    } else {
                        m::play_skip("Enter").into()
                    }
                });
                crate::opponents::OwnOut {
                    panel: band_box,
                    mark,
                    skip,
                    dimmed: !following,
                    veil: crate::opponents::OUT_VEIL,
                }
            })
            // Watched through a seat that ended (an audience's): the view
            // stopped, why said over all of it, and the way back.
            .or_else(|| {
                let w = watch.filter(|w| w.ended)?;
                let why = session.battle()?.seat_end()?;
                use ddi_l10n::msg::room as m;
                let skip: String = if screen.touch {
                    m::audience_ended_back_tap().into()
                } else {
                    m::audience_ended_back("Enter").into()
                };
                Some(crate::opponents::OwnOut {
                    panel: [0.0, 0.0, geo.width, geo.height],
                    mark: ddi_ui::screens::audience::seat_end_text(why).into(),
                    skip: (!w.over).then_some(skip),
                    dimmed: true,
                    veil: crate::opponents::ENDED_VEIL,
                })
            });
        // The slots as drawn: the miniatures', then the large field's.
        let all_fields = fields.clone().map(|mut f| {
            f.slots.extend(large);
            f
        });
        let large_at = large.and(all_fields.as_ref().map(|f| f.slots.len() - 1));
        let shown = session.battle().zip(all_fields).map(|(b, fields)| {
            let words = self.shown_words.get_or_insert_with(|| {
                use ddi_l10n::msg::room as m;
                [
                    m::play_out().into(),
                    m::play_won().into(),
                    m::play_last_standing().into(),
                ]
            });
            let mut opponents = b.views();
            let mut all: Vec<_> = b.feeds().collect();
            if own_seat.is_some() {
                opponents.push(b.own_view(&session.player, ddi_l10n::msg::room::you().into()));
                all.push(b.own_feed());
            }
            // Life and out as their streams show them.
            for (o, feed) in opponents.iter_mut().zip(all) {
                let shown = feed.shown(frame.song_time);
                o.life = shown.life;
                o.out = shown.out;
            }
            (
                crate::opponents::Shown {
                    opponents,
                    out: words[0].clone(),
                    // "Stage won", or before it "Last standing" while
                    // attacks against the player are still to come.
                    won: if b.won() {
                        Some(words[1].clone())
                    } else {
                        b.last_standing_me(&session.player).then(|| words[2].clone())
                    },
                    label,
                    large: large_at,
                    stale: b.stale_seats(),
                    own: own_seat.zip(watch).map(|(seat, w)| {
                        use ddi_l10n::msg::room as m;
                        let word: String = match w.why {
                            ddi_session::play::Watching::Left => m::play_forfeit().into(),
                            _ => m::play_out().into(),
                        };
                        (seat, word)
                    }),
                    ladder: self.ladder.clone().map(|l| {
                        use ddi_l10n::msg::room as m;
                        let words: String = match (l.place, self.ladder_page) {
                            (_, Some((page, pages))) => {
                                m::play_ladder_page(page + 1, pages, l.of, l.still_in).into()
                            }
                            (Some(place), None) => m::play_ladder(place, l.of, l.still_in).into(),
                            (None, None) => m::play_ladder_field(l.of, l.still_in).into(),
                        };
                        (words, l)
                    }),
                    cue,
                    // DDI's score (the sum) or percent, as the player's
                    // Score display has their own; another ruleset's
                    // percent.
                    score: if session.rules.is_ddi() && !render.ddi_percent {
                        crate::opponents::ScoreShown::Sum
                    } else {
                        crate::opponents::ScoreShown::Percent
                    },
                },
                fields,
                flashes,
                locks,
                screen.ratio,
            )
        });
        if lyrics.is_none() {
            self.lyrics_shown.clear();
        }
        // An attack against the player (its warning by the receptors) and
        // a full bar waiting (its line under the bar).
        let attack = session.battle().map(|b| {
            let t = frame.song_time;
            let in_sight = frame.notes.iter().any(|n| {
                n.note_index >= ddi_engine::battle::HAZARD_INDEX_BASE
                    && n.alpha > 0.0
                    && (0.0..=geo.height).contains(&geo.y(n.y))
            });
            // The player's own, or the followed competitor's (the frame
            // is then theirs).
            let incoming = match focus {
                Some(seat) => b.incoming_for(seat),
                None => b.incoming(&session.player),
            };
            let cue = crate::battle_cue::cue(incoming, t, in_sight, render.reduced_motion);
            let waiting = b
                .waiting(&session.player)
                .map(|_| String::from(ddi_l10n::msg::room::play_attack_waiting()));
            // How the policy is changed, beside it: the key or the pad's
            // control (on a touch screen the label itself is tapped).
            let policy_key = session.policy_key().and_then(|k| match k {
                ddi_session::play::PolicyKey::Key(_) if screen.touch => None,
                ddi_session::play::PolicyKey::Key(code) => {
                    Some(ddi_ui::controller_hints::policy_keycap(code).label())
                }
                ddi_session::play::PolicyKey::Pad(name) => Some(name.to_string()),
            });
            Attack {
                cue,
                waiting,
                policy: b.policy(&session.player),
                policy_key,
                links: std::mem::take(&mut links),
                warnings: std::mem::take(&mut warnings),
                own_out: own_out.clone(),
                geo,
                span,
                ratio: screen.ratio,
            }
        });
        // What a tap follows, and where one skips.
        let css = |r: [f32; 4]| r.map(|v| v / screen.ratio);
        let mut watch_at: Vec<(Option<usize>, [f32; 4])> = Vec::new();
        if watch.is_some()
            && let Some(f) = &fields
        {
            // (An empty slot, a paged grid's last page's, follows nobody.)
            watch_at.extend(f.slots.iter().filter(|s| s.seat != usize::MAX).map(|s| {
                let [x, y, w, h] = s.rect;
                (
                    // (The player's own miniature: back to their field.)
                    Some(s.seat).filter(|&seat| Some(seat) != own_seat),
                    css([x.min(s.panel[0]), y, (x + w).max(s.panel[2]), y + h]),
                )
            }));
            // The large field: back to every field.
            watch_at.extend(large.iter().map(|s| (None, css(s.panel))));
            // A paged grid's ladder: the next page.
            if self.ladder_page.is_some()
                && let Some(l) = f.ladder
            {
                let [x, y, w, h] = l;
                watch_at.push((
                    Some(ddi_session::play::TOUCH_PAGE_SEAT),
                    css([x, y, x + w, y + h]),
                ));
            }
        }
        let skip_at = own_out
            .as_ref()
            .and_then(|o| o.skip.as_ref().map(|_| css(o.skip_area(screen.ratio))));
        self.watch_drawn = watch.map(|w| WatchDrawn {
            view: match (w.why, w.focus) {
                (ddi_session::play::Watching::Spectator, None) => "grid",
                (ddi_session::play::Watching::Spectator, Some(_)) => "focus",
                (ddi_session::play::Watching::Out, _) => "out",
                (ddi_session::play::Watching::Left, _) => "left",
            },
            focus,
            lane: fields.as_ref().map_or(0.0, |f| f.lane),
            fields: shown
                .as_ref()
                .map_or_else(Vec::new, |(_, f, ..)| {
                    f.slots
                        .iter()
                        .filter(|s| s.seat != usize::MAX)
                        .map(|s| (s.seat, css(s.panel)))
                        .collect()
                }),
            label: shown.as_ref().and_then(|(s, ..)| s.label.clone()),
            cue: shown
                .as_ref()
                .and_then(|(s, ..)| s.cue.as_ref().map(|(c, _)| c.clone())),
            warned: attack
                .as_ref()
                .map_or_else(Vec::new, |a| a.warnings.iter().map(|(s, ..)| *s).collect()),
            mark: own_out.as_ref().map(|o| o.mark.clone()),
            skip: own_out.as_ref().and_then(|o| o.skip.clone()),
            over: w.over,
            page: self.ladder_page.map(|(page, pages)| (page + 1, pages)),
        });
        self.attack_drawn = attack.as_ref().and_then(|a| a.cue);
        self.waiting_drawn = attack.as_ref().is_some_and(|a| a.waiting.is_some());
        self.policy_drawn = attack.as_ref().and_then(|a| {
            Some((
                a.policy?,
                crate::battle_cue::policy_area(&a.geo, a.span, a.ratio),
            ))
        });
        let policy_at = self.policy_drawn.map(|(_, r)| r.map(|v| v / screen.ratio));
        // (An attack's parts show even where no miniature fits.)
        let attack_shown = attack.as_ref().is_some_and(|a| {
            a.cue.is_some() || a.waiting.is_some() || a.policy.is_some() || a.own_out.is_some()
        });
        if lyrics.is_some() || shown.is_some() || attack_shown {
            self.draw_overlay((device, queue, encoder), target, lyrics, shown, attack);
        } else {
            self.fields_drawn = None;
        }
        self.host.lap(Lap::Overlay);
        Drawn {
            song_time: frame.song_time,
            quit_at: quit.map(|b| b.map(|v| v / screen.ratio)),
            policy_at,
            watch_at,
            skip_at,
        }
    }

    /// Draws the lyrics' `lines` and what a stage's opponents' miniatures
    /// show around them through the 2D pass over the frame just encoded: through the target's plain
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
        shown: Option<(
            crate::opponents::Shown,
            crate::opponents::Fields,
            crate::opponents::Flashes,
            Vec<Vec<Option<f32>>>,
            f32,
        )>,
        attack: Option<Attack>,
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
            self.fields_drawn = None;
            self.ladder_drawn = None;
            self.standing_drawn = None;
            if let Some((shown, fields, flashes, locks, ratio)) = shown {
                self.fields_score = shown.score;
                self.standing_drawn = shown.won.clone();
                self.ladder_drawn = shown.ladder.clone().filter(|_| fields.ladder.is_some());
                self.fields_cue = crate::opponents::draw_fields(
                    list,
                    &shown,
                    &fields,
                    &flashes,
                    &locks,
                    ratio,
                    &mut measure,
                );
                self.fields_drawn = Some(fields);
            }
            // A player out of the stage: their own field dimmed, saying
            // why, and how to skip to the results.
            if let Some(o) = attack.as_ref().and_then(|a| a.own_out.as_ref()) {
                let ratio = attack.as_ref().map_or(1.0, |a| a.ratio);
                crate::opponents::draw_own_out(list, o, ratio, &mut measure);
            }
            // Who attacks whom, over the miniatures.
            if let (Some(a), Some(fields)) = (&attack, &self.fields_drawn) {
                crate::attack_links::draw(list, &a.links, fields, a.ratio);
            }
            if let Some(a) = &attack {
                // A grid's warnings, each beside its recipient's field.
                for (_, field, cue) in &a.warnings {
                    crate::battle_cue::draw(list, cue, field, a.span);
                }
                if let Some(cue) = &a.cue {
                    crate::battle_cue::draw(list, cue, &a.geo, a.span);
                }
                if let Some(policy) = a.policy {
                    crate::battle_cue::draw_policy(
                        list,
                        &crate::battle_cue::policy_word(policy),
                        a.policy_key.as_deref(),
                        &a.geo,
                        a.span,
                        a.ratio,
                        &mut measure,
                    );
                }
                if let Some(text) = &a.waiting {
                    crate::battle_cue::draw_waiting(
                        list,
                        text,
                        &a.geo,
                        a.span,
                        a.ratio,
                        &mut measure,
                    );
                }
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

/// A large field's standings for `crate::rivals`: every opponent but a
/// followed competitor (`focus`), as their streams show them at song
/// second `t`, and whom the slots gather around: the player (their own
/// play's life and score in `frame`), the competitor followed, or nobody
/// (a spectator's grid).
fn large_standings(
    b: &ddi_session::battle::Battle,
    frame: &ddi_engine::frame::Frame,
    focus: Option<usize>,
    t: f64,
) -> (Vec<crate::rivals::Standing>, Option<crate::rivals::Standing>) {
    let views = b.views();
    let all: Vec<crate::rivals::Standing> = views
        .iter()
        .zip(b.feeds())
        .map(|(o, feed)| {
            let shown = feed.shown(t);
            crate::rivals::Standing {
                life: shown.life,
                out: shown.out,
                score: o.sum.map_or(o.score, |s| s as f64),
            }
        })
        .collect();
    match focus {
        Some(f) => {
            let me = all.get(f).copied();
            let others = all
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != f)
                .map(|(_, s)| *s)
                .collect();
            (others, me)
        }
        None if b.competes() => {
            let me = crate::rivals::Standing {
                life: frame.life,
                out: frame.failed,
                score: frame
                    .score
                    .ddi
                    .map_or(frame.score.percent.unwrap_or(0.0), |d| d.score as f64),
            };
            (all, Some(me))
        }
        None => (all, None),
    }
}

/// What a stage's miniatures were arranged for (`crate::opponents::arrange`).
#[derive(Clone, Debug, PartialEq)]
struct FieldsKey {
    n: usize,
    geo: ddi_render::FieldGeometry,
    span: f32,
    reverse: bool,
    ratio: f32,
    keep_out: Vec<[f32; 4]>,
    /// Over the whole screen (a watched stage's grid), not beside a
    /// field.
    grid: bool,
}

/// A battle's attacks as a frame shows them (`crate::battle_cue`).
struct Attack {
    cue: Option<crate::battle_cue::Cue>,
    waiting: Option<String>,
    /// The player's attack policy, while its label shows.
    policy: Option<ddi_session::battle::Targeting>,
    /// What changes it, beside it (a key's or a pad control's name).
    policy_key: Option<String>,
    /// Who attacks whom, for the player's own attacks.
    links: Vec<crate::attack_links::Shown>,
    /// A watched grid's warnings: the recipient's seat, its field, the
    /// warning.
    warnings: Vec<(usize, ddi_render::FieldGeometry, crate::battle_cue::Cue)>,
    /// The player's own field after they are out of the stage.
    own_out: Option<crate::opponents::OwnOut>,
    geo: ddi_render::FieldGeometry,
    span: f32,
    ratio: f32,
}

/// Where the dancing hand goes behind the field `geo` (`span` lanes wide)
/// in a stage, `[x, y, width, height]` in the target's pixels (`ratio` a
/// logical pixel): the field's band, from past the receptors' row to the
/// end away from it (the bottom; the top under Reverse), so the receptors
/// never stand over it. The judgment, the combo and the notes do: it is
/// in the background. The hand is shown whole in it
/// (`crate::dance::behind_field_distance`), as a solo play shows it.
pub fn behind_field(
    geo: &ddi_render::FieldGeometry,
    span: f32,
    reverse: bool,
    ratio: f32,
) -> [f32; 4] {
    let [mid, band] = ddi_render::scene::filter_band(geo, span);
    let (margin, gap) = (12.0 * ratio, 8.0 * ratio);
    // The receptors' row: half a lane either side of its middle, and a
    // little more.
    let row = geo.arrow * 0.6;
    let (y0, y1) = if reverse {
        (margin, geo.receptor_y - row - gap)
    } else {
        (geo.receptor_y + row + gap, geo.height - margin)
    };
    [mid - band / 2.0, y0, band, (y1 - y0).max(0.0)]
}

/// A watched stage's moment as the status line words it, `names` the
/// competitors' by seat (`None`: a seat nobody has).
fn watch_words(m: ddi_session::battle::WatchMoment, names: &[String]) -> Option<String> {
    use ddi_l10n::msg::room as r;
    use ddi_session::battle::WatchMoment;
    let name = |seat: usize| names.get(seat).map(String::as_str);
    Some(
        match m {
            WatchMoment::Out { seat } => r::watch_cue_out(name(seat)?),
            WatchMoment::Won { seat } => r::watch_cue_won(name(seat)?),
            WatchMoment::Drawn => r::play_cue_stage_drawn(),
            WatchMoment::Attack { from, to } => r::watch_cue_attack(name(from)?, name(to)?),
            WatchMoment::LastStanding { seat } => r::watch_cue_last_standing(name(seat)?),
        }
        .into(),
    )
}

/// A battle's moment as the opponents' status line words it (`None`: one the
/// play does not show), stage `number` being played: one of the battle's
/// events no line said, or an attack's (against the player, or theirs).
fn cue_words(
    cue: ddi_session::announcer::battle::StageCue,
    number: u16,
    rung: impl Fn(u8) -> Option<String>,
) -> Option<String> {
    use ddi_l10n::msg::room as m;
    use ddi_library::announcer::Battle;
    use ddi_session::announcer::battle::StageCue;
    use ddi_session::battle::AttackMoment;
    let event = match cue {
        StageCue::Attack(AttackMoment::Incoming) => {
            return Some(m::play_cue_attack_incoming().into());
        }
        StageCue::Attack(AttackMoment::Launched) => {
            return Some(m::play_cue_attack_launched().into());
        }
        StageCue::Step { rung: r, up: false } => {
            return Some(m::play_cue_step_down(rung(r)?).into());
        }
        StageCue::Step { rung: r, up: true } => {
            return Some(m::play_cue_step_up(rung(r)?).into());
        }
        StageCue::Event(event) => event,
    };
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
