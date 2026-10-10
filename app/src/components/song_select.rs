//! Song list with difficulty picker and audio preview, plus the import
//! panel (folder or zip pickers and drag and drop anywhere on the page).
//!
//! Songs are grouped into packs, the bundled songs first, as an accordion:
//! one pack open at a time, each headed by its banner. The list keeps its
//! place across a play (open pack, last chart started) and moves with the
//! arrow keys, a controller or a dance pad ([`crate::menu`]).
//!
//! It also watches the connected controllers and points to the options
//! when one has no bindings: a controller without the standard mapping has
//! no defaults (its buttons are numbered arbitrarily), so it would do
//! nothing in a song.

use std::collections::HashMap;

use ddi_chart::Layout;
use ddi_engine::{ConnectedPad, PadUse};
use ddi_platform::{DeviceId, RawInput};
use gloo::events::{EventListener, EventListenerOptions};
use gloo::timers::callback::Interval;
use gloo::timers::callback::Timeout;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{AudioBuffer, DragEvent, HtmlInputElement, KeyboardEvent, ScrollLogicalPosition};
use yew::prelude::*;

use crate::import::{self, ImportReport};
use crate::menu::{self, MenuPlace, Nav};
use crate::mods::mods_summary;
use crate::preview::Preview;
use crate::router::Route;
use crate::settings::Settings;
use crate::songs::{
    Library, ManifestEntry, delete_imported, imported_banner_urls, keep_folder, kept_folders,
    pack_art_urls,
};

use crate::web::files::{self, PickedFile, revoke_object_url};
use crate::web::gamepad::Gamepads;
use crate::web::{folders, storage};

/// How often the connected controllers are checked for missing bindings.
const PAD_CHECK_MS: u32 = 500;

pub(crate) enum Msg {
    Loaded(Result<Library, String>),
    /// Object URLs for imported banners, by song id, for the library load
    /// numbered by the first field.
    Banners(u64, Vec<(String, String)>),
    /// A preview whose audio context the click handler created.
    PreviewStarted(Preview),
    /// The preview's song is decoded; play it.
    PreviewDecoded(String, AudioBuffer),
    /// Stop the preview of this song, if it is the one playing.
    PreviewEnded(String),
    Dragging(bool),
    /// Files picked or dropped; starts an import.
    Picked(Result<Vec<PickedFile>, String>),
    /// The shared import state changed (progress, or an import finished).
    ImportChanged,
    /// First click on a remove button: ask for a second one.
    AskRemove(Removal),
    Remove(Removal),
    Removed(Result<(), String>),
    Usage(Option<f64>),
    /// Whether the browser keeps the library (`navigator.storage.persisted`).
    Persisted(Option<bool>),
    /// The "keep them" button: ask the browser to keep the library.
    Persist,
    /// The "details for pack authors" box of the import panel.
    ImportDetails(bool),
    /// The "store a chosen folder's songs in the browser" box.
    ImportCopy(bool),
    /// The "also store background videos" box.
    ImportVideos(bool),
    /// Timer: look for connected controllers without bindings.
    PadsTick,
    /// Show the charts of this layout.
    Style(String),
    /// Open this pack (closing the others), or close it when open.
    Toggle(String),
    /// Pack banner URLs by lowercased pack name, for the library load
    /// numbered by the first field.
    PackArt(u64, Vec<(String, String)>),
    /// Close this pack (Escape on its header).
    Close(String),
    /// A controller edge, for moving through the list.
    PadEdge(RawInput),
    /// A held direction repeats.
    Repeat(Nav),
    /// The pause before repeating a held control is over.
    RepeatSteps(HeldKey),
    /// Kept folder handles, by storage key.
    Folders(Vec<(String, JsValue)>),
    /// A kept folder was picked or read again; import it.
    FromFolder(import::Origin, Result<Vec<PickedFile>, String>),
}

/// What a remove button removes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Removal {
    Song(String),
    Pack(String),
    /// Stored records that no longer parse.
    Unreadable,
}

pub(crate) struct SongSelect {
    state: State,
    banners: HashMap<String, String>,
    preview: Option<Preview>,
    settings: Settings,
    dragging: bool,
    /// Bumped on every library load; banner URLs from older loads are dropped.
    banner_load: u64,
    /// Finished imports already reflected in the library.
    seen_imports: u64,
    confirm: Option<Removal>,
    /// Bytes this site stores, from `navigator.storage.estimate()`.
    usage: Option<f64>,
    /// Whether the browser keeps the library.
    persisted: Option<bool>,
    folder_input: NodeRef,
    file_input: NodeRef,
    /// Polled once per frame only, to notice controllers.
    gamepads: Option<Gamepads>,
    /// Connected controllers with neither saved bindings nor the standard
    /// mapping.
    unbound_pads: Vec<String>,
    /// Key of the open pack ([`menu::pack_key`]).
    open: Option<String>,
    /// Whether `open` is known (from the saved place or a default).
    open_known: bool,
    /// Where the list was, restored once the library is drawn.
    place: Option<MenuPlace>,
    /// A pack just opened, to scroll to once drawn.
    scroll_to: Option<String>,
    /// Pack banner object URLs by lowercased pack name.
    pack_art: HashMap<String, String>,
    /// Kept folder handles (Chromium), by storage key.
    folders: HashMap<String, JsValue>,
    /// The held direction and the timer repeating it.
    repeat: Option<(HeldKey, Nav, RepeatTimer)>,
    _drag: Vec<EventListener>,
    _keys: Option<EventListener>,
    _pointer: Option<EventListener>,
    _pad_check: Option<Interval>,
}

/// Which control is held: the pad (two identical pads share an id, told
/// apart by slot) and the control.
type HeldKey = (DeviceId, u32, String);

