//! Find songs on desktop (`docs/plans/source-plugins.md`, stage 2): the
//! page's model ([`ddi_ui::screens::find_songs`]) over the sources the
//! game holds ([`super::Sources`]). It asks the chosen source (a search
//! once the field rests, a page more, a fetch), takes the host's answers
//! each frame the page shows, and hands a fetched item on: its songs
//! listed alone by song select's machinery and played without importing
//! (`crate::songs::found_screen`), or written into the library
//! ([`super::import`], on a thread) and the library read again.
//!
//! A source that fails says so on the page, in the game's words (its own
//! as Details), and the game goes on.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::Receiver;

use ddi_source_host::host::{Event, Failure, HostFailure, Request, Stop};
use ddi_source_host::protocol::{FailCode, ItemMeta, OpId, Step as HostStep};
use ddi_source_host::store::{FetchedSong, ItemKey};
use ddi_ui::Cx;
use ddi_ui::screens::Env;
use ddi_ui::screens::find_songs::{
    Busy, FindModel, FindPress, FindView, Note, Problem, ResultView, SourceName, SourceView,
    Step,
};

use super::this_computer;
use crate::preview::SharedPlayer;
use crate::songs::Songs;

/// How long the search field rests before it is searched.
const REST_S: f64 = 0.35;

/// Results asked for a page.
const PAGE: u32 = 50;

/// What a fetch is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    Play,
    Import,
}

struct Fetching {
    op: OpId,
    source: String,
    item: ItemMeta,
    purpose: Purpose,
    step: Option<Step>,
    done: Option<u64>,
    total: Option<u64>,
}

struct Importing {
    title: String,
    source: String,
    item: String,
    key: ItemKey,
    done: Receiver<Result<usize, String>>,
}

/// The page's model.
pub struct Finder {
    env: Env,
    songs: Rc<Songs>,
    sound: Option<SharedPlayer>,
    version: u64,
    chosen: usize,
    query: String,
    /// When the field changed and was not searched yet.
    typed_at: Option<f64>,
    /// The field changed since its last search (searched at the next
    /// frame's rest).
    dirty: bool,
    searched: bool,
    search: Option<(OpId, String)>,
    results: Vec<ItemMeta>,
    cursor: Option<String>,
    total: Option<u64>,
    fetching: Option<Fetching>,
    importing: Option<Importing>,
    problem: Option<Problem>,
    details: Option<String>,
    note: Option<Note>,
    /// Sources off for this run, by id, and why.
    off: BTreeMap<String, Problem>,
    /// The library's songs' keys (`song_key`), for "In your library",
    /// and how many songs it had when they were worked out.
    library_keys: HashSet<String>,
    library_seen: usize,
    /// Items imported before, by source and item (the import's catalog).
    imported: HashSet<(String, String)>,
    /// A found item's songs to show over the page (the next frame).
    show: Option<(String, Vec<FetchedSong>)>,
}

impl Finder {
    pub fn new(env: Env, songs: Rc<Songs>, sound: Option<SharedPlayer>) -> Finder {
        // What was read of the folders before is read again.
        if let Some(t) = songs
            .sources
            .borrow()
            .as_ref()
            .and_then(|s| s.this_computer.clone())
        {
            t.refresh();
        }
        let mut f = Finder {
            env,
            songs,
            sound,
            version: 0,
            chosen: 0,
            query: String::new(),
            typed_at: None,
            dirty: false,
            searched: false,
            search: None,
            results: Vec::new(),
            cursor: None,
            total: None,
            fetching: None,
            importing: None,
            problem: None,
            details: None,
            note: None,
            off: BTreeMap::new(),
            library_keys: HashSet::new(),
            library_seen: usize::MAX,
            imported: super::import::catalog()
                .into_iter()
                .map(|r| (r.source, r.item))
                .collect(),
            show: None,
        };
        f.library_changed();
        // This computer with folders, or a plugin: everything listed at
        // once.
        if f.folders().is_none_or(|f| !f.is_empty()) {
            f.dirty = true;
        }
        f
    }

    fn changed(&mut self) {
        self.version += 1;
    }

    /// The sources offered now: `(id, its plugin's name)`.
    fn offered(&self) -> Vec<(String, Option<String>)> {
        self.songs
            .sources
            .borrow()
            .as_ref()
            .map(|s| s.offered())
            .unwrap_or_default()
    }

    fn chosen_id(&self) -> Option<String> {
        self.offered().get(self.chosen).map(|(id, _)| id.clone())
    }

