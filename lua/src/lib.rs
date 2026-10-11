//! StepMania's Lua, run so that a player never has to trust a script
//! (`docs/plans/lua.md`): a hardened fork of piccolo (memory and string
//! limits checked before allocating, fuel charged in proportion to work,
//! no panics, Lua 5.1's behaviour), with what StepMania adds on top: the
//! `cmd` keyword ([`cmd`]) and Lua 5.1's globals ([`sandbox`]).
//!
//! A [`Vm`] is one script's world: its own heap, globals and limits, shared
//! with nothing; dropping it frees everything it made. Code runs as a
//! [`Task`] stepped by [`Vm::run`] under a [`Budget`]: fuel for this call
//! and a deadline on a clock the host gives, checked between steps of
//! [`Settings::step_fuel`]. Fuel decides in normal runs, so a script
//! behaves the same on every platform; the deadline is the backstop.
//!
//! Platform-free: no clock, no files. Host functions are piccolo callbacks
//! (re-exported as [`piccolo`]) and charge fuel for the work they do with
//! `Execution::fuel` or `Context::charge_fuel`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing
)]

pub mod cmd;
pub mod sandbox;

use std::fmt;

pub use piccolo;
use piccolo::compiler::{CompileBudget, CompileLimits};
use piccolo::fuel::cost;
use piccolo::{
    Closure, Compat, Context, Error, Executor, Fuel, FunctionPrototype, Lua, StashedExecutor,
    StashedFunction, StashedValue, Value, Variadic,
};
pub use piccolo::{LimitError, Limits};

/// What a [`Vm`] may use. The defaults are the plan's proposed bounds for
/// a noteskin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Bytes the VM may hold (its heap, stacks and buffers).
    pub memory: usize,
    /// Longest string, in bytes.
    pub string_len: usize,
    /// Fuel per `Executor::step`, between which the deadline is checked.
    /// Any step takes at most about 0.25 µs per fuel on a desktop CPU (the
    /// fork's cost model), so 1,000 keeps a step well under a millisecond.
    pub step_fuel: i32,
    /// Units of compiler work one chunk may take (the fork's
    /// `CompileBudget::work`, about 50 ns each).
    pub compile_work: u64,
    /// Longest source accepted, in bytes.
    pub source_len: usize,
    /// `math.random`'s seed (the VM has no entropy of its own).
    pub random_seed: u64,
    /// The seed strings and table keys are hashed with. A script that knows
    /// it can build keys that collide and make table operations slow, so a
    /// host running untrusted scripts sets one they cannot know (from the
    /// platform's randomness, per VM).
    pub hash_seed: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            memory: 32 << 20,
            string_len: 1 << 20,
            step_fuel: 1_000,
            compile_work: 2_000_000,
            source_len: 1 << 20,
            random_seed: 0,
            hash_seed: 0,
        }
    }
}

/// What one call to [`Vm::run`] may spend.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    /// Fuel for this call. When it runs out the task pauses: a load goes
    /// on next frame; during play the host treats it as a failure.
    pub fuel: u64,
    /// The host clock's time (as `now` returns it) past which the call
    /// fails, checked after every step.
    pub deadline: f64,
}

/// Why a script failed. Any failure turns that script's Lua off; the
/// message is for the import report and the logs.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The source was rejected before running: too long, a malformed
    /// `cmd(...)`, a syntax error, too complex to compile.
    Compile(String),
    /// A Lua error nothing caught, with its message.
    Error(String),
    /// The memory or string length limit (which `pcall` cannot catch).
    Limit(LimitError),
    /// The deadline passed.
    Deadline,
    /// The host's fuel for the work ran out (a [`Outcome::Paused`] the host
    /// does not resume).
    Fuel,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Compile(m) | Failure::Error(m) => f.write_str(m),
            Failure::Limit(e) => write!(f, "{e}"),
            Failure::Deadline => f.write_str("took too long"),
            Failure::Fuel => f.write_str("used too much fuel"),
        }
    }
}

/// Where a [`Task`] is after [`Vm::run`].
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Returned; [`Task::results`] has its values.
    Done,
    /// The budget's fuel ran out first; run it again to go on.
    Paused,
    Failed(Failure),
}

