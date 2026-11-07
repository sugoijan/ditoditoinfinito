//! The announcer studio at `#/announcer` (`docs/plans/announcer-studio.md`,
//! step 2): our theme's page (`ddi_ui::screens::studio`) on the app's
//! canvas, for someone recording an announcer in an app of their own.
//!
//! - **The draft** (`ddi_library::announcer::draft`) is kept in IndexedDB
//!   as it changes, in the `files` store: `studio/draft.json`, each
//!   recording under `studio/take/<id>`; a reload opens it again.
//! - **Recordings** come from each line's Add recording, from drops
//!   anywhere on the page (on a line's zone they go on that line, the
//!   zone under the pointer lit as they are dragged; elsewhere by their
//!   names, else unsorted) and from Advanced's imports of a pack made
//!   before (a zip, `.zip` or `.smzip`, or a folder): its recordings on
//!   their lines, its name, author and license where the draft has none.
//! - **Decoded** by the browser on an offline context at 48 kHz (the wasm
//!   decoder where it refuses), measured for the tidy; played on a
//!   context made in the press, tidied as it will go in the pack, at the
//!   announcer's share of the music volume.
//! - **Export** writes the zip (each take as it came, or decoded, tidied
//!   and written as a WAV), then Save downloads it in its press. **Use in
//!   the game** stores the same files as an imported announcer
//!   (`crate::announcers::store`), to choose under Options.
//!
//! - **Hear it in the game** (step 3) shows a line's scene on the screen
//!   the game speaks it on, over the studio, with the draft as the voice
//!   where it has something to say for the line, else our pack of the
//!   examples chosen (Hear an example in the game: its sounds fetched and
//!   decoded first, once a session)
//!   (`ddi_session::announcer::scene`): song select over the bundled
//!   songs with its preview playing, the intro said as it shows and the
//!   chart chosen a moment later (its comment said, as a play opening
//!   from it would have it), or the results of a play staged for the
//!   line, its line chosen by the announcer as after a play. Each ends
//!   once its lines are said, back in the studio; each scene of a line
//!   takes its next recording (or example) first.
//!
//! `__DDI_DEBUG.studio` says what the page holds: the takes (id, name,
//! line, sound, length, how each is written), the status, the zip ready,
//! what plays, the zone lit, the scene showing (`scene`: its line,
//! screen, caption, whether its chart was chosen, the lines said) and
//! the last one's (`heard`).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::{Rc, Weak};

use ddi_library::announcer::draft::Take;
use ddi_library::announcer::draft::{Draft, HEAD, Sound, Write, readme_credits};
use ddi_library::announcer::examples::Set;
use ddi_library::announcer::{Bundled, Line, Spoken, find_announcers, tidy};
use ddi_platform::AudioBackend;
use ddi_platform::storage::{self, Area};
use ddi_session::announcer::{Announcer, Timing, Voice, scene};
use ddi_ui::event::{MenuEvent, UiEvent};
use ddi_ui::message::{Message, Param};
use ddi_ui::screens::page::{PAGE_CHANGED, PAGE_TARGET, PageCall, PageFeed, PageScreen, PageShell};
use ddi_ui::screens::results::{ActionFn, ResultsActions, ResultsScreen};
use ddi_ui::screens::select::{OpenFn, PlayFn, SelectCall, SelectScreen, ShellFn};
use ddi_ui::screens::stage::{self, BackFn, StageFrame};
use ddi_ui::screens::start;
use ddi_ui::screens::studio::{self, Act, Example, View};
use ddi_ui::{Cx, MenuAction, UiRequest};
use gloo::events::{EventListener, EventListenerOptions};
use gloo::timers::callback::Timeout;
use js_sys::Reflect;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{AudioBuffer, Blob, DragEvent, HtmlElement, HtmlInputElement};

use crate::component::{Component, Context, NodeRef};
use crate::components::ui_dev::{self, Host, Started};
use crate::songs::ManifestEntry;
use crate::web::audio::{AudioDecoder, WebAudio};
use crate::web::files::{self, PickedFile};
use crate::web::idb::{self, Db};

const DRAFT_KEY: &str = "studio/draft.json";
/// Where the example set chosen is remembered.
const EXAMPLES_KEY: &str = "ddi.studio.examples";
/// The rate takes are decoded, measured and written at.
const RATE: f32 = 48_000.0;
/// Files a pick or drop takes as recordings by their extension, where
/// their first bytes say nothing (M4A, AAC, AIFF, WebM…).
const RECORDINGS: [&str; 17] = [
    "mp3", "wav", "ogg", "oga", "opus", "flac", "m4a", "aac", "aif", "aiff", "caf", "webm", "weba",
    "mp4", "3gp", "amr", "wma",
];

/// What a line's Add recording picks.
const RECORDING_TYPES: &str = "audio/*,.m4a,.opus,.flac,.caf,.aif,.aiff,.webm";
/// What Import a pack from a zip picks.
const PACK_TYPES: &str = ".zip,.smzip";

fn take_key(id: u64) -> String {
    format!("studio/take/{id}")
}

fn extension(path: &str) -> String {
    path.rsplit('/')
        .next()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) enum Msg {
    Failed(String),
}

/// A scene lasts this long at least, seconds: time to see its screen.
const SCENE_SECONDS: f64 = 3.5;
/// It ends this long after its last line, seconds.
const SCENE_TAIL: f64 = 1.5;
/// The start menu shows this long before Singleplayer is chosen (the
/// welcome's scene), seconds.
const START_SECONDS: f64 = 1.8;
/// Song select shows a chart's best this long before it is chosen (a
/// record's scene, a first clear's), seconds.
const BEFORE_SECONDS: f64 = 2.6;
/// What a comment is said for is pointed at this long before the chart
/// is chosen, seconds.
const MARK_LEAD: f64 = 1.2;
/// A screen's way in, about, seconds: the results' line waits for it.
const ENTRANCE: f64 = 0.4;

/// A voice, each line's length (seconds) and words (where known).
type Voiced = (
    Voice<AudioBuffer>,
    HashMap<Line, f64>,
    HashMap<Line, String>,
);

/// Who says a scene's lines.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Said {
    /// The draft: the line is recorded (or what speaks in its place).
    Draft,
    /// Our pack of this set of examples (Mr. Clank, ガキンさん): it is
    /// not yet.
    Example(Set),
}

impl Said {
    /// As the debug state says it: `draft`, `example:en`.
    fn key(self) -> String {
        match self {
            Said::Draft => "draft".into(),
            Said::Example(set) => format!("example:{}", set.key()),
        }
    }
}

/// A scene showing (`docs/plans/announcer-studio.md`, step 3): a line
/// heard on the screen the game speaks it on, in a frame over the
/// studio ([`StageFrame`]), what it is said for pointed at.
struct Scene {
    line: Line,
    said: Said,
    /// When it began, milliseconds (`Date.now`).
    began: f64,
    /// The screen shown now: `start`, `select` or `results`.
    screen: &'static str,
    /// What happens now (the frame's caption).
    caption: String,
    /// Song select's chart is to be chosen (a comment's scene, a results
    /// line's song select), and was (by the scene, or by the player
    /// first).
    choose: bool,
    chosen: bool,
    /// What it points at now (a spot of the screen's, `Ui::mark`), and
    /// whether it does yet.
    mark: &'static str,
    marked: bool,
    /// Its way out was asked for on the start menu (which Back does not
    /// leave): it leaves as Singleplayer is chosen.
    leaving: bool,
    /// It plays song select's preview (on song select's own sound).
    preview: bool,
    /// The lines said, by folder, and when each starts (milliseconds,
    /// `Date.now`): one cut by the scene's end was not heard.
    spoken: Vec<(String, f64)>,
    /// Each line's words (an example's) and length, seconds.
    words: HashMap<Line, String>,
    lengths: HashMap<Line, f64>,
    /// Its next step, and the steps under way (the mark, the words shown).
    timer: Option<Timeout>,
    later: Vec<Timeout>,
    /// Tells this scene's calls and timers from a later one's.
    n: u64,
}

/// A scene's next step.
#[derive(Clone, Copy)]
enum Next {
    /// Start: Singleplayer on the start menu, the chart on song select.
    Start,
    /// Back to the studio (Back), once the scene has shown long enough.
    End,
    /// Back to the studio at once (the frame's way back).
    Leave,
}

pub(crate) struct StudioPage {
    root: NodeRef,
    studio: Rc<Studio>,
    started: Option<Started>,
    failure: Option<String>,
}

