//! Noteskins on disk: the bundled ones (the index `xtask gen-noteskins`
//! writes, as the web reads it) and those in the `NoteSkins/` folder of
//! each StepMania root among the song folders, read in place and found as
//! the web's import finds skins (`ddi_noteskin::import::find_skins`).
//! Keys are the web's: `bundled:<game>/<name>` for a bundled skin,
//! `<game>/<name>` (lower case) for the player's, so the `noteskin`
//! setting means the same skin in both shells. The first folder holding
//! a skin wins, as for packs. Drawing them comes with play on desktop
//! (phase 9); for now the options list them.

use std::path::{Path, PathBuf};

use ddi_noteskin::bundled::Index;
use ddi_noteskin::import::{find_skins, kept_game};

/// A skin found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskSkin {
    pub game: String,
    pub name: String,
    /// Its folder.
    pub dir: PathBuf,
    pub bundled: bool,
    /// The skin it is a color scheme of (bundled ones).
    pub variant_of: Option<String>,
}

impl DiskSkin {
    /// The key a Lua stop counts against: a color variant's against the
    /// skin it is a variant of (they share their Lua), as the web's.
    pub fn strike_key(&self) -> String {
        ddi_noteskin::bundled::strike_key(
            &self.game,
            &self.name,
            self.bundled,
            self.variant_of.as_deref(),
        )
    }

    /// The `noteskin` setting's value for it, as the web's.
    pub fn key(&self) -> String {
        ddi_noteskin::bundled::key(&self.game, &self.name, self.bundled)
    }
}

/// The bundled skins' index beside the assets
/// (`target/noteskins/index.json`, as Trunk builds it); `None` when it was
/// never generated.
pub fn read_index() -> Option<Index> {
    let assets = crate::images::assets();
    [
        assets.join("../target/noteskins/index.json"),
        assets.join("noteskins/index.json"),
    ]
    .iter()
    .find_map(|p| std::fs::read_to_string(p).ok())
    .and_then(|t| serde_json::from_str::<Index>(&t).ok())
}

/// The bundled skins, from [`read_index`]; none when it was never
/// generated.
pub fn bundled() -> Vec<DiskSkin> {
    let assets = crate::images::assets();
    read_index()
        .map(|i| i.skins)
        .unwrap_or_default()
        .into_iter()
        .map(|b| DiskSkin {
            dir: assets.join("noteskins").join(&b.path),
            game: b.folder.game,
            name: b.folder.name,
            bundled: true,
            variant_of: b.variant_of,
        })
        .collect()
}

/// The skins under `folder`'s `NoteSkins/` (a StepMania root), dance's
/// and the shared ones.
pub fn in_root(folder: &Path) -> Vec<DiskSkin> {
    let Some(dir) = super::child(folder, "NoteSkins") else {
        return Vec::new();
    };
    let paths: Vec<String> = super::files::walk(&dir)
        .listed
        .into_iter()
        .map(|l| format!("NoteSkins/{}", l.path))
        .collect();
    find_skins(&paths)
        .into_iter()
        .filter(|f| kept_game(&f.game))
        .map(|f| DiskSkin {
            dir: folder.join(&f.dir),
            game: f.game,
            name: f.name,
            bundled: false,
            variant_of: None,
        })
        .collect()
}

/// Every skin: the player's folders' (the first holding a skin wins),
/// then the bundled ones.
pub fn all(folders: &[PathBuf]) -> Vec<DiskSkin> {
    let mut out: Vec<DiskSkin> = Vec::new();
    for f in folders {
        for s in in_root(f) {
            if !out.iter().any(|o| o.key() == s.key()) {
                out.push(s);
            }
        }
    }
    out.extend(bundled());
    out
}

/// Skins whose Lua stopped once in this run, and those whose Lua is off
/// for it (stopped twice), with why, by [`DiskSkin::strike_key`].
#[derive(Default)]
struct Stops {
    once: std::collections::HashSet<String>,
    off: std::collections::HashMap<String, String>,
}

static STOPS: std::sync::LazyLock<std::sync::Mutex<Stops>> =
    std::sync::LazyLock::new(Default::default);

