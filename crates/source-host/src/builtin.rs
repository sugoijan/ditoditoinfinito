//! Sources built into the app (`docs/plans/source-plugins.md`, section
//! 5, "This computer"): the game's own code, run in process on a thread
//! an operation, behind the same requests and events as a plugin's
//! ([`crate::Host::request`], [`crate::Event`]). The host treats one as it
//! treats a plugin switched on: at most [`MAX_OPS`] operations at once,
//! a cancel failing the operation at once, nothing new started while the
//! game is quiet ([`crate::Host::set_quiet`]), and what it fetches read by
//! the library's scanner into the store, where a play finds it.
//!
//! Unlike a plugin's, a built-in source's answers are not checked against
//! the protocol's limits: it is the game's own code. What it fetches is
//! still read as song data from outside.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ddi_library::path::RelPathBuf;
use ddi_source_protocol::check::Accepted;
use ddi_source_protocol::limits::MAX_OPS;
use ddi_source_protocol::{FailCode, FilterValue, Input, ItemMeta, Node, OpId, Step, Text};

use crate::host::{Event, Failure, HostConfig, HostFailure, Request};
use crate::store::{Assembled, ItemKey, Origin, SharedStore, Stored};

/// Why a built-in source's operation failed: a code the game words, and
/// what is worth knowing (data, for a Details fold).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failed {
    pub code: FailCode,
    pub detail: Option<String>,
}

impl Failed {
    pub fn new(code: FailCode) -> Failed {
        Failed { code, detail: None }
    }

    pub fn with(code: FailCode, detail: impl Into<String>) -> Failed {
        Failed {
            code,
            detail: Some(detail.into()),
        }
    }
}

/// What a built-in source's operation answers.
pub type Answer<T> = Result<T, Failed>;

/// An `Open`'s scope and its title.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Opened {
    pub scope: String,
    pub title: Option<Text>,
}

/// A page of a search's results.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub items: Vec<ItemMeta>,
    pub cursor: Option<String>,
    pub total: Option<u64>,
}

/// A page of a browse: a node's children and its items.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrowsePage {
    pub nodes: Vec<Node>,
    pub items: Vec<ItemMeta>,
    pub cursor: Option<String>,
    pub total: Option<u64>,
}

/// What a built-in fetch hands over: an item's files by the paths the
/// library's scanner reads them by (`Pack/Song/song.ssc`), each where it
/// is (the player's own file, read in place) or its bytes; and the pack
/// of songs at its root.
#[derive(Debug, Default)]
pub struct Delivered {
    pub files: BTreeMap<RelPathBuf, Stored>,
    pub pack: String,
}

/// A search asked of a built-in source.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Search {
    pub scope: Option<String>,
    pub query: String,
    pub filters: BTreeMap<String, FilterValue>,
    pub cursor: Option<String>,
    pub page_size: u32,
}

/// A browse asked of a built-in source (the scope's top without a node).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Browse {
    pub scope: Option<String>,
    pub node: Option<String>,
    pub cursor: Option<String>,
    pub page_size: u32,
}

/// A source built into the app. Every method has an answer: one it does
/// not offer is [`FailCode::Unsupported`], and the host asks only what
/// [`Builtin::accepted`] declares.
pub trait Builtin: Send + Sync + 'static {
    /// Its id: never a plugin's (an installed plugin of the same id is
    /// passed over).
    fn id(&self) -> &str;

    /// What it does, as a plugin's Hello says it.
    fn accepted(&self) -> Accepted;

    /// How far its songs may go, as a plugin's manifest says it (an
    /// item's may lower it): everything, unless it says otherwise.
    fn reach(&self) -> ddi_source_protocol::Reach {
        ddi_source_protocol::Reach::Keep
    }

    fn open(&self, cx: &Context, input: Input) -> Answer<Opened> {
        let _ = (cx, input);
        Err(Failed::new(FailCode::Unsupported))
    }

    fn search(&self, cx: &Context, search: Search) -> Answer<Page> {
        let _ = (cx, search);
        Err(Failed::new(FailCode::Unsupported))
    }

    fn browse(&self, cx: &Context, browse: Browse) -> Answer<BrowsePage> {
        let _ = (cx, browse);
        Err(Failed::new(FailCode::Unsupported))
    }

    fn describe(&self, cx: &Context, item: &str) -> Answer<ItemMeta> {
        let _ = (cx, item);
        Err(Failed::new(FailCode::Unsupported))
    }

    fn fetch(&self, cx: &Context, item: &str) -> Answer<Delivered> {
        let _ = (cx, item);
        Err(Failed::new(FailCode::Unsupported))
    }
}

