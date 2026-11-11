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
//!   and encoded as Opus, the browser's encoder first, `web::opus`), then
//!   Save downloads it in its press. **Use in
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

use ddi_l10n::UiText;
use ddi_l10n::msg::studio as ms;
use ddi_library::announcer::draft::{
    Adopted, Draft, HEAD, Sound, Write, install_plan, made_before, takes,
};
use ddi_library::announcer::examples::Set;
use ddi_library::announcer::{Bundled, Line, OUR_COMBOS, Spoken, tidy};
use ddi_platform::AudioBackend;
use ddi_session::announcer::scene::director::{
    self, Director, Pin, Said, SceneHost, StagedVoice, Timer, UiFn, Voiced,
};
use ddi_session::announcer::{Voice, scene};
use ddi_ui::event::{MenuEvent, UiEvent};
use ddi_ui::message::{Message, Param};
use ddi_ui::screens::errors::error_text;
use ddi_ui::screens::page::{PAGE_CHANGED, PAGE_TARGET, PageCall, PageFeed, PageScreen, PageShell};
use ddi_ui::screens::studio::{self, Act, Applied, Example, View};
use ddi_ui::{Cx, MenuAction, UiRequest};
use gloo::events::{EventListener, EventListenerOptions};
use gloo::render::{AnimationFrame, request_animation_frame};
use gloo::timers::callback::Timeout;
use js_sys::Reflect;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{AudioBuffer, Blob, DragEvent, HtmlInputElement};

use crate::component::{Component, Context};
use crate::components::ui_dev::{self, Host, UiPage};
use crate::songs::ManifestEntry;
use crate::web::audio::{AudioDecoder, WebAudio};
use crate::web::files::{self, PickedFile, carries_files};
use crate::web::idb::{self, Db};

const DRAFT_KEY: &str = "studio/draft.json";
/// The rate takes are decoded, measured and written at.
const RATE: f32 = 48_000.0;
/// The draft is saved at most this often while takes are decoded,
/// milliseconds.
const SAVE_EVERY_MS: f64 = 1000.0;

/// What a line's Add recording picks.
const RECORDING_TYPES: &str = "audio/*,.m4a,.opus,.flac,.caf,.aif,.aiff,.webm";
/// What Import a pack from a zip picks.
const PACK_TYPES: &str = ".zip,.smzip";

fn take_key(id: u64) -> String {
    format!("studio/take/{id}")
}

pub(crate) enum Msg {
    Failed(String),
}

