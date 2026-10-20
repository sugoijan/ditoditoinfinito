//! The play history on desktop in the data folder (`docs/plans/desktop.md`,
//! D10): `plays/` (two files a play, `crate::plays`) in the platform's
//! data directory, `~/Library/Application Support/DitoDitoInfinito` on
//! macOS (beside the settings), `%APPDATA%\DitoDitoInfinito` on Windows,
//! `~/.local/share/ditoditoinfinito` on Linux, where plays kept in the
//! config folder before (`~/.config/ditoditoinfinito/plays`) are moved
//! the first time.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use ddi_ui::state::history::Plays;

use crate::plays::FilePlayStore;

/// The app's folder's name in the platform's folders.
pub const APP_FOLDER: &str = if cfg!(target_os = "linux") {
    "ditoditoinfinito"
} else {
    "DitoDitoInfinito"
};

/// `plays/` in the data folder.
pub fn plays_dir() -> Option<PathBuf> {
    Some(crate::paths::data_dir()?.join("plays"))
}

/// What moving the plays did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Moved {
    /// Files moved.
    pub moved: usize,
    /// Files left in the old folder: the new one has a file of that
    /// name, or the copy failed.
    pub left: usize,
    /// Both folders held plays.
    pub both: bool,
}

/// Plays kept before in `old` go to `new`: the folder renamed when `new`
/// is not there, else file by file (each removed from `old` once copied,
/// so an interruption strands nothing and records nothing twice), never
/// over a file `new` has. `old` goes once it is empty.
pub fn move_plays(old: &Path, new: &Path) -> std::io::Result<Moved> {
    let mut m = Moved::default();
    if !old.is_dir() {
        return Ok(m);
    }
    if !new.exists() {
        if let Some(parent) = new.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::rename(old, new).is_ok() {
            m.moved = std::fs::read_dir(new)?.count();
            return Ok(m);
        }
    } else {
        m.both = true;
    }
    std::fs::create_dir_all(new)?;
    for e in std::fs::read_dir(old)?.flatten() {
        let to = new.join(e.file_name());
        if to.exists() || !e.path().is_file() {
            m.left += 1;
            continue;
        }
        // A rename where it can; across disks a copy, then the source
        // removed.
        let done = std::fs::rename(e.path(), &to).is_ok()
            || (std::fs::copy(e.path(), &to).is_ok() && std::fs::remove_file(e.path()).is_ok());
        if done {
            m.moved += 1;
        } else {
            m.left += 1;
        }
    }
    if m.left == 0 {
        let _ = std::fs::remove_dir(old);
    }
    Ok(m)
}

/// The history, loaded (the file store answers at once).
pub fn open() -> Option<Rc<Plays>> {
    let dir = plays_dir()?;
    // Linux's config and data folders differ: plays move before anything
    // records.
    if let Some(old) = crate::plays::default_dir()
        && old != dir
    {
        match move_plays(&old, &dir) {
            Ok(m) if m.left > 0 => eprintln!(
                "history: {} plays moved from {} to {}; {} left there (the same names are in \
                 both, or they could not be moved)",
                m.moved,
                old.display(),
                dir.display(),
                m.left
            ),
            Ok(_) => {}
            Err(e) => eprintln!(
                "history: moving {} to {}: {e}",
                old.display(),
                dir.display()
            ),
        }
    }
    let plays = Plays::new(Rc::new(FilePlayStore::new(dir)));
    plays.load(|r| {
        if let Err(e) = r {
            eprintln!("history: {e}");
        }
    });
    Some(plays)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_move_and_merge_never_over_newer_ones() {
        let base = std::env::temp_dir().join(format!("ddi-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (old, new) = (base.join("config/plays"), base.join("data/plays"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("1.json"), b"{}").unwrap();
        assert_eq!(move_plays(&old, &new).unwrap().moved, 1);
        assert!(new.join("1.json").is_file() && !old.exists());
        // Both hold plays: merged file by file, never over a newer one.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("1.json"), b"old").unwrap();
        std::fs::write(old.join("2.json"), b"{}").unwrap();
        let m = move_plays(&old, &new).unwrap();
        assert_eq!((m.moved, m.left, m.both), (1, 1, true));
        assert_eq!(std::fs::read(new.join("1.json")).unwrap(), b"{}");
        assert!(new.join("2.json").is_file() && !old.join("2.json").exists());
        assert!(old.join("1.json").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
