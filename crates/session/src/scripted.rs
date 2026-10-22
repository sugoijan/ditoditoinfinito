//! A song's scripted backgrounds and foregrounds in a play
//! (`docs/plans/scripted-backgrounds.md`): folders of the song that its
//! background changes (either layer, [`BgImage::Scripted`]) or its
//! `#FGCHANGES` name. One with a `default.lua` runs as StepMania 5.1 runs
//! it (`Background.cpp`, `Foreground.cpp`) on the noteskins' Lua host, in
//! a VM of the song's own (`ddi_noteskin::lua::LuaSkin` over
//! `Skin::song`); any other is an animation of StepMania 3.9's or ITG's
//! formats (`default.xml`, `BGAnimation.ini`, a 3.0 folder of layers),
//! built natively (`ddi_noteskin::bganimation`) and shown as StepMania
//! 3.9 shows it (`GainFocus`: its tweens stopped, `On` played again).
//!
//! - Every background and foreground is built when the play loads, a
//!   background once for each script and `Color1` it is shown with (one
//!   actor per `BackgroundDef`), in the `UpperLeft` effect's frame.
//! - When a layer's current background becomes a script: the one leaving,
//!   if a script, gets `LoseFocus`; the new one `InitState`, `On` and
//!   `GainFocus`, then is updated from the change's second on. A layer's
//!   current script, and the one leaving it while its transition runs,
//!   are updated every frame with the song time that passed.
//! - A foreground shows from its beat, with `On`, for as long as its tweens
//!   then last, and is never shown again.
//! - The whole play's scripts share one budget per frame; the first
//!   failure that ends the VM (fuel, memory, the deadline, a host limit)
//!   stops them all for the rest of the play ([`Scripts::stopped`]); a
//!   command that errors is reported and the play goes on.
//!
//! The shell gives each frame's song time; nothing here reaches the
//! judging, the clock or the score.

use ddi_actor::{Actor, Clocks};
use ddi_library::backgrounds::{self, BgImage, BgSegment, LAYERS};
use ddi_library::path::RelPathBuf;
use ddi_lua::Failure;
use ddi_noteskin::bganimation::{self, Animation};
use ddi_noteskin::build::{Built, Images, Textures};
use ddi_noteskin::lua::{Env, FrameBudget, LoadBudget, LuaSkin, Pause, Position};
use ddi_noteskin::skin::{Folder, Skin};
use ddi_ui::state::settings::BackgroundFrame;

/// What a song's scripts may spend loading, all together (the skins'
/// load budget).
pub const LOAD_BUDGET: LoadBudget = LoadBudget {
    fuel: 20_000_000,
    seconds: 5.0,
};

/// What they may spend in one frame, all together: half a skin's, the
/// field comes first.
pub const FRAME_BUDGET: FrameBudget = FrameBudget {
    fuel: 100_000,
    seconds: 0.008,
};

/// StepMania's screen height, in its pixels.
pub const SCREEN_HEIGHT: f64 = 480.0;

/// The size the shells draw scripts' text at, in StepMania's pixels
/// (`ddi_noteskin::lua::song_text_style`'s), white, as a work's layers'
/// text is drawn (`crate::stage::TEXT_SCALE` times finer).
pub const TEXT_SIZE: u32 = 20;

/// Different texts a song's scripts may show; past it, a new one is not
/// drawn.
pub const MAX_TEXTS: usize = 64;

/// The song's files its scripts see (`Skin::song`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SongFiles {
    /// The pack's and the song's folder names (`/Songs/<pack>/<song>/`).
    pub pack: String,
    pub song: String,
    /// The files kept, relative to the song's folder
    /// (`ManifestEntry::bg_scripts`).
    pub files: Vec<String>,
    /// The text of each text file among them ([`FileKind::Text`]).
    pub texts: Vec<(String, String)>,
}

