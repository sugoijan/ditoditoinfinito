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
//! Running commands during play is the next step; this one loads.
//!
//! A Lua error while an element loads makes that element the built-in
//! skin's, with a note (StepMania shows a blank one); the memory limit,
//! the deadline, the fuel or a host limit (too many actors) fail the whole
//! skin's Lua, and the caller falls back to the native build
//! ([`crate::build::build`]), as the plan's guarantees ask.

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
use std::rc::Rc;

use ddi_lua::piccolo::{StashedFunction, StashedTable, Table, Value};
use ddi_lua::{Budget, Failure, Outcome, Settings, Vm};

use crate::build::{Built, Element, Images, Textures};
use crate::load::{Button, sprite_only};
use crate::skin::Skin;

use self::stage::Stage;

/// The prelude (ours, after StepMania's `_fallback` scripts).
const PRELUDE: &str = include_str!("prelude.lua");

/// Actors one skin may make, over all its elements.
pub const MAX_ACTORS: usize = 4096;

/// The play a skin loads for, as far as its Lua asks
/// (`GAMESTATE`, `PREFSMAN`).
#[derive(Clone, Debug, PartialEq)]
pub struct Env {
    /// `single`, `solo` or `double`.
    pub style: String,
    pub columns: u32,
    /// `GetPlayerOptionsString`.
    pub player_options: String,
    /// `Difficulty_Beginner` … `Difficulty_Edit`.
    pub difficulty: String,
    /// `GetGameplayLeadIn`: the song has not started yet.
    pub lead_in: bool,
    /// The song's tempo while loading (`GetCurBPS`, the timing data's
    /// conversions: a single tempo from `offset` seconds).
    pub bpm: f64,
    pub offset: f64,
    pub first_beat: f64,
    pub last_beat: f64,
    /// Note every global and actor method a script reads that is not
    /// there (for finding what skins miss; scripts test for names too, so
    /// it is noise in a report).
    pub trace_missing: bool,
}

impl Default for Env {
    fn default() -> Env {
        Env {
            style: "single".into(),
            columns: 4,
            player_options: String::new(),
            difficulty: "Difficulty_Medium".into(),
            lead_in: true,
            bpm: 120.0,
            offset: 0.0,
            first_beat: 0.0,
            last_beat: 0.0,
            trace_missing: false,
        }
    }
}

