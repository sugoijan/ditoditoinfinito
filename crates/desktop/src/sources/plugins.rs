//! Options → Plugins on desktop (`docs/plans/source-plugins.md`, stage 4;
//! the section is `ddi_ui::screens::options_plugins`): the plugins folder
//! read off the frame ([`scan`]: manifests read and programs hashed,
//! nothing started), what the section shows from it and from the sources
//! the game holds ([`view`]), and the switches carried out at once: the
//! same file `--source-check` reads (`plugins.json`) and This computer's
//! setting `--sources` writes, the host and Find songs following without a
//! restart ([`switch_plugin`], [`switch_this_computer`], [`installed_again`]).
//!
//! The game never downloads, installs, updates or starts a plugin here: a
//! plugin switched on starts when Find songs first asks it for something,
//! and switched off it is stopped.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ddi_source_host::Installed;
use ddi_source_host::host::Stop;
use ddi_source_host::installed::{Found, Problem};
use ddi_source_host::protocol::{Capability, InputKind, Reach, VERSIONS};
use ddi_source_host::switches::Switches;
use ddi_ui::screens::find_songs::Reach as Shown;
use ddi_ui::screens::options_plugins::{Can, PluginView, PluginsView, Refusal, RefusedView, State};

use super::{Setting, Sources, this_computer};
use crate::songs::Songs;

/// What the plugins folder held when last read.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub found: Vec<Found>,
}

impl Scan {
    /// The plugins the game could start, in their folders' order.
    pub fn installed(&self) -> Vec<Installed> {
        self.found
            .iter()
            .filter_map(|f| match f {
                Found::Plugin(p) => Some(p.clone()),
                Found::Refused { .. } => None,
            })
            .collect()
    }
}

/// The plugins folder read: every manifest, every program hashed. Off the
/// frame (a program may be large).
pub fn scan() -> Scan {
    Scan {
        found: super::plugins_dir()
            .map(|d| ddi_source_host::discover(&d))
            .unwrap_or_default(),
    }
}

/// The plugins switched on, as the switches' file says.
pub fn switched_on() -> BTreeSet<String> {
    super::switches_path()
        .map(|p| Switches::read(&p).on)
        .unwrap_or_default()
}

/// What the section shows: `scan` as last read (`None`: being read for
/// the first time), the switches, This computer's setting, and what the
/// sources the game holds know of each plugin this run.
pub fn view(scan: Option<&Scan>, looking: bool, songs: Option<&Songs>) -> PluginsView {
    let setting = Setting::load();
    let on = switched_on();
    let sources = songs.map(|s| s.sources.borrow());
    let host = sources
        .as_ref()
        .and_then(|s| s.as_ref())
        .map(|s: &Sources| &s.host);
    let language = ddi_l10n::current().tag;
    let mut out = PluginsView {
        folder: super::plugins_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_default(),
        looking: looking || scan.is_none(),
        this_computer: setting.this_computer,
        folders: setting
            .folders
            .iter()
            .map(|f| f.display().to_string())
            .collect(),
        ..PluginsView::default()
    };
    for f in scan.map(|s| s.found.as_slice()).unwrap_or_default() {
        match f {
            Found::Plugin(p) => {
                let m = &p.manifest;
                let id = p.id();
                let mut can: Vec<Can> = m
                    .capabilities
                    .iter()
                    .filter(|c| !c.reserved() && **c != Capability::Input)
                    .map(|c| match c {
                        Capability::Search => Can::Search,
                        Capability::Filters => Can::Filters,
                        Capability::Browse => Can::Browse,
                        Capability::Metadata => Can::Describe,
                        _ => Can::Fetch,
                    })
                    .collect();
                can.extend(m.inputs.iter().map(|i| match i {
                    InputKind::Folder => Can::Folder,
                    InputKind::Url => Can::Url,
                    InputKind::Link => Can::Link,
                    InputKind::Text => Can::Text,
                }));
                let state = if !m.protocol.iter().any(|v| VERSIONS.contains(v)) {
                    State::Incompatible {
                        plugin_newer: m.protocol.iter().max() > VERSIONS.iter().max(),
                    }
                } else if host.is_some_and(|h| h.is_off_for_the_run(id)) {
                    match host.and_then(|h| h.off_why(id)) {
                        Some(Stop::Incompatible { plugin_newer }) => {
                            State::Incompatible { plugin_newer }
                        }
                        Some(Stop::InvalidHello(_)) => State::NotAsDeclared,
                        _ => State::StoppedWorking,
                    }
                } else {
                    State::Ready
                };
                out.plugins.push(PluginView {
                    id: id.to_string(),
                    name: m.name.get(language).to_string(),
                    version: m.version.clone(),
                    publisher: m.publisher.clone(),
                    homepage: m.homepage.clone(),
                    can,
                    links: m
                        .links
                        .iter()
                        .map(|l| format!("{}{}", l.host, l.path))
                        .collect(),
                    reach: match m.reach {
                        Reach::Play => Shown::Play,
                        Reach::Rooms => Shown::Rooms,
                        Reach::Keep => Shown::Keep,
                    },
                    sha256: p.sha256.clone(),
                    on: on.contains(id),
                    state,
                });
            }
            Found::Refused { dir, problem } => out.refused.push(RefusedView {
                folder: dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                why: match problem {
                    Problem::NoManifest(_) => Refusal::NoManifest,
                    Problem::Manifest(why) => Refusal::Manifest(why.clone()),
                    Problem::NotItsFolder => Refusal::NotItsFolder,
                    Problem::NoExecutable(path) => Refusal::NoProgram {
                        platform: ddi_source_host::installed::platform(),
                        path: path.display().to_string(),
                    },
                },
            }),
        }
    }
    out
}

