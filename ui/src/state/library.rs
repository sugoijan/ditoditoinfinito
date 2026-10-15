//! What screens read of the libraries (`docs/plans/ui-core.md`, P12): the
//! songs (their manifest entries, which hold the summaries and the
//! charts), where each came from, and its banner as a texture source; the
//! noteskins by key and name. A shell builds these from wherever it keeps
//! the libraries (the web: the bundled manifest and IndexedDB; desktop:
//! folders) and hands them to the screens; importing stays the shell's.

pub use ddi_library::manifest::{ChartInfo, ManifestEntry};

/// Where a song came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Shipped with the game.
    Bundled,
    /// Imported by the player, in this pack.
    Imported { pack: String },
}

/// A song as the song list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SongView {
    pub entry: ManifestEntry,
    pub origin: Origin,
    /// Its banner as a texture source (`ddi_render::texture_cache`'s
    /// `TextureKey::source`: a path of the site, `files:<key>`, a file).
    pub banner: Option<String>,
}

impl SongView {
    pub fn pack(&self) -> Option<&str> {
        match &self.origin {
            Origin::Bundled => None,
            Origin::Imported { pack } => Some(pack),
        }
    }
}

/// A noteskin the player can pick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteskinView {
    /// What [`crate::state::settings::Settings::noteskin`] stores.
    pub key: String,
    /// As shown.
    pub name: String,
}

/// The libraries as screens see them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibraryView {
    pub songs: Vec<SongView>,
    pub noteskins: Vec<NoteskinView>,
    /// Why imported songs could not be listed, if they could not.
    pub error: Option<String>,
}

/// Where a shell's libraries come from.
pub trait Libraries {
    /// The libraries now (a shell refreshes it after an import).
    fn view(&self) -> LibraryView;
}

impl LibraryView {
    pub fn song(&self, id: &str) -> Option<&SongView> {
        self.songs.iter().find(|s| s.entry.id == id)
    }

    /// The packs of imported songs, each once (told apart ignoring case,
    /// as `place::pack_key` does), in order of first song.
    pub fn packs(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for s in &self.songs {
            if let Some(p) = s.pack()
                && !out.iter().any(|o| o.to_lowercase() == p.to_lowercase())
            {
                out.push(p);
            }
        }
        out
    }
}