/// Holding a direction: a pause, then steady steps.
enum RepeatTimer {
    Delay(#[allow(dead_code)] Timeout),
    Steps(#[allow(dead_code)] Interval),
}

/// Before a held direction repeats, and between repeats, ms.
const REPEAT_DELAY_MS: u32 = 350;
const REPEAT_MS: u32 = 110;

enum State {
    Loading,
    Ready(Library),
    Failed(String),
}

impl Component for SongSelect {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        ctx.link()
            .send_future(async { Msg::Loaded(Library::load().await) });
        import::watch(Some(ctx.link().callback(|_| Msg::ImportChanged)));
        let gamepads = Gamepads::new();
        if let Some(g) = &gamepads {
            let link = ctx.link().clone();
            g.set_listener(Some(Box::new(move |edge: &RawInput| {
                link.send_message(Msg::PadEdge(edge.clone()));
            })));
        }
        let place = MenuPlace::load();
        let pad_check = gamepads.as_ref().map(|_| {
            let link = ctx.link().clone();
            Interval::new(PAD_CHECK_MS, move || link.send_message(Msg::PadsTick))
        });
        SongSelect {
            state: State::Loading,
            banners: HashMap::new(),
            preview: None,
            settings: Settings::load(),
            dragging: false,
            banner_load: 0,
            seen_imports: import::status().2,
            confirm: None,
            usage: None,
            persisted: None,
            folder_input: NodeRef::default(),
            file_input: NodeRef::default(),
            gamepads,
            unbound_pads: Vec::new(),
            open: place.open.clone(),
            open_known: place.known,
            place: Some(place),
            scroll_to: None,
            pack_art: HashMap::new(),
            folders: HashMap::new(),
            repeat: None,
            _drag: drag_listeners(ctx),
            _keys: key_listener(ctx),
            _pointer: pointer_listener(),
            _pad_check: pad_check,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Loaded(Ok(library)) => {
                self.banner_load += 1;
                let load = self.banner_load;
                let wanted: Vec<ManifestEntry> =
                    library.imported.iter().map(|s| s.entry.clone()).collect();
                ctx.link().send_future(async move {
                    Msg::Banners(load, imported_banner_urls(&wanted).await)
                });
                let mut packs: Vec<String> =
                    library.imported.iter().map(|s| s.pack.clone()).collect();
                packs.sort_by_key(|p| p.to_lowercase());
                packs.dedup_by_key(|p| p.to_lowercase());
                ctx.link()
                    .send_future(async move { Msg::PackArt(load, pack_art_urls(&packs).await) });
                if folders::supported() {
                    ctx.link()
                        .send_future(async { Msg::Folders(kept_folders().await) });
                }
                // A remembered pack that is gone counts as a first visit.
                let exists = |key: &String| {
                    *key == menu::pack_key(None) && !library.bundled.is_empty()
                        || library
                            .imported
                            .iter()
                            .any(|s| menu::pack_key(Some(&s.pack)) == *key)
                };
                if self.open.as_ref().is_some_and(|k| !exists(k)) {
                    self.open_known = false;
                }
                if !self.open_known {
                    // First visit: the bundled songs, else the first pack.
                    self.open_known = true;
                    self.open = Some(match library.imported.first() {
                        Some(s) if library.bundled.is_empty() => menu::pack_key(Some(&s.pack)),
                        _ => menu::pack_key(None),
                    });
                }
                if library.imported.is_empty() {
                    self.usage = None;
                } else {
                    ctx.link()
                        .send_future(async { Msg::Usage(storage_usage().await) });
                    ctx.link()
                        .send_future(async { Msg::Persisted(storage::persisted().await) });
                }
                self.state = State::Ready(library);
            }
            Msg::Loaded(Err(e)) => self.state = State::Failed(e),
            Msg::ImportCopy(on) => {
                let mut settings = Settings::load();
                settings.import_copy = on;
                settings.save();
                self.settings = settings;
            }
            Msg::ImportVideos(on) => {
                let mut settings = Settings::load();
                settings.import_videos = on;
                settings.save();
                self.settings = settings;
            }
            Msg::ImportDetails(on) => {
                let mut settings = Settings::load();
                settings.import_details = on;
                settings.save();
                self.settings = settings;
            }
            Msg::Banners(load, urls) => {
                if load != self.banner_load {
                    urls.iter().for_each(|(_, u)| revoke_object_url(u));
                    return false;
                }
                self.revoke_banners();
                self.banners = urls.into_iter().collect();
            }
            Msg::PreviewStarted(preview) => {
                self.stop_preview();
                let id = preview.id.clone();
                let load = preview.load();
                ctx.link().send_future(async move {
                    match load.await {
                        Ok(buffer) => Msg::PreviewDecoded(id, buffer),
                        Err(e) => {
                            web_sys::console::warn_1(&format!("preview: {e}").into());
                            Msg::PreviewEnded(id)
                        }
                    }
                });
                self.preview = Some(preview);
            }
            Msg::PreviewDecoded(id, buffer) => {
                let Some(p) = self.preview.as_mut().filter(|p| p.id == id) else {
                    return false;
                };
                let link = ctx.link().clone();
                let end = move || link.send_message(Msg::PreviewEnded(id));
                if p.play(&buffer, self.settings.volume, end).is_err() {
                    self.stop_preview();
                    return true;
                }
                return false;
            }
            Msg::PreviewEnded(id) => {
                if self.preview.as_ref().is_some_and(|p| p.id == id) {
                    self.stop_preview();
                } else {
                    return false;
                }
            }
            Msg::Dragging(on) => {
                if self.dragging == on {
                    return false;
                }
                self.dragging = on;
            }
            Msg::Picked(Err(e)) => {
                self.dragging = false;
                import::report_error(e);
            }
            Msg::Picked(Ok(files)) => {
                self.dragging = false;
                if import::start(files) {
                    self.stop_preview();
                }
            }
            Msg::ImportChanged => {
                let finished = import::status().2;
                if finished != self.seen_imports {
                    self.seen_imports = finished;
                    ctx.link()
                        .send_future(async { Msg::Loaded(Library::load().await) });
                }
            }
            Msg::AskRemove(r) => self.confirm = Some(r),
            Msg::Remove(r) => {
                self.confirm = None;
                let State::Ready(library) = &self.state else {
                    return false;
                };
                let ids: Vec<String> = match &r {
                    Removal::Unreadable => library.unreadable.clone(),
                    Removal::Song(_) | Removal::Pack(_) => library
                        .imported
                        .iter()
                        .filter(|s| match &r {
                            Removal::Song(id) => &s.entry.id == id,
                            Removal::Pack(pack) => &s.pack == pack,
                            Removal::Unreadable => false,
                        })
                        .map(|s| s.entry.id.clone())
                        .collect(),
                };
                if self.preview.as_ref().is_some_and(|p| ids.contains(&p.id)) {
                    self.stop_preview();
                }
                let mut settings = Settings::load();
                for id in &ids {
                    settings.set_song_offset(id, 0.0);
                }
                settings.save();
                self.settings = settings;
                ctx.link()
                    .send_future(async move { Msg::Removed(delete_imported(&ids).await) });
            }
            Msg::Removed(r) => {
                if let Err(e) = r {
                    import::report_error(e);
                }
                ctx.link()
                    .send_future(async { Msg::Loaded(Library::load().await) });
                return false;
            }
            Msg::Toggle(key) => {
                if self.open.as_ref() == Some(&key) {
                    self.open = None;
                } else {
                    self.open = Some(key.clone());
                    self.scroll_to = Some(key);
                }
                let mut place = MenuPlace::load();
                place.known = true;
                place.open = self.open.clone();
                place.save();
            }
            Msg::PackArt(load, urls) => {
                if load != self.banner_load {
                    urls.iter().for_each(|(_, u)| revoke_object_url(u));
                    return false;
                }
                self.revoke_pack_art();
                self.pack_art = urls.into_iter().collect();
            }
            Msg::Folders(f) => self.folders = f.into_iter().collect(),
            Msg::FromFolder(_, Err(e)) => import::report_error(e),
            Msg::FromFolder(origin, Ok(files)) => {
                if import::start_from(files, origin) {
                    self.stop_preview();
                }
                // Read again with the new handle.
                if folders::supported() {
                    ctx.link()
                        .send_future(async { Msg::Folders(kept_folders().await) });
                }
            }
            Msg::Close(key) => return self.close(&key),
            Msg::Repeat(nav) => {
                // A hidden tab stops polling pads, so no release would come.
                let hidden = web_sys::window()
                    .and_then(|w| w.document())
                    .is_some_and(|d| d.hidden());
                if hidden {
                    self.repeat = None;
                    return false;
                }
                return self.navigate(ctx, nav);
            }
            Msg::PadEdge(edge) => {
                let DeviceId::Gamepad(id) = &edge.device else {
                    return false;
                };
                let standard = self
                    .gamepads
                    .as_ref()
                    .and_then(|g| g.pads().into_iter().find(|p| p.index == edge.slot))
                    .is_some_and(|p| p.standard);
                let held: HeldKey = (edge.device.clone(), edge.slot, edge.control.clone());
                if !edge.pressed {
                    if self.repeat.as_ref().is_some_and(|(h, ..)| *h == held) {
                        self.repeat = None;
                    }
                    return false;
                }
                let Some(nav) = self
                    .settings
                    .controls
                    .menu_input(id, standard, &edge.control)
                else {
                    return false;
                };
                self.repeat = menu::repeats(nav).then(|| {
                    let link = ctx.link().clone();
                    let key = held.clone();
                    let timer = Timeout::new(REPEAT_DELAY_MS, move || {
                        link.send_message(Msg::RepeatSteps(key));
                    });
                    (held, nav, RepeatTimer::Delay(timer))
                });
                return self.navigate(ctx, nav);
            }
            Msg::RepeatSteps(key) => {
                if let Some((held, nav, timer)) = self.repeat.as_mut()
                    && *held == key
                {
                    let link = ctx.link().clone();
                    let nav = *nav;
                    *timer = RepeatTimer::Steps(Interval::new(REPEAT_MS, move || {
                        link.send_message(Msg::Repeat(nav))
                    }));
                    return self.navigate(ctx, nav);
                }
                return false;
            }
            Msg::Usage(u) => self.usage = u,
            Msg::Persisted(p) => self.persisted = p,
            Msg::Persist => {
                if let Some(answer) = storage::request_persist() {
                    ctx.link().send_future(async move {
                        Msg::Persisted(answer.await.ok().and_then(|v| v.as_bool()))
                    });
                }
                return false;
            }
            Msg::Style(id) => {
                let mut settings = Settings::load();
                settings.style = id;
                settings.save();
                self.settings = settings;
            }
            Msg::PadsTick => {
                let Some(g) = &self.gamepads else {
                    return false;
                };
                // Nothing here consumes the hub's edge queue.
                g.clear();
                let mut unbound: Vec<String> = Vec::new();
                let layout = self.shown_layout();
                let connected: Vec<ConnectedPad> = g
                    .pads()
                    .into_iter()
                    .map(|p| ConnectedPad {
                        id: p.id,
                        index: p.index,
                        standard: p.standard,
                    })
                    .collect();
                let plan = layout
                    .map(|l| self.settings.controls.plan(&l, &connected))
                    .unwrap_or_default();
                for (p, pad_use) in plan {
                    // Controllers beyond a two-pad layout's pads have no use
                    // but are not unbound.
                    let spare = pad_use == PadUse::Unused
                        && self.settings.pad_bindings(&p.id, p.standard).is_some();
                    if !pad_use.plays() && !spare && !unbound.contains(&p.id) {
                        unbound.push(p.id.clone());
                    }
                }
                if unbound == self.unbound_pads {
                    return false;
                }
                self.unbound_pads = unbound;
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let pads = html! {
            { for self.unbound_pads.iter().map(|id| html! {
                <p class="pad-notice">
                    { format!("Controller “{id}” has no bindings for {} yet — ", self.shown_layout().map_or_else(|| self.settings.style.clone(), |l| l.name)) }
                    <a href={Route::Options.to_hash()}>{ "bind it in Options" }</a>
                </p>
            }) }
        };
        let library = match &self.state {
            State::Loading => {
                return html! { <>{ pads }<p class="muted">{ "loading songs…" }</p></> };
            }
            State::Failed(e) => {
                return html! { <>{ pads }<p class="error">{ format!("could not load the song list: {e}") }</p></> };
            }
            State::Ready(l) => l,
        };
        let styles = library_styles(library);
        let style = self.shown_style();
        let has_style = |e: &ManifestEntry| e.charts.iter().any(|c| c.layout == style);
        let card = |entry: &ManifestEntry,
                    removal: Option<Removal>,
                    key: &str,
                    access: Option<JsValue>|
         -> Html {
            let playing = self.preview.as_ref().is_some_and(|p| p.id == entry.id);
            let banner = match &entry.banner {
                Some(_) if removal.is_some() => self.banners.get(&entry.id).cloned(),
                Some(b) => Some(crate::songs::asset_url(&format!("songs/{}/{b}", entry.id))),
                None => None,
            };
            let remove = removal.map(|r| self.remove_button(link, r, "remove"));
            song_card(
                entry,
                &style,
                CardInfo {
                    banner,
                    playing,
                    song_offset: self.settings.song_offset(&entry.id),
                    on_preview: self.preview_callback(link, entry, playing, access.clone()),
                    access,
                    remove,
                    pack_key: key.to_string(),
                },
            )
        };
        // The bundled songs, then each imported pack (in library order,
        // packs told apart ignoring case).
        let mut packs: Vec<(&str, Vec<&crate::songs::ImportedSong>)> = Vec::new();
        for s in library.imported.iter().filter(|s| has_style(&s.entry)) {
            match packs
                .iter_mut()
                .find(|(p, _)| p.to_lowercase() == s.pack.to_lowercase())
            {
                Some((_, songs)) => songs.push(s),
                None => packs.push((&s.pack, vec![s])),
            }
        }
        let bundled: Vec<&ManifestEntry> =
            library.bundled.iter().filter(|e| has_style(e)).collect();
        let mut groups: Vec<Html> = Vec::new();
        if !bundled.is_empty() {
            let key = menu::pack_key(None);
            let songs = bundled
                .iter()
                .map(|e| card(e, None, &key, None))
                .collect::<Vec<_>>();
            groups.push(self.pack_view(
                link,
                PackGroup {
                    key,
                    name: "Bundled songs".into(),
                    art: None,
                    songs,
                    remove: None,
                    folder: None,
                    linked: false,
                },
            ));
        }
        for (pack, songs) in &packs {
            let key = menu::pack_key(Some(pack));
            let art = self.pack_art.get(&pack.to_lowercase()).cloned();
            let cards = songs
                .iter()
                .map(|s| {
                    card(
                        &s.entry,
                        Some(Removal::Song(s.entry.id.clone())),
                        &key,
                        self.folder_of(s),
                    )
                })
                .collect::<Vec<_>>();
            let remove = self.remove_button(link, Removal::Pack(pack.to_string()), "remove pack");
            let folder = songs.iter().find_map(|s| s.folder.clone());
            let linked = songs.iter().any(|s| !s.links.is_empty());
            groups.push(self.pack_view(
                link,
                PackGroup {
                    key,
                    name: pack.to_string(),
                    art,
                    songs: cards,
                    remove: Some(remove),
                    folder,
                    linked,
                },
            ));
        }
        let mods = mods_summary(
            &self.settings.transform,
            self.settings.appearance,
            self.settings.scroll_action,
            None,
        );
        html! {
            <>
                { pads }
                { self.import_panel(link) }
                { if let Some(e) = &library.imported_error {
                    html!{ <p class="error">{ format!("imported songs are unavailable: {e}") }</p> }
                } else { html!{} } }
                { if library.unreadable.is_empty() { html!{} } else { html! {
                    <p class="error">
                        { format!("{} stored song{} could not be read. ", library.unreadable.len(), if library.unreadable.len() == 1 { "" } else { "s" }) }
                        { self.remove_button(link, Removal::Unreadable, "remove them") }
                    </p>
                } } }
                { if mods.is_empty() { html!{} } else { html! {
                    <p class="muted play-mods">
                        { "Playing with " }
                        <a href={Route::Options.to_hash()}>{ mods.join(" · ") }</a>
                    </p>
                } } }
                { style_select(link, &styles, &style) }
                <div class="packs">{ for groups }</div>
                { if let Some(u) = self.usage {
                    html!{
                        <p class="muted footer-links">
                            { format!("This site stores about {} in this browser. ", format_bytes(u)) }
                            { match self.persisted {
                                Some(true) => html!{ "The browser keeps your songs until you remove them." },
                                Some(false) => html!{ <>
                                    { "The browser may clear imported songs when space runs low. " }
                                    <button class="link-button" onclick={link.callback(|_| Msg::Persist)}>{ "keep them" }</button>
                                </> },
                                None => html!{},
                            } }
                        </p>
                    }
                } else { html!{} } }
            </>
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, _first_render: bool) {
        if !matches!(self.state, State::Ready(_)) {
            return;
        }
        // Back from a play: the chart that was started, or the scroll.
        if let Some(place) = self.place.take() {
            match place
                .chart
                .as_ref()
                .and_then(|(song, chart)| menu::chart_button(song, *chart))
            {
                Some(el) => menu::focus(&el, ScrollLogicalPosition::Center),
                None if place.scroll > 0.0 => {
                    if let Some(w) = web_sys::window() {
                        w.scroll_to_with_x_and_y(0.0, place.scroll);
                    }
                }
                None => {}
            }
        }
        if let Some(key) = self.scroll_to.take()
            && let Some(header) = menu::pack_header(&key)
        {
            let opts = web_sys::ScrollIntoViewOptions::new();
            opts.set_block(ScrollLogicalPosition::Start);
            header.scroll_into_view_with_scroll_into_view_options(&opts);
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        import::watch(None);
        if let Some(g) = &self.gamepads {
            g.set_listener(None);
        }
        self.stop_preview();
        self.revoke_banners();
        self.revoke_pack_art();
    }
}

impl SongSelect {
    /// The style the list shows: the saved one when the library has charts
    /// of it, else the first the library has.
    fn shown_style(&self) -> String {
        self.shown_layout()
            .map_or_else(|| self.settings.style.clone(), |l| l.id)
    }

    /// The layout the list shows (see [`SongSelect::shown_style`]).
    fn shown_layout(&self) -> Option<Layout> {
        let styles = match &self.state {
            State::Ready(l) => library_styles(l),
            _ => Vec::new(),
        };
        styles
            .iter()
            .find(|s| s.id == self.settings.style)
            .or(styles.first())
            .cloned()
            .or_else(|| Layout::builtin(&self.settings.style))
    }

    /// The ▶/■ button's handler; see [`Preview::start`] for why it starts
    /// in the click handler itself.
    fn preview_callback(
        &self,
        link: &html::Scope<Self>,
        entry: &ManifestEntry,
        playing: bool,
        access: Option<JsValue>,
    ) -> Callback<MouseEvent> {
        let id = entry.id.clone();
        if playing {
            return link.callback(move |_| Msg::PreviewEnded(id.clone()));
        }
        let entry = entry.clone();
        let link = link.clone();
        Callback::from(move |_: MouseEvent| {
            // A linked song's folder may need the permission asked again;
            // only the click itself can ask.
            if let Some(h) = &access {
                folders::ask(h);
            }
            match Preview::start(&entry) {
                Ok(preview) => link.send_message(Msg::PreviewStarted(preview)),
                Err(e) => web_sys::console::warn_1(&format!("preview: {e}").into()),
            }
        })
    }

    /// The kept folder a linked song plays from, to ask for access from a
    /// click.
    fn folder_of(&self, song: &crate::songs::ImportedSong) -> Option<JsValue> {
        if song.links.is_empty() {
            return None;
        }
        self.folders.get(song.folder.as_ref()?).cloned()
    }

    /// Stops any preview; returns the id that was playing.
    fn stop_preview(&mut self) -> Option<String> {
        // Dropping it closes its audio context (see [`Preview::load`]).
        self.preview.take().map(|p| p.id)
    }

    fn revoke_pack_art(&mut self) {
        revoke_later(self.pack_art.drain().map(|(_, u)| u).collect());
    }

    /// One move through the list; closes the pack whose header asks it.
    fn navigate(&mut self, _ctx: &Context<Self>, nav: Nav) -> bool {
        match menu::navigate(nav) {
            menu::Moved::Close(key) => self.close(&key),
            _ => false,
        }
    }

    /// Closes the pack `key` if it is the open one.
    fn close(&mut self, key: &str) -> bool {
        if self.open.as_deref() != Some(key) {
            return false;
        }
        self.open = None;
        let mut place = MenuPlace::load();
        place.known = true;
        place.open = None;
        place.save();
        true
    }

    /// One pack of the accordion: its header (banner, name, song count)
    /// and, when open, its songs.
    fn pack_view(&self, link: &html::Scope<Self>, group: PackGroup) -> Html {
        let PackGroup {
            key,
            name,
            art,
            songs,
            remove,
            folder,
            linked,
        } = group;
        let key = key.as_str();
        let name = name.as_str();
        let open = self.open.as_deref() == Some(key);
        let copy = !linked;
        let reread = folder.and_then(|f| {
            let handle = self.folders.get(&f)?.clone();
            let link = link.clone();
            // In the click itself: reading a kept folder again may ask.
            let onclick = Callback::from(move |_: MouseEvent| {
                let access = folders::access(&handle);
                let handle = handle.clone();
                let folder = f.clone();
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
                            link: !copy,
                        },
                        files,
                    )
                });
            });
            Some(html! {
                <button class="link-button" onclick={onclick} disabled={import::status().0.is_some()}
                    title="Import the folder again: changed songs are updated, new ones added, removed ones dropped">
                    { "re-read folder" }
                </button>
            })
        });
        let actions = remove.is_some() || reread.is_some();
        let count = format!(
            "{} song{}",
            songs.len(),
            if songs.len() == 1 { "" } else { "s" }
        );
        let toggle = {
            let key = key.to_string();
            link.callback(move |_| Msg::Toggle(key.clone()))
        };
        let art = match art {
            Some(src) => html! { <img src={src} alt="" /> },
            None => html! { <span class="pack-art-name">{ name }</span> },
        };
        html! {
            <section class={classes!("pack", open.then_some("open"))} key={key.to_string()}>
                <button class="pack-header" onclick={toggle} aria-expanded={open.to_string()}
                    data-nav-row="" data-nav-item="" data-pack-header={key.to_string()}>
                    <span class="pack-art">{ art }</span>
                    <span class="pack-info">
                        <span class="pack-name">{ name }</span>
                        <span class="muted">{ count }</span>
                    </span>
                    <span class="pack-chevron" aria-hidden="true">{ if open { "▾" } else { "▸" } }</span>
                </button>
                { if open { html! {
                    <div class="pack-body">
                        { if actions { html! {
                            <p class="pack-actions">{ for reread }{ " " }{ for remove }</p>
                        } } else { html! {} } }
                        <ul class="song-list">{ for songs }</ul>
                    </div>
                } } else { html! {} } }
            </section>
        }
    }

    fn revoke_banners(&mut self) {
        revoke_later(self.banners.drain().map(|(_, u)| u).collect());
    }

    fn remove_button(&self, link: &html::Scope<Self>, r: Removal, label: &str) -> Html {
        // Not while importing: removing runs the clean-up of shared records
        // the import may be about to refer to.
        let busy = import::status().0.is_some();
        if self.confirm.as_ref() == Some(&r) {
            html! {
                <button class="link-button error" disabled={busy} onclick={link.callback(move |_| Msg::Remove(r.clone()))}>
                    { "click again to remove" }
                </button>
            }
        } else {
            html! {
                <button class="link-button" disabled={busy} onclick={link.callback(move |_| Msg::AskRemove(r.clone()))}>
                    { label }
                </button>
            }
        }
    }

    fn import_panel(&self, link: &html::Scope<Self>) -> Html {
        let (running, report, _) = import::status();
        let busy = running.is_some();
        let on_pick = |input: NodeRef| {
            link.callback(move |_: Event| {
                // In the gesture: Firefox asks before it keeps the library.
                let _ = storage::request_persist();
                let Some(el) = input.cast::<HtmlInputElement>() else {
                    return Msg::Picked(Ok(Vec::new()));
                };
                let files = files::from_input(&el);
                // Clear so picking the same thing again fires `change`.
                el.set_value("");
                Msg::Picked(Ok(files))
            })
        };
        let class = classes!("import-panel", self.dragging.then_some("dragging"));
        let status = match (&running, &report) {
            (Some(text), _) => html! { <p class="import-status">{ text }</p> },
            (None, Some(r)) => report_view(r, self.settings.import_details),
            (None, None) => html! {},
        };
        html! {
            <section class={class}>
                <span>{ "Add your own songs (StepMania .sm/.ssc or .dwi packs, Dancing☆Onigiri works):" }</span>
                { if folders::supported() { html! {
                    // A kept handle lets the pack be re-read in place later.
                    <button class="button" disabled={busy} onclick={pick_folder(link, self.settings.import_copy)}>{ "choose folder" }</button>
                } } else { html! {
                    <label class={classes!("button", busy.then_some("disabled"))}>
                        { "choose folder" }
                        <input type="file" class="sr-only" ref={self.folder_input.clone()} webkitdirectory=true multiple=true disabled={busy}
                            onchange={on_pick(self.folder_input.clone())} />
                    </label>
                } } }
                <label class={classes!("button", busy.then_some("disabled"))}>
                    { "choose zip or files" }
                    <input type="file" class="sr-only" ref={self.file_input.clone()} multiple=true disabled={busy}
                        accept=".zip,.sm,.ssc,.dwi,.ogg,.oga,.opus,.mp3,.wav,.flac,.png,.jpg,.jpeg,.gif,.bmp,.webp,.avi,.f4v,.flv,.mkv,.mp4,.mpeg,.mpg,.mov,.ogv,.webm,.wmv"
                        onchange={on_pick(self.file_input.clone())} />
                </label>
                <span class="muted">{ if folders::supported() && !self.settings.import_copy {
                    "or drop them anywhere on this page. A chosen folder's music and backgrounds are read from the folder, the rest is stored in this browser only."
                } else {
                    "or drop them anywhere on this page. They stay in this browser only."
                } }</span>
                { if folders::supported() { html! {
                    <label class="import-details muted" title="Copied songs keep playing if the folder is moved or deleted, but take the space of their music in the browser">
                        <input type="checkbox" checked={self.settings.import_copy}
                            onchange={link.callback(|e: Event| Msg::ImportCopy(e.target_dyn_into::<HtmlInputElement>().is_some_and(|i| i.checked())))} />
                        { " store a chosen folder's songs in the browser (instead of reading them from the folder)" }
                    </label>
                } } else { html! {} } }
                <label class="import-details muted" title="Background videos are most of a pack's size; songs read from a chosen folder play them from the folder either way">
                    <input type="checkbox" checked={self.settings.import_videos}
                        onchange={link.callback(|e: Event| Msg::ImportVideos(e.target_dyn_into::<HtmlInputElement>().is_some_and(|i| i.checked())))} />
                    { " also store background videos when songs are stored in the browser" }
                </label>
                <label class="import-details muted">
                    <input type="checkbox" checked={self.settings.import_details}
                        onchange={link.callback(|e: Event| Msg::ImportDetails(e.target_dyn_into::<HtmlInputElement>().is_some_and(|i| i.checked())))} />
                    { " details for pack authors" }
                </label>
                { status }
            </section>
        }
    }
}

fn report_view(r: &ImportReport, details: bool) -> Html {
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut parts = Vec::new();
    if !r.imported.is_empty() {
        parts.push(format!(
            "Imported {}",
            plural(r.imported.len(), "song", "songs")
        ));
    }
    if r.shared_images > 0 {
        parts.push(format!(
            "stored {}",
            plural(
                r.shared_images,
                "shared background image",
                "shared background images"
            )
        ));
    }
    if r.shared_videos > 0 {
        parts.push(format!(
            "kept {}",
            plural(
                r.shared_videos,
                "shared background video",
                "shared background videos"
            )
        ));
    }
    if r.shared_skipped > 0 {
        parts.push(format!(
            "left out {} from shared folders",
            plural(r.shared_skipped, "other file", "other files")
        ));
    }
    if r.videos > 0 {
        parts.push(format!(
            "{} will play",
            plural(r.videos, "background video", "background videos")
        ));
    }
    if r.videos_left_out > 0 {
        parts.push(format!(
            "left out {} (\"also store background videos\" is off)",
            plural(r.videos_left_out, "background video", "background videos")
        ));
    }
    if r.removed > 0 {
        parts.push(format!(
            "removed {} no longer in the folder",
            plural(r.removed, "song", "songs")
        ));
    }
    if !r.warnings.is_empty() {
        parts.push(plural(r.warnings.len(), "warning", "warnings"));
    }
    if !r.skipped.is_empty() {
        parts.push(format!("skipped {}", r.skipped.len()));
    }
    if details && !r.notes.is_empty() {
        parts.push(plural(r.notes.len(), "detail", "details"));
    }
    let summary = if parts.is_empty() {
        r.error
            .clone()
            .unwrap_or_else(|| "No simfiles found in what was picked.".into())
    } else {
        let mut text = parts.join("; ");
        if let Some(first) = text.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        text + "."
    };
    let class = if r.imported.is_empty()
        && r.shared_images == 0
        && r.shared_videos == 0
        && r.shared_skipped == 0
        && r.videos_left_out == 0
        && r.removed == 0
    {
        "import-status error"
    } else {
        "import-status"
    };
    let list = |title: &str, class: &'static str, items: &[(String, String)]| {
        if items.is_empty() {
            return html! {};
        }
        html! {
            <>
                <p class="import-report-title">{ title.to_string() }</p>
                <ul class={classes!("import-report", class)}>
                    { for items.iter().map(|(what, why)| html!{ <li>{ format!("{what}: {why}") }</li> }) }
                </ul>
            </>
        }
    };
    html! {
        <>
            <p class={class}>{ summary }</p>
            { list("Not imported", "import-errors", &r.skipped) }
            { list("Imported with problems", "import-warnings muted", &r.warnings) }
            { if details { list("Details for pack authors (no difference in play)", "import-notes muted", &r.notes) } else { html!{} } }
        </>
    }
}