impl SongFiles {
    /// The song `song` of pack `pack` with `files` kept (no text read
    /// yet).
    pub fn new(pack: String, song: String, files: &[RelPathBuf]) -> SongFiles {
        SongFiles {
            pack,
            song,
            files: files.iter().map(|f| f.to_string()).collect(),
            texts: Vec::new(),
        }
    }
}

/// What a file of a song's scripts is to the shell loading them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// Text: Lua, and the actor files of StepMania 3.9's formats
    /// (`.ini`, `.xml`, `.sprite`, `.actor`, `.model`, MilkShape `.txt`,
    /// `.redir`), read as text ([`SongFiles::texts`]).
    Text,
    /// An image, decoded ([`Sizes`]).
    Image,
    /// Anything else (a sound, read when the scripts name it).
    Other,
}

impl FileKind {
    /// The kind of `path`, by its extension (any case).
    pub fn of(path: &str) -> FileKind {
        if ddi_noteskin::import::kept_text(path) {
            FileKind::Text
        } else if backgrounds::is_image(path) {
            FileKind::Image
        } else {
            FileKind::Other
        }
    }
}

/// The results' note for scripts that stopped during a play.
pub fn stop_note(why: &str) -> String {
    format!("The song's scripted backgrounds stopped ({why}); the song background showed.")
}

/// The shape of StepMania's screen the scripts are told under a
/// background `frame`: 4:3 under a 4:3 frame, 16:9 otherwise.
pub fn frame_aspect(frame: BackgroundFrame) -> f64 {
    match frame {
        BackgroundFrame::FourThree => 4.0 / 3.0,
        _ => 16.0 / 9.0,
    }
}

/// The environment a song's scripts read: the song's timing and
/// StepMania's screen for a background frame of shape `aspect`: 480
/// pixels tall and as wide as the shape makes it, unrounded
/// (`ScreenDimensions::GetScreenWidth`).
pub fn env(song: &ddi_chart::Song, aspect: f64) -> Env {
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        16.0 / 9.0
    };
    let notes = song.charts.iter().flat_map(|c| c.notes.iter());
    let first = notes.clone().map(|n| n.tick.beat()).reduce(f64::min);
    let last = notes.map(|n| n.end_tick().beat()).reduce(f64::max);
    Env {
        timing: song.timing.clone(),
        first_beat: first.unwrap_or(0.0),
        last_beat: last.unwrap_or(0.0),
        screen: [SCREEN_HEIGHT * aspect, SCREEN_HEIGHT],
        ..Env::default()
    }
}

/// The sizes of the images a song's scripts may load, as decoded (their
/// own pixels), by path relative to the song's folder, any case: what
/// the actors are sized by.
#[derive(Clone, Debug, Default)]
pub struct Sizes(std::collections::HashMap<String, (u32, u32)>);

impl Sizes {
    /// Image `path` (relative to the song's folder) is `size` pixels.
    pub fn insert(&mut self, path: &str, size: (u32, u32)) {
        self.0.insert(path.to_lowercase(), size);
    }
}

impl Images for Sizes {
    fn size(&self, _: ddi_noteskin::Dir, path: &str) -> Option<(u32, u32)> {
        self.0.get(&path.to_lowercase()).copied()
    }

    fn max_size(&self) -> u32 {
        ddi_noteskin::build::MAX_IMAGE_SIDE
    }
}

/// What a background or foreground draws.
enum Drawn {
    /// A script's tree, its Lua's.
    Lua(Box<Actor>),
    /// An animation of StepMania 3.9's formats, and a copy of it as it
    /// loaded (a retry starts it over).
    Native(Box<Animation>, Box<Animation>),
}

impl Drawn {
    fn native(a: Animation) -> Drawn {
        Drawn::Native(Box::new(a.clone()), Box::new(a))
    }

    fn tree(&mut self) -> &mut Actor {
        match self {
            Drawn::Lua(t) => t,
            Drawn::Native(a, _) => &mut a.tree,
        }
    }

    /// Back as it loaded.
    fn reset(&mut self) {
        match self {
            Drawn::Lua(t) => t.init_state(),
            Drawn::Native(a, loaded) => **a = (**loaded).clone(),
        }
    }
}

