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

thread_local! {
    /// The system pointer is hidden for the theme's own (`Shell::set_native_cursor`).
    static POINTER_HIDDEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The shell hid the system pointer for the theme's, or showed it again.
pub fn set_pointer_hidden(hidden: bool) {
    POINTER_HIDDEN.with(|h| h.set(hidden));
}

/// A system dialog, with the system pointer shown while it is open. On
/// macOS the theme's pointer hides it for the whole app (`NSCursor::hide`),
/// the dialog's window too, and the dialog runs inside the event that
/// opened it, before a blur or a leave could show it again.
pub fn dialog<T>(open: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "macos")]
    if POINTER_HIDDEN.with(|h| h.get()) {
        objc2_app_kit::NSCursor::unhide();
        let out = open();
        objc2_app_kit::NSCursor::hide();
        return out;
    }
    open()
}

/// Files (or a folder) the player picks; none when cancelled.
pub fn pick(folder: bool) -> Vec<std::path::PathBuf> {
    let dialog = rfd::FileDialog::new();
    if folder {
        self::dialog(|| dialog.pick_folder()).into_iter().collect()
    } else {
        // Packs come as zips (folders through the folder picker).
        let dialog = dialog.add_filter(
            ddi_l10n::msg::shell::file_zips().as_str(),
            &["zip", "ZIP", "smzip", "SMZIP"],
        );
        self::dialog(|| dialog.pick_files()).unwrap_or_default()
    }
}

thread_local! {
    /// What the next [`pick_music`] returns instead of asking (tests).
    static PICKED_MUSIC: std::cell::RefCell<Option<Option<std::path::PathBuf>>> =
        const { std::cell::RefCell::new(None) };
}

/// The music file the player picks for a new song (the chart editor's);
/// `None` when cancelled.
pub fn pick_music() -> Option<std::path::PathBuf> {
    if let Some(picked) = PICKED_MUSIC.with(|p| p.borrow_mut().take()) {
        return picked;
    }
    let extensions: Vec<String> = ddi_library::pack::AUDIO_EXTENSIONS
        .iter()
        .flat_map(|e| [e.to_string(), e.to_ascii_uppercase()])
        .collect();
    dialog(|| {
        rfd::FileDialog::new()
            .add_filter(ddi_l10n::msg::shell::file_music().as_str(), &extensions)
            .pick_file()
    })
}

/// Makes the next [`pick_music`] return `picked` without a dialog (tests;
/// `None`: as if cancelled).
pub fn inject_picked_music(picked: Option<std::path::PathBuf>) {
    PICKED_MUSIC.with(|p| *p.borrow_mut() = Some(picked));
}

pub fn copy(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .map_err(|e| e.to_string())
}

pub fn paste() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Starts `command`, trying again while its executable is busy. Linux
/// refuses to run a file another process holds open for writing, and a
/// just-written file is held briefly by any child forked in between that
/// has not reached its exec yet (tests write stand-in helpers while other
/// tests start processes).
pub fn spawn(command: &mut Command) -> std::io::Result<std::process::Child> {
    let mut tries = 0;
    loop {
        match command.spawn() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < 10 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(10 * tries));
            }
            spawned => return spawned,
        }
    }
}
