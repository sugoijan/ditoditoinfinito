//! Song select at `#/` (`docs/plans/ui-core.md`, step 6.2): our theme's
//! `SelectScreen` on the app's canvas over the whole window, on the real
//! library (the bundled manifest and the imported songs), with the
//! player's bests from the play history.
//!
//! The page keeps what is the browser's: the import's real file inputs
//! (`.import-panel input[type=file]`, hidden; the screen's buttons click
//! them inside the gesture), drag and drop anywhere on the page, previews
//! through one audio context made in the first gesture
//! (`crate::select_preview`), the storage estimate and the request to
//! keep it, kept folders read again, controllers without bindings, and
//! where the list was (`ddi.menu.v1` in `sessionStorage`), and the
//! selected song's preview movie drawn into the banner's place
//! (`crate::preview_movie`, the host's frame hook).
//!
//! `window.__DDI_DEBUG.songs` says what the library holds, what the
//! import does and what plays; `__DDI_DEBUG.ui.select` (as `ui.theme`)
//! what the screen shows.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::component::{Callback, Component, Context, NodeRef};
use ddi_engine::{ConnectedPad, PadUse};
use ddi_ui::screens::data::Notice;
use ddi_ui::screens::select::{
    FEED, Feed, OpenFn, PlayFn, SelectCall, SelectScreen, SharedFeed, ShellFn,
};
use ddi_ui::{TextMode, Ui, UiRequest};
use gloo::events::{EventListener, EventListenerOptions};
use gloo::timers::callback::{Interval, Timeout};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{DragEvent, HtmlElement, HtmlInputElement};

use crate::components::ui_dev::{self, Host, Started};
use crate::import;
use crate::select_data;
use crate::settings::Settings;
use crate::songs::{Library, delete_imported, keep_folder, kept_folders};
use crate::web::files::{self, PickedFile};
use crate::web::{folders, storage};
use ddi_ui::state::place::MenuPlace;

/// How often the connected controllers are checked for missing bindings.
const PAD_CHECK_MS: u32 = 1000;
/// What the zip and files input takes.
const ACCEPT: &str = ".zip,.sm,.ssc,.dwi,.swf,.as,.beatbox,.txt,.ogg,.oga,.opus,.mp3,.wav,.flac,.png,.jpg,.jpeg,.gif,.bmp,.webp,.avi,.f4v,.flv,.mkv,.mp4,.mpeg,.mpg,.mov,.ogv,.webm,.wmv";

/// The music a new song is made from (`SelectCall::NewSong`).
const ACCEPT_MUSIC: &str = "audio/*,.ogg,.oga,.opus,.mp3,.wav,.flac";

pub(crate) enum Msg {
    Library(Result<Library, String>),
    /// The music picked for a new song, and the song made (its id).
    Music(web_sys::File),
    NewSong(Result<String, String>),
    Folders(Vec<(String, JsValue)>),
    Usage(Option<f64>),
    Persisted(Option<bool>),
    ImportChanged,
    HistoryChanged,
    Picked(Result<Vec<PickedFile>, String>),
    FromFolder(import::Origin, Result<Vec<PickedFile>, String>),
    Remove(Vec<String>),
    Removed(Result<(), String>),
    Dragging(bool),
    PadsTick,
    Failed(String),
    /// A line to show for a while (an action refused).
    Say(String),
    /// The said line's time is over.
    Unsay(u64),
}

/// What the screen's calls and the page share.
struct Shared {
    link: crate::component::Scope<SelectPage>,
    library: RefCell<Option<Library>>,
    folders: RefCell<HashMap<String, JsValue>>,
    feed: SharedFeed,
    host: RefCell<Option<Weak<RefCell<Host>>>>,
    /// Where the chart shown goes for its prompt's diagram's skin to be
    /// readied ahead (the host's, `ui_dev::use_warm_cell`; the screen's
    /// first place report comes before the host is handed out).
    warm: ui_dev::WarmCell,
    folder_input: NodeRef,
    file_input: NodeRef,
    music_input: NodeRef,
    /// The place saved last (to skip writing the same).
    place: RefCell<MenuPlace>,
    /// The imported packs' names, once a library load (the debug state).
    packs: RefCell<Vec<String>>,
    /// The import as the debug state says it, as of its last change.
    import_debug: RefCell<ImportDebug>,
    /// The selected song's preview movie (`#PREVIEWVID`).
    movie: RefCell<crate::preview_movie::PreviewMovieHost>,
}

/// The import, for `__DDI_DEBUG.songs`.
#[derive(Clone, Default)]
struct ImportDebug {
    running: Option<String>,
    report: Option<(String, bool)>,
    finished: u64,
}

impl ImportDebug {
    fn now() -> ImportDebug {
        let (running, report, finished) = import::status();
        ImportDebug {
            running,
            report: report.map(|r| {
                let v = select_data::report_view(&r, Settings::load().import_details);
                (v.summary, v.error)
            }),
            finished,
        }
    }
}

/// The page's few DOM elements: the status line, the drop note, the
/// error, and the inputs' listeners.
struct Dom {
    status: web_sys::HtmlElement,
    drop: web_sys::HtmlElement,
    error: web_sys::HtmlElement,
    _listeners: Vec<EventListener>,
}