/// A background as shown with one `Color1`.
struct Background {
    script: String,
    color: [f32; 4],
    /// `None`: its script failed (StepMania's empty actor: nothing shows).
    tree: Option<Drawn>,
}

/// A foreground change.
struct Foreground {
    seconds: f64,
    tree: Option<Drawn>,
    /// Shown since its beat, until the second its tweens ended then.
    until: Option<f64>,
    done: bool,
}

/// A play's scripts.
pub struct Scripts {
    /// The song's VM, when one of them is Lua.
    lua: Option<LuaSkin>,
    textures: Textures,
    backgrounds: Vec<Background>,
    foregrounds: Vec<Foreground>,
    /// Each layer's current segment in the last frame.
    current: [Option<usize>; LAYERS],
    /// The last frame's song second.
    last: Option<f64>,
    /// Seconds of play, for effects on the timer clock.
    timer: f64,
    stopped: Option<String>,
    notes: Vec<String>,
}

impl Scripts {
    /// Builds every script `layers` (the play's schedules) and
    /// `foregrounds` name, from `files` with `images` (their decoded
    /// sizes). `None` when there are none. A failure that ends the VM
    /// fails the load: the play shows no script (the shell then shows
    /// the song background in their place).
    pub fn load(
        files: SongFiles,
        env: Env,
        layers: &[Vec<BgSegment>; LAYERS],
        foregrounds: &[backgrounds::Foreground],
        images: &dyn Images,
        clock: Box<dyn FnMut() -> f64>,
    ) -> Result<Option<Scripts>, String> {
        let load = Scripts::load_sliced(files, env, layers, foregrounds, images, clock, None);
        ddi_noteskin::lua::now(async { load.await.map_err(Failure::Error) })
            .map_err(|f| f.to_string())
    }

