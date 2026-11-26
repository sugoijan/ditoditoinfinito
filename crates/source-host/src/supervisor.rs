//! One plugin's thread: it starts the plugin's process when there is
//! something to ask, speaks the protocol with it, holds every message to
//! the limits and every operation to its order, and stops the process
//! when it goes quiet, overstays, breaks the protocol, or has nothing to
//! do for a while. Every operation still running when the process ends
//! fails with why; an abnormal end counts toward the plugin's three.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ddi_source_protocol::check::{self, Accepted, HelloProblem, plugin_message_problem};
use ddi_source_protocol::limits::{MAX_OPS, MAX_PAGE};
use ddi_source_protocol::text::clean;
use ddi_source_protocol::{
    AppInfo, AppMessage, Capability, Deliver, Delivery, Dirs, ItemMeta, Node, OpId,
    PluginMessage, RoomRule, Text, VERSIONS,
};

use crate::assemble::{Incoming, Reject, read_songs};
use crate::host::{Event, Failure, HostConfig, HostFailure, PluginState, Request, Stop};
use crate::installed::Installed;
use crate::process::{Connection, SendProblem};
use crate::store::{ItemFiles, ItemKey, Origin, SharedStore};

/// How often the thread looks at its timers while a process runs.
const TICK: Duration = Duration::from_millis(20);

/// Progress told the game at most this often per operation.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// How long the diagnostics of a plugin that ended badly are waited for.
const DIAGNOSTICS_SETTLE: Duration = Duration::from_millis(300);

pub(crate) enum Command {
    Start,
    Op { op: OpId, request: Request },
    Cancel { op: OpId },
    Stop,
}

pub(crate) struct Supervisor {
    pub(crate) plugin: Arc<Installed>,
    pub(crate) config: Arc<HostConfig>,
    pub(crate) store: SharedStore,
    pub(crate) events: Sender<Event>,
    pub(crate) wake: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) commands: Receiver<Command>,
    pub(crate) state: Arc<PluginState>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open,
    Search,
    Browse,
    Describe,
    Fetch,
}

struct Running {
    kind: Kind,
    started: Instant,
    /// When the plugin was told to cancel it.
    cancelled: Option<Instant>,
    /// The game was told how it ended (cancelled, too large): what else
    /// comes of it is let go.
    told: bool,
    /// Items and nodes so far, and the page's size.
    count: usize,
    page: usize,
    fetch: Option<FetchState>,
    progress_at: Option<Instant>,
}

struct FetchState {
    item: String,
    revision: Option<String>,
    rooms: RoomRule,
    deliver: Deliver,
    incoming: Option<Incoming>,
}

enum Phase {
    Hello(Instant),
    Ready(Accepted),
    /// Shut down: since when, whether it said Bye, and why.
    Stopping(Instant, bool, Stop),
}

struct Session<'a> {
    sup: &'a Supervisor,
    conn: Connection,
    phase: Phase,
    ops: BTreeMap<OpId, Running>,
    /// Asked before its Hello.
    waiting: Vec<(OpId, Request)>,
    /// Asked while it was shutting down: for its next process.
    later: Vec<Command>,
    heard: Instant,
    idle_since: Instant,
    /// When the process was first seen exited (its output may stay open:
    /// a child of its own can hold it).
    exit_seen: Option<Instant>,
}

