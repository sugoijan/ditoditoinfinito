//! Where the song list was (`docs/plans/ui-core.md`, P12): the open pack,
//! the song and chart, the scroll, the search. Browsing is kept between runs
//! under `ddi.menu.v1` in local storage; search is kept only for the session
//! (the web's `sessionStorage`, per tab, surviving a play and a reload);
//! storage can be unavailable, and the list then starts fresh.

use ddi_platform::storage::{self, Area};
use serde::{Deserialize, Serialize};

/// Where the place is kept.
pub const KEY: &str = "ddi.menu.v1";

/// Where a multiplayer room's song select was (the picker's own list,
/// `docs/plans/song-navigation.md`, "In a room"): its own place, kept for
/// the session only (the web's `sessionStorage`, surviving a stage's
/// routes and a reload; desktop's run), never singleplayer's.
pub const ROOM_KEY: &str = "ddi.room.menu.v1";

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

    /// A room's song select's place: where the last room's was this
    /// session, else where singleplayer's is (the first room starts there).
    pub fn load_room() -> MenuPlace {
        storage::load(Area::Session, ROOM_KEY).unwrap_or_else(MenuPlace::load)
    }

    /// Keeps a room's place for the session ([`ROOM_KEY`]).
    pub fn save_room(&self) {
        let _ = storage::save(Area::Session, ROOM_KEY, self);
    }
}

/// A room's song select's committed `report` (the picker's own list):
/// remembered in `place` and kept as the room's, never singleplayer's place
/// or the settings' style.
pub fn apply_room(report: &crate::screens::select::SelectPlace, place: &mut MenuPlace) {
    let before = place.clone();
    place.remember(report);
    if *place != before {
        place.save_room();
    }
}

/// A room's pick (`song`'s chart `chart`): the room's place keeps it
/// selected, as singleplayer's play keeps the song played, whatever the
/// screen had told before the press.
pub fn room_picked(song: &str, chart: usize) {
    let mut place = MenuPlace::load_room();
    place.known = true;
    place.song = Some(song.to_string());
    place.chart = Some((song.to_string(), chart));
    place.save_room();
}

/// Song select's committed `report` in either shell: remembered in `place`,
/// its style kept as the settings' (the play's bindings and Options' field
/// preview follow it), each saved only when it changed.
pub fn apply(
    report: &crate::screens::select::SelectPlace,
    place: &mut MenuPlace,
    settings: &mut crate::state::settings::Settings,
) {
    let before = place.clone();
    place.remember(report);
    if *place != before {
        place.save();
    }
    if !report.style.is_empty() && settings.style != report.style {
        settings.style = report.style.clone();
        settings.save();
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

/// Whether `a` and `b` name the same pack: equal ignoring case, as song
/// select groups packs (`select_view::select_data`, by `str::to_lowercase`),
/// so "Ünïcode" and "üNÏCODE" are one pack here too.
pub fn same_pack(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
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
    fn a_committed_place_keeps_its_style_as_the_settings_and_saves_changes_only() {
        use crate::state::settings::{KEY as SETTINGS, Settings};
        storage::install(Rc::new(storage::MemoryStore::default()));
        let (mut place, mut settings) = (MenuPlace::default(), Settings::default());
        let report = crate::screens::select::SelectPlace {
            song: Some("u-song".into()),
            chart: Some(1),
            style: "dance-double".into(),
            ..Default::default()
        };
        apply(&report, &mut place, &mut settings);
        assert_eq!(settings.style, "dance-double");
        assert_eq!(MenuPlace::load(), place);
        assert!(storage::get(Area::Local, SETTINGS).is_some());
        // The same place again: nothing is written.
        storage::remove(Area::Session, KEY);
        storage::remove(Area::Local, KEY);
        storage::remove(Area::Local, SETTINGS);
        apply(&report, &mut place, &mut settings);
        assert!(storage::get(Area::Session, KEY).is_none());
        assert!(storage::get(Area::Local, SETTINGS).is_none());
        // No style reported: the settings' stays.
        apply(
            &crate::screens::select::SelectPlace::default(),
            &mut place,
            &mut settings,
        );
        assert_eq!(settings.style, "dance-double");
    }

    /// A room's song select has a place of its own for the session: the
    /// first room starts at singleplayer's, a room's moves and picks never
    /// move singleplayer's or the settings' style, and the next room (the
    /// web's room page after a stage, a reload) starts where it was.
    #[test]
    fn a_rooms_place_is_its_own_for_the_session() {
        use crate::state::settings::{KEY as SETTINGS, Settings};
        storage::install(Rc::new(storage::MemoryStore::default()));
        let (mut solo, mut settings) = (MenuPlace::default(), Settings::default());
        let report = crate::screens::select::SelectPlace {
            song: Some("solo".into()),
            chart: Some(1),
            style: "dance-double".into(),
            open: Some(pack_key(Some("Solo Pack"))),
            ..Default::default()
        };
        apply(&report, &mut solo, &mut settings);
        storage::remove(Area::Local, SETTINGS);
        // The first room: singleplayer's place.
        let mut room = MenuPlace::load_room();
        assert_eq!(room, solo);
        let moved = crate::screens::select::SelectPlace {
            song: Some("room-song".into()),
            chart: Some(2),
            style: "dance-single".into(),
            open: Some(pack_key(Some("Room Pack"))),
            search: "room".into(),
            closed: vec![None],
        };
        apply_room(&moved, &mut room);
        room_picked("picked", 3);
        assert_eq!(MenuPlace::load(), solo, "singleplayer's stays");
        assert!(storage::get(Area::Local, SETTINGS).is_none(), "the style too");
        assert!(storage::get(Area::Local, ROOM_KEY).is_none(), "the session's");
        let again = MenuPlace::load_room();
        assert_eq!(again.song.as_deref(), Some("picked"));
        assert_eq!(again.chart, Some(("picked".into(), 3)));
        assert_eq!(again.open.as_deref(), Some("pack:room pack"));
        assert_eq!((again.search.as_str(), again.style.as_deref()), ("room", Some("dance-single")));
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