/// An operation's end, told once: by its thread, or by a cancel first.
type Ended = Arc<AtomicBool>;

/// Each running operation's cancel and end.
type Ops = Arc<Mutex<BTreeMap<OpId, (Arc<AtomicBool>, Ended)>>>;

/// What a built-in operation's thread may do besides answering: see
/// whether it was cancelled, say how far it is, and wait while the game
/// is quiet (a play).
pub struct Context {
    op: OpId,
    cancelled: Arc<AtomicBool>,
    quiet: Arc<AtomicBool>,
    events: Sender<Event>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Context {
    pub fn op(&self) -> OpId {
        self.op
    }

    /// The operation was cancelled: its answer is no longer wanted.
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// `Err(cancelled)` once it was cancelled; else waits while the game
    /// is quiet (a play: nothing of a source's runs beside it), then
    /// `Ok`. Called between steps of work.
    pub fn check(&self) -> Answer<()> {
        while self.quiet.load(Ordering::Relaxed) && !self.cancelled() {
            std::thread::sleep(QUIET_POLL);
        }
        if self.cancelled() {
            return Err(Failed::new(FailCode::Cancelled));
        }
        Ok(())
    }

    /// How far it is (the screens word `step`).
    pub fn progress(&self, step: Step, done: Option<u64>, total: Option<u64>) {
        if self.cancelled() {
            return;
        }
        let _ = self.events.send(Event::Progress {
            op: self.op,
            step,
            done,
            total,
            bytes: None,
        });
        if let Some(w) = &self.wake {
            w();
        }
    }
}

/// How often a paused operation looks whether the game is quiet still.
const QUIET_POLL: Duration = Duration::from_millis(50);

/// A built-in source's operations at once.
#[derive(Clone)]
pub(crate) struct Running {
    source: Arc<dyn Builtin>,
    ops: Ops,
}

impl Running {
    pub(crate) fn new(source: Arc<dyn Builtin>) -> Running {
        Running {
            source,
            ops: Arc::default(),
        }
    }

    pub(crate) fn source(&self) -> &Arc<dyn Builtin> {
        &self.source
    }
}

/// What a built-in operation's thread needs of the host.
pub(crate) struct Spawn {
    pub(crate) config: Arc<HostConfig>,
    pub(crate) store: SharedStore,
    pub(crate) events: Sender<Event>,
    pub(crate) wake: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) quiet: Arc<AtomicBool>,
}

impl Spawn {
    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        if let Some(w) = &self.wake {
            w();
        }
    }

    fn fail(&self, op: OpId, failure: Failure) {
        self.emit(Event::Failed {
            op,
            failure,
            diagnostics: None,
        });
    }
}

