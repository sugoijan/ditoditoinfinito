//! The announcer studio on desktop (`docs/plans/announcer-studio.md`,
//! step 5): the theme's page (`ddi_ui::screens::studio`) as the web has
//! it, over Options or by `--screen announcer`.
//!
//! - **The draft** (`ddi_library::announcer::draft`) is kept as it
//!   changes in the data folder's `studio/`: `draft.json`, each recording
//!   as it came under `take/<id>`, written on a thread of their own.
//! - **Recordings** come from each line's Add recording and Advanced's
//!   imports of a pack made before (a zip or a folder), read on a thread;
//!   there is no drop, as desktop's importer has none.
//! - **Decoded** at 48 kHz on a thread, measured for the tidy, and played
//!   on song select's sound, tidied as it will go in the pack.
//! - **Export** asks where with a save dialog and writes the zip, what
//!   is written anew encoded as Opus. **Use
//!   in the game** writes the announcer into the data folder's
//!   `announcers/`, listed beside the song folders' from then on.
//! - **Hear it in the game** is the shared scenes'
//!   (`ddi_session::announcer::scene::director`), with the draft as the
//!   voice or our pack of the examples chosen; a line said in play goes
//!   on to [`crate::play::host`], which shows the real play in the frame.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use ddi_l10n::UiText;
use ddi_l10n::msg::studio as ms;
use ddi_library::announcer::draft::{
    Draft, Export, HEAD, Sound as Read, Take, Write, readme_credits, readme_path, takes,
};
use ddi_library::announcer::examples::Set;
use ddi_library::announcer::tidy::{self, Measure};
use ddi_library::announcer::{Bundled, Line, MAX_BYTES, Spoken, find_announcers};
use ddi_library::path::RelPath;
use ddi_library::scan::Reader;
use ddi_platform::SoundId;
use ddi_platform::storage::{self, Area};
use ddi_session::announcer::scene::director::{
    self, Director, Heard, Said, SceneHost, StagedVoice, Timer, UiFn, Voiced,
};
use ddi_session::announcer::scene::{ResultsScene, in_play};
use ddi_session::announcer::{SharedVoice, Voice};
use ddi_session::preview::LEAD;
use ddi_ui::event::{MenuEvent, UiEvent};
use ddi_ui::message::Message;
use ddi_ui::screens::Env;
use ddi_ui::screens::data::Platform;
use ddi_ui::screens::errors::error_text;
use ddi_ui::screens::page::{PAGE_CHANGED, PageCall, PageDoc, PageFeed, PageScreen, PageShell};
use ddi_ui::screens::studio::{self, Act, Example, View};
use ddi_ui::state::settings::Settings;
use ddi_ui::{Cx, MenuAction, Ui, UiRequest};

use crate::audio::Sound;
use crate::audio::mixer::Pcm;
use crate::library::announcers::DiskAnnouncer;
use crate::preview::SharedPlayer;

/// The rate takes are decoded, measured and written at.
const RATE: u32 = 48_000;
/// Where the example set chosen is remembered.
const EXAMPLES_KEY: &str = "ddi.studio.examples";
/// The draft is saved at most this often while takes are decoded,
/// milliseconds.
const SAVE_EVERY_MS: f64 = 1000.0;
/// How soon the loop looks again while something is under way.
const SOON: Duration = Duration::from_millis(16);

/// Where the studio keeps what it keeps: the data folder's `studio/`.
fn folder() -> Option<PathBuf> {
    Some(crate::paths::data_dir()?.join("studio"))
}

fn take_path(dir: &Path, id: u64) -> PathBuf {
    dir.join("take").join(id.to_string())
}

/// A decoded sound's channels.
fn planar(sound: &Pcm) -> Vec<Vec<f32>> {
    let channels = usize::from(sound.channels.max(1));
    let mut out = vec![Vec::with_capacity(sound.frames()); channels];
    for frame in sound.data.chunks_exact(channels) {
        for (c, v) in frame.iter().enumerate() {
            out[c].push(*v);
        }
    }
    out
}

/// A sound from its channels at `rate` (kept as it is: the mixer steps
/// through a sound of another rate than the output's).
fn sound_of(channels: Vec<Vec<f32>>, rate: u32) -> Sound {
    let frames = channels.first().map_or(0, Vec::len);
    let count = channels.len().max(1);
    let mut data = Vec::with_capacity(frames * count);
    for f in 0..frames {
        for c in &channels {
            data.push(c.get(f).copied().unwrap_or(0.0));
        }
    }
    Arc::new(Pcm {
        rate,
        channels: count as u16,
        data,
    })
}

/// `sound` as the pack has it: tidied by `measure`, when it is.
fn shaped(sound: &Sound, measure: Option<&Measure>) -> Sound {
    let Some(m) = measure else {
        return sound.clone();
    };
    sound_of(tidy::apply(&planar(sound), sound.rate, m), sound.rate)
}

/// What the store's thread writes.
enum Job {
    Take(u64, Arc<Vec<u8>>),
    Remove(u64),
    Draft(String),
    Flush(Sender<()>),
}

/// The recordings and the draft on disk, written in order on a thread:
/// what was asked for is written before it goes ([`Drop`]).
struct Store {
    dir: PathBuf,
    tx: Option<Sender<Job>>,
    thread: Option<std::thread::JoinHandle<()>>,
    errors: Receiver<String>,
}

impl Store {
    fn open(dir: PathBuf) -> Store {
        let (tx, rx) = mpsc::channel::<Job>();
        let (fail, errors) = mpsc::channel();
        let at = dir.clone();
        let thread = std::thread::Builder::new()
            .name("ddi-studio-store".into())
            .spawn(move || {
                for job in rx {
                    let done = match job {
                        Job::Take(id, bytes) => {
                            crate::paths::write_whole(&take_path(&at, id), &bytes)
                        }
                        Job::Remove(id) => match std::fs::remove_file(take_path(&at, id)) {
                            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                            _ => Ok(()),
                        },
                        Job::Draft(json) => {
                            crate::paths::write_whole(&at.join("draft.json"), json.as_bytes())
                        }
                        Job::Flush(done) => {
                            let _ = done.send(());
                            Ok(())
                        }
                    };
                    if let Err(e) = done {
                        let _ = fail.send(e.to_string());
                    }
                }
            })
            .ok();
        Store {
            dir,
            tx: Some(tx),
            thread,
            errors,
        }
    }