impl Supervisor {
    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        if let Some(w) = &self.wake {
            w();
        }
    }

    fn fail(&self, op: OpId, why: HostFailure, diagnostics: Option<String>) {
        self.emit(Event::Failed {
            op,
            failure: Failure::Host(why),
            diagnostics,
        });
    }

    /// The thread: a process for each stretch of work, until the host
    /// stops it.
    pub(crate) fn run(self) {
        let mut queued: VecDeque<Command> = VecDeque::new();
        loop {
            let command = match queued.pop_front() {
                Some(c) => c,
                None => match self.commands.recv() {
                    Ok(c) => c,
                    Err(_) => return,
                },
            };
            match command {
                Command::Stop => {
                    for c in queued.drain(..) {
                        if let Command::Op { op, .. } = c {
                            self.fail(op, HostFailure::Stopped, None);
                        }
                    }
                    return;
                }
                // Nothing runs: an operation that ended already.
                Command::Cancel { .. } => {}
                Command::Start | Command::Op { .. } => {
                    if self.state.is_off() {
                        if let Command::Op { op, .. } = command {
                            self.fail(op, HostFailure::OffForTheRun, None);
                        }
                        continue;
                    }
                    let (later, stopped) = self.session(command);
                    queued.extend(later);
                    if stopped {
                        queued.push_front(Command::Stop);
                    }
                }
            }
        }
    }

    /// One process, from its start to its end: the commands asked while
    /// it was ending (for the next), and whether the host stopped it.
    fn session(&self, first: Command) -> (Vec<Command>, bool) {
        let conn = match Connection::spawn(&self.plugin.executable, &self.plugin.dir) {
            Ok(c) => c,
            Err(e) => {
                let why = Stop::StartFailed(e.to_string());
                let off = self.strike(&why);
                if let Command::Op { op, .. } = first {
                    self.fail(op, why.failure(), None);
                }
                self.emit(Event::Stopped {
                    plugin: self.plugin.id().to_string(),
                    why,
                    off,
                    diagnostics: None,
                });
                return (Vec::new(), false);
            }
        };
        let now = Instant::now();
        let mut s = Session {
            sup: self,
            conn,
            phase: Phase::Hello(now),
            ops: BTreeMap::new(),
            waiting: Vec::new(),
            later: Vec::new(),
            heard: now,
            idle_since: now,
            exit_seen: None,
        };
        let id = self.plugin.id();
        let hello = AppMessage::Hello {
            protocols: VERSIONS.to_vec(),
            features: Vec::new(),
            app: AppInfo {
                version: self.config.app_version.clone(),
                language: self.config.language.clone(),
            },
            dirs: Dirs {
                cache: self.config.plugin_cache.join(id),
                data: self.config.plugin_data.join(id),
            },
        };
        let mut stopped_by_host = false;
        let end = (|| -> Stop {
            if let Err(stop) = s.send(&hello) {
                return stop;
            }
            if let Command::Op { op, request } = first
                && let Err(stop) = s.accept(op, request)
            {
                return stop;
            }
            loop {
                match s.conn.incoming.recv_timeout(TICK) {
                    Ok(Ok(Some((message, attachment)))) => {
                        s.heard = Instant::now();
                        if let Err(stop) = s.on_message(message, attachment) {
                            return stop;
                        }
                        if let Phase::Stopping(_, true, why) = &s.phase {
                            let why = why.clone();
                            // Said Bye: its exit is waited for, then it is
                            // killed.
                            s.conn.wait_exit(self.config.timeouts.stop);
                            return why;
                        }
                    }
                    Ok(Ok(None)) | Err(RecvTimeoutError::Disconnected) => return s.ended(),
                    Ok(Err(e)) => {
                        return match e {
                            ddi_source_protocol::WireError::Io(_) => s.ended(),
                            // A frame cut short is a broken output, never a
                            // clean end.
                            ddi_source_protocol::WireError::Truncated => {
                                let status = s.conn.wait_exit(Duration::from_millis(500));
                                Stop::Crashed(status.map(|s| s.to_string()))
                            }
                            e => Stop::Misbehaved(e.to_string()),
                        };
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                }
                while let Ok(command) = self.commands.try_recv() {
                    let result = match command {
                        Command::Stop => {
                            stopped_by_host = true;
                            s.shut_down(Stop::Asked)
                        }
                        Command::Cancel { op } => s.cancel(op),
                        Command::Start => Ok(()),
                        Command::Op { op, request } => {
                            if matches!(s.phase, Phase::Stopping(..)) {
                                s.later.push(Command::Op { op, request });
                                Ok(())
                            } else {
                                s.accept(op, request)
                            }
                        }
                    };
                    if let Err(stop) = result {
                        return stop;
                    }
                }
                if let Err(stop) = s.timers() {
                    return stop;
                }
            }
        })();
        let later = s.finish(end);
        (later, stopped_by_host)
    }

    /// Counts `why` toward the plugin's strikes where it is one, and
    /// turns the plugin off for the run where it must be: whether it is.
    fn strike(&self, why: &Stop) -> bool {
        let mut off = matches!(why, Stop::Incompatible { .. } | Stop::InvalidHello(_));
        if why.strikes() {
            let n = self.state.strikes.fetch_add(1, Ordering::Relaxed) + 1;
            off |= n >= self.config.strikes;
        }
        if off && let Ok(mut o) = self.state.off.lock() {
            *o = true;
        }
        off
    }
}

