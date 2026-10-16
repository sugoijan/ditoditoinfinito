//! The first start's offer (`docs/plans/desktop.md`, D7): with no song
//! folder yet, the song folders of StepMania 5, ITGmania and OutFox found
//! in their usual places are offered, never read without the player's
//! choice. Only folder names are looked at: a place counts when it has a
//! `Songs` folder holding at least one folder.
//!
//! The places, per system:
//!
//! - StepMania 5 keeps its user files in one folder named after its
//!   product id (`StepMania 5`, `StepMania 5.1` from 5.1): on macOS in
//!   `~/Library/Application Support/`, on Windows in `%APPDATA%\`
//!   (`ArchHooks_MacOSX.mm` and `ArchHooks_Win32Static.cpp`,
//!   `MountUserFilesystems`), on Linux in `~/.stepmania-5.0` or
//!   `~/.stepmania-5.1` (`ArchHooks_Unix.cpp`). A portable install keeps
//!   them beside the program: Windows's installer suggests
//!   `C:\Games\StepMania 5`; macOS's builds are folders in
//!   `/Applications`.
//! - ITGmania, a fork of StepMania 5.1: `ITGmania` in the same places
//!   (`~/.itgmania` on Linux), installed in `C:\Games\ITGmania` on
//!   Windows by default.
//! - OutFox: portable by default (its folder wherever it was unpacked,
//!   `C:\Games\…` often; Steam's under `steamapps/common`), `~/.project-outfox`
//!   on Linux, `Project OutFox` in the user folders above otherwise.

use std::path::{Path, PathBuf};

/// The storage key of the player's answer (`added` or `declined`).
pub const OFFER_KEY: &str = "ddi.desktop.offer.v1";

/// A system's places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

impl Os {
    pub fn this() -> Os {
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

/// The folders the places are under.
#[derive(Clone, Debug, Default)]
pub struct Bases {
    pub home: Option<PathBuf>,
    /// macOS's `~/Library/Application Support`, Windows's `%APPDATA%`.
    pub app_data: Option<PathBuf>,
    /// Windows's system drive (`C:\`), for `Games\` and Steam.
    pub drive: Option<PathBuf>,
}

impl Bases {
    /// This system's.
    pub fn here() -> Bases {
        Bases {
            home: dirs::home_dir(),
            app_data: dirs::data_dir(),
            drive: std::env::var_os("SystemDrive")
                .map(|d| PathBuf::from(format!("{}\\", d.to_string_lossy()))),
        }
    }
}

/// User folders of the games, by name.
const USER_FOLDERS: &[&str] = &[
    "StepMania 5",
    "StepMania 5.1",
    "StepMania 5.2",
    "ITGmania",
    "Project OutFox",
];
/// Linux's dot folders.
const DOT_FOLDERS: &[&str] = &[
    ".stepmania-5.0",
    ".stepmania-5.1",
    ".stepmania-5.2",
    ".itgmania",
    ".project-outfox",
];
/// Beginnings of the names of installed (portable) games' folders.
const INSTALLED: &[&str] = &["stepmania", "itgmania", "outfox", "project outfox"];

/// Where to look on `os`: exact folders, and folders whose children
/// starting with one of [`INSTALLED`] are looked at.
fn places(os: Os, b: &Bases) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut exact = Vec::new();
    let mut parents = Vec::new();
    match os {
        Os::MacOs | Os::Windows => {
            if let Some(d) = &b.app_data {
                exact.extend(USER_FOLDERS.iter().map(|n| d.join(n)));
            }
        }
        Os::Linux => {
            if let Some(h) = &b.home {
                exact.extend(DOT_FOLDERS.iter().map(|n| h.join(n)));
            }
        }
    }
    match os {
        Os::MacOs => {
            parents.push(PathBuf::from("/Applications"));
            if let Some(h) = &b.home {
                parents.push(h.join("Applications"));
                parents.push(h.join("Library/Application Support/Steam/steamapps/common"));
            }
        }
        Os::Windows => {
            if let Some(c) = &b.drive {
                parents.push(c.join("Games"));
                parents.push(
                    c.join("Program Files (x86)")
                        .join("Steam")
                        .join("steamapps")
                        .join("common"),
                );
                parents.push(
                    c.join("Program Files")
                        .join("Steam")
                        .join("steamapps")
                        .join("common"),
                );
            }
        }
        Os::Linux => {
            if let Some(h) = &b.home {
                parents.push(h.join(".local/share/Steam/steamapps/common"));
                parents.push(h.join("Games"));
            }
        }
    }
    (exact, parents)
}

/// A child folder named `name` in any case.
fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
                && e.file_type().is_ok_and(|t| t.is_dir())
        })
        .map(|e| e.path())
}

