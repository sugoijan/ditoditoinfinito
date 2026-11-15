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

/// A link shown in the battle's place ([`PlayView::stage_links`], tests
/// and pictures only): from a seat to a seat (none: the local player),
/// `age` seconds after its commitment in every frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StagedLink {
    pub from: Option<usize>,
    pub to: Option<usize>,
    pub age: f32,
    pub chained: bool,
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
    /// The cap after the label while one is followed (what follows the
    /// next player: "tab", a pad's shoulder's name), where it was asked
    /// for (none on a touch screen).
    pub follow_key: Option<String>,
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
    /// Each field's rung as drawn (`fields`' order, the large one's
    /// last; 0 the stage's chart), the largest of the fields' view
    /// delays (seconds) and what changed from frame to frame
    /// ([`WatchTrace`]).
    pub rungs: Vec<u8>,
    pub delay: f64,
    pub trace: WatchTrace,
    /// The stage's song second the frame was drawn for, and the second
    /// of its own the followed competitor's field shows (its view delay
    /// behind the song's: `FieldFeed::view_time`); none where nobody is
    /// followed.
    pub song: f64,
    pub followed_at: Option<f64>,
    /// The song second the background, its movies, the song's scripts
    /// and the lyrics were drawn for: the stage's, whoever is followed.
    pub scene_at: f64,
    /// The followed competitor's name plate on their field
    /// (`crate::battle_cue::draw_followed`): who it says they are.
    pub plate: Option<crate::battle_cue::Followed>,
}

/// A watched stage's frames counted as they went, for catching a flicker
/// (the feedback round I): the frames drawn; the times a place showed
/// another seat while the view, its page and its arrangement stayed (a
/// paged grid's pages are in the room's order since `grid-pages-v1`, so
/// only the turns of too few slots and the rivals of a large field
/// change hands); the times a field's rung changed against its steps
/// taken; and the times the view itself changed (followed, the page, the
/// arrangement).
/// A watched frame's view: the seat followed, the page and the slots.
type TraceView = (Option<usize>, Option<(usize, usize)>, usize);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WatchTrace {
    pub frames: u64,
    pub seat_changes: u64,
    pub rung_changes: u64,
    pub steps: u64,
    pub view_changes: u64,
    /// The last frame's view, places' seats and each seat's rung.
    last: Option<(TraceView, Vec<usize>)>,
    rungs: std::collections::HashMap<usize, u8>,
}

impl WatchTrace {
    /// A frame drawn: the seat followed, the page, the slots' count, the
    /// places' seats and each field's `(seat, rung)`, and the steps the
    /// feeds hold.
    fn frame(
        &mut self,
        view: TraceView,
        seats: Vec<usize>,
        rungs: &[(usize, u8)],
        steps: u64,
    ) {
        self.frames += 1;
        match &self.last {
            Some((v, before)) if *v == view => {
                self.seat_changes +=
                    before.iter().zip(&seats).filter(|(a, b)| a != b).count() as u64;
            }
            Some(_) => self.view_changes += 1,
            None => {}
        }
        for &(seat, rung) in rungs {
            if self.rungs.insert(seat, rung).is_some_and(|r| r != rung) {
                self.rung_changes += 1;
            }
        }
        self.steps = steps;
        self.last = Some((view, seats));
    }
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
    /// The dancing hand giving way when the play runs late
    /// (`ddi_session::hand_load`), and the main-thread time each play
    /// frame burns on purpose (ms; a testing aid, [`PlayView::set_load_ms`]).
    hand_load: ddi_session::hand_load::HandLoad,
    load_ms: f64,
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
    shown_words: Option<[String; 5]>,
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
    /// (What they keep clear of beside the key.) The lines of who attacks
    /// whom between them, routed once while they stay
    /// (`crate::attack_links::Routes`).
    fields_for: Option<(FieldsKey, Vec<[f32; 4]>, Option<crate::opponents::Fields>)>,
    routes: crate::attack_links::Routes,
    /// The miniatures' names and scores as last worded.
    memo: crate::opponents::Memo,
    /// The miniatures' judgments' flashes and lanes' locks of this frame,
    /// their room kept from frame to frame.
    mini_flashes: crate::opponents::Flashes,
    mini_locks: crate::opponents::Locks,
    /// A large field's slots: the opponents that matter to the player
    /// now (`crate::rivals`), kept from frame to frame; and its ladder.
    rivals: crate::rivals::Rivals,
    ladder: Option<crate::rivals::Ladder>,
    /// A paged grid's page and pages, from 0.
    ladder_page: Option<(usize, usize)>,
    /// The ladder as drawn, with its words ([`PlayView::ladder_drawn`]).
    ladder_drawn: Option<(String, crate::rivals::Ladder)>,
    /// The miniatures the last frame drew, each from its feed's frame.
    minis_drawn: usize,
    /// The last frame's attack warning and waiting line
    /// (`crate::battle_cue`), if it drew them.
    attack_drawn: Option<crate::battle_cue::Cue>,
    waiting_drawn: bool,
    /// The last frame's attack policy label: the policy and where a tap
    /// cycles it, target pixels (`crate::battle_cue::policy_area`).
    policy_drawn: Option<(ddi_session::battle::Targeting, [f32; 4])>,
    /// The last frame's chart line (`crate::battle_cue::draw_chart`): the
    /// chart's words and how bright its step still showed.
    chart_drawn: Option<(String, f32)>,
    /// The followed competitor's name plate as the last frame drew it.
    plate_drawn: Option<crate::battle_cue::PlateDrawn>,
    /// The last frame's lines and outlines of who attacks whom
    /// (`crate::attack_links`).
    links_drawn: Vec<crate::attack_links::Shown>,
    /// Links shown in the battle's place ([`PlayView::stage_links`]:
    /// tests and pictures).
    staged_links: Option<Vec<StagedLink>>,
    /// A watched stage as the last frame drew it.
    watch_drawn: Option<WatchDrawn>,
    /// A watched stage's frames as they went ([`WatchTrace`]).
    watch_trace: WatchTrace,
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
            hand_load: Default::default(),
            load_ms: 0.0,
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
            routes: Default::default(),
            staged_links: None,
            memo: Default::default(),
            mini_flashes: Default::default(),
            mini_locks: Default::default(),
            rivals: crate::rivals::Rivals::default(),
            ladder: None,
            ladder_page: None,
            ladder_drawn: None,
            minis_drawn: 0,
            fields_drawn: None,
            fields_cue: false,
            standing_drawn: None,
            fields_score: Default::default(),
            attack_drawn: None,
            policy_drawn: None,
            chart_drawn: None,
            plate_drawn: None,
            links_drawn: Vec::new(),
            watch_drawn: None,
            watch_trace: WatchTrace::default(),
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
        self.renderer.under_miniatures().clear();
        if let Some(n) = self.noteskin.as_mut() {
            n.restart();
        }
        self.scripts.restart();
        self.script_movies.stop();
        self.guard.start(self.refresh_hz());
        self.hand_load.start(self.refresh_hz());
        self.dance.set_load(ddi_session::hand_load::HandStep::Full);
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

