//! Values kept between runs on desktop (P12): one JSON file of keys and
//! values (the same keys and JSON the web keeps in `localStorage`) in the
//! user's configuration folder; the session's in memory.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use ddi_platform::storage::{self, Area, MemoryStore, Store};
use ddi_ui::state::settings::{self, Settings};

/// `storage.json` in the configuration folder ([`crate::paths`]:
/// `$DDI_HOME/config` for a run that is not the player's).
pub fn default_path() -> Option<PathBuf> {
    Some(crate::paths::config_dir()?.join("storage.json"))
}

/// Local values in a file, written whole on every change.
pub struct FileStore {
    path: PathBuf,
    values: RefCell<BTreeMap<String, String>>,
    session: MemoryStore,
}

impl FileStore {
    /// The values in `path`. A file that cannot be read as values is
    /// kept aside (`storage.json.bad`, numbered if one is there) and
    /// reported, never overwritten: the store starts empty.
    pub fn open(path: PathBuf) -> FileStore {
        let values = match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(v) => v,
                Err(e) => {
                    let aside = set_aside(&path);
                    eprintln!(
                        "{} is unreadable ({e}); kept as {}, starting afresh",
                        path.display(),
                        aside.map_or("nothing".into(), |p| p.display().to_string())
                    );
                    BTreeMap::new()
                }
            },
            Err(_) => BTreeMap::new(),
        };
        FileStore {
            path,
            values: RefCell::new(values),
            session: MemoryStore::default(),
        }
    }

    fn write(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json =
            serde_json::to_string_pretty(&*self.values.borrow()).map_err(|e| e.to_string())?;
        // Whole or not at all.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }
}

/// Moves an unreadable `path` to `<path>.bad`, or `<path>.bad.<n>` when
/// that is taken; where it went.
fn set_aside(path: &std::path::Path) -> Option<PathBuf> {
    let base = path.with_extension("json.bad");
    let target = std::iter::once(base.clone())
        .chain((1..1000).map(|n| PathBuf::from(format!("{}.{n}", base.display()))))
        .find(|p| !p.exists())?;
    std::fs::rename(path, &target).ok()?;
    Some(target)
}

impl Store for FileStore {
    fn get(&self, area: Area, key: &str) -> Option<String> {
        match area {
            Area::Local => self.values.borrow().get(key).cloned(),
            Area::Session => self.session.get(area, key),
        }
    }

    fn set(&self, area: Area, key: &str, value: &str) -> Result<(), String> {
        match area {
            Area::Local => {
                self.values.borrow_mut().insert(key.into(), value.into());
                self.write()
            }
            Area::Session => self.session.set(area, key, value),
        }
    }

    fn remove(&self, area: Area, key: &str) {
        match area {
            Area::Local => {
                if self.values.borrow_mut().remove(key).is_some() {
                    let _ = self.write();
                }
            }
            Area::Session => self.session.remove(area, key),
        }
    }
}

thread_local! {
    /// The settings last loaded and the stored text they came from.
    static SETTINGS: RefCell<Option<(Option<String>, Settings)>> = const { RefCell::new(None) };
}

/// [`Settings::load`], read again only when the stored text changed
/// (for callers on every turn of the event loop).
pub fn settings_cached() -> Settings {
    let raw = storage::get(Area::Local, settings::KEY);
    let hit = SETTINGS.with_borrow(|c| match c {
        Some((text, s)) if *text == raw => Some(s.clone()),
        _ => None,
    });
    if let Some(s) = hit {
        return s;
    }
    let s = Settings::load();
    // Loading may have stored a sanitized copy.
    let raw = storage::get(Area::Local, settings::KEY);
    SETTINGS.set(Some((raw, s.clone())));
    s
}

/// Installs the file store (or memory, with no configuration folder) and
/// the system clock.
pub fn install() {
    match default_path() {
        Some(path) => storage::install(Rc::new(FileStore::open(path))),
        None => storage::install(Rc::new(MemoryStore::default())),
    }
    storage::install_clock(crate::play::epoch_ms);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("ddi-store-{}", std::process::id()));
        let path = dir.join("storage.json");
        let s = FileStore::open(path.clone());
        s.set(Area::Local, "ddi.settings.v1", r#"{"speed":3}"#)
            .unwrap();
        s.set(Area::Session, "ddi.menu.v1", "{}").unwrap();
        let again = FileStore::open(path);
        assert_eq!(
            again.get(Area::Local, "ddi.settings.v1").as_deref(),
            Some(r#"{"speed":3}"#)
        );
        assert_eq!(again.get(Area::Session, "ddi.menu.v1"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn menu_position_survives_reopening_the_desktop_store() {
        use ddi_ui::state::place::MenuPlace;
        let path = std::env::temp_dir().join(format!("ddi-menu-{}.json", std::process::id()));
        storage::install(Rc::new(FileStore::open(path.clone())));
        let place = MenuPlace {
            song: Some("u-last-song".into()),
            chart: Some(("u-last-song".into(), 4)),
            closed: Some(vec!["Bundled songs".into()]),
            ..Default::default()
        };
        place.save();
        storage::install(Rc::new(FileStore::open(path.clone())));
        assert_eq!(MenuPlace::load(), place);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_unreadable_file_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("ddi-store-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("storage.json");
        for round in 0..2 {
            std::fs::write(&path, "{not json").unwrap();
            let s = FileStore::open(path.clone());
            assert_eq!(s.get(Area::Local, "x"), None);
            assert!(!path.exists());
            let aside = if round == 0 {
                dir.join("storage.json.bad")
            } else {
                dir.join("storage.json.bad.1")
            };
            assert_eq!(std::fs::read_to_string(aside).unwrap(), "{not json");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