/// Plugin `id` switched on or off: the switches' file written, and the
/// host told (switched off, its process stopped). Where the game holds no
/// sources yet and this run may (the window's), switching one on makes
/// them, with `installed`. Nothing is started.
pub fn switch_plugin(
    songs: Option<&Songs>,
    installed: &[Installed],
    id: &str,
    on: bool,
) -> std::io::Result<()> {
    let path =
        super::switches_path().ok_or_else(|| std::io::Error::other("no configuration folder"))?;
    Switches::set(&path, id, on)?;
    let Some(songs) = songs else {
        return Ok(());
    };
    let made = match songs.sources.borrow_mut().as_mut() {
        Some(s) => {
            s.host.switch(id, on);
            true
        }
        None => false,
    };
    if !made && on {
        let setting = Setting::load();
        songs.make_sources(
            installed.to_vec(),
            &switched_on(),
            setting.this_computer.then_some(setting.folders),
        );
    }
    Ok(())
}

/// This computer switched on or off: its setting written (as `--sources`
/// writes it), and the sources the game holds given it or let go of it.
pub fn switch_this_computer(songs: Option<&Songs>, installed: &[Installed], on: bool) {
    super::save_switch(on);
    let Some(songs) = songs else {
        return;
    };
    let folders = Setting::load().folders;
    let made = match songs.sources.borrow_mut().as_mut() {
        Some(s) => {
            match (on, s.this_computer.is_some()) {
                (true, false) => {
                    let t = std::sync::Arc::new(this_computer::ThisComputer::new(folders.clone()));
                    s.host.add_builtin(t.clone());
                    s.this_computer = Some(t);
                }
                (false, true) => {
                    s.host.remove_builtin(this_computer::ID);
                    s.this_computer = None;
                }
                _ => {}
            }
            true
        }
        None => false,
    };
    if !made && on {
        songs.make_sources(installed.to_vec(), &switched_on(), Some(folders));
    }
}

/// The plugins folder read again: the host given what is installed now.
pub fn installed_again(songs: Option<&Songs>, installed: Vec<Installed>) {
    if let Some(songs) = songs
        && let Some(s) = songs.sources.borrow_mut().as_mut()
    {
        s.host.set_installed(installed, &switched_on());
    }
}

/// Everything reset (Options → Data and privacy → Everything): This
/// computer and every plugin switched off, the switches' file emptied.
pub fn all_off(songs: Option<&Songs>) {
    let on = switched_on();
    for id in &on {
        let _ = switch_plugin(songs, &[], id, false);
    }
    switch_this_computer(songs, &[], false);
}

/// The plugins folder, made where it is missing, shown in the system's
/// file browser.
pub fn open_folder() -> Result<PathBuf, String> {
    let dir = super::plugins_dir().ok_or_else(|| "no data folder".to_string())?;
    crate::os::open_folder(&dir)?;
    Ok(dir)
}
