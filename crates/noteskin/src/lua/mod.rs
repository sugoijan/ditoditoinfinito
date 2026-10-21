//! A skin's Lua, run as StepMania runs it when a skin loads
//! (`docs/plans/lua.md`, step 3), in a [`ddi_lua::Vm`] of its own: the
//! `NoteSkin.lua` chain (`NoteSkinManager::LoadNoteSkinDataRecursive`),
//! `Load()` per element with its thread variables (`PushActorTemplate`),
//! the actor tables it returns turned into native actors
//! (`XmlFileUtil::XNodeFromTable`, `ActorUtil::LoadFromNode`,
//! `Sprite::LoadFromNode`, `ActorFrame::LoadChildrenFromNode`) and their
//! `Init` played through Lua. The stand-ins' own Lua is StepMania's
//! (`crate::builtin`); `prelude.lua` defines the rest of what skins call.
//!
//! Every command of such a skin is a Lua function (metrics strings are
//! compiled as `ParseCommandList` compiles them) kept in the VM by actor
//! id; methods a command calls act on the native actors in [`Stage`].
//!
//! During a play ([`LuaSkin::begin_frame`]) the play holds the trees it
//! draws ([`LuaSkin::reclaim`], once every element is loaded, so elements
//! can reach each other while they load) and lends them back while Lua
//! runs ([`LuaSkin::lend`]): the
//! engine's events play commands ([`LuaSkin::play`]) and each tree's
//! update ([`LuaSkin::update`]) plays its queued commands and calls its
//! update functions through Lua, all of a frame within one budget.
//!
//! A Lua error while an element loads makes that element the built-in
//! skin's, with a note (StepMania shows a blank one); a command or update
//! function that fails is reported and the play goes on, as in StepMania.
//! The memory limit, the deadline, the fuel or a host limit (too many
//! actors) end the skin's Lua: at load the caller falls back to the native
//! build ([`crate::build::build`]); during a play every later call
//! returns the same failure and the play draws the built-in skin, as the
//! plan's guarantees ask.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

mod convert;
mod host;
mod stage;

use std::cell::RefCell;
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll, Waker};

use ddi_actor::{Actor, Clocks, Problems, TweenSteps};
use ddi_chart::timing::TimingMap;
use ddi_lua::lunita::{StashedFunction, StashedTable, Table, Value};
use ddi_lua::{Budget, Failure, Outcome, Settings, Vm};

use crate::build::{Built, Element, Images, Textures};
use crate::load::{Button, sprite_only};
use crate::skin::Skin;

pub use self::convert::song_text_style;
use self::stage::Stage;

/// The prelude (ours, after StepMania's `_fallback` scripts).
const PRELUDE: &str = include_str!("prelude.lua");

/// What a song's scripts read besides a skin's (`Skin::song`; run after
/// the prelude): StepMania's screen and the song's folder.
pub const SONG_PRELUDE: &str = include_str!("song.lua");

/// Actors one skin may make, over all its elements (and the copies of its
/// note elements per color, [`NOTE_COLORS`]).
pub const MAX_ACTORS: usize = 16384;

/// The play a skin loads for, as far as its Lua asks
/// (`GAMESTATE`, `PREFSMAN`).
#[derive(Clone, Debug, PartialEq)]
pub struct Env {
    /// The skin's game: `dance`, or `pump` for a pump skin. A dance skin
    /// is told `dance` on every layout: StepMania never loads one for
    /// another game, and skins that read the name switch to
    /// tables for that game's buttons (Dake), while the play passes dance
    /// buttons ([`dance_style`]).
    pub game: String,
    /// The style's name: `single`, `solo` or `double` (dance); `single`,
    /// `halfdouble` or `double` (pump).
    pub style: String,
    pub columns: u32,
    /// `GetPlayerOptionsString`.
    pub player_options: String,
    /// `Difficulty_Beginner` … `Difficulty_Edit`.
    pub difficulty: String,
    /// The steps' timing (`GetTimingData`, `GetSongBeat`, `GetCurBPS`).
    pub timing: TimingMap,
    pub first_beat: f64,
    pub last_beat: f64,
    /// Whether the field is upside down (`GAMESTATE:GetIsFieldReversed`,
    /// OutFox's).
    pub reversed: bool,
    /// Note every global and actor method a script reads that is not
    /// there (for finding what skins miss; scripts test for names too, so
    /// it is noise in a report).
    pub trace_missing: bool,
    /// StepMania's screen, width and height in its pixels, for a song's
    /// scripts (`SCREEN_WIDTH`, `SCREEN_HEIGHT`; [`SONG_PRELUDE`]).
    pub screen: [f64; 2],
    /// The values the player set for the skin's parameters (our
    /// extension, [`crate::params`]), as stored: what its declarations do
    /// not take is left out at load, and the rest defaulted.
    pub params: crate::params::Values,
}