/// What the page holds, shared by the shell's calls and its tasks.
struct Studio {
    draft: RefCell<Draft>,
    view: RefCell<View>,
    /// The recordings' files, by take.
    blobs: RefCell<HashMap<u64, Blob>>,
    /// Their sound, once decoded.
    decoded: RefCell<HashMap<u64, AudioBuffer>>,
    decoding: Cell<bool>,
    /// Ours, from the bundled index: each set's pack.
    ours: RefCell<Vec<Bundled>>,
    examples: RefCell<BTreeMap<Line, Vec<Example>>>,
    feed: PageFeed,
    host: RefCell<Option<Weak<RefCell<Host>>>>,
    env: RefCell<Option<ddi_ui::screens::Env>>,
    /// The pickers, and the line the next pick goes on.
    files_input: RefCell<Option<HtmlInputElement>>,
    folder_input: RefCell<Option<HtmlInputElement>>,
    pick_line: Cell<Option<Line>>,
    /// What plays: its own context, closed when dropped.
    sound: RefCell<Option<Rc<RefCell<WebAudio>>>>,
    /// The export made: its name and its zip.
    zip: RefCell<Option<(String, Blob)>>,
    /// The zone lit under what is dragged.
    target: RefCell<Option<String>>,
    /// Bumped by each sound started, so an old one's end leaves a new one
    /// alone.
    plays: Cell<u64>,
    /// The bundled songs' entries, for song select's previews in a scene.
    songs: RefCell<Vec<ManifestEntry>>,
    scene: RefCell<Option<Scene>>,
    scenes: Cell<u64>,
    /// Each line's recording a scene says first: the next each time one
    /// was said.
    turns: RefCell<HashMap<Line, usize>>,
    /// The last scene's line, what it said and who (the debug state).
    heard: RefCell<Option<(Line, Vec<String>, String)>>,
    /// Our packs' sounds a scene took, decoded (by pack and path), and
    /// each line's next (by set).
    example_sounds: RefCell<HashMap<String, AudioBuffer>>,
    example_turns: RefCell<HashMap<(&'static str, Line), usize>>,
}

impl Studio {
    fn new() -> Rc<Studio> {
        let examples = storage::get(Area::Local, EXAMPLES_KEY)
            .and_then(|k| Set::from_key(&k))
            .unwrap_or_default();
        Rc::new(Studio {
            draft: RefCell::new(Draft::default()),
            view: RefCell::new(View {
                examples,
                ..View::default()
            }),
            blobs: RefCell::default(),
            decoded: RefCell::default(),
            decoding: Cell::new(false),
            ours: RefCell::default(),
            examples: RefCell::default(),
            feed: Rc::new(RefCell::new(None)),
            host: RefCell::new(None),
            env: RefCell::new(None),
            files_input: RefCell::new(None),
            folder_input: RefCell::new(None),
            pick_line: Cell::new(None),
            sound: RefCell::new(None),
            zip: RefCell::new(None),
            target: RefCell::new(None),
            plays: Cell::new(0),
            songs: RefCell::default(),
            scene: RefCell::new(None),
            scenes: Cell::new(0),
            turns: RefCell::default(),
            heard: RefCell::new(None),
            example_sounds: RefCell::default(),
            example_turns: RefCell::default(),
        })
    }

    fn doc(&self) -> ddi_ui::screens::page::PageDoc {
        studio::doc(
            &self.draft.borrow(),
            &self.view.borrow(),
            &self.examples.borrow(),
        )
    }

    fn weak_host(&self) -> Option<Weak<RefCell<Host>>> {
        self.host.borrow().clone()
    }

    /// The page shown again, and the debug state.
    fn send(&self) {
        *self.feed.borrow_mut() = Some(self.doc());
        if let Some(weak) = self.weak_host() {
            ui_dev::broadcast_soon(weak, PAGE_CHANGED);
        }
        self.publish();
    }

    fn status(&self, text: impl Into<String>) {
        self.view.borrow_mut().status = text.into();
        self.send();
    }

    fn publish(&self) {
        if let Some(debug) = crate::web::debug_object() {
            self.debug_into(&debug);
        }
    }

    /// `__DDI_DEBUG.studio` (the host writes its debug object anew; this
    /// goes in each time, [`ui_dev::DebugFn`]).
    fn debug_into(&self, debug: &js_sys::Object) {
        let set = |o: &js_sys::Object, k: &str, v: JsValue| {
            let _ = Reflect::set(o, &k.into(), &v);
        };
        let o = js_sys::Object::new();
        let draft = self.draft.borrow();
        let takes = js_sys::Array::new();
        for t in &draft.takes {
            let x = js_sys::Object::new();
            set(&x, "id", (t.id as f64).into());
            set(&x, "source", t.source.as_str().into());
            set(
                &x,
                "line",
                t.line.map_or(JsValue::NULL, |l| l.folder().into()),
            );
            let sound = match &t.sound {
                Sound::Unread => "unread",
                Sound::Read(_) => "read",
                Sound::Unreadable(_) => "unreadable",
            };
            set(&x, "sound", sound.into());
            set(
                &x,
                "seconds",
                draft.seconds(t).map_or(JsValue::NULL, JsValue::from),
            );
            let write = match draft.write_of(t) {
                Write::AsIs => "as-is",
                Write::Wav { tidy: true } => "wav-tidied",
                Write::Wav { tidy: false } => "wav",
            };
            set(&x, "write", write.into());
            set(&x, "format", format!("{:?}", t.format).into());
            takes.push(&x);
        }
        set(&o, "takes", takes.into());
        set(&o, "name", draft.name.as_str().into());
        set(&o, "author", draft.author.as_str().into());
        set(&o, "license", draft.license.as_str().into());
        set(&o, "tidy", draft.tidy.into());
        let view = self.view.borrow();
        set(&o, "status", view.status.as_str().into());
        set(
            &o,
            "ready",
            view.ready.as_deref().map_or(JsValue::NULL, JsValue::from),
        );
        set(
            &o,
            "playing",
            view.playing.as_deref().map_or(JsValue::NULL, JsValue::from),
        );
        set(&o, "examples", view.examples.key().into());
        set(
            &o,
            "target",
            self.target
                .borrow()
                .as_deref()
                .map_or(JsValue::NULL, JsValue::from),
        );
        set(
            &o,
            "zipBytes",
            self.zip
                .borrow()
                .as_ref()
                .map_or(JsValue::NULL, |(_, b)| b.size().into()),
        );
        let folders = |list: &[String]| {
            let a = js_sys::Array::new();
            for f in list {
                a.push(&f.as_str().into());
            }
            JsValue::from(a)
        };
        let scene = match &*self.scene.borrow() {
            Some(sc) => {
                let x = js_sys::Object::new();
                set(&x, "line", sc.line.folder().into());
                set(&x, "said", sc.said.key().into());
                set(&x, "screen", sc.screen.into());
                set(&x, "caption", sc.caption.as_str().into());
                set(&x, "chosen", sc.chosen.into());
                set(
                    &x,
                    "mark",
                    if sc.marked {
                        sc.mark.into()
                    } else {
                        JsValue::NULL
                    },
                );
                let said: Vec<String> = sc.spoken.iter().map(|(f, _)| f.clone()).collect();
                set(&x, "spoken", folders(&said));
                x.into()
            }
            None => JsValue::NULL,
        };
        set(&o, "scene", scene);
        let heard = match &*self.heard.borrow() {
            Some((line, spoken, said)) => {
                let x = js_sys::Object::new();
                set(&x, "line", line.folder().into());
                set(&x, "said", said.as_str().into());
                set(&x, "spoken", folders(spoken));
                x.into()
            }
            None => JsValue::NULL,
        };
        set(&o, "heard", heard);
        set(debug, "studio", o.into());
    }

    /// The draft written to storage.
    fn save(&self) {
        let Ok(json) = serde_json::to_string(&*self.draft.borrow()) else {
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let saved = async {
                Db::open()
                    .await?
                    .write(vec![idb::Write::Put {
                        store: idb::FILES,
                        key: DRAFT_KEY.into(),
                        value: JsValue::from_str(&json),
                    }])
                    .await
            }
            .await;
            if let Err(e) = saved {
                web_sys::console::warn_1(&format!("studio: saving the draft: {e}").into());
            }
        });
    }

    /// The draft and its recordings from storage, and ours for the
    /// examples.
    async fn load(self: &Rc<Self>) {
        let read = async {
            let db = Db::open().await?;
            let draft: Option<Draft> = db
                .get(idb::FILES, DRAFT_KEY)
                .await?
                .and_then(|v| v.as_string())
                .and_then(|j| serde_json::from_str(&j).ok());
            let Some(draft) = draft else {
                return Ok::<_, String>(());
            };
            let mut blobs = HashMap::new();
            for t in &draft.takes {
                if let Some(v) = db.get(idb::FILES, &take_key(t.id)).await? {
                    blobs.insert(t.id, Blob::from(v));
                }
            }
            let mut draft = draft;
            // A take whose file is gone is gone.
            draft.takes.retain(|t| blobs.contains_key(&t.id));
            // Recordings added before it was read (a quick drop) keep the
            // draft they made.
            if self.draft.borrow().takes.is_empty() {
                *self.draft.borrow_mut() = draft;
            }
            self.blobs.borrow_mut().extend(blobs);
            Ok(())
        }
        .await;
        if let Err(e) = read {
            self.status(format!("The draft could not be read: {e}"));
        }
        match crate::announcers::bundled().await {
            Ok(ours) => *self.ours.borrow_mut() = ours,
            Err(e) => web_sys::console::warn_1(&format!("studio: our announcers: {e}").into()),
        }
        match crate::songs::load_manifest().await {
            Ok(m) => *self.songs.borrow_mut() = m.songs,
            Err(e) => web_sys::console::warn_1(&format!("studio: the songs: {e}").into()),
        }
        self.refresh_examples();
        self.send();
        self.decode_pending();
    }

    fn pack_of(&self, set: Set) -> Option<Bundled> {
        self.ours
            .borrow()
            .iter()
            .find(|b| b.kept.name == set.pack())
            .cloned()
    }

    fn refresh_examples(&self) {
        let set = self.view.borrow().examples;
        let ours = self.pack_of(set);
        *self.examples.borrow_mut() = studio::examples(set, ours.as_ref());
    }