/// Starts `request` as operation `op` of `running`'s source on a thread
/// of its own; or fails it (busy, unsupported) at once.
pub(crate) fn start(running: &Running, spawn: Spawn, op: OpId, request: Request) {
    let source = running.source.clone();
    if !source.accepted().capabilities.contains(&request.capability()) {
        spawn.fail(op, Failure::Host(HostFailure::Unsupported));
        return;
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let ended: Ended = Arc::default();
    {
        let Ok(mut ops) = running.ops.lock() else {
            spawn.fail(op, Failure::Host(HostFailure::Stopped));
            return;
        };
        if ops.len() >= MAX_OPS {
            drop(ops);
            spawn.fail(op, Failure::Host(HostFailure::Busy));
            return;
        }
        ops.insert(op, (cancelled.clone(), ended.clone()));
    }
    let ops = running.ops.clone();
    let (events, wake) = (spawn.events.clone(), spawn.wake.clone());
    let cx = Context {
        op,
        cancelled,
        quiet: spawn.quiet.clone(),
        events: spawn.events.clone(),
        wake: spawn.wake.clone(),
    };
    let thread = std::thread::Builder::new()
        .name(format!("ddi-source-{}", source.id()))
        .spawn(move || {
            let events = answer(&*source, &cx, &spawn, request);
            if let Ok(mut o) = ops.lock() {
                o.remove(&op);
            }
            // Told once: a cancel may have ended it first.
            if !ended.swap(true, Ordering::SeqCst) {
                for e in events {
                    spawn.emit(e);
                }
            }
        });
    if let Err(e) = thread {
        if let Ok(mut o) = running.ops.lock() {
            o.remove(&op);
        }
        let _ = events.send(Event::Failed {
            op,
            failure: Failure::Host(HostFailure::Start(e.to_string())),
            diagnostics: None,
        });
        if let Some(w) = wake {
            w();
        }
    }
}

/// Cancels `op` where `running` runs it: it fails as cancelled at once,
/// and its thread's answer is dropped. Whether it ran there.
pub(crate) fn cancel(running: &Running, op: OpId, events: &Sender<Event>) -> bool {
    let Some((cancelled, ended)) = running.ops.lock().ok().and_then(|o| o.get(&op).cloned())
    else {
        return false;
    };
    cancelled.store(true, Ordering::Relaxed);
    if !ended.swap(true, Ordering::SeqCst) {
        let _ = events.send(Event::Failed {
            op,
            failure: Failure::Host(HostFailure::Cancelled),
            diagnostics: None,
        });
    }
    true
}

/// The events that answer `request`, its end last.
fn answer(source: &dyn Builtin, cx: &Context, spawn: &Spawn, request: Request) -> Vec<Event> {
    let op = cx.op;
    let failed = |f: Failed| Event::Failed {
        op,
        failure: Failure::Plugin {
            code: f.code,
            detail: f.detail,
        },
        diagnostics: None,
    };
    if let Err(f) = cx.check() {
        return vec![failed(f)];
    }
    match request {
        Request::Open(input) => match source.open(cx, input) {
            Ok(o) => vec![Event::Opened {
                op,
                scope: o.scope,
                title: o.title,
            }],
            Err(f) => vec![failed(f)],
        },
        Request::Search {
            scope,
            query,
            filters,
            cursor,
            page_size,
        } => {
            let search = Search {
                scope,
                query,
                filters,
                cursor,
                page_size: page_size.clamp(1, ddi_source_protocol::limits::MAX_PAGE),
            };
            match source.search(cx, search) {
                Ok(page) => vec![
                    Event::Results {
                        op,
                        items: page.items,
                    },
                    Event::Done {
                        op,
                        cursor: page.cursor,
                        total: page.total,
                    },
                ],
                Err(f) => vec![failed(f)],
            }
        }
        Request::Browse {
            scope,
            node,
            cursor,
            page_size,
        } => {
            let browse = Browse {
                scope,
                node,
                cursor,
                page_size: page_size.clamp(1, ddi_source_protocol::limits::MAX_PAGE),
            };
            match source.browse(cx, browse) {
                Ok(page) => {
                    let mut out = Vec::new();
                    if !page.nodes.is_empty() {
                        out.push(Event::Nodes {
                            op,
                            nodes: page.nodes,
                        });
                    }
                    if !page.items.is_empty() {
                        out.push(Event::Results {
                            op,
                            items: page.items,
                        });
                    }
                    out.push(Event::Done {
                        op,
                        cursor: page.cursor,
                        total: page.total,
                    });
                    out
                }
                Err(f) => vec![failed(f)],
            }
        }
        Request::Describe { item } => match source.describe(cx, &item) {
            Ok(item) => vec![Event::Item { op, item }],
            Err(f) => vec![failed(f)],
        },
        Request::Fetch {
            item,
            revision,
            reach,
            ..
        } => {
            let delivered = match source.fetch(cx, &item) {
                Ok(d) => d,
                Err(f) => return vec![failed(f)],
            };
            if cx.cancelled() {
                return vec![failed(Failed::new(FailCode::Cancelled))];
            }
            vec![fetched(
                source,
                spawn,
                op,
                &item,
                revision,
                source.reach().narrowed(reach),
                delivered,
            )]
        }
    }
}

/// A built-in fetch's files read as songs and put in the store.
fn fetched(
    source: &dyn Builtin,
    spawn: &Spawn,
    op: OpId,
    item: &str,
    revision: Option<String>,
    reach: ddi_source_protocol::Reach,
    delivered: Delivered,
) -> Event {
    let memory: u64 = delivered
        .files
        .values()
        .map(|s| match s {
            Stored::Memory(b) => b.len() as u64,
            Stored::Disk(_) => 0,
        })
        .sum();
    let limits = spawn.store.lock().map(|s| s.limits()).unwrap_or_default();
    if memory > limits.item_memory || !crate::store::reserve_memory(&spawn.store, memory) {
        return Event::Failed {
            op,
            failure: Failure::Host(HostFailure::TooLarge),
            diagnostics: None,
        };
    }
    let key = ItemKey::of(source.id(), item, revision.as_deref());
    let files = crate::store::ItemFiles(Arc::new(delivered.files));
    let legacy = spawn.config.legacy;
    let pack = if delivered.pack.is_empty() {
        source.id().to_string()
    } else {
        delivered.pack
    };
    match crate::assemble::read_songs(&files, key, &pack, &legacy, spawn.config.more) {
        Ok((songs, skipped)) => {
            if let Ok(mut s) = spawn.store.lock() {
                s.insert(
                    key,
                    Origin {
                        plugin: source.id().to_string(),
                        item: item.to_string(),
                        revision,
                        reach,
                    },
                    Assembled {
                        files: (*files.0).clone(),
                        memory,
                        disk: 0,
                        owned: Vec::new(),
                    },
                    songs.clone(),
                );
            }
            Event::Fetched {
                op,
                key,
                songs,
                skipped,
            }
        }
        Err(failed) => {
            if let Ok(mut s) = spawn.store.lock() {
                s.release_memory(memory);
            }
            Event::Failed {
                op,
                failure: Failure::Host(HostFailure::Unreadable(failed)),
                diagnostics: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::atomic::AtomicU32;
    use std::time::Instant;

    use ddi_library::path::RelPath;
    use ddi_library::scan::Reader as _;
    use ddi_source_protocol::{Capability, InputKind, Reach};

    use super::*;
    use crate::store::{Store, StoreLimits};
    use crate::{Host, HostConfig};

    const SSC: &str = "#TITLE:Built In;#ARTIST:Someone;#MUSIC:song.ogg;#BPMS:0=120;\n#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:5;#NOTES:\n1000\n0100\n0010\n0001\n;\n";

    /// A source of one song, its fetch waiting while `hold` is set.
    #[derive(Default)]
    struct Fake {
        hold: Arc<AtomicBool>,
        fetches: AtomicU32,
    }

    impl Builtin for Fake {
        fn id(&self) -> &str {
            "fake"
        }

        fn accepted(&self) -> Accepted {
            Accepted {
                protocol: 1,
                capabilities: BTreeSet::from([Capability::Search, Capability::Fetch]),
                inputs: BTreeSet::from([InputKind::Folder]),
                filters: Vec::new(),
            }
        }

        fn search(&self, _cx: &Context, search: Search) -> Answer<Page> {
            Ok(Page {
                items: vec![ItemMeta {
                    id: "one".into(),
                    title: format!("Found {}", search.query),
                    ..ItemMeta::default()
                }],
                cursor: None,
                total: Some(1),
            })
        }

        fn fetch(&self, cx: &Context, item: &str) -> Answer<Delivered> {
            self.fetches.fetch_add(1, Ordering::Relaxed);
            while self.hold.load(Ordering::Relaxed) {
                cx.check()?;
                std::thread::sleep(Duration::from_millis(5));
            }
            cx.check()?;
            if item != "one" {
                return Err(Failed::new(FailCode::NotFound));
            }
            let mut files = BTreeMap::new();
            files.insert(
                RelPathBuf::from("Pack/Song/song.ssc"),
                Stored::Memory(SSC.as_bytes().into()),
            );
            files.insert(
                RelPathBuf::from("Pack/Song/song.ogg"),
                Stored::Memory(b"OggS".as_slice().into()),
            );
            Ok(Delivered {
                files,
                pack: "Fake".into(),
            })
        }
    }

    fn host(name: &str) -> (Host, Arc<Fake>) {
        let root = std::env::temp_dir().join(format!("ddi-builtin-{name}-{}", std::process::id()));
        let store = Store::open(root.join("sources"), StoreLimits::default()).shared();
        let config = HostConfig::new(root.join("plugins"), root.join("plugin-data"));
        let mut host = Host::new(config, Vec::new(), &BTreeSet::new(), store, None);
        let fake = Arc::new(Fake::default());
        host.add_builtin(fake.clone());
        (host, fake)
    }

    std::thread_local! {
        /// Events of other operations taken while waiting for one.
        static BACKLOG: std::cell::RefCell<Vec<Event>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// `op`'s events, to its end (others' kept for their turn).
    fn until_end(host: &mut Host, op: OpId) -> Vec<Event> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut out: Vec<Event> = BACKLOG.with(|b| {
            let mut b = b.borrow_mut();
            let (mine, rest) = b.drain(..).partition(|e: &Event| event_op(e) == Some(op));
            *b = rest;
            mine
        });
        if out.last().is_some_and(|e| e.ends() == Some(op)) {
            return out;
        }
        while Instant::now() < deadline {
            if let Some(e) = host.wait_event(Duration::from_millis(20)) {
                if event_op(&e) != Some(op) {
                    BACKLOG.with(|b| b.borrow_mut().push(e));
                    continue;
                }
                let end = e.ends() == Some(op);
                out.push(e);
                if end {
                    return out;
                }
            }
        }
        panic!("operation {op} did not end: {out:?}");
    }

    fn event_op(e: &Event) -> Option<OpId> {
        match e {
            Event::Progress { op, .. } | Event::Results { op, .. } | Event::Nodes { op, .. } => {
                Some(*op)
            }
            other => other.ends(),
        }
    }

    fn search(query: &str) -> Request {
        Request::Search {
            scope: None,
            query: query.into(),
            filters: BTreeMap::new(),
            cursor: None,
            page_size: 10,
        }
    }

    fn fetch(item: &str) -> Request {
        Request::Fetch {
            item: item.into(),
            revision: None,
            size: None,
            reach: None,
        }
    }

    #[test]
    fn a_builtin_answers_as_a_plugin_and_its_songs_are_in_the_store() {
        let (mut host, _) = host("answers");
        let op = host.request("fake", search("x"));
        let events = until_end(&mut host, op);
        assert!(matches!(&events[0], Event::Results { items, .. } if items[0].title == "Found x"));
        assert!(matches!(&events[1], Event::Done { total: Some(1), .. }));
        let op = host.request("fake", fetch("one"));
        let Some(Event::Fetched { songs, key, .. }) = until_end(&mut host, op).pop() else {
            panic!("not fetched");
        };
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].entry.title, "Built In");
        assert_eq!(songs[0].pack, "Pack");
        let (song, files) = host.store().lock().unwrap().song(&songs[0].id).unwrap();
        assert_eq!(song.key, key);
        assert_eq!(files.read(RelPath::new("Pack/Song/song.ogg")).unwrap(), b"OggS");
        assert_eq!(host.store().lock().unwrap().usage().memory, SSC.len() as u64 + 4);
        // Its level: everything, unless the item lowers it.
        let reach = |host: &Host, key| host.store().lock().unwrap().origin(key).map(|o| o.reach);
        assert_eq!(reach(&host, key), Some(Reach::Keep));
        let op = host.request(
            "fake",
            Request::Fetch {
                item: "one".into(),
                revision: Some("r2".into()),
                size: None,
                reach: Some(Reach::Rooms),
            },
        );
        let Some(Event::Fetched { key, .. }) = until_end(&mut host, op).pop() else {
            panic!("not fetched");
        };
        assert_eq!(reach(&host, key), Some(Reach::Rooms));
        // Not offered: browse; not found: another item.
        let op = host.request(
            "fake",
            Request::Browse {
                scope: None,
                node: None,
                cursor: None,
                page_size: 10,
            },
        );
        assert!(matches!(
            until_end(&mut host, op).pop(),
            Some(Event::Failed { failure: Failure::Host(HostFailure::Unsupported), .. })
        ));
        let op = host.request("fake", fetch("two"));
        assert!(matches!(
            until_end(&mut host, op).pop(),
            Some(Event::Failed {
                failure: Failure::Plugin { code: FailCode::NotFound, .. },
                ..
            })
        ));
    }

    #[test]
    fn a_cancel_ends_it_at_once_and_its_answer_is_dropped() {
        let (mut host, fake) = host("cancel");
        fake.hold.store(true, Ordering::Relaxed);
        let op = host.request("fake", fetch("one"));
        while fake.fetches.load(Ordering::Relaxed) == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
        host.cancel(op);
        let events = until_end(&mut host, op);
        assert!(matches!(
            events.last(),
            Some(Event::Failed { failure: Failure::Host(HostFailure::Cancelled), .. })
        ));
        fake.hold.store(false, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(100));
        assert!(host.try_event().is_none(), "nothing after its end");
        assert_eq!(host.store().lock().unwrap().usage().items, 0);
    }

    #[test]
    fn past_four_at_once_it_is_busy() {
        let (mut host, fake) = host("busy");
        fake.hold.store(true, Ordering::Relaxed);
        let held: Vec<OpId> = (0..4).map(|_| host.request("fake", fetch("one"))).collect();
        while fake.fetches.load(Ordering::Relaxed) < 4 {
            std::thread::sleep(Duration::from_millis(5));
        }
        let op = host.request("fake", fetch("one"));
        assert!(matches!(
            until_end(&mut host, op).pop(),
            Some(Event::Failed { failure: Failure::Host(HostFailure::Busy), .. })
        ));
        fake.hold.store(false, Ordering::Relaxed);
        for op in held {
            assert!(matches!(until_end(&mut host, op).pop(), Some(Event::Fetched { .. })));
        }
    }

    /// Nothing new starts while the game is quiet (a play); what ran
    /// waits at its next step, and both go on once it is not.
    #[test]
    fn while_quiet_nothing_starts_and_running_work_waits() {
        let (mut host, fake) = host("quiet");
        fake.hold.store(true, Ordering::Relaxed);
        let running = host.request("fake", fetch("one"));
        while fake.fetches.load(Ordering::Relaxed) == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
        host.set_quiet(true);
        fake.hold.store(false, Ordering::Relaxed);
        let asked = host.request("fake", search("later"));
        assert_eq!(host.queued(), 1);
        std::thread::sleep(Duration::from_millis(200));
        assert!(host.try_event().is_none(), "the running fetch waits, the search is kept");
        assert_eq!(fake.fetches.load(Ordering::Relaxed), 1);
        // A request kept and cancelled ends at once.
        let dropped = host.request("fake", search("dropped"));
        host.cancel(dropped);
        assert!(matches!(
            until_end(&mut host, dropped).pop(),
            Some(Event::Failed { failure: Failure::Host(HostFailure::Cancelled), .. })
        ));
        host.set_quiet(false);
        assert_eq!(host.queued(), 0);
        assert!(matches!(until_end(&mut host, running).pop(), Some(Event::Fetched { .. })));
        assert!(matches!(until_end(&mut host, asked).last(), Some(Event::Done { .. })));
    }
}