pub(crate) struct SelectPage {
    root: NodeRef,
    dom: std::cell::RefCell<Option<Dom>>,
    shared: Rc<Shared>,
    started: Option<Started>,
    page: Vec<EventListener>,
    failure: Option<String>,
    load_error: Option<String>,
    dragging: bool,
    usage: Option<f64>,
    persisted: Option<bool>,
    unbound: Vec<String>,
    /// The layout they were checked for.
    unbound_layout: String,
    seen_imports: u64,
    history_seen: u64,
    _pad_check: Option<Interval>,
    /// A line shown for a while, and which one it is.
    said: Option<(u64, String)>,
    said_count: u64,
    _said_timer: Option<Timeout>,
}

impl Component for SelectPage {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link()
            .send_future(async { Msg::Library(Library::load().await) });
        import::watch(Some(ctx.link().callback(|_| Msg::ImportChanged)));
        {
            let link = ctx.link().clone();
            crate::history::watch(Some(Rc::new(move || {
                link.send_message(Msg::HistoryChanged)
            })));
        }
        crate::select_preview::set_volume(Settings::load().volume);
        crate::select_preview::arrive();
        let shared = Rc::new(Shared {
            link: ctx.link().clone(),
            library: RefCell::new(None),
            folders: RefCell::new(HashMap::new()),
            feed: Rc::new(RefCell::new(Feed::default())),
            host: RefCell::new(None),
            warm: Rc::default(),
            folder_input: NodeRef::default(),
            file_input: NodeRef::default(),
            music_input: NodeRef::default(),
            place: RefCell::new(MenuPlace::load()),
            packs: RefCell::new(Vec::new()),
            import_debug: RefCell::new(ImportDebug::now()),
            movie: RefCell::default(),
        });
        let pad_check = {
            let link = ctx.link().clone();
            Some(Interval::new(PAD_CHECK_MS, move || {
                link.send_message(Msg::PadsTick)
            }))
        };
        SelectPage {
            root: NodeRef::default(),
            dom: std::cell::RefCell::new(None),
            shared,
            started: None,
            page: page_listeners(ctx),
            failure: None,
            load_error: None,
            dragging: false,
            usage: None,
            persisted: None,
            unbound: Vec::new(),
            unbound_layout: String::new(),
            seen_imports: import::status().2,
            history_seen: 0,
            _pad_check: pad_check,
            said: None,
            said_count: 0,
            _said_timer: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Library(Ok(library)) => {
                let imported = !library.imported.is_empty();
                let mut packs: Vec<String> = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for s in &library.imported {
                    if seen.insert(s.pack.to_lowercase()) {
                        packs.push(s.pack.clone());
                    }
                }
                *self.shared.packs.borrow_mut() = packs;
                *self.shared.library.borrow_mut() = Some(library);
                if folders::supported() {
                    ctx.link()
                        .send_future(async { Msg::Folders(kept_folders().await) });
                }
                if imported {
                    ctx.link()
                        .send_future(async { Msg::Usage(storage_usage().await) });
                    ctx.link()
                        .send_future(async { Msg::Persisted(storage::persisted().await) });
                } else {
                    self.usage = None;
                }
                if self.started.is_none() {
                    self.start(ctx);
                } else {
                    self.send_data();
                }
                return true;
            }
            Msg::Library(Err(e)) => {
                self.load_error = Some(e);
                return true;
            }
            Msg::Folders(f) => {
                *self.shared.folders.borrow_mut() = f.into_iter().collect();
                self.send_data();
            }
            Msg::Usage(u) => {
                self.usage = u;
                self.send_notices();
            }
            Msg::Persisted(p) => {
                self.persisted = p;
                self.send_notices();
            }
            Msg::ImportChanged => {
                *self.shared.import_debug.borrow_mut() = ImportDebug::now();
                let finished = import::status().2;
                if finished != self.seen_imports {
                    self.seen_imports = finished;
                    ctx.link()
                        .send_future(async { Msg::Library(Library::load().await) });
                }
                self.send_import();
                return true;
            }
            Msg::HistoryChanged => {
                let g = crate::history::plays().generation();
                if g != self.history_seen && self.started.is_some() {
                    self.history_seen = g;
                    self.send_data();
                }
            }
            Msg::Picked(Err(e)) => {
                let dropped = self.dragging;
                self.dragging = false;
                import::report_error(e);
                if dropped && self.started.is_some() {
                    self.shared.feed.borrow_mut().open_import = true;
                    self.send_import();
                }
                return true;
            }
            Msg::Picked(Ok(files)) => {
                let dropped = self.dragging;
                self.dragging = false;
                if import::start(files) {
                    crate::select_preview::stop();
                }
                // Dropped on the page: the dialog shows how it goes.
                if dropped && self.started.is_some() {
                    self.shared.feed.borrow_mut().open_import = true;
                    self.send_import();
                }
                return true;
            }
            Msg::Music(file) => {
                ctx.link()
                    .send_future(async move { Msg::NewSong(crate::editor::new_song(file).await) });
            }
            Msg::NewSong(Ok(id)) => {
                crate::select_preview::close();
                crate::router::navigate(
                    &crate::router::Route::Edit { song: id, chart: 0 }.to_hash(),
                );
            }
            Msg::NewSong(Err(e)) => {
                ctx.link()
                    .send_message(Msg::Say(format!("The new song could not be made: {e}")));
            }
            Msg::FromFolder(_, Err(e)) => import::report_error(e),
            Msg::FromFolder(origin, Ok(files)) => {
                if import::start_from(files, origin) {
                    crate::select_preview::stop();
                }
                if folders::supported() {
                    ctx.link()
                        .send_future(async { Msg::Folders(kept_folders().await) });
                }
            }
            Msg::Remove(ids) => {
                if ids.is_empty() {
                    return false;
                }
                crate::select_preview::select(None);
                let mut settings = Settings::load();
                for id in &ids {
                    settings.set_song_offset(id, 0.0);
                }
                settings.save();
                ctx.link()
                    .send_future(async move { Msg::Removed(delete_imported(&ids).await) });
            }
            Msg::Removed(r) => {
                if let Err(e) = r {
                    import::report_error(e);
                }
                ctx.link()
                    .send_future(async { Msg::Library(Library::load().await) });
            }
            Msg::Dragging(on) => {
                if self.dragging != on {
                    self.dragging = on;
                    return true;
                }
            }
            Msg::PadsTick => self.check_pads(),
            Msg::Say(text) => {
                self.said_count += 1;
                let n = self.said_count;
                self.said = Some((n, text));
                let link = ctx.link().clone();
                self._said_timer =
                    Some(Timeout::new(6000, move || link.send_message(Msg::Unsay(n))));
                self.send_notices();
            }
            Msg::Unsay(n) => {
                if self.said.as_ref().is_some_and(|(k, _)| *k == n) {
                    self.said = None;
                    self.send_notices();
                }
            }
            Msg::Failed(e) => {
                self.failure = Some(e);
                if let Some((host, _, pads)) = &self.started {
                    ui_dev::stop_ui(host, pads.as_ref());
                }
                return true;
            }
        }
        false
    }

    fn view(&self, ctx: &Context<Self>) {
        let root = ctx.root();
        if let Some(e) = &self.failure {
            self.dom.borrow_mut().take();
            crate::component::no_gpu(root, e);
            return;
        }
        let shared = &self.shared;
        let mut dom = self.dom.borrow_mut();
        let dom = dom.get_or_insert_with(|| {
            use crate::component::element;
            root.set_class_name("select-page");
            self.root.set(root);
            // The import's real inputs, hidden: the screen's buttons click
            // them inside the gesture; tests set files on them.
            let panel = element(root, "section", "import-panel");
            let _ = panel.set_attribute("aria-hidden", "true");
            let mut listeners = Vec::new();
            let mut input = |which: &NodeRef, folder: bool| {
                let el = element(&panel, "input", "");
                let _ = el.set_attribute("type", "file");
                let _ = el.set_attribute("multiple", "");
                let _ = el.set_attribute("tabindex", "-1");
                if folder {
                    let _ = el.set_attribute("webkitdirectory", "");
                } else {
                    let _ = el.set_attribute("accept", ACCEPT);
                }
                which.set(&el);
                let link = shared.link.clone();
                let which = which.clone();
                let pick: Callback<()> = Callback::from(move |()| {
                    let Some(el) = which.cast::<HtmlInputElement>() else {
                        return;
                    };
                    let files = files::from_input(&el);
                    // Clear so picking the same thing again fires `change`.
                    el.set_value("");
                    link.send_message(Msg::Picked(Ok(files)));
                });
                listeners.push(EventListener::new(&el, "change", move |_| pick.emit(())));
            };
            if !folders::supported() {
                input(&shared.folder_input, true);
            }
            input(&shared.file_input, false);
            // A new song's music: one file, opened in the editor.
            let el = element(&panel, "input", "");
            let _ = el.set_attribute("type", "file");
            let _ = el.set_attribute("tabindex", "-1");
            let _ = el.set_attribute("accept", ACCEPT_MUSIC);
            shared.music_input.set(&el);
            let (link, which) = (shared.link.clone(), shared.music_input.clone());
            listeners.push(EventListener::new(&el, "change", move |_| {
                let Some(el) = which.cast::<HtmlInputElement>() else {
                    return;
                };
                let file = el.files().and_then(|f| f.get(0));
                el.set_value("");
                if let Some(file) = file {
                    link.send_message(Msg::Music(file));
                }
            }));
            let status = element(root, "p", "import-status sr-only");
            let _ = status.set_attribute("role", "status");
            let _ = status.set_attribute("aria-live", "polite");
            let drop = element(root, "div", "drop-note");
            drop.set_text_content(Some("Drop to add the songs"));
            let error = element(root, "p", "error select-error");
            Dom {
                status,
                drop,
                error,
                _listeners: listeners,
            }
        });
        let (running, report, _) = import::status();
        let status = running.or_else(import::stills_status).or_else(|| {
            report.map(|r| select_data::report_view(&r, Settings::load().import_details).summary)
        });
        let status = status.unwrap_or_default();
        if dom.status.text_content().as_deref() != Some(status.as_str()) {
            dom.status.set_text_content(Some(&status));
        }
        dom.drop.set_hidden(!self.dragging);
        match &self.load_error {
            Some(e) => {
                dom.error.set_hidden(false);
                dom.error
                    .set_text_content(Some(&format!("could not load the song list: {e}")));
            }
            None => dom.error.set_hidden(true),
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        import::watch(None);
        crate::history::watch(None);
        crate::select_preview::close();
        self.page.clear();
        // Before the host lets the surface go (WebGL2's painter is the
        // menus' surface mode's only).
        self.shared
            .movie
            .borrow_mut()
            .release(crate::web::screen::current().as_deref());
        if let Some((host, _, pads)) = &self.started {
            ui_dev::stop_ui(host, pads.as_ref());
        }
        // What the screen said goes with it.
        crate::web::forget_debug(&["songs", "ui"]);
    }
}