impl Env {
    /// `StyleType_*`, `StepsType_*` for the style.
    fn style_types(&self) -> (&'static str, &'static str) {
        match self.style.as_str() {
            "double" => ("StyleType_OnePlayerTwoSides", "StepsType_Dance_Double"),
            "solo" => ("StyleType_OnePlayerOneSide", "StepsType_Dance_Solo"),
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

/// What the host functions share with [`LuaSkin`].
pub(crate) struct Host {
    pub skin: Skin<'static>,
    pub env: Env,
    pub settings: Settings,
    pub warnings: Vec<String>,
    pub stage: Stage,
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
}

/// A skin with its Lua running.
pub struct LuaSkin {
    vm: Vm,
    host: Rc<RefCell<Host>>,
    registries: Registries,
    driver: Driver,
    budget: LoadBudget,
    fuel_used: u64,
    clock: Box<dyn FnMut() -> f64>,
    started: f64,
}

impl LuaSkin {
    /// Starts a VM for `skin`, runs the prelude and the `NoteSkin.lua`
    /// chain. `clock` is the host's, in seconds.
    pub fn new(
        skin: Skin<'static>,
        env: Env,
        settings: Settings,
        budget: LoadBudget,
        mut clock: Box<dyn FnMut() -> f64>,
    ) -> Result<LuaSkin, Failure> {
        let started = clock();
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
        let host = Rc::new(RefCell::new(Host {
            skin,
            env,
            settings,
            warnings: Vec::new(),
            stage: Stage::default(),
        }));
        let mut vm = Vm::new(settings);
        let (prelude, h, registries) = vm.lua().enter(|ctx| {
            let (h, registries) = host::table(ctx, &host);
            let prelude = ddi_lua::compile(ctx, &settings, "=prelude", PRELUDE.as_bytes());
            (prelude.map(|f| ctx.stash(f)), ctx.stash(h), registries)
        });
        let prelude = prelude?;
        let mut skin = LuaSkin {
            vm,
            host,
            registries,
            driver: Driver {
                // Replaced below.
                chain: prelude.clone(),
                template: prelude.clone(),
                init: prelude.clone(),
            },
            budget,
            fuel_used: 0,
            clock,
            started,
        };
        let results = skin.run(&prelude, move |ctx| vec![Value::Table(ctx.fetch(&h))])?;
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
            })
        });
        skin.driver = driver.ok_or_else(|| Failure::Error("the prelude did not load".into()))?;

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
        skin.run(&chain, move |ctx| {
            let t = Table::new(&ctx);
            for (i, f) in compiled.iter().enumerate() {
                let _ = t.set(ctx, i as i64 + 1, ctx.fetch(f));
            }
            vec![Value::Table(t)]
        })?;
        Ok(skin)
    }

    /// What the Lua warned about (script errors it survived, `Warn`).
    pub fn warnings(&self) -> Vec<String> {
        let h = self.host.borrow();
        let mut w = h.warnings.clone();
        w.extend(h.skin.warnings());
        w
    }

    /// Fuel spent so far.
    pub fn fuel_used(&self) -> u64 {
        self.fuel_used
    }

    /// Bytes the VM holds.
    pub fn memory(&self) -> usize {
        self.vm.memory()
    }

    /// Runs `f` to the end within what is left of the budget.
    fn run<A>(
        &mut self,
        f: &StashedFunction,
        args: A,
    ) -> Result<Vec<ddi_lua::piccolo::StashedValue>, Failure>
    where
        A: for<'gc> FnOnce(ddi_lua::piccolo::Context<'gc>) -> Vec<Value<'gc>>,
    {
        let mut task = self.vm.start(f, args);
        let fuel = self.budget.fuel.saturating_sub(self.fuel_used);
        let deadline = self.started + self.budget.seconds;
        let outcome = self
            .vm
            .run(&mut task, Budget { fuel, deadline }, &mut *self.clock);
        self.fuel_used = self.fuel_used.saturating_add(task.fuel_used());
        match outcome {
            Outcome::Done => Ok(task.results().to_vec()),
            Outcome::Paused => Err(Failure::Fuel),
            Outcome::Failed(f) => Err(f),
        }
    }

    /// `NOTESKIN:LoadActor(button, element)` as the engine calls it: the
    /// element's table from `Load()`, its actors, their `Init`.
    pub fn element(
        &mut self,
        button: Button,
        element: &str,
        textures: &mut Textures,
        images: &dyn Images,
    ) -> Result<Element, Failure> {
        let only_sprite = sprite_only(element);
        let template = self.driver.template.clone();
        let (b, e) = (button.name().to_string(), element.to_string());
        let results = self.run(&template, move |ctx| {
            vec![
                Value::String(ctx.intern(b.as_bytes())),
                Value::String(ctx.intern(e.as_bytes())),
                Value::Boolean(only_sprite),
            ]
        });
        // A Lua error loads nothing for the element (StepMania shows a
        // blank sprite and reports it); the built-in skin stands in.
        let results = match results {
            Err(Failure::Error(m)) => return Ok(built_in(m)),
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
        })?;
        let Some(converted) = converted else {
            return Ok(built_in(format!("{element}: Load did not return a table")));
        };
        let mut notes = converted.notes;
        let Some((root_index, root_id)) = converted.root else {
            return Ok(Element {
                built: Built::Blank,
                notes,
            });
        };
        let init = self.driver.init.clone();
        match self.run(&init, move |_| vec![Value::Integer(i64::from(root_id))]) {
            Err(Failure::Error(m)) => return Ok(built_in(m)),
            r => r?,
        };
        let tree = self
            .host
            .borrow()
            .stage
            .root(root_index)
            .cloned()
            .ok_or_else(|| Failure::Error("the element was lost".into()))?;
        if converted.unsupported {
            return Ok(Element {
                built: Built::BuiltIn,
                notes,
            });
        }
        if only_sprite && !matches!(tree.kind, crate::actor::Kind::Sprite(_)) {
            // `NoteSkinManager::LoadActor`: "must be a Sprite", a blank one.
            notes.push(format!("{element} must be a Sprite"));
            return Ok(Element {
                built: Built::Blank,
                notes,
            });
        }
        Ok(Element {
            built: Built::Actor(tree),
            notes,
        })
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
