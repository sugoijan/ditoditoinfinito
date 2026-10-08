//! Song list with difficulty picker and audio preview, grouped into the
//! bundled songs and one group per imported pack, plus the import panel
//! (folder or zip pickers and drag and drop anywhere on the page).
//!
//! It also watches the connected controllers and points to the options
//! when one has no bindings: a controller without the standard mapping has
//! no defaults (its buttons are numbered arbitrarily), so it would do
//! nothing in a song.

use std::collections::HashMap;

use ddi_chart::Layout;
use ddi_engine::{ConnectedPad, PadUse};
use gloo::events::{EventListener, EventListenerOptions};
use gloo::timers::callback::Interval;
use wasm_bindgen::JsCast;
use web_sys::{AudioBuffer, DragEvent, HtmlInputElement};
use yew::prelude::*;

use crate::import::{self, ImportReport};
use crate::mods::mods_summary;
use crate::preview::Preview;
use crate::router::Route;
use crate::settings::Settings;
use crate::songs::{Library, ManifestEntry, delete_imported, imported_banner_urls};

use crate::web::files::{self, PickedFile, revoke_object_url};
use crate::web::gamepad::Gamepads;

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
    /// The "details for pack authors" box of the import panel.
    ImportDetails(bool),
    /// Timer: look for connected controllers without bindings.
    PadsTick,
    /// Show the charts of this layout.
    Style(String),
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
    folder_input: NodeRef,
    file_input: NodeRef,
    /// Polled once per frame only, to notice controllers.
    gamepads: Option<Gamepads>,
    /// Connected controllers with neither saved bindings nor the standard
    /// mapping.
    unbound_pads: Vec<String>,
    _drag: Vec<EventListener>,
    _pad_check: Option<Interval>,
}

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
            folder_input: NodeRef::default(),
            file_input: NodeRef::default(),
            gamepads,
            unbound_pads: Vec::new(),
            _drag: drag_listeners(ctx),
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
                if library.imported.is_empty() {
                    self.usage = None;
                } else {
                    ctx.link()
                        .send_future(async { Msg::Usage(storage_usage().await) });
                }
                self.state = State::Ready(library);
            }
            Msg::Loaded(Err(e)) => self.state = State::Failed(e),
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
            Msg::Usage(u) => self.usage = u,
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
        let card = |entry: &ManifestEntry, removal: Option<Removal>| -> Html {
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
                banner,
                playing,
                self.settings.song_offset(&entry.id),
                self.preview_callback(link, entry, playing),
                remove,
            )
        };
        let mut packs: Vec<(&str, Vec<&crate::songs::ImportedSong>)> = Vec::new();
        for s in library.imported.iter().filter(|s| has_style(&s.entry)) {
            match packs.last_mut() {
                Some((p, songs)) if p.to_lowercase() == s.pack.to_lowercase() => songs.push(s),
                _ => packs.push((&s.pack, vec![s])),
            }
        }
        let grouped = !packs.is_empty();
        let mut bundled = library.bundled.iter().filter(|e| has_style(e)).peekable();
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
                { if grouped && bundled.peek().is_some() { html!{ <h2 class="pack-heading">{ "Bundled songs" }</h2> } } else { html!{} } }
                <ul class="song-list">
                    { for bundled.map(|e| card(e, None)) }
                </ul>
                { for packs.into_iter().map(|(pack, songs)| html! {
                    <>
                        <h2 class="pack-heading">
                            { pack }
                            <span class="muted">{ format!("{} song{}", songs.len(), if songs.len() == 1 { "" } else { "s" }) }</span>
                            { self.remove_button(link, Removal::Pack(pack.to_string()), "remove pack") }
                        </h2>
                        <ul class="song-list">
                            { for songs.iter().map(|s| card(&s.entry, Some(Removal::Song(s.entry.id.clone())))) }
                        </ul>
                    </>
                }) }
                { if let Some(u) = self.usage {
                    html!{ <p class="muted footer-links">{ format!("This site stores about {} in this browser.", format_bytes(u)) }</p> }
                } else { html!{} } }
            </>
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        import::watch(None);
        self.stop_preview();
        self.revoke_banners();
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
    ) -> Callback<MouseEvent> {
        let id = entry.id.clone();
        if playing {
            return link.callback(move |_| Msg::PreviewEnded(id.clone()));
        }
        let entry = entry.clone();
        let link = link.clone();
        Callback::from(move |_: MouseEvent| match Preview::start(&entry) {
            Ok(preview) => link.send_message(Msg::PreviewStarted(preview)),
            Err(e) => web_sys::console::warn_1(&format!("preview: {e}").into()),
        })
    }

    /// Stops any preview; returns the id that was playing.
    fn stop_preview(&mut self) -> Option<String> {
        // Dropping it closes its audio context (see [`Preview::load`]).
        self.preview.take().map(|p| p.id)
    }

    fn revoke_banners(&mut self) {
        for url in self.banners.values() {
            revoke_object_url(url);
        }
        self.banners.clear();
    }

    fn remove_button(&self, link: &html::Scope<Self>, r: Removal, label: &str) -> Html {
        if self.confirm.as_ref() == Some(&r) {
            html! {
                <button class="link-button error" onclick={link.callback(move |_| Msg::Remove(r.clone()))}>
                    { "click again to remove" }
                </button>
            }
        } else {
            html! {
                <button class="link-button" onclick={link.callback(move |_| Msg::AskRemove(r.clone()))}>
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
                <label class={classes!("button", busy.then_some("disabled"))}>
                    { "choose folder" }
                    <input type="file" class="sr-only" ref={self.folder_input.clone()} webkitdirectory=true multiple=true disabled={busy}
                        onchange={on_pick(self.folder_input.clone())} />
                </label>
                <label class={classes!("button", busy.then_some("disabled"))}>
                    { "choose zip or files" }
                    <input type="file" class="sr-only" ref={self.file_input.clone()} multiple=true disabled={busy}
                        accept=".zip,.sm,.ssc,.dwi,.ogg,.oga,.opus,.mp3,.wav,.flac,.png,.jpg,.jpeg,.gif,.bmp,.webp"
                        onchange={on_pick(self.file_input.clone())} />
                </label>
                <span class="muted">{ "or drop them anywhere on this page. They stay in this browser only." }</span>
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
    if r.shared_skipped > 0 {
        parts.push(format!(
            "left out {} from shared folders (videos are not played)",
            plural(r.shared_skipped, "other file", "other files")
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
    let class = if r.imported.is_empty() && r.shared_images == 0 && r.shared_skipped == 0 {
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

fn song_card(
    entry: &ManifestEntry,
    style: &str,
    banner: Option<String>,
    playing: bool,
    song_offset: f64,
    on_preview: Callback<MouseEvent>,
    remove: Option<Html>,
) -> Html {
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
        <li class="song-card">
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
                        let href = Route::Play { song: entry.id.clone(), chart: c.index, force_gl: false, auto: false, auto_pad: false, bias_ms: 0 }.to_hash();
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
                        html! {
                            <a class={class} href={href} title={title}>
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
