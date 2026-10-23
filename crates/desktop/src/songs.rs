//! Song select on the library on disk (`docs/plans/desktop.md`, 7.3):
//! the bundled songs and the song folders' (`crate::library`), listed as
//! the web lists its library (`ddi_ui::state::select_view`), with the
//! player's bests from the history in the data folder
//! (`crate::history`). The list shows the cache's songs at once and
//! follows the scan through song select's feed. Song select's calls on
//! desktop: previews from disk, a folder or zips added through the
//! system's picker (read in place), a pack read again, packs and songs
//! taken off the list (files untouched).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use ddi_library::manifest::ManifestEntry;
use ddi_ui::message::Message;
use ddi_ui::screens::Env;
use ddi_ui::screens::data::SelectData;
use ddi_ui::screens::select::{
    FEED, OpenFn, PlayFn, SelectCall, SelectScreen, SharedFeed, ShellFn,
};
use ddi_ui::state::history::Plays;
use ddi_ui::state::place::MenuPlace;
use ddi_ui::state::select_view::{self, Listed};
use ddi_ui::state::settings::Settings;
use ddi_ui::{Cx, Ui};

use crate::library::{Hidden, LibSong, Library};
use crate::preview::SharedPlayer;

/// How often song select gets the history's bests while it keeps changing.
const HISTORY_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// What song select, options and the shell share of the library.
pub struct Songs {
    #[cfg(danoni_importer)]
    imports: RefCell<crate::danoni::Imports>,
    pub library: RefCell<Option<Library>>,
    pub plays: Option<Rc<Plays>>,
    /// The plays' stale summaries worked out again (`crate::history`).
    pub rework: RefCell<Option<crate::history::Rework>>,
    pub feed: SharedFeed,
    pub hidden: RefCell<Hidden>,
    /// Folders added for this run only (`--songs`).
    pub extra: RefCell<Vec<PathBuf>>,
    /// Folders just added or read again: what the player hid from them
    /// comes back once the scan has them.
    show_from: RefCell<Vec<PathBuf>>,
    /// The first start's offer was looked at (once a run).
    offered: Cell<bool>,
    /// Wakes the event loop (news from a thread).
    pub wake: RefCell<Option<Arc<dyn Fn() + Send + Sync>>>,
    /// Song select's notices (zips refused by the picker).
    notices: RefCell<Vec<ddi_ui::screens::data::Notice>>,
    pub bundled: Vec<ManifestEntry>,
    place: RefCell<MenuPlace>,
    history_seen: Cell<u64>,
    /// When song select last got the history's bests: a change within
    /// [`HISTORY_EVERY`] of it waits (a pass working out stale plays
    /// changes the history every few milliseconds).
    history_sent: Cell<Option<std::time::Instant>>,
    /// The options screen while it is open (its song folders' rows).
    pub options: Rc<RefCell<Option<Rc<crate::options::Shared>>>>,
    /// What song select asked to play (`crate::play::host` takes it).
    pub play: Rc<RefCell<Option<crate::play::Request>>>,
    /// Song select's autoplay switch.
    pub autoplay: Cell<bool>,
    /// Calibration's tests autoplayed, and how late (`--auto`, `--bias`:
    /// the web's testing parameters).
    pub testing: Cell<(bool, f64)>,
    /// The editor asked for by `--edit SONG [--chart N]`: opened once the
    /// library knows the song ([`Songs::edit_ready`]).
    pub edit: RefCell<Option<(String, usize)>>,
}