impl SelectPage {
    /// The screen, once the library is there.
    fn start(&mut self, ctx: &Context<Self>) {
        let Some(el) = self.root.cast::<HtmlElement>() else {
            return;
        };
        let theme = ui_dev::theme_host(Rc::new(ui_dev::nfkd));
        let env = theme.env().clone();
        // The beat: the preview's music as heard, else the BPM.
        let bpm = ddi_ui::screens::bpm_beat();
        let mut env2 = env.clone();
        env2.beat =
            Rc::new(move |id, t, b| crate::select_preview::beat(id).or_else(|| bpm(id, t, b)));
        let place = self.shared.place.borrow().clone();
        let data = self.data(&place);
        let play: PlayFn = {
            let shared = self.shared.clone();
            Rc::new(move |song, chart, _env, cx| {
                crate::web::screen::set_song_backdrop(&song.id, song.background.as_deref());
                let mut place = MenuPlace::load();
                place.known = true;
                place.chart = Some((song.id.clone(), chart.index));
                place.song = Some(song.id.clone());
                place.save();
                *shared.place.borrow_mut() = place;
                // A linked song's folder may need asking again: only the
                // gesture can.
                ask_folder(&shared, &song.id);
                // The audio context, running, for the play to ready its
                // music with before Start.
                if let Some(audio) = crate::select_preview::hand_over(cx.in_gesture()) {
                    crate::web::audio::keep_for_play(&song.id, chart.index, audio);
                }
                cx.request(UiRequest::Navigate(play_hash(&song.id, chart.index)));
            })
        };
        let options: OpenFn = Rc::new(|_env, cx| {
            cx.request(UiRequest::Navigate(crate::router::Route::Options.to_hash()));
        });
        let shell: ShellFn = {
            let shared = self.shared.clone();
            Rc::new(move |call, cx| shell_call(&shared, call, cx))
        };
        let screen = SelectScreen::new(env2, data, None, play, options)
            .with_shell(shell, self.shared.feed.clone())
            .with_editing()
            // A chart's kept plays (`docs/plans/play-history.md`, Replays).
            .with_plays(ddi_ui::state::history::plays_list(
                crate::history::plays(),
                Rc::new(crate::history::tz_minutes),
            ));
        let mut ui = Ui::new(ddi_ui::screens::DESIGN, TextMode::Mirror, true);
        // Under the boot shell, the screen comes in as the shell fades.
        ui.hold_entrances(crate::boot::shown());
        ui.set_focus_ring(false);
        ui.push(screen);
        let extra: ui_dev::DebugFn = {
            let shared = self.shared.clone();
            Box::new(move |obj: &js_sys::Object| debug_into(&shared, obj))
        };
        match ui_dev::start_ui(&el, ui, Some(theme), Some(extra)) {
            Ok(started) => {
                *self.shared.host.borrow_mut() = Some(Rc::downgrade(&started.0));
                ui_dev::use_warm_cell(&started.0, self.shared.warm.clone());
                ui_dev::set_frame_hook(&started.0, Some(movie_hook(&self.shared, env.clone())));
                let link = ctx.link().clone();
                ui_dev::attach_gfx(started.0.clone(), move |e| {
                    link.send_message(Msg::Failed(e))
                });
                self.started = Some(started);
                self.history_seen = crate::history::plays().generation();
                self.send_import();
                self.send_notices();
            }
            Err(e) => ctx.link().send_message(Msg::Failed(e)),
        }
    }

