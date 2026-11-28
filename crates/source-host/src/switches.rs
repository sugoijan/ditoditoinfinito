//! Which plugins are switched on (`docs/plans/source-plugins.md`,
//! "Installing": a plugin found in the folder is off until switched on;
//! one switch is the whole consent). Kept apart from the settings, as a
//! file of its own in the configuration folder, `plugins.json`:
//!
//! ```json
//! {"on": ["example-songs"]}
//! ```
//!
//! Desktop's Options → Plugins writes it (stage 4), as tests and the
//! person may; the host reads it at the window's start and is told of a
//! change (`Host::switch`). An id listed for a plugin no longer
//! installed is harmless.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The switches' file name, in the configuration folder.
pub const FILE: &str = "plugins.json";

/// The file's size, at most.
const MAX_BYTES: u64 = 64 << 10;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Switches {
    /// The plugins switched on, by id.
    #[serde(default)]
    pub on: BTreeSet<String>,
}

impl Switches {
    /// The switches in `path`: none (every plugin off) where it is
    /// missing, unreadable or not this file.
    pub fn read(path: &Path) -> Switches {
        let Ok(file) = std::fs::File::open(path) else {
            return Switches::default();
        };
        let mut bytes = Vec::new();
        if file.take(MAX_BYTES + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_BYTES
        {
            return Switches::default();
        }
        let mut s: Switches = serde_json::from_slice(&bytes).unwrap_or_default();
        s.on.retain(|id| ddi_source_protocol::manifest::is_plugin_id(id));
        s
    }

    /// Switches `id` on or off in the file at `path` (made where missing),
    /// written whole.
    pub fn set(path: &Path, id: &str, on: bool) -> std::io::Result<()> {
        let mut s = Switches::read(path);
        if on {
            s.on.insert(id.to_string());
        } else {
            s.on.remove(id);
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::write(&tmp, serde_json::to_vec(&s).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }

    pub fn is_on(&self, id: &str) -> bool {
        self.on.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugins_are_off_until_switched_on() {
        let dir = std::env::temp_dir().join(format!("ddi-switches-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(FILE);
        assert!(!Switches::read(&path).is_on("example"));
        Switches::set(&path, "example", true).unwrap();
        assert!(Switches::read(&path).is_on("example"));
        Switches::set(&path, "example", false).unwrap();
        assert!(!Switches::read(&path).is_on("example"));
        std::fs::write(&path, br#"{"on":["Bad Id","ok"]}"#).unwrap();
        assert_eq!(Switches::read(&path).on, BTreeSet::from(["ok".to_string()]));
        std::fs::write(&path, b"garbage").unwrap();
        assert!(Switches::read(&path).on.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