    /// [`Scripts::load`], giving the host a turn every
    /// `ddi_noteskin::lua::LOAD_SLICE` fuel (the page keeps drawing while
    /// a song's scripts load, as while a skin's do).
    pub async fn load_sliced(
        files: SongFiles,
        env: Env,
        layers: &[Vec<BgSegment>; LAYERS],
        foregrounds: &[backgrounds::Foreground],
        images: &dyn Images,
        clock: Box<dyn FnMut() -> f64>,
        mut pause: Option<&mut Pause>,
    ) -> Result<Option<Scripts>, String> {
        let scripted = |s: &BgSegment| match &s.image {
            BgImage::Scripted(name) => Some((name.clone(), s.look.color1)),
            _ => None,
        };
        if foregrounds.is_empty() && !layers.iter().flatten().any(|s| scripted(s).is_some()) {
            return Ok(None);
        }
        let pack = if files.pack.is_empty() {
            "Songs".to_string()
        } else {
            files.pack.clone()
        };
        let root = format!("/Songs/{pack}/{}/", files.song);
        let folder = Folder {
            game: pack,
            name: files.song.clone(),
            files: files.files,
            texts: files.texts,
        };
        let native = |name: &str| bganimation::is_native(&folder.files, name);
        let names = || {
            layers
                .iter()
                .flatten()
                .filter_map(scripted)
                .map(|(n, _)| n)
                .chain(foregrounds.iter().map(|f| f.script.clone()))
        };
        // The animations of StepMania 3.9's formats over a copy of the
        // folder; the VM only when a script is Lua.
        let animations = names()
            .any(|n| native(&n))
            .then(|| Skin::song(folder.clone()));
        let screen = env.screen.map(|v| v as f32);
        let mut lua = if names().any(|n| !native(&n)) {
            let lua = LuaSkin::new_sliced(
                Skin::song(folder.clone()),
                env,
                ddi_lua::Settings::default(),
                LOAD_BUDGET,
                clock,
                pause.as_deref_mut(),
            )
            .await
            .map_err(|f| f.to_string())?;
            Some(lua)
        } else {
            None
        };
        let mut textures = Textures::default();
        let mut notes = Vec::new();
        let mut list: Vec<Background> = Vec::new();
        for (script, color) in layers.iter().flatten().filter_map(scripted) {
            if list.iter().any(|b| b.script == script && b.color == color) {
                continue;
            }
            let tree = match (&animations, lua.as_mut()) {
                (Some(skin), _) if native(&script) => Some(Drawn::native(animation(
                    skin,
                    &script,
                    Some(color),
                    screen,
                    (&mut textures, images, &mut notes),
                ))),
                (_, Some(lua)) => {
                    let [r, g, b, a] = color;
                    let tint = format!("{r},{g},{b},{a}");
                    let path = format!("{root}{script}");
                    build(
                        lua,
                        (&path, Some(&tint)),
                        (&mut textures, images, &mut notes),
                        pause.as_deref_mut(),
                    )
                    .await?
                    .map(Drawn::Lua)
                }
                _ => None,
            };
            list.push(Background {
                script,
                color,
                tree,
            });
        }
        let mut fgs = Vec::new();
        for fg in foregrounds {
            let tree = match (&animations, lua.as_mut()) {
                (Some(skin), _) if native(&fg.script) => Some(Drawn::native(animation(
                    skin,
                    &fg.script,
                    None,
                    screen,
                    (&mut textures, images, &mut notes),
                ))),
                (_, Some(lua)) => {
                    let path = format!("{root}{}", fg.script);
                    build(
                        lua,
                        (&path, None),
                        (&mut textures, images, &mut notes),
                        pause.as_deref_mut(),
                    )
                    .await?
                    .map(Drawn::Lua)
                }
                _ => None,
            };
            fgs.push(Foreground {
                seconds: fg.seconds,
                tree,
                until: None,
                done: false,
            });
        }
        if let Some(lua) = &lua {
            notes.extend(lua.warnings());
        }
        notes.dedup();
        Ok(Some(Scripts {
            lua,
            textures,
            backgrounds: list,
            foregrounds: fgs,
            current: [None; LAYERS],
            last: None,
            timer: 0.0,
            stopped: None,
            notes,
        }))
    }

    /// A new play of the same song (a retry): no background on yet, no
    /// foreground shown yet, the animations of StepMania 3.9's formats as
    /// they loaded. Scripts that stopped stay stopped.
    pub fn restart(&mut self) {
        self.current = [None; LAYERS];
        self.last = None;
        self.timer = 0.0;
        for fg in &mut self.foregrounds {
            fg.until = None;
            fg.done = false;
            if let Some(tree) = fg.tree.as_mut() {
                tree.reset();
            }
        }
        for b in &mut self.backgrounds {
            if let Some(tree @ Drawn::Native(..)) = b.tree.as_mut() {
                tree.reset();
            }
        }
    }

    /// The foregrounds' times for the play (the song offset moves them),
    /// in the order [`Scripts::load`] was given them.
    pub fn retime(&mut self, foregrounds: &[backgrounds::Foreground]) {
        for (fg, new) in self.foregrounds.iter_mut().zip(foregrounds) {
            fg.seconds = new.seconds;
        }
    }

    /// The images the scripts draw, by the texture ids their sprites
    /// carry (each a file of the song's, `TextureKey::Skin`).
    pub fn textures(&self) -> &Textures {
        &self.textures
    }

    /// Why the scripts stopped, if they did: nothing is drawn after.
    pub fn stopped(&self) -> Option<&str> {
        self.stopped.as_deref()
    }

    /// What the scripts noted while loading (classes not drawn here,
    /// errors they survived).
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// The sound files the scripts may play (relative to the song's
    /// folder), for the shell to decode ahead.
    pub fn sound_files(&self) -> Vec<String> {
        self.lua.as_ref().map_or(Vec::new(), LuaSkin::sound_files)
    }

