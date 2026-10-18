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
