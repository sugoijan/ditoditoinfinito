//! Write a song source plugin for DitoDitoInfinito's desktop app.
//!
//! A plugin is a native program the player installs: the game starts it
//! when it needs it and speaks to it over its standard input and output
//! ([`ddi_source_protocol`], version 1). With this crate you implement
//! what your source does, as a [`Source`] whose every method is optional,
//! and [`serve`] does the rest: the framing, the Hello, operations run at
//! once on threads of their own, cancellation, and the limits the game
//! holds you to (a message past one gets your plugin stopped, so the SDK
//! checks before it sends).
//!
//! ```no_run
//! use ddi_source_sdk::*;
//!
//! struct Mine;
//!
//! impl Source for Mine {
//!     fn info(&self) -> Info {
//!         Info::new("my-songs", "1.0.0", [Capability::Search, Capability::Fetch])
//!     }
//!     fn search(&self, cx: &Context, query: SearchQuery) -> Outcome<Page> {
//!         let _ = (cx, query);
//!         Ok(Page::default())
//!     }
//! }
//!
//! fn main() {
//!     ddi_source_sdk::main(Mine);
//! }
//! ```
//!
//! Your plugin runs with the player's access and does its own network and
//! file I/O: see the README for what the game expects of it, and the
//! courtesy owed to the sites a plugin reads.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use ddi_source_protocol as protocol;
pub use ddi_source_protocol::{
    AppInfo, Capability, ChartMeta, Choice, Deliver, Delivery, Difficulty, Dirs, FailCode,
    Filter, FilterKind, FilterValue, Input, InputKind, ItemKind, ItemMeta, Node, Reach, Step,
    Text, Unit,
};
use ddi_source_protocol::check;
use ddi_source_protocol::frame;
use ddi_source_protocol::limits::{MAX_ATTACHMENT_BYTES, MAX_DETAIL_BYTES, MAX_OPS, MAX_PAGE};
use ddi_source_protocol::{AppMessage, OpId, PluginInfo, PluginMessage, VERSIONS};

/// Who the plugin is and what it does: its manifest's id, version,
/// capabilities and inputs (or fewer), and its filters.
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub id: String,
    pub version: String,
    pub capabilities: Vec<Capability>,
    pub inputs: Vec<InputKind>,
    pub filters: Vec<Filter>,
}

impl Info {
    pub fn new(
        id: impl Into<String>,
        version: impl Into<String>,
        capabilities: impl IntoIterator<Item = Capability>,
    ) -> Info {
        Info {
            id: id.into(),
            version: version.into(),
            capabilities: capabilities.into_iter().collect(),
            ..Info::default()
        }
    }

    pub fn inputs(mut self, inputs: impl IntoIterator<Item = InputKind>) -> Info {
        self.inputs = inputs.into_iter().collect();
        self
    }

    pub fn filters(mut self, filters: impl IntoIterator<Item = Filter>) -> Info {
        self.filters = filters.into_iter().collect();
        self
    }
}

/// Why an operation failed: a code the game words in the player's
/// language, and your own words as data (at most 2 KiB, shown in a fold).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: FailCode,
    pub detail: Option<String>,
}

impl Failure {
    pub fn new(code: FailCode) -> Failure {
        Failure { code, detail: None }
    }

    pub fn with(code: FailCode, detail: impl Into<String>) -> Failure {
        Failure {
            code,
            detail: Some(detail.into()),
        }
    }

    /// What a method you did not implement answers.
    pub fn unsupported() -> Failure {
        Failure::new(FailCode::Unsupported)
    }
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Failure {
        Failure::with(FailCode::Internal, e.to_string())
    }
}

/// An operation's result.
pub type Outcome<T> = Result<T, Failure>;

/// What `Open` resolved: the scope searches and browses then name, and
/// its title.
#[derive(Clone, Debug, Default)]
pub struct Opened {
    pub scope: String,
    pub title: Option<Text>,
}

/// A page of results.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub items: Vec<ItemMeta>,
    /// The next page's cursor, if there is one.
    pub cursor: Option<String>,
    /// How many there are in all, if known.
    pub total: Option<u64>,
}

/// A page of a node's children and items.
#[derive(Clone, Debug, Default)]
pub struct BrowsePage {
    pub nodes: Vec<Node>,
    pub items: Vec<ItemMeta>,
    pub cursor: Option<String>,
    pub total: Option<u64>,
}

