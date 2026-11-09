//! Options at `#/options` (`docs/plans/ui-core.md`, step 6.3): our theme's
//! `OptionsScreen` on the app's canvas over the whole window, on the
//! player's settings (`ddi.settings.v1` in `localStorage`, the same
//! migrations: `ddi_ui::state::settings`), each change saved as it is
//! made (`ddi_ui::screens::options_model`).
//!
//! The page keeps what is the browser's: the controllers connected
//! (polled: some browsers show a pad only once a button is pressed), the
//! noteskins and the library's counts read from storage, the volume
//! sample (an audio context made inside the press, `crate::preview`), the
//! play history's export and import (a hidden file input the screen's
//! Import clicks inside the press, or an export dropped on the page),
//! removing imported songs, noteskins and announcers, and the chosen
//! announcer's sample line (`crate::announcers::speak_sample`). The
//! bind dialog takes controllers' presses as they are (the UI core hands
//! a pad's edges on raw while it is open).
//!
//! `window.__DDI_DEBUG.options` says what the page knows (the sample,
//! the noteskins, the controllers); `__DDI_DEBUG.ui.theme` what the
//! screen shows (`section`, `optionId`, `value`, `rows`, `notice`,
//! `preview`).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::{Rc, Weak};

use crate::component::{Component, Context, NodeRef};
use ddi_chart::Layout;
use ddi_l10n::msg::options as m;
use ddi_ui::screens::Env as ThemeEnv;
use ddi_ui::screens::errors::error_text;
use ddi_ui::screens::options::{OPTIONS_CHANGED, OptionsFeed, OptionsScreen};
use ddi_ui::screens::options_model::{OptionsModel, Reset, Sample, World};
use ddi_ui::screens::options_shell::{self, Leave, OptionsPlatform, SHARES_CHANGED, SharesFeed};
use ddi_ui::{Cx, UiRequest};
use gloo::events::{EventListener, EventListenerOptions};
use gloo::timers::callback::Interval;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::{AudioBuffer, HtmlElement, HtmlInputElement};

use crate::components::ui_dev::{self, Host, UiPage};
use crate::import;
use crate::preview::Preview;
use crate::router::{PlayRoute, Route};
use crate::settings::Settings;
use crate::songs::{Library, ManifestEntry, clear_imported};
use crate::web::files::{self, carries_files};

/// How often the controllers connected are compared with those shown.
const PAD_REFRESH_MS: u32 = 300;

pub(crate) enum Msg {
    /// The library: a bundled song for the volume sample, the imported
    /// songs, the shared background images, the layouts it has charts
    /// for.
    Library(Option<ManifestEntry>, usize, usize, Vec<Layout>),
    Noteskins(Result<crate::noteskins::Choices, String>),
    Announcers(Result<Vec<(String, String)>, String>),
    AnnouncerRemoved(Result<(), String>),
    PadsTick,
    ImportChanged,
    HistoryChanged,
    SampleStarted(Preview),
    SampleDecoded(AudioBuffer),
    SampleEnded,
    PlaysExported(Result<usize, String>),
    /// A file to import plays from (picked or dropped).
    ImportPlays(web_sys::Blob),
    /// Plays kept so far by the import.
    PlaysImporting(usize),
    PlaysImported(ddi_ui::state::history::ImportReport),
    ResetDone(Reset, Result<(), String>),
    NoteskinRemoved(Result<(), String>),
    /// The history deleted; whether that was all asked (else part of
    /// resetting everything, which says so itself).
    HistoryReset(Result<(), String>, bool),
    Failed(String),
}

/// What the screen's calls and the page share.
struct Shared {
    link: crate::component::Scope<OptionsPage>,
    model: Rc<RefCell<OptionsModel>>,
    feed: OptionsFeed,
    host: RefCell<Option<Weak<RefCell<Host>>>>,
    env: RefCell<Option<ThemeEnv>>,
    sample_song: RefCell<Option<ManifestEntry>>,
    sample: RefCell<Option<Preview>>,
    /// The announcer's sample line speaking.
    voice: RefCell<Option<crate::announcers::Sample>>,
    pads: RefCell<Option<crate::web::gamepad::Gamepads>>,
    /// The field preview's settings, handed over as they are edited.
    field: RefCell<Option<Rc<RefCell<Option<Settings>>>>>,
    /// The play history's import's file input (hidden; Import clicks it).
    plays_input: NodeRef,
}