    fn data(&self, place: &MenuPlace) -> ddi_ui::screens::data::SelectData {
        let library = self.shared.library.borrow();
        let settings = Settings::load();
        let plays = crate::history::plays();
        let history = plays.history();
        let mut data = match library.as_ref() {
            Some(l) => select_data::select_data(
                l,
                &settings,
                &history,
                &self.shared.folders.borrow(),
                place,
            ),
            None => Default::default(),
        };
        data.notices = self.notices();
        data.import = select_data::import_view(&settings);
        data.autoplay = crate::router::autoplay();
        data
    }

    /// The library, the bests: to the screen.
    fn send_data(&self) {
        if self.started.is_none() {
            return;
        }
        let place = self.shared.place.borrow().clone();
        let data = self.data(&place);
        self.shared.feed.borrow_mut().data = Some(data);
        self.shared.feed.borrow_mut().notices = Some(self.notices());
        broadcast(&self.shared);
    }

    fn send_import(&self) {
        if self.started.is_none() {
            return;
        }
        self.shared.feed.borrow_mut().import = Some(select_data::import_view(&Settings::load()));
        broadcast(&self.shared);
    }

    fn send_notices(&self) {
        if self.started.is_none() {
            return;
        }
        self.shared.feed.borrow_mut().notices = Some(self.notices());
        broadcast(&self.shared);
    }

