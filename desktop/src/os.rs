//! What desktop asks of the system: links opened by its own command (no
//! extra crate), files picked through rfd, text through arboard.

use std::process::Command;

/// Opens `url` in the system's browser.
pub fn open_url(url: &str) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!("not a web link: {url}"));
    }
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

/// Files (or a folder) the player picks; none when cancelled.
pub fn pick(folder: bool) -> Vec<std::path::PathBuf> {
    let dialog = rfd::FileDialog::new();
    if folder {
        dialog.pick_folder().into_iter().collect()
    } else {
        dialog.pick_files().unwrap_or_default()
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