impl Songs {
    pub fn new(extra: Vec<PathBuf>) -> Rc<Songs> {
        // Their noteskins too.
        crate::library::set_extra_folders(&extra);
        let (plays, rework) = crate::history::open().unzip();
        Rc::new(Songs {
            #[cfg(danoni_importer)]
            imports: RefCell::default(),
            library: RefCell::new(None),
            plays,
            rework: RefCell::new(rework),
            feed: SharedFeed::default(),
            hidden: RefCell::new(Hidden::load()),
            extra: RefCell::new(extra),
            show_from: RefCell::default(),
            wake: RefCell::default(),
            offered: Cell::new(false),
            notices: RefCell::default(),
            bundled: crate::library::bundled(),
            place: RefCell::new(MenuPlace::load()),
            history_seen: Cell::new(u64::MAX),
            history_sent: Cell::new(None),
            options: Rc::default(),
            play: Rc::default(),
            autoplay: Cell::new(false),
            testing: Cell::new((false, 0.0)),
            edit: RefCell::default(),
        })
    }

    /// Cancel and drain native jobs without joining a live worker on the UI thread.
    pub fn shutdown(&self) -> bool {
        #[cfg(danoni_importer)]
        {
            self.imports.borrow_mut().shutdown()
        }
        #[cfg(not(danoni_importer))]
        {
            true
        }
    }
    pub fn drained(&self) -> bool {
        #[cfg(danoni_importer)]
        {
            self.imports.borrow().drained()
        }
        #[cfg(not(danoni_importer))]
        {
            false
        }
    }
    pub fn importing(&self) -> bool {
        #[cfg(danoni_importer)]
        {
            self.imports.borrow().busy()
        }
        #[cfg(not(danoni_importer))]
        {
            false
        }
    }

    /// Every folder read: the settings' then this run's.
    pub fn folders(&self) -> Vec<PathBuf> {
        let mut f = crate::library::folders();
        for e in self.extra.borrow().iter() {
            if !f.contains(e) {
                f.push(e.clone());
            }
        }
        f
    }

    /// Starts the scan (the cache's list first); `wake` wakes the loop.
    pub fn start(&self, wake: Arc<dyn Fn() + Send + Sync>) {
        *self.wake.borrow_mut() = Some(wake.clone());
        #[cfg(danoni_importer)]
        self.imports.borrow_mut().load(wake.clone());
        crate::library::set_preview_wake(wake.clone());
        let library = Library::start(
            self.folders(),
            crate::library::cache_dir().map(|d| d.join(crate::library::CACHE_FILE)),
            wake,
        );
        *self.library.borrow_mut() = Some(library);
    }

    /// Songs read outside a scan in the list at once, and `remove`d ones
    /// out (a new song made or taken away, `crate::editor::new_song`); a
    /// scan follows.
    pub fn put_songs(&self, songs: Vec<LibSong>, remove: &[String]) {
        if let Some(l) = self.library.borrow_mut().as_mut() {
            l.put_songs(songs, remove);
        }
        self.rescan(Vec::new());
    }

    /// The song and chart `--edit` asked for, once the library knows the
    /// song (or has read every folder without it).
    pub fn edit_ready(&self) -> Option<(String, usize)> {
        let known = self.edit.borrow().as_ref().is_some_and(|(id, _)| {
            crate::library::song(id).is_some() || self.bundled.iter().any(|e| &e.id == id)
        });
        if known || self.listed_all() {
            self.edit.borrow_mut().take()
        } else {
            None
        }
    }

    /// Scans again (`force`: these folders whole).
    pub fn rescan(&self, force: Vec<PathBuf>) {
        let folders = self.folders();
        if let Some(l) = self.library.borrow_mut().as_mut() {
            l.scan(folders, force, false);
        }
        self.tell_options();
    }

    /// `f` of the library's songs the list shows: by pack and title, what
    /// the player removed left out.
    fn shown<R>(&self, f: impl FnOnce(&[&LibSong]) -> R) -> R {
        let hidden = self.hidden.borrow();
        let library = self.library.borrow();
        let mut songs: Vec<&LibSong> = library
            .as_ref()
            .map(|l| l.songs.iter().filter(|s| !hidden.hides(s)).collect())
            .unwrap_or_default();
        songs.sort_by_cached_key(|s| (s.pack.to_lowercase(), s.entry.title.to_lowercase()));
        f(&songs)
    }

