//! Small values the game keeps between runs (`docs/plans/ui-core.md`,
//! P12): settings, the song list's place. A shell installs a [`Store`] at
//! boot (the web: `localStorage` and `sessionStorage`, under the keys
//! the game has always used; desktop: files) and a clock; everything else
//! reads and writes through [`load`] and [`save`], as JSON, which is how
//! the web has stored them all along (gloo's `Storage`). Until a store is
//! installed values live in memory, which is also what tests use.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// How long a value lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Area {
    /// Until removed (`localStorage`; a file).
    Local,
    /// For the session (`sessionStorage`: the tab, a reload included;
    /// desktop: the process).
    Session,
}

/// Where values are kept. Storage can be unavailable (a private window,
/// blocked site data): reads give `None` and writes fail quietly.
pub trait Store {
    fn get(&self, area: Area, key: &str) -> Option<String>;
    fn set(&self, area: Area, key: &str, value: &str) -> Result<(), String>;
    fn remove(&self, area: Area, key: &str);
}

/// Values in memory, gone with the process.
#[derive(Default)]
pub struct MemoryStore {
    values: RefCell<HashMap<(Area, String), String>>,
}

impl Store for MemoryStore {
    fn get(&self, area: Area, key: &str) -> Option<String> {
        self.values.borrow().get(&(area, key.to_string())).cloned()
    }

    fn set(&self, area: Area, key: &str, value: &str) -> Result<(), String> {
        self.values
            .borrow_mut()
            .insert((area, key.to_string()), value.to_string());
        Ok(())
    }

    fn remove(&self, area: Area, key: &str) {
        self.values.borrow_mut().remove(&(area, key.to_string()));
    }
}

thread_local! {
    static STORE: RefCell<Rc<dyn Store>> = RefCell::new(Rc::new(MemoryStore::default()));
    static CLOCK: Cell<Option<fn() -> f64>> = const { Cell::new(None) };
}

/// Makes `store` the one every load and save goes to.
pub fn install(store: Rc<dyn Store>) {
    STORE.with(|s| *s.borrow_mut() = store);
}

/// Makes `clock` (milliseconds since the Unix epoch: `Date.now()`) what
/// [`now_ms`] reads.
pub fn install_clock(clock: fn() -> f64) {
    CLOCK.with(|c| c.set(Some(clock)));
}

/// Milliseconds since the Unix epoch, by the installed clock (0 without
/// one).
pub fn now_ms() -> f64 {
    CLOCK.with(|c| c.get()).map_or(0.0, |f| f())
}

fn store() -> Rc<dyn Store> {
    STORE.with(|s| s.borrow().clone())
}

pub fn get(area: Area, key: &str) -> Option<String> {
    store().get(area, key)
}

pub fn set(area: Area, key: &str, value: &str) -> Result<(), String> {
    store().set(area, key, value)
}

pub fn remove(area: Area, key: &str) {
    store().remove(area, key)
}

/// The value under `key`, as JSON; `None` when absent or unreadable.
pub fn load<T: DeserializeOwned>(area: Area, key: &str) -> Option<T> {
    serde_json::from_str(&get(area, key)?).ok()
}

/// Stores `value` under `key` as JSON.
pub fn save<T: Serialize>(area: Area, key: &str, value: &T) -> Result<(), String> {
    let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
    set(area, key, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip_through_the_installed_store() {
        let mem = Rc::new(MemoryStore::default());
        install(mem.clone());
        assert_eq!(load::<Vec<u32>>(Area::Local, "k"), None);
        save(Area::Local, "k", &vec![1u32, 2]).unwrap();
        assert_eq!(load::<Vec<u32>>(Area::Local, "k"), Some(vec![1, 2]));
        // Areas are apart; unreadable JSON is absent.
        assert_eq!(get(Area::Session, "k"), None);
        set(Area::Session, "k", "{not json").unwrap();
        assert_eq!(load::<Vec<u32>>(Area::Session, "k"), None);
        remove(Area::Local, "k");
        assert_eq!(mem.get(Area::Local, "k"), None);
        assert_eq!(now_ms(), 0.0);
        install_clock(|| 42.0);
        assert_eq!(now_ms(), 42.0);
    }
}