/// What the studio keeps for the session while a scene in play shows on
/// the play route (the page goes, and comes again after): each line's next
/// recording and example, and our packs' sounds decoded.
#[derive(Default)]
struct Kept {
    decoded: HashMap<u64, AudioBuffer>,
    turns: HashMap<Line, usize>,
    example_turns: HashMap<(&'static str, Line), usize>,
    example_sounds: HashMap<String, AudioBuffer>,
    /// Where the page was (a scene in play comes back to it).
    place: ddi_ui::screens::page::SharedPlace,
}

thread_local! {
    static KEPT: RefCell<Kept> = RefCell::default();
}

impl Drop for Studio {
    fn drop(&mut self) {
        KEPT.with_borrow_mut(|k| {
            k.decoded = std::mem::take(&mut *self.decoded.borrow_mut());
            k.turns = std::mem::take(&mut *self.director.turns.borrow_mut());
            k.example_turns = std::mem::take(&mut *self.director.example_turns.borrow_mut());
            k.example_sounds = std::mem::take(&mut *self.example_sounds.borrow_mut());
            k.place = self.place.clone();
        });
    }
}

pub(crate) struct StudioPage {
    ui: UiPage,
    studio: Rc<Studio>,
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
    /// The pickers, and the line the next pick goes on (and the text
    /// line it is recorded for).
    files_input: RefCell<Option<HtmlInputElement>>,
    folder_input: RefCell<Option<HtmlInputElement>>,
    pick_line: Cell<Option<Line>>,
    pick_script: Cell<Option<u64>>,
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
    /// Runs the scenes (`ddi_session::announcer::scene::director`).
    director: Rc<Director<WebScenes>>,
    /// The last scene's line, what it said and who (the debug state).
    heard: RefCell<Option<(Line, Vec<String>, String)>>,
    /// Our packs' sounds a scene took, decoded (by pack and path).
    example_sounds: RefCell<HashMap<String, AudioBuffer>>,
    /// Where the page is, kept across a scene in play.
    place: ddi_ui::screens::page::SharedPlace,
    /// `__DDI_DEBUG.studio` as last built; `None` once the studio changed
    /// since (each frame's debug state takes it as it is).
    debug: RefCell<Option<js_sys::Object>>,
    /// The page is to be shown again at the next animation frame
    /// ([`Studio::send_soon`]).
    frame: RefCell<Option<AnimationFrame>>,
}

impl Studio {
    fn new() -> Rc<Studio> {
        let examples = studio::remembered_set();
        let studio = Rc::new_cyclic(|me: &Weak<Studio>| Studio {
            draft: RefCell::new(Draft::default()),
            view: RefCell::new(View {
                examples,
                ..View::default()
            }),
            blobs: RefCell::default(),
            decoded: RefCell::new(KEPT.with_borrow_mut(|k| std::mem::take(&mut k.decoded))),
            decoding: Cell::new(false),
            ours: RefCell::default(),
            examples: RefCell::default(),
            feed: Rc::new(RefCell::new(None)),
            host: RefCell::new(None),
            env: RefCell::new(None),
            files_input: RefCell::new(None),
            folder_input: RefCell::new(None),
            pick_line: Cell::new(None),
            pick_script: Cell::new(None),
            sound: RefCell::new(None),
            zip: RefCell::new(None),
            target: RefCell::new(None),
            plays: Cell::new(0),
            songs: RefCell::default(),
            director: Director::new(WebScenes(me.clone())),
            heard: RefCell::new(None),
            example_sounds: RefCell::new(
                KEPT.with_borrow_mut(|k| std::mem::take(&mut k.example_sounds)),
            ),
            place: KEPT.with_borrow(|k| k.place.clone()),
            debug: RefCell::new(None),
            frame: RefCell::new(None),
        });
        KEPT.with_borrow_mut(|k| {
            *studio.director.turns.borrow_mut() = std::mem::take(&mut k.turns);
            *studio.director.example_turns.borrow_mut() = std::mem::take(&mut k.example_turns);
        });
        studio
    }