    /// The lines over the screen: errors, controllers without bindings,
    /// the storage prompt.
    fn notices(&self) -> Vec<Notice> {
        let mut out = Vec::new();
        if let Some((_, text)) = &self.said {
            out.push(Notice {
                text: text.clone(),
                link: None,
                error: true,
            });
        }
        if let Some(l) = self.shared.library.borrow().as_ref() {
            if let Some(e) = &l.imported_error {
                out.push(Notice {
                    text: format!("Imported songs are unavailable: {e}"),
                    link: None,
                    error: true,
                });
            }
            let n = l.unreadable.len();
            if n > 0 {
                out.push(Notice {
                    text: format!(
                        "{n} stored song{} could not be read.",
                        if n == 1 { "" } else { "s" }
                    ),
                    link: Some(("remove them".into(), "remove-unreadable".into())),
                    error: true,
                });
            }
        }
        for id in &self.unbound {
            out.push(Notice {
                text: format!(
                    "Controller “{id}” has no bindings for {} yet.",
                    self.unbound_layout
                ),
                link: Some(("bind it in Options".into(), "options".into())),
                error: false,
            });
        }
        if let Some(u) = self.usage {
            let (more, link) = match self.persisted {
                Some(true) => (" The browser keeps your songs until you remove them.", None),
                Some(false) => (
                    " The browser may clear imported songs when space runs low.",
                    Some(("keep them".to_string(), "persist".to_string())),
                ),
                None => ("", None),
            };
            out.push(Notice {
                text: format!(
                    "This site stores about {} in this browser.{more}",
                    select_data::format_bytes(u)
                ),
                link,
                error: false,
            });
        }
        out
    }

    /// Controllers connected with neither saved bindings nor the
    /// standard mapping, for the style shown.
    fn check_pads(&mut self) {
        let Some(pads) = self.started.as_ref().and_then(|s| s.2.as_ref()) else {
            return;
        };
        let settings = Settings::load();
        let place = self.shared.place.borrow().clone();
        // The style shown, the songs' own layouts too.
        let style = place
            .style
            .clone()
            .unwrap_or_else(|| settings.style.clone());
        let layout = self
            .shared
            .library
            .borrow()
            .as_ref()
            .and_then(|l| {
                select_data::library_styles(l)
                    .into_iter()
                    .find(|x| x.id == style)
            })
            .or_else(|| ddi_chart::Layout::builtin(&style));
        let layout_name = layout.as_ref().map_or(style.clone(), |l| l.name.clone());
        let connected: Vec<ConnectedPad> = pads
            .pads()
            .into_iter()
            .map(|p| ConnectedPad {
                vendor_product: None,
                id: p.id,
                index: p.index,
                standard: p.standard,
            })
            .collect();
        let count = layout.as_ref().map_or(1, |l| l.pads());
        let plan = layout
            .map(|l| settings.controls.plan(&l, &connected))
            .unwrap_or_default();
        let played = plan
            .iter()
            .filter(|(_, u)| matches!(u, PadUse::OnePad { .. }))
            .count();
        let mut unbound: Vec<String> = Vec::new();
        for (p, pad_use) in plan {
            let spare = pad_use == PadUse::Unused && count > 1 && played >= usize::from(count);
            if !pad_use.plays() && !spare && !unbound.contains(&p.id) {
                unbound.push(p.id.clone());
            }
        }
        if unbound != self.unbound || layout_name != self.unbound_layout {
            self.unbound = unbound;
            self.unbound_layout = layout_name;
            self.send_notices();
        }
    }
}

/// Tells the screen the feed has something, as soon as the host is free.
fn broadcast(shared: &Rc<Shared>) {
    if let Some(weak) = shared.host.borrow().clone() {
        ui_dev::broadcast_soon(weak, FEED);
    }
}

/// The play route for a chart, with the testing parameters the page was
/// opened with (`gfx`, `auto`, `bias`, `slow`; kept for the session,
/// `crate::router::keep_testing`).
fn play_hash(song: &str, chart: usize) -> String {
    let current = web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .unwrap_or_default();
    crate::router::keep_testing(&current);
    let mut hash = format!(
        "#/play?song={}&chart={chart}",
        js_sys::encode_uri_component(song)
    );
    let kept = crate::router::testing();
    if !kept.is_empty() {
        hash.push('&');
        hash.push_str(&kept);
    }
    hash
}