impl Default for Env {
    fn default() -> Env {
        Env {
            game: "dance".into(),
            style: "single".into(),
            columns: 4,
            player_options: String::new(),
            difficulty: "Difficulty_Medium".into(),
            timing: TimingMap::constant(120.0, 0.0),
            first_beat: 0.0,
            last_beat: 0.0,
            reversed: false,
            trace_missing: false,
            screen: [640.0, 480.0],
            params: crate::params::Values::new(),
        }
    }
}

/// The dance style a dance skin is told a layout is: `solo`, `double` for
/// the layouts on two pads (dance double, pump half-double and double),
/// else `single`.
pub fn dance_style(layout: &ddi_chart::Layout) -> &'static str {
    match layout.id.as_str() {
        "dance-solo" => "solo",
        _ if layout.pads() > 1 => "double",
        _ => "single",
    }
}

/// What a skin's Lua sees of the chart it is loaded for, and of the
/// play (whether the field is reversed).
pub fn chart_env(
    song: &ddi_chart::Song,
    chart: &ddi_chart::Chart,
    layout: &ddi_chart::Layout,
    reversed: bool,
) -> Env {
    let difficulty = chart.difficulty.name();
    let starts = || chart.notes.iter().map(|n| n.tick.beat());
    // Holds and rolls last to their ends.
    let ends = chart.notes.iter().map(|n| n.end_tick().beat());
    Env {
        style: dance_style(layout).into(),
        columns: layout.lanes.len() as u32,
        difficulty: format!("Difficulty_{difficulty}"),
        timing: chart.timing(song).clone(),
        first_beat: starts().reduce(f64::min).unwrap_or(0.0),
        last_beat: ends.reduce(f64::max).unwrap_or(0.0),
        reversed,
        ..Env::default()
    }
}

impl Env {
    /// The env with its timing moved as [`ddi_chart::Song::shift_notes`]
    /// moves the song's: what a skin sees of a play shifted by the song
    /// offset, whose positions it is given (StepMania's offset edit
    /// changes the song's own timing). Beats stay where they are.
    pub fn shifted(mut self, seconds: f64) -> Env {
        self.timing.offset_seconds -= seconds;
        self
    }
}

/// The style a skin of `game` is told a layout is: a pump skin the pump
/// style of a pump layout (`single`, `halfdouble`, `double`), a dance
/// skin [`dance_style`].
pub fn style_for(game: &str, layout: &ddi_chart::Layout) -> &'static str {
    if game.eq_ignore_ascii_case("pump") {
        match layout.id.as_str() {
            "pump-halfdouble" => "halfdouble",
            "pump-double" => "double",
            _ => "single",
        }
    } else {
        dance_style(layout)
    }
}

impl Env {
    /// `StyleType_*`, `StepsType_*` for the style.
    fn style_types(&self) -> (&'static str, &'static str) {
        match (self.game.as_str(), self.style.as_str()) {
            ("pump", "double") => ("StyleType_OnePlayerTwoSides", "StepsType_Pump_Double"),
            ("pump", "halfdouble") => ("StyleType_OnePlayerTwoSides", "StepsType_Pump_Halfdouble"),
            ("pump", _) => ("StyleType_OnePlayerOneSide", "StepsType_Pump_Single"),
            (_, "double") => ("StyleType_OnePlayerTwoSides", "StepsType_Dance_Double"),
            (_, "solo") => ("StyleType_OnePlayerOneSide", "StepsType_Dance_Solo"),
            _ => ("StyleType_OnePlayerOneSide", "StepsType_Dance_Single"),
        }
    }
}

/// What a skin's Lua may spend loading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadBudget {
    /// Fuel for the whole skin.
    pub fuel: u64,
    /// Seconds on the host's clock for the whole skin.
    pub seconds: f64,
}

impl Default for LoadBudget {
    fn default() -> LoadBudget {
        // Real skins load with 0.25–0.45 million (StepMania's, ITGmania's,
        // 345 community skins); an endless loop spends this in about 2 s.
        LoadBudget {
            fuel: 20_000_000,
            seconds: 5.0,
        }
    }
}

