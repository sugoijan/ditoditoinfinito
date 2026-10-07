//! Song list with difficulty picker and audio preview, grouped into the
//! bundled songs and one group per imported pack, plus the import panel
//! (folder or zip pickers and drag and drop anywhere on the page).

use std::collections::HashMap;

use gloo::events::{EventListener, EventListenerOptions};
use wasm_bindgen::JsCast;
use web_sys::{DragEvent, HtmlAudioElement, HtmlInputElement};
use yew::prelude::*;

use crate::import::{self, ImportReport};
use crate::router::Route;
use crate::settings::Settings;
use crate::songs::{
    Library, ManifestEntry, Origin, delete_imported, imported_banner_urls, media_url,
};
use crate::web::files::{self, PickedFile, revoke_object_url};

pub(crate) enum Msg {
    Loaded(Result<Library, String>),
    /// Object URLs for imported banners, by song id, for the library load
    /// numbered by the first field.
    Banners(u64, Vec<(String, String)>),
    /// Toggle the preview of a song (by id).
    Preview(String),
    PreviewReady {
        id: String,
        url: String,
        origin: Origin,
    },
    PreviewEnded,
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
    _drag: Vec<EventListener>,
}

struct Preview {
    id: String,
    audio: HtmlAudioElement,
    /// Object URL to revoke when the preview stops (imported songs).
    object_url: Option<String>,
    /// `ended` and `error` (Safari cannot play Ogg previews).
    _listeners: [EventListener; 2],
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
            _drag: drag_listeners(ctx),
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
            Msg::Banners(load, urls) => {
                if load != self.banner_load {
                    urls.iter().for_each(|(_, u)| revoke_object_url(u));
                    return false;
                }
                self.revoke_banners();
                self.banners = urls.into_iter().collect();
            }
            Msg::Preview(id) => {
                let was_playing = self.stop_preview().as_deref() == Some(id.as_str());
                if was_playing {
                    return true;
                }
                let State::Ready(library) = &self.state else {
                    return false;
                };
                let Some((entry, origin)) = library.find(&id) else {
                    return false;
                };
                let entry = entry.clone();
                ctx.link().send_future(async move {
                    match media_url(&entry, origin, &entry.music).await {
                        Ok(url) => Msg::PreviewReady {
                            id: entry.id.clone(),
                            url,
                            origin,
                        },
                        Err(_) => Msg::PreviewEnded,
                    }
                });
            }
            Msg::PreviewReady { id, url, origin } => {
                let object_url = (origin == Origin::Imported).then(|| url.clone());
                let State::Ready(library) = &self.state else {
                    object_url.iter().for_each(|u| revoke_object_url(u));
                    return false;
                };
                let start = library.find(&id).map(|(e, _)| e.preview_start);
                let (Some(start), Ok(audio)) = (start, HtmlAudioElement::new_with_src(&url)) else {
                    object_url.iter().for_each(|u| revoke_object_url(u));
                    return false;
                };
                self.stop_preview();
                // The <audio> element is fine for previews: ±50 ms is harmless
                // here, and it streams instead of decoding the whole file.
                audio.set_current_time(start);
                audio.set_volume(0.6);
                let _ = audio.play();
                let stop = |kind: &'static str| {
                    let link = ctx.link().clone();
                    EventListener::new(&audio, kind, move |_| link.send_message(Msg::PreviewEnded))
                };
                let listeners = [stop("ended"), stop("error")];
                self.preview = Some(Preview {
                    id,
                    audio,
                    object_url,
                    _listeners: listeners,
                });
            }
            Msg::PreviewEnded => {
                self.stop_preview();
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
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let link = ctx.link();
        let library = match &self.state {
            State::Loading => return html! { <p class="muted">{ "loading songs…" }</p> },
            State::Failed(e) => {
                return html! { <p class="error">{ format!("could not load the song list: {e}") }</p> };
            }
            State::Ready(l) => l,
        };
        let card = |entry: &ManifestEntry, removal: Option<Removal>| -> Html {
            let id = entry.id.clone();
            let banner = match &entry.banner {
                Some(_) if removal.is_some() => self.banners.get(&entry.id).cloned(),
                Some(b) => Some(crate::songs::asset_url(&format!("songs/{}/{b}", entry.id))),
                None => None,
            };
            let remove = removal.map(|r| self.remove_button(link, r, "remove"));
            song_card(
                entry,
                banner,
                self.preview.as_ref().is_some_and(|p| p.id == entry.id),
                self.settings.song_offset(&entry.id),
                link.callback(move |_| Msg::Preview(id.clone())),
                remove,
            )
        };
        let mut packs: Vec<(&str, Vec<&crate::songs::ImportedSong>)> = Vec::new();
        for s in &library.imported {
            match packs.last_mut() {
                Some((p, songs)) if p.to_lowercase() == s.pack.to_lowercase() => songs.push(s),
                _ => packs.push((&s.pack, vec![s])),
            }
        }
        let grouped = !packs.is_empty();
        html! {
            <>
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
                { if grouped { html!{ <h2 class="pack-heading">{ "Bundled songs" }</h2> } } else { html!{} } }
                <ul class="song-list">
                    { for library.bundled.iter().map(|e| card(e, None)) }
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
    /// Stops any preview; returns the id that was playing.
    fn stop_preview(&mut self) -> Option<String> {
        let p = self.preview.take()?;
        let _ = p.audio.pause();
        p.audio.set_src("");
        if let Some(url) = &p.object_url {
            revoke_object_url(url);
        }
        Some(p.id)
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
            (None, Some(r)) => report_view(r),
            (None, None) => html! {},
        };
        html! {
            <section class={class}>
                <span>{ "Add your own songs (StepMania .sm/.ssc or .dwi packs):" }</span>
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
                { status }
            </section>
        }
    }
}