    /// The sounds the scripts played since last asked (none once they
    /// stopped), for the shell to play now.
    pub fn take_sounds(&mut self) -> Vec<String> {
        let played = self.lua.as_mut().map_or(Vec::new(), LuaSkin::take_sounds);
        if self.stopped.is_some() {
            return Vec::new();
        }
        played
    }

    /// Every actor tree of the scripts (their text measured through it).
    pub fn trees_mut(&mut self) -> impl Iterator<Item = &mut Actor> {
        self.backgrounds
            .iter_mut()
            .filter_map(|b| b.tree.as_mut())
            .chain(self.foregrounds.iter_mut().filter_map(|f| f.tree.as_mut()))
            .map(Drawn::tree)
    }

    /// The render targets the scripts made (`ActorFrameTexture::Create`),
    /// each one's size by its index, within the song's pixels
    /// (`ddi_noteskin::lua::MAX_TARGET_PIXELS`).
    pub fn render_targets(&self) -> Vec<[u32; 2]> {
        self.lua
            .as_ref()
            .map_or(Vec::new(), LuaSkin::render_targets)
    }

    /// Fuel the scripts spent in the last frame.
    pub fn frame_fuel(&self) -> u64 {
        self.lua.as_ref().map_or(0, LuaSkin::frame_fuel)
    }

    /// The scripted background segment `seg` shows, by its script and
    /// `Color1`.
    fn background_of(&self, seg: &BgSegment) -> Option<usize> {
        let BgImage::Scripted(script) = &seg.image else {
            return None;
        };
        self.backgrounds
            .iter()
            .position(|b| b.script == *script && b.color == seg.look.color1)
    }

    /// One frame at song second `t` (beat `beat` on the song's timing):
    /// the changes' commands, the foregrounds shown and hidden, every
    /// running script updated. `layers` are the play's schedules (the
    /// scripts [`Scripts::load`] was given, perhaps shifted by the song
    /// offset). Returns why the scripts stopped, in the frame they did.
    pub fn frame(
        &mut self,
        t: f64,
        beat: f64,
        layers: &[Vec<BgSegment>; LAYERS],
    ) -> Option<String> {
        if self.stopped.is_some() {
            return None;
        }
        let dt = self.last.map_or(0.0, |l| (t - l).max(0.0));
        self.last = Some(t);
        self.timer += dt;
        if let Some(lua) = self.lua.as_mut() {
            lua.begin_frame(Position { seconds: t, beat }, FRAME_BUDGET);
        }
        let clocks = Clocks {
            timer: self.timer as f32,
            music_seconds: t as f32,
            beat: beat as f32,
            music_seconds_no_offset: t as f32,
            beat_no_offset: beat as f32,
        };
        match self.step(t, dt, &clocks, layers) {
            Ok(()) => None,
            Err(f) => {
                let why = f.to_string();
                self.stopped = Some(why.clone());
                Some(why)
            }
        }
    }

    fn step(
        &mut self,
        t: f64,
        dt: f64,
        clocks: &Clocks,
        layers: &[Vec<BgSegment>; LAYERS],
    ) -> Result<(), Failure> {
        for (layer, schedule) in layers.iter().enumerate() {
            if schedule.is_empty() {
                continue;
            }
            let state = backgrounds::state_at(schedule, t);
            let before = self.current[layer];
            if before != Some(state.current) {
                self.current[layer] = Some(state.current);
                if let Some(b) = before.and_then(|i| self.background_of(schedule.get(i)?)) {
                    self.play(Tree::Background(b), &["LoseFocus"])?;
                }
                if let Some(b) = self.background_of(&schedule[state.current]) {
                    self.come_on(b)?;
                    // From the change's second on (`UpdateCurBGChange`).
                    let first = (t - schedule[state.current].seconds).max(0.0);
                    self.update(Tree::Background(b), first, clocks)?;
                    continue;
                }
            }
            let running = [Some(state.current), state.previous]
                .map(|i| i.and_then(|i| self.background_of(&schedule[i])));
            for b in running.into_iter().flatten() {
                self.update(Tree::Background(b), dt, clocks)?;
            }
        }
        for i in 0..self.foregrounds.len() {
            let fg = &self.foregrounds[i];
            if fg.done || t < fg.seconds {
                continue;
            }
            let delta = match fg.until {
                Some(_) => dt,
                None => {
                    self.play(Tree::Foreground(i), &["On"])?;
                    let fg = &mut self.foregrounds[i];
                    let left = fg.tree.as_mut().map_or(0.0, |t| t.tree().tween_time_left());
                    fg.until = Some(fg.seconds + f64::from(left));
                    t - fg.seconds
                }
            };
            self.update(Tree::Foreground(i), delta.max(0.0), clocks)?;
            let fg = &mut self.foregrounds[i];
            if fg.until.is_some_and(|u| t > u) {
                fg.done = true;
            }
        }
        Ok(())
    }