/// A search the game asks for.
#[derive(Clone, Debug, Default)]
pub struct SearchQuery {
    pub scope: Option<String>,
    pub query: String,
    pub filters: std::collections::BTreeMap<String, FilterValue>,
    pub cursor: Option<String>,
    /// At most this many items (and at most 50).
    pub page_size: u32,
}

/// A browse the game asks for (the scope's top without a node).
#[derive(Clone, Debug, Default)]
pub struct BrowseQuery {
    pub scope: Option<String>,
    pub node: Option<String>,
    pub cursor: Option<String>,
    pub page_size: u32,
}

/// What a source does. Every method is optional: one you leave out
/// answers [`Failure::unsupported`], and the game asks only for what your
/// [`Info`] declares.
pub trait Source: Send + Sync + 'static {
    fn info(&self) -> Info;

    fn open(&self, cx: &Context, input: Input) -> Outcome<Opened> {
        let _ = (cx, input);
        Err(Failure::unsupported())
    }

    fn search(&self, cx: &Context, query: SearchQuery) -> Outcome<Page> {
        let _ = (cx, query);
        Err(Failure::unsupported())
    }

    fn browse(&self, cx: &Context, query: BrowseQuery) -> Outcome<BrowsePage> {
        let _ = (cx, query);
        Err(Failure::unsupported())
    }

    fn describe(&self, cx: &Context, item: &str) -> Outcome<ItemMeta> {
        let _ = (cx, item);
        Err(Failure::unsupported())
    }

    /// Delivers `item`'s files through `out` ([`Fetch::file`], or a zip
    /// with [`Fetch::archive`]; into [`Fetch::cache_dir`] where the game
    /// gave one), or names where they already are on the player's disk
    /// ([`Delivery::InPlace`]).
    fn fetch(&self, cx: &Context, item: &str, out: &mut Fetch) -> Outcome<Delivery> {
        let _ = (cx, item, out);
        Err(Failure::unsupported())
    }
}

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

/// An operation's context: whether the game cancelled it, how to tell it
/// progress, and the plugin's own folders.
pub struct Context {
    op: OpId,
    out: Out,
    cancelled: Arc<AtomicBool>,
    app: AppInfo,
    dirs: Dirs,
    progress_at: Mutex<Option<Instant>>,
}

impl Context {
    /// The game cancelled the operation: end it soon (within five
    /// seconds).
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// `Err(cancelled)` once the game cancelled the operation, for `?`.
    pub fn check(&self) -> Outcome<()> {
        if self.cancelled() {
            return Err(Failure::new(FailCode::Cancelled));
        }
        Ok(())
    }

    /// The game's version and the player's language.
    pub fn app(&self) -> &AppInfo {
        &self.app
    }

    /// The plugin's own cache and data folders (made on first use by
    /// you).
    pub fn dirs(&self) -> &Dirs {
        &self.dirs
    }

    /// Tells the game how the operation is going (sent at most ten times
    /// a second; more is let go).
    pub fn progress(&self, step: Step, done: Option<u64>, total: Option<u64>, bytes: Option<u64>) {
        let mut at = self.progress_at.lock().unwrap_or_else(|e| e.into_inner());
        if at.is_some_and(|t| t.elapsed() < Duration::from_millis(100)) {
            return;
        }
        *at = Some(Instant::now());
        let total = total.filter(|&t| done.is_none_or(|d| d <= t));
        send(
            &self.out,
            &PluginMessage::Progress {
                op: self.op,
                step,
                done,
                total,
                bytes,
            },
            &[],
        );
    }
}

/// A fetch's delivery: files streamed to the game.
pub struct Fetch<'a> {
    cx: &'a Context,
    deliver: Deliver,
    files: u32,
    bytes: u64,
}

