//! Songs fetched from a source and added to an online room without
//! importing them (`docs/plans/source-plugins.md`, stage 3): what this
//! device keeps of them for the room.
//!
//! A fetched song goes into the room's list as any song of the library
//! does (`ddi_ui::state::room::RoomClient::add_song`, the same checks and
//! words, its `song_key` naming it, its upload read through
//! `crate::play::load::Files`): nothing source-specific reaches the room,
//! the server or the other devices, which load it from the room's share.
//! This device keeps the song's item in the store
//! (`ddi_source_host::store::hold`: in memory, or moved to the cache
//! folder under pressure, never dropped) for as long as the room needs
//! it here:
//!
//! - from Add to room until the room lists it (at most [`LISTING_WAIT`]:
//!   a song the room never lists, refused after the press, is let go);
//! - while the room lists it: its upload, and this device's own plays of
//!   it, read from the copy kept (as a library song is read from the
//!   library), never downloaded from its own upload;
//! - let go when its entry goes from the list, when the room is left,
//!   closed or this device removed from it, and when the library has the
//!   same song (imported since), whose copy the room's client then reads.
//!
//! A song the room lists whose copy on this device is a fetched one is
//! kept however it was added (Find songs' Add to room, or My songs'
//! "Fetched this session": [`sync`] finds it).

use std::cell::RefCell;
use std::rc::Weak;
use std::time::{Duration, Instant};

use ddi_room::{ContentStatus, LinkState, SongKey};
use ddi_ui::screens::data::SongEntry;
use ddi_ui::state::room::{RoomClient, SharedRoom, song_key};

use super::fetched::{self, Held};

/// How long a song added waits for the room to list it before its item is
/// let go (the room refused it after the press: another device filled the
/// list meanwhile).
pub const LISTING_WAIT: Duration = Duration::from_secs(30);

/// A fetched song kept for the room.
struct Kept {
    /// `src:<key>:<n>`.
    id: String,
    /// The room list's key for it.
    key: String,
    held: Held,
    /// The room listed it once.
    listed: bool,
    since: Instant,
}

#[derive(Default)]
struct State {
    /// The online room this device's songs are kept for: the one its song
    /// select shows ([`watch`]).
    room: Option<Weak<RefCell<RoomClient>>>,
    kept: Vec<Kept>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Songs are kept for online room `room` from now on (its song select is
/// made): those kept for another let go.
pub fn watch(room: &SharedRoom) {
    STATE.with_borrow_mut(|s| {
        let same = s
            .room
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|r| std::rc::Rc::ptr_eq(&r, room));
        if !same {
            s.kept.clear();
            s.room = Some(std::rc::Rc::downgrade(room));
        }
    });
}

/// The room left (or forgotten): every song kept for it let go.
pub fn release_all() {
    STATE.with_borrow_mut(|s| {
        s.kept.clear();
        s.room = None;
    });
}

/// Fetched song `song` (`src:` id) added to the room: its item kept
/// until the room lists it, then while it does. Whether it is kept.
pub fn keep(song: &SongEntry) -> bool {
    let Some(key) = song_key(song) else {
        return false;
    };
    STATE.with_borrow_mut(|s| keep_in(s, &song.id, key, false))
}

fn keep_in(s: &mut State, id: &str, key: String, listed: bool) -> bool {
    if s.kept.iter().any(|k| k.id == id) {
        return true;
    }
    let Some(held) = fetched::hold(id) else {
        return false;
    };
    s.kept.push(Kept {
        id: id.to_string(),
        key,
        held,
        listed,
        since: Instant::now(),
    });
    true
}

/// The fetched songs kept for the room now, by id.
pub fn kept() -> Vec<String> {
    STATE.with_borrow(|s| s.kept.iter().map(|k| k.id.clone()).collect())
}

/// Whether the room's link ended for good for this device (left, closed,
/// removed): what a sign-in or "Use this device" mends keeps its songs.
fn gone(state: LinkState) -> bool {
    matches!(
        state,
        LinkState::Left | LinkState::Closed | LinkState::Kicked | LinkState::NotAdmitted
    )
}

/// The songs kept checked against the room (each loop, `crate::songs`):
/// a listed song whose copy here is a fetched one kept, a song let go
/// once its entry went, it was never listed in [`LISTING_WAIT`], the
/// library has it (imported: its fetched copy let go from the store too,
/// once no upload of it runs), or the room is gone. Whether anything
/// changed.
pub fn sync() -> bool {
    STATE.with_borrow_mut(|s| {
        let Some(weak) = s.room.clone() else {
            return false;
        };
        let Some(room) = weak.upgrade() else {
            let had = !s.kept.is_empty();
            s.kept.clear();
            s.room = None;
            return had;
        };
        // A room borrowed elsewhere this moment: the next loop.
        let Ok(r) = room.try_borrow() else {
            return false;
        };
        if r.has_left() || gone(r.link_state()) {
            let had = !s.kept.is_empty();
            s.kept.clear();
            return had;
        }
        let before = s.kept.len();
        // Listed songs whose copy here is a fetched one (added from My
        // songs' fetched pack, or by an earlier page).
        for e in r.room_songs() {
            if let Some(song) = r.library_song(e)
                && fetched::is_fetched(&song.id)
            {
                keep_in(s, &song.id.clone(), e.key.as_str().to_string(), true);
            }
        }
        let mut imported = Vec::new();
        s.kept.retain_mut(|k| {
            let listed = r.state().songs.by_key(&SongKey::new(k.key.clone()));
            match listed {
                Some(e) => {
                    k.listed = true;
                    // The library's copy now (imported): the room's client
                    // reads it, once this device's upload is done with
                    // the fetched one.
                    let library = r
                        .library_song(e)
                        .is_some_and(|song| !fetched::is_fetched(&song.id));
                    if library && e.status != ContentStatus::Pending {
                        imported.push(k.held.key());
                        return false;
                    }
                    true
                }
                None => !k.listed && k.since.elapsed() < LISTING_WAIT,
            }
        });
        let changed = s.kept.len() != before;
        drop(r);
        // An imported song's fetched copy let go at once (as an import
        // lets it go when nothing else holds it).
        if !imported.is_empty()
            && let Some(store) = fetched::store()
            && let Ok(mut st) = store.lock()
        {
            for key in imported {
                st.remove(ddi_source_host::ItemKey(key));
            }
        }
        changed
    })
}