    /// What the 2D pass over the play holds and drew last (its text's
    /// runs drawn and uploaded so far, its draws), once it has drawn.
    pub fn overlay_stats(&self) -> Option<ddi_render::painter::PainterStats> {
        self.overlay.as_ref().map(|(p, _)| p.stats())
    }

    /// Where the last frame drew a stage's opponents' miniature fields,
    /// if it did.
    pub fn opponent_fields(&self) -> Option<&crate::opponents::Fields> {
        self.fields_drawn.as_ref()
    }

    /// How many miniatures the last frame drew (each from its feed's
    /// whole frame; a field not drawn asks its summary alone).
    pub fn miniatures_drawn(&self) -> usize {
        self.minis_drawn
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

    /// TESTS AND PICTURES ONLY: these links drawn in the place of the
    /// battle's own (none: the battle's again), each `age` seconds old
    /// in every frame, so a chosen arrangement of lines stands still to
    /// be looked at. Nothing of the battle changes: only what the frame
    /// draws of who attacks whom.
    pub fn stage_links(&mut self, links: Option<Vec<StagedLink>>) {
        self.staged_links = links;
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

    /// The chart line as the last frame drew it: the chart played now as
    /// song select names it, and how bright the step that brought it
    /// still is (0: at rest).
    /// The followed competitor's name plate as the last frame drew it
    /// (`crate::battle_cue::draw_followed`); none where nobody is
    /// followed.
    pub fn plate_drawn(&self) -> Option<&crate::battle_cue::PlateDrawn> {
        self.plate_drawn.as_ref()
    }

    pub fn chart_drawn(&self) -> Option<(&str, f32)> {
        self.chart_drawn.as_ref().map(|(t, k)| (t.as_str(), *k))
    }

    /// What the last frame drew behind the field.
    pub fn backdrop_drawn(&self) -> &Backdrop {
        &self.backdrop_drawn
    }

    /// The automatic video setting's rule.
    pub fn guard(&self) -> &MovieGuard {
        &self.guard
    }

    /// The dancing hand's rule under load (`hand-load-v0`).
    pub fn hand_load(&self) -> &ddi_session::hand_load::HandLoad {
        &self.hand_load
    }

    /// A TESTING AID: each frame of a play burns `ms` of the main thread
    /// on the host's clock before the hand prepares, as a busy machine
    /// would take it (the web's `load=` parameter, desktop's
    /// `--load-ms`); 0, none.
    pub fn set_load_ms(&mut self, ms: f64) {
        self.load_ms = ms.max(0.0);
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
        // The stage's song second (a followed competitor's frame says
        // its field's own, its view delay behind).
        let song = session.player.clock().render_time(present);
        // (A stage watched on after the player's own play ended goes on.)
        let started = session.player.started();
        let finished = started && !session.song_going();
        let refresh = self.refresh_hz();
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
        // What is behind the fields (the background, its movies, the
        // song's scripts, a work's layers) and the lyrics run on the
        // stage's song time, never on a followed field's own, which is
        // its view delay behind and changes with whom is followed.
        let (scene_at, scene_beat) = if focus.is_some() {
            session.stage_clock(present)
        } else {
            (frame.song_time, frame.beat)
        };
        self.playing_at = (started && !finished).then_some(scene_at);
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
                scene_at,
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
        render.backdrop = self.backdrop(scene_at);
        self.backdrop_drawn = render.backdrop;
        if let Some(g) = self.generated.as_mut() {
            g.draw_shown(
                &mut self.renderer,
                device,
                queue,
                encoder,
                &render,
                &self.pictures,
                (scene_at, scene_beat),
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
            (scene_at, scene_beat),
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
        let line_keep = keep_out.len();
        // The attack warning's place, shown or not (the slots never move).
        let battle = session.battle();
        let attacks = battle.is_some_and(|b| b.attacks().is_some());
        if attacks && !grid {
            keep_out.push(crate::battle_cue::place(&geo, span));
        }
        // In a stage the dancing hand goes behind the field, out of the
        // miniatures' way (the play study's "Hand behind field"): the
        // whole of it as a solo play shows it, its arm's end included,
        // as large as fits the band past the receptors, with no fade
        // and no cut; its Off still turns it off. Elsewhere where the
        // settings put it.
        let behind = battle.map(|_| behind_field(&geo, span, render.reverse, screen.ratio));
        self.dance.set_behind_field(behind);
        // The opponents as the play shows them, worked out once a frame
        // (the miniatures, the large field's standings, the status line
        // and the overlay read them).
        let views_now: Vec<ddi_session::battle::OpponentView> =
            battle.map(|b| b.views()).unwrap_or_default();
        // The player's own as the last seat, where they follow someone
        // else's.
        let own_view = battle
            .zip(own_seat)
            .map(|(b, _)| b.own_view(&session.player, ddi_l10n::msg::room::you().into()));
        // The seats shown as miniatures, in their order: everyone's but
        // the competitor followed (whose field is the large one), then
        // the player's own where they follow someone (`own_seat`). The
        // slots are placed and chosen by position in it.
        let shown_seats: Vec<usize> = battle.map_or_else(Vec::new, |b| {
            (0..b.feeds().count())
                .filter(|&s| Some(s) != focus)
                .chain(own_seat)
                .collect()
        });
        let fields = battle.and_then(|b| {
            let n = shown_seats.len();
            let key = FieldsKey {
                n,
                geo,
                span,
                reverse: render.reverse,
                ratio: screen.ratio,
                grid,
                most: render.opponents_up_to_eight,
            };
            if self
                .fields_for
                .as_ref()
                .is_none_or(|(k, kept, _)| *k != key || *kept != keep_out)
            {
                let most = if render.opponents_up_to_eight {
                    crate::opponents::ROOM_OPPONENTS
                } else {
                    usize::MAX
                };
                let arranged = if grid {
                    crate::opponents::arrange_grid(
                        n,
                        most,
                        &geo,
                        span,
                        render.reverse,
                        screen.ratio,
                        &keep_out,
                    )
                } else {
                    crate::opponents::arrange_up_to(
                        n,
                        most,
                        &geo,
                        span,
                        render.reverse,
                        screen.ratio,
                        &keep_out,
                    )
                };
                self.fields_for = Some((key, keep_out.clone(), arranged));
                self.routes.clear();
            }
            if self.fields_for.as_ref().is_none_or(|(.., f)| f.is_none()) {
                // No field fits (a window too small): no ladder nor page
                // of one drawn before (review finding shell-render-6).
                self.ladder = None;
                self.ladder_page = None;
                if grid {
                    session.set_watch_pages(1);
                }
            }
            let mut f = self.fields_for.as_ref()?.2.clone()?;
            // The seats kept in view whatever the turn: whoever attacks
            // the player, then whom they attacked last, while those
            // attacks last and a turn after.
            let every = f64::from(crate::opponents::ROTATION_V0.every);
            let pinned = pinned_seats(own_seat, b.kept_seats(&session.player, every));
            let page = spectator.map_or(0, |w| w.page);
            let (ladder, ladder_page) = crate::opponents::choose(
                &mut f,
                &crate::opponents::Choosing {
                    shown: &shown_seats,
                    grid,
                    page,
                    pinned: &pinned,
                    time: song,
                    reduced: render.reduced_motion,
                },
                &mut self.rivals,
                || large_standings(b, &views_now, &session.player, (focus, own_seat), song),
            );
            self.ladder = ladder;
            self.ladder_page = ladder_page;
            if grid {
                session.set_watch_pages(ladder_page.map_or(1, |(_, pages)| pages));
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
            battle
                .filter(|_| spectator.is_none() && !following)
                .and_then(|b| bar(b.my_power(&session.player))),
        );
        // The last frame's miniatures, each frame's room reused by the
        // one drawn in its place.
        let mut minis = self.renderer.take_miniatures();
        let mut drawn = 0;
        let flashes = &mut self.mini_flashes;
        flashes.clear();
        // Each slot's lanes a shock locked (DDI's lane lock), as its
        // stream says.
        let locks = &mut self.mini_locks;
        locks.clear();
        // The attack warnings on a grid's fields (each recipient's).
        let mut warnings: Vec<(usize, ddi_render::FieldGeometry, crate::battle_cue::Cue)> =
            Vec::new();
        // Each field's rung as drawn and the largest view delay, for the
        // watched stage's trace (`WatchTrace`).
        let mut rungs_drawn: Vec<(usize, u8)> = Vec::new();
        let mut delay_drawn = 0.0_f64;
        if let Some(b) = battle {
            let scroll = session.player.options().scroll;
            let hits = layer.as_ref().is_some_and(|l| l.miniature_hits()) || layer.is_none();
            let mut feeds: Vec<_> = b.feeds().collect();
            if own_seat.is_some() {
                feeds.push(b.own_feed());
            }
            let view_of = |seat: usize| views_now.get(seat).or(own_view.as_ref().filter(|_| seat == views_now.len()));
            // The stage's song second: each field takes its own view
            // delay from it (a followed competitor's frame's time is
            // their field's own, the delay behind already: read from
            // that, their field and the others' were drawn the delay
            // behind twice, the feedback round K's "ghost notes").
            let t = song;
            // Each slot's field, then the large one's (its notes as far
            // ahead as a play's own shows them).
            let slots = fields.iter().flat_map(|f| f.slots.iter().map(|s| (s, f.range)));
            for (slot, range) in slots.chain(large.iter().map(|s| (s, frame.visible_range))) {
                let Some(feed) = feeds.get(slot.seat) else {
                    // An empty slot keeps its rows, empty: they are read
                    // by slot (review finding shell-render-8).
                    locks.push(0, None);
                    if !hits {
                        flashes.push(0, None);
                    }
                    continue;
                };
                let track = b.track_of(slot.seat);
                feed.locks_into(t, locks.push(usize::from(track.lanes()), None));
                if watch.is_some() {
                    rungs_drawn.push((slot.seat, feed.rung_at(t)));
                    delay_drawn = delay_drawn.max(t - feed.view_time(t));
                }
                let followed = large.is_some_and(|l| l.seat == slot.seat);
                let combo = fields.as_ref().is_none_or(|f| f.carry.combo) || followed;
                let plain = !followed && fields.as_ref().is_some_and(|f| !f.carry.skin_notes);
                let fit_combo = !followed && fields.as_ref().is_some_and(|f| f.dense);
                let battle = bar(view_of(slot.seat).and_then(|v| v.power));
                if let Some(m) = minis.get_mut(drawn) {
                    feed.frame_into(&mut m.frame, track, scroll, t, range);
                    m.geometry = slot.field;
                    m.panel = slot.panel;
                    m.seat = slot.seat;
                    (m.combo, m.plain, m.fit_combo, m.battle) = (combo, plain, fit_combo, battle);
                } else {
                    minis.push(ddi_render::scene::Miniature {
                        combo,
                        plain,
                        fit_combo,
                        frame: feed.frame(track, scroll, t, range),
                        geometry: slot.field,
                        panel: slot.panel,
                        seat: slot.seat,
                        battle,
                    });
                }
                let mini = &minis[drawn].frame;
                drawn += 1;
                if !hits {
                    let row = flashes.push(mini.receptors.len(), None);
                    for (f, r) in row.iter_mut().zip(&mini.receptors) {
                        *f = r.flash;
                    }
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
            }
        }
        minis.truncate(drawn);
        // The followed field's own time: the large one's, the last.
        let followed_at = large.and_then(|_| minis.last()).map(|m| m.frame.song_time);
        self.minis_drawn = minis.len();
        self.renderer.set_miniatures(minis);
        // Who attacks whom: a line and the recipient's field outlined
        // (`crate::attack_links`). For the player's own attacks; for
        // someone watching, between the competitors' fields (a grid's,
        // or the miniatures beside a player's own dimmed field), or the
        // followed competitor's as a player's own.
        let band_box = ddi_render::scene::band_box(&geo, span);
        // (Tests' and pictures' own links in the battle's place.)
        let staged: Option<Vec<ddi_session::battle::SeenLink>> =
            self.staged_links.as_ref().map(|links| {
                links
                    .iter()
                    .enumerate()
                    .map(|(i, l)| ddi_session::battle::SeenLink {
                        attack: 9000 + i as u32,
                        from: l.from.unwrap_or(usize::MAX),
                        to: l.to.unwrap_or(usize::MAX),
                        shown_from: song - f64::from(l.age),
                        first: song + 4.0,
                        clear: song + 8.0,
                        chained: l.chained,
                    })
                    .collect()
            });
        let links = match (battle, &fields) {
            (Some(b), Some(f)) if watch.is_some() => {
                let seen = staged.unwrap_or_else(|| b.seen_links());
                match focus {
                    Some(seat) => crate::attack_links::shown(
                        &seen.iter().filter_map(|l| l.of(seat)).collect::<Vec<_>>(),
                        &crate::attack_links::Layout {
                            geo: &geo,
                            span,
                            fields: f,
                            ratio: screen.ratio,
                            keep_out: &keep_out[..line_keep],
                        },
                        &mut self.routes,
                        song,
                        render.reduced_motion,
                        crate::attack_links::ATTACK_LINKS_V1,
                    ),
                    None => {
                        // Around a player's own field, where there is one.
                        let mut keep = keep_out[..line_keep].to_vec();
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
                            &mut self.routes,
                            song,
                            render.reduced_motion,
                            crate::attack_links::ATTACK_LINKS_V1,
                        )
                    }
                }
            }
            (Some(b), Some(f)) => crate::attack_links::shown(
                &staged.map_or_else(
                    || b.links(&session.player),
                    |s| s.iter().filter_map(|l| l.of(usize::MAX)).collect(),
                ),
                &crate::attack_links::Layout {
                    geo: &geo,
                    span,
                    fields: f,
                    ratio: screen.ratio,
                    keep_out: &keep_out[..line_keep],
                },
                &mut self.routes,
                song,
                render.reduced_motion,
                crate::attack_links::ATTACK_LINKS_V1,
            ),
            _ => Vec::new(),
        };
        // Their lines go under the miniatures: the renderer draws them
        // with the scene, before the miniatures' bands.
        let under = self.renderer.under_miniatures();
        under.clear();
        crate::attack_links::draw_behind(under, &links, screen.ratio);
        // (Booked with the scene: the hand's lap stays its own.)
        if self.load_ms > 0.0 && started && !finished {
            let until = host.seconds() + self.load_ms / 1000.0;
            while host.seconds() < until {}
        }
        host.lap(Lap::Scene);
        host.before_render(device, encoder);
        // Out of the stage or left it: the hand rests where the play
        // ended rather than dancing on behind the dimmed field.
        let hand_time = session
            .hand_rests_at()
            .map_or(frame.song_time, |at| frame.song_time.min(at));
        // A step to another chart: the hand plays out what it was given
        // and rests to the end of the stage.
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
        // The hand gives way when frames miss their refresh while it
        // plays: lighter, resting, then gone (one way, a new play fresh).
        let watching = started && !finished && self.dance.playing();
        if let Some(step) =
            self.hand_load
                .frame(moment.now.0 * 1000.0, moment.second, refresh, watching)
        {
            self.dance.set_load(step);
        }
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
            let number = battle.map_or(1, |b| b.setup().number);
            let rung = |r: u8| battle?.chart_name(r).map(str::to_string);
            Some((
                cue_words(cue, number, rung)?,
                policy.cue_alpha(age, render.reduced_motion),
            ))
        });
        // Someone watching: the stage's moments, nobody's "you" (who
        // went out, who attacks whom, who won), before a battle's moment
        // no line said.
        let names: Vec<&str> = if watch.is_some() {
            views_now.iter().map(|v| &*v.name).collect()
        } else {
            Vec::new()
        };
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
                    .map(|&n| ddi_l10n::msg::room::watch_following(n).into()),
                None if w.age < ddi_session::play::WATCH_HINT_SECONDS && !w.over => {
                    Some(if self.ladder_page.is_some() {
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
                battle
                    .filter(|b| b.live_lost())
                    .map(|_| ddi_l10n::msg::room::live_lost().into())
            });
        // While one is followed: how to follow another, after the label
        // (a key's cap or the pad's shoulder; a field is tapped on a
        // touch screen).
        let label_key = watch
            .filter(|w| w.focus.is_some() && !w.over && !screen.touch)
            .and_then(|_| session.follow_key())
            .map(|k| {
                let cap = match k {
                    ddi_session::play::PolicyKey::Key(code) => {
                        ddi_ui::controller_hints::policy_keycap(code).label().to_string()
                    }
                    ddi_session::play::PolicyKey::Pad(name) => name.to_string(),
                };
                (cap, String::from(ddi_l10n::msg::room::watch_next()))
            });
        // A stage's opponents beside the field: what their miniatures
        // show around them.
        // A stage watched after the player is out of it: the mark over
        // their own dimmed field and the skip's words.
        // The mark of the player's own field out of the stage: they left
        // it, or are out.
        let own_word = watch
            .filter(|w| w.why != ddi_session::play::Watching::Spectator)
            .map(|w| -> String {
                match w.why {
                    ddi_session::play::Watching::Left => ddi_l10n::msg::room::play_forfeit().into(),
                    _ => ddi_l10n::msg::room::play_out().into(),
                }
            });
        let own_out = watch
            .zip(own_word.clone())
            .map(|(w, mark)| {
                use ddi_l10n::msg::room as m;
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
                let why = battle?.seat_end()?;
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
        let lane = fields.as_ref().map_or(0.0, |f| f.lane);
        // The followed competitor's name plate (the feedback round K):
        // who they are, their place of everyone as the stage ranks, Out
        // or Left as their field's mark says it.
        let followed = battle.zip(focus).and_then(|(b, seat)| {
            use ddi_l10n::msg::room as m;
            let o = views_now.get(seat)?;
            let feed = b.feeds().nth(seat)?;
            let (others, me) =
                large_standings(b, &views_now, &session.player, (focus, own_seat), song);
            let ladder = crate::rivals::Ladder::of(&others, me);
            Some(crate::battle_cue::Followed {
                name: o.name.clone(),
                level: o.level,
                place: ladder.place.map(|p| m::watch_place(p, ladder.of).into()),
                out: feed.shown(song).out.then(|| {
                    if feed.left() {
                        m::table_left().into()
                    } else {
                        m::play_out().into()
                    }
                }),
            })
        });
        // The slots as drawn: the miniatures', then the large field's.
        let mut fields = fields;
        let large_at = fields.as_mut().zip(large).map(|(f, l)| {
            f.slots.push(l);
            f.slots.len() - 1
        });
        let shown = battle.zip(fields).map(|(b, fields)| {
            let words = self.shown_words.get_or_insert_with(|| {
                use ddi_l10n::msg::room as m;
                [
                    m::play_out().into(),
                    m::play_won().into(),
                    m::play_last_standing().into(),
                    m::play_forfeit().into(),
                    m::table_left().into(),
                ]
            });
            let mut opponents = views_now;
            let mut all: Vec<_> = b.feeds().collect();
            if let Some(own) = own_view {
                opponents.push(own);
                all.push(b.own_feed());
            }
            // Life and out as their streams show them.
            for (o, feed) in opponents.iter_mut().zip(all) {
                let shown = feed.shown(song);
                o.life = shown.life;
                o.out = shown.out;
                o.left = shown.out && feed.left();
            }
            Overlay {
                shown: crate::opponents::Shown {
                    opponents,
                    out: words[0].clone(),
                    left: (words[3].clone(), words[4].clone()),
                    // "Stage won", or before it "Last standing" while
                    // attacks against the player are still to come.
                    won: if b.won() {
                        Some(words[1].clone())
                    } else {
                        b.last_standing_me(&session.player).then(|| words[2].clone())
                    },
                    label,
                    label_key,
                    large: large_at,
                    stale: b.stale_seats(),
                    own: own_seat.zip(own_word),
                    ladder: self.ladder.take().map(|l| {
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
                    // Who attacks the player or is attacked by them: a
                    // name beside a field too small to carry its own.
                    named: links
                        .iter()
                        .filter(|l| l.outline > 0.0 && l.from.is_none())
                        .map(|l| l.seat)
                        .collect(),
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
            }
        });
        self.links_drawn = links;
        if lyrics.is_none() {
            self.lyrics_shown.clear();
        }
        // An attack against the player (its warning by the receptors) and
        // a full bar waiting (its line under the bar).
        let attack = battle.map(|b| {
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
            // The chart played now, where the stage has a ladder: the
            // player's own, or the followed competitor's (none over a
            // spectator's grid).
            let steps: Option<Vec<(f64, u8)>> = match focus {
                Some(seat) => b
                    .feeds()
                    .nth(seat)
                    .map(|f| f.steps().map(|(_, boundary, rung)| (boundary, rung)).collect()),
                None if grid => None,
                None => Some(
                    session
                        .player
                        .steps()
                        .iter()
                        .map(|st| (st.boundary, st.rung))
                        .collect(),
                ),
            };
            let chart = steps
                .and_then(|steps| b.chart_line(steps, t))
                .and_then(|line| {
                    let text = b.chart_shown(line.rung)?.to_string();
                    let step = line.step.map(|(up, age)| {
                        (up, crate::battle_cue::chart_emphasis(age, render.reduced_motion))
                    });
                    Some((text, step))
                });
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
                warnings: std::mem::take(&mut warnings),
                own_out: own_out.clone(),
                chart,
                followed: followed.clone(),
            }
        });
        let skip_at = own_out
            .as_ref()
            .and_then(|o| o.skip.as_ref().map(|_| css(o.skip_area(screen.ratio))));
        if let Some(w) = watch {
            let seats = shown
                .as_ref()
                .map_or_else(Vec::new, |o| o.fields.slots.iter().map(|s| s.seat).collect());
            let steps = battle.map_or(0, |b| b.feeds().map(|f| f.steps().count() as u64).sum());
            let slots = seats.len();
            self.watch_trace
                .frame((w.focus, self.ladder_page, slots), seats, &rungs_drawn, steps);
        } else if self.watch_trace.frames > 0 {
            self.watch_trace = WatchTrace::default();
        }
        self.watch_drawn = watch.map(|w| WatchDrawn {
            view: match (w.why, w.focus) {
                (ddi_session::play::Watching::Spectator, None) => "grid",
                (ddi_session::play::Watching::Spectator, Some(_)) => "focus",
                (ddi_session::play::Watching::Out, _) => "out",
                (ddi_session::play::Watching::Left, _) => "left",
            },
            focus,
            lane,
            fields: shown.as_ref().map_or_else(Vec::new, |o| {
                o.fields
                    .slots
                    .iter()
                    .filter(|s| s.seat != usize::MAX)
                    .map(|s| (s.seat, css(s.panel)))
                    .collect()
            }),
            label: shown.as_ref().and_then(|o| o.shown.label.clone()),
            follow_key: shown
                .as_ref()
                .and_then(|o| o.shown.label_key.as_ref().map(|k| k.0.clone())),
            cue: shown
                .as_ref()
                .and_then(|o| o.shown.cue.as_ref().map(|(c, _)| c.clone())),
            warned: attack
                .as_ref()
                .map_or_else(Vec::new, |a| a.warnings.iter().map(|(s, ..)| *s).collect()),
            mark: own_out.as_ref().map(|o| o.mark.clone()),
            skip: own_out.as_ref().and_then(|o| o.skip.clone()),
            over: w.over,
            page: self.ladder_page.map(|(page, pages)| (page + 1, pages)),
            rungs: rungs_drawn.iter().map(|&(_, r)| r).collect(),
            delay: delay_drawn,
            trace: self.watch_trace.clone(),
            song,
            followed_at,
            scene_at,
            plate: attack.as_ref().and_then(|a| a.followed.clone()),
        });
        self.attack_drawn = attack.as_ref().and_then(|a| a.cue);
        self.waiting_drawn = attack.as_ref().is_some_and(|a| a.waiting.is_some());
        self.chart_drawn = attack.as_ref().and_then(|a| {
            a.chart
                .as_ref()
                .map(|(t, step)| (t.clone(), step.map_or(0.0, |(_, k)| k)))
        });
        self.policy_drawn = attack.as_ref().and_then(|a| {
            Some((
                a.policy?,
                crate::battle_cue::policy_area(&geo, span, screen.ratio),
            ))
        });
        let policy_at = self.policy_drawn.map(|(_, r)| r.map(|v| v / screen.ratio));
        // (An attack's parts show even where no miniature fits.)
        let attack_shown = attack.as_ref().is_some_and(|a| {
            a.cue.is_some()
                || a.waiting.is_some()
                || a.policy.is_some()
                || a.own_out.is_some()
                || a.chart.is_some()
                || a.followed.is_some()
        });
        self.plate_drawn = None;
        if lyrics.is_some() || shown.is_some() || attack_shown {
            let place = (&geo, span, screen.ratio);
            self.draw_overlay((device, queue, encoder), target, place, lyrics, shown, attack);
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
    /// the view itself. The play's field is `geo`, `span` lanes wide,
    /// `ratio` target pixels a logical one.
    fn draw_overlay(
        &mut self,
        (device, queue, encoder): (&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder),
        target: &Target<'_>,
        (geo, span, ratio): (&ddi_render::FieldGeometry, f32, f32),
        lyrics: Option<(
            Vec<ddi_engine::lyrics::LyricLine>,
            ddi_render::lyrics::Place,
        )>,
        shown: Option<Overlay>,
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
            if let Some(Overlay { shown, fields }) = shown {
                self.fields_score = shown.score;
                self.fields_cue = crate::opponents::draw_fields(
                    list,
                    &shown,
                    &fields,
                    &self.mini_flashes,
                    &self.mini_locks,
                    ratio,
                    &mut measure,
                    &mut self.memo,
                );
                self.standing_drawn = shown.won;
                self.ladder_drawn = shown.ladder.filter(|_| fields.ladder.is_some());
                self.fields_drawn = Some(fields);
            }
            // A player out of the stage: their own field dimmed, saying
            // why, and how to skip to the results.
            if let Some(o) = attack.as_ref().and_then(|a| a.own_out.as_ref()) {
                crate::opponents::draw_own_out(list, o, ratio, &mut measure);
            }
            // Who attacks whom: the outlines and where the lines land,
            // over the miniatures (the lines went under them).
            if let (Some(_), Some(fields)) = (&attack, &self.fields_drawn) {
                crate::attack_links::draw(list, &self.links_drawn, fields, ratio);
            }
            if let Some(a) = &attack {
                // A grid's warnings, each beside its recipient's field.
                for (_, field, cue) in &a.warnings {
                    crate::battle_cue::draw(list, cue, field, span);
                }
                if let Some(cue) = &a.cue {
                    crate::battle_cue::draw(list, cue, geo, span);
                }
                // The cue's line under the life gauge: the policy at the
                // left, the chart at the right, the waiting line under the
                // chart where both are due; where no second line fits, the
                // waiting line in the chart's place but for a step's
                // bright moment.
                let clear = a.policy.map(|policy| {
                    crate::battle_cue::draw_policy(
                        list,
                        &crate::battle_cue::policy_word(policy),
                        a.policy_key.as_deref(),
                        geo,
                        span,
                        ratio,
                        &mut measure,
                    )
                });
                // A followed competitor's name plate, in the policy's
                // place (their field has no policy label).
                let reserve = a.chart.as_ref().map_or(0.0, |(text, _)| {
                    crate::battle_cue::chart_room(text, ratio, &mut measure)
                });
                self.plate_drawn = a.followed.as_ref().map(|who| {
                    crate::battle_cue::draw_followed(
                        list,
                        who,
                        reserve,
                        geo,
                        span,
                        ratio,
                        &mut measure,
                    )
                });
                let clear = match (clear, &self.plate_drawn) {
                    (c, Some(p)) => Some(c.unwrap_or(0.0).max(p.end)),
                    (c, None) => c,
                };
                let below = crate::battle_cue::waiting_below_fits(geo, span, ratio);
                let bright = a
                    .chart
                    .as_ref()
                    .is_some_and(|(_, step)| step.is_some_and(|(_, k)| k > 0.0));
                let (chart, waiting_row) = match (&a.chart, &a.waiting) {
                    (Some(_), Some(_)) if below => (a.chart.as_ref(), Some(1)),
                    (Some(_), Some(_)) if bright => (a.chart.as_ref(), None),
                    (Some(_), Some(_)) => (None, Some(0)),
                    (chart, waiting) => (chart.as_ref(), waiting.as_ref().map(|_| 0)),
                };
                if chart.is_none() {
                    self.chart_drawn = None;
                }
                if let Some((text, step)) = chart {
                    crate::battle_cue::draw_chart(
                        list,
                        text,
                        *step,
                        clear.unwrap_or(0.0),
                        geo,
                        span,
                        ratio,
                        &mut measure,
                    );
                }
                if let (Some(text), Some(row)) = (&a.waiting, waiting_row) {
                    crate::battle_cue::draw_waiting_row(
                        list,
                        text,
                        geo,
                        span,
                        ratio,
                        row,
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

/// The large field's standings (`crate::rivals`): every competitor's but
/// the followed competitor's (`focus`), as their streams show them at
/// song second `t`, then the player's own where they follow someone
/// (`own_seat`: the seats shown end with it), and whom the slots gather
/// around: the player, the competitor followed, or nobody (a spectator's
/// grid). The player's own read from the battle and their play (`human`),
/// never from a watched frame, which says nobody failed (review finding
/// shell-render-2); all of them ranked by the stage's rule
/// (shell-render-3).
fn large_standings(
    b: &ddi_session::battle::Battle,
    views: &[ddi_session::battle::OpponentView],
    human: &ddi_engine::player::Player,
    (focus, own_seat): (Option<usize>, Option<usize>),
    t: f64,
) -> (Vec<crate::rivals::Standing>, Option<crate::rivals::Standing>) {
    let rank = b.setup().rules.rank_by();
    let all: Vec<crate::rivals::Standing> = views
        .iter()
        .zip(b.feeds())
        .map(|(o, feed)| {
            let shown = feed.shown(t);
            crate::rivals::Standing {
                life: shown.life,
                out: shown.out,
                score: o.sum.map_or(o.score, |s| s as f64),
                // One who left the stage last, as the stage ranks a
                // forfeit (their stream's end said so), whenever it was.
                out_at: match feed.out_at().filter(|_| shown.out) {
                    Some(_) if feed.left() => f64::NEG_INFINITY,
                    Some(at) => at,
                    None => f64::INFINITY,
                },
                rank,
            }
        })
        .collect();
    let own = b.competes().then(|| {
        let view = human.ruleset().score.view();
        let out = !b.me_in(human);
        crate::rivals::Standing {
            life: if out { 0.0 } else { human.ruleset().gauge.life() },
            out,
            score: view.ddi.map_or(view.percent.unwrap_or(0.0), |d| d.score as f64),
            // A forfeit last, as the stage ranks it.
            out_at: match (b.forfeited_at(), human.gauge_failed_at()) {
                (Some(_), _) => f64::NEG_INFINITY,
                (None, Some(at)) => at,
                (None, None) => f64::INFINITY,
            },
            rank,
        }
    });
    standings_shown(all, own, focus, own_seat)
}

/// The seats kept in view whatever the turn: the player's own where
/// they are out of the stage and follow someone (`own_seat`: their way
/// back to their field, review finding shell-render-5), then those the
/// stage keeps (`kept`).
fn pinned_seats(own_seat: Option<usize>, kept: Vec<usize>) -> Vec<usize> {
    own_seat.into_iter().chain(kept).collect()
}

/// [`large_standings`] from everyone's standings (`all`, in seat order)
/// and the player's own (`own`, none for a spectator): those shown as
/// miniatures in the order the seats shown are (all but the followed
/// `focus`, then the player's own where `own_seat` says it is shown), and
/// whom the slots gather around (the followed, else the player).
fn standings_shown(
    all: Vec<crate::rivals::Standing>,
    own: Option<crate::rivals::Standing>,
    focus: Option<usize>,
    own_seat: Option<usize>,
) -> (Vec<crate::rivals::Standing>, Option<crate::rivals::Standing>) {
    match focus {
        Some(f) => {
            let me = all.get(f).copied();
            let others = all
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != f)
                .map(|(_, s)| *s)
                .chain(own.filter(|_| own_seat.is_some()))
                .collect();
            (others, me)
        }
        None => (all, own),
    }
}

/// What a stage's miniatures were arranged for (`crate::opponents::arrange_up_to`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct FieldsKey {
    n: usize,
    geo: ddi_render::FieldGeometry,
    span: f32,
    reverse: bool,
    ratio: f32,
    /// Over the whole screen (a watched stage's grid), not beside a
    /// field.
    grid: bool,
    /// A room of nine's eight at most (Options' "Up to eight").
    most: bool,
}

/// A battle's attacks as a frame shows them (`crate::battle_cue`).
struct Attack {
    cue: Option<crate::battle_cue::Cue>,
    waiting: Option<String>,
    /// The player's attack policy, while its label shows.
    policy: Option<ddi_session::battle::Targeting>,
    /// What changes it, beside it (a key's or a pad control's name).
    policy_key: Option<String>,
    /// A watched grid's warnings: the recipient's seat, its field, the
    /// warning.
    warnings: Vec<(usize, ddi_render::FieldGeometry, crate::battle_cue::Cue)>,
    /// The player's own field after they are out of the stage.
    own_out: Option<crate::opponents::OwnOut>,
    /// The chart played now, where the stage has a ladder, and the step
    /// that brought it: up, and how bright it still is.
    chart: Option<(String, Option<(bool, f32)>)>,
    /// The competitor followed, for the name plate on their field.
    followed: Option<crate::battle_cue::Followed>,
}

/// What a stage's miniatures show over the play in a frame
/// (`crate::opponents::draw_fields`): around them, their places, their
/// judgments' flashes and their lanes a shock locked.
struct Overlay {
    shown: crate::opponents::Shown,
    fields: crate::opponents::Fields,
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
fn watch_words(m: ddi_session::battle::WatchMoment, names: &[&str]) -> Option<String> {
    use ddi_l10n::msg::room as r;
    use ddi_session::battle::WatchMoment;
    let name = |seat: usize| names.get(seat).copied();
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
        StageCue::Stay { rung: r } => {
            return Some(m::play_cue_stay(rung(r)?).into());
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

#[cfg(test)]
mod standings_tests {
    use super::*;
    use crate::rivals::{Ladder, Standing};

    fn standing(life: f32) -> Standing {
        Standing {
            life,
            score: f64::from(life) * 1000.0,
            ..Standing::default()
        }
    }

    /// A player out of a large stage counts as out on the ladder and has
    /// a place among everyone, whether they follow someone or not; while
    /// they follow someone their own standing is among those shown, last
    /// as their seat is, and their own miniature is kept in view (review
    /// findings shell-render-2 and shell-render-5).
    #[test]
    fn a_player_out_of_a_large_stage_is_out_on_the_ladder_and_keeps_their_miniature() {
        let all: Vec<Standing> = (0..30).map(|i| standing(1.0 - i as f32 / 40.0)).collect();
        let own = Standing {
            life: 0.0,
            out: true,
            out_at: 20.0,
            ..Standing::default()
        };
        // Not following anyone: everyone and the player, who is out.
        let (others, me) = standings_shown(all.clone(), Some(own), None, None);
        let l = Ladder::of(&others, me);
        assert_eq!((l.of, l.still_in, l.place), (31, 30, Some(31)), "{l:?}");
        // Following seat 3: the others and the player's own, last.
        let own_seat = Some(30);
        let (others, me) = standings_shown(all.clone(), Some(own), Some(3), own_seat);
        assert_eq!(others.len(), 30, "29 others and the player's own");
        assert_eq!(others.last(), Some(&own));
        let l = Ladder::of(&others, me);
        assert_eq!((l.of, l.still_in), (31, 30), "{l:?}");
        // The rivals' slots on a phone keep the player's own in view.
        let g = ddi_render::FieldGeometry::placed(390.0 * 3.0, 844.0 * 3.0, 4.0, false, Default::default());
        let mut f = crate::opponents::arrange_up_to(30, usize::MAX, &g, 4.0, false, 3.0, &[]).unwrap();
        assert!(f.rotating, "not all fit: the rivals' slots");
        let shown: Vec<usize> = (0..30).filter(|&s| s != 3).chain(own_seat).collect();
        let pinned = pinned_seats(own_seat, Vec::new());
        let mut rivals = crate::rivals::Rivals::default();
        crate::opponents::choose(
            &mut f,
            &crate::opponents::Choosing {
                shown: &shown,
                grid: false,
                page: 0,
                pinned: &pinned,
                time: 1.0,
                reduced: true,
            },
            &mut rivals,
            || (others, me),
        );
        assert!(f.slots.iter().any(|s| s.seat == 30), "{:?}", f.slots.iter().map(|s| s.seat).collect::<Vec<_>>());
    }
}
