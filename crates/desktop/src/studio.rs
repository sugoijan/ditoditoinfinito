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
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::time::{Duration, Instant};

use ddi_l10n::UiText;
use ddi_l10n::msg::studio as ms;
use ddi_library::announcer::draft::{
    self, Adopted, Decoded, Draft, Export, HEAD, Sound as Read, TAKE_MAX_BYTES,
    Take, Write, install_plan, made_before, takes,
};
use ddi_library::announcer::examples::Set;
use ddi_library::announcer::tidy;
use ddi_library::announcer::{Bundled, Line, Spoken};
use ddi_library::path::RelPath;
use ddi_library::scan::Reader;
use ddi_platform::SoundId;
use ddi_session::announcer::scene::director::{
    self, Director, Heard, Pin, Planned, Said, SceneHost, StagedVoice, Timer, UiFn, Voiced,
};
use ddi_session::announcer::scene::{ResultsScene, in_play};
use ddi_session::announcer::{SharedVoice, Voice};
use ddi_session::preview::LEAD;
use ddi_ui::event::{MenuEvent, UiEvent};
use ddi_ui::message::Message;
use ddi_ui::screens::Env;
use ddi_ui::screens::errors::error_text;
use ddi_ui::screens::page::{PAGE_CHANGED, PageCall, PageDoc, PageFeed, PageScreen, PageShell};
use ddi_ui::screens::studio::{self, Act};
use ddi_ui::screens::studio_shell::{self, StudioHost, StudioState};
use ddi_ui::state::settings::Settings;
use ddi_ui::{Cx, MenuAction, Ui, UiRequest};

use crate::audio::Sound;
use crate::audio::mixer::Pcm;
use crate::library::announcers::DiskAnnouncer;
use crate::preview::SharedPlayer;

/// The rate takes are decoded, measured and written at.
const RATE: u32 = 48_000;
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

/// Files read ahead of the page taking them in, at most
/// ([`read_picked`]).
const READ_AHEAD: usize = 4;
/// Recordings waiting to be decoded while more are taken in, at most.
const DECODE_AHEAD: usize = 8;

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
fn shaped(sound: &Sound, processing: ddi_library::announcer::draft::Processing) -> Sound {
    if processing.measure.is_none() && !processing.stadium {
        return sound.clone();
    }
    let (pcm, rate) = processing.apply(&planar(sound), sound.rate);
    sound_of(pcm, rate)
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
    Pack(Adopted),
    File {
        path: String,
        size: u64,
        bytes: Arc<Vec<u8>>,
    },
    Done {
        passed: usize,
        failed: Option<String>,
        /// Recordings never read: too large to be one, or the draft
        /// was full ([`Draft::has_room`]).
        over: usize,
    },
}

/// Picked files being read: what goes on which line.
struct Adding {
    rx: Receiver<Arrived>,
    line: Option<Line>,
    /// The text line the first is recorded for.
    script: Option<u64>,
    added: usize,
    /// Recordings left out: the draft had no room for them.
    over: usize,
    /// Told to the reading thread once the draft is full: it reads no
    /// more.
    full: Arc<AtomicBool>,
}

/// A file picked: where it reads from.
enum Source {
    File(PathBuf),
    Zip(usize, ddi_library::path::RelPathBuf),
}

