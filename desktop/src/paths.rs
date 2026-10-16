//! Where desktop keeps what it keeps (D10): the platform's configuration,
//! data and cache folders, or, when `DDI_HOME` names a folder, its
//! `config/`, `data/` and `cache/` instead. Every run that is not the
//! player's own (an agent's, a test's) sets `DDI_HOME` to a scratch
//! folder, so the player's settings, history and library are never
//! touched.

use std::path::PathBuf;

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
