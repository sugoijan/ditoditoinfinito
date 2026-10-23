//! The chart editor on the web (`docs/plans/editor.md`): the song being
//! edited kept for the session (a playtest's page and the way back find
//! it again, its undo and cursor as they were), saved back into this
//! browser's copy of an imported song, downloaded as a simfile, and
//! played on the play route (`#/play?…&edit=1`) as edited.
//!
//! Saving an imported song writes its chart as `.ssc` in its stored
//! files (`chart.ssc`, the old chart file let go) and its list entry
//! made again (`ddi_library::edited`); the files it was imported from
//! are never touched. A bundled song cannot be saved here: downloaded
//! only. A download is a link clicked inside the press that asked for
//! it (browsers allow it then). The autosave
//! (`ddi_ui::state::editor::autosave`) is kept in local storage; opening
//! its song again (after a reload, a closed tab) recovers it.
//!
//! Listening: the chart's music is decoded when the editor opens (on an
//! offline context: no gesture needed), so the press that asks plays it
//! at once, on a context made inside that press; asked before the
//! decode ends, it starts when it does (the context runs by then). A
//! chart whose music is the file decoded already is not decoded again.
//! The music decoded is analyzed for the overview and the audio view
//! (`ddi_ui::state::editor::audio`) a few milliseconds each animation
//! frame, its MP3 lead kept, so what is drawn is what is heard.
//!
//! A new song from a music file (`new_song`) is stored when it is made;
//! left before its first save, it is taken away again.

mod new_song;
pub(crate) use new_song::create as new_song;

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use ddi_chart::Song;
use ddi_chart::formats::sm_write;
use ddi_library::manifest::ManifestEntry;
use ddi_library::pack::ChartFormat;
use ddi_library::path::RelPathBuf;
use ddi_platform::storage::{self, Area};
use ddi_session::listen::{Cues, Listener};
use ddi_ui::message::{Message, Param};
use ddi_ui::screens::editor::{EditorCall, EditorHost, EditorPlace, SAVED};
use ddi_ui::state::editor::audio::{Analysis, Builder};
use ddi_ui::state::editor::autosave::{self, Autosave};
use ddi_ui::state::editor::{Editor, export};
use ddi_ui::{Cx, UiRequest};
use wasm_bindgen::{JsCast, JsValue};

use crate::components::ui_dev;
use crate::router::Route;
use crate::web::audio::{AudioDecoder, WebAudio};
use crate::web::idb::{self, Db, Write};

/// The song being edited this session.
struct Session {
    id: String,
    editor: Rc<RefCell<Editor>>,
    /// The playtest asked for: the song as edited, the second it starts.
    playtest: Option<(Song, f64)>,
    /// A new song not saved yet.
    fresh: bool,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    /// New songs left unsaved, being taken away: the library leaves them
    /// out meanwhile.
    static DROPPED: RefCell<std::collections::HashSet<String>> = RefCell::default();
}

/// Song `id`'s editor if it is being edited.
pub(crate) fn editing(id: &str) -> Option<Rc<RefCell<Editor>>> {
    SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .filter(|s| s.id == id)
            .map(|s| s.editor.clone())
    })
}

/// Starts editing song `id` (what was edited before is let go).
pub(crate) fn begin(id: &str, editor: Editor) -> Rc<RefCell<Editor>> {
    let editor = Rc::new(RefCell::new(editor));
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            id: id.to_string(),
            editor: editor.clone(),
            playtest: None,
            fresh: false,
        })
    });
    editor
}

/// Starts editing new song `id` (stored already, never saved).
fn begin_new(id: &str, editor: Editor) {
    DROPPED.with(|d| d.borrow_mut().remove(id));
    begin(id, editor);
    SESSION.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.fresh = true;
        }
    });
}

/// Song `id` is a new song being edited, not saved yet.
pub(crate) fn fresh(id: &str) -> bool {
    SESSION.with(|s| s.borrow().as_ref().is_some_and(|s| s.id == id && s.fresh))
}

/// Song `id` is a new song left unsaved, being taken away.
pub(crate) fn dropped(id: &str) -> bool {
    DROPPED.with(|d| d.borrow().contains(id))
}

