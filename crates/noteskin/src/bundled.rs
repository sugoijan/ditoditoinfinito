//! The skins shipped with the game (`assets/noteskins/<game>/<name>/`):
//! the index `xtask gen-noteskins` writes and the app fetches, so a play
//! needs no request for a skin's texts.

use serde::{Deserialize, Serialize};

use crate::Folder;

/// `noteskins/index.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Index {
    pub skins: Vec<Bundled>,
}

/// One bundled skin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bundled {
    /// Its folder under `noteskins/`, `<game>/<name>` as in the repository.
    pub path: String,
    /// Its files (not its `PROVENANCE.toml`) and kept texts.
    pub folder: Folder,
    /// Who made it and under which licence, in a line (the skin's origin,
    /// as an imported skin's is the folder it came from).
    pub credit: String,
    /// The skin this one is a color scheme of; such skins are chosen
    /// through it, not listed on their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_of: Option<String>,
    /// Each of `folder.files`' content hash (`crate::report::hash`), in
    /// the same order: the web fetches a file under its hash, so a new
    /// build's file is never taken from an old one's cache.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hashes: Vec<String>,
}

/// The start of a bundled skin's key.
pub const KEY_PREFIX: &str = "bundled:";

/// The `noteskin` setting's value for the built-in arrows without a skin.
pub const BUILT_IN: &str = "builtin";

/// The default SVG noteskin's stable key.
pub const INFINITO: &str = "bundled:dance/infinito";

/// Peter Lawrence's DDR skin, with Vivid and subdivision colors.
pub const PETERS_DDR: &str = "bundled:dance/peters-ddr";

/// Names of Infinito's parameters the game reads. Infinito declares its
/// controls in its metrics; Colors selects its Vivid, Rainbow or Flat
/// material behavior.
pub mod infinito {
    pub const COLORS: &str = "Colors";
    pub const EXPLOSIONS: &str = "Explosions";
}

/// Infinito's own `metrics.ini`.
const INFINITO_METRICS: &str = include_str!("../../../assets/noteskins/dance/Infinito/metrics.ini");

/// Infinito's declarations, read from its own `metrics.ini` (built in, so
/// the settings and Options know them before any skin is read).
pub fn infinito_parameters() -> &'static crate::params::Declared {
    static DECLARED: std::sync::OnceLock<crate::params::Declared> = std::sync::OnceLock::new();
    DECLARED.get_or_init(|| {
        crate::params::Declared::from_ini(&crate::ini::Ini::parse(INFINITO_METRICS))
    })
}

/// `<game>/<name>` in lower case (names match ignoring case, as
/// StepMania's do): an imported skin's key, and a bundled one's after
/// [`KEY_PREFIX`].
pub fn skin_key(game: &str, name: &str) -> String {
    format!(
        "{}/{}",
        game.to_ascii_lowercase(),
        name.to_ascii_lowercase()
    )
}

/// A skin's key, the `noteskin` setting's value for it: [`skin_key`],
/// after [`KEY_PREFIX`] for a bundled skin.
pub fn key(game: &str, name: &str, bundled: bool) -> String {
    let key = skin_key(game, name);
    if bundled {
        format!("{KEY_PREFIX}{key}")
    } else {
        key
    }
}

/// The key a Lua stop of a skin counts against: a bundled color
/// variant's (`variant_of` its skin's name) against the skin it is a
/// variant of (they share their Lua), any other's against its own
/// [`key`].
pub fn strike_key(game: &str, name: &str, bundled: bool, variant_of: Option<&str>) -> String {
    match variant_of {
        Some(base) if bundled => format!("{KEY_PREFIX}{}", skin_key(game, base)),
        _ => key(game, name, bundled),
    }
}

/// A skin as [`pickable`] reads it.
#[derive(Clone, Copy, Debug)]
pub struct Candidate<'a> {
    pub game: &'a str,
    pub name: &'a str,
    pub bundled: bool,
    /// It is a bundled skin's color scheme ([`Bundled::variant_of`]).
    pub variant: bool,
    /// Its Lua is off (it stopped twice).
    pub lua_off: bool,
}

/// The dance and pump skins one can pick, `(key, name)`: the bundled ones
/// first, then the player's, each by name ignoring case. Infinito (the
/// default), its colors and `default` are chosen otherwise; pump skins are
/// listed for the pump layouts. A name says when the skin comes with the
/// game and when its Lua is off.
pub fn pickable<'a>(skins: impl IntoIterator<Item = Candidate<'a>>) -> Vec<(String, String)> {
    let mut list: Vec<(bool, String, String)> = skins
        .into_iter()
        .filter(|s| {
            (s.game == "dance" || s.game == "pump")
                && !s.name.eq_ignore_ascii_case("default")
                && !s.variant
                && !(s.bundled && s.name == "Infinito")
        })
        .map(|s| {
            let mut name = s.name.to_string();
            if s.bundled {
                name.push_str(" (with the game)");
            }
            if s.lua_off {
                name.push_str(" (its Lua is off)");
            }
            (!s.bundled, key(s.game, s.name, s.bundled), name)
        })
        .collect();
    list.sort_by_key(|(mine, _, n)| (*mine, n.to_lowercase()));
    list.into_iter().map(|(_, k, n)| (k, n)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(key("dance", "Infinito", true), INFINITO);
        assert_eq!(key("Pump", "Cmd", false), "pump/cmd");
        assert_eq!(
            strike_key("dance", "Infinito", true, Some("Infinito")),
            INFINITO
        );
        assert!(!key("dance", "Delta", false).starts_with(KEY_PREFIX));
        assert_ne!(BUILT_IN, key("dance", "builtin", true));
    }

    #[test]
    fn infinito_declares_its_svg_controls_without_procedural_choices() {
        use crate::params::{Kind, Value};
        let d = infinito_parameters();
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
        for name in ["ChartColors", "Lighter"] {
            assert!(d.get(name).is_none());
        }
        let colors = d.get(infinito::COLORS).unwrap();
        let Kind::Choice(choices) = &colors.kind else {
            panic!("a color choice")
        };
        assert_eq!(
            choices.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["Vivid", "Rainbow", "Flat"]
        );
        assert_eq!(colors.default, Value::Text("Vivid".into()));
        assert!(choices.iter().all(|c| c.skin.is_none()));
        for (name, default) in [
            ("Onigiri", true),
            ("ShockArrows", false),
            (infinito::EXPLOSIONS, true),
            ("ReceptorFill", true),
            ("HitVariation", true),
        ] {
            let p = d.get(name).unwrap();
            assert_eq!(
                (&p.kind, &p.default),
                (&Kind::Toggle, &Value::Bool(default))
            );
        }
    }
}