    /// The list now: the bundled songs, then the library's by pack and
    /// title, what the player removed left out.
    pub fn listed(&self) -> Vec<Listed> {
        let mut out: Vec<Listed> = self.bundled.iter().map(Listed::bundled).collect();
        self.shown(|songs| {
            out.extend(songs.iter().map(|s| Listed {
                entry: s.entry.clone(),
                pack: Some(s.pack.clone()),
                pack_banner: s.pack_banner.as_deref().map(|p| s.root_source(p)),
                banner: s.entry.banner.as_deref().map(|b| s.source(b)),
                background: s.entry.background.as_deref().map(|b| s.source(b)),
                jacket: s.entry.jacket.as_deref().map(|b| s.source(b)),
                cd_title: s.entry.cd_title.as_deref().map(|b| s.source(b)),
                removable: true,
            }))
        });
        out
    }

    /// The layouts with charts in the list ([`Songs::listed`]'s songs, in
    /// its order).
    pub fn library_styles(&self) -> Vec<ddi_chart::Layout> {
        self.shown(|songs| {
            select_view::library_styles(self.bundled.iter().chain(songs.iter().map(|s| &s.entry)))
        })
    }

    /// Song select's data now.
    pub fn data(&self) -> SelectData {
        let settings = Settings::load();
        let empty = ddi_ui::state::history::History::default();
        let history = self.plays.as_ref().map(|p| p.history());
        let rereadable = self
            .library
            .borrow()
            .as_ref()
            .map(|l| l.rereadable_packs())
            .unwrap_or_default();
        let mut data = select_view::select_data(
            &self.listed(),
            // Managed snapshots are published by imports; only folders re-read.
            &|pack| rereadable.contains(&pack.to_lowercase()),
            &settings,
            history.as_deref().unwrap_or(&empty),
            &self.place.borrow(),
        );
        data.loading =
            self.library.borrow().as_ref().is_none_or(|l| l.scanning) || self.importing();
        data.autoplay = self.autoplay.get();
        // The screens' wording: song folders read in place.
        data.import.platform = ddi_ui::screens::data::Platform::Desktop;
        #[cfg(danoni_importer)]
        self.imports.borrow().view(&mut data.import);
        data
    }

    /// Hands song select the list now.
    pub fn send(&self, ui: &mut Ui) {
        self.feed.borrow_mut().data = Some(self.data());
        ui.broadcast(Message::new(FEED));
    }

    /// No first start's offer in this run (one opened from the command
    /// line, `--play`, `--calibrate`: its prompt would be under it).
    pub fn skip_offer(&self) {
        self.offered.set(true);
    }

    /// The first start's offer (`crate::library::offer`): with no song
    /// folder and no answer yet, the other games' song folders found are
    /// offered in a dialog over song select; the answer is kept.
    pub fn offer(self: &Rc<Self>, ui: &mut Ui) {
        use crate::library::offer;
        use ddi_platform::storage::{self, Area};
        if self.offered.replace(true)
            || !self.folders().is_empty()
            || storage::load::<String>(Area::Local, offer::OFFER_KEY).is_some()
        {
            return;
        }
        let found = offer::find(offer::Os::this(), &offer::Bases::here());
        if found.is_empty() {
            return;
        }
        let shown: Vec<String> = found.iter().map(|p| p.display().to_string()).collect();
        let me = Rc::downgrade(self);
        self.feed.borrow_mut().overlay = Some(Box::new(move |env: &Env, cx: &mut Cx| {
            let answer: ddi_ui::screens::folders_offer::AnswerFn = Rc::new(move |add, _cx| {
                let _ = storage::save(
                    Area::Local,
                    offer::OFFER_KEY,
                    &if add { "added" } else { "declined" },
                );
                if add && let Some(songs) = me.upgrade() {
                    songs.add_folders(found.clone());
                }
            });
            cx.overlay(
                ddi_ui::screens::folders_offer::FoldersOfferDialog::new(env.clone(), shown, answer),
                true,
            );
        }));
        ui.broadcast(Message::new(FEED));
    }