/// New song `id` taken away (left unsaved): left out of the library
/// at once, its stored record and files after.
fn forget_new(id: &str) {
    DROPPED.with(|d| d.borrow_mut().insert(id.to_string()));
    let ids = vec![id.to_string()];
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = crate::songs::delete_imported(&ids).await {
            web_sys::console::warn_1(&format!("editor: the new song: {e}").into());
        }
    });
}

/// The playtest of song `id`: the song as edited and its start second.
pub(crate) fn playtest(id: &str) -> Option<(Song, f64)> {
    SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .filter(|s| s.id == id)
            .and_then(|s| s.playtest.clone())
    })
}

fn end() {
    SESSION.with(|s| *s.borrow_mut() = None);
}

/// The autosave of song `id`, if the last song edited was it.
pub(crate) fn autosaved(id: &str) -> Option<Autosave> {
    storage::load::<Autosave>(Area::Local, autosave::KEY).filter(|a| a.song_id == id)
}

fn forget_autosave() {
    storage::remove(Area::Local, autosave::KEY);
}

/// What the web can do with song `id`.
pub(crate) fn place(id: &str) -> EditorPlace {
    let imported = crate::songs::is_imported_id(id);
    let fresh = fresh(id);
    EditorPlace {
        save: imported.then(|| "this browser's copy of the song".into()),
        confirm: (imported && !fresh).then(|| {
            "The song stored in this browser is replaced (the files it was imported from are \
             not changed)."
                .into()
        }),
        export: Some("Download".into()),
        listen: true,
        playtest: true,
        discard: fresh.then(|| "The new song is not kept.".into()),
    }
}

/// The editor's calls on the web; `ui` is the page's UI host once it runs.
pub(crate) struct WebEditorHost {
    pub(crate) id: String,
    pub(crate) ui: Rc<RefCell<Option<Weak<RefCell<ui_dev::Host>>>>>,
    pub(crate) listen: Rc<RefCell<Listen>>,
}

/// Listening: the page's audio (made in the first press), the music of a
/// chart once decoded, what plays.
#[derive(Default)]
pub(crate) struct Listen {
    audio: Option<WebAudio>,
    /// The chart whose music was asked for last.
    chart: Option<usize>,
    /// The song's entry once read: which file each chart's music is.
    entry: Option<ManifestEntry>,
    listener: Listener<WebAudio>,
    /// The music's analysis once made, the decodes and the analysis
    /// under way (a newer read stops an older analysis).
    analysis: Option<Rc<Analysis>>,
    reads: u32,
    analyzing: u32,
    /// Reads begun.
    begun: u64,
}

impl Listen {
    /// Decodes chart `chart`'s music of song `id` (on `decoder`, or an
    /// offline context), unless that file is decoded already.
    pub(crate) fn read(
        this: &Rc<RefCell<Listen>>,
        id: &str,
        chart: usize,
        decoder: Option<AudioDecoder>,
    ) {
        let known = {
            let mut l = this.borrow_mut();
            l.chart = Some(chart);
            match l.entry.as_ref().map(|e| e.music_for(chart).to_owned()) {
                Some(file) if !l.listener.read(&file) => return,
                Some(file) => Some(file),
                None => {
                    l.listener.forget();
                    None
                }
            }
        };
        let Some(decoder) = decoder.or_else(|| AudioDecoder::offline(48_000.0)) else {
            return;
        };
        let read = {
            let mut l = this.borrow_mut();
            l.reads += 1;
            l.begun += 1;
            l.analysis = None;
            l.begun
        };
        let (this, id) = (this.clone(), id.to_string());
        wasm_bindgen_futures::spawn_local(async move {
            let decoded = Listen::decode(&this, &id, chart, known, decoder).await;
            this.borrow_mut().analyzing += 1;
            this.borrow_mut().reads -= 1;
            if let Some(buffer) = decoded {
                let stale = || this.borrow().begun != read;
                if let Some(a) = analyze(&buffer, stale).await.filter(|_| !stale()) {
                    this.borrow_mut().analysis = Some(Rc::new(a));
                }
            }
            this.borrow_mut().analyzing -= 1;
        });
    }