    fn send(&self, job: Job) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(job);
        }
    }

    /// Waits until what was asked for so far is written.
    fn flush(&self) {
        let (tx, rx) = mpsc::channel();
        self.send(Job::Flush(tx));
        let _ = rx.recv_timeout(Duration::from_secs(10));
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Where a take's bytes to decode are.
enum Bytes {
    Memory(Arc<Vec<u8>>),
    File(PathBuf),
}

struct DecodeJob {
    id: u64,
    from: Bytes,
    extension: Option<String>,
}

/// A take decoded, or why not.
type DecodeResult = (u64, Result<Sound, String>);

/// The takes decoded, one after another, on a thread.
fn decoder() -> (Sender<DecodeJob>, Receiver<DecodeResult>) {
    let (jobs, rx) = mpsc::channel::<DecodeJob>();
    let (tx, results) = mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("ddi-studio-decode".into())
        .spawn(move || {
            for job in rx {
                let bytes = match job.from {
                    Bytes::Memory(b) => Ok(Arc::try_unwrap(b).unwrap_or_else(|b| (*b).clone())),
                    Bytes::File(p) => std::fs::read(p).map_err(|e| e.to_string()),
                };
                let sound = bytes.and_then(|b| {
                    crate::audio::decoder::sound(
                        b,
                        job.extension.as_deref(),
                        Default::default(),
                        RATE,
                    )
                });
                if tx.send((job.id, sound)).is_err() {
                    return;
                }
            }
        });
    (jobs, results)
}

/// What the files picked read as, on their thread.
enum Arrived {
    /// A pack made before: its name and credits.
    Pack {
        name: Option<String>,
        by: Option<String>,
        license: Option<String>,
    },
    File {
        path: String,
        size: u64,
        bytes: Arc<Vec<u8>>,
    },
    Done {
        passed: usize,
        failed: Option<String>,
    },
}

/// Picked files being read: what goes on which line.
struct Adding {
    rx: Receiver<Arrived>,
    line: Option<Line>,
    added: usize,
}

/// A file picked: where it reads from.
enum Source {
    File(PathBuf),
    Zip(usize, ddi_library::path::RelPathBuf),
}

/// Lists and reads what was picked (`files`, on a thread): zips opened,
/// folders walked, what is no recording passed over, each named by its
/// path in what was picked.
fn read_picked(picked: Vec<PathBuf>, tx: Sender<Arrived>) {
    use crate::library::files::{self, ZipReader};
    let mut listed: Vec<(String, u64, Source)> = Vec::new();
    let mut zips: Vec<ZipReader> = Vec::new();
    let mut failed = None;
    let name_of = |p: &Path| {
        p.file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    };
    for p in picked {
        let name = name_of(&p);
        if p.is_dir() {
            for f in files::walk(&p).listed {
                if let Some(path) = f.path.to_path(&p) {
                    listed.push((format!("{name}/{}", f.path), f.size, Source::File(path)));
                }
            }
        } else if let Some(stem) = ddi_library::zip::archive_stem(&name) {
            match ZipReader::open(&p, &crate::library::legacy_text) {
                Ok(zip) => {
                    let i = zips.len();
                    for n in &zip.names {
                        listed.push((
                            format!("{stem}/{n}"),
                            zip.size(n).unwrap_or(0),
                            Source::Zip(i, n.clone()),
                        ));
                    }
                    zips.push(zip);
                }
                Err(e) => failed = Some(e),
            }
        } else {
            let size = std::fs::metadata(&p).map_or(0, |m| m.len());
            listed.push((name, size, Source::File(p)));
        }
    }
    let read = |s: &Source| match s {
        Source::File(p) => std::fs::read(p).map_err(|e| e.to_string()),
        Source::Zip(i, n) => zips[*i].read(n),
    };
    let head = |s: &Source| match s {
        Source::File(p) => {
            use std::io::Read as _;
            let mut buffer = vec![0; HEAD];
            let n = std::fs::File::open(p)
                .and_then(|mut f| f.read(&mut buffer))
                .unwrap_or(0);
            buffer.truncate(n);
            buffer
        }
        Source::Zip(i, n) => zips[*i].read_at(n, 0, HEAD).unwrap_or_default(),
    };
    // A pack made before: its name, and the credits its README gives.
    let names: Vec<(String, u64)> = listed.iter().map(|(p, s, _)| (p.clone(), *s)).collect();
    let found = find_announcers(&names, "");
    let readme = match found.announcers.as_slice() {
        [_] => readme_path(names.iter().map(|(p, _)| p.as_str())).map(str::to_string),
        _ => None,
    };
    if let [a] = found.announcers.as_slice() {
        let (by, license) = readme
            .as_deref()
            .and_then(|r| listed.iter().find(|(p, ..)| p == r))
            .and_then(|(_, _, s)| read(s).ok())
            .map(|b| readme_credits(&String::from_utf8_lossy(&b)))
            .unwrap_or_default();
        let _ = tx.send(Arrived::Pack {
            name: Some(a.name.clone()),
            by,
            license,
        });
    }
    let mut passed = 0;
    for (path, size, source) in &listed {
        if !takes(path, &head(source)) || *size > MAX_BYTES {
            if readme.as_deref() != Some(path.as_str()) {
                passed += 1;
            }
            continue;
        }
        match read(source) {
            Ok(bytes) => {
                let sent = tx.send(Arrived::File {
                    path: path.clone(),
                    size: *size,
                    bytes: Arc::new(bytes),
                });
                // The page went: no more.
                if sent.is_err() {
                    return;
                }
            }
            Err(e) => failed = Some(e),
        }
    }
    let _ = tx.send(Arrived::Done { passed, failed });
}

/// Where a pack goes.
enum Packing {
    /// A zip at this path (from the save dialog).
    Zip(PathBuf),
    /// The data folder's announcers, to choose under Options.
    Game,
}

/// A pack's entry, for its thread.
struct PackEntry {
    path: String,
    take: u64,
    write: Write,
    measure: Option<Measure>,
    extension: Option<String>,
    decoded: Option<Sound>,
}

/// What a pack made says: the zip's file name and how many sounds, or
/// the announcer's name.
enum Packed {
    Zip(String),
    Game(String),
}

/// A pack's entry's bytes.
fn entry_bytes(dir: &Path, e: PackEntry) -> Result<(String, Vec<u8>), String> {
    let file = take_path(dir, e.take);
    let read = |f: &Path| std::fs::read(f).map_err(|x| format!("{}: {x}", e.path));
    let bytes = match e.write {
        Write::AsIs => read(&file)?,
        Write::Opus { tidy: tidied } | Write::Wav { tidy: tidied } => {
            let sound = match e.decoded {
                Some(s) => s,
                None => crate::audio::decoder::sound(
                    read(&file)?,
                    e.extension.as_deref(),
                    Default::default(),
                    RATE,
                )?,
            };
            let measure = e.measure.filter(|_| tidied);
            let shaped = shaped(&sound, measure.as_ref());
            let pcm = planar(&shaped);
            if matches!(e.write, Write::Opus { .. }) {
                ddi_platform::opus::encode(&pcm, shaped.rate)
                    .map_err(|x| format!("{}: {x}", e.path))?
            } else {
                ddi_library::wav::write(&pcm, shaped.rate)
            }
        }
    };
    Ok((e.path, bytes))
}

/// Writes the announcer whose files are `files` into `shelf` (its
/// folder there replaced), its readme beside its lines: its name.
fn into_game(shelf: &Path, files: Vec<(String, Vec<u8>)>, readme: &str) -> Result<String, String> {
    let listed: Vec<(String, u64)> = files
        .iter()
        .map(|(p, b)| (p.clone(), b.len() as u64))
        .collect();
    let a = find_announcers(&listed, "root")
        .announcers
        .into_iter()
        .next()
        .ok_or("it has no lines yet")?;
    let kept = a.kept();
    let prefix = format!("{}/", a.dir);
    let folder = a.dir.file_name().to_string();
    let place = shelf.join(&folder);
    // Whole or not at all: beside it, then in its place.
    let partial = shelf.join(format!(".{folder}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_dir_all(&partial);
    let written = (|| -> Result<(), String> {
        for (path, bytes) in &files {
            let rest = path.strip_prefix(&prefix).unwrap_or(path);
            let to = RelPath::new(rest)
                .to_path(&partial)
                .ok_or_else(|| format!("{rest}: not a file's path"))?;
            crate::paths::write_whole(&to, bytes).map_err(|e| e.to_string())?;
        }
        crate::paths::write_whole(&partial.join("README.txt"), readme.as_bytes())
            .map_err(|e| e.to_string())?;
        if place.exists() {
            std::fs::remove_dir_all(&place).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&partial, &place).map_err(|e| e.to_string())
    })();
    if written.is_err() {
        let _ = std::fs::remove_dir_all(&partial);
    }
    written.map(|()| kept.name)
}

/// A pack made on a thread.
fn make_pack(
    dir: PathBuf,
    export: Export,
    entries: Vec<PackEntry>,
    to: Packing,
) -> Result<Packed, String> {
    let mut files = Vec::with_capacity(entries.len() + 1);
    for e in entries {
        files.push(entry_bytes(&dir, e)?);
    }
    match to {
        Packing::Zip(path) => {
            files.push(("README.txt".to_string(), export.readme.into_bytes()));
            let zip = ddi_library::zip::write_stored(&files).ok_or("the pack is too large")?;
            crate::paths::write_whole(&path, &zip).map_err(|e| e.to_string())?;
            Ok(Packed::Zip(
                path.file_name()
                    .map_or(export.file_name, |n| n.to_string_lossy().into_owned()),
            ))
        }
        Packing::Game => {
            let shelf = crate::library::announcers::made_dir().ok_or("no data folder")?;
            std::fs::create_dir_all(&shelf).map_err(|e| e.to_string())?;
            let name = into_game(&shelf, files, &export.readme)?;
            crate::library::announcers::forget();
            Ok(Packed::Game(name))
        }
    }
}

/// A timer's handle: dropped, it is off.
struct Handle(Rc<Cell<bool>>);

impl Drop for Handle {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// A timer waiting: when (the studio's clock), whether it is still on,
/// and what it does.
type Pending = (f64, Rc<Cell<bool>>, Box<dyn FnOnce()>);

/// Our pack's sounds read and decoded, by key.
type ExampleSounds = Result<Vec<(String, Sound)>, String>;
/// An example being read, to play, and its key.
type ExampleRead = (Receiver<Result<Sound, String>>, String);

/// What a scene's voice waits for.
enum WaitFor {
    /// The draft's recordings of these lines to be decoded.
    Draft(Box<dyn Fn(Line) -> bool>),
    /// Our pack's sounds, read and decoded on a thread.
    Example {
        rx: Receiver<ExampleSounds>,
        /// Each line, its sound's key and its words.
        plan: Vec<(Line, String, Option<String>)>,
    },
}

/// What is done with a scene's voice once it is ready.
type Then = Box<dyn FnOnce(&Rc<Studio>, Result<Voiced<Sound>, String>)>;

/// A scene waiting for its voice.
struct Waiting {
    n: u64,
    wait: WaitFor,
    then: Then,
}

/// The page, its draft and its work under way.
pub struct Studio {
    draft: RefCell<Draft>,
    view: RefCell<View>,
    env: Env,
    sound: SharedPlayer,
    /// A play asked for (a line said in play's).
    play: Rc<RefCell<Option<crate::play::Request>>>,
    feed: PageFeed,
    store: Store,
    /// Their sound, once decoded (at [`RATE`]).
    decoded: RefCell<HashMap<u64, Sound>>,
    decode: Sender<DecodeJob>,
    results: Receiver<DecodeResult>,
    /// The takes being decoded.
    decoding: RefCell<BTreeSet<u64>>,
    adding: RefCell<Vec<Adding>>,
    packing: RefCell<Option<Receiver<Result<Packed, String>>>>,
    /// An example being read, to play, and what it is.
    example: RefCell<Option<ExampleRead>>,
    /// Ours, each with where its sounds are.
    ours: RefCell<Vec<(Bundled, DiskAnnouncer)>>,
    examples: RefCell<BTreeMap<Line, Vec<Example>>>,
    /// Our packs' sounds a scene took, decoded (by pack and path).
    example_sounds: RefCell<HashMap<String, Sound>>,
    director: Rc<Director<Scenes>>,
    waiting: RefCell<Option<Waiting>>,
    /// What the last scene in play said (the play's).
    heard_in_play: Rc<RefCell<Option<Heard>>>,
    /// The last scene's line, what it said and who.
    heard: RefCell<Option<(Line, Vec<String>, String)>>,
    /// What plays now, on the preview's sound.
    playing: Cell<Option<SoundId>>,
    /// Ends what plays.
    playing_end: RefCell<Option<Timer>>,
    /// The studio's clock, milliseconds (tests move it on).
    clock: RefCell<Box<dyn Fn() -> f64>>,
    timers: RefCell<Vec<Pending>>,
    /// What a scene asks of the UI and presses, for the next poll.
    todo: RefCell<Vec<UiFn>>,
    presses: RefCell<Vec<MenuAction>>,
    /// The page changed (shown again at the next poll).
    dirty: Cell<bool>,
    /// The draft is to be saved, and when it last was.
    unsaved: Cell<bool>,
    saved_at: Cell<f64>,
    /// Told an announcer was added (Options lists them again).
    used: Option<Rc<dyn Fn()>>,
    /// The page was left: nothing more is asked of it.
    left: Cell<bool>,
}

impl Studio {
    /// The studio on `sound`, its draft opened from the data folder (none
    /// where there is no such folder); a line said in play asks `play`;
    /// `used` is told an announcer was added.
    pub fn new(
        env: &Env,
        sound: SharedPlayer,
        play: Rc<RefCell<Option<crate::play::Request>>>,
        used: Option<Rc<dyn Fn()>>,
    ) -> Rc<Studio> {
        let dir = folder().unwrap_or_else(|| std::env::temp_dir().join("ddi-studio"));
        let (draft, status) = open_draft(&dir);
        let examples = storage::get(Area::Local, EXAMPLES_KEY)
            .and_then(|k| Set::from_key(&k))
            .unwrap_or_else(studio::first_set);
        let (decode, results) = decoder();
        let studio = Rc::new_cyclic(|me: &Weak<Studio>| Studio {
            draft: RefCell::new(draft),
            view: RefCell::new(View {
                examples,
                status,
                platform: Platform::Desktop,
                ..View::default()
            }),
            env: env.clone(),
            sound,
            play,
            feed: PageFeed::default(),
            store: Store::open(dir),
            decoded: RefCell::default(),
            decode,
            results,
            decoding: RefCell::default(),
            adding: RefCell::default(),
            packing: RefCell::default(),
            example: RefCell::default(),
            ours: RefCell::new(ours()),
            examples: RefCell::default(),
            example_sounds: RefCell::default(),
            director: Director::new(Scenes(me.clone())),
            waiting: RefCell::default(),
            heard_in_play: Rc::default(),
            heard: RefCell::default(),
            playing: Cell::default(),
            playing_end: RefCell::default(),
            clock: RefCell::new(Box::new(crate::play::epoch_ms)),
            timers: RefCell::default(),
            todo: RefCell::default(),
            presses: RefCell::default(),
            dirty: Cell::new(false),
            unsaved: Cell::new(false),
            saved_at: Cell::new(0.0),
            used,
            left: Cell::new(false),
        });
        studio.refresh_examples();
        studio.decode_pending();
        studio
    }

    /// The page to push.
    pub fn screen(self: &Rc<Self>) -> PageScreen {
        let shell: PageShell = {
            let weak = Rc::downgrade(self);
            Rc::new(move |call, cx| weak.upgrade()?.call(call, cx))
        };
        PageScreen::new(self.env.clone(), studio::NAME, self.doc())
            .with_shell(shell, self.feed.clone())
    }

    /// The studio's clock, for tests.
    pub fn set_clock(&self, clock: impl Fn() -> f64 + 'static) {
        *self.clock.borrow_mut() = Box::new(clock);
    }

    /// The page was left.
    pub fn left(&self) -> bool {
        self.left.get()
    }

    /// The draft as it is.
    pub fn draft(&self) -> Draft {
        self.draft.borrow().clone()
    }

    /// What the page says it is doing, or did.
    pub fn status(&self) -> String {
        self.view.borrow().status.clone()
    }

    /// What plays (`take:<id>`, `example:<folder>:<i>`).
    pub fn playing(&self) -> Option<String> {
        self.view.borrow().playing.clone()
    }

    /// The scene showing.
    pub fn scene(&self) -> Option<director::SceneState> {
        self.director.state()
    }

    /// The last scene's line, what it said, and who.
    pub fn last_heard(&self) -> Option<(Line, Vec<String>, String)> {
        self.heard.borrow().clone()
    }

    /// Waits until what was asked to be stored is written (tests).
    pub fn flush(&self) {
        self.store.flush();
    }

    /// Whether something is under way that a poll will take further.
    pub fn working(&self) -> bool {
        !self.decoding.borrow().is_empty()
            || !self.adding.borrow().is_empty()
            || self.packing.borrow().is_some()
            || self.example.borrow().is_some()
            || self.waiting.borrow().is_some()
    }

    fn now(&self) -> f64 {
        (self.clock.borrow())()
    }

    fn doc(&self) -> PageDoc {
        studio::doc(
            &self.draft.borrow(),
            &self.view.borrow(),
            &self.examples.borrow(),
        )
    }

    fn set_status(&self, text: impl Into<String>) {
        self.view.borrow_mut().status = text.into();
        self.dirty.set(true);
    }

    /// The draft kept.
    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&*self.draft.borrow()) {
            self.store.send(Job::Draft(json));
        }
        self.unsaved.set(false);
        self.saved_at.set(self.now());
    }

    fn pack_of(&self, set: Set) -> Option<(Bundled, DiskAnnouncer)> {
        self.ours
            .borrow()
            .iter()
            .find(|(b, _)| b.kept.name == set.pack())
            .cloned()
    }

    fn refresh_examples(&self) {
        let set = self.view.borrow().examples;
        let ours = self.pack_of(set).map(|(b, _)| b);
        *self.examples.borrow_mut() = studio::examples(set, ours.as_ref());
    }

    /// Picked files added: read on a thread, each on `line` when given
    /// (else by its name).
    fn add(&self, picked: Vec<PathBuf>, line: Option<Line>) {
        self.set_status(ms::status_adding(picked.len()));
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("ddi-studio-read".into())
            .spawn(move || read_picked(picked, tx));
        self.adding.borrow_mut().push(Adding { rx, line, added: 0 });
    }

    fn extension_of(t: &Take) -> Option<String> {
        t.format
            .extension()
            .map(str::to_string)
            .or_else(|| RelPath::new(&t.source).extension().map(str::to_string))
    }

    /// Take `id` decoded from `from`.
    fn decode_take(&self, id: u64, from: Bytes) {
        let extension = self.draft.borrow().take(id).and_then(Self::extension_of);
        if self.decoding.borrow_mut().insert(id) {
            let _ = self.decode.send(DecodeJob {
                id,
                from,
                extension,
            });
        }
    }

    /// The takes not read yet (an import stopped halfway), decoded.
    fn decode_pending(&self) {
        let ids: Vec<u64> = self
            .draft
            .borrow()
            .takes
            .iter()
            .filter(|t| matches!(t.sound, Read::Unread))
            .map(|t| t.id)
            .collect();
        for id in ids {
            self.decode_file(id);
        }
    }

    fn decode_file(&self, id: u64) {
        self.decode_take(id, Bytes::File(take_path(&self.store.dir, id)));
    }

    fn measure_of(&self, id: u64) -> Option<Measure> {
        let d = self.draft.borrow();
        let t = d.take(id)?;
        d.tidies(t).then(|| t.measure().copied()).flatten()
    }

    fn stop(&self) {
        if let Some(id) = self.playing.take() {
            self.sound.borrow_mut().audio.stop_at(id, None);
        }
        self.playing_end.borrow_mut().take();
        self.example.borrow_mut().take();
        self.view.borrow_mut().playing = None;
        self.dirty.set(true);
    }

    /// `sound` played, at the announcer's share of the music volume.
    fn speak(self: &Rc<Self>, key: String, sound: &Sound) {
        let settings = Settings::load();
        let id = {
            let mut p = self.sound.borrow_mut();
            p.set_volume(settings.volume);
            let at = p.audio.context_time() + LEAD;
            p.audio
                .play_voice_sound(sound, at, settings.announcer_volume)
        };
        self.playing.set(Some(id));
        self.view.borrow_mut().playing = Some(key);
        self.dirty.set(true);
        let weak = Rc::downgrade(self);
        let timer = Scenes(weak.clone()).timer(
            sound.duration() + 0.15,
            Box::new(move || {
                if let Some(s) = weak.upgrade() {
                    s.stop();
                }
            }),
        );
        *self.playing_end.borrow_mut() = Some(timer);
    }

    /// Take `id` played as it will be in the pack (again: stopped).
    fn play_take(self: &Rc<Self>, id: u64) {
        let key = format!("take:{id}");
        if self.view.borrow().playing.as_deref() == Some(&key) {
            return self.stop();
        }
        self.stop();
        let Some(sound) = self.decoded.borrow().get(&id).cloned() else {
            // Decoded first (the page opened again); pressed again then.
            self.decode_file(id);
            return;
        };
        let m = self.measure_of(id);
        self.speak(key, &shaped(&sound, m.as_ref()));
    }

    /// Our example `i` of `line` played.
    fn play_example(self: &Rc<Self>, line: Line, i: usize) {
        let key = format!("example:{}:{i}", line.folder());
        if self.view.borrow().playing.as_deref() == Some(&key) {
            return self.stop();
        }
        self.stop();
        let path = self
            .examples
            .borrow()
            .get(&line)
            .and_then(|e| e.get(i))
            .and_then(|e| e.sound.clone());
        let pack = self.pack_of(self.view.borrow().examples);
        let (Some(path), Some((_, a))) = (path, pack) else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("ddi-studio-example".into())
            .spawn(move || {
                let _ = tx.send(read_example(&a, &path));
            });
        *self.example.borrow_mut() = Some((rx, key));
    }

    /// The pack's entries, for its thread.
    fn pack_entries(&self, export: &Export) -> Vec<PackEntry> {
        let draft = self.draft.borrow();
        let decoded = self.decoded.borrow();
        export
            .entries
            .iter()
            .map(|e| {
                let t = draft.take(e.take);
                PackEntry {
                    path: e.path.clone(),
                    take: e.take,
                    write: e.write,
                    measure: t.and_then(|t| t.measure().copied()),
                    extension: t.and_then(Self::extension_of),
                    decoded: decoded.get(&e.take).cloned(),
                }
            })
            .collect()
    }

    /// The pack made and put where it goes, on a thread.
    fn pack(&self, to: Packing) {
        let export = self.draft.borrow().export();
        let entries = self.pack_entries(&export);
        let dir = self.store.dir.clone();
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("ddi-studio-pack".into())
            .spawn(move || {
                let _ = tx.send(make_pack(dir, export, entries, to));
            });
        *self.packing.borrow_mut() = Some(rx);
        self.view.borrow_mut().busy = true;
    }

    fn remove(&self, id: u64) {
        self.draft.borrow_mut().remove(id);
        self.decoded.borrow_mut().remove(&id);
        self.store.send(Job::Remove(id));
        self.save();
    }

    /// What `f` keeps of the draft's lines, decoded or being: those not
    /// asked for yet (the page opened again) are, here.
    fn ask_lines(&self, which: &dyn Fn(Line) -> bool) {
        let ids: Vec<u64> = self
            .draft
            .borrow()
            .takes
            .iter()
            .filter(|t| t.line.is_some_and(which) && matches!(t.sound, Read::Read(_)))
            .map(|t| t.id)
            .filter(|id| !self.decoded.borrow().contains_key(id))
            .collect();
        for id in ids {
            self.decode_file(id);
        }
    }

    /// Whether every take of the lines `which` keeps is decoded (or could
    /// not be).
    fn lines_ready(&self, which: &dyn Fn(Line) -> bool) -> bool {
        let decoding = self.decoding.borrow();
        !self
            .draft
            .borrow()
            .takes
            .iter()
            .any(|t| t.line.is_some_and(which) && decoding.contains(&t.id))
    }

    /// The draft as a voice for the lines `which` keeps.
    fn staged_voice(&self, which: impl Fn(Line) -> bool) -> Voiced<Sound> {
        let draft = self.draft.borrow();
        director::draft_voice(
            &draft,
            which,
            &self.director.turns.borrow(),
            &self.decoded.borrow(),
            |t, s| {
                let m = draft.tidies(t).then(|| t.measure().copied()).flatten();
                Some(shaped(s, m.as_ref()))
            },
            |s| s.duration(),
        )
    }

    /// What our pack of `set` says for `lines`: one sound of each (as the
    /// first of its chain the pack has), the line's next each scene, read
    /// and decoded once a session.
    fn example_wait(&self, set: Set, lines: &[Line]) -> Result<WaitFor, String> {
        let (pack, a) = self
            .pack_of(set)
            .ok_or_else(|| format!("{} is not here", set.pack()))?;
        let mut plan = Vec::new();
        let mut seen = BTreeSet::new();
        let mut need = Vec::new();
        for &line in lines {
            let Some((found, files)) = line.chain().into_iter().find_map(|l| {
                let files = pack.kept.lines.get(&l.folder())?;
                (!files.is_empty()).then(|| (l, files.clone()))
            }) else {
                continue;
            };
            if !seen.insert(found) {
                continue;
            }
            let turn = self.director.example_turn(set, found);
            let path = files[turn % files.len()].to_string();
            let key = format!("{}/{path}", pack.path);
            if !self.example_sounds.borrow().contains_key(&key) {
                need.push((key.clone(), path.clone()));
            }
            plan.push((found, key, pack.said.get(&path).cloned()));
        }
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("ddi-studio-examples".into())
            .spawn(move || {
                let read: ExampleSounds = need
                    .into_iter()
                    .map(|(key, path)| Ok((key, read_example(&a, &path)?)))
                    .collect();
                let _ = tx.send(read);
            });
        Ok(WaitFor::Example { rx, plan })
    }

    /// `then` with scene `n`'s voice: the draft's lines `which` keeps
    /// once they are decoded, or our pack's `lines`.
    fn with_voice(
        self: &Rc<Self>,
        n: u64,
        said: Said,
        which: impl Fn(Line) -> bool + 'static,
        lines: &[Line],
        then: impl FnOnce(&Rc<Studio>, Voiced<Sound>) + 'static,
    ) {
        let wait = match said {
            Said::Draft => {
                self.ask_lines(&which);
                WaitFor::Draft(Box::new(which))
            }
            Said::Example(set) => match self.example_wait(set, lines) {
                Ok(w) => w,
                Err(e) => {
                    return self
                        .director
                        .give_up(n, ms::status_example_unreadable(error_text(&e)));
                }
            },
        };
        let then: Then = Box::new(move |s, voiced| match voiced {
            Ok(v) => then(s, v),
            Err(e) => s
                .director
                .give_up(n, ms::status_example_unreadable(error_text(&e))),
        });
        *self.waiting.borrow_mut() = Some(Waiting { n, wait, then });
    }

    /// Hears `line` in the game: its scene, said by the draft where it
    /// has something to say for the line, else by our pack of the
    /// examples chosen.
    fn hear(self: &Rc<Self>, line: Line) {
        if self.director.busy() {
            return;
        }
        self.stop();
        let said = if studio::recorded(line, &self.draft.borrow()) {
            Said::Draft
        } else {
            Said::Example(self.view.borrow().examples)
        };
        match line.spoken() {
            Spoken::Menu => self.menu_scene(line, said),
            Spoken::Results => self.results_scene(line, said),
            Spoken::Play => self.play_scene(line, said),
        }
    }

    fn menu_scene(self: &Rc<Self>, line: Line, said: Said) {
        let Some((n, scene)) = self.director.begin_menu(line, said) else {
            return;
        };
        let which = |l: Line| l.spoken() == Spoken::Menu;
        let menu: Vec<Line> = Line::all().into_iter().filter(|l| which(*l)).collect();
        self.with_voice(n, said, which, &menu, move |s, voiced| {
            s.director.show_menu(n, scene, voiced);
        });
    }

    fn results_scene(self: &Rc<Self>, line: Line, said: Said) {
        let recorded: BTreeSet<Line> = self
            .draft
            .borrow()
            .takes
            .iter()
            .filter_map(|t| t.line)
            .collect();
        let Some((n, scene)) = self.director.begin_results(line, said, &recorded) else {
            return;
        };
        let lines = director::results_said_lines(&scene);
        let which = |l: Line| l.spoken() == Spoken::Results;
        self.with_voice(n, said, which, &lines, move |s, voiced| {
            s.director.show_results(n, scene, voiced);
        });
    }

    /// A line said in play: the real play, staged on its chart, its voice
    /// the draft's play and results lines or our pack's.
    fn play_scene(self: &Rc<Self>, line: Line, said: Said) {
        let Some(pick) = in_play::pick(line) else {
            return;
        };
        let chart = crate::library::bundled()
            .iter()
            .find(|e| e.id == pick.song)
            .and_then(|e| {
                e.charts
                    .iter()
                    .find(|c| c.layout == in_play::STYLE && c.difficulty == pick.difficulty.name())
                    .map(|c| c.index)
            });
        let Some(chart) = chart else {
            return self.set_status(ms::status_no_play());
        };
        let n = self.director.begin_play(line, said);
        let which = |l: Line| matches!(l.spoken(), Spoken::Play | Spoken::Results);
        let lines: Vec<Line> = Line::all().into_iter().filter(|l| which(*l)).collect();
        self.with_voice(n, said, which, &lines, move |s, voiced| {
            let (voice, lengths, words) = voiced;
            let heard = s.heard_in_play.clone();
            let order = crate::play::SceneOrder {
                line,
                said,
                voice_label: said.label().to_string(),
                name: studio::name(line).to_string(),
                voice: Rc::new(RefCell::new(voice)) as SharedVoice<Sound>,
                lengths,
                words,
                heard: Rc::new(move |h| *heard.borrow_mut() = Some(h)),
            };
            s.director.handed_over(n);
            *s.play.borrow_mut() = Some(crate::play::Request::scene(pick.song, chart, order));
        });
    }

    /// A press on the page, inside its handler.
    fn call(self: &Rc<Self>, call: PageCall, cx: &mut Cx) -> Option<PageDoc> {
        let id = match call {
            PageCall::Leave => {
                self.stop();
                self.left.set(true);
                cx.close(true);
                return None;
            }
            PageCall::Field { id, text } => {
                if studio::set_field(&mut self.draft.borrow_mut(), &id, &text) {
                    self.save();
                }
                return None;
            }
            PageCall::Button(id) => id,
        };
        match studio::act(&id)? {
            Act::Move(id) => {
                // Heard as its line is chosen.
                if self.view.borrow().playing.as_deref() != Some(&format!("take:{id}")) {
                    self.play_take(id);
                }
                let weak = Rc::downgrade(self);
                let picker = studio::line_picker(
                    self.env.clone(),
                    &self.draft.borrow(),
                    id,
                    Rc::new(move |id, line, _cx| {
                        let Some(s) = weak.upgrade() else {
                            return;
                        };
                        s.set_line(id, line);
                    }),
                )?;
                cx.overlay(picker, true);
                None
            }
            act => self.act(act),
        }
    }

    /// Take `id` put on `line` (`None`: unsorted).
    pub fn set_line(&self, id: u64, line: Option<Line>) {
        if let Some(t) = self.draft.borrow_mut().take_mut(id) {
            t.line = line;
        }
        self.stop();
        self.save();
        self.dirty.set(true);
    }

    /// What a press asks: the page again where it shows at once.
    pub fn act(self: &Rc<Self>, act: Act) -> Option<PageDoc> {
        match act {
            Act::Add(line) => {
                let picked = crate::os::pick_recordings();
                if !picked.is_empty() {
                    self.add(picked, Some(line));
                }
                None
            }
            Act::ImportFiles => {
                let picked = crate::os::pick(false);
                if !picked.is_empty() {
                    self.add(picked, None);
                }
                None
            }
            Act::ImportFolder => {
                let picked = crate::os::pick(true);
                if !picked.is_empty() {
                    self.add(picked, None);
                }
                None
            }
            Act::Play(id) => {
                self.play_take(id);
                Some(self.doc())
            }
            Act::Example(line, i) => {
                self.play_example(line, i);
                Some(self.doc())
            }
            Act::Tidy => {
                {
                    let mut d = self.draft.borrow_mut();
                    d.tidy = !d.tidy;
                    for t in &mut d.takes {
                        t.tidy = None;
                    }
                }
                self.save();
                Some(self.doc())
            }
            // (The dialog's: its caller's.)
            Act::Move(_) => None,
            Act::Remove(id) => {
                self.remove(id);
                Some(self.doc())
            }
            Act::Examples => {
                let next = studio::next_set(self.view.borrow().examples);
                self.view.borrow_mut().examples = next;
                let _ = storage::set(Area::Local, EXAMPLES_KEY, next.key());
                self.refresh_examples();
                Some(self.doc())
            }
            Act::StepMania => {
                {
                    let mut d = self.draft.borrow_mut();
                    d.stepmania = !d.stepmania;
                }
                self.save();
                Some(self.doc())
            }
            Act::Details => {
                {
                    let mut v = self.view.borrow_mut();
                    v.details = !v.details;
                }
                Some(self.doc())
            }
            Act::Export => {
                let name = self.draft.borrow().export().file_name;
                if let Some(path) = crate::os::save_zip(&name) {
                    self.view.borrow_mut().status = ms::status_exporting().into();
                    self.pack(Packing::Zip(path));
                    return Some(self.doc());
                }
                None
            }
            // Export asks where in its own press.
            Act::Save => None,
            Act::UseInGame => {
                self.view.borrow_mut().status = ms::status_using().into();
                self.pack(Packing::Game);
                Some(self.doc())
            }
            Act::Hear(line) => {
                self.hear(line);
                None
            }
        }
    }

    /// What was picked, as it is read.
    fn poll_adding(&self) {
        let mut adding = self.adding.borrow_mut();
        adding.retain_mut(|a| {
            loop {
                match a.rx.try_recv() {
                    Ok(Arrived::Pack { name, by, license }) => {
                        let mut d = self.draft.borrow_mut();
                        if let Some(n) = name.filter(|_| d.name.trim().is_empty()) {
                            d.name = n;
                        }
                        if let Some(b) = by.filter(|_| d.author.trim().is_empty()) {
                            d.author = b;
                        }
                        if let Some(l) = license.filter(|_| d.license.trim().is_empty()) {
                            d.license = l;
                        }
                    }
                    Ok(Arrived::File { path, size, bytes }) => {
                        let id = {
                            let mut d = self.draft.borrow_mut();
                            let head = &bytes[..bytes.len().min(HEAD)];
                            let id = d.add(&path, size, head);
                            if let Some(l) = a.line
                                && let Some(t) = d.take_mut(id)
                            {
                                t.line = Some(l);
                            }
                            id
                        };
                        self.store.send(Job::Take(id, bytes.clone()));
                        self.decode_take(id, Bytes::Memory(bytes));
                        a.added += 1;
                        self.unsaved.set(true);
                        self.dirty.set(true);
                    }
                    Ok(Arrived::Done { passed, failed }) => {
                        self.save();
                        self.set_status(match failed {
                            Some(e) if a.added == 0 => ms::status_files_unreadable(error_text(&e)),
                            _ => ms::status_added(a.added, passed),
                        });
                        return false;
                    }
                    Err(TryRecvError::Empty) => return true,
                    Err(TryRecvError::Disconnected) => return false,
                }
            }
        });
    }

    /// The takes decoded since.
    fn poll_decoded(&self) {
        for (id, result) in self.results.try_iter() {
            self.decoding.borrow_mut().remove(&id);
            let was_read = {
                let d = self.draft.borrow();
                matches!(d.take(id).map(|t| &t.sound), Some(Read::Read(_)))
            };
            let sound = match result {
                Ok(s) => {
                    let m = tidy::measure(&planar(&s), s.rate);
                    self.decoded.borrow_mut().insert(id, s);
                    Read::Read(m)
                }
                Err(e) => Read::Unreadable(e),
            };
            // A take read before keeps what it measured.
            if !was_read && let Some(t) = self.draft.borrow_mut().take_mut(id) {
                t.sound = sound;
                self.unsaved.set(true);
            }
            self.dirty.set(true);
        }
        if self.unsaved.get()
            && (self.decoding.borrow().is_empty()
                || self.now() - self.saved_at.get() >= SAVE_EVERY_MS)
        {
            self.save();
        }
    }

    fn poll_pack(&self) {
        let done = match &*self.packing.borrow() {
            Some(rx) => match rx.try_recv() {
                Ok(done) => done,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => Err("it stopped".to_string()),
            },
            None => return,
        };
        self.packing.borrow_mut().take();
        self.view.borrow_mut().busy = false;
        match done {
            Ok(Packed::Zip(name)) => {
                self.set_status(ms::status_saved(&name, Platform::Desktop.l10n()))
            }
            Ok(Packed::Game(name)) => {
                self.set_status(ms::status_used(name));
                if let Some(used) = &self.used {
                    used();
                }
            }
            Err(e) => {
                let e = error_text(&e);
                let exporting = self.view.borrow().status == ms::status_exporting().to_string();
                self.set_status(if exporting {
                    ms::status_not_exported(e)
                } else {
                    ms::status_not_used(e)
                });
            }
        }
    }

    fn poll_example(self: &Rc<Self>) {
        let done = match &*self.example.borrow() {
            Some((rx, _)) => match rx.try_recv() {
                Ok(done) => done,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => Err("it stopped".to_string()),
            },
            None => return,
        };
        let Some((_, key)) = self.example.borrow_mut().take() else {
            return;
        };
        match done {
            Ok(sound) => self.speak(key, &sound),
            Err(e) => self.set_status(ms::status_example_unplayable(error_text(&e))),
        }
    }

    /// A scene's voice, once it is ready.
    fn poll_waiting(self: &Rc<Self>) {
        let ready = match &*self.waiting.borrow() {
            Some(w) if !self.director.is(w.n) => None,
            Some(w) => match &w.wait {
                WaitFor::Draft(which) => self.lines_ready(which).then_some(Ok(Vec::new())),
                WaitFor::Example { rx, .. } => match rx.try_recv() {
                    Ok(done) => Some(done),
                    Err(TryRecvError::Empty) => return,
                    Err(TryRecvError::Disconnected) => Some(Err("it stopped".to_string())),
                },
            },
            None => return,
        };
        let Some(sounds) = ready else {
            // Its scene left while it waited (or its lines are decoding).
            if self
                .waiting
                .borrow()
                .as_ref()
                .is_some_and(|w| !self.director.is(w.n))
            {
                self.waiting.borrow_mut().take();
            }
            return;
        };
        let Some(w) = self.waiting.borrow_mut().take() else {
            return;
        };
        let voiced = sounds.map(|read| {
            self.example_sounds.borrow_mut().extend(read);
            match w.wait {
                WaitFor::Draft(which) => self.staged_voice(which),
                WaitFor::Example { plan, .. } => {
                    let mut voice: Voice<Sound> = Voice::default();
                    let mut lengths = HashMap::new();
                    let mut words = HashMap::new();
                    let sounds = self.example_sounds.borrow();
                    for (line, key, said) in plan {
                        let Some(sound) = sounds.get(&key) else {
                            continue;
                        };
                        voice.expect([line]);
                        lengths.insert(line, sound.duration());
                        voice.add(line, sound.clone());
                        if let Some(said) = said {
                            words.insert(line, said);
                        }
                    }
                    (voice, lengths, words)
                }
            }
        });
        (w.then)(self, voiced);
    }

    /// Moves things on, once per turn of the loop: what the threads
    /// brought, the timers due, what a scene asks of `ui`. What the UI
    /// asked meanwhile, and when to look again.
    pub fn poll(self: &Rc<Self>, ui: &mut Ui) -> (Vec<UiRequest>, Option<Instant>) {
        self.poll_adding();
        self.poll_decoded();
        self.poll_pack();
        self.poll_example();
        self.poll_waiting();
        for e in self.store.errors.try_iter() {
            eprintln!("announcer studio: keeping the draft: {e}");
            self.set_status(ms::status_added_unkept(
                0,
                0,
                error_text(&e),
                Platform::Desktop.l10n(),
            ));
        }
        if let Some(h) = self.heard_in_play.borrow_mut().take() {
            self.director.heard(h);
        }
        let now = self.now();
        let due: Vec<Pending> = {
            let mut timers = self.timers.borrow_mut();
            let (due, later): (Vec<_>, Vec<_>) = timers.drain(..).partition(|(at, ..)| *at <= now);
            *timers = later;
            due
        };
        for (_, on, f) in due {
            if on.get() {
                f();
            }
        }
        let mut requests = Vec::new();
        for f in self.todo.borrow_mut().drain(..) {
            f(ui);
        }
        for action in self.presses.borrow_mut().drain(..).collect::<Vec<_>>() {
            for down in [true, false] {
                let out = ui.handle(UiEvent::Menu(MenuEvent {
                    action,
                    down,
                    source: u32::MAX,
                    gesture: false,
                    time: 0.0,
                }));
                requests.extend(out.requests);
            }
        }
        // Timers and presses may have asked for more of the UI.
        for f in self.todo.borrow_mut().drain(..) {
            f(ui);
        }
        if self.dirty.take() {
            *self.feed.borrow_mut() = Some(self.doc());
            ui.broadcast(Message::new(PAGE_CHANGED));
        }
        let timer = self
            .timers
            .borrow()
            .iter()
            .filter(|(_, on, _)| on.get())
            .map(|(at, ..)| *at)
            .reduce(f64::min)
            .map(|at| Instant::now() + Duration::from_secs_f64(((at - now) / 1000.0).max(0.0)));
        let soon = self.working().then(|| Instant::now() + SOON);
        (requests, timer.into_iter().chain(soon).min())
    }
}

/// Reads and decodes sound `path` of our pack `a`.
fn read_example(a: &DiskAnnouncer, path: &str) -> Result<Sound, String> {
    let sound = RelPath::new(path);
    let bytes = a.reader()?.read(&a.name_in_reader(sound))?;
    crate::audio::decoder::sound(bytes, sound.extension(), Default::default(), RATE)
}

/// Ours, each with what its sounds say (the bundled index's, where it
/// was generated).
fn ours() -> Vec<(Bundled, DiskAnnouncer)> {
    let index = crate::library::announcers::index();
    crate::library::announcers::bundled()
        .into_iter()
        .map(|a| {
            let said = index
                .iter()
                .find(|b| b.kept.name == a.kept.name)
                .map(|b| b.said.clone())
                .unwrap_or_default();
            let bundled = Bundled {
                path: a.kept.name.clone(),
                kept: a.kept.clone(),
                hashes: Default::default(),
                said,
            };
            (bundled, a)
        })
        .collect()
}

/// The draft kept in `dir` (none yet: a new one), and what the page says
/// of it; a take whose file is gone is gone.
fn open_draft(dir: &Path) -> (Draft, String) {
    let text = match std::fs::read_to_string(dir.join("draft.json")) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (Draft::default(), String::new());
        }
        Err(e) => {
            let e = e.to_string();
            return (
                Draft::default(),
                ms::status_draft_unreadable(error_text(&e)).into(),
            );
        }
    };
    match serde_json::from_str::<Draft>(&text) {
        Ok(mut draft) => {
            draft.takes.retain(|t| take_path(dir, t.id).is_file());
            (draft, String::new())
        }
        Err(e) => {
            let e = e.to_string();
            (
                Draft::default(),
                ms::status_draft_unreadable(error_text(&e)).into(),
            )
        }
    }
}

