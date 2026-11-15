//! The editor's autosave: the song as edited, kept by the shell every
//! few seconds while it has unsaved changes (the web in local storage,
//! desktop in its data folder), so a crash or a closed tab loses
//! nothing. Each song edited has a slot of its own ([`Slot::Song`]),
//! forgotten only by that song's own save or by leaving it without
//! saving (an explicit discard): opening, leaving or saving another
//! song never touches it. Opening the song again recovers it, unsaved.
//! The one slot builds before 2026-11-15 kept ([`Slot::Legacy`]) is
//! still read, for the song it holds.

use ddi_chart::{Song, Tick};
use serde::{Deserialize, Serialize};

use super::Editor;

/// Where the web keeps it (`localStorage`): the one slot of earlier
/// builds under this key, a song's own under `<KEY>:<name>`.
pub const KEY: &str = "ddi.editor.v1";

/// Where an autosave is kept.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    /// The one slot earlier builds kept, whatever song it holds.
    Legacy,
    /// A song's own, by a name safe in a file name and a storage key
    /// ([`Slot::of`]).
    Song(String),
}

impl Slot {
    /// Song `song_id`'s slot: its name is a hash of the id (two songs
    /// sharing one is harmless: an autosave says whose it is).
    pub fn of(song_id: &str) -> Slot {
        // FNV-1a, 64 bits.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in song_id.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Slot::Song(format!("{h:016x}"))
    }

    /// The web's storage key.
    pub fn key(&self) -> String {
        match self {
            Slot::Legacy => KEY.to_string(),
            Slot::Song(name) => format!("{KEY}:{name}"),
        }
    }
}

/// A shell's store of autosaves, as JSON text.
pub trait Slots {
    fn read(&self, slot: &Slot) -> Option<String>;
    fn write(&self, slot: &Slot, json: &str) -> Result<(), String>;
    fn remove(&self, slot: &Slot);
}

fn held(slots: &dyn Slots, slot: &Slot, song_id: &str) -> Option<Autosave> {
    serde_json::from_str::<Autosave>(&slots.read(slot)?)
        .ok()
        .filter(|a| a.song_id == song_id)
}

/// Song `song_id`'s autosave, if one is kept.
pub fn load(slots: &dyn Slots, song_id: &str) -> Option<Autosave> {
    held(slots, &Slot::of(song_id), song_id).or_else(|| held(slots, &Slot::Legacy, song_id))
}

/// Keeps `a` in its song's slot (an older one of the same song in the
/// legacy slot let go).
pub fn keep(slots: &dyn Slots, a: &Autosave) -> Result<(), String> {
    let json = serde_json::to_string(a).map_err(|e| e.to_string())?;
    slots.write(&Slot::of(&a.song_id), &json)?;
    if held(slots, &Slot::Legacy, &a.song_id).is_some() {
        slots.remove(&Slot::Legacy);
    }
    Ok(())
}

/// Forgets song `song_id`'s autosave (it was saved, or left without
/// saving), and no other song's.
pub fn forget(slots: &dyn Slots, song_id: &str) {
    for slot in [Slot::of(song_id), Slot::Legacy] {
        if held(slots, &slot, song_id).is_some() {
            slots.remove(&slot);
        }
    }
}

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
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct Memory(RefCell<HashMap<Slot, String>>);

    impl Slots for Memory {
        fn read(&self, slot: &Slot) -> Option<String> {
            self.0.borrow().get(slot).cloned()
        }
        fn write(&self, slot: &Slot, json: &str) -> Result<(), String> {
            self.0.borrow_mut().insert(slot.clone(), json.to_string());
            Ok(())
        }
        fn remove(&self, slot: &Slot) {
            self.0.borrow_mut().remove(slot);
        }
    }

    fn edited(title: &str) -> Editor {
        let mut e = Editor::new_song(Some("a.ogg".into()), title);
        e.mark_saved();
        e.toggle_at(1, Tick::from_beats(2));
        e
    }

    /// Review studio-editor-1: another song opened and left, or saved,
    /// leaves this song's autosave alone; only its own save or discard
    /// forgets it. And a second song's edits never replace it.
    #[test]
    fn a_song_s_autosave_is_forgotten_only_for_that_song() {
        let slots = Memory::default();
        keep(&slots, &edited("A").autosave("pack:a", 1.0)).unwrap();
        // B opened clean and left (or saved): what the shells do then.
        forget(&slots, "pack:b");
        assert!(load(&slots, "pack:a").is_some(), "A's work is still kept");
        // B edited and autosaved: beside A's.
        keep(&slots, &edited("B").autosave("pack:b", 2.0)).unwrap();
        assert_eq!(load(&slots, "pack:a").unwrap().song.title, "A");
        assert_eq!(load(&slots, "pack:b").unwrap().song.title, "B");
        forget(&slots, "pack:b");
        assert!(load(&slots, "pack:b").is_none());
        assert!(load(&slots, "pack:a").is_some());
        forget(&slots, "pack:a");
        assert!(load(&slots, "pack:a").is_none());
        assert!(slots.0.borrow().is_empty());
    }

    /// The one slot of earlier builds: read for its song, left alone by
    /// another's, let go once its song is kept anew or forgotten.
    #[test]
    fn the_one_slot_of_earlier_builds_is_still_read() {
        let slots = Memory::default();
        let old = serde_json::to_string(&edited("A").autosave("pack:a", 1.0)).unwrap();
        slots.write(&Slot::Legacy, &old).unwrap();
        assert!(load(&slots, "pack:b").is_none());
        forget(&slots, "pack:b");
        keep(&slots, &edited("B").autosave("pack:b", 2.0)).unwrap();
        assert_eq!(load(&slots, "pack:a").unwrap().song.title, "A");
        keep(&slots, &edited("A2").autosave("pack:a", 3.0)).unwrap();
        assert!(slots.read(&Slot::Legacy).is_none(), "the older one let go");
        assert_eq!(load(&slots, "pack:a").unwrap().song.title, "A2");
        assert_eq!(Slot::Legacy.key(), KEY);
        assert!(Slot::of("pack:a").key().starts_with("ddi.editor.v1:"));
    }

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