    /// [`Listen::read`]'s decode: the music handed to the listener, and
    /// for the overview when it is the chart's.
    async fn decode(
        this: &Rc<RefCell<Listen>>,
        id: &str,
        chart: usize,
        known: Option<RelPathBuf>,
        decoder: AudioDecoder,
    ) -> Option<web_sys::AudioBuffer> {
        {
            let mut file = known;
            let decoded = match crate::songs::load_song(id, chart).await {
                Ok(loaded) => {
                    {
                        let mut l = this.borrow_mut();
                        if file.is_none() {
                            // Found out now: read here unless another
                            // chart is asked for, or its read reads it.
                            let f = loaded.entry.music_for(chart).to_owned();
                            if l.chart != Some(chart) || !l.listener.read(&f) {
                                return None;
                            }
                            file = Some(f);
                        }
                        l.entry.get_or_insert_with(|| loaded.entry.clone());
                    }
                    let target = ddi_session::music_target(&loaded.song);
                    decoder.decode(&loaded.music_bytes, target).await
                }
                Err(e) => Err(e.message()),
            };
            let l = &mut *this.borrow_mut();
            match (decoded, &file) {
                (Ok(buffer), Some(file)) => {
                    let kept = buffer.clone();
                    if let Err(e) = l.listener.music_ready(file, buffer, l.audio.as_mut()) {
                        web_sys::console::warn_1(&format!("editor: listening: {e}").into());
                    }
                    (l.chart == Some(chart)).then_some(kept)
                }
                (Ok(_), None) => None,
                (Err(e), _) => {
                    web_sys::console::warn_1(&format!("editor: the music: {e}").into());
                    if file.is_some() || l.chart == Some(chart) {
                        l.listener.music_failed(file.as_deref());
                    }
                    None
                }
            }
        }
    }

    fn stop(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            self.listener.stop(audio);
        }
    }
}