pub(crate) struct OptionsPage {
    ui: UiPage,
    shared: Rc<Shared>,
    history_seen: u64,
    _pad_refresh: Option<Interval>,
    /// The import's input's and the page's drop listeners.
    _listeners: Vec<EventListener>,
}

impl Component for OptionsPage {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link().send_future(async {
            let shared = match crate::web::idb::Db::open().await {
                Ok(db) => crate::songs::shared_paths(&db).await.map_or(0, |p| p.len()),
                Err(_) => 0,
            };
            match Library::load().await {
                Ok(l) => {
                    let styles = crate::select_data::library_styles(&l);
                    Msg::Library(
                        l.bundled.into_iter().next(),
                        l.imported.len(),
                        shared,
                        styles,
                    )
                }
                Err(_) => Msg::Library(None, 0, shared, Vec::new()),
            }
        });
        ctx.link()
            .send_future(async { Msg::Noteskins(crate::noteskins::choices().await) });
        ctx.link()
            .send_future(async { Msg::Announcers(crate::announcers::choices().await) });
        import::watch(Some(ctx.link().callback(|_| Msg::ImportChanged)));
        {
            let link = ctx.link().clone();
            crate::history::watch(Some(Rc::new(move || {
                link.send_message(Msg::HistoryChanged)
            })));
        }
        let settings = Settings::load();
        let world = World {
            plays: crate::history::plays().history().len(),
            importing: import::status().0.is_some(),
            sharing: settings.sharing,
            share_url: crate::share::base(),
            ..World::default()
        };
        let shared = Rc::new(Shared {
            link: ctx.link().clone(),
            model: Rc::new(RefCell::new(OptionsModel::new(settings, world))),
            feed: Rc::new(RefCell::new(None)),
            host: RefCell::new(None),
            env: RefCell::new(None),
            sample_song: RefCell::new(None),
            sample: RefCell::new(None),
            voice: RefCell::new(None),
            pads: RefCell::new(None),
            field: RefCell::new(None),
            plays_input: NodeRef::default(),
        });
        let link = ctx.link().clone();
        OptionsPage {
            ui: UiPage::default(),
            shared,
            history_seen: crate::history::plays().generation(),
            _pad_refresh: Some(Interval::new(PAD_REFRESH_MS, move || {
                link.send_message(Msg::PadsTick)
            })),
            _listeners: Vec::new(),
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.start(ctx);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        let s = &self.shared;
        match msg {
            Msg::Library(song, imported, shared, styles) => {
                let mut m = s.model.borrow_mut();
                m.world.sample = if song.is_some() {
                    Sample::Ready
                } else {
                    Sample::None
                };
                m.world.imported = imported;
                m.world.shared = shared;
                m.world.library_styles = styles;
                m.follow_style();
                drop(m);
                *s.sample_song.borrow_mut() = song;
                send(s);
            }
            Msg::Noteskins(Ok(c)) => {
                s.model.borrow_mut().set_noteskins(
                    c.skins,
                    c.default_lua_off.is_some(),
                    c.complete,
                );
                s.model.borrow_mut().set_skin_parameters(c.parameters);
                // A chosen skin let go: the preview follows.
                if let Some(f) = s.field.borrow().as_ref() {
                    *f.borrow_mut() = Some(s.model.borrow().preview_settings());
                }
                send(s);
            }
            // Neither storage nor the bundled skins could be read (another
            // tab upgrading storage, the network): the choice stays.
            Msg::Noteskins(Err(e)) => web_sys::console::warn_1(&e.into()),
            Msg::Announcers(Ok(list)) => {
                s.model.borrow_mut().set_announcers(list, true);
                send(s);
            }
            // Storage could not be read: the choice stays.
            Msg::Announcers(Err(e)) => web_sys::console::warn_1(&e.into()),
            Msg::AnnouncerRemoved(r) => {
                s.model.borrow_mut().notice = match r {
                    Ok(()) => m::notice_announcer_removed(),
                    Err(e) => m::notice_announcer_not_removed(error_text(&e)),
                }
                .into();
                s.link
                    .send_future(async { Msg::Announcers(crate::announcers::choices().await) });
                send(s);
            }
            Msg::PadsTick => {
                let Some(g) = s.pads.borrow().clone() else {
                    return false;
                };
                let pads = g.connected();
                if pads != s.model.borrow().world.connected {
                    s.model.borrow_mut().world.connected = pads;
                    send(s);
                }
            }
            Msg::ImportChanged => {
                let importing = import::status().0.is_some();
                if importing != s.model.borrow().world.importing {
                    s.model.borrow_mut().world.importing = importing;
                    send(s);
                }
            }
            Msg::HistoryChanged => {
                let plays = crate::history::plays();
                if plays.generation() != self.history_seen {
                    self.history_seen = plays.generation();
                    s.model.borrow_mut().world.plays = plays.history().len();
                    send(s);
                }
            }
            Msg::SampleStarted(preview) => {
                let load = preview.load();
                let link = s.link.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    link.send_message(match load.await {
                        Ok(buffer) => Msg::SampleDecoded(buffer),
                        Err(e) => {
                            web_sys::console::warn_1(&format!("volume sample: {e}").into());
                            Msg::SampleEnded
                        }
                    });
                });
                *s.sample.borrow_mut() = Some(preview);
            }
            Msg::SampleDecoded(buffer) => {
                let volume = s.model.borrow().settings.volume;
                let link = s.link.clone();
                let failed = s.sample.borrow_mut().as_mut().is_some_and(|p| {
                    p.play(&buffer, volume, move || link.send_message(Msg::SampleEnded))
                        .is_err()
                });
                if failed {
                    stop_sample(s);
                }
            }
            Msg::SampleEnded => stop_sample(s),
            Msg::PlaysExported(r) => {
                s.model.borrow_mut().notice = match r {
                    Ok(n) => m::notice_exported(n),
                    Err(e) => m::notice_export_failed(error_text(&e)),
                }
                .into();
                send(s);
            }
            Msg::ImportPlays(file) => {
                if s.model.borrow().world.importing_plays {
                    return false;
                }
                {
                    let mut m = s.model.borrow_mut();
                    m.world.importing_plays = true;
                    m.notice = ddi_l10n::msg::options::notice_importing().into();
                }
                send(s);
                let link = s.link.clone();
                s.link.send_future(async move {
                    let report = crate::history::import(file, move |n| {
                        link.send_message(Msg::PlaysImporting(n))
                    })
                    .await;
                    Msg::PlaysImported(report)
                });
            }
            Msg::PlaysImporting(n) => {
                let mut m = s.model.borrow_mut();
                if m.world.importing_plays {
                    m.notice = ddi_l10n::msg::options::notice_importing_kept(n).into();
                    drop(m);
                    send(s);
                }
            }
            Msg::PlaysImported(report) => {
                {
                    let mut m = s.model.borrow_mut();
                    m.world.importing_plays = false;
                    m.notice = report.notice();
                    m.world.plays = crate::history::plays().history().len();
                }
                send(s);
            }
            Msg::ResetDone(r, result) => {
                {
                    let mut m = s.model.borrow_mut();
                    m.notice = match (&r, result) {
                        (_, Err(e)) => {
                            ddi_l10n::msg::options::notice_songs_not_removed(error_text(&e))
                        }
                        (Reset::Everything, Ok(())) => {
                            ddi_l10n::msg::options::notice_everything_removed()
                        }
                        (_, Ok(())) => ddi_l10n::msg::options::notice_songs_removed(),
                    }
                    .into();
                    m.world.imported = 0;
                    m.world.shared = 0;
                }
                s.link
                    .send_future(async { Msg::Noteskins(crate::noteskins::choices().await) });
                s.link
                    .send_future(async { Msg::Announcers(crate::announcers::choices().await) });
                send(s);
            }
            Msg::HistoryReset(r, alone) => {
                {
                    let mut m = s.model.borrow_mut();
                    match r {
                        Ok(()) if alone => {
                            m.notice = ddi_l10n::msg::options::notice_history_deleted().into()
                        }
                        Ok(()) => {}
                        Err(e) => {
                            m.notice =
                                ddi_l10n::msg::options::notice_history_not_deleted(error_text(&e))
                                    .into()
                        }
                    }
                    m.world.plays = crate::history::plays().history().len();
                }
                send(s);
            }
            Msg::NoteskinRemoved(r) => {
                s.model.borrow_mut().notice = match r {
                    Err(e) => m::notice_noteskin_not_removed(error_text(&e)),
                    Ok(_) => m::notice_noteskin_removed(),
                }
                .into();
                s.link
                    .send_future(async { Msg::Noteskins(crate::noteskins::choices().await) });
                send(s);
            }
            Msg::Failed(e) => {
                self.ui.fail(e);
                return true;
            }
        }
        false
    }

    fn view(&self, ctx: &Context<Self>) {
        self.ui.view(ctx, "select-page options-page");
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        import::watch(None);
        crate::history::watch(None);
        stop_sample(&self.shared);
        self.shared.voice.borrow_mut().take();
        self.ui.destroy(&["options", "ui"]);
    }
}