/// Lists and reads what was picked (`files`, on a thread): zips opened,
/// folders walked, what is no recording passed over, each named by its
/// path in what was picked.
///
/// What is read waits in `tx` a few files at most (the page takes them
/// in as it polls), a file too large to be a recording is never read,
/// and nothing more is once `full` says the draft has no room: a folder
/// of songs picked by mistake costs a few files' memory, not all of it.
fn read_picked(picked: Vec<PathBuf>, tx: SyncSender<Arrived>, full: Arc<AtomicBool>) {
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
    let mut readme = None;
    if let Some((name, path)) = made_before(&names) {
        let text = path
            .as_deref()
            .and_then(|r| listed.iter().find(|(p, ..)| p == r))
            .and_then(|(_, _, s)| read(s).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned());
        readme = path;
        let _ = tx.send(Arrived::Pack(Adopted::new(name, text.as_deref())));
    }
    let mut passed = 0;
    let mut over = 0;
    for (path, size, source) in &listed {
        if !takes(path, &head(source)) {
            if readme.as_deref() != Some(path.as_str()) {
                passed += 1;
            }
            continue;
        }
        if *size > TAKE_MAX_BYTES || full.load(Ordering::Relaxed) {
            over += 1;
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
    let _ = tx.send(Arrived::Done {
        passed,
        failed,
        over,
    });
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
    processing: ddi_library::announcer::draft::Processing,
    extension: Option<String>,
    decoded: Option<Sound>,
}

/// What a pack made says: the zip's file name and how many sounds, or
/// the announcer's name; or, made but not read back by the game's reader
/// as made (`Draft::check`), why: nothing written.
enum Packed {
    Zip(String),
    Game(String),
    Unread(String),
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
            let mut processing = e.processing;
            if !tidied {
                processing.measure = None;
            }
            let shaped = shaped(&sound, processing);
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
    let plan = install_plan(&listed)?;
    let place = shelf.join(&plan.folder);
    // Whole or not at all: beside it, then in its place.
    let partial = shelf.join(format!(".{}.{}.tmp", plan.folder, std::process::id()));
    let _ = std::fs::remove_dir_all(&partial);
    let written = (|| -> Result<(), String> {
        for (path, bytes) in &files {
            let rest = plan.inside(path);
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
    written.map(|()| plan.kept.name)
}

/// A pack made on a thread.
fn make_pack(
    dir: PathBuf,
    draft: Draft,
    export: Export,
    entries: Vec<PackEntry>,
    to: Packing,
) -> Result<Packed, String> {
    let mut files = Vec::with_capacity(entries.len() + 3);
    for e in entries {
        files.push(entry_bytes(&dir, e)?);
    }
    // Read back as the game reads it before it is written anywhere.
    let sizes: Vec<(String, u64)> = files
        .iter()
        .map(|(p, b)| (p.clone(), b.len() as u64))
        .collect();
    if let Err(u) = draft.check(&export, &sizes) {
        return Ok(Packed::Unread(u.0));
    }
    for (path, text) in &export.texts {
        files.push((path.clone(), text.clone().into_bytes()));
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
        plan: Vec<Planned>,
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
    /// The draft and what the page shows of it (`ddi_ui`'s, both shells').
    state: StudioState,
    env: Env,
    sound: SharedPlayer,
    /// A play asked for (a line said in play's).
    play: Rc<RefCell<Option<crate::play::Request>>>,
    feed: PageFeed,
    store: Store,
    /// Their sound, once decoded (at [`RATE`]).
    decoded: RefCell<Decoded<Sound>>,
    processed: RefCell<ddi_library::announcer::draft::Processed<Sound>>,
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
    /// The draft may be saved ([`open_draft`]).
    saves: bool,
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
        let (draft, status, saves) = open_draft(&dir);
        let (decode, results) = decoder();
        let studio = Rc::new_cyclic(|me: &Weak<Studio>| Studio {
            state: StudioState::new(draft, status),
            env: env.clone(),
            sound,
            play,
            feed: PageFeed::default(),
            store: Store::open(dir),
            decoded: RefCell::default(),
            processed: RefCell::default(),
            decode,
            results,
            decoding: RefCell::default(),
            adding: RefCell::default(),
            packing: RefCell::default(),
            example: RefCell::default(),
            ours: RefCell::new(ours()),
            example_sounds: RefCell::default(),
            director: Director::new(Scenes(me.clone())),
            waiting: RefCell::default(),
            heard_in_play: Rc::default(),
            heard: RefCell::default(),
            playing: Cell::default(),
            playing_end: RefCell::default(),
            clock: RefCell::new(Box::new(crate::clock::epoch_ms)),
            timers: RefCell::default(),
            todo: RefCell::default(),
            presses: RefCell::default(),
            dirty: Cell::new(false),
            unsaved: Cell::new(false),
            saved_at: Cell::new(0.0),
            used,
            left: Cell::new(false),
            saves,
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

    /// The page is opened again while this studio still lives (it was
    /// left with work under way, which it finishes): the one studio on
    /// its folder goes on, its draft the one draft, rather than a second
    /// one reading a draft the first is still writing.
    pub fn reopened(self: &Rc<Self>) -> PageScreen {
        self.left.set(false);
        self.dirty.set(true);
        self.screen()
    }

    /// The draft as it is.
    pub fn draft(&self) -> Draft {
        self.state.draft.borrow().clone()
    }

    /// TESTING ONLY: the draft changed as the page's dialogs change it
    /// (they are the shared page's, `ddi_ui::screens::studio_shell`, and
    /// tested there), then kept and shown.
    pub fn edit(&self, f: impl FnOnce(&mut Draft)) {
        f(&mut self.state.draft.borrow_mut());
        self.save();
        self.dirty.set(true);
    }

    /// What the page says it is doing, or did.
    pub fn status(&self) -> String {
        self.state.view.borrow().status.clone()
    }

    /// What plays (`take:<id>`, `example:<folder>:<i>`).
    pub fn playing(&self) -> Option<String> {
        self.state.view.borrow().playing.clone()
    }

    /// The scene showing.
    pub fn scene(&self) -> Option<director::SceneState> {
        self.director.state()
    }

    /// The last scene's line, what it said, and who.
    pub fn last_heard(&self) -> Option<(Line, Vec<String>, String)> {
        self.heard.borrow().clone()
    }

    /// The decoded sound kept: how many takes', and its bytes (tests).
    pub fn decoded(&self) -> (usize, usize) {
        let d = self.decoded.borrow();
        (d.len(), d.bytes())
    }

    /// At most `bytes` of decoded sound kept from now on (tests; the
    /// game's is [`draft::DECODED_BYTES`]).
    pub fn set_decoded_cap(&self, bytes: usize) {
        *self.decoded.borrow_mut() = Decoded::with_cap(bytes);
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
        self.state.doc(self.env.platform)
    }

    fn set_status(&self, text: impl Into<String>) {
        self.state.view.borrow_mut().status = text.into();
        self.dirty.set(true);
    }

    /// The draft kept (never over one that could not be read nor moved
    /// aside, [`open_draft`]).
    fn save(&self) {
        if self.saves
            && let Ok(json) = serde_json::to_string(&*self.state.draft.borrow())
        {
            self.store.send(Job::Draft(json));
        }
        self.unsaved.set(false);
        self.saved_at.set(self.now());
    }

    /// The built-in announcer's voice in `set`'s language, as a pack.
    fn pack_of(&self, set: Set) -> Option<(Bundled, DiskAnnouncer)> {
        let path = format!("{}/{}", ddi_library::announcer::BUILT_IN_FOLDER, set.key());
        self.ours
            .borrow()
            .iter()
            .find(|(b, _)| b.path == path)
            .cloned()
    }

    fn refresh_examples(&self) {
        let ours = self.example_pack(self.state.set());
        self.state.refresh_examples(ours.as_ref());
    }

    /// Picked files added: read on a thread, each on `line` when given
    /// (else by its name), the first in text line `script`'s place when
    /// given.
    fn add(&self, picked: Vec<PathBuf>, line: Option<Line>, script: Option<u64>) {
        self.set_status(ms::status_adding(picked.len()));
        let (tx, rx) = mpsc::sync_channel(READ_AHEAD);
        let full = Arc::new(AtomicBool::new(false));
        let told = full.clone();
        let _ = std::thread::Builder::new()
            .name("ddi-studio-read".into())
            .spawn(move || read_picked(picked, tx, told));
        self.adding.borrow_mut().push(Adding {
            rx,
            line,
            script,
            added: 0,
            over: 0,
            full,
        });
    }

    fn extension_of(t: &Take) -> Option<String> {
        t.format
            .extension()
            .map(str::to_string)
            .or_else(|| RelPath::new(&t.source).extension().map(str::to_string))
    }

    /// Take `id` decoded from `from`.
    fn decode_take(&self, id: u64, from: Bytes) {
        let extension = self.state.draft.borrow().take(id).and_then(Self::extension_of);
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
            .state
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

    fn processed(
        &self,
        id: u64,
        sound: &Sound,
        key: ddi_library::announcer::draft::Processing,
    ) -> Sound {
        if let Some(copy) = self.processed.borrow().get(id, key) {
            return copy;
        }
        let copy = shaped(sound, key);
        self.processed
            .borrow_mut()
            .insert(id, key, copy.clone(), copy.data.len() * 4);
        copy
    }

    fn stop(&self) {
        if let Some(id) = self.playing.take() {
            self.sound.borrow_mut().audio.stop_at(id, None);
        }
        self.playing_end.borrow_mut().take();
        self.example.borrow_mut().take();
        self.state.view.borrow_mut().playing = None;
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
        self.state.view.borrow_mut().playing = Some(key);
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
        if self.state.view.borrow().playing.as_deref() == Some(&key) {
            return self.stop();
        }
        self.stop();
        let Some(sound) = self.decoded.borrow().get(id).cloned() else {
            // Decoded first (the page opened again); pressed again then.
            self.decode_file(id);
            return;
        };
        let processing = {
            let d = self.state.draft.borrow();
            d.take(id).map(|t| d.processing(t)).unwrap_or_default()
        };
        self.speak(key, &self.processed(id, &sound, processing));
    }

    /// Our example `i` of `line` played.
    fn play_example(self: &Rc<Self>, line: Line, i: usize) {
        let key = format!("example:{}:{i}", line.folder());
        if self.state.view.borrow().playing.as_deref() == Some(&key) {
            return self.stop();
        }
        self.stop();
        let path = self
            .state
            .examples
            .borrow()
            .get(&line)
            .and_then(|e| e.get(i))
            .and_then(|e| e.sound.clone());
        let pack = self.pack_of(self.state.view.borrow().examples);
        let (Some(path), Some((_, a))) = (path, pack) else {
            return;
        };
        let rx = crate::os::background("ddi-studio-example", None, move || read_example(&a, &path));
        *self.example.borrow_mut() = Some((rx, key));
    }

    /// The pack's entries, for its thread.
    fn pack_entries(&self, export: &Export) -> Vec<PackEntry> {
        let draft = self.state.draft.borrow();
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
                    processing: t.map(|t| draft.processing(t)).unwrap_or_default(),
                    extension: t.and_then(Self::extension_of),
                    decoded: decoded.get(e.take).cloned(),
                }
            })
            .collect()
    }

    /// The pack made and put where it goes, on a thread.
    fn pack(&self, to: Packing) {
        let draft = self.state.draft.borrow().clone();
        let export = draft.export();
        let entries = self.pack_entries(&export);
        let dir = self.store.dir.clone();
        let rx = crate::os::background("ddi-studio-pack", None, move || {
            make_pack(dir, draft, export, entries, to)
        });
        *self.packing.borrow_mut() = Some(rx);
        self.state.view.borrow_mut().busy = true;
    }

    /// What `f` keeps of the draft's lines, decoded or being: those not
    /// asked for yet (the page opened again) are, here.
    fn ask_lines(&self, which: &dyn Fn(Line) -> bool) {
        let ids: Vec<u64> = self
            .state
            .draft
            .borrow()
            .takes
            .iter()
            .filter(|t| t.line.is_some_and(which) && matches!(t.sound, Read::Read(_)))
            .map(|t| t.id)
            .filter(|id| !self.decoded.borrow().contains(*id))
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
            .state
            .draft
            .borrow()
            .takes
            .iter()
            .any(|t| t.line.is_some_and(which) && decoding.contains(&t.id))
    }

    /// The draft as a voice for the lines `which` keeps.
    fn staged_voice(&self, which: impl Fn(Line) -> bool) -> Voiced<Sound> {
        let draft = self.state.draft.borrow();
        director::draft_voice(
            &draft,
            which,
            &self.director.turns.borrow(),
            self.director.pinned_take(),
            self.decoded.borrow().sounds(),
            |t, s| Some(self.processed(t.id, s, draft.processing(t))),
            |s| (s.duration(), crate::play::voice::tail(s)),
        )
    }

    /// What our pack of `set` says for `lines`: one sound of each (as the
    /// first of its chain the pack has), the line's next each scene, read
    /// and decoded once a session.
    fn example_wait(&self, set: Set, lines: &[Line]) -> Result<WaitFor, String> {
        let (pack, a) = self
            .pack_of(set)
            .ok_or_else(|| format!("{} is not here", set.pack()))?;
        let plan = self.director.example_plan(&pack, set, lines);
        let need: Vec<(String, String)> = plan
            .iter()
            .filter(|p| !self.example_sounds.borrow().contains_key(&p.key))
            .map(|p| (p.key.clone(), p.path.clone()))
            .collect();
        let rx = crate::os::background("ddi-studio-examples", None, move || -> ExampleSounds {
            need.into_iter()
                .map(|(key, path)| Ok((key, read_example(&a, &path)?)))
                .collect()
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
    /// examples chosen; `pin`, the recording or example pressed beside,
    /// said for it.
    fn hear_line(self: &Rc<Self>, line: Line, pin: Option<Pin>) {
        if self.director.busy() {
            return;
        }
        self.stop();
        let said =
            self.director
                .hearing(line, pin, &self.state.draft.borrow(), self.state.view.borrow().examples);
        match line.spoken() {
            Spoken::Menu => self.menu_scene(line, said),
            Spoken::Results => self.results_scene(line, said),
            Spoken::Play => self.play_scene(line, said),
            // No scene for a multiplayer battle's lines yet (the page
            // offers none to hear: `studio::hearable`).
            Spoken::Battle => {}
        }
    }

    fn menu_scene(self: &Rc<Self>, line: Line, said: Said) {
        let Some((n, scene)) = self.director.begin_menu(line, said) else {
            return;
        };
        let menu = director::example_lines(Spoken::Menu);
        let which = director::draft_lines(Spoken::Menu);
        self.with_voice(n, said, which, &menu, move |s, voiced| {
            s.director.show_menu(n, scene, voiced);
        });
    }

    fn results_scene(self: &Rc<Self>, line: Line, said: Said) {
        let Some((n, scene)) = self
            .director
            .begin_results(line, said, &self.state.draft.borrow())
        else {
            return;
        };
        let lines = director::results_said_lines(&scene);
        let which = director::draft_lines(Spoken::Results);
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
        let Some(chart) = in_play::chart_index(&pick, &crate::library::bundled()) else {
            return self.set_status(ms::status_no_play());
        };
        let n = self.director.begin_play(line, said);
        let which = director::draft_lines(Spoken::Play);
        // Our packs' combos past 1000 too (an example's voice).
        let lines = director::example_lines(Spoken::Play);
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

    /// A press on the page, inside its handler (`ddi_ui`'s, both
    /// shells').
    fn call(self: &Rc<Self>, call: PageCall, cx: &mut Cx) -> Option<PageDoc> {
        studio_shell::call(self, call, cx)
    }

    /// Take `id` put on `line` (`None`: unsorted).
    pub fn set_line(&self, id: u64, line: Option<Line>) {
        studio_shell::set_line(self, id, line);
    }

    /// What a press asks: the page again where it shows at once.
    pub fn act(self: &Rc<Self>, act: Act) -> Option<PageDoc> {
        studio_shell::act(self, act)
    }

    /// What was picked, as it is read.
    fn poll_adding(&self) {
        let mut adding = self.adding.borrow_mut();
        adding.retain_mut(|a| {
            loop {
                // No further ahead of the decoder than a few files: what
                // waits for it is held in memory.
                if self.decoding.borrow().len() >= DECODE_AHEAD {
                    return true;
                }
                match a.rx.try_recv() {
                    Ok(Arrived::Pack(pack)) => self.state.draft.borrow_mut().adopt(pack),
                    Ok(Arrived::File { path, size, bytes }) => {
                        let head = &bytes[..bytes.len().min(HEAD)];
                        let Some(id) = self.state.take_in(&path, size, head, a.line, &mut a.script)
                        else {
                            // No room: left out, and no more read.
                            a.over += 1;
                            a.full.store(true, Ordering::Relaxed);
                            continue;
                        };
                        self.store.send(Job::Take(id, bytes.clone()));
                        self.decode_take(id, Bytes::Memory(bytes));
                        a.added += 1;
                        self.unsaved.set(true);
                        self.dirty.set(true);
                    }
                    Ok(Arrived::Done {
                        passed,
                        failed,
                        over,
                    }) => {
                        self.save();
                        let over = a.over + over;
                        self.set_status(studio_shell::added_status(
                            a.added,
                            passed,
                            over,
                            failed.as_deref(),
                        ));
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
                let d = self.state.draft.borrow();
                matches!(d.take(id).map(|t| &t.sound), Some(Read::Read(_)))
            };
            let sound = match result {
                Ok(s) => {
                    let m = tidy::measure(&planar(&s), s.rate);
                    let bytes = std::mem::size_of_val(&*s.data);
                    self.decoded.borrow_mut().insert(id, s, bytes);
                    Read::Read(m)
                }
                Err(e) => Read::Unreadable(e),
            };
            // A take read before keeps what it measured.
            if !was_read && let Some(t) = self.state.draft.borrow_mut().take_mut(id) {
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
        self.state.view.borrow_mut().busy = false;
        match done {
            Ok(Packed::Zip(name)) => {
                self.set_status(ms::status_saved(&name, self.env.platform.l10n()))
            }
            Ok(Packed::Game(name)) => {
                self.set_status(ms::status_used(name));
                if let Some(used) = &self.used {
                    used();
                }
            }
            Ok(Packed::Unread(why)) => {
                eprintln!("announcer studio: the game's reader took the pack otherwise: {why}");
                self.set_status(ms::status_unread(error_text(&why)));
            }
            Err(e) => {
                let e = error_text(&e);
                let exporting = self.state.view.borrow().status == ms::status_exporting().to_string();
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
                    let sounds = self.example_sounds.borrow();
                    director::voiced_from(plan, |key| sounds.get(key).cloned(), |s| {
                        (s.duration(), crate::play::voice::tail(s))
                    })
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
                self.env.platform.l10n(),
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

/// Ours, each voice as a pack of its own (the built-in's, a language
/// each), with what its sounds say (the bundled index's, where it was
/// generated).
fn ours() -> Vec<(Bundled, DiskAnnouncer)> {
    use crate::library::announcers::Base;
    let index = crate::library::announcers::index();
    crate::library::announcers::bundled()
        .into_iter()
        .flat_map(|a| {
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
            let langs: Vec<String> = a.kept.voices.keys().cloned().collect();
            langs
                .into_iter()
                .filter_map(|lang| {
                    let voice = bundled.voice(&lang)?;
                    let base = match &a.base {
                        Base::Dir(d) => Base::Dir(d.join(&lang)),
                        Base::Zip { zip, dir } => Base::Zip {
                            zip: zip.clone(),
                            dir: dir.join(&lang),
                        },
                    };
                    let disk = DiskAnnouncer {
                        kept: voice.kept.clone(),
                        base,
                    };
                    Some((voice, disk))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The draft kept in `dir` (none yet: a new one), what the page says
/// of it, and whether it may be saved; a take whose file is gone is
/// gone. A draft that cannot be read or understood (a damaged file, one
/// from a newer game) is never taken for none: it is kept aside under a
/// name of its own ([`draft::aside_name`], never replaced) and a new one
/// begun, the page saying so with that name; where it cannot even be
/// moved, the new draft is not saved over it. Either way the ids of new
/// recordings start past every recording stored, so no file of the old
/// draft is replaced.
fn open_draft(dir: &Path) -> (Draft, String, bool) {
    let file = dir.join("draft.json");
    let read = match std::fs::read_to_string(&file) {
        Ok(text) => serde_json::from_str::<Draft>(&text).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Draft::default()),
        Err(e) => Err(e.to_string()),
    };
    let (mut draft, status, saves) = match read {
        Ok(mut draft) => {
            draft.takes.retain(|t| take_path(dir, t.id).is_file());
            (draft, String::new(), true)
        }
        Err(e) => {
            let name = draft::aside_name(|n| dir.join(n).exists());
            match std::fs::rename(&file, dir.join(&name)) {
                Ok(()) => (
                    Draft::default(),
                    ms::status_draft_set_aside(error_text(&e), name).into(),
                    true,
                ),
                Err(_) => (
                    Draft::default(),
                    ms::status_draft_unreadable(error_text(&e)).into(),
                    false,
                ),
            }
        }
    };
    draft.start_past(highest_take(dir));
    (draft, status, saves)
}

/// The highest id a recording stored in `dir` has (0: none).
fn highest_take(dir: &Path) -> u64 {
    std::fs::read_dir(dir.join("take"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

/// What the shared page asks of desktop.
impl StudioHost for Studio {
    fn state(&self) -> &StudioState {
        &self.state
    }

    fn env(&self) -> Option<Env> {
        Some(self.env.clone())
    }

    fn example_pack(&self, set: Set) -> Option<Bundled> {
        self.pack_of(set).map(|(b, _)| b)
    }

    fn save(&self) {
        Studio::save(self);
    }

    fn show(&self) {
        self.dirty.set(true);
    }

    fn stop(&self) {
        Studio::stop(self);
    }

    fn leave(&self, cx: &mut Cx) {
        Studio::stop(self);
        self.left.set(true);
        cx.close(true);
    }

    fn pick_recordings(self: &Rc<Self>, line: Line, script: Option<u64>) {
        let picked = crate::os::pick_recordings();
        if !picked.is_empty() {
            self.add(picked, Some(line), script);
        }
    }

    fn pick_pack(self: &Rc<Self>, folder: bool) {
        let picked = crate::os::pick(folder);
        if !picked.is_empty() {
            self.add(picked, None, None);
        }
    }

    fn play_take(self: &Rc<Self>, id: u64) -> bool {
        Studio::play_take(self, id);
        true
    }

    fn play_example(self: &Rc<Self>, line: Line, i: usize) -> bool {
        Studio::play_example(self, line, i);
        true
    }

    fn removed(&self, id: u64) {
        self.decoded.borrow_mut().remove(id);
        self.store.send(Job::Remove(id));
    }

    fn export(self: &Rc<Self>) -> bool {
        let name = self.state.draft.borrow().export().file_name;
        let Some(path) = crate::os::save_zip(&name) else {
            return false;
        };
        self.state.view.borrow_mut().status = ms::status_exporting().into();
        self.pack(Packing::Zip(path));
        true
    }

    /// Export asks where in its own press.
    fn save_zip(self: &Rc<Self>) -> bool {
        false
    }

    fn use_in_game(self: &Rc<Self>) -> bool {
        self.state.view.borrow_mut().status = ms::status_using().into();
        self.pack(Packing::Game);
        true
    }

    fn hear(self: &Rc<Self>, act: Act) {
        let hear = director::hear_of(
            act,
            &self.state.draft.borrow(),
            &self.state.examples.borrow(),
        );
        if let Some((line, pin)) = hear {
            self.hear_line(line, pin);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch/studio-tests")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("take")).unwrap();
        dir
    }

    /// Review studio-editor-4: a draft that cannot be understood is kept
    /// aside, never taken for none and written over, and the new draft's
    /// recordings never take the old ones' files.
    #[test]
    fn stadium_preview_and_wav_export_use_the_same_original_and_processing() {
        use ddi_library::announcer::draft::Processing;
        let frames = 48000;
        let source = sound_of(
            vec![
                (0..frames).map(|i| (i as f32 * 0.03).sin() * 0.1).collect(),
                vec![0.0; frames],
            ],
            48000,
        );
        let key = Processing {
            stadium: true,
            line: Some(Line::Combo(100)),
            ..Processing::default()
        };
        let preview = shaped(&source, key);
        assert_eq!(preview.frames(), 76800);
        let dir = std::env::temp_dir().join(format!("ddi-stadium-entry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (_, bytes) = entry_bytes(
            &dir,
            PackEntry {
                path: "gameplay 100 combo/voice.wav".into(),
                take: 1,
                write: Write::Wav { tidy: false },
                processing: key,
                extension: Some("wav".into()),
                decoded: Some(source.clone()),
            },
        )
        .unwrap();
        assert_eq!(bytes, ddi_library::wav::write(&planar(&preview), 48000));
        let (_, opus) = entry_bytes(
            &dir,
            PackEntry {
                path: "gameplay 100 combo/voice.opus".into(),
                take: 1,
                write: Write::Opus { tidy: false },
                processing: key,
                extension: Some("wav".into()),
                decoded: Some(source.clone()),
            },
        )
        .unwrap();
        let decoded = ddi_platform::decode_all::decode(&opus, Some("opus")).unwrap();
        assert_eq!(decoded.frames(), 76800);
        assert_eq!(decoded.channels.len(), 2);
        assert!(
            decoded
                .channels
                .iter()
                .flatten()
                .all(|s| s.is_finite() && s.abs() < 1.0)
        );
        assert!(planar(&preview)[1].iter().all(|s| *s == 0.0));
        assert_eq!(source.frames(), 48000);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A draft stored before languages, tags and takes on several lines
    /// (the library's fixture, as the studio wrote it then) opens from the
    /// data folder as it was: one language, not named.
    #[test]
    fn a_draft_stored_before_this_round_opens_as_it_was() {
        let dir = scratch("old");
        let old = include_str!("../../library/src/announcer/draft/old-draft.json");
        std::fs::write(dir.join("draft.json"), old).unwrap();
        for id in 1..=5 {
            std::fs::write(take_path(&dir, id), b"ID3").unwrap();
        }
        let (d, status, saves) = open_draft(&dir);
        assert!(status.is_empty() && saves);
        assert_eq!(d, serde_json::from_str::<Draft>(old).unwrap());
        assert_eq!(d.takes.len(), 5);
        assert!(d.languages.is_empty() && !d.declares());
        assert!(d.export().texts.is_empty());
    }

    #[test]
    fn an_unreadable_draft_is_kept_aside_and_its_recordings_files_are_not_reused() {
        let dir = scratch("unreadable");
        std::fs::write(dir.join("draft.json"), "{ a draft of a newer game").unwrap();
        std::fs::write(take_path(&dir, 1), b"the first recording").unwrap();
        std::fs::write(take_path(&dir, 7), b"the seventh").unwrap();
        let (mut d, status, saves) = open_draft(&dir);
        assert!(d.takes.is_empty());
        assert!(saves);
        assert!(status.contains("draft.unreadable.json"), "{status}");
        assert!(!dir.join("draft.json").exists(), "nothing to write over");
        assert_eq!(
            std::fs::read_to_string(dir.join("draft.unreadable.json")).unwrap(),
            "{ a draft of a newer game"
        );
        assert_eq!(d.add("new.wav", 3, b"RIFF"), 8, "past every stored file");
        // A second one is kept beside the first, which stays.
        std::fs::write(dir.join("draft.json"), "again").unwrap();
        let (_, status, _) = open_draft(&dir);
        assert!(status.contains("draft.unreadable.2.json"), "{status}");
        assert_eq!(
            std::fs::read_to_string(dir.join("draft.unreadable.json")).unwrap(),
            "{ a draft of a newer game"
        );
        // A draft read behind what is stored (its last takes' files
        // written, its own JSON not yet) gives no stored id again.
        let behind = serde_json::to_string(&Draft::default()).unwrap();
        std::fs::write(dir.join("draft.json"), behind).unwrap();
        let (mut d, status, saves) = open_draft(&dir);
        assert!(status.is_empty() && saves);
        assert_eq!(d.add("new.wav", 3, b"RIFF"), 8);
        // None at all: a new one, from 1.
        let (mut d, status, _) = open_draft(&scratch("none"));
        assert!(status.is_empty());
        assert_eq!(d.add("new.wav", 3, b"RIFF"), 1);
    }
}
