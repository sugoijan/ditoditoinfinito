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
//!
//! Opened from an online room (stage 3, [`room_screen`]): a result is
//! added to the room's list (Add to room, fetched first where the library
//! does not have it) through the room's client as My songs adds a song,
//! with the same checks and the same words, its item kept for the room
//! (`super::rooms`).
//!
//! Every play of a song found is kept (`super::fetched::keeps_as`): the
//! page lists the songs played before from the source chosen from their
//! plays' origins ([`ddi_ui::state::history::History::played_before`],
//! at most [`PLAYED_MAX`] a source, no source asked to list them), and
//! those of sources not on now with why; Play on one asks its source what
//! the item is now (`Describe`) and fetches its current revision (saying
//! so where that is not the one last played), then lists its songs as Play
//! on a result does. A source without metadata is fetched by the revision
//! last played; one that says it no longer has the item marks its row so
//! for the run. Nothing is deleted.
//!
//! Import from This computer adds the song's folder to the library's song
//! folders in place (`super::this_computer::InPlace`), copying nothing
//! and recording nothing in the import catalog; a plugin's songs are
//! written into the import store as before.
//!
//! How far a result may go is its source's level lowered by its item's
//! (`super::Reach`): below `rooms` it offers no Add to room, below `keep`
//! no Import, and its line says why; each is checked again at the press
//! and once the item is fetched (by the level the store keeps), and the
//! import checks it itself (`super::import::import`).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::Receiver;

use ddi_source_host::host::{Event, Failure, HostFailure, Request, Stop};
use ddi_source_host::protocol::{FailCode, ItemMeta, OpId, Step as HostStep};
use ddi_source_host::store::{FetchedSong, ItemKey};
use ddi_ui::Cx;
use ddi_ui::screens::Env;
use ddi_ui::screens::data::SongEntry;
use ddi_ui::screens::find_songs::{
    Busy, FindModel, FindPress, FindView, Note, PlayedSection, PlayedState, PlayedView, Problem,
    ResultView, RoomOffer, SourceName, SourceView, Step,
};
use ddi_ui::screens::room_dialogs::NoticeDialog;
use ddi_ui::state::history::{PlayOrigin, Plays};
use ddi_ui::state::room::{Refusal, SharedRoom, Unsupported};

use super::Reach;
use super::this_computer;
use crate::preview::SharedPlayer;
use crate::songs::Songs;

/// How long the search field rests before it is searched.
const REST_S: f64 = 0.35;

/// Results asked for a page.
const PAGE: u32 = 50;

/// The songs played before a source lists, the most recent first (the
/// rest counted; their plays kept all the same).
pub const PLAYED_MAX: usize = 50;

/// What a fetch is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    Play,
    Import,
    /// Added to the online room the page was opened from.
    Room,
}

struct Fetching {
    op: OpId,
    source: String,
    item: ItemMeta,
    purpose: Purpose,
    step: Option<Step>,
    done: Option<u64>,
    total: Option<u64>,
    /// A song played before fetched again: its origin, and whether its
    /// source has another revision of it now.
    again: Option<(PlayOrigin, bool)>,
}

/// A song played before, its source asked what the item is now.
struct Describing {
    op: OpId,
    source: String,
    origin: PlayOrigin,
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
    library_keys: Rc<HashSet<String>>,
    library_seen: usize,
    /// Items imported before, by source and item (the import's catalog).
    imported: HashSet<(String, String)>,
    /// A found item's songs to show over the page (the next frame).
    show: Option<(String, Vec<FetchedSong>)>,
    /// The online room the page was opened from (stage 3), and its
    /// version as last shown.
    room: Option<SharedRoom>,
    room_seen: u64,
    /// A fetched item's songs to add to the room (the next frame).
    add: Option<Vec<FetchedSong>>,
    /// The play history (the songs played before), and its generation as
    /// last shown.
    plays: Option<Rc<Plays>>,
    plays_seen: u64,
    /// A song played before being looked up at its source.
    describing: Option<Describing>,
    /// Items their sources said they no longer have, by source and item
    /// (this run).
    gone: HashSet<(String, String)>,
}