    /// Picked or dropped files added: zips opened, what is no recording
    /// passed over, each on `line` when given (else by its name).
    fn add(self: &Rc<Self>, picked: Vec<PickedFile>, line: Option<Line>) {
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let count = picked.len();
            this.status(format!(
                "Adding {count} file{}…",
                if count == 1 { "" } else { "s" }
            ));
            let mut files: Vec<(String, Blob)> = Vec::new();
            for p in picked {
                if ddi_library::zip::archive_stem(&p.path).is_some() {
                    match zip_files(&p).await {
                        Ok(inside) => files.extend(inside),
                        Err(e) => {
                            web_sys::console::warn_1(&format!("studio: {}: {e}", p.path).into())
                        }
                    }
                } else {
                    files.push((p.path.clone(), p.file.into()));
                }
            }
            // A pack made before: its name, and the credits its README
            // gives, where the draft has none.
            let listed: Vec<(String, u64)> = files
                .iter()
                .map(|(p, b)| (p.clone(), b.size() as u64))
                .collect();
            let found = find_announcers(&listed, "");
            let readme = match found.announcers.as_slice() {
                [_] => files
                    .iter()
                    .filter(|(p, _)| {
                        p.rsplit('/')
                            .next()
                            .is_some_and(|n| n.eq_ignore_ascii_case("readme.txt"))
                    })
                    .min_by_key(|(p, _)| p.matches('/').count())
                    .cloned(),
                _ => None,
            };
            let credits = match &readme {
                Some((_, blob)) => wasm_bindgen_futures::JsFuture::from(blob.text())
                    .await
                    .ok()
                    .and_then(|t| t.as_string())
                    .map(|t| readme_credits(&t)),
                None => None,
            };
            {
                let mut d = this.draft.borrow_mut();
                if let [a] = found.announcers.as_slice()
                    && d.name.trim().is_empty()
                {
                    d.name = a.name.clone();
                }
                let (by, license) = credits.unwrap_or_default();
                if let Some(by) = by.filter(|_| d.author.trim().is_empty()) {
                    d.author = by;
                }
                if let Some(l) = license.filter(|_| d.license.trim().is_empty()) {
                    d.license = l;
                }
            }
            let readme = readme.map(|(p, _)| p);
            let mut writes = Vec::new();
            let mut added = 0;
            let mut passed = 0;
            for (path, blob) in files {
                let head = head_of(&blob).await.unwrap_or_default();
                let format = ddi_library::announcer::draft::Format::sniff(&head);
                let name = path.rsplit('/').next().unwrap_or(&path);
                if name.starts_with('.')
                    || (format == ddi_library::announcer::draft::Format::Other
                        && !RECORDINGS.contains(&extension(&path).as_str()))
                {
                    if readme.as_deref() != Some(path.as_str()) {
                        passed += 1;
                    }
                    continue;
                }
                let id = {
                    let mut d = this.draft.borrow_mut();
                    let id = d.add(&path, blob.size() as u64, &head);
                    if let Some(l) = line
                        && let Some(t) = d.take_mut(id)
                    {
                        t.line = Some(l);
                    }
                    id
                };
                writes.push(idb::Write::Put {
                    store: idb::FILES,
                    key: take_key(id),
                    value: blob.clone().into(),
                });
                this.blobs.borrow_mut().insert(id, blob);
                added += 1;
            }
            let stored = async { Db::open().await?.write(writes).await }.await;
            this.forget_export();
            let mut said = format!(
                "Added {added} recording{}.",
                if added == 1 { "" } else { "s" }
            );
            if passed > 0 {
                said += &format!(
                    " {passed} file{} passed over: not sounds.",
                    if passed == 1 { " was" } else { "s were" }
                );
            }
            if let Err(e) = stored {
                said += &format!(" They could not be kept in the browser: {e}");
            }
            this.save();
            this.status(said);
            this.decode_pending();
        });
    }

    /// The takes not decoded yet, decoded one after another.
    fn decode_pending(self: &Rc<Self>) {
        if self.decoding.replace(true) {
            return;
        }
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let Some(decoder) = AudioDecoder::offline(RATE) else {
                this.decoding.set(false);
                this.status("This browser cannot decode sounds here.");
                return;
            };
            loop {
                let next = this
                    .draft
                    .borrow()
                    .takes
                    .iter()
                    .find(|t| matches!(t.sound, Sound::Unread))
                    .map(|t| t.id);
                let Some(id) = next else {
                    break;
                };
                let sound = match this.read_buffer(&decoder, id).await {
                    Ok(buffer) => {
                        let channels = channels(&buffer);
                        let m = tidy::measure(&channels, buffer.sample_rate() as u32);
                        this.decoded.borrow_mut().insert(id, buffer);
                        Sound::Read(m)
                    }
                    Err(e) => Sound::Unreadable(e),
                };
                if let Some(t) = this.draft.borrow_mut().take_mut(id) {
                    t.sound = sound;
                }
                this.save();
                this.send();
            }
            this.decoding.set(false);
        });
    }

    async fn read_buffer(&self, decoder: &AudioDecoder, id: u64) -> Result<AudioBuffer, String> {
        let blob = self
            .blobs
            .borrow()
            .get(&id)
            .cloned()
            .ok_or("its file is gone")?;
        let bytes = files::blob_bytes(&blob).await?;
        decoder.decode(&bytes, Default::default()).await
    }

    /// Take `id`'s sound, decoded now where it was not.
    async fn buffer(&self, id: u64) -> Result<AudioBuffer, String> {
        if let Some(b) = self.decoded.borrow().get(&id) {
            return Ok(b.clone());
        }
        let decoder = AudioDecoder::offline(RATE).ok_or("this browser cannot decode sounds")?;
        let b = self.read_buffer(&decoder, id).await?;
        self.decoded.borrow_mut().insert(id, b.clone());
        Ok(b)
    }

    /// What take `id` sounds like in the pack: its channels, tidied when
    /// it is, at its rate.
    async fn pcm(&self, id: u64, tidied: bool) -> Result<(Vec<Vec<f32>>, u32), String> {
        let buffer = self.buffer(id).await?;
        Ok(self.shaped(id, &buffer, tidied))
    }

    /// Take `id`'s sound from its decoded `buffer`, tidied when it is.
    fn shaped(&self, id: u64, buffer: &AudioBuffer, tidied: bool) -> (Vec<Vec<f32>>, u32) {
        let rate = buffer.sample_rate() as u32;
        let raw = channels(buffer);
        if !tidied {
            return (raw, rate);
        }
        let measured = self
            .draft
            .borrow()
            .take(id)
            .and_then(|t| t.measure().copied());
        let m = measured.unwrap_or_else(|| tidy::measure(&raw, rate));
        (tidy::apply(&raw, rate, &m), rate)
    }

    /// A context made in the press to play on (Safari starts one only
    /// there): what played before stops.
    fn new_sound(&self, playing: String) -> Option<(Rc<RefCell<WebAudio>>, u64)> {
        self.sound.borrow_mut().take();
        let audio = match WebAudio::new() {
            Ok(a) => a,
            Err(e) => {
                self.view.borrow_mut().status = format!("No sound: {e}");
                return None;
            }
        };
        audio.resume_now();
        let settings = crate::settings::Settings::load();
        audio.set_master_volume(settings.volume);
        let audio = Rc::new(RefCell::new(audio));
        *self.sound.borrow_mut() = Some(audio.clone());
        let n = self.plays.get() + 1;
        self.plays.set(n);
        self.view.borrow_mut().playing = Some(playing);
        Some((audio, n))
    }

    fn stop(&self) {
        self.sound.borrow_mut().take();
        self.view.borrow_mut().playing = None;
    }

    /// `pcm` played on `audio` (started for play `n`), and the page told
    /// when it ends.
    fn play_pcm(
        self: &Rc<Self>,
        audio: &Rc<RefCell<WebAudio>>,
        n: u64,
        pcm: Vec<Vec<f32>>,
        rate: u32,
    ) -> Result<(), String> {
        let share = crate::settings::Settings::load().announcer_volume;
        let mut a = audio.borrow_mut();
        let buffer = a.buffer_from_pcm(pcm, f64::from(rate))?;
        let at = a.context().current_time() + 0.05;
        a.play_voice(&buffer, at, share)?;
        let ms = ((buffer.duration() + 0.15) * 1000.0) as u32;
        let weak = Rc::downgrade(self);
        gloo::timers::callback::Timeout::new(ms, move || {
            if let Some(s) = weak.upgrade()
                && s.plays.get() == n
            {
                s.stop();
                s.send();
            }
        })
        .forget();
        Ok(())
    }

    /// Take `id` played as it will be in the pack (again: stopped).
    fn play(self: &Rc<Self>, id: u64) {
        let key = format!("take:{id}");
        if self.view.borrow().playing.as_deref() == Some(&key) {
            self.stop();
            self.send();
            return;
        }
        let Some((audio, n)) = self.new_sound(key) else {
            self.send();
            return;
        };
        self.send();
        let this = self.clone();
        let running = audio.borrow().decoder();
        wasm_bindgen_futures::spawn_local(async move {
            let tidied = {
                let d = this.draft.borrow();
                d.take(id).is_some_and(|t| d.tidies(t))
            };
            let pcm = this.pcm(id, tidied).await;
            running.resume().await;
            let played = match pcm {
                Ok((pcm, rate)) => this.play_pcm(&audio, n, pcm, rate),
                Err(e) => Err(e),
            };
            if let Err(e) = played {
                this.stop();
                this.status(format!("It could not be played: {e}"));
            }
        });
    }

    /// Our example `i` of `line` played.
    fn example(self: &Rc<Self>, line: Line, i: usize) {
        let key = format!("example:{}:{i}", line.folder());
        if self.view.borrow().playing.as_deref() == Some(&key) {
            self.stop();
            self.send();
            return;
        }
        let path = self
            .examples
            .borrow()
            .get(&line)
            .and_then(|e| e.get(i))
            .and_then(|e| e.sound.clone());
        let pack = self.pack_of(self.view.borrow().examples);
        let (Some(path), Some(pack)) = (path, pack) else {
            return;
        };
        let Some((audio, n)) = self.new_sound(key) else {
            self.send();
            return;
        };
        self.send();
        let this = self.clone();
        let running = audio.borrow().decoder();
        wasm_bindgen_futures::spawn_local(async move {
            let played = async {
                let bytes = crate::announcers::bundled_sound(&pack, &path).await?;
                let decoder = AudioDecoder::offline(RATE).ok_or("no decoder")?;
                let buffer = decoder.decode(&bytes, Default::default()).await?;
                let rate = buffer.sample_rate() as u32;
                running.resume().await;
                this.play_pcm(&audio, n, channels(&buffer), rate)
            }
            .await;
            if let Err(e) = played {
                this.stop();
                this.status(format!("The example could not be played: {e}"));
            }
        });
    }

    /// An export made before no longer says what the draft does.
    fn forget_export(&self) {
        self.zip.borrow_mut().take();
        self.view.borrow_mut().ready = None;
    }

    /// The pack's files: each export entry's, and where it goes.
    async fn files(&self) -> Result<(String, Vec<(String, Blob)>, String), String> {
        let export = self.draft.borrow().export();
        let mut out = Vec::new();
        for e in &export.entries {
            let blob = match e.write {
                Write::AsIs => self
                    .blobs
                    .borrow()
                    .get(&e.take)
                    .cloned()
                    .ok_or_else(|| format!("{}: its file is gone", e.path))?,
                Write::Wav { tidy } => {
                    let (pcm, rate) = self.pcm(e.take, tidy).await?;
                    files::bytes_blob(&ddi_library::wav::write(&pcm, rate), "audio/wav")?
                }
            };
            out.push((e.path.clone(), blob));
        }
        Ok((export.file_name, out, export.readme))
    }

    /// The export made, to save with the next press.
    fn export(self: &Rc<Self>) {
        self.view.borrow_mut().busy = true;
        self.status("Making the pack…");
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let made = async {
                let (name, files, readme) = this.files().await?;
                let mut entries = Vec::with_capacity(files.len() + 1);
                for (path, blob) in &files {
                    entries.push((path.clone(), files::blob_bytes(blob).await?));
                }
                entries.push(("README.txt".to_string(), readme.into_bytes()));
                let zip =
                    ddi_library::zip::write_stored(&entries).ok_or("the pack is too large")?;
                let blob = files::bytes_blob(&zip, "application/zip")?;
                Ok::<_, String>((name, blob, files.len()))
            }
            .await;
            this.view.borrow_mut().busy = false;
            match made {
                Ok((name, blob, n)) => {
                    *this.zip.borrow_mut() = Some((name.clone(), blob));
                    this.view.borrow_mut().ready = Some(name.clone());
                    this.status(format!(
                        "{name} is ready, with {n} sound{}: press Save {name}.",
                        if n == 1 { "" } else { "s" }
                    ));
                }
                Err(e) => this.status(format!("The pack could not be made: {e}")),
            }
        });
    }

    /// The zip made, downloaded (inside the press).
    fn save_zip(&self) {
        let Some((name, blob)) = self.zip.borrow().clone() else {
            return;
        };
        let Some(url) = files::object_url(&blob) else {
            return;
        };
        let clicked = (|| {
            let doc = web_sys::window()?.document()?;
            let a: HtmlElement = doc.create_element("a").ok()?.dyn_into().ok()?;
            a.set_attribute("href", &url).ok()?;
            a.set_attribute("download", &name).ok()?;
            doc.body()?.append_child(&a).ok()?;
            a.click();
            a.remove();
            Some(())
        })();
        // The download has the blob by now; the URL goes a little later.
        gloo::timers::callback::Timeout::new(60_000, move || files::revoke_object_url(&url))
            .forget();
        self.view.borrow_mut().status = match clicked {
            Some(()) => format!("{name} saved to your downloads."),
            None => "The pack could not be saved.".into(),
        };
    }

    /// The pack stored as an imported announcer, to choose under Options.
    fn use_in_game(self: &Rc<Self>) {
        self.view.borrow_mut().busy = true;
        self.status("Adding it to the game…");
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let stored = async {
                let (_, files, _) = this.files().await?;
                let listed: Vec<(String, u64)> = files
                    .iter()
                    .map(|(p, b)| (p.clone(), b.size() as u64))
                    .collect();
                let a = find_announcers(&listed, "root")
                    .announcers
                    .into_iter()
                    .next()
                    .ok_or("it has no lines yet")?;
                let kept = a.kept();
                let prefix = format!("{}/", a.dir);
                let sounds: Vec<(String, Blob)> = files
                    .into_iter()
                    .map(|(p, b)| (p.strip_prefix(&prefix).unwrap_or(&p).to_string(), b))
                    .collect();
                let writes = crate::announcers::store(&kept, sounds)?;
                Db::open().await?.write(writes).await?;
                Ok::<_, String>(kept.name)
            }
            .await;
            this.view.borrow_mut().busy = false;
            match stored {
                Ok(name) => this.status(format!(
                    "{name} is in the game: choose it under Options, Sound, Announcer."
                )),
                Err(e) => this.status(format!("It could not be added: {e}")),
            }
        });
    }

    fn remove(&self, id: u64) {
        self.draft.borrow_mut().remove(id);
        self.blobs.borrow_mut().remove(&id);
        self.decoded.borrow_mut().remove(&id);
        self.forget_export();
        self.save();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(db) = Db::open().await {
                let _ = db
                    .write(vec![idb::Write::Delete {
                        store: idb::FILES,
                        key: take_key(id),
                    }])
                    .await;
            }
        });
    }

    /// The zone under a point of the window: a line's, or the unsorted.
    fn zone_at(&self, client: [f64; 2]) -> Option<String> {
        let host = self.weak_host()?.upgrade()?;
        let p = ui_dev::virtual_point(&host, client);
        let h = host.borrow();
        let mut ids = vec!["unsorted".to_string()];
        ids.extend(self.draft.borrow().lines().iter().map(|l| l.folder()));
        ids.into_iter().find(|id| {
            h.ui.visible_bounds(&format!("Zone.{id}"))
                .is_some_and(|b| p[0] >= b[0] && p[0] <= b[2] && p[1] >= b[1] && p[1] <= b[3])
        })
    }

    /// The zone lit (`None`: none).
    fn light(&self, zone: Option<String>) {
        if *self.target.borrow() == zone {
            return;
        }
        *self.target.borrow_mut() = zone.clone();
        if let Some(weak) = self.weak_host() {
            ui_dev::broadcast_message_soon(
                weak,
                Message::new(PAGE_TARGET).with("Zone", Param::Str(zone.unwrap_or_default())),
            );
        }
        self.publish();
    }

    /// The draft as a voice for the lines `which` keeps: each line's
    /// recordings decoded so far, as the pack has them (tidied where they
    /// are), the line's next one first, each made a buffer by `make`;
    /// with the first one's length of each line.
    fn staged_voice(
        &self,
        which: impl Fn(Line) -> bool,
        make: impl Fn(Vec<Vec<f32>>, u32) -> Option<AudioBuffer>,
    ) -> Voiced {
        let draft = self.draft.borrow();
        let turns = self.turns.borrow();
        let decoded = self.decoded.borrow();
        let mut voice = Voice::default();
        let mut lengths = HashMap::new();
        for line in Line::all().into_iter().filter(|l| which(*l)) {
            let takes: Vec<&Take> = draft
                .takes_of(line)
                .filter(|t| decoded.contains_key(&t.id))
                .collect();
            if takes.is_empty() {
                continue;
            }
            voice.expect([line]);
            let first = turns.get(&line).copied().unwrap_or(0) % takes.len();
            for k in 0..takes.len() {
                let t = takes[(first + k) % takes.len()];
                let (pcm, rate) = self.shaped(t.id, &decoded[&t.id], draft.tidies(t));
                if let Some(b) = make(pcm, rate) {
                    lengths.entry(line).or_insert(b.duration());
                    voice.add(line, b);
                }
            }
        }
        // A recording's words are not known: its line by name.
        (voice, lengths, HashMap::new())
    }

    /// Hears `line` in the game, in the press: its scene over the studio,
    /// said by the draft where it has something to say for the line, else
    /// by our pack of the examples chosen.
    fn hear(self: &Rc<Self>, line: Line) {
        if self.scene.borrow().is_some() {
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
            Spoken::Play => {}
        }
        self.publish();
    }

    #[allow(clippy::too_many_arguments)]
    fn begin_scene(
        &self,
        line: Line,
        said: Said,
        screen: &'static str,
        choose: bool,
        caption: &str,
        mark: &'static str,
        preview: bool,
    ) -> u64 {
        let n = self.scenes.get() + 1;
        self.scenes.set(n);
        *self.scene.borrow_mut() = Some(Scene {
            line,
            said,
            began: js_sys::Date::now(),
            screen,
            caption: caption.into(),
            choose,
            chosen: false,
            mark,
            marked: false,
            leaving: false,
            preview,
            spoken: Vec::new(),
            words: HashMap::new(),
            lengths: HashMap::new(),
            timer: None,
            later: Vec::new(),
            n,
        });
        n
    }

    /// Whether scene `n` is still the one under way.
    fn scene_is(&self, n: u64) -> bool {
        self.scene.borrow().as_ref().is_some_and(|s| s.n == n)
    }

    /// `f` on scene `n` (while it is the one under way).
    fn with_scene<T>(&self, n: u64, f: impl FnOnce(&mut Scene) -> T) -> Option<T> {
        self.scene.borrow_mut().as_mut().filter(|s| s.n == n).map(f)
    }

    /// Scene `n` given up before it showed (its example could not be
    /// read): the studio says why.
    fn drop_scene(&self, n: u64, why: String) {
        let Some(sc) = self.scene.borrow_mut().take_if(|s| s.n == n) else {
            return;
        };
        crate::announcers::stage(None);
        if sc.preview {
            crate::select_preview::close();
        }
        self.stop();
        self.status(why);
    }

    /// `f` on the page's UI: now, or just after the event it is handling
    /// (a screen's call: the scene's steps come from them), then a frame.
    fn on_ui(&self, f: impl FnOnce(&mut ddi_ui::Ui) + 'static) {
        let Some(weak) = self.weak_host() else {
            return;
        };
        let Some(host) = weak.upgrade() else {
            return;
        };
        if let Ok(mut h) = host.try_borrow_mut() {
            f(&mut h.ui);
            drop(h);
            ui_dev::wake(&host);
            return;
        }
        Timeout::new(0, move || {
            if let Some(host) = weak.upgrade()
                && let Ok(mut h) = host.try_borrow_mut()
            {
                f(&mut h.ui);
                drop(h);
                ui_dev::wake(&host);
            }
        })
        .forget();
    }

    /// The frame scene `n` is shown in ([`StageFrame`]): its line, who
    /// says it, what happens first; its way back leaves the scene.
    fn frame(self: &Rc<Self>, env: ddi_ui::screens::Env, n: u64, caption: &str) -> StageFrame {
        let (line, said) = self
            .with_scene(n, |sc| (sc.line, sc.said))
            .unwrap_or((Line::Ready, Said::Draft));
        let voice = match said {
            Said::Draft => "Your recording".to_string(),
            Said::Example(set) => format!("An example by {}", set.pack()),
        };
        let weak = Rc::downgrade(self);
        let back: BackFn = Rc::new(move |_| {
            if let Some(s) = weak.upgrade() {
                s.after(n, 0.0, Next::Leave);
            }
        });
        StageFrame::new(env, &studio::name(line), &voice, caption, back)
    }

    /// The scene's frame over the studio and its first screen in it, the
    /// studio leaving first (out of the press: what it says is ready),
    /// `mark` pointed at; whether they were.
    fn show(
        &self,
        frame: StageFrame,
        screen: impl ddi_ui::Screen + 'static,
        mark: Option<&str>,
    ) -> bool {
        let Some(host) = self.weak_host().and_then(|w| w.upgrade()) else {
            return false;
        };
        let Ok(mut h) = host.try_borrow_mut() else {
            return false;
        };
        h.ui.push_chrome(frame);
        h.ui.push_over(screen);
        h.ui.mark(mark);
        drop(h);
        ui_dev::wake(&host);
        true
    }

    /// Scene `n` points at what it is for (its spot), now.
    fn mark(&self, n: u64) {
        let Some(mark) = self.with_scene(n, |sc| {
            sc.marked = true;
            sc.mark
        }) else {
            return;
        };
        self.on_ui(move |ui| ui.mark(Some(mark)));
        self.publish();
    }

    /// The frame's caption: what happens now.
    fn caption_now(&self, n: u64, caption: &str) {
        self.with_scene(n, |sc| sc.caption = caption.into());
        let m = Message::new(stage::CAPTION).with("Text", Param::Str(caption.into()));
        self.on_ui(move |ui| ui.broadcast(m));
    }

    /// The frame's bar: the scene ends in about `seconds`.
    fn until(&self, n: u64, seconds: f64) {
        if !self.scene_is(n) {
            return;
        }
        let m = Message::new(stage::UNTIL).with("Seconds", Param::Num(seconds));
        self.on_ui(move |ui| ui.broadcast(m));
    }

    /// The frame shows the words of `line` (said now) for `seconds`: an
    /// example's as its pack says them, a recording's line by name.
    fn say(&self, n: u64, line: Line, seconds: f64) {
        let Some(words) = self.with_scene(n, |sc| sc.words.get(&line).cloned()) else {
            return;
        };
        let text = words.unwrap_or_else(|| format!("[{}]", studio::name(line)));
        let m = Message::new(stage::SAY)
            .with("Text", Param::Str(text))
            .with("Seconds", Param::Num(seconds));
        self.on_ui(move |ui| ui.broadcast(m));
    }

    /// `f` in `seconds`, while scene `n` lasts.
    fn later(self: &Rc<Self>, n: u64, seconds: f64, f: impl FnOnce(&Rc<Self>) + 'static) {
        let weak = Rc::downgrade(self);
        let t = Timeout::new((seconds.max(0.0) * 1000.0) as u32, move || {
            if let Some(s) = weak.upgrade()
                && s.scene_is(n)
            {
                f(&s);
            }
        });
        self.with_scene(n, |sc| sc.later.push(t));
    }

    /// Our pack of `set` as a voice for `lines` (each as the first of its
    /// chain the pack has): one sound of each, the line's next each scene,
    /// fetched and decoded once a session; with each one's length and
    /// words.
    async fn example_voice(&self, set: Set, lines: &[Line]) -> Result<Voiced, String> {
        let pack = self
            .pack_of(set)
            .ok_or_else(|| format!("{} is not here", set.pack()))?;
        let mut voice = Voice::default();
        let mut lengths = HashMap::new();
        let mut words = HashMap::new();
        let mut decoder = None;
        for &line in lines {
            let Some((found, files)) = line.chain().into_iter().find_map(|l| {
                let files = pack.kept.lines.get(&l.folder())?;
                (!files.is_empty()).then(|| (l, files.clone()))
            }) else {
                continue;
            };
            if lengths.contains_key(&found) {
                continue;
            }
            let turn = self
                .example_turns
                .borrow()
                .get(&(set.key(), found))
                .copied()
                .unwrap_or(0);
            let path = files[turn % files.len()].to_string();
            let key = format!("{}/{path}", pack.path);
            let kept = self.example_sounds.borrow().get(&key).cloned();
            let buffer = match kept {
                Some(b) => b,
                None => {
                    let bytes = crate::announcers::bundled_sound(&pack, &path).await?;
                    if decoder.is_none() {
                        decoder = Some(
                            AudioDecoder::offline(RATE)
                                .ok_or("this browser cannot decode sounds here")?,
                        );
                    }
                    let b = decoder
                        .as_ref()
                        .ok_or("no decoder")?
                        .decode(&bytes, Default::default())
                        .await?;
                    self.example_sounds.borrow_mut().insert(key, b.clone());
                    b
                }
            };
            if let Some(w) = pack.said.get(&path) {
                words.insert(found, w.clone());
            }
            voice.expect([found]);
            lengths.insert(found, buffer.duration());
            voice.add(found, buffer);
        }
        Ok((voice, lengths, words))
    }

    /// Scene `n`'s words and lengths, kept for its steps.
    fn keep(&self, n: u64, lengths: &HashMap<Line, f64>, words: HashMap<Line, String>) {
        self.with_scene(n, |sc| {
            sc.lengths = lengths.clone();
            sc.words = words;
        });
    }

    /// How long `line` is said for in scene `n` (as the first of its
    /// chain the voice has), about.
    fn length(&self, n: u64, line: Line) -> f64 {
        self.with_scene(n, |sc| {
            line.chain()
                .into_iter()
                .find_map(|l| sc.lengths.get(&l).copied())
        })
        .flatten()
        .unwrap_or(1.5)
    }

    /// Song select's scene: the bundled songs, the preview playing on
    /// song select's own sound (made in this press), the intro said as it
    /// shows (the welcome's from the start menu, Singleplayer chosen on
    /// it); a comment's chart pointed at and chosen once the intro is
    /// said.
    fn menu_scene(self: &Rc<Self>, line: Line, said: Said) {
        let started = storage::started_ms();
        let Some(scene) = scene::menu(line, started, js_sys::Date::now()) else {
            return;
        };
        let Some(env) = self.env.borrow().clone() else {
            return;
        };
        let settings = crate::settings::Settings::load();
        crate::select_preview::set_volume(settings.volume);
        crate::select_preview::gesture();
        let (first, caption) = match &scene.start {
            Some(c) => ("start", c.clone()),
            None => ("select", scene.caption.clone()),
        };
        let n = self.begin_scene(line, said, first, scene.choose, &caption, scene.mark, true);
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let menu: Vec<Line> = Line::all()
                .into_iter()
                .filter(|l| l.spoken() == Spoken::Menu)
                .collect();
            let voiced = match said {
                Said::Draft => Ok(this.staged_voice(
                    |l| l.spoken() == Spoken::Menu,
                    crate::select_preview::make_buffer,
                )),
                Said::Example(set) => this.example_voice(set, &menu).await,
            };
            if !this.scene_is(n) {
                return;
            }
            let (voice, lengths, words) = match voiced {
                Ok(v) => v,
                Err(e) => return this.drop_scene(n, format!("The example could not be read: {e}")),
            };
            this.keep(n, &lengths, words);
            let told: crate::announcers::Told = {
                let weak = Rc::downgrade(&this);
                Rc::new(move |asked, spoke| {
                    if let Some(s) = weak.upgrade() {
                        s.told(n, asked, spoke);
                    }
                })
            };
            crate::announcers::stage(Some(crate::announcers::Staged {
                voice,
                share: settings.announcer_volume,
                told,
            }));
            let play: PlayFn = {
                let weak = Rc::downgrade(&this);
                Rc::new(move |_, _, _, _| {
                    if let Some(s) = weak.upgrade() {
                        s.chosen(n);
                    }
                })
            };
            let options: OpenFn = Rc::new(|_, _| {});
            let shell: ShellFn = {
                let weak = Rc::downgrade(&this);
                Rc::new(move |call, cx| {
                    if let Some(s) = weak.upgrade() {
                        s.select_call(n, call, cx);
                    }
                })
            };
            let select =
                SelectScreen::new(env.clone(), scene.data, Some(&scene.song), play, options)
                    .with_shell(shell, Rc::default());
            let frame = this.frame(env.clone(), n, &caption);
            let intro = this.length(n, Line::SelectMusicIntro);
            let shown = match &scene.start {
                Some(_) => {
                    // Singleplayer brings song select in, as on the start
                    // menu.
                    let next = Rc::new(RefCell::new(Some(select)));
                    let weak = Rc::downgrade(&this);
                    let caption = scene.caption.clone();
                    let action: start::Action = Rc::new(move |choice, cx| {
                        if choice == start::Choice::Singleplayer
                            && let Some(s) = weak.upgrade()
                        {
                            s.singleplayer(n, &next, &caption, cx);
                        }
                    });
                    let start = start::StartScreen::new(env, false, action)
                        .with_focus(Some(start::Choice::Singleplayer.id().into()));
                    this.show(frame, start, Some(scene.mark))
                }
                None => this.show(frame, select, None),
            };
            if !shown {
                return this.drop_scene(n, "Song select could not be shown.".into());
            }
            this.with_scene(n, |sc| sc.marked = scene.start.is_some());
            if scene.start.is_some() {
                this.after(n, START_SECONDS, Next::Start);
                this.until(n, START_SECONDS + ENTRANCE + intro + SCENE_TAIL);
            } else if scene.choose {
                // Should nothing be said as it shows: on regardless.
                let comment = this.length(n, line);
                this.later(n, 3.0 - MARK_LEAD, move |s| s.mark(n));
                this.after(n, 3.0, Next::Start);
                this.until(
                    n,
                    ENTRANCE + (intro + MARK_LEAD).max(3.0) + comment + SCENE_TAIL,
                );
            } else {
                this.after(n, 3.0, Next::End);
            }
            this.publish();
        });
    }

    /// The welcome's start menu chose Singleplayer (scene `n`, in `cx`):
    /// song select `next` comes in, as on the start menu; the scene
    /// leaves instead where its way out was asked for meanwhile.
    fn singleplayer(
        self: &Rc<Self>,
        n: u64,
        next: &RefCell<Option<SelectScreen>>,
        caption: &str,
        cx: &mut Cx,
    ) {
        let leaving = self.with_scene(n, |sc| sc.leaving).unwrap_or(true);
        if leaving {
            return self.leave_scene(n, cx);
        }
        let Some(select) = next.borrow_mut().take() else {
            return;
        };
        cx.replace(select);
        self.with_scene(n, |sc| {
            sc.screen = "select";
            sc.timer = None;
        });
        self.caption_now(n, caption);
        // Should nothing be said as it shows: on regardless.
        self.after(n, 3.0 + ENTRANCE, Next::End);
        self.publish();
    }

    /// What song select's scene `n` asks.
    fn select_call(self: &Rc<Self>, n: u64, call: SelectCall, cx: &mut Cx) {
        match call {
            SelectCall::Selected { song, .. } => {
                let entry =
                    song.and_then(|id| self.songs.borrow().iter().find(|e| e.id == id).cloned());
                crate::select_preview::select(entry);
            }
            SelectCall::Shown(false) => crate::select_preview::stop(),
            SelectCall::Volume(v) => crate::select_preview::set_volume(v),
            SelectCall::Leave => self.leave_scene(n, cx),
            _ => {}
        }
    }

    /// Scene `n`'s chart chosen (song select's Start): as a play opening
    /// takes song select's sound, the preview and the intro stop; the
    /// comment comes next (`UiRequest::Announce`, [`Studio::told`]).
    fn chosen(&self, n: u64) {
        self.with_scene(n, |sc| {
            sc.chosen = true;
            sc.timer = None;
        });
        crate::select_preview::hush();
        self.publish();
    }

    /// A line of song select's scene `n` asked for (`asked`), and what
    /// was said for it and how long: the scene's next step after it (a
    /// comment's chart pointed at once the intro is said, then chosen).
    fn told(self: &Rc<Self>, n: u64, asked: Line, spoke: Option<(Line, f64)>) {
        let Some((chosen, choose)) = self.with_scene(n, |sc| {
            if let Some((found, _)) = spoke {
                sc.spoken.push((found.folder(), js_sys::Date::now()));
            }
            (sc.chosen, sc.choose)
        }) else {
            return;
        };
        if let Some((found, len)) = spoke {
            self.say(n, found, len);
        }
        let len = spoke.map_or(0.0, |(_, l)| l);
        match asked {
            Line::SelectMusicIntro if chosen => {}
            Line::SelectMusicIntro if choose => {
                // The steps in case nothing was said give way.
                let line = self
                    .with_scene(n, |sc| {
                        sc.later.clear();
                        sc.line
                    })
                    .unwrap_or(asked);
                let at = (len + MARK_LEAD).max(1.8);
                self.later(n, at - MARK_LEAD, move |s| s.mark(n));
                self.after(n, at, Next::Start);
                self.until(n, at + self.length(n, line) + SCENE_TAIL);
            }
            _ => self.after(n, len + SCENE_TAIL, Next::End),
        }
        self.publish();
    }

    /// The results' scene: a play staged for the line, judged by the
    /// player's rules where they give it; its line said on a sound of its
    /// own (made in this press) as the results open, as after a play. A
    /// record's and a first clear's come from song select, showing the
    /// chart's best before the play, the chart chosen on it.
    fn results_scene(self: &Rc<Self>, line: Line, said: Said) {
        let settings = crate::settings::Settings::load();
        let recorded: std::collections::BTreeSet<Line> = self
            .draft
            .borrow()
            .takes
            .iter()
            .filter_map(|t| t.line)
            .collect();
        // Our packs say every line.
        let has = |l: Line| said != Said::Draft || recorded.contains(&l);
        let Some(scene) = scene::results(line, &settings.ruleset, &has) else {
            return;
        };
        let Some(env) = self.env.borrow().clone() else {
            return;
        };
        let Some((audio, _)) = self.new_sound(format!("scene:{}", line.folder())) else {
            return;
        };
        let before = scene.before.is_some();
        if before {
            crate::select_preview::set_volume(settings.volume);
            crate::select_preview::gesture();
        }
        let (first, caption, mark) = match &scene.before {
            Some(b) => ("select", b.caption.clone(), b.mark),
            None => ("results", scene.caption.clone(), scene.mark),
        };
        let n = self.begin_scene(line, said, first, before, &caption, mark, before);
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let voiced = match said {
                Said::Draft => Ok(this.staged_voice(
                    |l| l.spoken() == Spoken::Results,
                    |pcm, rate| audio.borrow().buffer_from_pcm(pcm, f64::from(rate)).ok(),
                )),
                Said::Example(set) => {
                    // The lines the results may say.
                    let ending = scene.ending();
                    let mut lines = ddi_session::announcer::results_lines(
                        Some(scene.prior),
                        &ending,
                        &scene.grades,
                        scene.combo_w4,
                    );
                    if ddi_session::announcer::cheers(&ending, &scene.grades) {
                        lines.push(Line::Cheer);
                    }
                    this.example_voice(set, &lines).await
                }
            };
            if !this.scene_is(n) {
                return;
            }
            let (voice, lengths, words) = match voiced {
                Ok(v) => v,
                Err(e) => return this.drop_scene(n, format!("The example could not be read: {e}")),
            };
            this.keep(n, &lengths, words);
            let back: ActionFn = {
                let weak = Rc::downgrade(&this);
                Rc::new(move |_, cx| {
                    if let Some(s) = weak.upgrade() {
                        s.leave_scene(n, cx);
                    }
                })
            };
            let actions = ResultsActions {
                retry: Some(back.clone()),
                song_list: Some(back),
                replay: None,
                offset: None,
            };
            let results = ResultsScreen::new(env.clone(), scene.data.clone(), actions);
            let frame = this.frame(env.clone(), n, &caption);
            let scene = Rc::new(scene);
            let Some(before) = &scene.before else {
                if !this.show(frame, results, Some(scene.mark)) {
                    return this.drop_scene(n, "The results could not be shown.".into());
                }
                this.with_scene(n, |sc| sc.marked = true);
                this.results_said(n, &scene, &audio, voice);
                return;
            };
            // Song select says nothing in a results line's scene.
            crate::announcers::stage(Some(crate::announcers::Staged {
                voice: Voice::default(),
                share: settings.announcer_volume,
                told: Rc::new(|_, _| {}),
            }));
            let next = Rc::new(RefCell::new(Some((results, voice))));
            let play: PlayFn = {
                let weak = Rc::downgrade(&this);
                let scene = scene.clone();
                let audio = audio.clone();
                Rc::new(move |_, _, _, cx| {
                    if let Some(s) = weak.upgrade() {
                        s.played(n, &next, &scene, &audio, cx);
                    }
                })
            };
            let options: OpenFn = Rc::new(|_, _| {});
            let shell: ShellFn = {
                let weak = Rc::downgrade(&this);
                Rc::new(move |call, cx| {
                    if let Some(s) = weak.upgrade() {
                        s.select_call(n, call, cx);
                    }
                })
            };
            let select =
                SelectScreen::new(env, before.data.clone(), Some(&before.song), play, options)
                    .with_shell(shell, Rc::default());
            if !this.show(frame, select, Some(before.mark)) {
                return this.drop_scene(n, "Song select could not be shown.".into());
            }
            this.with_scene(n, |sc| sc.marked = true);
            this.after(n, BEFORE_SECONDS, Next::Start);
            this.until(
                n,
                BEFORE_SECONDS + 2.0 * ENTRANCE + this.length(n, line) + SCENE_TAIL,
            );
            this.publish();
        });
    }

    /// The chart shown before a results line's play chosen (scene `n`, in
    /// `cx`): its results come in, then their line.
    fn played(
        self: &Rc<Self>,
        n: u64,
        next: &RefCell<Option<(ResultsScreen, Voice<AudioBuffer>)>>,
        scene: &Rc<scene::ResultsScene>,
        audio: &Rc<RefCell<WebAudio>>,
        cx: &mut Cx,
    ) {
        let Some((results, voice)) = next.borrow_mut().take() else {
            return;
        };
        crate::select_preview::hush();
        cx.replace(results);
        self.with_scene(n, |sc| {
            sc.chosen = true;
            sc.screen = "results";
            sc.mark = scene.mark;
            sc.timer = None;
        });
        self.caption_now(n, &scene.caption);
        self.mark(n);
        // Said as the results come in.
        let scene = scene.clone();
        let audio = audio.clone();
        self.later(n, ENTRANCE, move |s| {
            s.results_said(n, &scene, &audio, voice)
        });
        self.publish();
    }

    /// The results' line of scene `n`, said now by `voice` on `audio`, as
    /// [`Announcer::results`] chooses it; the scene ends after it.
    fn results_said(
        self: &Rc<Self>,
        n: u64,
        scene: &scene::ResultsScene,
        audio: &Rc<RefCell<WebAudio>>,
        voice: Voice<AudioBuffer>,
    ) {
        let settings = crate::settings::Settings::load();
        let mut announcer = Announcer::new(
            true,
            settings.announcer_volume,
            Some(scene.prior),
            Timing {
                start: 0.0,
                offset: 0.0,
                rate: 1.0,
            },
            None,
        );
        announcer.set_voice(Rc::new(RefCell::new(voice)));
        let now = audio.borrow().context().current_time();
        announcer.results(
            &mut *audio.borrow_mut(),
            now,
            &scene.ending(),
            &scene.grades,
            scene.combo_w4,
        );
        let spoken = announcer.report.spoken;
        let mut end = now;
        for (f, at) in &spoken {
            let Some(l) = Line::parse(f) else {
                continue;
            };
            let len = self.length(n, l);
            end = end.max(at + len);
            // Its words as it is said.
            self.later(n, at - now, move |s| s.say(n, l, len));
        }
        let wall = js_sys::Date::now();
        self.with_scene(n, |sc| {
            sc.spoken = spoken
                .into_iter()
                .map(|(f, at)| (f, wall + (at - now) * 1000.0))
                .collect();
        });
        self.after(n, end - now + SCENE_TAIL, Next::End);
        self.publish();
    }

    /// Scene `n`'s next step in `seconds`, by the input a player would
    /// give: Start (Singleplayer on the start menu, the chart on song
    /// select), Back to leave (the start menu, which Back does not leave,
    /// leaves as Singleplayer is chosen).
    fn after(self: &Rc<Self>, n: u64, seconds: f64, next: Next) {
        // Long enough to see the screen.
        let seconds = match (next, self.scene.borrow().as_ref()) {
            (Next::End, Some(sc)) => {
                seconds.max(SCENE_SECONDS - (js_sys::Date::now() - sc.began) / 1000.0)
            }
            _ => seconds,
        };
        if matches!(next, Next::End) {
            self.until(n, seconds);
        }
        let weak = Rc::downgrade(self);
        let timer = Timeout::new((seconds.max(0.0) * 1000.0) as u32, move || {
            let Some(s) = weak.upgrade() else {
                return;
            };
            let action = s.with_scene(n, |sc| match next {
                Next::Start if sc.chosen => None,
                Next::Start => Some(MenuAction::Start),
                Next::End | Next::Leave if sc.screen == "start" => {
                    sc.leaving = true;
                    Some(MenuAction::Start)
                }
                Next::End | Next::Leave => Some(MenuAction::Back),
            });
            let (Some(Some(action)), Some(host)) = (action, s.weak_host()) else {
                return;
            };
            for down in [true, false] {
                ui_dev::dispatch(
                    &host,
                    UiEvent::Menu(MenuEvent {
                        action,
                        down,
                        source: u32::MAX,
                        gesture: false,
                        time: 0.0,
                    }),
                );
            }
        });
        self.with_scene(n, |sc| sc.timer = Some(timer));
    }

    /// Scene `n` leaves (its screen closed in `cx`, the frame going):
    /// back in the studio, which says what was heard.
    fn leave_scene(&self, n: u64, cx: &mut Cx) {
        let Some(sc) = self.scene.borrow_mut().take_if(|s| s.n == n) else {
            return;
        };
        cx.close(true);
        cx.broadcast(Message::new(stage::END));
        self.on_ui(|ui| ui.mark(None));
        crate::announcers::stage(None);
        if sc.preview {
            crate::select_preview::close();
        }
        self.stop();
        // What was heard: the lines begun by now (one scheduled later was
        // cut).
        let wall = js_sys::Date::now();
        let heard: Vec<String> = sc
            .spoken
            .into_iter()
            .filter(|(_, at)| *at <= wall)
            .map(|(f, _)| f)
            .collect();
        for l in heard.iter().filter_map(|f| Line::parse(f)) {
            match sc.said {
                Said::Draft => *self.turns.borrow_mut().entry(l).or_insert(0) += 1,
                Said::Example(set) => {
                    *self
                        .example_turns
                        .borrow_mut()
                        .entry((set.key(), l))
                        .or_insert(0) += 1
                }
            }
        }
        let names: Vec<String> = heard
            .iter()
            .filter_map(|f| Line::parse(f))
            .map(|l| format!("“{}”", studio::name(l)))
            .collect();
        self.view.borrow_mut().status = match (names.is_empty(), sc.line) {
            (true, Line::Cheer) => "Nothing was said in the game: the cheer follows the results' \
                                    line of a play graded AA or better, and none of those lines \
                                    is recorded."
                .into(),
            (true, _) => "Nothing was said in the game.".into(),
            (false, _) => match sc.said {
                Said::Draft => format!("Heard in the game: {}.", names.join(", then ")),
                Said::Example(set) => format!(
                    "Heard in the game, an example by {}: {}.",
                    set.pack(),
                    names.join(", then ")
                ),
            },
        };
        *self.heard.borrow_mut() = Some((sc.line, heard, sc.said.key()));
        self.send();
    }

    /// A press on the page, inside its handler.
    fn call(
        self: &Rc<Self>,
        call: PageCall,
        cx: &mut Cx,
    ) -> Option<ddi_ui::screens::page::PageDoc> {
        let id = match call {
            PageCall::Leave => {
                self.stop();
                cx.request(UiRequest::Back);
                return None;
            }
            PageCall::Field { id, text } => {
                if studio::set_field(&mut self.draft.borrow_mut(), &id, &text) {
                    self.forget_export();
                    self.save();
                    self.publish();
                }
                return None;
            }
            PageCall::Button(id) => id,
        };
        let act = studio::act(&id)?;
        match act {
            Act::Add(line) => {
                self.pick_line.set(Some(line));
                if let Some(i) = self.files_input.borrow().as_ref() {
                    let _ = i.set_attribute("accept", RECORDING_TYPES);
                    i.click();
                }
                None
            }
            Act::ImportFiles => {
                self.pick_line.set(None);
                if let Some(i) = self.files_input.borrow().as_ref() {
                    let _ = i.set_attribute("accept", PACK_TYPES);
                    i.click();
                }
                None
            }
            Act::ImportFolder => {
                self.pick_line.set(None);
                if let Some(i) = self.folder_input.borrow().as_ref() {
                    i.click();
                }
                None
            }
            Act::Play(id) => {
                self.play(id);
                None
            }
            Act::Example(line, i) => {
                self.example(line, i);
                None
            }
            Act::Tidy => {
                {
                    let mut d = self.draft.borrow_mut();
                    d.tidy = !d.tidy;
                    for t in &mut d.takes {
                        t.tidy = None;
                    }
                }
                self.forget_export();
                self.save();
                self.publish();
                Some(self.doc())
            }
            Act::Move(id) => {
                // Heard as its line is chosen.
                if self.view.borrow().playing.as_deref() != Some(&format!("take:{id}")) {
                    self.play(id);
                }
                let env = self.env.borrow().clone()?;
                let weak = Rc::downgrade(self);
                let picker = studio::line_picker(
                    env,
                    &self.draft.borrow(),
                    id,
                    Rc::new(move |id, line, _cx| {
                        let Some(s) = weak.upgrade() else {
                            return;
                        };
                        if let Some(t) = s.draft.borrow_mut().take_mut(id) {
                            t.line = line;
                        }
                        s.stop();
                        s.forget_export();
                        s.save();
                        s.send();
                    }),
                )?;
                cx.overlay(picker, true);
                None
            }
            Act::Remove(id) => {
                self.remove(id);
                self.publish();
                Some(self.doc())
            }
            Act::Examples => {
                let next = studio::next_set(self.view.borrow().examples);
                self.view.borrow_mut().examples = next;
                let _ = storage::set(Area::Local, EXAMPLES_KEY, next.key());
                self.refresh_examples();
                self.publish();
                Some(self.doc())
            }
            Act::StepMania => {
                {
                    let mut d = self.draft.borrow_mut();
                    d.stepmania = !d.stepmania;
                }
                self.forget_export();
                self.save();
                self.publish();
                Some(self.doc())
            }
            Act::Details => {
                let mut v = self.view.borrow_mut();
                v.details = !v.details;
                drop(v);
                Some(self.doc())
            }
            Act::Export => {
                self.export();
                None
            }
            Act::Save => {
                self.save_zip();
                self.publish();
                Some(self.doc())
            }
            Act::UseInGame => {
                self.use_in_game();
                None
            }
            Act::Hear(line) => {
                self.hear(line);
                None
            }
        }
    }
}

