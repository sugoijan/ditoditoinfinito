//! Where the song list was (`docs/plans/ui-core.md`, P12): the open pack,
//! the song and chart, the scroll, the search. Browsing is kept between runs
//! under `ddi.menu.v1` in local storage; search is kept only for the session
//! (the web's `sessionStorage`, per tab, surviving a play and a reload);
//! storage can be unavailable, and the list then starts fresh.

use ddi_platform::storage::{self, Area};
use serde::{Deserialize, Serialize};

/// Where the place is kept.
pub const KEY: &str = "ddi.menu.v1";

/// Where the song list was.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MenuPlace {
    /// Whether `open` was ever set (false before the first visit).
    pub known: bool,
    /// Key of the open pack ([`pack_key`]), `None` when all are closed.
    pub open: Option<String>,
    /// Song and chart index last selected.
    pub chart: Option<(String, usize)>,
    /// How far the list was scrolled (the web page's `scrollY`).
    pub scroll: f64,
    /// What the search field held, and the text the search shown was run
    /// on (short text is not searched until asked).
    pub search: String,
    pub searched: String,
    /// Song select (step 6.2): the song shown, the style (its id) and the
    /// packs closed (by name, the bundled songs by [`CLOSED_BUNDLED`];
    /// `None` before the first visit).
    pub song: Option<String>,
    pub style: Option<String>,
    pub closed: Option<Vec<String>>,
}

impl MenuPlace {
    /// Apply a committed selection from either shell.
    pub fn remember(&mut self, place: &crate::screens::select::SelectPlace) {
        let open = place.open.clone();
        if place.song.is_some() || self.open != open {
            self.song = place.song.clone();
            self.chart = place
                .song
                .as_ref()
                .zip(place.chart)
                .map(|(s, c)| (s.clone(), c));
        }
        self.known = true;
        self.open = open;
        self.style = Some(place.style.clone());
        self.search = place.search.clone();
        self.searched = place.search.clone();
        self.closed = Some(
            place
                .closed
                .iter()
                .map(|p| p.as_deref().unwrap_or(CLOSED_BUNDLED).to_string())
                .collect(),
        );
    }

    /// Whether the pack named `name` (`None`: the bundled songs) was
    /// closed.
    pub fn is_closed(&self, name: Option<&str>) -> bool {
        let name = name.unwrap_or(CLOSED_BUNDLED);
        self.closed
            .as_ref()
            .is_some_and(|c| c.iter().any(|n| n == name))
    }

    pub fn load() -> MenuPlace {
        storage::load(Area::Session, KEY)
            .or_else(|| storage::load(Area::Local, KEY))
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = storage::save(Area::Session, KEY, self);
        // Searching must not replace the position to resume in a new run.
        if self.search.trim().is_empty() {
            let _ = storage::save(Area::Local, KEY, self);
        }
    }
}

/// How the closed packs kept by earlier runs name the bundled songs (a
/// saved value: a pack of this name shares their closing).
pub const CLOSED_BUNDLED: &str = "Bundled songs";

/// Key of a group of the list: the bundled songs or an imported pack
/// (packs are told apart ignoring case).
pub fn pack_key(pack: Option<&str>) -> String {
    match pack {
        None => "bundled".into(),
        Some(p) => format!("pack:{}", p.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn browsing_survives_a_new_session_but_search_does_not_replace_it() {
        storage::install(Rc::new(storage::MemoryStore::default()));
        let mut place = MenuPlace::default();
        let mut selection = crate::screens::select::SelectPlace {
            song: Some("u-song".into()),
            chart: Some(2),
            style: "dance-single".into(),
            open: Some(pack_key(Some("My Pack"))),
            closed: vec![None],
            ..Default::default()
        };
        place.remember(&selection);
        place.save();
        selection.search = "another song".into();
        selection.song = Some("other".into());
        place.remember(&selection);
        place.save();
        assert_eq!(
            MenuPlace::load().search,
            "another song",
            "same tab keeps its search"
        );
        storage::remove(Area::Session, KEY);
        let resumed = MenuPlace::load();
        assert_eq!(resumed.song.as_deref(), Some("u-song"));
        assert_eq!(resumed.chart, Some(("u-song".into(), 2)));
        assert_eq!(resumed.open.as_deref(), Some("pack:my pack"));
        assert!(resumed.search.is_empty());
        assert_eq!(resumed.closed, Some(vec![CLOSED_BUNDLED.into()]));
        assert!(resumed.is_closed(None) && !resumed.is_closed(Some("My Pack")));
    }

    #[test]
    fn headers_keep_the_last_song_only_within_the_open_pack() {
        let mut place = MenuPlace::default();
        let mut selection = crate::screens::select::SelectPlace {
            open: Some(pack_key(Some("Pack"))),
            song: Some("u-song".into()),
            chart: Some(3),
            ..Default::default()
        };
        place.remember(&selection);
        selection.song = None;
        place.remember(&selection);
        assert_eq!(place.song.as_deref(), Some("u-song"));
        selection.open = Some(pack_key(Some("Other")));
        place.remember(&selection);
        assert!(place.song.is_none());
        assert!(place.chart.is_none());
    }

    #[test]
    fn places_keep_their_stored_shape() {
        storage::install(Rc::new(storage::MemoryStore::default()));
        assert_eq!(MenuPlace::load(), MenuPlace::default());
        // As the web stored it, a field missing.
        storage::set(
            Area::Session,
            KEY,
            r#"{"known":true,"open":"pack:ddr","chart":["u-1",2],"scroll":120.5,"search":"sak"}"#,
        )
        .unwrap();
        let p = MenuPlace::load();
        assert_eq!(p.open.as_deref(), Some("pack:ddr"));
        assert_eq!(p.chart, Some(("u-1".into(), 2)));
        assert_eq!(p.searched, "");
        p.save();
        assert_eq!(MenuPlace::load(), p);
        assert_eq!(pack_key(Some("DDR")), "pack:ddr");
        assert_eq!(pack_key(None), "bundled");
    }
}
