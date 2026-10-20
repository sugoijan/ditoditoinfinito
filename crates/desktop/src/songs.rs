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

/// What song select, options and the shell share of the library.
pub struct Songs {
    #[cfg(danoni_importer)]
    imports: RefCell<crate::danoni::Imports>,
    pub library: RefCell<Option<Library>>,
    pub plays: Option<Rc<Plays>>,
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
    /// The options screen while it is open (its song folders' rows).
    pub options: Rc<RefCell<Option<Rc<crate::options::Shared>>>>,
    /// What song select asked to play (`crate::play::host` takes it).
    pub play: Rc<RefCell<Option<crate::play::Request>>>,
    /// Song select's autoplay switch.
    pub autoplay: Cell<bool>,
    /// Calibration's tests autoplayed, and how late (`--auto`, `--bias`:
    /// the web's testing parameters).
    pub testing: Cell<(bool, f64)>,
}

impl Songs {
    pub fn new(extra: Vec<PathBuf>) -> Rc<Songs> {
        // Their noteskins too.
        crate::library::set_extra_folders(&extra);
        Rc::new(Songs {
            #[cfg(danoni_importer)]
            imports: RefCell::default(),
            library: RefCell::new(None),
            plays: crate::history::open(),
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
            options: Rc::default(),
            play: Rc::default(),
            autoplay: Cell::new(false),
            testing: Cell::new((false, 0.0)),
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

    /// Scans again (`force`: these folders whole).
    pub fn rescan(&self, force: Vec<PathBuf>) {
        let folders = self.folders();
        if let Some(l) = self.library.borrow_mut().as_mut() {
            l.scan(folders, force, false);
        }
        self.tell_options();
    }

    /// The list now: the bundled songs, then the library's by pack and
    /// title, what the player removed left out.
    pub fn listed(&self) -> Vec<Listed> {
        let mut out: Vec<Listed> = self
            .bundled
            .iter()
            .map(|e| Listed {
                entry: e.clone(),
                pack: None,
                pack_banner: None,
                banner: e.banner.as_ref().map(|b| format!("songs/{}/{b}", e.id)),
                background: e.background.as_ref().map(|b| format!("songs/{}/{b}", e.id)),
                removable: false,
            })
            .collect();
        let hidden = self.hidden.borrow();
        let library = self.library.borrow();
        let mut songs: Vec<&LibSong> = library
            .as_ref()
            .map(|l| l.songs.iter().filter(|s| !hidden.hides(s)).collect())
            .unwrap_or_default();
        songs.sort_by_cached_key(|s| (s.pack.to_lowercase(), s.entry.title.to_lowercase()));
        out.extend(songs.into_iter().map(|s| Listed {
            entry: s.entry.clone(),
            pack: Some(s.pack.clone()),
            pack_banner: s.pack_banner.as_deref().map(|p| s.root_source(p)),
            banner: s.entry.banner.as_deref().map(|b| s.source(b)),
            background: s.entry.background.as_deref().map(|b| s.source(b)),
            removable: true,
        }));
        out
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
        self.offer(ui);
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
        let history = generation != self.history_seen.replace(generation);
        if scanned {
            self.show_again();
            self.tell_options();
        }
        if scanned || history {
            self.send(ui);
            return true;
        }
        false
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
            let ids: Vec<&str> = l
                .songs
                .iter()
                .filter(|s| s.pack.eq_ignore_ascii_case(pack))
                .map(|s| s.entry.id.as_str())
                .collect();
            changed |= hidden.show(pack, &ids);
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
                    let ids: Vec<&str> = l
                        .songs
                        .iter()
                        .filter(|s| s.pack.eq_ignore_ascii_case(&pack))
                        .map(|s| s.entry.id.as_str())
                        .collect();
                    if self.hidden.borrow_mut().show(&pack, &ids) {
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
            SelectCall::Open(_) | SelectCall::ImportOption(..) | SelectCall::Notice(_) => {}
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
                cx.push(crate::calibrate::menu(&env, &songs, sound.clone()));
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
        .with_shell(calls, songs.feed.clone());
    // A chart's kept plays (`docs/plans/play-history.md`, Replays).
    if let Some(plays) = &songs.plays {
        screen = screen.with_plays(ddi_ui::state::history::plays_list(
            plays.clone(),
            Rc::new(crate::play::replays::tz_minutes),
        ));
    }
    screen
}