    fn doc(&self) -> ddi_ui::screens::page::PageDoc {
        studio::doc(
            &self.draft.borrow(),
            &self.view.borrow(),
            &self.examples.borrow(),
            ddi_ui::screens::data::Platform::Web,
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

    /// [`Studio::send`] at the next animation frame, however often asked
    /// before it (each take decoded).
    fn send_soon(self: &Rc<Self>) {
        if self.frame.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        let frame = request_animation_frame(move |_| {
            if let Some(s) = weak.upgrade() {
                s.frame.borrow_mut().take();
                s.send();
            }
        });
        *self.frame.borrow_mut() = Some(frame);
    }

    fn status(&self, text: impl Into<String>) {
        self.view.borrow_mut().status = text.into();
        self.send();
    }

    /// The debug state made again: the studio changed.
    fn publish(&self) {
        self.changed();
        if let Some(debug) = crate::web::debug_object() {
            self.debug_into(&debug);
        }
    }

    /// The debug state is to be made again when next asked.
    fn changed(&self) {
        self.debug.borrow_mut().take();
    }

    /// `__DDI_DEBUG.studio` (the host writes its debug object anew each
    /// frame; this goes in each time, [`ui_dev::DebugFn`]), made again
    /// only after the studio changed.
    fn debug_into(&self, debug: &js_sys::Object) {
        let built = self
            .debug
            .borrow_mut()
            .get_or_insert_with(|| self.studio_debug())
            .clone();
        let _ = Reflect::set(debug, &"studio".into(), &built);
    }

    /// What `__DDI_DEBUG.studio` says.
    fn studio_debug(&self) -> js_sys::Object {
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
                Write::Opus { tidy: true } => "opus-tidied",
                Write::Opus { tidy: false } => "opus",
                Write::Wav { tidy: true } => "wav-tidied",
                Write::Wav { tidy: false } => "wav",
            };
            set(&x, "write", write.into());
            set(&x, "format", format!("{:?}", t.format).into());
            set(
                &x,
                "words",
                t.words.as_deref().map_or(JsValue::NULL, JsValue::from),
            );
            takes.push(&x);
        }
        set(&o, "takes", takes.into());
        let scripts = js_sys::Array::new();
        for s in &draft.scripts {
            let x = js_sys::Object::new();
            set(&x, "id", (s.id as f64).into());
            set(&x, "line", s.line.folder().into());
            set(&x, "words", s.words.as_str().into());
            scripts.push(&x);
        }
        set(&o, "scripts", scripts.into());
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
        let (by_browser, by_ours) = crate::web::opus::encoded();
        set(&o, "opusByBrowser", by_browser.into());
        set(&o, "opusByOurs", by_ours.into());
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
        let scene = match self.director.state() {
            Some(sc) => {
                let x = js_sys::Object::new();
                set(&x, "line", sc.line.folder().into());
                set(&x, "said", sc.said.key().into());
                set(
                    &x,
                    "pin",
                    self.director
                        .pin_key()
                        .map_or(JsValue::NULL, |p| p.as_str().into()),
                );
                set(&x, "screen", sc.screen.key().into());
                set(&x, "caption", sc.caption.as_str().into());
                set(&x, "chosen", sc.chosen.into());
                set(
                    &x,
                    "mark",
                    sc.mark.map_or(JsValue::NULL, |m| m.as_str().into()),
                );
                set(&x, "spoken", folders(&sc.spoken));
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
        o
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
            self.status(ms::status_draft_unreadable(error_text(&e)));
        }
        match crate::announcers::bundled().await {
            Ok(ours) => *self.ours.borrow_mut() = ours,
            Err(e) => web_sys::console::warn_1(&format!("studio: our announcers: {e}").into()),
        }
        match crate::songs::load_manifest().await {
            Ok(m) => *self.songs.borrow_mut() = m.songs,
            Err(e) => web_sys::console::warn_1(&format!("studio: the songs: {e}").into()),
        }
        self.heard_in_play();
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
    /// passed over, each on `line` when given (else by its name), the
    /// first in text line `script`'s place when given.
    fn add(self: &Rc<Self>, picked: Vec<PickedFile>, line: Option<Line>, script: Option<u64>) {
        let mut script = script;
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let count = picked.len();
            this.status(ms::status_adding(count));
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
            let mut readme = None;
            if let Some((name, path)) = made_before(&listed) {
                let blob = path
                    .as_ref()
                    .and_then(|r| files.iter().find(|(p, _)| p == r));
                let text = match blob {
                    Some((_, blob)) => wasm_bindgen_futures::JsFuture::from(blob.text())
                        .await
                        .ok()
                        .and_then(|t| t.as_string()),
                    None => None,
                };
                readme = path;
                this.draft
                    .borrow_mut()
                    .adopt(Adopted::new(name, text.as_deref()));
            }
            let mut writes = Vec::new();
            let mut added = 0;
            let mut passed = 0;
            for (path, blob) in files {
                let head = head_of(&blob).await.unwrap_or_default();
                if !takes(&path, &head) {
                    if readme.as_deref() != Some(path.as_str()) {
                        passed += 1;
                    }
                    continue;
                }
                let id = {
                    let mut d = this.draft.borrow_mut();
                    let id = match script.take() {
                        Some(s) => d.add_to(s, &path, blob.size() as u64, &head),
                        None => d.add(&path, blob.size() as u64, &head),
                    };
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
            let said = match stored {
                Ok(_) => ms::status_added(added, passed),
                Err(e) => ms::status_added_unkept(
                    added,
                    passed,
                    error_text(&e),
                    ddi_ui::screens::data::Platform::Web.l10n(),
                ),
            };
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
                this.status(ms::status_no_decoder());
                return;
            };
            // Saved and shown now and then while they come in (the work
            // each grows with the takes), and once at the end.
            let mut saved = js_sys::Date::now();
            let mut any = false;
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
                any = true;
                let now = js_sys::Date::now();
                if now - saved >= SAVE_EVERY_MS {
                    saved = now;
                    this.save();
                }
                this.send_soon();
            }
            this.decoding.set(false);
            if any {
                this.save();
                this.send();
            }
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
                self.view.borrow_mut().status = ms::status_no_sound(error_text(&e)).into();
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
        self.changed();
        Some((audio, n))
    }

    fn stop(&self) {
        self.sound.borrow_mut().take();
        self.view.borrow_mut().playing = None;
        self.changed();
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
                this.status(ms::status_unplayable(error_text(&e)));
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
                this.status(ms::status_example_unplayable(error_text(&e)));
            }
        });
    }