/// Window-level drag listeners: highlight the import panel while files are
/// dragged over the page and import them on drop.
fn drag_listeners(ctx: &Context<SongSelect>) -> Vec<EventListener> {
    let Some(window) = web_sys::window() else {
        return Vec::new();
    };
    let carries_files = |e: &DragEvent| {
        e.data_transfer()
            .is_some_and(|dt| dt.types().includes(&"Files".into(), 0))
    };
    let opts = EventListenerOptions::enable_prevent_default();
    let over = {
        let link = ctx.link().clone();
        EventListener::new_with_options(&window, "dragover", opts, move |e| {
            let Some(e) = e.dyn_ref::<DragEvent>() else {
                return;
            };
            if carries_files(e) {
                // Without this the browser opens the file instead of dropping.
                e.prevent_default();
                link.send_message(Msg::Dragging(true));
            }
        })
    };
    let leave = {
        let link = ctx.link().clone();
        EventListener::new(&window, "dragleave", move |e| {
            // Leaving the window has no related target.
            if let Some(e) = e.dyn_ref::<DragEvent>()
                && e.related_target().is_none()
            {
                link.send_message(Msg::Dragging(false));
            }
        })
    };
    let drop = {
        let link = ctx.link().clone();
        EventListener::new_with_options(&window, "drop", opts, move |e| {
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
        })
    };
    vec![over, leave, drop]
}

