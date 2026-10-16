//! Where the song list was (`docs/plans/ui-core.md`, P12): the open pack,
//! the last chart started, the scroll, the search. Kept for the session
//! under `ddi.menu.v1` ([`ddi_platform::storage::Area::Session`]: the
//! web's `sessionStorage`, per tab, surviving the play page and a reload);
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
    /// Song and chart index of the last chart started.
    pub chart: Option<(String, usize)>,
    /// How far the list was scrolled (the web page's `scrollY`).
    pub scroll: f64,
    /// What the search field held, and the text the search shown was run
    /// on (short text is not searched until asked).
    pub search: String,
    pub searched: String,
    /// Song select (step 6.2): the song shown, the style (its id) and the
    /// packs closed (by name; `None` before the first visit).
    pub song: Option<String>,
    pub style: Option<String>,
    pub closed: Option<Vec<String>>,
}

impl MenuPlace {
    pub fn load() -> MenuPlace {
        storage::load(Area::Session, KEY).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = storage::save(Area::Session, KEY, self);
    }
}

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
