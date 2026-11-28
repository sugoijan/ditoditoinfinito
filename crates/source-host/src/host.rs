//! The host the game holds: the installed plugins, each started as a
//! process of its own when first asked ([`crate::supervisor`]), the
//! operations asked of them, and what they answer as [`Event`]s. Nothing
//! here waits: a request is queued and answered later, events are taken
//! when the game looks ([`Host::try_event`]), and `wake` tells it to.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use ddi_library::path::RelPathBuf;
use ddi_library::report::Finding;
use ddi_source_protocol::check::Accepted;
use ddi_source_protocol::{
    Capability, FailCode, FilterValue, Input, ItemMeta, Node, OpId, Reach, Step, Text,
};

use crate::builtin::{self, Builtin, Running};
use crate::installed::Installed;
use crate::store::{FetchedSong, ItemKey, SharedStore};
use crate::supervisor::{Command, Supervisor};

/// How long a plugin may take (`docs/plans/source-plugins.md`, "What the
/// game still does, whatever the plugin").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeouts {
    /// From its start to its Hello.
    pub hello: Duration,
    /// Silent while an operation runs.
    pub silence: Duration,
    /// An open, a search, a browse or a description, whole.
    pub search: Duration,
    /// From a cancel to the operation's end.
    pub cancel: Duration,
    /// From a shutdown to its exit.
    pub stop: Duration,
    /// With nothing to do, before it is shut down.
    pub idle: Duration,
}

impl Default for Timeouts {
    fn default() -> Timeouts {
        Timeouts {
            hello: Duration::from_secs(10),
            silence: Duration::from_secs(30),
            search: Duration::from_secs(60),
            cancel: Duration::from_secs(5),
            stop: Duration::from_secs(5),
            idle: Duration::from_secs(60),
        }
    }
}

/// What the host needs of the app.
#[derive(Clone)]
pub struct HostConfig {
    /// The game's version and the player's language, told each plugin.
    pub app_version: String,
    pub language: String,
    /// Where each plugin's own folders are made: the cache folder's
    /// `plugins/` and the data folder's `plugin-data/` (`<id>/` in each).
    pub plugin_cache: PathBuf,
    pub plugin_data: PathBuf,
    pub timeouts: Timeouts,
    /// Abnormal ends in a run after which a plugin is off until the next.
    pub strikes: u32,
    /// Simfiles and zip names not in UTF-8 (desktop's `encoding_rs`).
    pub legacy: fn(ddi_library::legacy::Legacy, &[u8]) -> Option<String>,
    /// The songs an item holds beyond its simfiles, read as the shell's
    /// library reads a folder's (desktop's works and FFR levels).
    pub more: crate::store::MoreSongs,
}

impl HostConfig {
    pub fn new(plugin_cache: PathBuf, plugin_data: PathBuf) -> HostConfig {
        HostConfig {
            app_version: String::new(),
            language: "en".into(),
            plugin_cache,
            plugin_data,
            timeouts: Timeouts::default(),
            strikes: 3,
            legacy: ddi_library::legacy::none,
            more: crate::store::no_more,
        }
    }
}

/// What the game asks of a plugin.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Open(Input),
    Search {
        scope: Option<String>,
        query: String,
        filters: BTreeMap<String, FilterValue>,
        cursor: Option<String>,
        page_size: u32,
    },
    Browse {
        scope: Option<String>,
        node: Option<String>,
        cursor: Option<String>,
        page_size: u32,
    },
    Describe {
        item: String,
    },
    /// An item's files, read as songs into the store. `revision`, `size`
    /// and `reach` as its metadata said (a revision is another item for
    /// the store; a size over the item's memory asks for the cache folder;
    /// `reach` lowers the manifest's level, never raises it).
    Fetch {
        item: String,
        revision: Option<String>,
        size: Option<u64>,
        reach: Option<Reach>,
    },
}

impl Request {
    /// The capability it needs.
    pub fn capability(&self) -> Capability {
        match self {
            Request::Open(_) => Capability::Input,
            Request::Search { .. } => Capability::Search,
            Request::Browse { .. } => Capability::Browse,
            Request::Describe { .. } => Capability::Metadata,
            Request::Fetch { .. } => Capability::Fetch,
        }
    }
}