    /// Background `b` comes on: a script's state set anew, then `On`
    /// and `GainFocus` (StepMania 5.1's `UpdateCurBGChange`); an
    /// animation of StepMania 3.9's formats gains the focus as 3.9's
    /// layers do (`BGAnimationLayer::GainFocus`).
    fn come_on(&mut self, b: usize) -> Result<(), Failure> {
        match self.backgrounds[b].tree.as_mut() {
            Some(Drawn::Native(a, _)) => {
                a.gain_focus();
                Ok(())
            }
            Some(Drawn::Lua(tree)) => {
                tree.init_state();
                self.play(Tree::Background(b), &["On", "GainFocus"])
            }
            None => Ok(()),
        }
    }

    fn slot(&mut self, which: Tree) -> Option<&mut Option<Drawn>> {
        match which {
            Tree::Background(i) => Some(&mut self.backgrounds.get_mut(i)?.tree),
            Tree::Foreground(i) => Some(&mut self.foregrounds.get_mut(i)?.tree),
        }
    }

    /// Runs `lua` on a script's tree's actor id with the tree lent to its
    /// Lua, or `native` on an animation of StepMania 3.9's formats.
    fn lent(
        &mut self,
        which: Tree,
        lua: impl FnOnce(&mut LuaSkin, u32) -> Result<(), Failure>,
        native: impl FnOnce(&mut Animation),
    ) -> Result<(), Failure> {
        let Some(slot) = self.slot(which) else {
            return Ok(());
        };
        let mut tree = match slot.take() {
            Some(Drawn::Lua(tree)) => tree,
            Some(Drawn::Native(mut a, loaded)) => {
                native(&mut a);
                *slot = Some(Drawn::Native(a, loaded));
                return Ok(());
            }
            None => return Ok(()),
        };
        let Some(vm) = self.lua.as_mut() else {
            if let Some(slot) = self.slot(which) {
                *slot = Some(Drawn::Lua(tree));
            }
            return Ok(());
        };
        let lent = vm.lend(&mut tree);
        let r = lua(vm, tree.id);
        if lent {
            vm.reclaim(&mut tree);
        }
        if let Some(slot) = self.slot(which) {
            *slot = Some(Drawn::Lua(tree));
        }
        r
    }

    /// Plays `commands` on a tree.
    fn play(&mut self, which: Tree, commands: &[&str]) -> Result<(), Failure> {
        self.lent(
            which,
            |lua, id| commands.iter().try_for_each(|c| lua.play(id, c, &[])),
            |a| commands.iter().for_each(|c| a.play(c)),
        )
    }

    /// Updates a tree for `dt` seconds.
    fn update(&mut self, which: Tree, dt: f64, clocks: &Clocks) -> Result<(), Failure> {
        self.lent(
            which,
            |lua, id| lua.update(id, dt as f32, clocks),
            |a| a.update(dt as f32, clocks),
        )
    }