/// The play route for a kept play: its replay, or its results; with the
/// testing parameters kept that mean something there (`gfx`, `slow`).
fn replay_hash(id: &str, results: bool) -> String {
    let mut hash = format!("#/play?replay={}", js_sys::encode_uri_component(id));
    if results {
        hash.push_str("&view=results");
    }
    for kv in crate::router::testing().split('&') {
        if matches!(kv.split('=').next(), Some("gfx" | "slow")) {
            hash.push('&');
            hash.push_str(kv);
        }
    }
    hash
}

/// Song select's preview movie in each frame (`crate::preview_movie`):
/// the song the screen shows (its published state) followed, song select
/// told whose movie shows.
fn movie_hook(shared: &Rc<Shared>, env: ddi_ui::screens::Env) -> ui_dev::FrameHook {
    let shared = shared.clone();
    Box::new(move |gfx, encoder, ui, now_ms| {
        let song = env.state.borrow().song_id.clone();
        let entry = |id: &str| {
            let library = shared.library.borrow();
            let l = library.as_ref()?;
            l.bundled
                .iter()
                .chain(l.imported.iter().map(|s| &s.entry))
                .find(|e| e.id == id)
                .cloned()
        };
        let (busy, news) = shared.movie.borrow_mut().frame(
            gfx,
            encoder,
            ui,
            (Some(&song), env.reduced()),
            now_ms,
            &entry,
        );
        if let Some(news) = news {
            shared.feed.borrow_mut().preview_movie = Some(news);
            broadcast(&shared);
        }
        busy
    })
}

/// Asks for a linked song's kept folder (inside the gesture).
fn ask_folder(shared: &Shared, id: &str) {
    let library = shared.library.borrow();
    let Some(song) = library
        .as_ref()
        .and_then(|l| l.imported.iter().find(|s| s.entry.id == id))
    else {
        return;
    };
    if song.links.is_empty() {
        return;
    }
    if let Some(h) = song
        .folder
        .as_ref()
        .and_then(|f| shared.folders.borrow().get(f).cloned())
    {
        folders::ask(&h);
    }
}