impl EditorHost for WebEditorHost {
    fn call(&self, call: EditorCall, editor: &Editor, cx: &mut Cx) {
        match call {
            EditorCall::Save { revision } => {
                let text = sm_write::write_ssc(editor.song());
                let id = self.id.clone();
                let ui = self.ui.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let m = match save_imported(&id, &text).await {
                        Ok(()) => {
                            forget_autosave();
                            SESSION.with(|s| {
                                if let Some(s) = s.borrow_mut().as_mut().filter(|s| s.id == id) {
                                    s.fresh = false;
                                }
                            });
                            Message::new(SAVED)
                                .with("Revision", Param::Int(revision.map_or(-1, |r| r as i64)))
                        }
                        Err(e) => Message::new(SAVED).with("Error", Param::Str(e)),
                    };
                    if let Some(host) = ui.borrow().clone() {
                        ui_dev::broadcast_message_soon(host, m);
                    }
                });
            }
            EditorCall::Export { sm } => {
                let (text, ext) = export::simfile(editor.song(), sm);
                let stem = export::file_stem(&editor.song().title);
                if let Err(e) = download(&text, &format!("{stem}.{ext}")) {
                    cx.broadcast(Message::new(SAVED).with("Error", Param::Str(e.into())));
                }
            }
            EditorCall::Playtest { from_cursor, auto } => {
                let (song, chart, start) = editor.playtest(from_cursor);
                SESSION.with(|s| {
                    if let Some(s) = s.borrow_mut().as_mut() {
                        s.playtest = Some((song, start));
                    }
                });
                let route = Route::Play {
                    song: self.id.clone(),
                    chart,
                    force_gl: crate::router::testing().split('&').any(|kv| kv == "gfx=gl"),
                    auto,
                    auto_pad: false,
                    bias_ms: 0,
                    slow: false,
                    replay: None,
                    view_results: false,
                    edit: true,
                };
                cx.request(UiRequest::Navigate(route.to_hash()));
            }
            EditorCall::Listen { from } | EditorCall::Record { from, .. } => {
                let mut l = self.listen.borrow_mut();
                // Inside the press: the context made and resumed now.
                if l.audio.is_none() {
                    match WebAudio::new() {
                        Ok(a) => l.audio = Some(a),
                        Err(e) => {
                            web_sys::console::warn_1(&format!("editor: audio: {e}").into());
                            return;
                        }
                    }
                }
                if let Some(a) = &l.audio {
                    a.resume_now();
                }
                let chart = editor.chart_index();
                if l.chart != Some(chart) {
                    let decoder = l.audio.as_ref().map(WebAudio::decoder);
                    drop(l);
                    Listen::read(&self.listen, &self.id, chart, decoder);
                    l = self.listen.borrow_mut();
                }
                let s = crate::settings::Settings::load();
                let l = &mut *l;
                l.listener
                    .set_cues(ddi_session::listen::cues_for(&call, editor));
                // The output's offset, as a play on it takes.
                let offset = l.audio.as_ref().map_or(s.audio_offset, |a| {
                    s.audio_offset_for(Some(&a.fingerprint()))
                });
                if let Some(audio) = l.audio.as_mut()
                    && let Err(e) = l.listener.ask(audio, from, s.volume, offset)
                {
                    web_sys::console::warn_1(&format!("editor: listening: {e}").into());
                }
            }
            EditorCall::Stop => self.listen.borrow_mut().stop(),
            EditorCall::Cues => self.listen.borrow_mut().listener.set_cues(Cues::of(editor)),
            EditorCall::Autosave => {
                let a = editor.autosave(&self.id, storage::now_ms());
                if let Err(e) = storage::save(Area::Local, autosave::KEY, &a) {
                    web_sys::console::warn_1(&format!("editor: autosave: {e}").into());
                }
            }
            EditorCall::Leave => {
                if fresh(&self.id) {
                    forget_new(&self.id);
                }
                end();
                forget_autosave();
                self.listen.borrow_mut().stop();
                cx.request(UiRequest::Back);
            }
        }
    }

    fn music_time(&self) -> Option<f64> {
        let l = &mut *self.listen.borrow_mut();
        let audio = l.audio.as_mut()?;
        l.listener.cue(audio);
        l.listener.time(audio)
    }

    fn heard_at(&self, host: f64) -> Option<f64> {
        self.listen
            .borrow()
            .listener
            .heard_at(ddi_platform::HostTime(host))
    }

    fn record_input(
        &self,
        layout: &ddi_chart::Layout,
    ) -> Option<ddi_ui::screens::editor::RecordInput> {
        Some(ddi_session::listen::record_input(
            &crate::settings::Settings::load(),
            layout,
            &crate::web::gamepad::page_pads(),
        ))
    }

    fn audio(&self) -> Option<Rc<Analysis>> {
        self.listen.borrow().analysis.clone()
    }

    fn audio_coming(&self) -> bool {
        let l = self.listen.borrow();
        l.reads > 0 || l.analyzing > 0
    }

    fn show_audio(&self, picture: Option<&ddi_ui::screens::editor::audio::AudioPicture>) {
        crate::editor_audio::show(picture);
    }

    fn clock(&self) -> Option<f64> {
        Some(now_ms() / 1000.0)
    }

    fn skin_field(&self) -> Option<ddi_ui::screens::editor_field::SkinField> {
        Some(ddi_shell::editor_field::skin_field(
            &crate::settings::Settings::load(),
        ))
    }

    fn show_field(
        &self,
        picture: Option<&ddi_ui::screens::editor_field::FieldPicture>,
        editor: &Editor,
    ) {
        crate::editor_field::show(picture, editor);
    }
}

/// Milliseconds a frame the analysis takes at most.
const ANALYSIS_BUDGET_MS: f64 = 4.0;

/// The page's clock, milliseconds.
fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}

