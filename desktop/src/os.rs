//! What desktop asks of the system: links opened by its own command (no
//! extra crate), files picked through rfd, text through arboard.

use std::process::Command;

/// Opens `url` in the system's browser; a link relative to the site (the
/// credits' `video/…` files, served next to the web app) as the file it
/// is built from in this checkout (`target/…`), when it is there.
pub fn open_url(url: &str) -> Result<(), String> {
    let web = url.starts_with("https://") || url.starts_with("http://");
    let url = if web {
        url.to_string()
    } else {
        local_file(url)
            .ok_or_else(|| format!("not on this machine: {url}"))?
            .to_string_lossy()
            .into_owned()
    };
    let url = url.as_str();
    let mut cmd = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut c = Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        Command::new("xdg-open")
    };
    let mut child = cmd.arg(url).spawn().map_err(|e| e.to_string())?;
    // Reaped once it is done (the command hands the link over and quits).
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Where a site-relative link's file is in this checkout: the site's
/// files are built under `target/` (`target/video/…` is served as
/// `video/…`), the rest as they are in the repository.
fn local_file(link: &str) -> Option<std::path::PathBuf> {
    if link.contains("://") || link.starts_with('/') || link.split('/').any(|p| p == "..") {
        return None;
    }
    let link = link.split(['#', '?']).next().unwrap_or(link);
    [
        std::path::Path::new("target").join(link),
        std::path::Path::new("target/credits").join(link),
        std::path::PathBuf::from(link),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .and_then(|p| p.canonicalize().ok())
}

/// Files (or a folder) the player picks; none when cancelled.
pub fn pick(folder: bool) -> Vec<std::path::PathBuf> {
    let dialog = rfd::FileDialog::new();
    if folder {
        dialog.pick_folder().into_iter().collect()
    } else {
        // Packs come as zips (folders through the folder picker).
        dialog
            .add_filter("Zip archives", &["zip", "ZIP"])
            .pick_files()
            .unwrap_or_default()
    }
}

pub fn copy(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .map_err(|e| e.to_string())
}

pub fn paste() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}