/// What the screen asked. A selection may settle on a frame; requests
/// that need a gesture arrive synchronously from the input that asked.
fn shell_call(shared: &Rc<Shared>, call: SelectCall, cx: &mut ddi_ui::Cx) {
    let link = shared.link.clone();
    match call {
        SelectCall::Selected { song, from_pad } => {
            // A song left in a kept folder: its access asked in this
            // gesture (a key, a click, a tap; WebKit counts no pad press,
            // and there the preview waits for a tap), so its preview can
            // read it.
            if !(from_pad && is_webkit())
                && let Some(id) = &song
            {
                ask_folder(shared, id);
            }
            // Recorded even without an audio context yet: the next gesture
            // starts the song wanted.
            let entry = song.and_then(|id| {
                let library = shared.library.borrow();
                let l = library.as_ref()?;
                l.bundled
                    .iter()
                    .chain(l.imported.iter().map(|s| &s.entry))
                    .find(|e| e.id == id)
                    .cloned()
            });
            crate::select_preview::select(entry);
        }
        SelectCall::Place(p) => {
            let mut place = MenuPlace::load();
            place.remember(&p);
            if *shared.place.borrow() != place {
                place.save();
                *shared.place.borrow_mut() = place;
            }
            // The style the settings keep (the play's bindings and the
            // options' preview follow it).
            let mut settings = Settings::load();
            if settings.style != p.style && !p.style.is_empty() {
                settings.style = p.style;
                settings.save();
            }
            // The chart shown: its prompt's diagram's skin readied ahead
            // (the app's field preview, at the host's next frame).
            if let (Some(song), Some(chart)) = (&p.song, p.chart) {
                let library = shared.library.borrow();
                let controls = library.as_ref().and_then(|l| {
                    let entry = l
                        .bundled
                        .iter()
                        .chain(l.imported.iter().map(|s| &s.entry))
                        .find(|e| e.id == *song)?;
                    let c = ddi_ui::screens::prompt_controls::Controls::for_chart(
                        entry, chart, &settings,
                    )?;
                    crate::songs::note_shown_chart(entry, chart, c.layout.clone());
                    Some(c)
                });
                if let Some(c) = controls {
                    *shared.warm.borrow_mut() = Some(Rc::new(c));
                }
            }
        }
        SelectCall::Leave => {
            crate::select_preview::close();
            cx.request(UiRequest::Back);
        }
        SelectCall::WatchPlay(id) => cx.request(UiRequest::Navigate(replay_hash(&id, false))),
        SelectCall::ShowPlay(id) => cx.request(UiRequest::Navigate(replay_hash(&id, true))),
        SelectCall::RemovePlay(id) => crate::history::remove_play(&id),
        // The chart editor (`docs/plans/editor.md`).
        SelectCall::Edit { song, chart } => {
            crate::select_preview::close();
            cx.request(UiRequest::Navigate(
                crate::router::Route::Edit { song, chart }.to_hash(),
            ));
        }
        SelectCall::Open(what) => {
            if what == "calibrate" {
                cx.request(UiRequest::Navigate(
                    crate::router::Route::Calibrate.to_hash(),
                ));
            }
        }
        SelectCall::ImportDanoni { .. }
        | SelectCall::DanoniOriginDecision { .. }
        | SelectCall::CancelDanoni => {}
        SelectCall::PickFolder => {
            // In the gesture: Firefox asks before it keeps the library.
            let _ = storage::request_persist();
            if folders::supported() {
                let copy = Settings::load().import_copy;
                pick_folder(link, copy);
            } else if let Some(i) = shared.folder_input.cast::<HtmlInputElement>() {
                i.click();
            }
        }
        SelectCall::PickFiles => {
            let _ = storage::request_persist();
            if let Some(i) = shared.file_input.cast::<HtmlInputElement>() {
                i.click();
            }
        }
        // In the gesture: the picker opens only then.
        SelectCall::NewSong => {
            let _ = storage::request_persist();
            if let Some(i) = shared.music_input.cast::<HtmlInputElement>() {
                i.click();
            }
        }
        SelectCall::ImportOption(name, on) => {
            let mut settings = Settings::load();
            match name.as_str() {
                "copy" => settings.import_copy = on,
                "videos" => settings.import_videos = on,
                _ => settings.import_details = on,
            }
            settings.save();
            shared.feed.borrow_mut().import = Some(select_data::import_view(&settings));
            broadcast(shared);
        }
        SelectCall::Reread(_) | SelectCall::RemovePack(_) | SelectCall::RemoveSong(_)
            if import::status().0.is_some() =>
        {
            // Removing clears up shared files the import may be writing.
            link.send_message(Msg::Say(
                "An import is running: removing and reading folders again wait until it ends."
                    .into(),
            ));
        }
        SelectCall::Notice(id) if id == "remove-unreadable" && import::status().0.is_some() => {
            link.send_message(Msg::Say(
                "An import is running: removing waits until it ends.".into(),
            ));
        }
        SelectCall::Reread(pack) => {
            let library = shared.library.borrow();
            let Some(l) = library.as_ref() else { return };
            let songs: Vec<_> = l
                .imported
                .iter()
                .filter(|s| s.pack.to_lowercase() == pack.to_lowercase())
                .collect();
            let Some(folder) = songs.iter().find_map(|s| s.folder.clone()) else {
                return;
            };
            let Some(handle) = shared.folders.borrow().get(&folder).cloned() else {
                return;
            };
            let linked = songs.iter().any(|s| !s.links.is_empty());
            // In the gesture: reading a kept folder again may ask.
            let access = folders::access(&handle);
            link.send_future(async move {
                let files = if access.await {
                    folders::read(&handle).await
                } else {
                    Err("reading the folder again was not allowed".into())
                };
                Msg::FromFolder(
                    import::Origin {
                        folder,
                        rescan: true,
                        link: linked,
                    },
                    files,
                )
            });
        }
        SelectCall::RemovePack(pack) => {
            let ids = shared.library.borrow().as_ref().map_or(Vec::new(), |l| {
                l.imported
                    .iter()
                    .filter(|s| s.pack.to_lowercase() == pack.to_lowercase())
                    .map(|s| s.entry.id.clone())
                    .collect()
            });
            link.send_message(Msg::Remove(ids));
        }
        SelectCall::RemoveSong(id) => link.send_message(Msg::Remove(vec![id])),
        SelectCall::Notice(id) => match id.as_str() {
            "persist" => {
                if let Some(answer) = storage::request_persist() {
                    link.send_future(async move {
                        Msg::Persisted(answer.await.ok().and_then(|v| v.as_bool()))
                    });
                }
            }
            "remove-unreadable" => {
                let ids = shared
                    .library
                    .borrow()
                    .as_ref()
                    .map_or(Vec::new(), |l| l.unreadable.clone());
                link.send_message(Msg::Remove(ids));
            }
            _ => {}
        },
    }
}

/// Chromium's folder picker: it opens in the gesture itself, the handle is
/// kept for reading again, and the folder's files are imported.
fn pick_folder(link: crate::component::Scope<SelectPage>, copy: bool) {
    let picked = match folders::pick() {
        Ok(p) => p,
        Err(e) => return link.send_message(Msg::Picked(Err(e))),
    };
    link.send_future(async move {
        let handle = match picked.await {
            Ok(Some(h)) => h,
            Ok(None) => return Msg::Picked(Ok(Vec::new())),
            Err(e) => return Msg::Picked(Err(e)),
        };
        let files = folders::read(&handle).await;
        match keep_folder(&handle).await {
            Ok(folder) => Msg::FromFolder(
                import::Origin {
                    folder,
                    rescan: false,
                    link: !copy,
                },
                files,
            ),
            Err(e) => {
                web_sys::console::warn_1(&format!("keeping the folder: {e}").into());
                Msg::Picked(files)
            }
        }
    });
}

fn is_webkit() -> bool {
    web_sys::window()
        .and_then(|w| w.navigator().user_agent().ok())
        .is_some_and(|ua| ua.contains("AppleWebKit") && !ua.contains("Chrome"))
}

