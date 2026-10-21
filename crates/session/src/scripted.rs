//! A song's scripted backgrounds and foregrounds in a play
//! (`docs/plans/scripted-backgrounds.md`): folders of the song with a
//! `default.lua` that its background changes (either layer,
//! [`BgImage::Scripted`]) or its `#FGCHANGES` name, run as StepMania 5.1
//! runs them (`Background.cpp`, `Foreground.cpp`) on the noteskins' Lua
//! host, in a VM of the song's own (`ddi_noteskin::lua::LuaSkin` over
//! `Skin::song`).
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
use ddi_lua::Failure;
use ddi_noteskin::build::{Built, Images, Textures};
use ddi_noteskin::lua::{Env, FrameBudget, LoadBudget, LuaSkin, Pause, Position};
use ddi_noteskin::skin::{Folder, Skin};

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
    /// The text of each Lua file among them.
    pub texts: Vec<(String, String)>,
}

/// The results' note for scripts that stopped during a play.
pub fn stop_note(why: &str) -> String {
    format!("The song's scripted backgrounds stopped ({why}); the song background showed.")
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

/// A background script as shown with one `Color1`.
struct Background {
    script: String,
    color: [f32; 4],
    /// `None`: its script failed (StepMania's empty actor: nothing shows).
    tree: Option<Box<Actor>>,
}

/// A foreground change.
struct Foreground {
    seconds: f64,
    tree: Option<Box<Actor>>,
    /// Shown since its beat, until the second its tweens ended then.
    until: Option<f64>,
    done: bool,
}

/// A play's scripts.
pub struct Scripts {
    lua: LuaSkin,
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

/// The texts of a failure, for the results and the log.
fn reason(f: &Failure) -> String {
    format!("{f}")
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
        ddi_noteskin::lua::now(async { load.await.map_err(Failure::Error) }).map_err(|f| reason(&f))
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
        let mut lua = LuaSkin::new_sliced(
            Skin::song(folder),
            env,
            ddi_lua::Settings::default(),
            LOAD_BUDGET,
            clock,
            pause.as_deref_mut(),
        )
        .await
        .map_err(|f| reason(&f))?;
        let mut textures = Textures::default();
        let mut notes = Vec::new();
        let mut list: Vec<Background> = Vec::new();
        for (script, color) in layers.iter().flatten().filter_map(scripted) {
            if list.iter().any(|b| b.script == script && b.color == color) {
                continue;
            }
            let [r, g, b, a] = color;
            let tint = format!("{r},{g},{b},{a}");
            let path = format!("{root}{script}");
            let tree = build(
                &mut lua,
                (&path, Some(&tint)),
                (&mut textures, images, &mut notes),
                pause.as_deref_mut(),
            )
            .await?;
            list.push(Background {
                script,
                color,
                tree,
            });
        }
        let mut fgs = Vec::new();
        for fg in foregrounds {
            let path = format!("{root}{}", fg.script);
            let tree = build(
                &mut lua,
                (&path, None),
                (&mut textures, images, &mut notes),
                pause.as_deref_mut(),
            )
            .await?;
            fgs.push(Foreground {
                seconds: fg.seconds,
                tree,
                until: None,
                done: false,
            });
        }
        notes.extend(lua.warnings());
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
    /// foreground shown yet. Scripts that stopped stay stopped.
    pub fn restart(&mut self) {
        self.current = [None; LAYERS];
        self.last = None;
        self.timer = 0.0;
        for fg in &mut self.foregrounds {
            fg.until = None;
            fg.done = false;
            if let Some(tree) = fg.tree.as_mut() {
                tree.init_state();
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
        self.lua.sound_files()
    }

    /// The sounds the scripts played since last asked (none once they
    /// stopped), for the shell to play now.
    pub fn take_sounds(&mut self) -> Vec<String> {
        let played = self.lua.take_sounds();
        if self.stopped.is_some() {
            return Vec::new();
        }
        played
    }

    /// Every actor tree of the scripts (their text measured through it).
    pub fn trees_mut(&mut self) -> impl Iterator<Item = &mut Actor> {
        self.backgrounds
            .iter_mut()
            .filter_map(|b| b.tree.as_deref_mut())
            .chain(
                self.foregrounds
                    .iter_mut()
                    .filter_map(|f| f.tree.as_deref_mut()),
            )
    }

    /// Fuel the scripts spent in the last frame.
    pub fn frame_fuel(&self) -> u64 {
        self.lua.frame_fuel()
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
        self.lua
            .begin_frame(Position { seconds: t, beat }, FRAME_BUDGET);
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
                let why = reason(&f);
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
                    if let Some(tree) = self.backgrounds[b].tree.as_mut() {
                        tree.init_state();
                    }
                    self.play(Tree::Background(b), &["On", "GainFocus"])?;
                    // From the change's second on (`UpdateCurBGChange`).
                    let first = (t - schedule[state.current].seconds).max(0.0);
                    self.update(Tree::Background(b), first, clocks)?;
                    continue;
                }
            }
            let running = [Some(state.current), state.previous];
            let running: Vec<usize> = running
                .into_iter()
                .flatten()
                .filter_map(|i| self.background_of(&schedule[i]))
                .collect();
            for b in running {
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
                    let left = fg.tree.as_ref().map_or(0.0, |t| t.tween_time_left());
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

    fn slot(&mut self, which: Tree) -> Option<&mut Option<Box<Actor>>> {
        match which {
            Tree::Background(i) => Some(&mut self.backgrounds.get_mut(i)?.tree),
            Tree::Foreground(i) => Some(&mut self.foregrounds.get_mut(i)?.tree),
        }
    }

    /// Runs `f` on a tree's actor id with the tree lent to its Lua.
    fn lent(
        &mut self,
        which: Tree,
        f: impl FnOnce(&mut LuaSkin, u32) -> Result<(), Failure>,
    ) -> Result<(), Failure> {
        let Some(mut tree) = self.slot(which).and_then(Option::take) else {
            return Ok(());
        };
        let lent = self.lua.lend(&mut tree);
        let r = f(&mut self.lua, tree.id);
        if lent {
            self.lua.reclaim(&mut tree);
        }
        if let Some(slot) = self.slot(which) {
            *slot = Some(tree);
        }
        r
    }

    /// Plays `commands` on a tree.
    fn play(&mut self, which: Tree, commands: &[&str]) -> Result<(), Failure> {
        self.lent(which, |lua, id| {
            commands.iter().try_for_each(|c| lua.play(id, c, &[]))
        })
    }

    /// Updates a tree for `dt` seconds.
    fn update(&mut self, which: Tree, dt: f64, clocks: &Clocks) -> Result<(), Failure> {
        self.lent(which, |lua, id| lua.update(id, dt as f32, clocks))
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
        let shown: Vec<bool> = self
            .foregrounds
            .iter()
            .map(|f| !f.done && f.until.is_some_and(|u| t <= u))
            .collect();
        let mut out: Vec<(&mut Actor, f32, bool)> = Vec::new();
        let mut backgrounds: Vec<Option<&mut Box<Actor>>> = self
            .backgrounds
            .iter_mut()
            .map(|b| b.tree.as_mut())
            .collect();
        for (b, alpha) in alphas {
            if let Some(tree) = backgrounds.get_mut(b).and_then(Option::take) {
                out.push((tree, alpha, false));
            }
        }
        for (fg, shown) in self.foregrounds.iter_mut().zip(shown) {
            if let Some(tree) = fg.tree.as_mut().filter(|_| shown) {
                out.push((tree, 1.0, true));
            }
        }
        out
    }
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
        .map_err(|f| reason(&f))?;
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