impl Session<'_> {
    fn send(&self, message: &AppMessage) -> Result<(), Stop> {
        self.conn.send(message).map_err(|e| match e {
            SendProblem::NotReading => Stop::Misbehaved("it does not read what it is sent".into()),
            SendProblem::Closed => Stop::Crashed(None),
        })
    }

    /// Asks `op` of the plugin, or fails it at once where it cannot be.
    fn accept(&mut self, op: OpId, request: Request) -> Result<(), Stop> {
        let accepted = match &self.phase {
            Phase::Hello(_) => {
                self.waiting.push((op, request));
                return Ok(());
            }
            Phase::Stopping(..) => {
                self.later.push(Command::Op { op, request });
                return Ok(());
            }
            Phase::Ready(a) => a,
        };
        let offered = accepted.capabilities.contains(&request.capability())
            && match &request {
                Request::Open(input) => {
                    accepted.inputs.contains(&input.kind)
                        && input.value.len() <= ddi_source_protocol::limits::MAX_INPUT_BYTES
                }
                Request::Search { filters, query, .. } => {
                    query.chars().count() <= ddi_source_protocol::limits::MAX_QUERY_CHARS
                        && (filters.is_empty()
                            || accepted.capabilities.contains(&Capability::Filters))
                        && check::filter_values_problem(&accepted.filters, filters).is_none()
                }
                _ => true,
            };
        if !offered {
            self.sup.fail(op, HostFailure::Unsupported, None);
            return Ok(());
        }
        if self.ops.len() >= MAX_OPS {
            self.sup.fail(op, HostFailure::Busy, None);
            return Ok(());
        }
        let page = |p: u32| p.clamp(1, MAX_PAGE);
        let (kind, page_size, fetch, message) = match request {
            Request::Open(input) => (Kind::Open, 0, None, AppMessage::Open { op, input }),
            Request::Search {
                scope,
                query,
                filters,
                cursor,
                page_size,
            } => (
                Kind::Search,
                page(page_size),
                None,
                AppMessage::Search {
                    op,
                    scope,
                    query,
                    filters,
                    cursor,
                    page_size: page(page_size),
                },
            ),
            Request::Browse {
                scope,
                node,
                cursor,
                page_size,
            } => (
                Kind::Browse,
                page(page_size),
                None,
                AppMessage::Browse {
                    op,
                    scope,
                    node,
                    cursor,
                    page_size: page(page_size),
                },
            ),
            Request::Describe { item } => (Kind::Describe, 0, None, AppMessage::Describe { op, item }),
            Request::Fetch {
                item,
                revision,
                size,
                rooms,
            } => {
                let (budget, folder) = match self.sup.store.lock() {
                    Ok(mut s) => {
                        let budget = s.limits().item_memory;
                        let over = size.is_some_and(|n| n > budget);
                        (budget, over.then(|| s.new_folder("cached")))
                    }
                    Err(_) => {
                        self.sup.fail(op, HostFailure::TooLarge, None);
                        return Ok(());
                    }
                };
                let deliver = match folder {
                    Some(path) => {
                        if let Err(e) = std::fs::create_dir_all(&path) {
                            self.sup.fail(op, HostFailure::Start(e.to_string()), None);
                            return Ok(());
                        }
                        Deliver::CacheDir { path }
                    }
                    None => Deliver::Memory { budget },
                };
                // An item's metadata narrows its manifest's rule, never
                // widens it.
                let rooms = match (self.sup.plugin.manifest.rooms, rooms) {
                    (RoomRule::None, _) | (_, Some(RoomRule::None)) => RoomRule::None,
                    _ => RoomRule::Upload,
                };
                let fetch = FetchState {
                    item: item.clone(),
                    revision,
                    rooms,
                    deliver: deliver.clone(),
                    incoming: Some(Incoming::new(self.sup.store.clone())),
                };
                (
                    Kind::Fetch,
                    0,
                    Some(fetch),
                    AppMessage::Fetch { op, item, deliver },
                )
            }
        };
        if self.ops.is_empty() {
            self.heard = Instant::now();
        }
        self.ops.insert(
            op,
            Running {
                kind,
                started: Instant::now(),
                cancelled: None,
                told: false,
                count: 0,
                page: page_size as usize,
                fetch,
                progress_at: None,
            },
        );
        self.send(&message)
    }

    fn cancel(&mut self, op: OpId) -> Result<(), Stop> {
        if let Some(i) = self.waiting.iter().position(|(o, _)| *o == op) {
            self.waiting.remove(i);
            self.sup.fail(op, HostFailure::Cancelled, None);
            return Ok(());
        }
        let Some(run) = self.ops.get_mut(&op) else {
            return Ok(());
        };
        if !run.told {
            run.told = true;
            self.sup.fail(op, HostFailure::Cancelled, None);
        }
        self.stop_op(op)
    }

    /// Tells the plugin to stop `op` (the game has its end already).
    fn stop_op(&mut self, op: OpId) -> Result<(), Stop> {
        let Some(run) = self.ops.get_mut(&op) else {
            return Ok(());
        };
        if let Some(f) = &mut run.fetch {
            f.incoming = None;
        }
        if run.cancelled.is_none() {
            run.cancelled = Some(Instant::now());
            return self.send(&AppMessage::Cancel { op });
        }
        Ok(())
    }

    fn shut_down(&mut self, why: Stop) -> Result<(), Stop> {
        if !matches!(self.phase, Phase::Stopping(..)) {
            self.phase = Phase::Stopping(Instant::now(), false, why);
            return self.send(&AppMessage::Shutdown {});
        }
        Ok(())
    }

    /// The process's output ended: how it ended.
    fn ended(&mut self) -> Stop {
        let status = self.conn.wait_exit(Duration::from_millis(500));
        if let Phase::Stopping(_, _, why) = &self.phase {
            return why.clone();
        }
        match status {
            Some(s) if s.success() && self.ops.is_empty() && self.waiting.is_empty() => {
                Stop::Exited
            }
            status => Stop::Crashed(status.map(|s| s.to_string())),
        }
    }

    fn timers(&mut self) -> Result<(), Stop> {
        let t = self.sup.config.timeouts;
        if self.conn.exited().is_some() {
            let seen = *self.exit_seen.get_or_insert_with(Instant::now);
            if seen.elapsed() > Duration::from_millis(500) {
                return Err(self.ended());
            }
        }
        match &self.phase {
            Phase::Hello(since) if since.elapsed() > t.hello => return Err(Stop::TimedOut),
            Phase::Stopping(since, _, why) if since.elapsed() > t.stop => {
                // Shut down and still running: killed.
                return Err(match why {
                    Stop::Idle | Stop::Asked => Stop::TimedOut,
                    other => other.clone(),
                });
            }
            _ => {}
        }
        if !self.ops.is_empty() && self.heard.elapsed() > t.silence {
            return Err(Stop::TimedOut);
        }
        for run in self.ops.values() {
            let whole = matches!(
                run.kind,
                Kind::Open | Kind::Search | Kind::Browse | Kind::Describe
            );
            if (whole && run.started.elapsed() > t.search)
                || run.cancelled.is_some_and(|c| c.elapsed() > t.cancel)
            {
                return Err(Stop::TimedOut);
            }
        }
        if matches!(self.phase, Phase::Ready(_))
            && self.ops.is_empty()
            && self.idle_since.elapsed() > t.idle
        {
            self.shut_down(Stop::Idle)?;
        }
        Ok(())
    }

    fn on_message(&mut self, message: PluginMessage, attachment: Vec<u8>) -> Result<(), Stop> {
        if let Some(problem) = plugin_message_problem(&message) {
            return Err(Stop::Misbehaved(problem));
        }
        match (&self.phase, message) {
            (Phase::Hello(_), hello @ PluginMessage::Hello { .. }) => {
                let accepted =
                    check::accept_hello(&hello, &self.sup.plugin.manifest, VERSIONS).map_err(
                        |p| match p {
                            HelloProblem::Incompatible { plugin_newer } => {
                                Stop::Incompatible { plugin_newer }
                            }
                            HelloProblem::Invalid(why) => Stop::InvalidHello(why),
                        },
                    )?;
                let mut shown = accepted.clone();
                for f in &mut shown.filters {
                    clean_text(&mut f.label);
                    if let ddi_source_protocol::FilterKind::Choice { choices, .. } = &mut f.kind {
                        for c in choices {
                            clean_text(&mut c.label);
                        }
                    }
                }
                self.sup.emit(Event::Ready {
                    plugin: self.sup.plugin.id().to_string(),
                    accepted: shown,
                });
                self.phase = Phase::Ready(accepted);
                self.idle_since = Instant::now();
                for (op, request) in std::mem::take(&mut self.waiting) {
                    self.accept(op, request)?;
                }
                Ok(())
            }
            (Phase::Hello(_), _) => Err(Stop::Misbehaved("a message before its Hello".into())),
            (Phase::Stopping(since, _, why), PluginMessage::Bye {}) => {
                self.phase = Phase::Stopping(*since, true, why.clone());
                Ok(())
            }
            // Shutting down: what else comes is let go (its Hello too,
            // where it was shut down before it said one).
            (Phase::Stopping(..), _) => Ok(()),
            (_, PluginMessage::Bye {}) => Err(Stop::Misbehaved("Bye unasked".into())),
            (_, PluginMessage::Hello { .. }) => Err(Stop::Misbehaved("a second Hello".into())),
            (Phase::Ready(_), message) => self.on_op_message(message, attachment),
        }
    }

    fn on_op_message(&mut self, message: PluginMessage, attachment: Vec<u8>) -> Result<(), Stop> {
        let op = message.op().expect("not Hello or Bye");
        let Some(run) = self.ops.get_mut(&op) else {
            return Err(Stop::Misbehaved(format!("a message of an operation not running ({op})")));
        };
        let told = run.told;
        let kind = run.kind;
        let wrong = || Stop::Misbehaved("a message its operation does not take".into());
        let ended = match message {
            PluginMessage::Progress {
                step,
                done,
                total,
                bytes,
                ..
            } => {
                let due = run.progress_at.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY);
                if !told && due {
                    run.progress_at = Some(Instant::now());
                    self.sup.emit(Event::Progress {
                        op,
                        step,
                        done,
                        total,
                        bytes,
                    });
                }
                return Ok(());
            }
            PluginMessage::Failed { code, detail, .. } => {
                Event::Failed {
                    op,
                    failure: Failure::Plugin {
                        code,
                        detail: detail.map(|d| clean(&d)).filter(|d| !d.is_empty()),
                    },
                    diagnostics: None,
                }
            }
            PluginMessage::Opened { scope, title, .. } if kind == Kind::Open => {
                Event::Opened {
                    op,
                    scope,
                    title: title.map(|mut t| {
                        clean_text(&mut t);
                        t
                    }),
                }
            }
            PluginMessage::Results { mut items, .. }
                if matches!(kind, Kind::Search | Kind::Browse) =>
            {
                run.count += items.len();
                if run.count > run.page {
                    return Err(Stop::Misbehaved("more results than its page".into()));
                }
                if !told {
                    items.iter_mut().for_each(clean_item);
                    self.sup.emit(Event::Results { op, items });
                }
                return Ok(());
            }
            PluginMessage::Nodes { mut nodes, .. } if kind == Kind::Browse => {
                run.count += nodes.len();
                if run.count > run.page {
                    return Err(Stop::Misbehaved("more nodes than its page".into()));
                }
                if !told {
                    nodes.iter_mut().for_each(clean_node);
                    self.sup.emit(Event::Nodes { op, nodes });
                }
                return Ok(());
            }
            PluginMessage::Done { cursor, total, .. }
                if matches!(kind, Kind::Search | Kind::Browse) =>
            {
                Event::Done { op, cursor, total }
            }
            PluginMessage::Item { mut item, .. } if kind == Kind::Describe => {
                clean_item(&mut item);
                Event::Item { op, item }
            }
            PluginMessage::File {
                path,
                size,
                offset,
                last,
                ..
            } if kind == Kind::Fetch => {
                let fetch = run.fetch.as_mut().expect("a fetch has its state");
                if matches!(fetch.deliver, Deliver::CacheDir { .. }) {
                    return Err(Stop::Misbehaved("files streamed into a cache folder's fetch".into()));
                }
                let Some(incoming) = fetch.incoming.as_mut() else {
                    // Let go after its end was told.
                    return Ok(());
                };
                return match incoming.chunk(&path, size, offset, last, &attachment) {
                    Ok(()) => Ok(()),
                    Err(Reject::Violation(why)) => Err(Stop::Misbehaved(why)),
                    Err(other) => {
                        let why = match other {
                            Reject::TooLarge => HostFailure::TooLarge,
                            Reject::Unreadable(why) => HostFailure::Unreadable(vec![(
                                Default::default(),
                                ddi_library::report::Finding::Other(why),
                            )]),
                            Reject::Violation(_) => unreachable!(),
                        };
                        run.told = true;
                        self.sup.fail(op, why, None);
                        self.stop_op(op)
                    }
                };
            }
            PluginMessage::Fetched { delivery, .. } if kind == Kind::Fetch => {
                let run = self.ops.remove(&op).expect("looked up above");
                self.op_ended();
                let fetch = run.fetch.expect("a fetch has its state");
                if told {
                    if let (Delivery::Cached { .. }, Deliver::CacheDir { path }) =
                        (&delivery, &fetch.deliver)
                    {
                        let _ = std::fs::remove_dir_all(path);
                    }
                    return Ok(());
                }
                return self.fetched(op, fetch, delivery);
            }
            _ => return Err(wrong()),
        };
        // The operation's end.
        self.ops.remove(&op);
        self.op_ended();
        if !told {
            self.sup.emit(ended);
        }
        Ok(())
    }

    fn op_ended(&mut self) {
        if self.ops.is_empty() {
            self.idle_since = Instant::now();
        }
    }

    /// A fetch's end: its files assembled, read as songs, and put in the
    /// store.
    fn fetched(&mut self, op: OpId, mut fetch: FetchState, delivery: Delivery) -> Result<(), Stop> {
        let incoming = fetch
            .incoming
            .take()
            .unwrap_or_else(|| Incoming::new(self.sup.store.clone()));
        let legacy = self.sup.config.legacy;
        let assembled = match incoming.finish(&delivery, &fetch.deliver, &legacy) {
            Ok(a) => a,
            Err(Reject::Violation(why)) => {
                self.sup.fail(op, HostFailure::Misbehaved(why.clone()), None);
                return Err(Stop::Misbehaved(why));
            }
            Err(other) => {
                if let Deliver::CacheDir { path } = &fetch.deliver {
                    let _ = std::fs::remove_dir_all(path);
                }
                let why = match other {
                    Reject::TooLarge => HostFailure::TooLarge,
                    Reject::Unreadable(why) => HostFailure::Unreadable(vec![(
                        Default::default(),
                        ddi_library::report::Finding::Other(why),
                    )]),
                    Reject::Violation(_) => unreachable!(),
                };
                self.sup.fail(op, why, None);
                return Ok(());
            }
        };
        let plugin = self.sup.plugin.id().to_string();
        let key = ItemKey::of(&plugin, &fetch.item, fetch.revision.as_deref());
        let files = ItemFiles(Arc::new(assembled.files.clone()));
        let pack = self.sup.plugin.manifest.name.get("en").to_string();
        match read_songs(&files, key, &pack, &legacy) {
            Ok((songs, skipped)) => {
                if let Ok(mut s) = self.sup.store.lock() {
                    s.insert(
                        key,
                        Origin {
                            plugin,
                            item: fetch.item,
                            revision: fetch.revision,
                            rooms: fetch.rooms,
                        },
                        assembled,
                        songs.clone(),
                    );
                }
                self.sup.emit(Event::Fetched {
                    op,
                    key,
                    songs,
                    skipped,
                });
            }
            Err(failed) => {
                if let Ok(mut s) = self.sup.store.lock() {
                    s.release_memory(assembled.memory);
                    s.release_disk(assembled.disk);
                }
                for dir in &assembled.owned {
                    let _ = std::fs::remove_dir_all(dir);
                }
                self.sup.fail(op, HostFailure::Unreadable(failed), None);
            }
        }
        Ok(())
    }

    /// The process's end: killed where it still runs, every operation
    /// failed with why, the plugin's strikes counted. The commands asked
    /// while it ended, for the next process.
    fn finish(mut self, why: Stop) -> Vec<Command> {
        let clean_end = matches!(why, Stop::Idle | Stop::Asked | Stop::Exited);
        self.conn.kill();
        let diagnostics = if clean_end {
            None
        } else {
            self.conn.diagnostics(DIAGNOSTICS_SETTLE)
        };
        let off = self.sup.strike(&why);
        let failure = why.failure();
        for (op, run) in std::mem::take(&mut self.ops) {
            if !run.told {
                self.sup.fail(op, failure.clone(), diagnostics.clone());
            }
            if let Some(FetchState {
                deliver: Deliver::CacheDir { path },
                ..
            }) = &run.fetch
            {
                let _ = std::fs::remove_dir_all(path);
            }
        }
        let later = std::mem::take(&mut self.later);
        let waiting = std::mem::take(&mut self.waiting);
        // Asked before a Hello that never came (or was refused): failed
        // with the rest, unless the plugin may start again.
        let mut again = Vec::new();
        for (op, request) in waiting {
            if off || why.strikes() || !clean_end {
                self.sup.fail(op, failure.clone(), diagnostics.clone());
            } else {
                again.push(Command::Op { op, request });
            }
        }
        self.sup.emit(Event::Stopped {
            plugin: self.sup.plugin.id().to_string(),
            why,
            off,
            diagnostics,
        });
        again.extend(later);
        again
    }
}

fn clean_text(text: &mut Text) {
    for value in text.0.values_mut() {
        *value = clean(value);
    }
}

fn clean_opt(text: &mut Option<String>) {
    if let Some(t) = text {
        *t = clean(t);
    }
}

/// An item's text as the game shows it: control and direction characters
/// out ([`clean`]).
fn clean_item(item: &mut ItemMeta) {
    item.title = clean(&item.title);
    for t in [
        &mut item.subtitle,
        &mut item.artist,
        &mut item.title_translit,
        &mut item.subtitle_translit,
        &mut item.artist_translit,
        &mut item.pack,
        &mut item.license,
        &mut item.attribution,
        &mut item.updated,
    ] {
        clean_opt(t);
    }
    for chart in &mut item.charts {
        clean_opt(&mut chart.name);
        clean_opt(&mut chart.author);
    }
}

fn clean_node(node: &mut Node) {
    node.title = clean(&node.title);
}
