//! Telling noteskins apart in an import: a picked folder, zip or
//! `NoteSkins/<game>/<name>/` tree may hold skins next to songs.

/// A skin folder found among an import's paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The folder's path in the import.
    pub dir: String,
    /// `dance`, `pump`, `common`, or another game (not kept).
    pub game: String,
    pub name: String,
    /// Written for StepMania 3.9 or OpenITG (`.sprite`, `.model`, `.actor`
    /// elements, which StepMania 5 skins never use): those files are not
    /// read.
    pub legacy: bool,
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
    "downleft ",
    "downright ",
    "center ",
    "fallback ",
];

/// What a skin's element files say of its game: dance's buttons (Up,
/// Down, Left, Right), or only pump's (UpLeft, UpRight, Center, DownLeft,
/// DownRight; dance's solo has UpLeft and UpRight too, beside its own).
#[derive(Clone, Copy, Debug, Default)]
struct Buttons {
    dance: bool,
    pump: bool,
}

impl Buttons {
    fn see(&mut self, element: &str) {
        let word = element.split([' ', '_']).next().unwrap_or_default();
        match word {
            "up" | "down" | "left" | "right" => self.dance = true,
            "center" | "downleft" | "downright" => self.pump = true,
            _ => {}
        }
    }

    fn pump(self) -> bool {
        self.pump && !self.dance
    }
}

/// The skin folders among `paths` (files, `/`-separated): a folder holding
/// a `NoteSkin.lua`, or a `metrics.ini` with element files or parts
/// (`_down tap note model.txt`: a variant may hold only those, falling back
/// on its parent for the rest) beside it or under a `NoteSkins` folder (a
/// theme also has a `metrics.ini`). A skin's
/// game is its parent folder's name when that is a game or `common`, else
/// `pump` when its element files name only pump's buttons (a pump skin
/// picked on its own), else `dance`. Folders inside a skin (OutFox and ITGmania variants) and
/// anything under `Themes` are not skins of their own. StepMania 5 skins
/// come first, so one wins over an older skin of the same name.
pub fn find_skins(paths: &[String]) -> Vec<Found> {
    use std::collections::BTreeMap;
    // Folder → (has NoteSkin.lua, has metrics.ini, has element files, has
    // StepMania 3.9 elements, the buttons its files name).
    let mut dirs: BTreeMap<String, (bool, bool, bool, bool, Buttons)> = BTreeMap::new();
    for p in paths {
        let (dir, file) = p.rsplit_once('/').unwrap_or(("", p));
        let lower = file.to_ascii_lowercase();
        let e = dirs.entry(dir.to_string()).or_default();
        e.0 |= lower == "noteskin.lua";
        e.1 |= lower == "metrics.ini";
        let element = lower.strip_prefix('_').unwrap_or(&lower);
        e.2 |= ELEMENT_PREFIXES.iter().any(|w| element.starts_with(w));
        e.4.see(element);
        e.3 |= [".sprite", ".model", ".actor"]
            .iter()
            .any(|x| lower.ends_with(x));
    }
    let mut found: Vec<Found> = Vec::new();
    for (dir, (script, metrics, elements, legacy, buttons)) in &dirs {
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
            _ if buttons.pump() => "pump",
            _ => "dance",
        };
        found.push(Found {
            dir: dir.clone(),
            game: game.to_string(),
            name,
            legacy: *legacy && !*script,
        });
    }
    found.sort_by_key(|f| f.legacy);
    found
}

/// Whether the import keeps a skin of `game`: dance skins (which draw
/// Dancing☆Onigiri's layouts too), pump skins (the pump layouts) and the
/// shared `common` ones.
pub fn kept_game(game: &str) -> bool {
    game == "dance" || game == "pump" || game == "common"
}

/// Whether a skin's file is kept as text with the skin's record: its
/// `metrics.ini`, other `.ini` files (animated textures), redirects, Lua and
/// `.txt` files (3D models; what [`crate::Folder::texts`] holds).
pub fn kept_text(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".ini", ".redir", ".lua", ".txt", ".sprite", ".actor", ".model", ".xml",
    ]
    .iter()
    .any(|e| lower.ends_with(e))
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
            "Vivid/metrics.ini",
            "Vivid/_down tap note model.txt",
            "Vivid/textures/Fill.png",
            "Old/vivid/metrics.ini",
            "Old/vivid/Down Tap Note 4th.model",
            "Old/vivid/Down Hold Body Active.png",
            "Themes/Simply Love/metrics.ini",
            "Songs/Pack/Song/song.sm",
            "Loose/metrics.ini",
            "common/NoteSkin.lua",
            "Prime/metrics.ini",
            "Prime/UpLeft Tap Note 3x2.png",
            "Prime/Center Tap Note 3x2.png",
            "Prime/_downleft hold body active.png",
            "Solo/NoteSkin.lua",
            "Solo/UpLeft Tap Note.png",
            "Solo/Down Tap Note.png",
        ]));
        let got: Vec<(&str, &str, &str)> = found
            .iter()
            .filter(|f| !f.legacy)
            .map(|f| (f.dir.as_str(), f.game.as_str(), f.name.as_str()))
            .collect();
        // A StepMania 3.9 skin comes last, after a skin of its name.
        let last = found.last().unwrap();
        assert!(last.legacy && last.dir == "Old/vivid", "{found:?}");
        assert_eq!(
            got,
            [
                ("Mine", "dance", "Mine"),
                ("NoteSkins/common/common", "common", "common"),
                ("NoteSkins/dance/Delta", "dance", "Delta"),
                ("NoteSkins/global/broadhead", "dance", "broadhead"),
                ("NoteSkins/pump/default", "pump", "default"),
                ("Prime", "pump", "Prime"),
                ("Solo", "dance", "Solo"),
                ("Vivid", "dance", "Vivid"),
                ("common", "common", "common"),
            ]
        );
        assert!(kept_game("dance") && kept_game("common") && kept_game("pump"));
        assert!(!kept_game("kb7"));
    }
}