    fn chosen_name(&self) -> String {
        match self.offered().get(self.chosen) {
            Some((_, Some(name))) => name.clone(),
            _ => ddi_l10n::msg::sources::this_computer().into(),
        }
    }

    /// This computer's folders, while it is the source chosen.
    fn folders(&self) -> Option<Vec<std::path::PathBuf>> {
        if self.chosen_id().as_deref() != Some(this_computer::ID) {
            return None;
        }
        let sources = self.songs.sources.borrow();
        sources
            .as_ref()?
            .this_computer
            .as_ref()
            .map(|t| t.folders())
    }

    /// "In your library" worked out again where the library changed (an
    /// import, a scan).
    fn library_changed(&mut self) {
        let n = self
            .songs
            .library
            .borrow()
            .as_ref()
            .map_or(0, |l| l.songs.len());
        if n != self.library_seen {
            self.library_seen = n;
            self.library_keys = self.library_keys();
            self.changed();
        }
    }

    /// Every library song's `song_key`.
    fn library_keys(&self) -> HashSet<String> {
        let library = self.songs.library.borrow();
        let Some(l) = library.as_ref() else {
            return HashSet::new();
        };
        l.songs
            .iter()
            .filter(|s| s.fetched.is_none())
            .map(|s| &s.entry)
            .chain(self.songs.bundled.iter())
            .filter_map(|e| {
                let digests: Option<Vec<String>> = e
                    .charts
                    .iter()
                    .filter(|c| c.derivation.is_none())
                    .map(|c| c.digest.map(|d| d.to_string()))
                    .collect();
                digests
                    .filter(|d| !d.is_empty())
                    .map(|d| ddi_library::share::song_key(d.iter().map(String::as_str)))
            })
            .collect()
    }

    fn request(&mut self, source: &str, request: Request) -> Option<OpId> {
        let mut sources = self.songs.sources.borrow_mut();
        Some(sources.as_mut()?.host.request(source, request))
    }

    fn cancel(&mut self, op: OpId) {
        if let Some(s) = self.songs.sources.borrow_mut().as_mut() {
            s.host.cancel(op);
        }
    }

    /// A search of the chosen source from its first page.
    fn search_now(&mut self) {
        self.typed_at = None;
        self.dirty = false;
        if let Some((op, _)) = self.search.take() {
            self.cancel(op);
        }
        let Some(source) = self.chosen_id() else {
            return;
        };
        self.results.clear();
        self.cursor = None;
        self.total = None;
        self.problem = None;
        self.details = None;
        if matches!(self.note, Some(Note::Partial)) {
            self.note = None;
        }
        self.searched = true;
        if self.folders().is_some_and(|f| f.is_empty()) {
            self.changed();
            return;
        }
        let op = self.request(
            &source,
            Request::Search {
                scope: None,
                query: self.query.trim().to_string(),
                filters: BTreeMap::new(),
                cursor: None,
                page_size: PAGE,
            },
        );
        self.search = op.map(|op| (op, source));
        self.changed();
    }

    /// The next page of the last search.
    fn more(&mut self) {
        let (Some(source), Some(cursor)) = (self.chosen_id(), self.cursor.take()) else {
            return;
        };
        if self.search.is_some() {
            return;
        }
        let op = self.request(
            &source,
            Request::Search {
                scope: None,
                query: self.query.trim().to_string(),
                filters: BTreeMap::new(),
                cursor: Some(cursor),
                page_size: PAGE,
            },
        );
        self.search = op.map(|op| (op, source));
        self.changed();
    }

    /// Result `i` fetched, for `purpose`.
    fn fetch(&mut self, i: usize, purpose: Purpose) {
        if self.fetching.is_some() || self.importing.is_some() {
            return;
        }
        let (Some(item), Some(source)) = (self.results.get(i).cloned(), self.chosen_id()) else {
            return;
        };
        self.problem = None;
        self.details = None;
        self.note = None;
        let op = self.request(
            &source,
            Request::Fetch {
                item: item.id.clone(),
                revision: item.revision.clone(),
                size: item.size,
                rooms: item.rooms,
            },
        );
        if let Some(op) = op {
            self.fetching = Some(Fetching {
                op,
                source,
                item,
                purpose,
                step: None,
                done: None,
                total: None,
            });
        }
        self.changed();
    }

    fn add_folder(&mut self) {
        let picked = crate::os::pick(true);
        if picked.is_empty() {
            return;
        }
        let mut setting = super::Setting::load();
        for p in picked {
            if !setting.folders.contains(&p) {
                setting.folders.push(p);
            }
        }
        setting.save();
        self.set_folders(setting.folders);
    }