impl OptionsPage {
    fn start(&mut self, ctx: &Context<Self>) {
        self._listeners = import_listeners(ctx.root(), &self.shared, ctx);
        let theme = ui_dev::theme_host(Rc::new(ui_dev::nfkd));
        let env = theme.env().clone();
        *self.shared.env.borrow_mut() = Some(env.clone());
        let shell = options_shell::shell(
            env.clone(),
            self.shared.model.clone(),
            self.shared.feed.clone(),
            Rc::new(Web(self.shared.clone())),
        );
        let data = self.shared.model.borrow().data();
        let screen = OptionsScreen::new(env, data, None)
            .with_position(crate::router::saved_options())
            .with_shell(shell, self.shared.feed.clone());
        let mut ui = crate::components::ui_dev::theme_ui();
        ui.push(screen);
        let extra: ui_dev::DebugFn = {
            let shared = self.shared.clone();
            Box::new(move |obj: &js_sys::Object| debug_into(&shared, obj))
        };
        let Some(started) = self
            .ui
            .start(ctx, ui, Some(theme), Some(extra), Msg::Failed)
        else {
            return;
        };
        *self.shared.host.borrow_mut() = Some(Rc::downgrade(&started.0));
        *self.shared.pads.borrow_mut() = started.2.clone();
        let field = ui_dev::field_settings(&started.0);
        *field.borrow_mut() = Some(self.shared.model.borrow().preview_settings());
        *self.shared.field.borrow_mut() = Some(field);
        self.shared.model.borrow_mut().world.pads_supported = started.2.is_some();
        send(&self.shared);
        ctx.link().send_message(Msg::PadsTick);
    }
}

