//! The web's [`ddi_platform::storage::Store`]: `localStorage` and
//! `sessionStorage`, read and written as strings under the keys the game
//! has always used (settings in `ddi.settings.v1`, the song list's place in
//! `ddi.menu.v1`), so what earlier versions stored still loads. Either can
//! be missing or throw (a private window, blocked site data): reads are
//! then empty and writes fail quietly.

use std::rc::Rc;

use ddi_platform::storage::{self, Area, Store};

struct WebStore;

fn area(area: Area) -> Option<web_sys::Storage> {
    let w = web_sys::window()?;
    match area {
        Area::Local => w.local_storage().ok().flatten(),
        Area::Session => w.session_storage().ok().flatten(),
    }
}

impl Store for WebStore {
    fn get(&self, a: Area, key: &str) -> Option<String> {
        area(a)?.get_item(key).ok().flatten()
    }

    fn set(&self, a: Area, key: &str, value: &str) -> Result<(), String> {
        area(a)
            .ok_or("no storage")?
            .set_item(key, value)
            .map_err(|e| super::js_err("storage", e))
    }

    fn remove(&self, a: Area, key: &str) {
        if let Some(s) = area(a) {
            let _ = s.remove_item(key);
        }
    }
}

/// Installs the browser's storage and `Date.now()`: first thing at boot.
pub(crate) fn install() {
    storage::install(Rc::new(WebStore));
    storage::install_clock(js_sys::Date::now);
}
