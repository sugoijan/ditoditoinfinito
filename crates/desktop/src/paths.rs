//! Where desktop keeps what it keeps (D10): the platform's configuration,
//! data and cache folders, or, when `DDI_HOME` names a folder, its
//! `config/`, `data/` and `cache/` instead. Every run that is not the
//! player's own (an agent's, a test's) sets `DDI_HOME` to a scratch
//! folder, so the player's settings, history and library are never
//! touched.

use std::path::{Path, PathBuf};

/// `DDI_HOME`, when set (and not empty).
pub fn home() -> Option<PathBuf> {
    std::env::var_os("DDI_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The configuration folder (`storage.json`): `~/Library/Application
/// Support/DitoDitoInfinito` on macOS, `%APPDATA%\DitoDitoInfinito` on
/// Windows, `$XDG_CONFIG_HOME/ditoditoinfinito` (or `~/.config/…`)
/// elsewhere; `$DDI_HOME/config`.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(h) = home() {
        return Some(h.join("config"));
    }
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    Some(if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support/DitoDitoInfinito")
    } else if cfg!(windows) {
        env("APPDATA")?.join("DitoDitoInfinito")
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))?
            .join("ditoditoinfinito")
    })
}

/// The data folder (the play history): the platform's, or
/// `$DDI_HOME/data`.
pub fn data_dir() -> Option<PathBuf> {
    match home() {
        Some(h) => Some(h.join("data")),
        None => Some(dirs::data_dir()?.join(crate::history::APP_FOLDER)),
    }
}

/// The cache folder (the library's list, previews, compiled movies'
/// decoder): the platform's, or `$DDI_HOME/cache`.
pub fn cache_dir() -> Option<PathBuf> {
    match home() {
        Some(h) => Some(h.join("cache")),
        None => Some(dirs::cache_dir()?.join(crate::history::APP_FOLDER)),
    }
}

/// Writes `bytes` to `path` whole or not at all: its folder made, a
/// synced temporary file beside it (`<name>.<process>.tmp`, removed on a
/// failure) renamed over it.
pub fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let written = std::fs::File::create(&tmp)
        .and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// The folders a package keeps the game's own files in (`assets/`,
/// `video/`, `danoni-importer/`), nearest first: the executable's folder
/// (the relocatable folder, a build's profile folder) and, for an
/// executable in a macOS app's `Contents/MacOS/`, the app's
/// `Contents/Helpers/` (executables) and `Contents/Resources/` (the rest;
/// `docs/plans/packaging.md`).
pub fn package_dirs(exe: &Path) -> Vec<PathBuf> {
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    let mut dirs = vec![dir.to_path_buf()];
    if dir.file_name().is_some_and(|n| n == "MacOS")
        && let Some(contents) = dir.parent()
        && contents.file_name().is_some_and(|n| n == "Contents")
    {
        dirs.push(contents.join("Helpers"));
        dirs.push(contents.join("Resources"));
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_bundle_adds_its_helpers_and_resources() {
        let app = Path::new("/Applications/Dito Dito Infinito.app/Contents");
        assert_eq!(
            package_dirs(&app.join("MacOS/ditoditoinfinito")),
            [
                app.join("MacOS"),
                app.join("Helpers"),
                app.join("Resources")
            ]
        );
        // A plain folder, even one named MacOS outside a bundle.
        assert_eq!(
            package_dirs(Path::new("/games/ddi/ditoditoinfinito")),
            [PathBuf::from("/games/ddi")]
        );
        assert_eq!(
            package_dirs(Path::new("/games/MacOS/ditoditoinfinito")),
            [PathBuf::from("/games/MacOS")]
        );
    }

    #[test]
    fn a_file_is_written_whole_into_its_folder() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/write-whole");
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("a/b.json");
        write_whole(&file, b"one").unwrap();
        write_whole(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(dir.join("a")).unwrap().count(), 1);
        // A folder in the way: nothing written, no temporary file left.
        std::fs::create_dir_all(dir.join("c")).unwrap();
        assert!(write_whole(&dir.join("c"), b"x").is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    }
}