impl Fetch<'_> {
    /// What the game asked for: memory (stream with [`Fetch::file`]), or
    /// a cache folder to write the files into.
    pub fn deliver(&self) -> &Deliver {
        &self.deliver
    }

    /// The folder the game gave for this fetch, where it asked for one
    /// (the item is over its memory budget): write the files there and
    /// answer [`Fetch::cached`].
    pub fn cache_dir(&self) -> Option<&Path> {
        match &self.deliver {
            Deliver::CacheDir { path } => Some(path),
            Deliver::Memory { .. } => None,
        }
    }

    /// Streams one file of the item, `path` relative to the item
    /// (`Pack/Song/song.ssc`).
    pub fn file(&mut self, path: &str, bytes: &[u8]) -> Outcome<()> {
        self.file_from(path, bytes.len() as u64, &mut &bytes[..])
    }

    /// Streams one file of `size` bytes read from `reader`.
    pub fn file_from(&mut self, path: &str, size: u64, reader: &mut dyn Read) -> Outcome<()> {
        if let Some(p) = check::path_problem(path) {
            return Err(Failure::with(FailCode::Internal, format!("{path:?}: {p}")));
        }
        let mut buf = vec![0u8; (size.min(MAX_ATTACHMENT_BYTES as u64)) as usize];
        let mut offset = 0u64;
        loop {
            self.cx.check()?;
            let want = ((size - offset).min(buf.len() as u64)) as usize;
            reader.read_exact(&mut buf[..want])?;
            let last = offset + want as u64 == size;
            let message = PluginMessage::File {
                op: self.cx.op,
                path: path.to_string(),
                size,
                offset,
                last,
            };
            if !send(&self.cx.out, &message, &buf[..want]) {
                return Err(Failure::new(FailCode::Cancelled));
            }
            offset += want as u64;
            if last {
                break;
            }
        }
        self.files += 1;
        self.bytes += size;
        Ok(())
    }

    /// The delivery of the files streamed: return it from
    /// [`Source::fetch`].
    pub fn files(&self) -> Delivery {
        Delivery::Files {
            files: self.files,
            bytes: self.bytes,
        }
    }

    /// Streams a zip (or `.smzip`) of the item and returns its delivery.
    pub fn archive(&mut self, name: &str, zip: &[u8]) -> Outcome<Delivery> {
        self.file(name, zip)?;
        Ok(Delivery::Archive {
            path: name.to_string(),
            bytes: zip.len() as u64,
        })
    }

    /// The delivery of the files written into [`Fetch::cache_dir`],
    /// counted.
    pub fn cached(&self) -> Outcome<Delivery> {
        let dir = self
            .cache_dir()
            .ok_or_else(|| Failure::with(FailCode::Internal, "no cache folder was given"))?;
        let (mut files, mut bytes) = (0u32, 0u64);
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(&d)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    stack.push(entry.path());
                } else if kind.is_file() {
                    files += 1;
                    bytes += entry.metadata()?.len();
                }
            }
        }
        Ok(Delivery::Cached { files, bytes })
    }
}

/// Writes one frame; `false` where the game is gone.
fn send(out: &Out, message: &PluginMessage, attachment: &[u8]) -> bool {
    let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
    frame::write(&mut *out, message, attachment).is_ok()
}

/// Serves `source` on the standard input and output, and exits the
/// process when the game is done with it: a plugin's `main`.
pub fn main<S: Source>(source: S) -> ! {
    let code = match serve(source, std::io::stdin(), std::io::stdout()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("plugin: {e}");
            1
        }
    };
    std::process::exit(code)
}