/// A function call in progress.
pub struct Task {
    executor: StashedExecutor,
    fuel_used: u64,
    steps: u64,
    results: Vec<StashedValue>,
    finished: Option<Outcome>,
}

impl Task {
    /// Fuel spent so far, over every [`Vm::run`].
    pub fn fuel_used(&self) -> u64 {
        self.fuel_used
    }

    /// Executor steps taken so far.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// What the function returned, once [`Outcome::Done`].
    pub fn results(&self) -> &[StashedValue] {
        &self.results
    }
}

/// One script's Lua state.
pub struct Vm {
    lua: Lua,
    settings: Settings,
}

impl Vm {
    pub fn new(settings: Settings) -> Vm {
        let mut lua = Lua::empty();
        // Before anything makes a string (it would be hashed with the
        // default keys); on a new instance it always takes.
        let _ = lua.set_hash_seed(settings.hash_seed);
        lua.set_compat(Compat::Lua51);
        lua.set_random_seed(settings.random_seed);
        lua.enter(sandbox::load);
        lua.set_limits(Limits {
            memory: Some(settings.memory),
            string_len: Some(settings.string_len),
        });
        Vm { lua, settings }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The piccolo state, for host functions and reading results.
    pub fn lua(&mut self) -> &mut Lua {
        &mut self.lua
    }

    /// Bytes the VM holds now, as its memory limit counts them.
    pub fn memory(&self) -> usize {
        self.lua.total_memory()
    }

    /// Compiles a chunk with the globals as its environment, after the
    /// `cmd` rewrite. `name` is the chunk name errors show (StepMania
    /// uses `@` and the file's path).
    pub fn load(&mut self, name: &str, source: &[u8]) -> Result<StashedFunction, Failure> {
        let settings = self.settings;
        self.lua.enter(|ctx| {
            let f = compile(ctx, &settings, name, source)?;
            Ok(ctx.stash(f))
        })
    }

    /// Starts calling `function` with `args` (made inside the arena).
    pub fn start<A>(&mut self, function: &StashedFunction, args: A) -> Task
    where
        A: for<'gc> FnOnce(Context<'gc>) -> Vec<Value<'gc>>,
    {
        let executor = self.lua.enter(|ctx| {
            let f = ctx.fetch(function);
            let args = args(ctx);
            ctx.stash(Executor::start(ctx, f, Variadic(args)))
        });
        Task {
            executor,
            fuel_used: 0,
            steps: 0,
            results: Vec::new(),
            finished: None,
        }
    }

    /// Steps `task` until it returns, fails, spends `budget.fuel` or
    /// passes `budget.deadline` on the host's clock `now`. A finished task
    /// keeps its outcome.
    pub fn run(
        &mut self,
        task: &mut Task,
        budget: Budget,
        now: &mut dyn FnMut() -> f64,
    ) -> Outcome {
        if let Some(outcome) = &task.finished {
            return outcome.clone();
        }
        let step_fuel = self.settings.step_fuel;
        let mut spent: u64 = 0;
        loop {
            if spent >= budget.fuel {
                return Outcome::Paused;
            }
            let mut fuel = Fuel::with(step_fuel);
            let finished = self
                .lua
                .enter(|ctx| ctx.fetch(&task.executor).step(ctx, &mut fuel));
            // Fuel may go negative when an operation charged more than the
            // step had left.
            let used = i64::from(step_fuel)
                .saturating_sub(i64::from(fuel.remaining()))
                .max(0) as u64;
            spent = spent.saturating_add(used);
            task.fuel_used = task.fuel_used.saturating_add(used);
            task.steps = task.steps.saturating_add(1);
            let outcome = match finished {
                Ok(false) => None,
                Ok(true) => Some(self.finish(task)),
                Err(e) => Some(Outcome::Failed(Failure::Error(e.to_string()))),
            };
            if let Some(outcome) = outcome {
                task.finished = Some(outcome.clone());
                return outcome;
            }
            if now() >= budget.deadline {
                let outcome = Outcome::Failed(Failure::Deadline);
                task.finished = Some(outcome.clone());
                return outcome;
            }
        }
    }