/// What an export is: its file types, for the picker.
const PLAYS_ACCEPT: &str =
    ".jsonl,.ndjson,.json,.txt,application/x-ndjson,application/json,text/plain";

/// The play history's import: a hidden file input (the screen's Import
/// clicks it) and a file dropped anywhere on the page.
fn import_listeners(
    root: &HtmlElement,
    shared: &Rc<Shared>,
    ctx: &Context<OptionsPage>,
) -> Vec<EventListener> {
    let mut out = Vec::new();
    let input = files::file_input(root, Some(PLAYS_ACCEPT), false);
    let _ = input.set_attribute("aria-hidden", "true");
    let _ = input.set_attribute("data-ddi", "import-plays");
    input.set_hidden(true);
    shared.plays_input.set(&input);
    {
        let link = ctx.link().clone();
        let el = input.clone();
        out.push(EventListener::new(&input, "change", move |_| {
            let Some(input) = el.dyn_ref::<HtmlInputElement>() else {
                return;
            };
            let file = input.files().and_then(|l| l.get(0));
            // Cleared so the same file again is a change too.
            input.set_value("");
            if let Some(file) = file {
                link.send_message(Msg::ImportPlays(file.into()));
            }
        }));
    }
    let Some(window) = web_sys::window() else {
        return out;
    };
    let opts = EventListenerOptions::enable_prevent_default();
    out.push(EventListener::new_with_options(
        &window,
        "dragover",
        opts,
        move |e| {
            if let Some(e) = e.dyn_ref::<web_sys::DragEvent>()
                && carries_files(e)
            {
                // Without this the browser opens the file instead.
                e.prevent_default();
            }
        },
    ));
    let link = ctx.link().clone();
    out.push(EventListener::new_with_options(
        &window,
        "drop",
        opts,
        move |e| {
            let Some(e) = e.dyn_ref::<web_sys::DragEvent>() else {
                return;
            };
            let Some(dt) = e.data_transfer().filter(|_| carries_files(e)) else {
                return;
            };
            e.prevent_default();
            if let Some(file) = dt.files().and_then(|l| l.get(0)) {
                link.send_message(Msg::ImportPlays(file.into()));
            }
        },
    ));
    out
}

/// Puts the rows in the feed and tells the screen, once the host is free.
fn send(shared: &Rc<Shared>) {
    *shared.feed.borrow_mut() = Some(shared.model.borrow().data());
    if let Some(weak) = shared.host.borrow().clone() {
        ui_dev::broadcast_soon(weak, OPTIONS_CHANGED);
    }
}