/// Serves `source` on `input` and `output` until the game shuts it down
/// or goes away.
pub fn serve<S: Source>(
    source: S,
    input: impl Read + Send + 'static,
    output: impl Write + Send + 'static,
) -> std::io::Result<()> {
    let bad = |e: frame::WireError| std::io::Error::other(e.to_string());
    let source = Arc::new(source);
    let mut input = std::io::BufReader::new(input);
    let out: Out = Arc::new(Mutex::new(Box::new(std::io::BufWriter::new(output))));
    let info = source.info();
    let Some(AppMessage::Hello {
        protocols, app, dirs, ..
    }) = frame::read_app(&mut input).map_err(bad)?
    else {
        return Err(std::io::Error::other("the game did not begin with its Hello"));
    };
    let chosen = VERSIONS.iter().rev().find(|v| protocols.contains(v)).copied();
    let hello = PluginMessage::Hello {
        protocol: chosen.unwrap_or(*VERSIONS.last().expect("a version")),
        features: Vec::new(),
        plugin: PluginInfo {
            id: info.id.clone(),
            version: info.version.clone(),
        },
        capabilities: info.capabilities.clone(),
        inputs: info.inputs.clone(),
        filters: info.filters.clone(),
    };
    if let Some(p) = check::plugin_message_problem(&hello) {
        return Err(std::io::Error::other(format!("this plugin's Info: {p}")));
    }
    send(&out, &hello, &[]);
    if chosen.is_none() {
        return Err(std::io::Error::other(
            "no protocol version in common with the game",
        ));
    }
    let running: Arc<Mutex<HashMap<OpId, Arc<AtomicBool>>>> = Arc::default();
    let mut threads = Vec::new();
    loop {
        let message = match frame::read_app(&mut input) {
            Ok(Some(m)) => m,
            // The game is gone: so is the plugin.
            Ok(None) => return Ok(()),
            Err(e) => return Err(bad(e)),
        };
        let op = match &message {
            AppMessage::Shutdown {} => {
                for flag in running.lock().unwrap_or_else(|e| e.into_inner()).values() {
                    flag.store(true, Ordering::Relaxed);
                }
                let until = Instant::now() + Duration::from_secs(4);
                for t in threads.drain(..) {
                    while !std::thread::JoinHandle::is_finished(&t) && Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                send(&out, &PluginMessage::Bye {}, &[]);
                return Ok(());
            }
            AppMessage::Cancel { op } => {
                if let Some(flag) = running.lock().unwrap_or_else(|e| e.into_inner()).get(op) {
                    flag.store(true, Ordering::Relaxed);
                }
                continue;
            }
            AppMessage::Hello { .. } => {
                return Err(std::io::Error::other("a second Hello"));
            }
            AppMessage::Open { op, .. }
            | AppMessage::Search { op, .. }
            | AppMessage::Browse { op, .. }
            | AppMessage::Describe { op, .. }
            | AppMessage::Fetch { op, .. } => *op,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut r = running.lock().unwrap_or_else(|e| e.into_inner());
            if r.len() >= MAX_OPS || r.contains_key(&op) {
                drop(r);
                send(&out, &failed(op, Failure::new(FailCode::Busy)), &[]);
                continue;
            }
            r.insert(op, cancelled.clone());
        }
        threads.retain(|t: &std::thread::JoinHandle<()>| !t.is_finished());
        let cx = Context {
            op,
            out: out.clone(),
            cancelled,
            app: app.clone(),
            dirs: dirs.clone(),
            progress_at: Mutex::new(None),
        };
        let source = source.clone();
        let running = running.clone();
        threads.push(std::thread::spawn(move || {
            let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(&*source, &cx, message)
            }))
            .unwrap_or_else(|_| vec![failed(op, Failure::with(FailCode::Internal, "panicked"))]);
            running
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&op);
            for m in answer {
                send(&cx.out, &m, &[]);
            }
        }));
    }
}

fn failed(op: OpId, f: Failure) -> PluginMessage {
    let detail = f.detail.map(|mut d| {
        while d.len() > MAX_DETAIL_BYTES {
            d.pop();
        }
        d
    });
    PluginMessage::Failed {
        op,
        code: f.code,
        detail,
    }
}

/// Items the game takes: each checked, those past the limits left out
/// (and said on the diagnostics).
fn valid(items: Vec<ItemMeta>, page: usize) -> Vec<ItemMeta> {
    items
        .into_iter()
        .filter(|i| match check::item_problem(i) {
            None => true,
            Some(p) => {
                eprintln!("plugin: item {:?} left out: {p}", i.id);
                false
            }
        })
        .take(page)
        .collect()
}

/// One operation run: the messages that answer and end it.
fn run<S: Source + ?Sized>(source: &S, cx: &Context, message: AppMessage) -> Vec<PluginMessage> {
    let op = cx.op;
    let page = |p: u32| p.clamp(1, MAX_PAGE) as usize;
    let result: Outcome<Vec<PluginMessage>> = match message {
        AppMessage::Open { input, .. } => source.open(cx, input).map(|o| {
            vec![PluginMessage::Opened {
                op,
                scope: o.scope,
                title: o.title,
            }]
        }),
        AppMessage::Search {
            scope,
            query,
            filters,
            cursor,
            page_size,
            ..
        } => source
            .search(
                cx,
                SearchQuery {
                    scope,
                    query,
                    filters,
                    cursor,
                    page_size: page(page_size) as u32,
                },
            )
            .map(|p| {
                let items = valid(p.items, page(page_size));
                let mut out = Vec::new();
                if !items.is_empty() {
                    out.push(PluginMessage::Results { op, items });
                }
                out.push(PluginMessage::Done {
                    op,
                    cursor: p.cursor,
                    total: p.total,
                });
                out
            }),
        AppMessage::Browse {
            scope,
            node,
            cursor,
            page_size,
            ..
        } => source
            .browse(
                cx,
                BrowseQuery {
                    scope,
                    node,
                    cursor,
                    page_size: page(page_size) as u32,
                },
            )
            .map(|p| {
                let room = page(page_size);
                let nodes: Vec<Node> = p
                    .nodes
                    .into_iter()
                    .filter(|n| check::node_problem(n).is_none())
                    .take(room)
                    .collect();
                let items = valid(p.items, room - nodes.len());
                let mut out = Vec::new();
                if !nodes.is_empty() {
                    out.push(PluginMessage::Nodes { op, nodes });
                }
                if !items.is_empty() {
                    out.push(PluginMessage::Results { op, items });
                }
                out.push(PluginMessage::Done {
                    op,
                    cursor: p.cursor,
                    total: p.total,
                });
                out
            }),
        AppMessage::Describe { item, .. } => source.describe(cx, &item).and_then(|item| {
            match check::item_problem(&item) {
                None => Ok(vec![PluginMessage::Item { op, item }]),
                Some(p) => Err(Failure::with(FailCode::Internal, p)),
            }
        }),
        AppMessage::Fetch { item, deliver, .. } => {
            let mut out = Fetch {
                cx,
                deliver,
                files: 0,
                bytes: 0,
            };
            source
                .fetch(cx, &item, &mut out)
                .map(|delivery| vec![PluginMessage::Fetched { op, delivery }])
        }
        AppMessage::Hello { .. } | AppMessage::Cancel { .. } | AppMessage::Shutdown {} => {
            Ok(Vec::new())
        }
    };
    match result {
        Ok(messages) if cx.cancelled() && messages.is_empty() => {
            vec![failed(op, Failure::new(FailCode::Cancelled))]
        }
        Ok(messages) => messages,
        Err(f) => vec![failed(op, f)],
    }
}