/// Until the next animation frame.
async fn next_frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.request_animation_frame(&resolve);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// The analysis of `buffer` (its lead: where its first sample is
/// heard), its samples read a block at a time and analyzed
/// [`ANALYSIS_BUDGET_MS`] each animation frame; `None` once `stale`
/// says another read began.
async fn analyze(buffer: &web_sys::AudioBuffer, stale: impl Fn() -> bool) -> Option<Analysis> {
    let frames = buffer.length() as usize;
    let mut b = Builder::new(
        f64::from(buffer.sample_rate()),
        frames,
        crate::web::audio::lead(buffer),
    );
    let channels = (buffer.number_of_channels() as usize).max(1);
    let mut blocks = vec![vec![0.0f32; 1 << 15]; channels];
    loop {
        let until = now_ms() + ANALYSIS_BUDGET_MS;
        while now_ms() < until {
            let at = b.fed();
            if at < frames {
                let n = blocks[0].len().min(frames - at);
                for (c, block) in blocks.iter_mut().enumerate() {
                    if buffer
                        .copy_from_channel_with_start_in_channel(
                            &mut block[..n],
                            c as i32,
                            at as u32,
                        )
                        .is_err()
                    {
                        block[..n].fill(0.0);
                    }
                }
                let planar: Vec<&[f32]> = blocks.iter().map(|b| &b[..n]).collect();
                b.add_planar(&planar);
            }
            if b.step(8) {
                return Some(b.finish());
            }
        }
        next_frame().await;
        if stale() {
            return None;
        }
    }
}

/// The page's debug state: what the cues scheduled, as
/// `window.__DDI_DEBUG.editorCues` (`accents`, `beats`, `claps`, and the
/// last one's `kind`, `song` second and context time `at`).
pub(crate) fn debug(listen: &Rc<RefCell<Listen>>) -> ui_dev::DebugFn {
    let listen = Rc::downgrade(listen);
    Box::new(move |obj| {
        let Some(listen) = listen.upgrade() else {
            return;
        };
        let n = listen.borrow().listener.scheduled;
        let o = js_sys::Object::new();
        let set = |k: &str, v: JsValue| {
            let _ = js_sys::Reflect::set(&o, &k.into(), &v);
        };
        set("accents", n.accents.into());
        set("beats", n.beats.into());
        set("claps", n.claps.into());
        if let Some((cue, song, at)) = n.last {
            set("kind", format!("{cue:?}").into());
            set("song", song.into());
            set("at", at.into());
        }
        let _ = js_sys::Reflect::set(obj, &"editorCues".into(), &o);
    })
}

/// `text` downloaded as `name` (inside the press: a link clicked).
fn download(text: &str, name: &str) -> Result<(), &'static str> {
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("text/plain;charset=utf-8");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(
        &js_sys::Array::of1(&JsValue::from_str(text)),
        &options,
    )
    .map_err(|_| "no file")?;
    let url = crate::web::files::object_url(&blob).ok_or("no object URL")?;
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let a: web_sys::HtmlElement = document
        .create_element("a")
        .map_err(|_| "no link")?
        .unchecked_into();
    let _ = a.set_attribute("href", &url);
    let _ = a.set_attribute("download", name);
    a.click();
    gloo::timers::callback::Timeout::new(10_000, move || {
        crate::web::files::revoke_object_url(&url)
    })
    .forget();
    Ok(())
}

/// Writes `text` as imported song `id`'s chart: its stored `chart.ssc`,
/// its entry made again; the old chart file let go.
async fn save_imported(id: &str, text: &str) -> Result<(), String> {
    let db = Db::open().await?;
    let mut record = crate::songs::imported_record(&db, id).await?;
    if record.work.is_some() || ChartFormat::of(record.entry.chart.as_str()).is_none() {
        return Err("only simfiles can be saved".into());
    }
    let chart = RelPathBuf::from("chart.ssc");
    let old = record.entry.chart.clone();
    record.entry = ddi_library::edited::entry_after_edit(&record.entry, text, chart.clone())?;
    // Read from the stored copy from now on, not the kept folder.
    record.links.remove(chart.as_str());
    record.links.remove(old.as_str());
    let blob = web_sys::Blob::new_with_str_sequence(&js_sys::Array::of1(&JsValue::from_str(text)))
        .map_err(|_| "no file".to_string())?;
    let json = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    let mut writes = vec![
        Write::Put {
            store: idb::FILES,
            key: crate::songs::file_key(id, chart.as_str()),
            value: blob.into(),
        },
        Write::Put {
            store: idb::SONGS,
            key: id.to_string(),
            value: JsValue::from_str(&json),
        },
    ];
    if old != chart {
        writes.push(Write::Delete {
            store: idb::FILES,
            key: crate::songs::file_key(id, old.as_str()),
        });
    }
    db.write(writes).await
}