/// What a skin's Lua may spend in one frame of a play, over all its
/// calls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameBudget {
    pub fuel: u64,
    /// Seconds on the host's clock: the backstop for slow machines.
    pub seconds: f64,
}

impl Default for FrameBudget {
    fn default() -> FrameBudget {
        // Measured on the local corpus (`docs/plans/lua.md`, step 4).
        FrameBudget {
            fuel: FRAME_FUEL,
            seconds: 0.05,
        }
    }
}

/// `Var "Color"` for each note type, 4ths to 192nds (`NoteTypeToString`),
/// in [`crate::notes::note_type`]'s order.
pub const NOTE_COLORS: [&str; 9] = [
    "4th", "8th", "12th", "16th", "24th", "32nd", "48th", "64th", "192nd",
];

/// Fuel for one frame of a play.
pub const FRAME_FUEL: u64 = 200_000;

/// Fuel a load runs between two turns of the host ([`Pause`]).
pub const LOAD_SLICE: u64 = 100_000;

/// Fuel every call into the VM costs on top of its instructions (starting
/// it, the clock): cheap calls many times a frame are not free.
const RUN_FEE: u64 = 200;

/// The song's position in a frame (`GAMESTATE:GetSongPosition()`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position {
    /// Music seconds (negative before the music starts).
    pub seconds: f64,
    pub beat: f64,
}

/// A parameter of a message's command (`Message::SetParam`).
#[derive(Clone, Debug, PartialEq)]
pub enum Param {
    Str(String),
    Int(i64),
    Bool(bool),
    /// Finite numeric presentation data, e.g. a hit's local coordinates.
    Float(f64),
}

/// Gives the host its turn between slices of a load (a loading screen
/// keeps drawing): resolves once the host has had it.
pub type Pause = dyn FnMut() -> Pin<Box<dyn Future<Output = ()>>>;

/// What the host functions share with [`LuaSkin`].
pub(crate) struct Host {
    pub skin: Skin<'static>,
    pub env: Env,
    pub settings: Settings,
    pub warnings: Vec<String>,
    pub stage: Stage,
    /// Frames with an update function.
    pub updating: HashSet<u32>,
    /// A song's scripts' `Sound` actors: their file (relative to the
    /// song's folder) by actor id, and the files played since last asked
    /// ([`LuaSkin::take_sounds`]).
    pub sounds: std::collections::HashMap<u32, String>,
    pub played: Vec<String>,
    /// The frame's song position; `None` while loading.
    pub position: Option<Position>,
    /// The skin's parameters (`NOTESKIN:GetParameter`), their values in
    /// [`Env::params`].
    pub declared: crate::params::Declared,
}

impl Host {
    pub fn warn(&mut self, w: String) {
        const MAX: usize = 64;
        let w: String = w.chars().take(400).collect();
        if self.warnings.len() < MAX && !self.warnings.contains(&w) {
            self.warnings.push(w);
        }
    }
}

/// The VM's tables the converter fills.
struct Registries {
    /// Actor id → its userdata.
    actors: StashedTable,
    /// Actor id → `{ Name = function }`.
    commands: StashedTable,
    /// Actor id → `{ child ids }`.
    children: StashedTable,
    /// The userdata metatables: Actor, Sprite, ActorFrame, Model.
    metatables: [StashedTable; 4],
}

/// The prelude's entry points.
struct Driver {
    chain: StashedFunction,
    template: StashedFunction,
    init: StashedFunction,
    play: StashedFunction,
    update: StashedFunction,
    /// `LoadActor` of a path: a song's background or foreground.
    actor: StashedFunction,
}

/// What is left to spend: of the load, or of the current frame.
#[derive(Clone, Copy, Debug)]
struct Allowance {
    fuel: u64,
    /// Seconds spent in Lua count; the host's turns between slices do not.
    seconds: f64,
}

/// A skin with its Lua running.
pub struct LuaSkin {
    vm: Vm,
    host: Rc<RefCell<Host>>,
    registries: Registries,
    driver: Driver,
    left: Allowance,
    fuel_used: u64,
    frame_fuel: u64,
    clock: Box<dyn FnMut() -> f64>,
    /// Why the Lua stopped: every later call fails the same way.
    failed: Option<Failure>,
}

impl LuaSkin {
    /// Starts a VM for `skin`, runs the prelude and the `NoteSkin.lua`
    /// chain. `clock` is the host's, in seconds.
    pub fn new(
        skin: Skin<'static>,
        env: Env,
        settings: Settings,
        budget: LoadBudget,
        clock: Box<dyn FnMut() -> f64>,
    ) -> Result<LuaSkin, Failure> {
        now(LuaSkin::new_sliced(
            skin, env, settings, budget, clock, None,
        ))
    }