/// What happened, for the game to show.
#[allow(clippy::large_enum_variant)] // Taken one at a time.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A plugin started and said what it does.
    Ready { plugin: String, accepted: Accepted },
    /// A plugin's process ended: why, whether it is off for the run now,
    /// and the last of its diagnostics where it ended badly.
    Stopped {
        plugin: String,
        why: Stop,
        off: bool,
        diagnostics: Option<String>,
    },
    Opened {
        op: OpId,
        scope: String,
        title: Option<Text>,
    },
    Results {
        op: OpId,
        items: Vec<ItemMeta>,
    },
    Nodes {
        op: OpId,
        nodes: Vec<Node>,
    },
    Done {
        op: OpId,
        cursor: Option<String>,
        total: Option<u64>,
    },
    Item {
        op: OpId,
        item: ItemMeta,
    },
    Progress {
        op: OpId,
        step: Step,
        done: Option<u64>,
        total: Option<u64>,
        bytes: Option<u64>,
    },
    /// A fetch's songs, in the store under `key`; `skipped` the folders
    /// that did not read, with why.
    Fetched {
        op: OpId,
        key: ItemKey,
        songs: Vec<FetchedSong>,
        skipped: Vec<(RelPathBuf, Finding)>,
    },
    Failed {
        op: OpId,
        failure: Failure,
        diagnostics: Option<String>,
    },
}

impl Event {
    /// The operation it ends, if it ends one.
    pub fn ends(&self) -> Option<OpId> {
        match self {
            Event::Opened { op, .. }
            | Event::Done { op, .. }
            | Event::Item { op, .. }
            | Event::Fetched { op, .. }
            | Event::Failed { op, .. } => Some(*op),
            _ => None,
        }
    }
}

/// Why an operation failed, as values the game words.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The plugin said so: its code, and its own words (data, cleaned).
    Plugin {
        code: FailCode,
        detail: Option<String>,
    },
    /// The host's.
    Host(HostFailure),
}

#[derive(Clone, Debug, PartialEq)]
pub enum HostFailure {
    /// No plugin of that id is installed.
    NotInstalled,
    /// Installed and not switched on.
    SwitchedOff,
    /// Off for this run (three abnormal ends, or a Hello the game does
    /// not take).
    OffForTheRun,
    /// It does not offer what was asked (or not with these values).
    Unsupported,
    /// [`ddi_source_protocol::limits::MAX_OPS`] operations already run.
    Busy,
    Cancelled,
    /// It could not be started (the system's words).
    Start(String),
    /// It took too long, and was stopped.
    TimedOut,
    /// It exited or broke its output while working.
    Crashed,
    /// It broke the protocol (why, for a log), and was stopped.
    Misbehaved(String),
    /// No protocol version in common.
    Incompatible { plugin_newer: bool },
    /// Past the store's limits.
    TooLarge,
    /// Nothing in what it fetched read as songs (why, data).
    Unreadable(Vec<(RelPathBuf, Finding)>),
    /// The game stopped it (or the host was shut down).
    Stopped,
}

/// Why a plugin's process ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Shut down after a while with nothing to do.
    Idle,
    /// Shut down by the game.
    Asked,
    /// It exited by itself with nothing running, successfully.
    Exited,
    /// It exited or broke its output while it had work, or exited
    /// unsuccessfully (its exit status, data).
    Crashed(Option<String>),
    TimedOut,
    Misbehaved(String),
    Incompatible { plugin_newer: bool },
    /// Its Hello is not its manifest's, or breaks the limits.
    InvalidHello(String),
    StartFailed(String),
}

impl Stop {
    /// It counts toward the plugin's three abnormal ends.
    pub fn strikes(&self) -> bool {
        matches!(
            self,
            Stop::Crashed(_) | Stop::TimedOut | Stop::Misbehaved(_) | Stop::StartFailed(_)
        )
    }

    /// What it means for an operation still running.
    pub fn failure(&self) -> HostFailure {
        match self {
            Stop::Idle | Stop::Asked | Stop::Exited => HostFailure::Stopped,
            Stop::Crashed(_) => HostFailure::Crashed,
            Stop::TimedOut => HostFailure::TimedOut,
            Stop::Misbehaved(why) | Stop::InvalidHello(why) => HostFailure::Misbehaved(why.clone()),
            Stop::Incompatible { plugin_newer } => HostFailure::Incompatible {
                plugin_newer: *plugin_newer,
            },
            Stop::StartFailed(why) => HostFailure::Start(why.clone()),
        }
    }
}

/// What lasts of a plugin across its processes in a run.
#[derive(Default)]
pub(crate) struct PluginState {
    pub(crate) strikes: AtomicU32,
    /// Off for the run.
    pub(crate) off: Mutex<bool>,
    /// What turned it off for the run (its last end then).
    pub(crate) why: Mutex<Option<Stop>>,
}