/// The game's difficulty for a name sites commonly use (StepMania's,
/// DDR's and a few more), if it is one.
pub fn difficulty(name: &str) -> Option<Difficulty> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "beginner" | "novice" => Difficulty::Beginner,
        "easy" | "basic" | "light" => Difficulty::Easy,
        "medium" | "another" | "trick" | "standard" | "normal" => Difficulty::Medium,
        "hard" | "maniac" | "heavy" | "expert" => Difficulty::Hard,
        "challenge" | "oni" | "smaniac" | "extreme" => Difficulty::Challenge,
        "edit" => Difficulty::Edit,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Nothing;

    impl Source for Nothing {
        fn info(&self) -> Info {
            Info::new("nothing", "1", [Capability::Search])
        }
    }

    /// The game's side of a pipe, in memory.
    fn exchange(source: impl Source, sent: Vec<AppMessage>) -> Vec<PluginMessage> {
        let mut input = Vec::new();
        for m in &sent {
            frame::write(&mut input, m, &[]).unwrap();
        }
        let output = Arc::new(Mutex::new(Vec::new()));
        struct Shared(Arc<Mutex<Vec<u8>>>);
        impl Write for Shared {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        serve(source, std::io::Cursor::new(input), Shared(output.clone())).unwrap();
        let bytes = output.lock().unwrap().clone();
        let mut r = bytes.as_slice();
        let mut out = Vec::new();
        while let Some((m, _)) = frame::read_plugin(&mut r).unwrap() {
            out.push(m);
        }
        out
    }

    fn hello() -> AppMessage {
        AppMessage::Hello {
            protocols: vec![1],
            features: vec![],
            app: AppInfo {
                version: "0".into(),
                language: "en".into(),
            },
            dirs: Dirs {
                cache: "/tmp/c".into(),
                data: "/tmp/d".into(),
            },
        }
    }

    #[test]
    fn a_method_left_out_answers_unsupported() {
        let got = exchange(
            Nothing,
            vec![
                hello(),
                AppMessage::Search {
                    op: 1,
                    scope: None,
                    query: String::new(),
                    filters: Default::default(),
                    cursor: None,
                    page_size: 10,
                },
            ],
        );
        assert!(matches!(&got[0], PluginMessage::Hello { protocol: 1, .. }));
        // The input ends before the operation's thread answers: the
        // answer may or may not be written; when it is, it is a value.
        if let Some(m) = got.get(1) {
            assert_eq!(
                *m,
                PluginMessage::Failed {
                    op: 1,
                    code: FailCode::Unsupported,
                    detail: None
                }
            );
        }
    }

    #[test]
    fn a_game_of_another_version_is_answered_with_ours() {
        let mut h = hello();
        if let AppMessage::Hello { protocols, .. } = &mut h {
            *protocols = vec![7];
        }
        let mut input = Vec::new();
        frame::write(&mut input, &h, &[]).unwrap();
        let out = std::io::Cursor::new(Vec::new());
        assert!(serve(Nothing, std::io::Cursor::new(input), out).is_err());
    }

    #[test]
    fn site_difficulty_names_map_to_ours() {
        assert_eq!(difficulty("Oni"), Some(Difficulty::Challenge));
        assert_eq!(difficulty(" basic "), Some(Difficulty::Easy));
        assert_eq!(difficulty("mystery"), None);
    }
}