impl Finder {
    pub fn new(
        env: Env,
        songs: Rc<Songs>,
        sound: Option<SharedPlayer>,
        room: Option<SharedRoom>,
    ) -> Finder {
        // What was read of the folders before is read again.
        if let Some(t) = songs
            .sources
            .borrow()
            .as_ref()
            .and_then(|s| s.this_computer.clone())
        {
            t.refresh();
        }
        let plays = songs.plays.clone();
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
            library_keys: Rc::default(),
            library_seen: usize::MAX,
            imported: super::import::catalog()
                .into_iter()
                .map(|r| (r.source, r.item))
                .collect(),
            show: None,
            room,
            room_seen: 0,
            add: None,
            plays,
            plays_seen: 0,
            describing: None,
            gone: HashSet::new(),
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
            self.library_keys = self.songs.library_keys();
            self.changed();
        }
    }

    /// How far result `r`'s songs may go: the chosen source's level,
    /// lowered by the item's own (never raised).
    fn reach_of(&self, r: &ItemMeta) -> Reach {
        self.chosen_id()
            .and_then(|id| Some(self.songs.sources.borrow().as_ref()?.reach(&id)))
            .unwrap_or(Reach::Play)
            .narrowed(r.reach)
    }

    /// Why a result of level `reach` may not be used as `purpose` wants,
    /// in the page's words (the same as its line's), or `None`.
    fn refused_for(reach: Reach, purpose: Purpose) -> Option<Problem> {
        match purpose {
            Purpose::Play => None,
            Purpose::Room if !reach.rooms() => Some(Problem::PlayOnly),
            Purpose::Import if !reach.keeps() => Some(if reach.rooms() {
                Problem::NotForKeeping
            } else {
                Problem::PlayOnly
            }),
            Purpose::Room | Purpose::Import => None,
        }
    }

    /// What result `r` offers the room the page was opened from.
    fn room_offer(&self, r: &ItemMeta) -> RoomOffer {
        let Some(room) = &self.room else {
            return RoomOffer::None;
        };
        if !self.reach_of(r).rooms() {
            return RoomOffer::NotForRooms;
        }
        // By the key its source states, else by its songs as fetched in
        // this run (a source that states none).
        let mut keys: Vec<String> = r.song_key.iter().cloned().collect();
        if keys.is_empty()
            && let Some(source) = self.chosen_id()
            && let Some(store) = super::fetched::store()
            && let Ok(st) = store.lock()
        {
            let key = ItemKey::of(&source, &r.id, r.revision.as_deref());
            keys.extend(
                st.songs(key)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|s| s.song_key.clone()),
            );
        }
        let room = room.borrow();
        let listed = keys
            .into_iter()
            .any(|k| room.state().songs.by_key(&ddi_room::SongKey::new(k)).is_some());
        if listed {
            RoomOffer::InRoom
        } else {
            RoomOffer::Add
        }
    }

    /// The room's own checks before an addition, as My songs makes them
    /// (`RoomClient::may_add`, `add_blocked`): the refusal in its words.
    fn room_refusal(room: &SharedRoom) -> Option<Refusal> {
        let r = room.borrow();
        if !r.may_add() {
            return Some(Refusal::Room(ddi_room::Rejection::Unauthorized {
                capability: ddi_room::Capability::AddSongs,
            }));
        }
        r.add_blocked().map(Refusal::Room)
    }

    /// Charts of styles `styles` the room cannot play at all (its CPUs'
    /// four lanes, one player): refused as a library song's would be,
    /// where none of them is playable.
    fn unplayable<'a>(room: &SharedRoom, mut styles: impl Iterator<Item = &'a str>) -> Option<Refusal> {
        let r = room.borrow();
        let first = styles.next()?;
        if r.style_playable(first) || styles.any(|s| r.style_playable(s)) {
            return None;
        }
        Some(Refusal::Chart(Unsupported::Layout {
            style: first.to_string(),
        }))
    }

    /// Result `i` added to the room: the room's checks first; the
    /// library's own copy where it has the song, else fetched first.
    fn add_to_room(&mut self, i: usize, cx: &mut Cx) {
        if self.fetching.is_some() || self.importing.is_some() {
            return;
        }
        let (Some(room), Some(item)) = (self.room.clone(), self.results.get(i).cloned()) else {
            return;
        };
        // Checked here, not only by the button's absence.
        if let Some(p) = Self::refused_for(self.reach_of(&item), Purpose::Room) {
            self.problem = Some(p);
            self.changed();
            return;
        }
        let refused = Self::room_refusal(&room)
            .or_else(|| Self::unplayable(&room, item.charts.iter().map(|c| c.style.as_str())));
        if let Some(why) = refused {
            NoticeDialog::show_refusal(self.env.clone(), &room, &why, cx);
            return;
        }
        let mine = item
            .song_key
            .as_ref()
            .filter(|k| self.library_keys.contains(*k))
            .and_then(|k| {
                self.songs
                    .data()
                    .songs
                    .into_iter()
                    .find(|s| ddi_ui::state::room::song_key(s).as_ref() == Some(k))
            });
        match mine {
            Some(song) => self.add_entries(&room, vec![song], false, cx),
            None => self.fetch(i, Purpose::Room),
        }
    }

    /// `entries` added to `room`'s list as My songs adds songs (one, or a
    /// pack's at once), refused in the room's words; a fetched song's item
    /// kept for the room (`fetched`).
    fn add_entries(&mut self, room: &SharedRoom, entries: Vec<SongEntry>, fetched: bool, cx: &mut Cx) {
        let originals = |e: &SongEntry| -> Vec<String> {
            e.charts
                .iter()
                .filter(|c| c.version.is_none())
                .map(|c| c.style.clone())
                .collect()
        };
        let playable: Vec<SongEntry> = entries
            .iter()
            .filter(|e| {
                let styles = originals(e);
                Self::unplayable(room, styles.iter().map(String::as_str)).is_none()
                    && !styles.is_empty()
            })
            .cloned()
            .collect();
        if playable.is_empty() {
            let styles: Vec<String> = entries.iter().flat_map(originals).collect();
            let why = Self::unplayable(room, styles.iter().map(String::as_str)).unwrap_or(
                Refusal::Chart(Unsupported::NoDigest),
            );
            NoticeDialog::show_refusal(self.env.clone(), room, &why, cx);
            return;
        }
        let fresh: Vec<SongEntry> = {
            let r = room.borrow();
            playable.iter().filter(|e| r.listed(e).is_none()).cloned().collect()
        };
        let added = if playable.len() == 1 {
            room.borrow_mut().add_song(&playable[0]).map(|()| 1)
        } else {
            room.borrow_mut().add_pack(&playable)
        };
        let added = match added {
            // None of a pack's (all listed, or no room left): the room's
            // words for the first.
            Ok(0) => room.borrow_mut().add_song(&playable[0]).map(|()| 1),
            other => other,
        };
        match added {
            Ok(n) => {
                if fetched {
                    for e in &fresh {
                        super::rooms::keep(e);
                    }
                }
                self.note = Some(Note::AddedToRoom { songs: n });
            }
            Err(why) => NoticeDialog::show_refusal(self.env.clone(), room, &why, cx),
        }
        self.changed();
    }

    /// A fetched item's songs added to the room: its level as the store
    /// has it (the source's, lowered by the item's), then as any song.
    fn add_fetched(&mut self, songs: Vec<FetchedSong>, cx: &mut Cx) {
        let Some(room) = self.room.clone() else {
            return;
        };
        let reach = songs
            .first()
            .and_then(|s| super::fetched::item_reach(s.key))
            .unwrap_or(Reach::Play);
        if let Some(p) = Self::refused_for(reach, Purpose::Room) {
            self.problem = Some(p);
            self.changed();
            return;
        }
        if let Some(why) = Self::room_refusal(&room) {
            NoticeDialog::show_refusal(self.env.clone(), &room, &why, cx);
            return;
        }
        let ids: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
        let entries = crate::songs::found_data(&self.songs, String::new(), &ids).songs;
        self.add_entries(&room, entries, true, cx);
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
        // A new search: what the page said of the last one goes.
        self.note = None;
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
        if self.busy() {
            return;
        }
        let (Some(item), Some(source)) = (self.results.get(i).cloned(), self.chosen_id()) else {
            return;
        };
        self.problem = None;
        self.details = None;
        self.note = None;
        // Checked at the press, not only by the button's absence.
        if let Some(p) = Self::refused_for(self.reach_of(&item), purpose) {
            self.problem = Some(p);
            return;
        }
        // For a room, an item fetched already in this run is used as it
        // is (it may be held: a song added before, then removed).
        if purpose == Purpose::Room {
            let key = ItemKey::of(&source, &item.id, item.revision.as_deref());
            let songs = super::fetched::store().and_then(|st| {
                let mut st = st.lock().ok()?;
                st.contains(key).then(|| st.songs(key).map(<[FetchedSong]>::to_vec))?
            });
            if let Some(songs) = songs.filter(|s| !s.is_empty()) {
                self.add = Some(songs);
                self.changed();
                return;
            }
        }
        let op = self.request(
            &source,
            Request::Fetch {
                item: item.id.clone(),
                revision: item.revision.clone(),
                size: item.size,
                reach: item.reach,
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
                again: None,
            });
        }
        self.changed();
    }

    /// Something runs that a press waits for.
    fn busy(&self) -> bool {
        self.fetching.is_some() || self.importing.is_some() || self.describing.is_some()
    }

    /// The songs played before from source `source`, at most
    /// [`PLAYED_MAX`], and how many in all.
    fn played(&self, source: &str) -> (Vec<ddi_ui::state::history::PlayedBefore>, usize) {
        self.plays
            .as_ref()
            .map(|p| p.history().played_before(source, PLAYED_MAX))
            .unwrap_or_default()
    }

    /// Song `i` of the chosen source's played before, fetched again: its
    /// source asked what the item is now first.
    fn play_again(&mut self, i: usize) {
        if self.busy() {
            return;
        }
        let Some(source) = self.chosen_id() else {
            return;
        };
        let Some(row) = self.played(&source).0.into_iter().nth(i) else {
            return;
        };
        if self.gone.contains(&(source.clone(), row.origin.item.clone())) {
            return;
        }
        self.problem = None;
        self.details = None;
        self.note = None;
        let op = self.request(
            &source,
            Request::Describe {
                item: row.origin.item.clone(),
            },
        );
        if let Some(op) = op {
            self.describing = Some(Describing {
                op,
                source,
                origin: row.origin,
            });
        }
        self.changed();
    }

    /// A song played before fetched as `item` says it is now (its
    /// revision, size and level), said changed where its revision is not
    /// the one last played.
    fn fetch_again(&mut self, d: Describing, item: ItemMeta) {
        let changed = item.revision != d.origin.revision;
        let op = self.request(
            &d.source,
            Request::Fetch {
                item: d.origin.item.clone(),
                revision: item.revision.clone(),
                size: item.size,
                reach: item.reach,
            },
        );
        if let Some(op) = op {
            self.fetching = Some(Fetching {
                op,
                source: d.source,
                item: ItemMeta {
                    id: d.origin.item.clone(),
                    title: if item.title.is_empty() {
                        d.origin.title.clone()
                    } else {
                        item.title
                    },
                    ..item
                },
                purpose: Purpose::Play,
                step: None,
                done: None,
                total: None,
                again: Some((d.origin, changed)),
            });
        }
    }

    /// Import from This computer: the song's folder added to the library's
    /// song folders where it is, nothing copied; said by its folder's name.
    fn import_in_place(&mut self, i: usize) {
        if self.busy() {
            return;
        }
        let Some(item) = self.results.get(i).cloned() else {
            return;
        };
        self.problem = None;
        self.details = None;
        self.note = None;
        let listed = self.songs.folders();
        let found = self
            .songs
            .sources
            .borrow()
            .as_ref()
            .and_then(|s| s.this_computer.clone())
            .and_then(|t| t.in_place(&item.id, &listed));
        let name = |p: &std::path::Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string())
        };
        match found {
            Some(this_computer::InPlace::Add(path)) => {
                self.note = Some(Note::AddedFolder { folder: name(&path) });
                self.songs.add_folders(vec![path]);
            }
            Some(this_computer::InPlace::Covered(path)) => {
                self.note = Some(Note::InFolders { folder: name(&path) });
            }
            // Not known now (the page's list is older than what was read).
            None => self.problem = Some(Problem::NotFound),
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
        let describe = self.describing.as_ref().map(|d| d.op);
        match event {
            Event::Item { op, item } if Some(op) == describe => {
                if let Some(d) = self.describing.take() {
                    self.fetch_again(d, item);
                }
                self.changed();
            }
            Event::Failed { op, failure, .. } if Some(op) == describe => {
                let Some(d) = self.describing.take() else {
                    return;
                };
                let gone = matches!(
                    failure,
                    Failure::Plugin {
                        code: FailCode::NotFound | FailCode::Gone,
                        ..
                    }
                );
                let unsupported = matches!(
                    failure,
                    Failure::Host(HostFailure::Unsupported)
                        | Failure::Plugin {
                            code: FailCode::Unsupported,
                            ..
                        }
                );
                if gone {
                    self.gone.insert((d.source.clone(), d.origin.item.clone()));
                } else if unsupported {
                    // No metadata: fetched by the revision last played.
                    let item = ItemMeta {
                        id: d.origin.item.clone(),
                        title: d.origin.title.clone(),
                        revision: d.origin.revision.clone(),
                        ..ItemMeta::default()
                    };
                    self.fetch_again(d, item);
                } else {
                    self.problem = Self::problem_of(&failure);
                }
                self.changed();
            }
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
                } else if let Some(f) = self.fetching.take()
                    && let Some((origin, _)) = f.again
                    && matches!(
                        failure,
                        Failure::Plugin {
                            code: FailCode::NotFound | FailCode::Gone,
                            ..
                        }
                    )
                {
                    // A song played before its source no longer has.
                    self.gone.insert((f.source, origin.item));
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
                if f.again.as_ref().is_some_and(|(_, changed)| *changed) {
                    self.note = Some(Note::Changed);
                }
                match f.purpose {
                    Purpose::Play => self.show = Some((self.chosen_name(), songs)),
                    Purpose::Import => self.import(f, key, songs),
                    Purpose::Room => self.add = Some(songs),
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

    /// An item fetched for Import: written into the library on a thread,
    /// where the level the store keeps for it allows keeping it.
    fn import(&mut self, f: Fetching, key: ItemKey, songs: Vec<FetchedSong>) {
        let reach = super::fetched::item_reach(key).unwrap_or(Reach::Play);
        if let Some(p) = Self::refused_for(reach, Purpose::Import) {
            self.problem = Some(p);
            return;
        }
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

impl Finder {
    /// The songs played before: the chosen source's, then those of the
    /// sources with plays that are not on now (`offered` the sources on).
    fn played_sections(&self, offered: &[(String, Option<String>)]) -> Vec<PlayedSection> {
        let Some(plays) = &self.plays else {
            return Vec::new();
        };
        let chosen = self.chosen_id();
        let percent = ddi_ui::state::settings::Settings::cached().ddi_percent();
        let section = |id: &str, name: SourceName, state: PlayedState, chosen: bool| {
            let (rows, total) = plays.history().played_before(id, PLAYED_MAX);
            PlayedSection {
                id: id.to_string(),
                source: name,
                chosen,
                unlisted: total - rows.len(),
                rows: rows
                    .iter()
                    .map(|r| {
                        let gone = self.gone.contains(&(id.to_string(), r.origin.item.clone()));
                        played_view(r, if gone { PlayedState::Gone } else { state }, percent)
                    })
                    .collect(),
            }
        };
        let mut out = Vec::new();
        if let Some(id) = &chosen {
            let name = offered
                .iter()
                .find(|(i, _)| i == id)
                .and_then(|(_, n)| n.clone())
                .map_or(SourceName::ThisComputer, SourceName::Plugin);
            out.push(section(id, name, PlayedState::Ready, true));
        }
        let sources = plays.history().origin_sources();
        let installed = self.songs.sources.borrow();
        for id in sources {
            if offered.iter().any(|(i, _)| *i == id) {
                continue;
            }
            let plugin = installed
                .as_ref()
                .and_then(|s| s.host.plugins().find(|p| p.id() == id).cloned());
            let (name, state) = if id == this_computer::ID {
                (SourceName::ThisComputer, PlayedState::SourceOff)
            } else if let Some(p) = plugin {
                let language = ddi_l10n::current().tag;
                (
                    SourceName::Plugin(p.manifest.name.get(language).to_string()),
                    PlayedState::SourceOff,
                )
            } else {
                (SourceName::Plugin(id.clone()), PlayedState::NotInstalled)
            };
            out.push(section(&id, name, state, false));
        }
        out
    }
}

/// A song played before as the page's row.
fn played_view(
    r: &ddi_ui::state::history::PlayedBefore,
    state: PlayedState,
    percent: bool,
) -> PlayedView {
    use ddi_ui::state::history as h;
    let c = &r.origin.chart;
    let style: String = ddi_ui::screens::data::style_name(&c.style, &c.style).into();
    PlayedView {
        title: r.origin.title.clone(),
        artist: r.origin.artist.clone(),
        chart: ddi_ui::screens::prompt::chart_line(c.difficulty, c.meter, &style),
        best: r.best.as_ref().map(|b| {
            let grade = h::list_grade(&b.grade);
            let score = h::score_text_as(&b.score, percent);
            if grade.is_empty() {
                score
            } else {
                format!("{score} {grade}")
            }
        }),
        when: h::local_time(
            r.last_played,
            crate::play::replays::tz_minutes(r.last_played),
        ),
        state,
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
                reach: match self.reach_of(r) {
                    Reach::Play => ddi_ui::screens::find_songs::Reach::Play,
                    Reach::Rooms => ddi_ui::screens::find_songs::Reach::Rooms,
                    Reach::Keep => ddi_ui::screens::find_songs::Reach::Keep,
                },
                room: self.room_offer(r),
            })
            .collect();
        let busy = if let Some(i) = &self.importing {
            Some(Busy::Importing {
                title: i.title.clone(),
            })
        } else if let Some(d) = &self.describing {
            Some(Busy::Fetching {
                title: d.origin.title.clone(),
                step: Some(Step::Resolving),
                done: None,
                total: None,
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
            room: self.room.is_some(),
            played: if self.room.is_some() {
                Vec::new()
            } else {
                self.played_sections(&offered)
            },
        }
    }

    fn press(&mut self, press: FindPress, cx: &mut Cx) {
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
            FindPress::Import(i) if self.chosen_id().as_deref() == Some(this_computer::ID) => {
                self.import_in_place(i)
            }
            FindPress::Import(i) => self.fetch(i, Purpose::Import),
            FindPress::Played(i) => self.play_again(i),
            FindPress::AddToRoom(i) => self.add_to_room(i, cx),
            FindPress::Cancel => {
                if let Some((op, _)) = self.search.take() {
                    self.cancel(op);
                }
                if let Some(f) = self.fetching.take() {
                    self.cancel(f.op);
                }
                if let Some(d) = self.describing.take() {
                    self.cancel(d.op);
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
        // A play kept (or judged again): the songs played before.
        let generation = self.plays.as_ref().map_or(0, |p| p.generation());
        if generation != self.plays_seen {
            self.plays_seen = generation;
            self.changed();
        }
        // The field searched once it rests.
        if self.dirty {
            let now = cx.time();
            match self.typed_at {
                None => self.typed_at = Some(now),
                Some(at) if now - at >= REST_S => self.search_now(),
                Some(_) => {}
            }
        }
        // The room's list moved (an entry added, removed): "In the room".
        if let Some(room) = &self.room {
            let v = room.borrow().version();
            if v != self.room_seen {
                self.room_seen = v;
                self.changed();
            }
        }
        if let Some(songs) = self.add.take() {
            self.add_fetched(songs, cx);
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
        if let Some(d) = self.describing.take() {
            self.cancel(d.op);
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
    let model = Rc::new(RefCell::new(Finder::new(env.clone(), songs.clone(), sound, None)));
    ddi_ui::screens::find_songs::screen(env.clone(), model)
}

/// The Find songs page over `songs`' sources opened from online room
/// `room` (its More, stage 3): what it finds added to the room's list.
pub fn room_screen(
    env: &Env,
    songs: &Rc<Songs>,
    sound: Option<SharedPlayer>,
    room: SharedRoom,
) -> ddi_ui::screens::find_songs::FindScreen {
    let model = Rc::new(RefCell::new(Finder::new(
        env.clone(),
        songs.clone(),
        sound,
        Some(room),
    )));
    ddi_ui::screens::find_songs::screen(env.clone(), model)
}