/// Skin `strike`'s Lua stopped (at a load or in a play) because of
/// `why`: the next play tries again; the second time its Lua is off for
/// the run (the web's rule for a bundled skin; a skin on disk can change
/// under us, so it is not kept past the run).
pub fn lua_stopped(strike: &str, why: &str) {
    eprintln!("noteskin {strike}: {why}");
    let Ok(mut s) = STOPS.lock() else {
        return;
    };
    if s.once.insert(strike.to_string()) {
        return;
    }
    s.off.insert(strike.to_string(), why.to_string());
}

/// Why skin `strike`'s Lua is off, when it is.
pub fn lua_off(strike: &str) -> Option<String> {
    STOPS.lock().ok()?.off.get(strike).cloned()
}

/// The dance and pump skins one can pick `(key, name)`, as the web
/// lists them:
/// the bundled ones first, then the player's, by name; Dito (the
/// default), its colors and `default` are chosen otherwise.
pub fn choices(skins: &[DiskSkin]) -> Vec<(String, String)> {
    ddi_noteskin::bundled::pickable(skins.iter().map(|s| ddi_noteskin::bundled::Candidate {
        game: &s.game,
        name: &s.name,
        bundled: s.bundled,
        variant: s.variant_of.is_some(),
        lua_off: lua_off(&s.strike_key()).is_some(),
    }))
}

/// The parameters skins declare, by key (`ddi_noteskin::params`).
pub type Parameters = std::collections::BTreeMap<String, Vec<ddi_noteskin::params::Parameter>>;

/// The parameters the skins of `choices` among `skins` declare, by key
/// (Dito's are built in).
pub fn parameters(skins: &[DiskSkin], choices: &[(String, String)]) -> Parameters {
    let folders = crate::play::skins::folders(skins);
    let keys: Vec<String> = choices.iter().map(|(k, _)| k.clone()).collect();
    ddi_noteskin::params::catalog(folders.iter().zip(skins.iter().map(|s| s.bundled)), &keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skins_lua_is_off_after_its_second_stop() {
        let skin = |name: &str, variant_of: Option<&str>| DiskSkin {
            game: "dance".into(),
            name: name.into(),
            dir: PathBuf::new(),
            bundled: true,
            variant_of: variant_of.map(str::to_string),
        };
        let base = skin("Strikes", None);
        let variant = skin("Strikes vivid", Some("Strikes"));
        // A variant's stops are its skin's.
        assert_eq!(variant.strike_key(), base.strike_key());
        lua_stopped(&variant.strike_key(), "out of fuel");
        assert_eq!(lua_off(&base.strike_key()), None);
        lua_stopped(&base.strike_key(), "out of memory");
        assert_eq!(
            lua_off(&base.strike_key()).as_deref(),
            Some("out of memory")
        );
        let listed = choices(std::slice::from_ref(&base));
        assert_eq!(listed[0].1, "Strikes (with the game) (its Lua is off)");
    }

    #[test]
    fn a_stepmania_roots_skins_are_listed_with_the_webs_keys() {
        let root = std::env::temp_dir()
            .join("ddi-library-tests")
            .join(format!("skins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for p in [
            "NoteSkins/dance/Metal/NoteSkin.lua",
            "NoteSkins/dance/Metal/metrics.ini",
            "NoteSkins/dance/default/metrics.ini",
            "NoteSkins/dance/default/Down Tap Note.png",
            "NoteSkins/common/common/metrics.ini",
            "NoteSkins/pump/Old/metrics.ini",
            "Songs/Pack/Song/s.sm",
        ] {
            let path = root.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"x").unwrap();
        }
        let skins = in_root(&root);
        let keys: Vec<String> = skins.iter().map(DiskSkin::key).collect();
        assert!(keys.contains(&"dance/metal".to_string()), "{keys:?}");
        assert!(keys.contains(&"common/common".to_string()), "{keys:?}");
        // Pump skins are kept for the pump layouts; `default` is not one
        // to pick.
        assert!(keys.contains(&"pump/old".to_string()), "{keys:?}");
        let metal = skins.iter().find(|s| s.name == "Metal").unwrap();
        assert_eq!(metal.dir, root.join("NoteSkins/dance/Metal"));
        // Listed as the web lists them: `default` and `common` are not
        // chosen by name.
        let picked = choices(&skins);
        assert_eq!(
            picked,
            vec![
                ("dance/metal".into(), "Metal".into()),
                ("pump/old".into(), "Old".into())
            ]
        );
        // The same root twice: listed once.
        assert_eq!(
            all(&[root.clone(), root.clone()]).len(),
            skins.len() + bundled().len()
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