    fn remove_folder(&mut self, i: usize) {
        let mut setting = super::Setting::load();
        if i < setting.folders.len() {
            setting.folders.remove(i);
            setting.save();
            self.set_folders(setting.folders);
        }
    }

    fn set_folders(&mut self, folders: Vec<std::path::PathBuf>) {
        if let Some(t) = self
            .songs
            .sources
            .borrow()
            .as_ref()
            .and_then(|s| s.this_computer.clone())
        {
            t.set_folders(folders);
        }
        self.search_now();
    }

    /// What a host failure is, in the page's words.
    fn problem_of(failure: &Failure) -> Option<Problem> {
        Some(match failure {
            Failure::Plugin { code, .. } => match code {
                FailCode::Cancelled => return None,
                FailCode::Busy => Problem::Busy,
                FailCode::InvalidInput | FailCode::UnsupportedInput => Problem::InvalidInput,
                FailCode::Unsupported => Problem::Unsupported,
                FailCode::NotFound | FailCode::Gone => Problem::NotFound,
                FailCode::RefusedBySource => Problem::Refused,
                FailCode::RateLimited => Problem::RateLimited,
                FailCode::Unreachable => Problem::Unreachable,
                FailCode::TooLarge => Problem::TooLarge,
                FailCode::UnreadableContent => Problem::Unreadable,
                FailCode::SourceChanged => Problem::Changed,
                FailCode::Internal => Problem::Failed,
            },
            Failure::Host(h) => match h {
                HostFailure::Cancelled => return None,
                HostFailure::NotInstalled | HostFailure::SwitchedOff => Problem::SwitchedOff,
                HostFailure::OffForTheRun => Problem::OffForTheRun,
                HostFailure::Unsupported => Problem::Unsupported,
                HostFailure::Busy => Problem::Busy,
                HostFailure::Start(_) => Problem::NotStarted,
                HostFailure::TimedOut => Problem::TimedOut,
                HostFailure::Crashed => Problem::Crashed,
                HostFailure::Misbehaved(_) => Problem::Misbehaved,
                HostFailure::Incompatible { plugin_newer: true } => Problem::NeedsNewerGame,
                HostFailure::Incompatible { plugin_newer: false } => Problem::TooOld,
                HostFailure::TooLarge => Problem::TooLarge,
                HostFailure::Unreadable(_) => Problem::Unreadable,
                HostFailure::Stopped => Problem::Stopped,
            },
        })
    }