/// A buffer's channels.
fn channels(buffer: &AudioBuffer) -> Vec<Vec<f32>> {
    (0..buffer.number_of_channels())
        .map(|c| buffer.get_channel_data(c).unwrap_or_default())
        .collect()
}

/// A file's first bytes, for its format.
async fn head_of(blob: &Blob) -> Result<Vec<u8>, String> {
    let end = (blob.size()).min(HEAD as f64);
    let slice = blob
        .slice_with_f64_and_f64(0.0, end)
        .map_err(|e| crate::web::js_err("Blob.slice", e))?;
    files::blob_bytes(&slice).await
}

/// The files in a zip, each with its path in it under the zip's own name
/// (an announcer at its root is named after it).
async fn zip_files(p: &PickedFile) -> Result<Vec<(String, Blob)>, String> {
    let stem = ddi_library::zip::archive_stem(&p.path)
        .unwrap_or(&p.path)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();
    let entries = crate::import::zip_entries(&p.file).await?;
    let mut out = Vec::new();
    for e in entries.iter().filter(|e| !e.is_dir && !e.encrypted) {
        let blob = crate::import::zip_entry_blob(&p.file, &e.name).await?;
        out.push((format!("{stem}/{}", e.name), blob));
    }
    Ok(out)
}

impl Component for StudioPage {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        StudioPage {
            root: NodeRef::default(),
            studio: Studio::new(),
            started: None,
            failure: None,
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.start(ctx);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Failed(e) => {
                self.failure = Some(e);
                if let Some((host, _, pads)) = &self.started {
                    ui_dev::stop_ui(host, pads.as_ref());
                }
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        if let Some(e) = &self.failure {
            crate::component::no_gpu(ctx.root(), e);
            return;
        }
        ctx.root().set_class_name("select-page");
        self.root.set(ctx.root());
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.studio.stop();
        if let Some((host, _, pads)) = &self.started {
            ui_dev::stop_ui(host, pads.as_ref());
        }
        crate::web::forget_debug(&["ui", "studio"]);
    }
}

impl StudioPage {
    fn start(&mut self, ctx: &Context<Self>) {
        let Some(el) = self.root.cast::<HtmlElement>() else {
            return;
        };
        let studio = self.studio.clone();
        let theme = ui_dev::theme_host(Rc::new(ui_dev::nfkd));
        let env = theme.env().clone();
        *studio.env.borrow_mut() = Some(env.clone());
        let shell: PageShell = {
            let weak = Rc::downgrade(&studio);
            Rc::new(move |call, cx| weak.upgrade()?.call(call, cx))
        };
        let screen =
            PageScreen::new(env, studio::NAME, studio.doc()).with_shell(shell, studio.feed.clone());
        let mut ui = ui_dev::theme_ui();
        ui.push(screen);
        let mut listeners = Vec::new();
        // The pickers: files (a zip too), and a folder.
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            for folder in [false, true] {
                let Some(input) = doc
                    .create_element("input")
                    .ok()
                    .and_then(|e| e.dyn_into::<HtmlInputElement>().ok())
                else {
                    continue;
                };
                input.set_type("file");
                input.set_multiple(true);
                if folder {
                    let _ = input.set_attribute("webkitdirectory", "");
                } else {
                    let _ = input.set_attribute("accept", RECORDING_TYPES);
                }
                let _ = input.set_attribute("tabindex", "-1");
                let _ = input.set_attribute("aria-hidden", "true");
                let _ = input.set_attribute(
                    "data-ddi",
                    if folder {
                        "studio-folder"
                    } else {
                        "studio-files"
                    },
                );
                input.set_hidden(true);
                let _ = el.append_child(&input);
                let weak = Rc::downgrade(&studio);
                let i = input.clone();
                listeners.push(EventListener::new(&input, "change", move |_| {
                    let picked = files::from_input(&i);
                    i.set_value("");
                    if let Some(s) = weak.upgrade()
                        && !picked.is_empty()
                    {
                        let line = s.pick_line.take();
                        s.add(picked, line);
                    }
                }));
                if folder {
                    *studio.folder_input.borrow_mut() = Some(input);
                } else {
                    *studio.files_input.borrow_mut() = Some(input);
                }
            }
        }
        // Drops anywhere: on a line's zone they go on that line.
        if let Some(window) = web_sys::window() {
            let opts = EventListenerOptions::enable_prevent_default();
            let carries_files = |e: &DragEvent| {
                e.data_transfer()
                    .is_some_and(|dt| dt.types().includes(&"Files".into(), 0))
            };
            let weak = Rc::downgrade(&studio);
            listeners.push(EventListener::new_with_options(
                &window,
                "dragover",
                opts,
                move |e| {
                    let Some(e) = e.dyn_ref::<DragEvent>().filter(|e| carries_files(e)) else {
                        return;
                    };
                    // Without this the browser opens the file instead.
                    e.prevent_default();
                    if let Some(s) = weak.upgrade() {
                        let zone = s.zone_at([f64::from(e.client_x()), f64::from(e.client_y())]);
                        s.light(zone);
                    }
                },
            ));
            let weak = Rc::downgrade(&studio);
            listeners.push(EventListener::new(&window, "dragleave", move |e| {
                // Out of the window: nothing lit.
                if let Some(e) = e.dyn_ref::<DragEvent>()
                    && e.related_target().is_none()
                    && let Some(s) = weak.upgrade()
                {
                    s.light(None);
                }
            }));
            let weak = Rc::downgrade(&studio);
            listeners.push(EventListener::new_with_options(
                &window,
                "drop",
                opts,
                move |e| {
                    let Some(e) = e.dyn_ref::<DragEvent>() else {
                        return;
                    };
                    let Some(dt) = e.data_transfer().filter(|_| carries_files(e)) else {
                        return;
                    };
                    e.prevent_default();
                    let Some(s) = weak.upgrade() else {
                        return;
                    };
                    let zone = s.zone_at([f64::from(e.client_x()), f64::from(e.client_y())]);
                    s.light(None);
                    let line = zone.as_deref().and_then(Line::parse);
                    let entries = files::drop_entries(&dt);
                    if entries.is_empty() {
                        return;
                    }
                    let _ = crate::web::storage::request_persist();
                    wasm_bindgen_futures::spawn_local(async move {
                        match entries.collect().await {
                            Ok(picked) => s.add(picked, line),
                            Err(e) => s.status(format!("The files could not be read: {e}")),
                        }
                    });
                },
            ));
        }
        let extra: ui_dev::DebugFn = {
            let weak = Rc::downgrade(&studio);
            Box::new(move |obj| {
                if let Some(s) = weak.upgrade() {
                    s.debug_into(obj);
                }
            })
        };
        match ui_dev::start_ui(&el, ui, Some(theme), Some(extra)) {
            Ok(mut started) => {
                *studio.host.borrow_mut() = Some(Rc::downgrade(&started.0));
                let link = ctx.link().clone();
                ui_dev::attach_gfx(started.0.clone(), move |e| {
                    link.send_message(Msg::Failed(e))
                });
                started.1.extend(listeners);
                self.started = Some(started);
                let _ = crate::web::storage::request_persist();
                wasm_bindgen_futures::spawn_local(async move { studio.load().await });
            }
            Err(e) => ctx.link().send_message(Msg::Failed(e)),
        }
    }
}