    /// The scan's and the history's news; whether the list was sent.
    pub fn poll(self: &Rc<Self>, ui: &mut Ui) -> bool {
        if ui.layers().last().is_some_and(|l| l.name == "select") {
            self.offer(ui);
        }
        #[cfg(danoni_importer)]
        {
            let news = self.imports.borrow_mut().poll();
            if let Some(news) = news {
                if let (Some(songs), Some(library)) =
                    (news.songs, self.library.borrow_mut().as_mut())
                {
                    library.set_managed(songs);
                    let ids = news.restore.iter().map(String::as_str).collect::<Vec<_>>();
                    if !ids.is_empty()
                        && self.hidden.borrow_mut().show_selected(&library.songs, &ids)
                    {
                        self.hidden.borrow().save();
                    }
                }
                let data = self.data();
                {
                    let mut feed = self.feed.borrow_mut();
                    feed.import = Some(data.import.clone());
                    feed.data = Some(data);
                }
                ui.broadcast(Message::new(FEED));
            }
            // Build just the import view; rebuilding the complete library each
            // frame is expensive on large song collections.
            if let Some(view) = self.imports.borrow_mut().changed_view() {
                self.feed.borrow_mut().import = Some(view);
                ui.broadcast(Message::new(FEED));
            }
        }
        let scanned = self
            .library
            .borrow_mut()
            .as_mut()
            .is_some_and(Library::take);
        let generation = self.plays.as_ref().map_or(0, |p| p.generation());
        // A change soon after the last one sent waits for its turn
        // (`history_due`), as the web publishes at most once a second.
        let history = generation != self.history_seen.get()
            && self
                .history_due()
                .is_none_or(|at| at <= std::time::Instant::now());
        if history {
            self.history_seen.set(generation);
            self.history_sent.set(Some(std::time::Instant::now()));
        }
        if scanned {
            self.show_again();
            self.tell_options();
            if let Some(r) = self.rework.borrow_mut().as_mut() {
                r.library_changed();
            }
        }
        if scanned || history {
            self.send(ui);
            return true;
        }
        false
    }

    /// The library knows its songs (no scan running): the history's
    /// stale plays look for their charts only then.
    fn listed_all(&self) -> bool {
        self.library.borrow().as_ref().is_some_and(|l| !l.scanning)
    }

    /// The history's stale plays worked out again (`crate::history::
    /// Rework`): its thread's answers taken, its next step when due.
    /// `playing` and `shown`: a song's play is on, its prompt, play or
    /// results are up. Before [`Songs::poll`], which sends song select the
    /// bests that changed.
    pub fn rework(&self, playing: bool, shown: bool) {
        let mut rework = self.rework.borrow_mut();
        let (Some(plays), Some(rework)) = (&self.plays, rework.as_mut()) else {
            return;
        };
        let wake = self.wake.borrow().clone();
        rework.poll(plays, playing, shown, self.listed_all(), wake.as_ref());
        // An import's progress and report, to options while it is open.
        if let Some(o) = self.options.borrow().as_ref() {
            let report = rework.take_imported();
            o.set_import(rework.importing(), report, plays.history().len());
        }
    }

    /// Imports the plays of the export at `path` (`crate::history::
    /// Rework::import`); whether it started (none while one runs).
    pub fn import_plays(&self, path: PathBuf) -> bool {
        let mut rework = self.rework.borrow_mut();
        let (Some(plays), Some(rework)) = (&self.plays, rework.as_mut()) else {
            return false;
        };
        let wake = self.wake.borrow().clone();
        rework.import(path, plays, wake.as_ref())
    }

    /// When [`Songs::rework`] has a step to take, or [`Songs::poll`] bests
    /// it held back to send.
    pub fn rework_due(&self) -> Option<std::time::Instant> {
        let history = self.history_due().filter(|_| {
            self.plays.as_ref().map_or(0, |p| p.generation()) != self.history_seen.get()
        });
        let rework = if self.listed_all() {
            self.rework.borrow().as_ref().and_then(|r| r.due())
        } else {
            None
        };
        history.into_iter().chain(rework).min()
    }