/// The page's own listeners: a gesture readies the previews' audio (in
/// the capture phase, before the screen hears it); files dragged over and
/// dropped anywhere import.
fn page_listeners(ctx: &Context<SelectPage>) -> Vec<EventListener> {
    let Some(window) = web_sys::window() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let capture = EventListenerOptions::run_in_capture_phase();
    for name in ["pointerdown", "keydown", "touchend"] {
        out.push(EventListener::new_with_options(
            &window,
            name,
            capture,
            |e| {
                // A key held repeating is not a gesture, nor is a
                // modifier or Escape alone (no user activation: a context
                // asked to start then is refused, and Firefox says so).
                if let Some(k) = e.dyn_ref::<web_sys::KeyboardEvent>()
                    && k.repeat()
                {
                    return;
                }
                if !crate::web::autoplay::activated(e) {
                    return;
                }
                crate::select_preview::gesture();
            },
        ));
    }
    let carries_files = |e: &DragEvent| {
        e.data_transfer()
            .is_some_and(|dt| dt.types().includes(&"Files".into(), 0))
    };
    let opts = EventListenerOptions::enable_prevent_default();
    {
        let link = ctx.link().clone();
        out.push(EventListener::new_with_options(
            &window,
            "dragover",
            opts,
            move |e| {
                let Some(e) = e.dyn_ref::<DragEvent>() else {
                    return;
                };
                if carries_files(e) {
                    // Without this the browser opens the file instead.
                    e.prevent_default();
                    link.send_message(Msg::Dragging(true));
                }
            },
        ));
    }
    {
        let link = ctx.link().clone();
        out.push(EventListener::new(&window, "dragleave", move |e| {
            if let Some(e) = e.dyn_ref::<DragEvent>()
                && e.related_target().is_none()
            {
                link.send_message(Msg::Dragging(false));
            }
        }));
    }
    {
        let link = ctx.link().clone();
        out.push(EventListener::new_with_options(
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
                let _ = storage::request_persist();
                let entries = files::drop_entries(&dt);
                if entries.is_empty() {
                    link.send_message(Msg::Dragging(false));
                    return;
                }
                link.send_future(async move { Msg::Picked(entries.collect().await) });
            },
        ));
    }
    out
}

async fn storage_usage() -> Option<f64> {
    let storage = web_sys::window()?.navigator().storage();
    let estimate = wasm_bindgen_futures::JsFuture::from(storage.estimate().ok()?)
        .await
        .ok()?;
    js_sys::Reflect::get(&estimate, &"usage".into())
        .ok()?
        .as_f64()
}

/// `__DDI_DEBUG.songs`, and `ui.select` (the screen's state, as
/// `ui.theme`).
fn debug_into(shared: &Shared, obj: &js_sys::Object) {
    let set = |o: &js_sys::Object, k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(o, &k.into(), &v);
    };
    let songs = js_sys::Object::new();
    if let Some(l) = shared.library.borrow().as_ref() {
        set(&songs, "bundled", (l.bundled.len() as f64).into());
        set(&songs, "imported", (l.imported.len() as f64).into());
        set(&songs, "unreadable", (l.unreadable.len() as f64).into());
        // Worked out once a library load (this runs every frame).
        let packs = js_sys::Array::new();
        for p in shared.packs.borrow().iter() {
            packs.push(&p.as_str().into());
        }
        set(&songs, "packs", packs.into());
        if let Some(e) = &l.imported_error {
            set(&songs, "error", e.as_str().into());
        }
    }
    // As of the import's last change (not worked out every frame).
    let d = shared.import_debug.borrow().clone();
    set(&songs, "importing", d.running.is_some().into());
    if let Some(r) = d.running {
        set(&songs, "importStatus", r.into());
    }
    if let Some(r) = import::stills_status() {
        set(&songs, "stills", r.into());
    }
    if let Some((summary, error)) = d.report {
        set(&songs, "report", summary.into());
        set(&songs, "reportError", error.into());
    }
    set(&songs, "imports", (d.finished as f64).into());
    let (wanted, playing, running) = crate::select_preview::status();
    let preview = js_sys::Object::new();
    set(
        &preview,
        "wanted",
        wanted.map_or(JsValue::NULL, |s| s.into()),
    );
    // What plays, and its window: from, length, the file's length.
    let window = playing.as_ref().map_or(JsValue::NULL, |(_, w)| {
        w.iter()
            .map(|v| JsValue::from(*v))
            .collect::<js_sys::Array>()
            .into()
    });
    set(
        &preview,
        "playing",
        playing.map_or(JsValue::NULL, |(s, _)| s.into()),
    );
    set(&preview, "window", window);
    set(&preview, "audio", running.into());
    set(&preview, "movie", shared.movie.borrow().debug().into());
    set(&songs, "preview", preview.into());
    set(obj, "songs", songs.into());
    if let Ok(ui) = js_sys::Reflect::get(obj, &"ui".into())
        && ui.is_object()
        && let Ok(theme) = js_sys::Reflect::get(&ui, &"theme".into())
    {
        let _ = js_sys::Reflect::set(&ui, &"select".into(), &theme);
    }
}