/// What the director asks of the page.
struct Scenes(Weak<Studio>);

impl SceneHost for Scenes {
    type Sound = Sound;

    fn now_ms(&self) -> f64 {
        self.0.upgrade().map_or(0.0, |s| s.now())
    }

    fn timer(&self, seconds: f64, f: Box<dyn FnOnce()>) -> Timer {
        let on = Rc::new(Cell::new(true));
        if let Some(s) = self.0.upgrade() {
            let at = s.now() + seconds.max(0.0) * 1000.0;
            s.timers.borrow_mut().push((at, on.clone(), f));
        }
        Box::new(Handle(on))
    }

    fn env(&self) -> Option<Env> {
        Some(self.0.upgrade()?.env.clone())
    }

    fn on_ui(&self, f: UiFn) {
        if let Some(s) = self.0.upgrade() {
            s.todo.borrow_mut().push(f);
        }
    }

    fn show(&self, f: UiFn) -> bool {
        self.on_ui(f);
        true
    }

    fn press(&self, action: MenuAction) {
        if let Some(s) = self.0.upgrade() {
            s.presses.borrow_mut().push(action);
        }
    }

    fn preview_open(&self, volume: f32) {
        if let Some(s) = self.0.upgrade() {
            let mut p = s.sound.borrow_mut();
            p.resume();
            p.set_volume(volume);
        }
    }