    /// [`LuaSkin::new`], giving the host a turn every [`LOAD_SLICE`] fuel.
    pub async fn new_sliced(
        skin: Skin<'static>,
        env: Env,
        settings: Settings,
        budget: LoadBudget,
        clock: Box<dyn FnMut() -> f64>,
        mut pause: Option<&mut Pause>,
    ) -> Result<LuaSkin, Failure> {
        let scripts: Vec<(String, String)> = skin
            .scripts()
            .into_iter()
            .filter_map(|dir| {
                let path = format!("{}NoteSkin.lua", skin.dir_path(dir));
                let file = skin.locate(&path)?;
                let text = match file.0 {
                    crate::skin::Dir::BuiltIn(b) => b.lua_source("NoteSkin.lua")?.to_string(),
                    _ => skin.folder(dir)?.text("NoteSkin.lua")?.to_string(),
                };
                Some((path, text))
            })
            .collect();
        // The parameters' values as the skin's declarations take them.
        let declared = crate::params::Declared::from_ini(&skin.metrics);
        let mut env = env;
        env.params = declared.resolve(&env.params);
        let host = Rc::new(RefCell::new(Host {
            skin,
            env,
            settings,
            warnings: Vec::new(),
            stage: Stage::default(),
            updating: HashSet::new(),
            sounds: std::collections::HashMap::new(),
            played: Vec::new(),
            position: None,
            declared,
        }));
        // What its declarations left out, said as the Lua's warnings are.
        {
            let mut h = host.borrow_mut();
            for w in h.declared.warnings.clone() {
                h.warn(format!("metrics.ini: {w}"));
            }
        }
        let mut vm = Vm::new(settings);
        let (prelude, h, registries) = vm.lua().enter(|ctx| {
            let (h, registries) = host::table(ctx, &host);
            let prelude = ddi_lua::compile(ctx, &settings, "=prelude", PRELUDE.as_bytes());
            (prelude.map(|f| ctx.stash(f)), ctx.stash(h), registries)
        });
        let prelude = prelude?;
        let h_song = h.clone();
        let mut skin = LuaSkin {
            vm,
            host,
            registries,
            driver: Driver {
                // Replaced below.
                chain: prelude.clone(),
                template: prelude.clone(),
                init: prelude.clone(),
                play: prelude.clone(),
                update: prelude.clone(),
                actor: prelude.clone(),
            },
            left: Allowance {
                fuel: budget.fuel,
                seconds: budget.seconds,
            },
            fuel_used: 0,
            frame_fuel: 0,
            clock,
            failed: None,
        };
        let results = skin
            .run(
                &prelude,
                move |ctx| vec![Value::Table(ctx.fetch(&h))],
                pause.as_deref_mut(),
            )
            .await?;
        let driver = skin.vm.lua().enter(|ctx| {
            let Some(Value::Table(t)) = results.first().map(|v| ctx.fetch(v)) else {
                return None;
            };
            let f = |name: &'static str| match t.get_value(ctx, name) {
                Value::Function(f) => Some(ctx.stash(f)),
                _ => None,
            };
            Some(Driver {
                chain: f("chain")?,
                template: f("template")?,
                init: f("init")?,
                play: f("play")?,
                update: f("update")?,
                actor: f("actor")?,
            })
        });
        skin.driver = driver.ok_or_else(|| Failure::Error("the prelude did not load".into()))?;
        if skin.host.borrow().skin.is_song() {
            let song = skin.vm.lua().enter(|ctx| {
                ddi_lua::compile(ctx, &settings, "=song", SONG_PRELUDE.as_bytes())
                    .map(|f| ctx.stash(f))
            })?;
            skin.run(
                &song,
                move |ctx| vec![Value::Table(ctx.fetch(&h_song))],
                pause.as_deref_mut(),
            )
            .await?;
        }