impl PluginState {
    pub(crate) fn is_off(&self) -> bool {
        self.off.lock().map(|o| *o).unwrap_or(true)
    }
}

struct Plugin {
    installed: Arc<Installed>,
    on: bool,
    state: Arc<PluginState>,
    runner: Option<(Sender<Command>, JoinHandle<()>)>,
}

pub struct Host {
    config: Arc<HostConfig>,
    store: SharedStore,
    plugins: BTreeMap<String, Plugin>,
    events: Sender<Event>,
    received: Receiver<Event>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    next_op: OpId,
    /// Each running operation's plugin.
    running: HashMap<OpId, String>,
    /// The sources built into the app, by id ([`crate::builtin`]).
    builtins: BTreeMap<String, Running>,
    /// The game is quiet (a play): nothing new starts, built-in work
    /// waits ([`Host::set_quiet`]).
    quiet: Arc<AtomicBool>,
    /// Asked while quiet: started once it is not.
    queued: Vec<(OpId, String, Request)>,
}

impl Host {
    /// A host for `installed` plugins, those in `on` switched on; nothing
    /// starts until asked.
    pub fn new(
        config: HostConfig,
        installed: Vec<Installed>,
        on: &BTreeSet<String>,
        store: SharedStore,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Host {
        let (events, received) = mpsc::channel();
        let plugins = installed
            .into_iter()
            .map(|p| {
                let id = p.id().to_string();
                let plugin = Plugin {
                    on: on.contains(&id),
                    installed: Arc::new(p),
                    state: Arc::default(),
                    runner: None,
                };
                (id, plugin)
            })
            .collect();
        Host {
            config: Arc::new(config),
            store,
            plugins,
            events,
            received,
            wake,
            next_op: 0,
            running: HashMap::new(),
            builtins: BTreeMap::new(),
            quiet: Arc::default(),
            queued: Vec::new(),
        }
    }

    /// Adds a source built into the app, asked as a plugin switched on
    /// is (`id` is its own; an installed plugin of the same id is passed
    /// over).
    pub fn add_builtin(&mut self, source: Arc<dyn Builtin>) {
        let id = source.id().to_string();
        self.plugins.remove(&id);
        self.builtins.insert(id, Running::new(source));
    }

    /// Takes the built-in source `id` out (its running operations end
    /// as they would; nothing new is asked of it).
    pub fn remove_builtin(&mut self, id: &str) {
        self.builtins.remove(id);
    }

    /// The sources built into the app.
    pub fn builtins(&self) -> impl Iterator<Item = &Arc<dyn Builtin>> {
        self.builtins.values().map(Running::source)
    }

    /// Whether `id` is a source built into the app.
    pub fn is_builtin(&self, id: &str) -> bool {
        self.builtins.contains_key(id)
    }

    /// The game is quiet, or no longer (`docs/plans/source-plugins.md`,
    /// section 3: nothing new starts in a source during a play). While
    /// quiet, a request is kept and started once it is not; a built-in
    /// source's operations wait at their next step; a plugin's already
    /// running go on in its own process, at its lower priority, and what
    /// they answer waits in the host until the game looks.
    pub fn set_quiet(&mut self, quiet: bool) {
        self.quiet.store(quiet, Ordering::Relaxed);
        if !quiet {
            for (op, plugin, request) in std::mem::take(&mut self.queued) {
                self.dispatch(op, &plugin, request);
            }
        }
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet.load(Ordering::Relaxed)
    }

    /// Requests kept while quiet.
    pub fn queued(&self) -> usize {
        self.queued.len()
    }

    pub fn store(&self) -> &SharedStore {
        &self.store
    }

    /// The installed plugins.
    pub fn plugins(&self) -> impl Iterator<Item = &Installed> {
        self.plugins.values().map(|p| &*p.installed)
    }

    pub fn is_on(&self, plugin: &str) -> bool {
        self.plugins.get(plugin).is_some_and(|p| p.on)
    }

    /// Off for the rest of the run.
    pub fn is_off_for_the_run(&self, plugin: &str) -> bool {
        self.plugins.get(plugin).is_some_and(|p| p.state.is_off())
    }

    /// What turned `plugin` off for the run (its third abnormal end, a
    /// protocol it does not share, a Hello not its manifest's), where it
    /// is off.
    pub fn off_why(&self, plugin: &str) -> Option<Stop> {
        let p = self.plugins.get(plugin)?;
        if !p.state.is_off() {
            return None;
        }
        p.state.why.lock().ok()?.clone()
    }

    /// Switches `plugin` on or off for the host (the switches' file is the
    /// game's to write): off, its process is shut down and what it was
    /// asked fails as stopped; on, it starts when next asked. Nothing is
    /// started here.
    pub fn switch(&mut self, plugin: &str, on: bool) {
        let Some(p) = self.plugins.get_mut(plugin) else {
            return;
        };
        p.on = on;
        if !on {
            if let Some((tx, _)) = p.runner.take() {
                let _ = tx.send(Command::Stop);
            }
            let (kept, dropped): (Vec<_>, Vec<_>) = std::mem::take(&mut self.queued)
                .into_iter()
                .partition(|(_, id, _)| id != plugin);
            self.queued = kept;
            for (op, ..) in dropped {
                self.fail(op, HostFailure::SwitchedOff);
            }
        }
    }

    /// The plugins installed now (the folder read again, "Look again"),
    /// those in `on` switched on. A plugin as it was keeps its process and
    /// what it is this run (its strikes, off for the run); one whose folder
    /// changed (its manifest, its program) is stopped and taken as new but
    /// for what it is this run; one gone is stopped and forgotten. Nothing
    /// is started.
    pub fn set_installed(&mut self, installed: Vec<Installed>, on: &BTreeSet<String>) {
        let mut before = std::mem::take(&mut self.plugins);
        for p in installed {
            let id = p.id().to_string();
            if self.builtins.contains_key(&id) {
                continue;
            }
            let plugin = match before.remove(&id) {
                Some(mut old) if *old.installed == p => {
                    old.on = on.contains(&id);
                    old
                }
                Some(mut old) => {
                    if let Some((tx, _)) = old.runner.take() {
                        let _ = tx.send(Command::Stop);
                    }
                    Plugin {
                        installed: Arc::new(p),
                        on: on.contains(&id),
                        state: old.state.clone(),
                        runner: None,
                    }
                }
                None => Plugin {
                    installed: Arc::new(p),
                    on: on.contains(&id),
                    state: Arc::default(),
                    runner: None,
                },
            };
            self.plugins.insert(id, plugin);
        }
        for (id, mut gone) in before {
            if let Some((tx, _)) = gone.runner.take() {
                let _ = tx.send(Command::Stop);
            }
            let (kept, dropped): (Vec<_>, Vec<_>) = std::mem::take(&mut self.queued)
                .into_iter()
                .partition(|(_, plugin, _)| *plugin != id);
            self.queued = kept;
            for (op, ..) in dropped {
                self.fail(op, HostFailure::NotInstalled);
            }
        }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        if let Some(w) = &self.wake {
            w();
        }
    }

    /// The plugin's supervisor, started where it is not running; or why
    /// it cannot be.
    fn runner(&mut self, plugin: &str) -> Result<Sender<Command>, HostFailure> {
        let Some(p) = self.plugins.get_mut(plugin) else {
            return Err(HostFailure::NotInstalled);
        };
        if !p.on {
            return Err(HostFailure::SwitchedOff);
        }
        if p.state.is_off() {
            return Err(HostFailure::OffForTheRun);
        }
        if let Some((tx, thread)) = &p.runner
            && !thread.is_finished()
        {
            return Ok(tx.clone());
        }
        let (tx, commands) = mpsc::channel();
        let supervisor = Supervisor {
            plugin: p.installed.clone(),
            config: self.config.clone(),
            store: self.store.clone(),
            events: self.events.clone(),
            wake: self.wake.clone(),
            commands,
            state: p.state.clone(),
        };
        let thread = std::thread::Builder::new()
            .name(format!("ddi-plugin-{plugin}"))
            .spawn(move || supervisor.run())
            .map_err(|e| HostFailure::Start(e.to_string()))?;
        p.runner = Some((tx.clone(), thread));
        Ok(tx)
    }

    /// Starts `plugin` without asking it anything (`--source-check`): an
    /// [`Event::Ready`] or [`Event::Stopped`] follows. `false` where it
    /// cannot start (not installed, switched off, off for the run).
    pub fn start(&mut self, plugin: &str) -> bool {
        match self.runner(plugin) {
            Ok(tx) => tx.send(Command::Start).is_ok(),
            Err(_) => false,
        }
    }

    /// Asks `plugin` (or a built-in source) for `request`: its answers
    /// and its end come as events of the operation returned. While the
    /// game is quiet ([`Host::set_quiet`]) it waits to start.
    pub fn request(&mut self, plugin: &str, request: Request) -> OpId {
        self.next_op = self.next_op.wrapping_add(1).max(1);
        let op = self.next_op;
        // A fetch of an item already in the store needs no plugin.
        if let Request::Fetch { item, revision, .. } = &request {
            let key = ItemKey::of(plugin, item, revision.as_deref());
            let songs = self.store.lock().ok().and_then(|mut s| {
                s.contains(key).then(|| s.songs(key).map(<[_]>::to_vec)).flatten()
            });
            if let Some(songs) = songs {
                self.emit(Event::Fetched {
                    op,
                    key,
                    songs,
                    skipped: Vec::new(),
                });
                return op;
            }
        }
        if self.is_quiet() {
            self.queued.push((op, plugin.to_string(), request));
            return op;
        }
        self.dispatch(op, plugin, request);
        op
    }

    fn dispatch(&mut self, op: OpId, plugin: &str, request: Request) {
        if let Some(running) = self.builtins.get(plugin) {
            builtin::start(
                running,
                builtin::Spawn {
                    config: self.config.clone(),
                    store: self.store.clone(),
                    events: self.events.clone(),
                    wake: self.wake.clone(),
                    quiet: self.quiet.clone(),
                },
                op,
                request,
            );
            self.running.insert(op, plugin.to_string());
            return;
        }
        match self.runner(plugin) {
            Ok(tx) if tx.send(Command::Op { op, request }).is_ok() => {
                self.running.insert(op, plugin.to_string());
            }
            Ok(_) => self.fail(op, HostFailure::Stopped),
            Err(why) => self.fail(op, why),
        }
    }

    fn fail(&self, op: OpId, why: HostFailure) {
        self.emit(Event::Failed {
            op,
            failure: Failure::Host(why),
            diagnostics: None,
        });
    }

    /// Cancels `op`: it fails as cancelled at once, and its plugin is
    /// told to stop it.
    pub fn cancel(&mut self, op: OpId) {
        if let Some(i) = self.queued.iter().position(|(o, ..)| *o == op) {
            self.queued.remove(i);
            self.fail(op, HostFailure::Cancelled);
            return;
        }
        if let Some(running) = self
            .running
            .get(&op)
            .and_then(|plugin| self.builtins.get(plugin))
        {
            if builtin::cancel(running, op, &self.events)
                && let Some(w) = &self.wake
            {
                w();
            }
            return;
        }
        if let Some(plugin) = self.running.get(&op)
            && let Some((tx, _)) = self.plugins.get(plugin).and_then(|p| p.runner.as_ref())
        {
            let _ = tx.send(Command::Cancel { op });
        }
    }

    /// The next event, if one is waiting.
    pub fn try_event(&mut self) -> Option<Event> {
        let event = self.received.try_recv().ok()?;
        if let Some(op) = event.ends() {
            self.running.remove(&op);
        }
        Some(event)
    }

    /// The next event, waiting up to `timeout` for it (off the frame:
    /// tests and `--source-check`).
    pub fn wait_event(&mut self, timeout: Duration) -> Option<Event> {
        let event = self.received.recv_timeout(timeout).ok()?;
        if let Some(op) = event.ends() {
            self.running.remove(&op);
        }
        Some(event)
    }

    /// Shuts `plugin`'s process down (its running operations fail as
    /// stopped).
    pub fn stop(&mut self, plugin: &str) {
        if let Some((tx, _)) = self.plugins.get(plugin).and_then(|p| p.runner.as_ref()) {
            let _ = tx.send(Command::Stop);
        }
    }

    /// Every plugin shut down, each within its stop's time (then killed),
    /// and their threads ended. For the game's exit.
    pub fn shutdown(&mut self) {
        for (op, ..) in std::mem::take(&mut self.queued) {
            self.fail(op, HostFailure::Stopped);
        }
        let runners: Vec<_> = self
            .plugins
            .values_mut()
            .filter_map(|p| p.runner.take())
            .collect();
        for (tx, _) in &runners {
            let _ = tx.send(Command::Stop);
        }
        for (tx, thread) in runners {
            drop(tx);
            let _ = thread.join();
        }
    }

    /// Abnormal ends of `plugin`'s so far this run.
    pub fn strikes(&self, plugin: &str) -> u32 {
        self.plugins
            .get(plugin)
            .map_or(0, |p| p.state.strikes.load(Ordering::Relaxed))
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shutdown();
    }
}