/// Revokes object URLs once nothing can still be loading them: a lazy
/// image may start loading an old URL just before the list redraws with
/// new ones.
fn revoke_later(urls: Vec<String>) {
    if urls.is_empty() {
        return;
    }
    Timeout::new(REVOKE_DELAY_MS, move || {
        urls.iter().for_each(|u| revoke_object_url(u));
    })
    .forget();
}

/// How long replaced object URLs stay valid, ms.
const REVOKE_DELAY_MS: u32 = 5000;

/// The Chromium folder button: the picker opens in the click itself, the
/// handle is kept for re-reading, and the folder's files are imported.
fn pick_folder(link: &html::Scope<SongSelect>, copy: bool) -> Callback<MouseEvent> {
    let link = link.clone();
    Callback::from(move |_: MouseEvent| {
        let _ = storage::request_persist();
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
    })
}

/// Arrow keys move through the list and Escape goes back, unless typing
/// in a field; Enter on a focused item works as it always does.
fn key_listener(ctx: &Context<SongSelect>) -> Option<EventListener> {
    let window = web_sys::window()?;
    let link = ctx.link().clone();
    let opts = EventListenerOptions::enable_prevent_default();
    Some(EventListener::new_with_options(
        &window,
        "keydown",
        opts,
        move |e| {
            let Some(e) = e.dyn_ref::<KeyboardEvent>() else {
                return;
            };
            if e.alt_key() || e.ctrl_key() || e.meta_key() || e.default_prevented() {
                return;
            }
            let typing = e
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .is_some_and(|t| matches!(t.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA"));
            if typing {
                return;
            }
            let nav = match e.key().as_str() {
                "ArrowUp" => Nav::Up,
                "ArrowDown" => Nav::Down,
                "ArrowLeft" => Nav::Left,
                "ArrowRight" => Nav::Right,
                "Escape" => Nav::Back,
                _ => return,
            };
            // Moved here, so the page scrolls as usual when nothing moves.
            match menu::navigate(nav) {
                menu::Moved::No => {}
                menu::Moved::Yes => e.prevent_default(),
                menu::Moved::Close(key) => {
                    e.prevent_default();
                    link.send_message(Msg::Close(key));
                }
            }
        },
    ))
}

/// The mouse takes over from the keys: drop the moved-to mark.
fn pointer_listener() -> Option<EventListener> {
    let window = web_sys::window()?;
    Some(EventListener::new(&window, "pointerdown", |_| {
        menu::clear_current()
    }))
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

fn format_bytes(b: f64) -> String {
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else {
        format!("{:.0} MB", (b / 1e6).max(1.0))
    }
}

/// Layouts with charts in the library: the built-in ones in their order,
/// then the ones songs define themselves, by name.
pub(crate) fn library_styles(library: &Library) -> Vec<Layout> {
    let entries = || {
        library
            .bundled
            .iter()
            .chain(library.imported.iter().map(|s| &s.entry))
    };
    let mut ids: Vec<&str> = entries()
        .flat_map(|e| e.charts.iter().map(|c| c.layout.as_str()))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let mut layouts: Vec<Layout> = ids
        .into_iter()
        .filter_map(|id| {
            Layout::builtin(id)
                .or_else(|| entries().find_map(|e| e.layouts.iter().find(|l| l.id == id).cloned()))
        })
        .collect();
    let order = Layout::builtin_ids();
    layouts.sort_by(|a, b| {
        let rank = |l: &Layout| order.iter().position(|b| *b == l.id).unwrap_or(order.len());
        rank(a)
            .cmp(&rank(b))
            .then(a.name.cmp(&b.name))
            .then(a.id.cmp(&b.id))
    });
    layouts
}

/// The style buttons, when the library has charts of more than one layout.
fn style_select(link: &html::Scope<SongSelect>, styles: &[Layout], current: &str) -> Html {
    if styles.len() < 2 {
        return html! {};
    }
    html! {
        <div class="style-select" role="group" aria-label="Style">
            { for styles.iter().map(|l| {
                let id = &l.id;
                let selected = id == current;
                let msg = id.clone();
                html! {
                    <button class={classes!("style-button", selected.then_some("selected"))}
                        aria-pressed={selected.to_string()}
                        onclick={link.callback(move |_| Msg::Style(msg.clone()))}>
                        { &l.name }
                    </button>
                }
            }) }
        </div>
    }
}

/// One pack of the accordion.
struct PackGroup {
    /// [`menu::pack_key`].
    key: String,
    name: String,
    /// Banner object URL.
    art: Option<String>,
    songs: Vec<Html>,
    /// The pack's remove button, for imported packs.
    remove: Option<Html>,
    /// The kept folder it came from, if any (storage key).
    folder: Option<String>,
    /// Whether its songs read their files from that folder (re-reading
    /// keeps how the pack was imported).
    linked: bool,
}

/// What a song card shows besides the entry.
struct CardInfo {
    banner: Option<String>,
    playing: bool,
    song_offset: f64,
    on_preview: Callback<MouseEvent>,
    remove: Option<Html>,
    /// The pack the card is in ([`menu::pack_key`]).
    pack_key: String,
    /// The kept folder a linked song plays from.
    access: Option<JsValue>,
}

fn song_card(entry: &ManifestEntry, style: &str, info: CardInfo) -> Html {
    let CardInfo {
        banner,
        playing,
        song_offset,
        on_preview,
        remove,
        pack_key,
        access,
    } = info;
    let banner =
        banner.map(|src| html! { <img class="song-banner" src={src} alt="" loading="lazy" /> });
    let charts: Vec<_> = entry.charts.iter().filter(|c| c.layout == style).collect();
    let mut sub = vec![entry.artist.clone()];
    if !entry.bpm.is_empty() {
        sub.push(format!("{} BPM", entry.bpm));
    }
    if !entry.credit.is_empty() {
        sub.push(entry.credit.clone());
    }
    if song_offset != 0.0 {
        sub.push(format!("offset {:+.0} ms", song_offset * 1000.0));
    }
    html! {
        <li class="song-card" data-nav-row="">
            { for banner }
            <div class="song-meta">
                <div class="song-title">
                    { &entry.title }
                    { " " }
                    <button class="preview-button" onclick={on_preview} title="preview">{ if playing { "■" } else { "▶" } }</button>
                </div>
                <div class="muted song-sub">{ sub.join(" · ") }{ " " }{ for remove }</div>
                <div class="chart-buttons">
                    { for charts.iter().map(|c| {
                        let href = Route::Play { song: entry.id.clone(), chart: c.index, force_gl: false, auto: false, auto_pad: false, bias_ms: 0, slow: false }.to_hash();
                        let class = format!("chart-button diff-{}", c.difficulty.to_lowercase());
                        let title = if c.credit.is_empty() {
                            format!("{} · {} notes", c.name, c.notes)
                        } else {
                            format!("{} · {} notes · steps by {}", c.name, c.notes, c.credit)
                        };
                        // Edits (and Dancing☆Onigiri charts, which have no
                        // difficulty slots) go by their name.
                        let named = c.difficulty == "Edit" && !c.name.trim().is_empty();
                        let label = if named { c.name.trim() } else { c.difficulty.as_str() };
                        let show_meter = !named || c.meter > 0;
                        // Remember the place, for coming back from the play.
                        let remember = {
                            let place = MenuPlace {
                                known: true,
                                open: Some(pack_key.clone()),
                                chart: Some((entry.id.clone(), c.index)),
                                scroll: 0.0,
                            };
                            let access = access.clone();
                            Callback::from(move |_: MouseEvent| {
                                // Ask now for a linked song's folder: the
                                // play page cannot without a click.
                                if let Some(h) = &access {
                                    folders::ask(h);
                                }
                                let mut place = place.clone();
                                place.scroll = web_sys::window().and_then(|w| w.scroll_y().ok()).unwrap_or(0.0);
                                place.save();
                            })
                        };
                        html! {
                            <a class={class} href={href} title={title} onclick={remember}
                                data-nav-item="" data-song={entry.id.clone()} data-chart={c.index.to_string()}>
                                <span class="chart-diff">{ label }</span>
                                { if show_meter { html! { <span class="chart-meter">{ c.meter }</span> } } else { html! {} } }
                            </a>
                        }
                    }) }
                </div>
            </div>
        </li>
    }
}