        // The chain: each script compiled (one that does not compile is
        // skipped, as `RunScript` failing leaves the previous table), then
        // run in order.
        let mut compiled = Vec::new();
        for (path, text) in &scripts {
            let r = skin.vm.lua().enter(|ctx| {
                ddi_lua::compile(ctx, &settings, &format!("@{path}"), text.as_bytes())
                    .map(|f| ctx.stash(f))
            });
            match r {
                Ok(f) => compiled.push(f),
                Err(Failure::Limit(l)) => return Err(Failure::Limit(l)),
                Err(e) => skin.host.borrow_mut().warn(format!("{e}")),
            }
        }
        let chain = skin.driver.chain.clone();
        skin.run(
            &chain,
            move |ctx| {
                let t = Table::new(&ctx);
                for (i, f) in compiled.iter().enumerate() {
                    let _ = t.set(ctx, i as i64 + 1, ctx.fetch(f));
                }
                vec![Value::Table(t)]
            },
            pause,
        )
        .await?;
        Ok(skin)
    }

    /// What the Lua warned about (script errors it survived, `Warn`).
    pub fn warnings(&self) -> Vec<String> {
        let h = self.host.borrow();
        let mut w = h.warnings.clone();
        w.extend(h.skin.warnings());
        w
    }

    /// The skin the Lua runs for.
    pub fn skin(&self) -> std::cell::Ref<'_, Skin<'static>> {
        std::cell::Ref::map(self.host.borrow(), |h| &h.skin)
    }

    /// Warnings after the first `seen` ([`LuaSkin::warnings`] keeps at
    /// most 64).
    pub fn warnings_since(&self, seen: usize) -> Vec<String> {
        self.host
            .borrow()
            .warnings
            .get(seen..)
            .map(<[String]>::to_vec)
            .unwrap_or_default()
    }

    /// Fuel spent so far.
    pub fn fuel_used(&self) -> u64 {
        self.fuel_used
    }

    /// Bytes the VM holds.
    pub fn memory(&self) -> usize {
        self.vm.memory()
    }

    /// Fuel spent in the current frame (since [`LuaSkin::begin_frame`]).
    pub fn frame_fuel(&self) -> u64 {
        self.frame_fuel
    }

    /// The values of the skin's parameters its Lua reads
    /// (`NOTESKIN:GetParameter`), as its declarations took them.
    pub fn parameters(&self) -> crate::params::Values {
        self.host.borrow().env.params.clone()
    }

    /// Why the skin's Lua stopped, if it did.
    pub fn failure(&self) -> Option<&Failure> {
        self.failed.as_ref()
    }

    /// Runs `f` to the end within what is left of the allowance, in
    /// slices of [`LOAD_SLICE`] fuel with the host's turn between them
    /// when `pause` is given. Limits, fuel and the deadline end the skin's
    /// Lua; an error is returned as it is.
    async fn run<A>(
        &mut self,
        f: &StashedFunction,
        args: A,
        mut pause: Option<&mut Pause>,
    ) -> Result<Vec<ddi_lua::lunita::StashedValue>, Failure>
    where
        A: for<'gc> FnOnce(ddi_lua::lunita::Context<'gc>) -> Vec<Value<'gc>>,
    {
        if let Some(f) = &self.failed {
            return Err(f.clone());
        }
        if self.left.fuel <= RUN_FEE {
            self.failed = Some(Failure::Fuel);
            return Err(Failure::Fuel);
        }
        self.left.fuel -= RUN_FEE;
        self.fuel_used = self.fuel_used.saturating_add(RUN_FEE);
        self.frame_fuel = self.frame_fuel.saturating_add(RUN_FEE);
        let mut task = self.vm.start(f, args);
        let outcome = loop {
            let slice = match pause {
                Some(_) => self.left.fuel.min(LOAD_SLICE),
                None => self.left.fuel,
            };
            let started = (self.clock)();
            let deadline = started + self.left.seconds.max(0.0);
            let before = task.fuel_used();
            let outcome = self.vm.run(
                &mut task,
                Budget {
                    fuel: slice,
                    deadline,
                },
                &mut *self.clock,
            );
            let used = task.fuel_used().saturating_sub(before);
            let elapsed = (self.clock)() - started;
            self.left.fuel = self.left.fuel.saturating_sub(used);
            self.left.seconds -= elapsed.max(0.0);
            self.fuel_used = self.fuel_used.saturating_add(used);
            self.frame_fuel = self.frame_fuel.saturating_add(used);
            match (outcome, pause.as_deref_mut()) {
                (Outcome::Paused, Some(p)) if self.left.fuel > 0 => p().await,
                (outcome, _) => break outcome,
            }
        };
        let r = match outcome {
            Outcome::Done => return Ok(task.results().to_vec()),
            Outcome::Paused => Failure::Fuel,
            Outcome::Failed(f) => f,
        };
        if !matches!(r, Failure::Error(_) | Failure::Compile(_)) {
            self.failed = Some(r.clone());
        }
        Err(r)
    }

    /// `NOTESKIN:LoadActor(button, element)` as the engine calls it: the
    /// element's table from `Load()`, its actors, their `Init`. The tree
    /// is the caller's to draw; a play lends it back with
    /// [`LuaSkin::swap`].
    pub fn element(
        &mut self,
        button: Button,
        element: &str,
        textures: &mut Textures,
        images: &dyn Images,
    ) -> Result<Element, Failure> {
        now(self.element_sliced(button, element, textures, images, None))
    }

    /// [`LuaSkin::element`], giving the host a turn every [`LOAD_SLICE`]
    /// fuel.
    pub async fn element_sliced(
        &mut self,
        button: Button,
        element: &str,
        textures: &mut Textures,
        images: &dyn Images,
        pause: Option<&mut Pause>,
    ) -> Result<Element, Failure> {
        Ok(self
            .element_colored_sliced(button, element, NOTE_COLORS[0], textures, images, pause)
            .await?
            .0)
    }

    /// [`LuaSkin::element_sliced`] with `Var "Color"` set to `color` (one
    /// of [`NOTE_COLORS`]), and whether its Lua read it: then the element
    /// looks different per color and is loaded once for each, as ITGmania
    /// and OutFox load notes.
    pub async fn element_colored_sliced(
        &mut self,
        button: Button,
        element: &str,
        color: &str,
        textures: &mut Textures,
        images: &dyn Images,
        mut pause: Option<&mut Pause>,
    ) -> Result<(Element, bool), Failure> {
        let only_sprite = sprite_only(element);
        let template = self.driver.template.clone();
        let (b, e, c) = (
            button.name().to_string(),
            element.to_string(),
            color.to_string(),
        );
        let results = self
            .run(
                &template,
                move |ctx| {
                    vec![
                        Value::String(ctx.intern(b.as_bytes())),
                        Value::String(ctx.intern(e.as_bytes())),
                        Value::Boolean(only_sprite),
                        Value::String(ctx.intern(c.as_bytes())),
                    ]
                },
                pause.as_deref_mut(),
            )
            .await;
        // A Lua error loads nothing for the element (StepMania shows a
        // blank sprite and reports it); the built-in skin stands in.
        let results = match results {
            Err(Failure::Error(m)) if self.failed.is_none() => return Ok((built_in(m), false)),
            r => r?,
        };
        let colored = self.vm.lua().enter(|ctx| {
            matches!(
                results.get(1).map(|v| ctx.fetch(v)),
                Some(Value::Boolean(true))
            )
        });
        let host = self.host.clone();
        let registries = &self.registries;
        let converted = self.vm.lua().enter(|ctx| {
            let Some(Value::Table(t)) = results.first().map(|v| ctx.fetch(v)) else {
                return Ok(None);
            };
            let mut host = host.borrow_mut();
            convert::element(ctx, &mut host, registries, textures, images, t).map(Some)
        });
        let converted = match converted {
            Err(f) => {
                // A host limit ends the skin's Lua like the VM's own.
                self.failed = Some(f.clone());
                return Err(f);
            }
            Ok(c) => c,
        };
        let Some(converted) = converted else {
            return Ok((
                built_in(format!("{element}: Load did not return a table")),
                colored,
            ));
        };
        let mut notes = converted.notes;
        let Some((_, root_id)) = converted.root else {
            return Ok((
                Element {
                    built: Built::Blank,
                    notes,
                },
                colored,
            ));
        };
        let init = self.driver.init.clone();
        match self
            .run(
                &init,
                move |_| vec![Value::Integer(i64::from(root_id))],
                pause,
            )
            .await
        {
            Err(Failure::Error(m)) if self.failed.is_none() => return Ok((built_in(m), colored)),
            r => r?,
        };
        if converted.unsupported {
            return Ok((
                Element {
                    built: Built::BuiltIn,
                    notes,
                },
                colored,
            ));
        }
        // The stage keeps the tree until the play reclaims it.
        let tree = self
            .host
            .borrow()
            .stage
            .get(root_id)
            .cloned()
            .ok_or_else(|| Failure::Error("the element was lost".into()))?;
        if only_sprite && !matches!(tree.kind, ddi_actor::Kind::Sprite(_)) {
            // `NoteSkinManager::LoadActor`: "must be a Sprite", a blank one.
            notes.push(format!("{element} must be a Sprite"));
            return Ok((
                Element {
                    built: Built::Blank,
                    notes,
                },
                colored,
            ));
        }
        Ok((
            Element {
                built: Built::Actor(tree),
                notes,
            },
            colored,
        ))
    }

    /// A song's background or foreground ([`Skin::song`]): `LoadActor` of
    /// `path` (as Lua names it, `/Songs/<pack>/<song>/<folder>`), in the
    /// `UpperLeft` effect's frame tinted by `color1` for a background,
    /// itself for a foreground; converted and initialized as an element
    /// is. What is not drawn here (text, render targets) stays in the tree
    /// as actors that draw nothing, with a note: the rest still draws. A
    /// Lua error is [`Built::Blank`] with its message.
    pub fn actor_file(
        &mut self,
        path: &str,
        color1: Option<&str>,
        textures: &mut Textures,
        images: &dyn Images,
    ) -> Result<Element, Failure> {
        now(self.actor_file_sliced(path, color1, textures, images, None))
    }

    /// [`LuaSkin::actor_file`], giving the host a turn every
    /// [`LOAD_SLICE`] fuel.
    pub async fn actor_file_sliced(
        &mut self,
        path: &str,
        color1: Option<&str>,
        textures: &mut Textures,
        images: &dyn Images,
        mut pause: Option<&mut Pause>,
    ) -> Result<Element, Failure> {
        let actor = self.driver.actor.clone();
        let (p, c) = (path.to_string(), color1.map(str::to_string));
        let results = self
            .run(
                &actor,
                move |ctx| {
                    let mut args = vec![Value::String(ctx.intern(p.as_bytes()))];
                    args.extend(c.map(|c| Value::String(ctx.intern(c.as_bytes()))));
                    args
                },
                pause.as_deref_mut(),
            )
            .await;
        let blank = |m: String| Element {
            built: Built::Blank,
            notes: vec![m],
        };
        let results = match results {
            Err(Failure::Error(m)) if self.failed.is_none() => return Ok(blank(m)),
            r => r?,
        };
        let host = self.host.clone();
        let registries = &self.registries;
        let converted = self.vm.lua().enter(|ctx| {
            let Some(Value::Table(t)) = results.first().map(|v| ctx.fetch(v)) else {
                return Ok(None);
            };
            let mut host = host.borrow_mut();
            convert::element(ctx, &mut host, registries, textures, images, t).map(Some)
        });
        let converted = match converted {
            Err(f) => {
                self.failed = Some(f.clone());
                return Err(f);
            }
            Ok(c) => c,
        };
        let Some(converted) = converted else {
            return Ok(blank(format!("{path}: did not return a table")));
        };
        let notes = converted.notes;
        let Some((_, root_id)) = converted.root else {
            return Ok(Element {
                built: Built::Blank,
                notes,
            });
        };
        let init = self.driver.init.clone();
        match self
            .run(
                &init,
                move |_| vec![Value::Integer(i64::from(root_id))],
                pause,
            )
            .await
        {
            Err(Failure::Error(m)) if self.failed.is_none() => return Ok(blank(m)),
            r => r?,
        };
        let tree = self
            .host
            .borrow()
            .stage
            .get(root_id)
            .cloned()
            .ok_or_else(|| Failure::Error("the actor was lost".into()))?;
        Ok(Element {
            built: Built::Actor(tree),
            notes,
        })
    }

    /// The files a song's scripts' `Sound` actors may play (relative to
    /// the song's folder), each once.
    pub fn sound_files(&self) -> Vec<String> {
        let mut files: Vec<String> = self.host.borrow().sounds.values().cloned().collect();
        files.sort();
        files.dedup();
        files
    }

    /// The sounds the scripts played (`play` on a `Sound` actor) since
    /// last asked, in order.
    pub fn take_sounds(&mut self) -> Vec<String> {
        std::mem::take(&mut self.host.borrow_mut().played)
    }

    // The play.

    /// Starts a frame of a play: the song's position for the scripts, and
    /// a new allowance.
    pub fn begin_frame(&mut self, position: Position, budget: FrameBudget) {
        self.host.borrow_mut().position = Some(position);
        self.left = Allowance {
            fuel: budget.fuel,
            seconds: budget.seconds,
        };
        self.frame_fuel = 0;
    }

    /// Takes the stage's tree of an element [`LuaSkin::element`] built
    /// for the play, in exchange for `tree` (its copy): once every element
    /// is loaded, then after each [`LuaSkin::lend`]. False when `tree` is
    /// no element of this skin or the play holds it already.
    pub fn reclaim(&mut self, tree: &mut Box<Actor>) -> bool {
        self.host.borrow_mut().stage.reclaim(tree)
    }

    /// Lends a tree the play holds back to the skin's Lua while it runs.
    /// False when `tree` is no element of this skin or is lent already.
    pub fn lend(&mut self, tree: &mut Box<Actor>) -> bool {
        self.host.borrow_mut().stage.lend(tree)
    }

    /// `PlayCommand(name)` on actor `id` and its children (a message's
    /// command, `HandleMessage`, with `params`; an empty table without,
    /// as StepMania's `Message` always has one). The tree must be lent.
    /// A command that fails is reported ([`LuaSkin::warnings`]).
    pub fn play(&mut self, id: u32, name: &str, params: &[(&str, Param)]) -> Result<(), Failure> {
        let play = self.driver.play.clone();
        let r = now(self.run(
            &play,
            |ctx| {
                let t = Table::new(&ctx);
                for (k, v) in params {
                    let v = match v {
                        Param::Str(s) => Value::String(ctx.intern(s.as_bytes())),
                        Param::Int(i) => Value::Integer(*i),
                        Param::Bool(b) => Value::Boolean(*b),
                        Param::Float(n) => Value::Number(if n.is_finite() { *n } else { 0.0 }),
                    };
                    let _ = t.set(ctx, ctx.intern(k.as_bytes()), v);
                }
                let p = Value::Table(t);
                vec![
                    Value::Integer(i64::from(id)),
                    Value::String(ctx.intern(name.as_bytes())),
                    p,
                ]
            },
            None,
        ));
        self.soft(r)
    }

    /// `Actor::Update` of actor `id` and its children for `dt` seconds:
    /// the native update, with the commands its tweens queued and the
    /// frames' update functions played through Lua. The tree must be lent.
    pub fn update(&mut self, id: u32, dt: f32, clocks: &Clocks) -> Result<(), Failure> {
        self.update_depth(id, dt, clocks, 0)
    }

    fn update_depth(
        &mut self,
        id: u32,
        dt: f32,
        clocks: &Clocks,
        depth: u32,
    ) -> Result<(), Failure> {
        // As deep as an element can be.
        if depth > 40 {
            return Ok(());
        }
        let mut problems = Problems::default();
        let Some(dt) = self
            .host
            .borrow_mut()
            .stage
            .get_mut(id)
            .and_then(|a| a.update_begin(dt, clocks))
        else {
            return Ok(());
        };
        let mut steps = TweenSteps::new(dt);
        loop {
            let command = self
                .host
                .borrow_mut()
                .stage
                .get_mut(id)
                .and_then(|a| a.tween_step(&mut steps, &mut problems));
            let Some(command) = command else { break };
            self.play(id, &command, &[])?;
        }
        let children = {
            let mut host = self.host.borrow_mut();
            if let Some(a) = host.stage.get_mut(id) {
                a.update_end(dt);
            }
            for p in problems.messages {
                host.warn(p);
            }
            host.stage.children(id)
        };
        for c in children {
            self.update_depth(c, dt, clocks, depth + 1)?;
        }
        if self.host.borrow().updating.contains(&id) {
            let update = self.driver.update.clone();
            let r = now(self.run(
                &update,
                move |_| vec![Value::Integer(i64::from(id)), Value::Number(f64::from(dt))],
                None,
            ));
            self.soft(r)?;
        }
        Ok(())
    }

    /// An error the driver let through is reported; anything else ends
    /// the skin's Lua, and so does any failure once it has ended.
    fn soft<T>(&mut self, r: Result<T, Failure>) -> Result<(), Failure> {
        match r {
            Ok(_) => Ok(()),
            Err(Failure::Error(m) | Failure::Compile(m)) if self.failed.is_none() => {
                self.host.borrow_mut().warn(m);
                Ok(())
            }
            Err(f) => Err(f),
        }
    }
}

/// Runs a future that never waits (no [`Pause`]) to its end.
pub fn now<T>(f: impl Future<Output = Result<T, Failure>>) -> Result<T, Failure> {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut TaskContext::from_waker(Waker::noop())) {
        Poll::Ready(r) => r,
        Poll::Pending => Err(Failure::Error("the load waited without a pause".into())),
    }
}

fn built_in(message: String) -> Element {
    Element {
        built: Built::BuiltIn,
        notes: vec![message],
    }
}

/// Builds every element a skin's Lua defines, or says why its Lua could
/// not run (the caller then builds natively).
pub fn load(
    skin: Skin<'static>,
    env: Env,
    budget: LoadBudget,
    clock: Box<dyn FnMut() -> f64>,
) -> Result<LuaSkin, Failure> {
    LuaSkin::new(skin, env, Settings::default(), budget, clock)
}

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
mod tests;