    /// The words worth a Details fold: the source's own, the host's why,
    /// the diagnostics' tail (data).
    fn details_of(failure: &Failure, diagnostics: Option<&str>) -> Option<String> {
        let own = match failure {
            Failure::Plugin { detail, .. } => detail.clone(),
            Failure::Host(HostFailure::Start(why) | HostFailure::Misbehaved(why)) => {
                Some(why.clone())
            }
            Failure::Host(HostFailure::Unreadable(why)) => Some(
                why.iter()
                    .map(|(dir, f)| {
                        ddi_ui::screens::import_report::line(dir.as_str(), f)
                            .as_str()
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Failure::Host(_) => None,
        };
        let tail = diagnostics.map(|d| {
            let lines: Vec<&str> = d.lines().collect();
            lines[lines.len().saturating_sub(12)..].join("\n")
        });
        match (own, tail) {
            (Some(a), Some(b)) if !b.trim().is_empty() => Some(format!("{a}\n{b}")),
            (Some(a), _) => Some(a),
            (None, b) => b.filter(|b| !b.trim().is_empty()),
        }
    }

    /// The host's answers taken.
    fn take_events(&mut self) {
        loop {
            let event = {
                let mut sources = self.songs.sources.borrow_mut();
                let Some(s) = sources.as_mut() else {
                    return;
                };
                match s.host.try_event() {
                    Some(e) => e,
                    None => return,
                }
            };
            self.on_event(event);
        }
    }

    fn on_event(&mut self, event: Event) {
        let search = self.search.as_ref().map(|(op, _)| *op);
        let fetch = self.fetching.as_ref().map(|f| f.op);
        match event {
            Event::Results { op, items } if Some(op) == search => {
                self.results.extend(items);
                self.changed();
            }
            Event::Done { op, cursor, total } if Some(op) == search => {
                self.search = None;
                self.cursor = cursor;
                self.total = total;
                let cut = self
                    .songs
                    .sources
                    .borrow()
                    .as_ref()
                    .and_then(|s| s.this_computer.as_ref().map(|t| t.cut()))
                    .unwrap_or(false);
                if cut && self.chosen_id().as_deref() == Some(this_computer::ID) {
                    self.note = Some(Note::Partial);
                }
                self.changed();
            }
            Event::Failed {
                op,
                failure,
                diagnostics,
            } if Some(op) == search || Some(op) == fetch => {
                if Some(op) == search {
                    self.search = None;
                } else {
                    self.fetching = None;
                }
                self.problem = Self::problem_of(&failure);
                self.details = self
                    .problem
                    .and_then(|_| Self::details_of(&failure, diagnostics.as_deref()));
                if !matches!(failure, Failure::Host(HostFailure::Cancelled))
                    && !matches!(
                        failure,
                        Failure::Plugin {
                            code: FailCode::Cancelled,
                            ..
                        }
                    )
                {
                    eprintln!("sources: operation {op} failed: {failure:?}");
                }
                self.changed();
            }
            Event::Progress {
                op,
                step,
                done,
                total,
                ..
            } if Some(op) == fetch => {
                if let Some(f) = &mut self.fetching {
                    f.step = Some(match step {
                        HostStep::Resolving => Step::Resolving,
                        HostStep::Searching => Step::Searching,
                        HostStep::Downloading => Step::Downloading,
                        HostStep::Reading => Step::Reading,
                        HostStep::Converting => Step::Converting,
                        HostStep::Checking => Step::Checking,
                    });
                    f.done = done;
                    f.total = total;
                }
                self.changed();
            }
            Event::Fetched { op, key, songs, .. } if Some(op) == fetch => {
                let Some(f) = self.fetching.take() else {
                    return;
                };
                match f.purpose {
                    Purpose::Play => self.show = Some((self.chosen_name(), songs)),
                    Purpose::Import => self.import(f, key, songs),
                }
                self.changed();
            }
            Event::Stopped {
                plugin,
                why,
                off,
                diagnostics,
            } => {
                if off {
                    let p = match why {
                        Stop::Incompatible { plugin_newer: true } => Problem::NeedsNewerGame,
                        Stop::Incompatible { plugin_newer: false } => Problem::TooOld,
                        Stop::InvalidHello(_) => Problem::Misbehaved,
                        _ => Problem::OffForTheRun,
                    };
                    self.off.insert(plugin.clone(), p);
                }
                if why.strikes() {
                    eprintln!(
                        "sources: {plugin} stopped: {why:?}{}",
                        diagnostics
                            .as_deref()
                            .map(|d| format!("\n{d}"))
                            .unwrap_or_default()
                    );
                }
                self.changed();
            }
            _ => {}
        }
    }

    /// An item fetched for Import: written into the library on a thread.
    fn import(&mut self, f: Fetching, key: ItemKey, songs: Vec<FetchedSong>) {
        let Some(files) = super::fetched::files(key.0) else {
            self.problem = Some(Problem::ImportFailed);
            return;
        };
        let title = f.item.title.clone();
        let (source, item, revision) = (f.source, f.item.id, f.item.revision);
        let wake = self.songs.wake.borrow().clone();
        let (s, i) = (source.clone(), item.clone());
        let done = crate::os::background("ddi-source-import", wake, move || {
            super::import::import(&s, &i, revision.as_deref(), &songs, &files)
        });
        self.importing = Some(Importing {
            title,
            source,
            item,
            key,
            done,
        });
    }

    fn poll_import(&mut self) {
        let Some(got) = self.importing.as_ref().and_then(|i| i.done.try_recv().ok()) else {
            return;
        };
        let Some(importing) = self.importing.take() else {
            return;
        };
        if got.is_ok() {
            self.imported
                .insert((importing.source.clone(), importing.item.clone()));
        }
        match got {
            Ok(n) => {
                self.note = Some(Note::Imported { songs: n });
                // The library reads its import store again; the fetched
                // copy is let go.
                self.songs.rescan(Vec::new());
                if let Some(store) = super::fetched::store()
                    && let Ok(mut s) = store.lock()
                {
                    s.remove(importing.key);
                }
            }
            Err(e) => {
                eprintln!("sources: importing “{}”: {e}", importing.title);
                self.problem = Some(Problem::ImportFailed);
                self.details = Some(e);
            }
        }
        self.changed();
    }
}

impl FindModel for Finder {
    fn version(&self) -> u64 {
        self.version
    }

    fn view(&self) -> FindView {
        let offered = self.offered();
        let sources = offered
            .iter()
            .map(|(id, name)| SourceView {
                id: id.clone(),
                name: match name {
                    Some(n) => SourceName::Plugin(n.clone()),
                    None => SourceName::ThisComputer,
                },
                problem: self.off.get(id).copied(),
            })
            .collect();
        let source = self.chosen_id().unwrap_or_default();
        let results = self
            .results
            .iter()
            .map(|r| ResultView {
                id: r.id.clone(),
                title: r.title.clone(),
                tags: tags(r),
                // The same song by its charts, or this item imported.
                in_library: r
                    .song_key
                    .as_ref()
                    .is_some_and(|k| self.library_keys.contains(k))
                    || self.imported.contains(&(source.clone(), r.id.clone())),
            })
            .collect();
        let busy = if let Some(i) = &self.importing {
            Some(Busy::Importing {
                title: i.title.clone(),
            })
        } else if let Some(f) = &self.fetching {
            Some(Busy::Fetching {
                title: f.item.title.clone(),
                step: f.step,
                done: f.done,
                total: f.total,
            })
        } else {
            self.search.as_ref().map(|_| Busy::Searching)
        };
        FindView {
            sources,
            chosen: self.chosen.min(offered.len().saturating_sub(1)),
            folders: self
                .folders()
                .map(|f| f.iter().map(|p| p.display().to_string()).collect()),
            query: self.query.clone(),
            searched: self.searched,
            results,
            total: self.total,
            more: self.cursor.is_some(),
            busy,
            problem: self.problem,
            details: self.details.clone(),
            note: self.note.clone(),
        }
    }

    fn press(&mut self, press: FindPress, _cx: &mut Cx) {
        match press {
            FindPress::Source(i) => {
                if i != self.chosen && i < self.offered().len() {
                    self.chosen = i;
                    self.note = None;
                    self.search_now();
                }
            }
            FindPress::AddFolder => self.add_folder(),
            FindPress::RemoveFolder(i) => self.remove_folder(i),
            FindPress::Search => self.search_now(),
            FindPress::More => self.more(),
            FindPress::Play(i) => self.fetch(i, Purpose::Play),
            FindPress::Import(i) => self.fetch(i, Purpose::Import),
            FindPress::Cancel => {
                if let Some((op, _)) = self.search.take() {
                    self.cancel(op);
                }
                if let Some(f) = self.fetching.take() {
                    self.cancel(f.op);
                }
            }
        }
        self.changed();
    }

    fn typed(&mut self, query: String) {
        if query != self.query {
            self.query = query;
            self.dirty = true;
            self.typed_at = None;
        }
    }

    fn tick(&mut self, cx: &mut Cx) {
        self.take_events();
        self.poll_import();
        self.library_changed();
        // The field searched once it rests.
        if self.dirty {
            let now = cx.time();
            match self.typed_at {
                None => self.typed_at = Some(now),
                Some(at) if now - at >= REST_S => self.search_now(),
                Some(_) => {}
            }
        }
        if let Some((source, songs)) = self.show.take() {
            let ids: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
            let screen = crate::songs::found_screen(
                &self.env,
                &self.songs,
                self.sound.clone(),
                source,
                ids,
            );
            cx.push(screen);
        }
    }

    fn leave(&mut self, _cx: &mut Cx) {
        if let Some((op, _)) = self.search.take() {
            self.cancel(op);
        }
        if let Some(f) = self.fetching.take() {
            self.cancel(f.op);
        }
    }
}

/// A result's line under its title, as tags: its artist and pack as the
/// source states them (data), its charts.
fn tags(r: &ItemMeta) -> Vec<String> {
    use ddi_l10n::UiText;
    let mut out: Vec<String> = Vec::new();
    for t in [r.artist.as_deref(), r.pack.as_deref()].into_iter().flatten() {
        if !t.is_empty() {
            out.push(UiText::data(t).into());
        }
    }
    out.extend(
        r.charts
            .iter()
            .take(6)
            .map(|c| ddi_ui::screens::find_songs::chart_tag(&c.style, c.meter)),
    );
    out
}

/// The Find songs page over `songs`' sources.
pub fn screen(
    env: &Env,
    songs: &Rc<Songs>,
    sound: Option<SharedPlayer>,
) -> ddi_ui::screens::find_songs::FindScreen {
    let model = Rc::new(RefCell::new(Finder::new(env.clone(), songs.clone(), sound)));
    ddi_ui::screens::find_songs::screen(env.clone(), model)
}