    /// When song select may next get the history's bests.
    fn history_due(&self) -> Option<std::time::Instant> {
        self.history_sent.get().map(|t| t + HISTORY_EVERY)
    }

    /// Song `id`'s chart read for its play: its stale plays come first.
    pub fn chart_loaded(&self, id: &str, song: &ddi_chart::Song) {
        if let (Some(plays), Some(rework)) = (&self.plays, self.rework.borrow_mut().as_mut()) {
            rework.chart_loaded(plays, id, song);
        }
    }

    /// What options shows of the folders and the last scan.
    pub fn folders_note(&self) -> String {
        let library = self.library.borrow();
        let Some(l) = library.as_ref() else {
            return String::new();
        };
        let r = &l.report;
        let mut parts = vec![format!(
            "{} songs{}",
            l.songs.len(),
            if l.scanning { ", reading…" } else { "" }
        )];
        if !r.failed.is_empty() {
            parts.push(format!("{} not read", r.failed.len()));
        }
        if !r.duplicates.is_empty() {
            parts.push(format!(
                "{} packs also in an earlier folder",
                r.duplicates.len()
            ));
        }
        if !r.missing.is_empty() {
            parts.push(format!("{} folders missing", r.missing.len()));
        }
        parts.join(" · ")
    }

    /// The options screen's rows follow the folders.
    pub fn tell_options(&self) {
        if let Some(o) = self.options.borrow().as_ref() {
            o.set_folders(
                self.folders()
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect(),
                self.folders_note(),
            );
        }
    }

    /// Puts back on the list what the player hid from the folders just
    /// added or read again (`show_from`), now the scan has them.
    fn show_again(&self) {
        let from = std::mem::take(&mut *self.show_from.borrow_mut());
        if from.is_empty() {
            return;
        }
        let library = self.library.borrow();
        let Some(l) = library.as_ref() else {
            return;
        };
        let mut hidden = self.hidden.borrow_mut();
        let mut changed = false;
        let mut packs: Vec<&str> = l
            .songs
            .iter()
            .filter(|s| from.iter().any(|f| s.root.starts_with(f)))
            .map(|s| s.pack.as_str())
            .collect();
        packs.sort_unstable();
        packs.dedup();
        for pack in packs {
            changed |= hidden.show_pack(&l.songs, pack);
        }
        if changed {
            hidden.save();
        }
    }

    /// Adds `paths` (folders or zips) to the settings' folders and reads
    /// them; what was hidden from them comes back.
    pub fn add_folders(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let mut list = crate::library::folders();
        for p in &paths {
            if !list.contains(p) {
                list.push(p.clone());
            }
        }
        crate::library::set_folders(&list);
        self.show_from.borrow_mut().extend(paths);
        self.rescan(Vec::new());
    }

    /// Takes folder `index` of [`Songs::folders`] off the list (the
    /// settings', or this run's `--songs`); nothing is deleted.
    pub fn remove_folder(&self, index: usize) {
        let Some(path) = self.folders().get(index).cloned() else {
            return;
        };
        let mut list = crate::library::folders();
        if list.contains(&path) {
            list.retain(|p| p != &path);
            crate::library::set_folders(&list);
        }
        self.extra.borrow_mut().retain(|p| p != &path);
        crate::library::set_extra_folders(&self.extra.borrow());
        self.rescan(Vec::new());
    }

    /// Forgets every song folder, what was hidden and the cache (options'
    /// reset; nothing in the folders is deleted).
    pub fn forget_all(&self) {
        crate::library::set_folders(&[]);
        self.extra.borrow_mut().clear();
        crate::library::set_extra_folders(&[]);
        *self.hidden.borrow_mut() = Hidden::default();
        self.hidden.borrow().save();
        if let Some(dir) = crate::library::cache_dir() {
            let _ = std::fs::remove_file(dir.join(crate::library::CACHE_FILE));
            let _ = std::fs::remove_dir_all(dir.join("previews"));
        }
        self.rescan(Vec::new());
    }