    /// An export made before no longer says what the draft does.
    fn forget_export(&self) {
        self.zip.borrow_mut().take();
        self.view.borrow_mut().ready = None;
        self.changed();
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
                Write::Opus { tidy } => {
                    let (pcm, rate) = self.pcm(e.take, tidy).await?;
                    let file = crate::web::opus::encode(&pcm, rate)
                        .await
                        .map_err(|x| format!("{}: {x}", e.path))?;
                    files::bytes_blob(&file, "audio/ogg")?
                }
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
        self.status(ms::status_exporting());
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
                    this.status(ms::status_exported(&name, n));
                }
                Err(e) => this.status(ms::status_not_exported(error_text(&e))),
            }
        });
    }

    /// The zip made, downloaded (inside the press).
    fn save_zip(&self) {
        let Some((name, blob)) = self.zip.borrow().clone() else {
            return;
        };
        self.view.borrow_mut().status = match files::save_blob(&blob, &name) {
            Ok(()) => ms::status_saved(&name, ddi_ui::screens::data::Platform::Web.l10n()),
            Err(_) => ms::status_not_saved(),
        }
        .into();
    }

    /// The pack stored as an imported announcer, to choose under Options.
    fn use_in_game(self: &Rc<Self>) {
        self.view.borrow_mut().busy = true;
        self.status(ms::status_using());
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let stored = async {
                let (_, files, _) = this.files().await?;
                let listed: Vec<(String, u64)> = files
                    .iter()
                    .map(|(p, b)| (p.clone(), b.size() as u64))
                    .collect();
                let plan = install_plan(&listed)?;
                let sounds: Vec<(String, Blob)> = files
                    .into_iter()
                    .map(|(p, b)| (plan.inside(&p).to_string(), b))
                    .collect();
                let writes = crate::announcers::store(&plan.kept, sounds)?;
                Db::open().await?.write(writes).await?;
                Ok::<_, String>(plan.kept.name)
            }
            .await;
            this.view.borrow_mut().busy = false;
            match stored {
                Ok(name) => this.status(ms::status_used(name)),
                Err(e) => this.status(ms::status_not_used(error_text(&e))),
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

    /// The recordings of the lines `which` keeps decoded (those not yet:
    /// after the page opened again), before a scene's voice is made of
    /// them; one that cannot be is left out.
    async fn decode_lines(&self, which: impl Fn(Line) -> bool) {
        let ids: Vec<u64> = self
            .draft
            .borrow()
            .takes
            .iter()
            .filter(|t| t.line.is_some_and(&which) && matches!(t.sound, Sound::Read(_)))
            .map(|t| t.id)
            .filter(|id| !self.decoded.borrow().contains_key(id))
            .collect();
        for id in ids {
            let _ = self.buffer(id).await;
        }
    }

    /// The draft as a voice for the lines `which` keeps: each line's
    /// recordings decoded so far, as the pack has them (tidied where they
    /// are), each made a buffer by `make`.
    fn staged_voice(
        &self,
        which: impl Fn(Line) -> bool,
        make: impl Fn(Vec<Vec<f32>>, u32) -> Option<AudioBuffer>,
    ) -> Voiced<AudioBuffer> {
        let draft = self.draft.borrow();
        director::draft_voice(
            &draft,
            which,
            &self.director.turns.borrow(),
            self.director.pinned_take(),
            &self.decoded.borrow(),
            |t, b| {
                let (pcm, rate) = self.shaped(t.id, b, draft.tidies(t));
                make(pcm, rate)
            },
            AudioBuffer::duration,
        )
    }

    /// Hears `line` in the game, in the press: its scene over the studio,
    /// said by the draft where it has something to say for the line, else
    /// by our pack of the examples chosen; `pin`, the recording or example
    /// pressed beside, said for it.
    fn hear(self: &Rc<Self>, line: Line, pin: Option<Pin>) {
        if self.director.busy() {
            return;
        }
        self.stop();
        let said =
            self.director
                .hearing(line, pin, &self.draft.borrow(), self.view.borrow().examples);
        match line.spoken() {
            Spoken::Menu => self.menu_scene(line, said),
            Spoken::Results => self.results_scene(line, said),
            Spoken::Play => self.play_scene(line, said),
            // No scene for a multiplayer battle's lines yet (the page
            // offers none to hear: `studio::hearable`).
            Spoken::Battle => {}
        }
        self.publish();
    }

    /// A line said in play: its scene is the real play
    /// (`docs/plans/announcer-studio.md`, step 4), staged on its chart
    /// (`in_play::pick`) on the play route, which shows it in the frame;
    /// the play's sound made in this press and kept for it, the voice
    /// (the draft's play and results lines, or our pack's) handed over
    /// (`crate::studio_scene`). The studio says what was heard once it
    /// shows again ([`Studio::heard_in_play`]).
    fn play_scene(self: &Rc<Self>, line: Line, said: Said) {
        use ddi_session::announcer::scene::in_play;
        let Some(pick) = in_play::pick(line) else {
            return;
        };
        let Some(chart) = in_play::chart_index(&pick, &self.songs.borrow()) else {
            return self.status(ms::status_no_play());
        };
        // The play's sound, from this press (the play starts on it).
        let audio = match WebAudio::new() {
            Ok(a) => a,
            Err(e) => return self.status(ms::status_no_sound(error_text(&e))),
        };
        audio.resume_now();
        crate::web::audio::keep_for_play(pick.song, chart, audio);
        let n = self.director.begin_play(line, said);
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            // Our packs' combos past 1000 too (an example's voice).
            let lines: Vec<Line> = Line::all()
                .into_iter()
                .chain(OUR_COMBOS.map(Line::Combo))
                .filter(|l| matches!(l.spoken(), Spoken::Play | Spoken::Results))
                .collect();
            let voiced = match said {
                Said::Draft => {
                    let which = |l: Line| matches!(l.spoken(), Spoken::Play | Spoken::Results);
                    this.decode_lines(which).await;
                    // Made on a context of its own, gone after: a buffer
                    // plays on any (the play's).
                    let maker = WebAudio::new().ok();
                    Ok(this.staged_voice(which, |pcm, rate| {
                        maker.as_ref()?.buffer_from_pcm(pcm, f64::from(rate)).ok()
                    }))
                }
                Said::Example(set) => this.example_voice(set, &lines).await,
            };
            if !this.director.is(n) {
                return;
            }
            let (voice, lengths, words) = match voiced {
                Ok(v) => v,
                Err(e) => {
                    return this
                        .director
                        .give_up(n, ms::status_example_unreadable(error_text(&e)));
                }
            };
            crate::studio_scene::stage(director::StagedPlay {
                line,
                said,
                voice_label: said.label().to_string(),
                name: studio::name(line).to_string(),
                voice,
                lengths,
                words,
            });
            this.director.handed_over(n);
            crate::router::Route::Play(crate::router::PlayRoute {
                scene: Some(line.folder()),
                ..crate::router::PlayRoute::song(pick.song, chart)
            })
            .navigate();
        });
    }

    /// What the last scene in play said, as the studio shows again.
    fn heard_in_play(&self) {
        if let Some(h) = crate::studio_scene::take_heard() {
            self.director.heard(h);
        }
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

    /// Our pack of `set` as a voice for `lines` (each as the first of its
    /// chain the pack has): one sound of each, the line's next each scene,
    /// fetched and decoded once a session; with each one's length and
    /// words.
    async fn example_voice(&self, set: Set, lines: &[Line]) -> Result<Voiced<AudioBuffer>, String> {
        let pack = self
            .pack_of(set)
            .ok_or_else(|| format!("{} is not here", set.pack()))?;
        let plan = self.director.example_plan(&pack, set, lines);
        let mut decoder = None;
        for p in &plan {
            if self.example_sounds.borrow().contains_key(&p.key) {
                continue;
            }
            let bytes = crate::announcers::bundled_sound(&pack, &p.path).await?;
            if decoder.is_none() {
                decoder = Some(
                    AudioDecoder::offline(RATE).ok_or("this browser cannot decode sounds here")?,
                );
            }
            let b = decoder
                .as_ref()
                .ok_or("no decoder")?
                .decode(&bytes, Default::default())
                .await?;
            self.example_sounds.borrow_mut().insert(p.key.clone(), b);
        }
        let sounds = self.example_sounds.borrow();
        Ok(director::voiced_from(
            plan,
            |key| sounds.get(key).cloned(),
            AudioBuffer::duration,
        ))
    }

    /// Song select's scene: its voice got ready (made on song select's own
    /// sound, opened in this press), then the director shows it.
    fn menu_scene(self: &Rc<Self>, line: Line, said: Said) {
        let Some((n, scene)) = self.director.begin_menu(line, said) else {
            return;
        };
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let menu: Vec<Line> = Line::all()
                .into_iter()
                .filter(|l| l.spoken() == Spoken::Menu)
                .collect();
            let voiced = match said {
                Said::Draft => {
                    let which = |l: Line| l.spoken() == Spoken::Menu;
                    this.decode_lines(which).await;
                    Ok(this.staged_voice(which, crate::select_preview::make_buffer))
                }
                Said::Example(set) => this.example_voice(set, &menu).await,
            };
            if !this.director.is(n) {
                return;
            }
            match voiced {
                Ok(v) => this.director.show_menu(n, scene, v),
                Err(e) => this
                    .director
                    .give_up(n, ms::status_example_unreadable(error_text(&e))),
            }
        });
    }

    /// The results' scene: its voice got ready, then the director shows
    /// it (its line said on a sound made in this press).
    fn results_scene(self: &Rc<Self>, line: Line, said: Said) {
        let Some((n, scene)) = self
            .director
            .begin_results(line, said, &self.draft.borrow())
        else {
            return;
        };
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let voiced = match said {
                Said::Draft => {
                    let which = |l: Line| l.spoken() == Spoken::Results;
                    this.decode_lines(which).await;
                    let audio = this.sound.borrow().clone();
                    Ok(this.staged_voice(which, |pcm, rate| {
                        audio
                            .as_ref()?
                            .borrow()
                            .buffer_from_pcm(pcm, f64::from(rate))
                            .ok()
                    }))
                }
                Said::Example(set) => {
                    this.example_voice(set, &director::results_said_lines(&scene))
                        .await
                }
            };
            if !this.director.is(n) {
                return;
            }
            match voiced {
                Ok(v) => this.director.show_results(n, scene, v),
                Err(e) => this
                    .director
                    .give_up(n, ms::status_example_unreadable(error_text(&e))),
            }
        });
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
        let applied = studio::apply(
            act,
            &mut self.draft.borrow_mut(),
            &mut self.view.borrow_mut(),
        );
        if let Some(applied) = applied {
            match applied {
                Applied::Draft => {
                    self.forget_export();
                    self.save();
                    self.publish();
                }
                Applied::Examples => {
                    self.refresh_examples();
                    self.publish();
                }
                Applied::View => {}
            }
            return Some(self.doc());
        }
        match act {
            // ([`studio::apply`]'s.)
            Act::Tidy
            | Act::StepMania
            | Act::Examples
            | Act::Details
            | Act::RemoveCombo(_)
            | Act::Write(_)
            | Act::Unwrite(_) => None,
            Act::Add(_) | Act::AddTo(_) => {
                let (line, script) = match act {
                    Act::AddTo(id) => {
                        let line = self
                            .draft
                            .borrow()
                            .scripts
                            .iter()
                            .find(|s| s.id == id)?
                            .line;
                        (line, Some(id))
                    }
                    Act::Add(line) => (line, None),
                    _ => return None,
                };
                self.pick_line.set(Some(line));
                self.pick_script.set(script);
                if let Some(i) = self.files_input.borrow().as_ref() {
                    let _ = i.set_attribute("accept", RECORDING_TYPES);
                    i.click();
                }
                None
            }
            Act::ImportFiles => {
                self.pick_line.set(None);
                self.pick_script.set(None);
                if let Some(i) = self.files_input.borrow().as_ref() {
                    let _ = i.set_attribute("accept", PACK_TYPES);
                    i.click();
                }
                None
            }
            Act::ImportFolder => {
                self.pick_line.set(None);
                self.pick_script.set(None);
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
            Act::AddCombo => {
                let env = self.env.borrow().clone()?;
                let weak = Rc::downgrade(self);
                let dialog = studio::combo_dialog(
                    env,
                    Rc::new(move |n, _cx| {
                        let Some(s) = weak.upgrade() else {
                            return true;
                        };
                        if !s.draft.borrow_mut().add_combo(n) {
                            return false;
                        }
                        s.save();
                        s.send();
                        true
                    }),
                );
                cx.overlay(dialog, true);
                None
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
            Act::Hear(_) | Act::HearTake(_) | Act::HearExample(..) => {
                let hear = director::hear_of(act, &self.draft.borrow(), &self.examples.borrow());
                if let Some((line, pin)) = hear {
                    self.hear(line, pin);
                }
                None
            }
        }
    }
}

