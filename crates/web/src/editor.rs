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
//! it (browsers allow it then).

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use ddi_chart::Song;
use ddi_chart::formats::sm_write;
use ddi_library::path::RelPathBuf;
use ddi_ui::message::{Message, Param};
use ddi_ui::screens::editor::{EditorCall, EditorHost, EditorPlace, SAVED};
use ddi_ui::state::editor::Editor;
use ddi_ui::{Cx, UiRequest};
use wasm_bindgen::{JsCast, JsValue};

use crate::components::ui_dev;
use crate::router::Route;
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
        listen: false,
        playtest: true,
    }
}

/// The editor's calls on the web; `ui` is the page's UI host once it runs.
pub(crate) struct WebEditorHost {
    pub(crate) id: String,
    pub(crate) ui: Rc<RefCell<Option<Weak<RefCell<ui_dev::Host>>>>>,
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
                        Ok(()) => Message::new(SAVED)
                            .with("Revision", Param::Int(revision.map_or(-1, |r| r as i64))),
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
            EditorCall::Leave => {
                end();
                cx.request(UiRequest::Back);
            }
            // Listening waits for its step (`docs/plans/editor.md`, 5).
            EditorCall::Listen { .. } | EditorCall::Stop => {}
        }
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