fn report_view(r: &ImportReport) -> Html {
    let summary = match (r.imported.len(), r.skipped.len()) {
        (0, 0) => r
            .error
            .clone()
            .unwrap_or_else(|| "No simfiles found in what was picked.".into()),
        (n, 0) => format!("Imported {n} song{}.", if n == 1 { "" } else { "s" }),
        (n, k) => format!(
            "Imported {n} song{}; skipped {k}.",
            if n == 1 { "" } else { "s" }
        ),
    };
    let class = if r.imported.is_empty() {
        "import-status error"
    } else {
        "import-status"
    };
    html! {
        <>
            <p class={class}>{ summary }</p>
            { if r.skipped.is_empty() { html!{} } else { html! {
                <ul class="import-report muted">
                    { for r.skipped.iter().map(|(what, why)| html!{ <li>{ format!("{what}: {why}") }</li> }) }
                </ul>
            } } }
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

fn song_card(
    entry: &ManifestEntry,
    banner: Option<String>,
    playing: bool,
    song_offset: f64,
    on_preview: Callback<MouseEvent>,
    remove: Option<Html>,
) -> Html {
    let banner =
        banner.map(|src| html! { <img class="song-banner" src={src} alt="" loading="lazy" /> });
    let charts: Vec<_> = entry
        .charts
        .iter()
        .filter(|c| c.layout == "dance-single")
        .collect();
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
                        let href = Route::Play { song: entry.id.clone(), chart: c.index, force_gl: false, auto: false, bias_ms: 0 }.to_hash();
                        let class = format!("chart-button diff-{}", c.difficulty.to_lowercase());
                        let title = if c.credit.is_empty() {
                            format!("{} · {} notes", c.name, c.notes)
                        } else {
                            format!("{} · {} notes · steps by {}", c.name, c.notes, c.credit)
                        };
                        html! {
                            <a class={class} href={href} title={title}>
                                <span class="chart-diff">{ &c.difficulty }</span>
                                <span class="chart-meter">{ c.meter }</span>
                            </a>
                        }
                    }) }
                </div>
            </div>
        </li>
    }
}