    fn finish(&mut self, task: &mut Task) -> Outcome {
        self.lua.enter(|ctx| {
            match ctx
                .fetch(&task.executor)
                .take_result::<Variadic<Vec<Value>>>(ctx)
            {
                Ok(Ok(values)) => {
                    task.results = values.0.into_iter().map(|v| ctx.stash(v)).collect();
                    Outcome::Done
                }
                Ok(Err(e)) => Outcome::Failed(match e.limit_error() {
                    Some(l) => Failure::Limit(l),
                    None => Failure::Error(error_message(&e)),
                }),
                Err(e) => Outcome::Failed(Failure::Error(e.to_string())),
            }
        })
    }
}

/// Compiles a chunk inside the arena (for host functions such as
/// `loadfile`) as [`Vm::load`] does: at most `settings.source_len` bytes,
/// the `cmd` rewrite, the compiler's work cap, the globals as its
/// environment. The work is charged as fuel to the running step.
pub fn compile<'gc>(
    ctx: Context<'gc>,
    settings: &Settings,
    name: &str,
    source: &[u8],
) -> Result<piccolo::Function<'gc>, Failure> {
    if source.len() > settings.source_len {
        return Err(Failure::Compile(format!(
            "{}: source is longer than {} bytes",
            chunk_id(name),
            settings.source_len
        )));
    }
    let rewritten = cmd::rewrite(source)
        .map_err(|e| Failure::Compile(format!("{}:{}: {}", chunk_id(name), e.line, e.message)))?;
    let source = rewritten.as_deref().unwrap_or(source);
    // Positions in errors show the chunk name as it is stored.
    let id = chunk_id(name);
    let limits = ctx.limits();
    let budget = CompileBudget::new(CompileLimits {
        memory: limits
            .memory
            .map(|m| (m - m / 16).saturating_sub(ctx.metrics().total_allocation())),
        string_len: limits.string_len,
        work: Some(settings.compile_work),
    });
    let result = FunctionPrototype::compile_with_budget(ctx, &id, source, &budget);
    let fuel =
        budget.work() / cost::COMPILE_WORK as u64 + source.len() as u64 / cost::SOURCE_BYTES as u64;
    ctx.charge_fuel(i32::try_from(fuel).unwrap_or(i32::MAX));
    let proto = result.map_err(|e| match e {
        piccolo::CompilerError::Limit(l) => Failure::Limit(l),
        e => Failure::Compile(compile_message(name, &e)),
    })?;
    let closure = Closure::new(&ctx, proto, Some(ctx.globals()))
        .map_err(|e| Failure::Compile(format!("{}: {e}", chunk_id(name))))?;
    Ok(piccolo::Function::from(closure))
}

/// `luaO_chunkid`'s display of a chunk name: `@path` and `=name` without
/// their mark, anything else as `[string "..."]`.
pub fn chunk_id(name: &str) -> String {
    if let Some(rest) = name.strip_prefix('@').or_else(|| name.strip_prefix('=')) {
        rest.to_string()
    } else {
        let first = name.lines().next().unwrap_or_default();
        if first.len() < name.len() || first.len() > 40 {
            let cut: String = first.chars().take(40).collect();
            format!("[string \"{cut}...\"]")
        } else {
            format!("[string \"{first}\"]")
        }
    }
}

fn compile_message(name: &str, e: &piccolo::CompilerError) -> String {
    match e {
        piccolo::CompilerError::Parsing(p) => {
            format!("{}:{}: {}", chunk_id(name), p.line_number, p.kind)
        }
        piccolo::CompilerError::Compilation(c) => {
            format!("{}:{}: {}", chunk_id(name), c.line_number, c.kind)
        }
        e => format!("{}: {e}", chunk_id(name)),
    }
}

/// An uncaught error's message, without running any Lua (a `__tostring`
/// could loop): strings and numbers as they are, other values by type.
fn error_message(e: &Error<'_>) -> String {
    match e {
        Error::Lua(l) => match l.0 {
            Value::String(s) => s.display_lossy().to_string(),
            v @ (Value::Integer(_) | Value::Number(_)) => v.display().to_string(),
            v => format!("(error object is a {} value)", v.type_name()),
        },
        Error::Runtime(r) => format!("{r:#}"),
    }
}