    /// Song select's notices now.
    fn send_notices(&self, cx: &mut Cx) {
        self.feed.borrow_mut().notices = Some(self.notices.borrow().clone());
        cx.broadcast(Message::new(FEED));
    }

    /// Song select's calls.
    pub fn call(self: &Rc<Self>, call: SelectCall, sound: Option<&SharedPlayer>, cx: &mut Cx) {
        match call {
            SelectCall::ImportDanoni { url, offline } => {
                #[cfg(danoni_importer)]
                if let Some(wake) = self.wake.borrow().clone() {
                    self.imports.borrow_mut().start(url, offline, wake);
                    self.feed.borrow_mut().import = Some(self.imports.borrow().current_view());
                    cx.broadcast(Message::new(FEED));
                }
                #[cfg(not(danoni_importer))]
                let _ = (url, offline);
            }
            SelectCall::DanoniOriginDecision {
                request_id,
                allow,
                remember,
            } => {
                #[cfg(danoni_importer)]
                self.imports
                    .borrow_mut()
                    .origin_decision(request_id, allow, remember);
                #[cfg(not(danoni_importer))]
                let _ = (request_id, allow, remember);
            }
            SelectCall::CancelDanoni => {
                #[cfg(danoni_importer)]
                self.imports.borrow().cancel();
            }
            SelectCall::Leave => {
                if let Some(p) = sound {
                    p.borrow_mut().pause();
                }
                cx.close(true);
            }
            SelectCall::Selected { song, .. } => {
                if let Some(p) = sound {
                    p.borrow_mut().select(song.as_deref());
                }
            }
            SelectCall::PickFolder => self.add_folders(crate::os::pick(true)),
            SelectCall::PickFiles => {
                let (zips, other): (Vec<PathBuf>, Vec<PathBuf>) =
                    crate::os::pick(false).into_iter().partition(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
                    });
                if !other.is_empty() {
                    let names: Vec<String> = other
                        .iter()
                        .filter_map(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .collect();
                    self.notices
                        .borrow_mut()
                        .push(ddi_ui::screens::data::Notice {
                            text: format!(
                                "Only zips are read as packs; left out: {}. A folder of songs is \
                             added with “choose folder”.",
                                names.join(", ")
                            ),
                            link: None,
                            error: true,
                        });
                    self.send_notices(cx);
                }
                self.add_folders(zips);
            }
            SelectCall::Reread(pack) => {
                // That pack read whole, the rest from the cache; hidden
                // songs of it come back.
                let folders = self.folders();
                if let Some(l) = self.library.borrow_mut().as_mut() {
                    if self.hidden.borrow_mut().show_pack(&l.songs, &pack) {
                        self.hidden.borrow().save();
                    }
                    l.reread_pack(folders, &pack);
                }
                self.tell_options();
            }
            SelectCall::RemovePack(name) => {
                self.hidden.borrow_mut().hide_pack(&name);
                self.hidden.borrow().save();
            }
            SelectCall::RemoveSong(id) => {
                self.hidden.borrow_mut().hide_song(&id);
                self.hidden.borrow().save();
            }
            SelectCall::Place(p) => {
                let mut place = self.place.borrow_mut();
                place.remember(&p);
                place.save();
            }
            // Plays from now on autoplayed (never counted).
            SelectCall::Autoplay(on) => self.autoplay.set(on),
            // A kept play (`docs/plans/play-history.md`, Replays): its
            // replay or its results through the play's host; forgotten.
            SelectCall::WatchPlay(ref id) | SelectCall::ShowPlay(ref id) => {
                let results = matches!(call, SelectCall::ShowPlay(_));
                if let Some(plays) = &self.plays {
                    *self.play.borrow_mut() =
                        Some(crate::play::replays::request(plays, id, results));
                }
            }
            SelectCall::RemovePlay(id) => {
                if let Some(plays) = &self.plays {
                    plays.remove_play(&id, |r| {
                        if let Err(e) = r {
                            eprintln!("play history: removing a play: {e}");
                        }
                    });
                }
            }
            // The editor is opened by `screen`'s calls.
            SelectCall::Open(_)
            | SelectCall::ImportOption(..)
            | SelectCall::Notice(_)
            | SelectCall::Edit { .. }
            | SelectCall::NewSong => {}
        }
    }
}

