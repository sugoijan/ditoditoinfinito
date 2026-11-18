//! The player's favorite songs (`docs/plans/song-navigation.md`,
//! "Favorites"): a song is a favorite or not, whatever its charts.
//!
//! A favorite is the song's library id, which both shells give a song
//! alike: a bundled song's own id, an imported or read-in-place one
//! `ddi_library::pack::song_id` of its pack's name and its folder's, the
//! id the song's own offset and the list's place keep too. So it outlives
//! a restart, a library read again and a pack removed and imported again
//! under the same name; a pack or song folder renamed is a new song. A
//! share's songs (`shared:<id>:<n>`) are never favorites: nothing is kept
//! of a share.
//!
//! Kept under [`KEY`] in the device's local storage (the web's
//! `localStorage`, desktop's `storage.json`), apart from the settings: a
//! list of ids in the order they were added (about 21 bytes an imported
//! song, some 21 kB at 1,000). Read once and kept in memory (again only
//! for another store, [`ddi_platform::storage::generation`]), written at
//! each change. A favorite whose song is not in the library now is kept,
//! and shows again with its song.

use std::cell::RefCell;
use std::collections::HashSet;

use ddi_platform::storage::{self, Area};
use serde::{Deserialize, Serialize};

/// Where the favorites are kept.
pub const KEY: &str = "ddi.favorites.v1";

/// The favorites as stored.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    /// Song ids, oldest first, each once.
    songs: Vec<String>,
}

/// The favorites in memory: as stored, and as a set.
#[derive(Default)]
struct Kept {
    stored: Stored,
    set: HashSet<String>,
}

impl Kept {
    fn load() -> Kept {
        let mut stored: Stored = storage::load(Area::Local, KEY).unwrap_or_default();
        // Each once (a hand-edited value).
        let mut set = HashSet::new();
        stored
            .songs
            .retain(|id| !id.is_empty() && set.insert(id.clone()));
        Kept { stored, set }
    }
}

thread_local! {
    /// The favorites read, and the store generation they were read from.
    static KEPT: RefCell<Option<(u64, Kept)>> = const { RefCell::new(None) };
}

/// `f` over the favorites as kept now (read first where they were not).
fn with<T>(f: impl FnOnce(&mut Kept) -> T) -> T {
    let generation = storage::generation();
    KEPT.with_borrow_mut(|kept| {
        if kept.as_ref().is_none_or(|(g, _)| *g != generation) {
            *kept = Some((generation, Kept::load()));
        }
        let (_, kept) = kept.as_mut().expect("loaded");
        f(kept)
    })
}

/// Whether the song `id` is a favorite.
pub fn is_favorite(id: &str) -> bool {
    with(|k| k.set.contains(id))
}

/// Every favorite's id, oldest first (whether its song is in the library
/// or not).
pub fn all() -> Vec<String> {
    with(|k| k.stored.songs.clone())
}

/// Makes the song `id` a favorite (`on`) or not, kept at once; whether
/// that changed anything.
pub fn set_favorite(id: &str, on: bool) -> bool {
    if id.is_empty() || is_shared(id) {
        return false;
    }
    with(|k| {
        let changed = if on {
            k.set.insert(id.to_string())
        } else {
            k.set.remove(id)
        };
        if !changed {
            return false;
        }
        if on {
            k.stored.songs.push(id.to_string());
        } else {
            k.stored.songs.retain(|s| s != id);
        }
        let _ = storage::save(Area::Local, KEY, &k.stored);
        true
    })
}

/// A share's song (`shared:<id>:<n>`): never a favorite.
fn is_shared(id: &str) -> bool {
    id.starts_with("shared:")
}

/// Turns the song `id` a favorite or not; whether it is one now.
pub fn toggle(id: &str) -> bool {
    let on = !is_favorite(id);
    set_favorite(id, on);
    is_favorite(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn favorites_are_kept_in_their_own_key_and_read_back() {
        let store = Rc::new(storage::MemoryStore::default());
        storage::install(store.clone());
        assert!(all().is_empty());
        assert!(!is_favorite("u-0123456789abcdef"));
        assert!(toggle("u-0123456789abcdef"));
        assert!(set_favorite("bundled-song", true));
        // Once.
        assert!(!set_favorite("bundled-song", true));
        assert_eq!(
            storage::get(Area::Local, KEY).as_deref(),
            Some(r#"{"songs":["u-0123456789abcdef","bundled-song"]}"#)
        );
        // Never in the settings.
        assert!(storage::get(Area::Local, crate::state::settings::KEY).is_none());
        // Read back from what was stored (the store installed again, as
        // a new run).
        storage::install(store.clone());
        assert_eq!(all(), ["u-0123456789abcdef", "bundled-song"]);
        assert!(!toggle("u-0123456789abcdef"));
        assert_eq!(all(), ["bundled-song"]);
        storage::install(store);
        assert_eq!(all(), ["bundled-song"]);
    }

    #[test]
    fn a_shares_songs_are_never_favorites() {
        storage::install(Rc::new(storage::MemoryStore::default()));
        let id = ddi_library::share::song_id("abcdefghij", 0);
        assert!(!set_favorite(&id, true));
        assert!(!toggle(&id));
        assert!(!set_favorite("", true));
        assert!(storage::get(Area::Local, KEY).is_none());
    }

    #[test]
    fn a_stored_list_is_read_whatever_was_written_into_it() {
        let store = Rc::new(storage::MemoryStore::default());
        storage::install(store.clone());
        storage::set(Area::Local, KEY, r#"{"songs":["a","a","","b"],"later":1}"#).unwrap();
        storage::install(store.clone());
        assert_eq!(all(), ["a", "b"]);
        storage::set(Area::Local, KEY, "not json").unwrap();
        storage::install(store);
        assert!(all().is_empty());
        // The size at 1,000 imported songs' ids.
        let ids: Vec<String> = (0..1000u64)
            .map(|i| format!("u-{:016x}", i * 7919))
            .collect();
        let json = serde_json::to_string(&Stored { songs: ids }).unwrap();
        assert!(json.len() < 22_000, "{}", json.len());
    }
}