fn stop_sample(shared: &Rc<Shared>) {
    // Dropping the preview closes its audio context.
    shared.sample.borrow_mut().take();
    let mut m = shared.model.borrow_mut();
    if m.world.sample == Sample::Playing {
        m.world.sample = Sample::Ready;
        drop(m);
        send(shared);
    }
}

/// What the page does for the screen ([`OptionsPlatform`]).
struct Web(Rc<Shared>);

impl OptionsPlatform for Web {
    fn sample(&self, play: bool, _volume: f32, cx: &mut Cx) -> bool {
        let shared = &self.0;
        if !play {
            shared.sample.borrow_mut().take();
            return false;
        }
        let Some(song) = shared.sample_song.borrow().clone() else {
            return false;
        };
        // A pad's press on a page with no activation yet: a context made
        // now could not start (the browser would say so); the next key,
        // click or tap plays it.
        if (!cx.in_gesture() || cx.menu_from_pad()) && !crate::web::autoplay::may_start() {
            return false;
        }
        // Inside the press itself (Safari).
        match Preview::start(&song) {
            Ok(p) => {
                shared.link.send_message(Msg::SampleStarted(p));
                true
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("volume sample: {e}").into());
                false
            }
        }
    }

    fn volume(&self, volume: f32) {
        if let Some(p) = self.0.sample.borrow().as_ref() {
            p.set_volume(volume);
        }
    }

    /// A context made inside the press (as the volume sample's), the line
    /// read and decoded after.
    fn announce(&self, key: &str, volume: f32, share: f32, cx: &mut Cx) {
        if (!cx.in_gesture() || cx.menu_from_pad()) && !crate::web::autoplay::may_start() {
            return;
        }
        // One line at a time: the last one's context closes.
        self.0.voice.borrow_mut().take();
        match crate::announcers::speak_sample(key, volume, share) {
            Ok(v) => *self.0.voice.borrow_mut() = Some(v),
            Err(e) => web_sys::console::warn_1(&format!("announcer sample: {e}").into()),
        }
    }

    fn export(&self) {
        let link = self.0.link.clone();
        crate::history::export(move |r| link.send_message(Msg::PlaysExported(r)));
    }

    /// The file picker, inside the press (a browser opens it only from a
    /// key, a click or a tap: not from a controller's press).
    fn import_plays(&self, cx: &mut Cx) {
        if !cx.in_gesture() || cx.menu_from_pad() {
            self.0.model.borrow_mut().notice =
                ddi_l10n::msg::options::notice_import_gesture().into();
            send(&self.0);
            return;
        }
        if let Some(input) = self.0.plays_input.cast::<HtmlInputElement>() {
            input.click();
        }
    }

    /// The shares read on a task; the dialog told through its feed.
    fn list_shares(&self, into: SharesFeed) {
        let host = self.0.host.borrow().clone();
        wasm_bindgen_futures::spawn_local(async move {
            let token = Settings::load().share_token;
            let view = match crate::share::list(&token).await {
                Ok(v) => v,
                Err(e) => ddi_ui::screens::data::SharesView::failed(e),
            };
            *into.borrow_mut() = view;
            if let Some(h) = host {
                ui_dev::broadcast_soon(h, SHARES_CHANGED);
            }
        });
    }

    /// The share's page in this tab (`#/s/<id>`): Back returns here.
    fn open_share(&self, id: &str, cx: &mut Cx) -> bool {
        cx.request(ddi_ui::UiRequest::Navigate(
            crate::router::Route::Shared { id: id.to_string() }.to_hash(),
        ));
        true
    }

    fn delete_share(&self, id: &str, into: SharesFeed) {
        let host = self.0.host.borrow().clone();
        let id = id.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let token = Settings::load().share_token;
            let view = match crate::share::delete(&token, &id).await {
                Ok(()) => crate::share::list(&token).await,
                Err(e) => Err(e),
            };
            *into.borrow_mut() = match view {
                Ok(v) => v,
                Err(e) => ddi_ui::screens::data::SharesView::failed(e),
            };
            if let Some(h) = host {
                ui_dev::broadcast_soon(h, SHARES_CHANGED);
            }
        });
    }

    fn remove(&self, reset: &Reset) {
        let link = self.0.link.clone();
        match reset.clone() {
            Reset::Settings => {}
            reset @ (Reset::Songs | Reset::Everything) => {
                // Everything: the play history too.
                if reset == Reset::Everything {
                    let l = link.clone();
                    crate::history::remove_all(move |r| {
                        l.send_message(Msg::HistoryReset(r, false))
                    });
                }
                wasm_bindgen_futures::spawn_local(async move {
                    let r = clear_imported().await;
                    link.send_message(Msg::ResetDone(reset, r));
                });
            }
            Reset::Noteskin(key) => {
                wasm_bindgen_futures::spawn_local(async move {
                    let r = crate::noteskins::delete(&key).await;
                    link.send_message(Msg::NoteskinRemoved(r));
                });
            }
            Reset::Announcer(key) => {
                self.0.voice.borrow_mut().take();
                wasm_bindgen_futures::spawn_local(async move {
                    let r = crate::announcers::delete(&key).await;
                    link.send_message(Msg::AnnouncerRemoved(r));
                });
            }
            // The `plays` and `play_summaries` stores emptied.
            Reset::History => {
                crate::history::remove_all(move |r| link.send_message(Msg::HistoryReset(r, true)));
            }
        }
    }

    fn held(&self, id: &str) -> HashSet<String> {
        self.0
            .pads
            .borrow()
            .as_ref()
            .map_or_else(HashSet::new, |g| {
                g.pads()
                    .into_iter()
                    .filter(|p| p.id == id)
                    .flat_map(|p| p.active)
                    .map(|c| c.to_string())
                    .collect()
            })
    }

    fn changed(&self, model: &OptionsModel) {
        if let Some(env) = self.0.env.borrow().as_ref() {
            env.follow_cursor(&model.settings);
        }
        // The preview shows the settings as edited, from the next frame.
        if let Some(f) = self.0.field.borrow().as_ref() {
            *f.borrow_mut() = Some(model.preview_settings());
        }
    }

    fn motion(&self, _model: &OptionsModel) {
        follow_motion(&self.0);
    }

    fn leave(&self, to: Leave, cx: &mut Cx) {
        let route = match to {
            Leave::Back => {
                cx.request(UiRequest::Back);
                return;
            }
            Leave::Calibrate => Route::Calibrate,
            Leave::Studio => Route::Studio,
            Leave::Demo { gl } => Route::Play(PlayRoute {
                force_gl: gl,
                auto: true,
                ..PlayRoute::song("some-things-must", 2)
            }),
        };
        cx.request(UiRequest::Navigate(route.to_hash()));
    }
}

