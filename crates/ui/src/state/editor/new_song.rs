//! A new song from a music file (`docs/plans/editor.md`, "New songs"),
//! as both shells make it: its title and artist from the file's tags
//! (`ddi_library::tags`), else the title from its name; its folder,
//! named after the title, in the pack [`PACK`]; the editor on it.

use ddi_library::pack;
use ddi_library::tags::{self, Tags};

use super::{Editor, export};

/// The pack new songs are made in (desktop: a folder of that name in
/// the first song folder; the web: the stored songs' pack).
pub const PACK: &str = "Dito Edits";

/// What the editor says when it opens on a new song.
pub const STATUS: &str = "A new song: saving keeps it";

/// A new song's names and where it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSong {
    pub title: String,
    pub artist: String,
    /// Its folder's name in [`PACK`]: the title as a file name, numbered
    /// when that is taken.
    pub folder: String,
    /// The music's file name in the folder (the file's own).
    pub music: String,
}

impl NewSong {
    /// The song for music file `file` (its name), `head` its first bytes
    /// ([`tags::HEAD`]), a folder of which `taken` says whether its name
    /// is in use.
    pub fn plan(file: &str, head: &[u8], taken: impl Fn(&str) -> bool) -> NewSong {
        let Tags { title, artist } = tags::read(head);
        let music = file.rsplit(['/', '\\']).next().unwrap_or(file).to_string();
        let title = title.unwrap_or_else(|| title_from_name(&music));
        let stem = export::file_stem(&title);
        let folder = (1..)
            .map(|n| {
                if n == 1 {
                    stem.clone()
                } else {
                    format!("{stem} {n}")
                }
            })
            .find(|f| !taken(f))
            .unwrap_or(stem);
        NewSong {
            title,
            artist: artist.unwrap_or_default(),
            folder,
            music,
        }
    }

    /// The library's id of the song (`ddi_library::pack::song_id`).
    pub fn id(&self) -> String {
        pack::song_id(PACK, &self.folder)
    }

    /// The editor on it: [`Editor::new_song`] with its names, not saved.
    pub fn editor(&self) -> Editor {
        let mut e = Editor::new_song(Some(self.music.clone()), &self.title);
        e.song.artist = self.artist.trim().to_string();
        e
    }
}

/// A title from a file's name: without its extension, `_` as spaces.
fn title_from_name(name: &str) -> String {
    let stem = match name.rsplit_once('.') {
        Some((s, _)) if !s.is_empty() => s,
        _ => name,
    };
    let t = stem.replace('_', " ");
    let t = t.trim();
    if t.is_empty() {
        "New song".into()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_tags_else_the_file_name() {
        let s = NewSong::plan("/music/my_tune.ogg", b"", |_| false);
        assert_eq!(s.title, "my tune");
        assert_eq!(s.artist, "");
        assert_eq!(
            (s.folder.as_str(), s.music.as_str()),
            ("my tune", "my_tune.ogg")
        );
        let e = s.editor();
        assert_eq!(e.song().music.as_deref(), Some("my_tune.ogg"));
        assert_eq!(e.song().charts.len(), 1);
        assert!(e.is_dirty(), "not saved yet");
        // Written, it is listed with its one empty chart.
        let text = ddi_chart::formats::sm_write::write_ssc(e.song());
        let entry =
            ddi_library::edited::entry_for_new(&s.id(), &text, "chart.ssc".into(), "t.ogg".into())
                .unwrap();
        assert_eq!((entry.title.as_str(), entry.charts.len()), ("my tune", 1));
        // ID3: the title and artist; a title a file system refuses, and
        // one taken twice.
        let mut id3 = b"ID3\x03\0\0\0\0\0\x30".to_vec();
        for (id, text) in [(b"TIT2", "\x03A/B"), (b"TPE1", "\x03Band")] {
            id3.extend(id);
            id3.extend((text.len() as u32).to_be_bytes());
            id3.extend([0, 0]);
            id3.extend(text.as_bytes());
        }
        id3.resize(64, 0);
        let s = NewSong::plan("x.mp3", &id3, |f| f == "A_B" || f == "A_B 2");
        assert_eq!((s.title.as_str(), s.artist.as_str()), ("A/B", "Band"));
        assert_eq!(s.folder, "A_B 3");
        assert_eq!(s.id(), pack::song_id(PACK, "A_B 3"));
        let e = s.editor();
        assert_eq!(
            (e.song().title.as_str(), e.song().artist.as_str()),
            ("A/B", "Band")
        );
        assert_eq!(NewSong::plan(".ogg", b"", |_| false).title, ".ogg");
        assert_eq!(NewSong::plan("_.wav", b"", |_| false).title, "New song");
    }
}