/// Song select on the library: Options opens the options on the
/// settings; a chart chosen is played (`crate::play::host`), autoplayed
/// when the switch says so.
pub fn screen(env: &Env, songs: &Rc<Songs>, sound: Option<SharedPlayer>) -> SelectScreen {
    let play: PlayFn = {
        let songs = songs.clone();
        Rc::new(move |song, chart, _env, _cx| {
            let mut request =
                crate::play::Request::song(&song.id, chart.index, songs.autoplay.get());
            request.backdrop = song.background.clone();
            *songs.play.borrow_mut() = Some(request);
        })
    };
    let open: OpenFn = {
        let (songs, sound) = (songs.clone(), sound.clone());
        Rc::new(move |env: &Env, cx: &mut Cx| {
            if let Some(p) = &sound {
                p.borrow_mut().pause();
            }
            let (screen, shared) = crate::options::screen(env, sound.clone(), Some(songs.clone()));
            *songs.options.borrow_mut() = Some(shared);
            songs.tell_options();
            cx.push(screen);
        })
    };
    let calls: ShellFn = {
        let (songs, env) = (songs.clone(), env.clone());
        Rc::new(move |call, cx| {
            if matches!(&call, SelectCall::Open(what) if what == "calibrate") {
                if let Some(p) = &sound {
                    p.borrow_mut().pause();
                }
                cx.push(crate::calibrate::menu(&env, &songs, sound.clone()));
                return;
            }
            // The chart editor (`docs/plans/editor.md`).
            if let SelectCall::Edit { song, chart } = &call {
                match crate::editor::screen(&env, &songs, song, *chart) {
                    Ok(screen) => {
                        if let Some(p) = &sound {
                            p.borrow_mut().pause();
                        }
                        cx.push(screen);
                    }
                    Err(e) => eprintln!("editor: {song}: {e}"),
                }
                return;
            }
            // A new song from a music file, in the editor.
            if matches!(call, SelectCall::NewSong) {
                match crate::editor::new_song::pick(&env, &songs) {
                    Some(Ok(screen)) => {
                        if let Some(p) = &sound {
                            p.borrow_mut().pause();
                        }
                        songs.feed.borrow_mut().data = Some(songs.data());
                        cx.broadcast(Message::new(FEED));
                        cx.push(screen);
                    }
                    Some(Err(e)) => {
                        songs
                            .notices
                            .borrow_mut()
                            .push(ddi_ui::screens::data::Notice {
                                text: format!("The new song could not be made: {e}"),
                                link: None,
                                error: true,
                            });
                        songs.send_notices(cx);
                    }
                    None => {}
                }
                return;
            }
            let rows_change = matches!(
                call,
                SelectCall::RemovePack(_) | SelectCall::RemoveSong(_) | SelectCall::Reread(_)
            );
            songs.call(call, sound.as_ref(), cx);
            if rows_change {
                songs.feed.borrow_mut().data = Some(songs.data());
                cx.broadcast(Message::new(FEED));
            }
        })
    };
    let mut screen = SelectScreen::new(env.clone(), songs.data(), None, play, open)
        .with_shell(calls, songs.feed.clone())
        .with_editing();
    // A chart's kept plays (`docs/plans/play-history.md`, Replays).
    if let Some(plays) = &songs.plays {
        screen = screen.with_plays(ddi_ui::state::history::plays_list(
            plays.clone(),
            Rc::new(crate::play::replays::tz_minutes),
        ));
    }
    screen
}