/// Reduced motion as the settings now say, followed at once.
fn follow_motion(shared: &Rc<Shared>) {
    let Some(env) = shared.env.borrow().clone() else {
        return;
    };
    let setting = shared.model.borrow().settings.reduced_motion;
    env.follow_motion(setting, ui_dev::system_reduced());
}

/// `__DDI_DEBUG.options`.
fn debug_into(shared: &Shared, obj: &js_sys::Object) {
    let set = |o: &js_sys::Object, k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(o, &k.into(), &v);
    };
    let m = shared.model.borrow();
    let o = js_sys::Object::new();
    set(
        &o,
        "sample",
        match m.world.sample {
            Sample::None => "none",
            Sample::Ready => "ready",
            Sample::Playing => "playing",
        }
        .into(),
    );
    set(&o, "playing", shared.sample.borrow().is_some().into());
    let skins = js_sys::Array::new();
    for (k, _) in &m.world.noteskins {
        skins.push(&k.as_str().into());
    }
    set(&o, "noteskins", skins.into());
    let announcers = js_sys::Array::new();
    for (k, _) in &m.world.announcers {
        announcers.push(&k.as_str().into());
    }
    set(&o, "announcers", announcers.into());
    // The last sample line: `{spoken}` or `{error}`.
    if let Some(r) = crate::announcers::spoken() {
        let a = js_sys::Object::new();
        match r {
            Ok(path) => set(&a, "spoken", path.as_str().into()),
            Err(e) => set(&a, "error", e.as_str().into()),
        }
        set(&o, "announcer", a.into());
    }
    let pads = js_sys::Array::new();
    for p in &m.world.connected {
        pads.push(&p.id.as_str().into());
    }
    set(&o, "pads", pads.into());
    set(&o, "bindLayout", m.bind_layout.id.as_str().into());
    set(&o, "notice", m.notice.as_str().into());
    set(&o, "imported", (m.world.imported as f64).into());
    set(&o, "plays", (m.world.plays as f64).into());
    set(&o, "importingPlays", m.world.importing_plays.into());
    set(obj, "options", o.into());
}