    /// What to draw at song second `t`, back to front: each layer's
    /// scripts (the one coming in, then the one leaving over it) with
    /// their opacity through the layer's transition (a crossfade with what
    /// the layer's images draw under them, whatever the transition:
    /// decision 3), then the foregrounds shown; `true` for a foreground.
    /// Nothing once stopped.
    pub fn draws(
        &mut self,
        t: f64,
        layers: &[Vec<BgSegment>; LAYERS],
    ) -> Vec<(&mut Actor, f32, bool)> {
        if self.stopped.is_some() {
            return Vec::new();
        }
        let mut alphas: Vec<(usize, f32)> = Vec::new();
        for schedule in layers {
            if schedule.is_empty() {
                continue;
            }
            let state = backgrounds::state_at(schedule, t);
            let mix = state.mix.clamp(0.0, 1.0);
            let coming = self.background_of(&schedule[state.current]);
            let leaving = state
                .previous
                .and_then(|i| self.background_of(&schedule[i]));
            if let Some(b) = coming {
                alphas.push((b, if state.previous.is_some() { mix } else { 1.0 }));
            }
            if let Some(b) = leaving.filter(|&b| Some(b) != coming) {
                alphas.push((b, 1.0 - mix));
            }
        }
        let mut out: Vec<(&mut Actor, f32, bool)> = Vec::new();
        let mut backgrounds: Vec<Option<&mut Actor>> = self
            .backgrounds
            .iter_mut()
            .map(|b| b.tree.as_mut().map(Drawn::tree))
            .collect();
        for (b, alpha) in alphas {
            if let Some(tree) = backgrounds.get_mut(b).and_then(Option::take) {
                out.push((tree, alpha, false));
            }
        }
        for fg in &mut self.foregrounds {
            let shown = !fg.done && fg.until.is_some_and(|u| t <= u);
            if let Some(tree) = fg.tree.as_mut().filter(|_| shown) {
                out.push((tree.tree(), 1.0, true));
            }
        }
        out
    }
}

/// The animation of StepMania 3.9's formats in the song's folder `name`
/// of `skin` on StepMania's `screen`, its notes kept: a background's in a
/// frame tinted by its `Color1` (as the `UpperLeft` effect tints a
/// script), a foreground's as it is.
fn animation(
    skin: &Skin<'_>,
    name: &str,
    color: Option<[f32; 4]>,
    screen: [f32; 2],
    (textures, images, notes): (&mut Textures, &dyn Images, &mut Vec<String>),
) -> Animation {
    let (mut a, n) = bganimation::load(skin, name, screen, textures, images);
    notes.extend(n);
    if let Some([r, g, b, alpha]) = color {
        let mut frame = Actor::new(ddi_actor::Kind::Frame(vec![std::mem::replace(
            &mut a.tree,
            Actor::new(ddi_actor::Kind::Actor),
        )]));
        let mut p = ddi_actor::Problems::default();
        if let Ok(calls) = ddi_actor::commands::parse(&format!("diffuse,{r},{g},{b},{alpha}")) {
            for c in &calls {
                let _ = frame.call(c, &mut p, 0);
            }
        }
        a.tree = frame;
    }
    a
}

/// Builds the actor at `path` (a background's with its `Color1`), its
/// images among `textures` and its notes kept: the tree its Lua holds
/// for the play, `None` when it draws nothing.
async fn build(
    lua: &mut LuaSkin,
    (path, color): (&str, Option<&str>),
    (textures, images, notes): (&mut Textures, &dyn Images, &mut Vec<String>),
    pause: Option<&mut Pause>,
) -> Result<Option<Box<Actor>>, String> {
    let element = lua
        .actor_file_sliced(path, color, textures, images, pause)
        .await
        .map_err(|f| f.to_string())?;
    notes.extend(element.notes);
    Ok(match element.built {
        Built::Actor(tree) => {
            let mut tree = Box::new(tree);
            lua.reclaim(&mut tree).then_some(tree)
        }
        Built::BuiltIn | Built::Blank => None,
    })
}

/// A tree of [`Scripts`].
#[derive(Clone, Copy)]
enum Tree {
    Background(usize),
    Foreground(usize),
}

#[cfg(test)]
mod tests;