/// Whether `dir` is a game's folder with songs: a `Songs` folder holding
/// at least one folder (names only).
fn has_songs(dir: &Path) -> bool {
    child(dir, "Songs").is_some_and(|s| {
        std::fs::read_dir(s)
            .map(|mut r| r.any(|e| e.is_ok_and(|e| e.path().is_dir())))
            .unwrap_or(false)
    })
}

/// The game folders with songs on `os` under `bases`, each once, in the
/// order of the places.
pub fn find(os: Os, bases: &Bases) -> Vec<PathBuf> {
    let (exact, parents) = places(os, bases);
    let mut out: Vec<PathBuf> = exact.into_iter().filter(|d| has_songs(d)).collect();
    for p in parents {
        let Ok(entries) = std::fs::read_dir(&p) else {
            continue;
        };
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| {
                e.file_name().to_str().is_some_and(|n| {
                    let n = n.to_ascii_lowercase();
                    INSTALLED.iter().any(|w| n.starts_with(w))
                })
            })
            .map(|e| e.path())
            .filter(|d| d.is_dir() && has_songs(d))
            .collect();
        found.sort();
        out.extend(found);
    }
    // Each folder once (a link, the same folder in two places).
    let mut seen = std::collections::HashSet::new();
    out.retain(|d| seen.insert(d.canonicalize().unwrap_or_else(|_| d.clone())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(root: &Path, p: &str) {
        std::fs::create_dir_all(root.join(p)).unwrap();
    }

    #[test]
    fn the_games_usual_folders_are_found_by_their_names() {
        let root = std::env::temp_dir().join(format!("ddi-offer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // macOS-like: one with songs, one without, one with an empty Songs.
        let support = root.join("mac/Library/Application Support");
        dir(&support, "StepMania 5/Songs/Pack");
        dir(&support, "ITGmania/Themes");
        dir(&support, "Project OutFox/songs");
        let mac = Bases {
            home: Some(root.join("mac")),
            app_data: Some(support.clone()),
            drive: None,
        };
        dir(&root, "mac/Applications/StepMania-5.0.12/Songs/Pack");
        dir(&root, "mac/Applications/Other/Songs/Pack");
        let found = find(Os::MacOs, &mac);
        assert!(found.contains(&support.join("StepMania 5")), "{found:?}");
        assert!(
            found.contains(&root.join("mac/Applications/StepMania-5.0.12")),
            "{found:?}"
        );
        assert!(
            !found
                .iter()
                .any(|f| f.ends_with("ITGmania") || f.ends_with("Other"))
        );
        assert!(!found.iter().any(|f| f.ends_with("Project OutFox")));
        // Windows-like: %APPDATA%, C:\Games.
        let win = Bases {
            home: None,
            app_data: Some(root.join("win/AppData")),
            drive: Some(root.join("win/C")),
        };
        dir(&root, "win/AppData/StepMania 5.1/Songs/P");
        dir(&root, "win/C/Games/ITGmania/Songs/P");
        dir(&root, "win/C/Games/OutFox 0.5.0 Alpha Win64/Songs/P");
        let found = find(Os::Windows, &win);
        assert_eq!(found.len(), 3, "{found:?}");
        // Linux-like: dot folders and Steam.
        let linux = Bases {
            home: Some(root.join("lin")),
            app_data: None,
            drive: None,
        };
        dir(&root, "lin/.itgmania/Songs/P");
        dir(&root, "lin/.stepmania-5.1/Songs");
        dir(
            &root,
            "lin/.local/share/Steam/steamapps/common/Project OutFox/Songs/P",
        );
        let found = find(Os::Linux, &linux);
        assert_eq!(
            found,
            vec![
                root.join("lin/.itgmania"),
                root.join("lin/.local/share/Steam/steamapps/common/Project OutFox"),
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
