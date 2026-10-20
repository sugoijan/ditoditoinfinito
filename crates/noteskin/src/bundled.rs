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
}

/// The start of a bundled skin's key.
pub const KEY_PREFIX: &str = "bundled:";

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
/// first, then the player's, each by name ignoring case. Dito (the
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
                && !(s.bundled && s.name == "Dito")
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
