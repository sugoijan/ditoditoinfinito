//! The editor's autosave: the song as edited, kept by the shell every
//! few seconds while it has unsaved changes (the web in local storage,
//! desktop in its data folder) and forgotten once it is saved or left,
//! so a crash or a closed tab loses nothing. One slot: the last song
//! edited. Opening that song again recovers it, unsaved.

use ddi_chart::{Song, Tick};
use serde::{Deserialize, Serialize};

use super::Editor;

/// Where the web keeps it (`localStorage`).
pub const KEY: &str = "ddi.editor.v1";

/// Seconds between autosaves while there are changes.
pub const EVERY: f64 = 5.0;

/// A song being edited, as kept.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Autosave {
    /// The library's id of the song.
    pub song_id: String,
    pub chart: usize,
    pub cursor: i64,
    /// When it was kept, ms since the Unix epoch.
    pub at_ms: f64,
    pub song: Song,
}

impl Editor {
    /// The song as edited now, for song `song_id`'s autosave.
    pub fn autosave(&self, song_id: &str, at_ms: f64) -> Autosave {
        Autosave {
            song_id: song_id.to_string(),
            chart: self.chart,
            cursor: self.cursor.0,
            at_ms,
            song: self.song.clone(),
        }
    }

    /// An autosave opened again: its song unsaved, where it was.
    pub fn recover(a: Autosave) -> Editor {
        let mut e = Editor::open(a.song, a.chart);
        e.saved = Some(0);
        e.cursor = Tick(a.cursor.max(0));
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_autosave_recovers_the_song_unsaved() {
        let mut e = Editor::new_song(Some("a.ogg".into()), "Kept");
        e.mark_saved();
        e.toggle_at(2, Tick::from_beats(3));
        e.set_cursor(Tick::from_beats(3), false);
        let json = serde_json::to_string(&e.autosave("pack:song", 12.0)).unwrap();
        let a: Autosave = serde_json::from_str(&json).unwrap();
        assert_eq!(a.song_id, "pack:song");
        let r = Editor::recover(a);
        assert_eq!(r.song(), e.song());
        assert_eq!(r.cursor(), Tick::from_beats(3));
        assert!(r.is_dirty(), "recovered, not saved");
        assert!(!r.can_undo());
    }
}