    fn preview_select(&self, song: Option<String>) {
        if let Some(s) = self.0.upgrade() {
            s.sound.borrow_mut().select(song.as_deref());
        }
    }

    fn preview_stop(&self) {
        if let Some(s) = self.0.upgrade() {
            s.sound.borrow_mut().pause();
        }
    }

    fn preview_volume(&self, volume: f32) {
        if let Some(s) = self.0.upgrade() {
            s.sound.borrow_mut().set_volume(volume);
        }
    }

    fn preview_hush(&self) {
        if let Some(s) = self.0.upgrade() {
            s.sound.borrow_mut().hush();
        }
    }

    fn preview_close(&self) {
        if let Some(s) = self.0.upgrade() {
            let mut p = s.sound.borrow_mut();
            p.close();
            p.resume();
        }
    }

    fn stage_voice(&self, staged: Option<StagedVoice<Sound>>) {
        if let Some(s) = self.0.upgrade() {
            s.sound.borrow_mut().stage(staged);
        }
    }

    fn open_results_sound(&self, _: Line) -> bool {
        true
    }

    fn say_results(
        &self,
        scene: &ResultsScene,
        voice: Voice<Sound>,
    ) -> Option<(Vec<(String, f64)>, f64)> {
        let s = self.0.upgrade()?;
        let share = Settings::load().announcer_volume;
        let mut p = s.sound.borrow_mut();
        let now = p.audio.context_time();
        let spoken = director::say_results(&mut p.audio, now, scene, voice, share);
        Some((spoken, now))
    }

    fn stop_sound(&self) {
        if let Some(s) = self.0.upgrade() {
            s.stop();
            s.sound.borrow_mut().audio.stop_all();
        }
    }

    fn changed(&self) {}

    fn publish(&self) {}

    fn gave_up(&self, why: UiText) {
        if let Some(s) = self.0.upgrade() {
            s.set_status(why);
        }
    }

    fn heard(&self, status: UiText, line: Line, spoken: Vec<String>, said: Said) {
        if let Some(s) = self.0.upgrade() {
            s.set_status(status);
            *s.heard.borrow_mut() = Some((line, spoken, said.key()));
        }
    }
}
