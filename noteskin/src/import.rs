//! Telling noteskins apart in an import: a picked folder, zip or
//! `NoteSkins/<game>/<name>/` tree may hold skins next to songs.

/// A skin folder found among an import's paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The folder's path in the import.
    pub dir: String,
    /// `dance`, `common`, or another game (not kept).
    pub game: String,
    pub name: String,
}

/// StepMania's and OutFox's game folders under `NoteSkins/`.
const GAMES: &[&str] = &[
    "dance",
    "pump",
    "kb7",
    "ez2",
    "para",
    "ds3ddx",
    "beat",
    "maniax",
    "techno",
    "popn",
    "lights",
    "kickbox",
    "taiko",
    "gh",
    "gddm",
    "gdgf",
    "be-mu",
    "po-mu",
    "smx",
    "stepstage",
    "kbx",
    "global",
];

/// The buttons and words element files start with.
const ELEMENT_PREFIXES: &[&str] = &[
    "down ",
    "left ",
    "up ",
    "right ",
    "upleft ",
    "upright ",
    "fallback ",
];

/// The skin folders among `paths` (files, `/`-separated): a folder holding
/// a `NoteSkin.lua`, or a `metrics.ini` with element files beside it or
/// under a `NoteSkins` folder (a theme also has a `metrics.ini`). A skin's
/// game is its parent folder's name when that is a game or `common`, else
/// `dance`. Folders inside a skin (OutFox and ITGmania variants) and
/// anything under `Themes` are not skins of their own.
pub fn find_skins(paths: &[String]) -> Vec<Found> {
    use std::collections::BTreeMap;
    // Folder → (has NoteSkin.lua, has metrics.ini, has element files).
    let mut dirs: BTreeMap<String, (bool, bool, bool)> = BTreeMap::new();
    for p in paths {
        let (dir, file) = p.rsplit_once('/').unwrap_or(("", p));
        let lower = file.to_ascii_lowercase();
        let e = dirs.entry(dir.to_string()).or_default();
        e.0 |= lower == "noteskin.lua";
        e.1 |= lower == "metrics.ini";
        e.2 |= ELEMENT_PREFIXES.iter().any(|w| lower.starts_with(w));
    }
    let mut found: Vec<Found> = Vec::new();
    for (dir, (script, metrics, elements)) in &dirs {
        let parts: Vec<&str> = dir.split('/').collect();
        let lower: Vec<String> = parts.iter().map(|p| p.to_ascii_lowercase()).collect();
        if dir.is_empty() || lower.iter().any(|p| p == "themes") {
            continue;
        }
        let under_noteskins = lower.iter().any(|p| p == "noteskins");
        if !(*script || *metrics && (*elements || under_noteskins)) {
            continue;
        }
        // Inside another skin: a variant or a skin's own subfolder.
        if found
            .iter()
            .any(|f| dir.starts_with(&format!("{}/", f.dir)))
        {
            continue;
        }
        let name = parts.last().copied().unwrap_or_default().to_string();
        let parent = lower.len().checked_sub(2).map(|i| lower[i].as_str());
        let game = match parent {
            Some("common") => "common",
            // `common` picked on its own is the shared skin.
            _ if name.eq_ignore_ascii_case("common") => "common",
            Some("global") => "dance",
            Some(g) if GAMES.contains(&g) => g,
            _ => "dance",
        };
        found.push(Found {
            dir: dir.clone(),
            game: game.to_string(),
            name,
        });
    }
    found
}

/// Whether the import keeps a skin of `game`: the dance layouts' skins and
/// the shared `common` ones.
pub fn kept_game(game: &str) -> bool {
    game == "dance" || game == "common"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn skins_in_trees_and_on_their_own() {
        let found = find_skins(&paths(&[
            "NoteSkins/dance/Delta/metrics.ini",
            "NoteSkins/dance/Delta/NoteSkin.lua",
            "NoteSkins/dance/Delta/Down Tap Note.lua",
            "NoteSkins/common/common/metrics.ini",
            "NoteSkins/pump/default/NoteSkin.lua",
            "NoteSkins/global/broadhead/NoteSkin.lua",
            "Mine/metrics.ini",
            "Mine/Down Tap Note 4x2.png",
            "Mine/variants/red/metrics.ini",
            "Themes/Simply Love/metrics.ini",
            "Songs/Pack/Song/song.sm",
            "Loose/metrics.ini",
            "common/NoteSkin.lua",
        ]));
        let got: Vec<(&str, &str, &str)> = found
            .iter()
            .map(|f| (f.dir.as_str(), f.game.as_str(), f.name.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("Mine", "dance", "Mine"),
                ("NoteSkins/common/common", "common", "common"),
                ("NoteSkins/dance/Delta", "dance", "Delta"),
                ("NoteSkins/global/broadhead", "dance", "broadhead"),
                ("NoteSkins/pump/default", "pump", "default"),
                ("common", "common", "common"),
            ]
        );
        assert!(kept_game("dance") && kept_game("common") && !kept_game("pump"));
    }
}