/// What the director asks of the page.
struct WebScenes(Weak<Studio>);

impl SceneHost for WebScenes {
    type Sound = AudioBuffer;

    fn now_ms(&self) -> f64 {
        js_sys::Date::now()
    }

    fn timer(&self, seconds: f64, f: Box<dyn FnOnce()>) -> Timer {
        Box::new(Timeout::new((seconds.max(0.0) * 1000.0) as u32, f))
    }

    fn env(&self) -> Option<ddi_ui::screens::Env> {
        self.0.upgrade()?.env.borrow().clone()
    }

    fn on_ui(&self, f: UiFn) {
        if let Some(s) = self.0.upgrade() {
            s.on_ui(f);
        }
    }

    fn show(&self, f: UiFn) -> bool {
        let Some(host) = self
            .0
            .upgrade()
            .and_then(|s| s.weak_host())
            .and_then(|w| w.upgrade())
        else {
            return false;
        };
        let Ok(mut h) = host.try_borrow_mut() else {
            return false;
        };
        f(&mut h.ui);
        drop(h);
        ui_dev::wake(&host);
        true
    }

    fn press(&self, action: MenuAction) {
        let Some(host) = self.0.upgrade().and_then(|s| s.weak_host()) else {
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
    }

    fn preview_open(&self, volume: f32) {
        crate::select_preview::set_volume(volume);
        crate::select_preview::gesture();
    }

    fn preview_select(&self, song: Option<String>) {
        let entry = song.and_then(|id| {
            self.0
                .upgrade()?
                .songs
                .borrow()
                .iter()
                .find(|e| e.id == id)
                .cloned()
        });
        crate::select_preview::select(entry);
    }

    fn preview_stop(&self) {
        crate::select_preview::stop();
    }

    fn preview_volume(&self, volume: f32) {
        crate::select_preview::set_volume(volume);
    }

    fn preview_hush(&self) {
        crate::select_preview::hush();
    }

    fn preview_close(&self) {
        crate::select_preview::close();
    }

    fn stage_voice(&self, staged: Option<StagedVoice<AudioBuffer>>) {
        crate::announcers::stage(staged);
    }

    fn open_results_sound(&self, line: Line) -> bool {
        self.0
            .upgrade()
            .and_then(|s| s.new_sound(format!("scene:{}", line.folder())))
            .is_some()
    }

    fn say_results(
        &self,
        scene: &scene::ResultsScene,
        voice: Voice<AudioBuffer>,
    ) -> Option<(Vec<(String, f64)>, f64)> {
        let s = self.0.upgrade()?;
        let audio = s.sound.borrow().clone()?;
        let share = crate::settings::Settings::load().announcer_volume;
        let now = audio.borrow().context().current_time();
        let spoken = director::say_results(&mut *audio.borrow_mut(), now, scene, voice, share);
        Some((spoken, now))
    }

    fn stop_sound(&self) {
        if let Some(s) = self.0.upgrade() {
            s.stop();
        }
    }

    fn changed(&self) {
        if let Some(s) = self.0.upgrade() {
            s.changed();
        }
    }

    fn publish(&self) {
        if let Some(s) = self.0.upgrade() {
            s.publish();
        }
    }

    fn gave_up(&self, why: UiText) {
        if let Some(s) = self.0.upgrade() {
            s.status(why);
        }
    }

    fn heard(&self, status: UiText, line: Line, spoken: Vec<String>, said: Said) {
        if let Some(s) = self.0.upgrade() {
            s.view.borrow_mut().status = status.into();
            *s.heard.borrow_mut() = Some((line, spoken, said.key()));
            s.send();
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
            ui: UiPage::default(),
            studio: Studio::new(),
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
                self.ui.fail(e);
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) {
        self.ui.view(ctx, "select-page");
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        self.studio.stop();
        self.ui.destroy(&["ui", "studio"]);
    }
}

impl StudioPage {
    fn start(&mut self, ctx: &Context<Self>) {
        let studio = self.studio.clone();
        let theme = ui_dev::theme_host(Rc::new(ui_dev::nfkd));
        let env = theme.env().clone();
        *studio.env.borrow_mut() = Some(env.clone());
        let shell: PageShell = {
            let weak = Rc::downgrade(&studio);
            Rc::new(move |call, cx| weak.upgrade()?.call(call, cx))
        };
        let screen = PageScreen::new(env, studio::NAME, studio.doc())
            .with_shell(shell, studio.feed.clone())
            .with_place(studio.place.clone());
        let mut ui = ui_dev::theme_ui();
        ui.push(screen);
        let mut listeners = Vec::new();
        // The pickers: files (a zip too), and a folder.
        for folder in [false, true] {
            let input = files::file_input(ctx.root(), (!folder).then_some(RECORDING_TYPES), true);
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
            let weak = Rc::downgrade(&studio);
            let i = input.clone();
            listeners.push(EventListener::new(&input, "change", move |_| {
                let picked = files::from_input(&i);
                i.set_value("");
                if let Some(s) = weak.upgrade()
                    && !picked.is_empty()
                {
                    let line = s.pick_line.take();
                    s.add(picked, line, s.pick_script.take());
                }
            }));
            if folder {
                *studio.folder_input.borrow_mut() = Some(input);
            } else {
                *studio.files_input.borrow_mut() = Some(input);
            }
        }
        // Drops anywhere: on a line's zone they go on that line.
        if let Some(window) = web_sys::window() {
            let opts = EventListenerOptions::enable_prevent_default();
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
                            Ok(picked) => s.add(picked, line, None),
                            Err(e) => s.status(ms::status_files_unreadable(error_text(&e))),
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
        let Some(started) = self
            .ui
            .start(ctx, ui, Some(theme), Some(extra), Msg::Failed)
        else {
            return;
        };
        *studio.host.borrow_mut() = Some(Rc::downgrade(&started.0));
        started.1.extend(listeners);
        let _ = crate::web::storage::request_persist();
        wasm_bindgen_futures::spawn_local(async move { studio.load().await });
    }
}
