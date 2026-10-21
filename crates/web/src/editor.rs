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
//! decode ends, it starts when it does (the context runs by then).

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use ddi_chart::Song;
use ddi_chart::formats::sm_write;
use ddi_library::path::RelPathBuf;
use ddi_platform::storage::{self, Area};
use ddi_session::listen::Listening;
use ddi_ui::message::{Message, Param};
use ddi_ui::screens::editor::{EditorCall, EditorHost, EditorPlace, SAVED};
use ddi_ui::state::editor::Editor;
use ddi_ui::state::editor::autosave::{self, Autosave};
use ddi_ui::{Cx, UiRequest};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::AudioBuffer;

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
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
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
        })
    });
    editor
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

/// Whether song `id`'s chart file is one the editor writes.
pub(crate) fn editable(chart: &str) -> bool {
    let ext = chart.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "sm" | "ssc" | "dwi")
}

/// What the web can do with song `id`.
pub(crate) fn place(id: &str) -> EditorPlace {
    let imported = crate::songs::is_imported_id(id);
    EditorPlace {
        save: imported.then(|| "this browser's copy of the song".into()),
        confirm: imported.then(|| {
            "The song stored in this browser is replaced (the files it was imported from are \
             not changed)."
                .into()
        }),
        export: Some("Download".into()),
        listen: true,
        playtest: true,
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
    /// The chart whose music is decoded or being decoded.
    chart: Option<usize>,
    music: Option<AudioBuffer>,
    /// Asked for while the music decoded: from that second.
    waiting: Option<f64>,
    playing: Option<Listening>,
}

impl Listen {
    /// Decodes chart `chart`'s music of song `id` (on `decoder`, or an
    /// offline context).
    pub(crate) fn read(
        this: &Rc<RefCell<Listen>>,
        id: &str,
        chart: usize,
        decoder: Option<AudioDecoder>,
    ) {
        {
            let mut l = this.borrow_mut();
            l.chart = Some(chart);
            l.music = None;
        }
        let Some(decoder) = decoder.or_else(|| AudioDecoder::offline(48_000.0)) else {
            return;
        };
        let (this, id) = (this.clone(), id.to_string());
        wasm_bindgen_futures::spawn_local(async move {
            let decoded = match crate::songs::load_song(&id, chart).await {
                Ok(loaded) => {
                    let target = ddi_session::music_target(&loaded.song);
                    decoder.decode(&loaded.music_bytes, target).await
                }
                Err(e) => Err(e.message()),
            };
            let mut l = this.borrow_mut();
            if l.chart != Some(chart) {
                return;
            }
            match decoded {
                Ok(buffer) => {
                    l.music = Some(buffer);
                    if let Some(from) = l.waiting.take() {
                        l.start(from);
                    }
                }
                Err(e) => {
                    web_sys::console::warn_1(&format!("editor: the music: {e}").into());
                    l.waiting = None;
                }
            }
        });
    }

    fn start(&mut self, from: f64) {
        let (Some(audio), Some(music)) = (self.audio.as_mut(), self.music.as_ref()) else {
            return;
        };
        let s = crate::settings::Settings::load();
        self.playing = Listening::start(audio, music, from, s.volume, s.audio_offset)
            .map_err(|e| web_sys::console::warn_1(&format!("editor: listening: {e}").into()))
            .ok();
    }

    fn stop(&mut self) {
        if let (Some(p), Some(audio)) = (self.playing.take(), self.audio.as_mut()) {
            p.stop(audio);
        }
        self.waiting = None;
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
                let (text, ext) = if sm {
                    (sm_write::write_sm(editor.song()), "sm")
                } else {
                    (sm_write::write_ssc(editor.song()), "ssc")
                };
                if let Err(e) = download(&text, &format!("{}.{ext}", file_stem(editor.song()))) {
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
            EditorCall::Listen { from } => {
                let mut l = self.listen.borrow_mut();
                l.stop();
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
                if l.music.is_some() {
                    l.start(from);
                } else {
                    l.waiting = Some(from);
                }
            }
            EditorCall::Stop => self.listen.borrow_mut().stop(),
            EditorCall::Autosave => {
                let a = editor.autosave(&self.id, storage::now_ms());
                if let Err(e) = storage::save(Area::Local, autosave::KEY, &a) {
                    web_sys::console::warn_1(&format!("editor: autosave: {e}").into());
                }
            }
            EditorCall::Leave => {
                end();
                forget_autosave();
                self.listen.borrow_mut().stop();
                cx.request(UiRequest::Back);
            }
        }
    }

    fn music_time(&self) -> Option<f64> {
        let mut l = self.listen.borrow_mut();
        if let Some(from) = l.waiting {
            return Some(from);
        }
        let t = match (&l.playing, &l.audio) {
            (Some(p), Some(a)) => p.time(a),
            _ => None,
        };
        if t.is_none() {
            l.playing = None;
        }
        t
    }
}

/// A file name from the song's title.
fn file_stem(song: &Song) -> String {
    let s: String = song
        .title
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "song".into() } else { s }
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
    if record.work.is_some() || !editable(record.entry.chart.as_str()) {
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
