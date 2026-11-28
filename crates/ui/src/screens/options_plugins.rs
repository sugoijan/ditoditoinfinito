//! Options → Plugins (`docs/plans/source-plugins.md`, stage 4; desktop
//! only, the module absent from the web's build): a section the desktop
//! shell adds to Options (`World::sections`), at the Advanced level alone
//! (`docs/plans/ui-levels.md`, section 6), made from [`PluginsView`], what
//! the shell read of the plugins folder and its song sources.
//!
//! - **Its lead** says once what a plugin is (a program that runs with
//!   the player's access, which the game cannot check), how one is added
//!   and that the game never fetches one itself.
//! - **The plugins folder**: Open folder (the system's file browser) and
//!   Look again (the folder read again).
//! - **This computer**, the source built into the game: its switch, the
//!   same setting `--sources` writes; its folders are chosen on Find songs.
//! - **Each installed plugin**: its switch (switching one on asks first,
//!   the shell's), beside it its version, publisher and digest as its
//!   folder says or what keeps it from running now; its description
//!   saying what it is now, what it does, how far its songs may go, its
//!   page and its program's whole SHA-256; a row opening its page. A
//!   folder that is no plugin the game takes says why.
//!
//! Every switch is Off by default: at Basic the section shows only its
//! count of switches on (the levels' rule that nothing set acts unseen).
//! Nothing here starts a plugin: what it does is read from its manifest.
//! Every text is a message or data: a plugin's name, version, publisher,
//! page and hosts, a folder's name and paths are data.

use ddi_l10n::UiText;
use ddi_l10n::msg::{common, sources as m};

use super::data::{OptionDef, OptionKind, OptionSection};
use super::find_songs::Reach;
use crate::state::settings::Level;

/// The section's id (its English name).
pub const SECTION: &str = "Plugins";
/// The plugins folder's row: Open folder, Look again.
pub const FOLDER: &str = "plugins.folder";
/// This computer's switch.
pub const THIS_COMPUTER: &str = "plugins.this-computer";
/// The row saying no plugin is installed.
pub const NONE: &str = "plugins.none";
/// A plugin's switch, by its id after the prefix.
pub const PLUGIN: &str = "plugins.plugin.";
/// A plugin's page's row, by its id after the prefix.
pub const PAGE: &str = "plugins.page.";
/// A folder that is not a plugin the game takes, by its place in the list.
pub const REFUSED: &str = "plugins.refused.";

/// How many characters of a program's SHA-256 its row shows (the whole in
/// its description).
pub const SHORT_SHA: usize = 12;

/// What the section shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PluginsView {
    /// The plugins folder's path (data), where there is one.
    pub folder: String,
    /// The folder is being read (the page opened, Look again).
    pub looking: bool,
    /// This computer is on, and the folders it searches (data).
    pub this_computer: bool,
    pub folders: Vec<String>,
    pub plugins: Vec<PluginView>,
    pub refused: Vec<RefusedView>,
}

/// An installed plugin, as its folder says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PluginView {
    pub id: String,
    /// Its name in the player's language (else English), its version and
    /// publisher, as its manifest says (data, cleaned).
    pub name: String,
    pub version: String,
    pub publisher: String,
    /// Its page, where its manifest names one (an http or https link).
    pub homepage: Option<String>,
    /// What it does, from its manifest.
    pub can: Vec<Can>,
    /// The sites and paths it claims links of (data).
    pub links: Vec<String>,
    /// How far its songs may go.
    pub reach: Reach,
    /// Its program's SHA-256, in lowercase hex.
    pub sha256: String,
    /// Switched on.
    pub on: bool,
    pub state: State,
}

/// What a plugin does, as its manifest declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Can {
    Search,
    Filters,
    Browse,
    Describe,
    Fetch,
    /// What the player may give it.
    Folder,
    Url,
    Link,
    Text,
}

/// What a plugin is now, beyond its switch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    /// Nothing keeps it from running.
    #[default]
    Ready,
    /// It ended badly three times this run: off until the game starts
    /// again.
    StoppedWorking,
    /// It did not start as its manifest says (its Hello): off for the run.
    NotAsDeclared,
    /// It speaks no protocol version the game speaks (from its manifest,
    /// or its Hello): it cannot run here.
    Incompatible { plugin_newer: bool },
}

/// A folder in the plugins folder that is no plugin the game takes.
#[derive(Clone, Debug, PartialEq)]
pub struct RefusedView {
    /// Its folder's name (data).
    pub folder: String,
    pub why: Refusal,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// No `plugin.toml` it can read.
    NoManifest,
    /// Its `plugin.toml` does not read: why, in the reader's words (data).
    Manifest(String),
    /// Its folder is not named as its manifest says.
    NotItsFolder,
    /// No program for this computer: this computer's kind and where it was
    /// looked for (data).
    NoProgram { platform: String, path: String },
}

/// The section, every row Advanced.
pub fn section(view: &PluginsView) -> OptionSection {
    let mut rows = Vec::new();
    let note: UiText = if view.looking {
        m::plugins_folder_looking()
    } else if view.plugins.is_empty() {
        m::plugins_folder_none()
    } else {
        m::plugins_folder_count(view.plugins.len())
    };
    rows.push(row(
        FOLDER,
        m::plugins_folder().into(),
        OptionKind::Actions(vec![
            m::plugins_open_folder().into(),
            m::plugins_look_again().into(),
        ]),
        0,
        note.into(),
        m::plugins_folder_description(view.folder.as_str()).into(),
    ));
    let mut this_computer = switch(
        THIS_COMPUTER,
        m::this_computer().into(),
        view.this_computer,
        String::new(),
        if view.folders.is_empty() {
            m::plugins_this_computer_description()
        } else {
            m::plugins_this_computer_folders(view.folders.join(" · "))
        }
        .into(),
    );
    this_computer.heading = Some(m::plugins_built_in_heading().into());
    rows.push(this_computer);
    let first = rows.len();
    if view.plugins.is_empty() && view.refused.is_empty() && !view.looking {
        rows.push(row(
            NONE,
            String::new(),
            OptionKind::Actions(Vec::new()),
            0,
            m::plugins_none().into(),
            m::plugins_none_description().into(),
        ));
    }
    for p in &view.plugins {
        rows.extend(plugin_rows(p));
    }
    for (i, r) in view.refused.iter().enumerate() {
        rows.push(refused_row(i, r));
    }
    if let Some(r) = rows.get_mut(first) {
        r.heading = Some(m::plugins_installed_heading().into());
    }
    OptionSection {
        id: SECTION.into(),
        name: m::plugins_section().into(),
        options: rows,
        lead: m::plugins_lead().into(),
    }
}

/// A plugin's switch (or, where it cannot run here at all, its row
/// saying why) and its page's row.
fn plugin_rows(p: &PluginView) -> Vec<OptionDef> {
    let name: String = UiText::data(&p.name).into();
    let description = description(p);
    let state: Option<UiText> = match p.state {
        State::Ready => None,
        State::StoppedWorking => Some(m::plugins_state_stopped()),
        State::NotAsDeclared => Some(m::plugins_state_not_as_declared()),
        State::Incompatible { plugin_newer: true } => Some(m::plugins_state_newer()),
        State::Incompatible {
            plugin_newer: false,
        } => Some(m::plugins_state_older()),
    };
    let note: String = state
        .unwrap_or_else(|| {
            let short: String = p.sha256.chars().take(SHORT_SHA).collect();
            m::plugins_version_by(p.version.as_str(), p.publisher.as_str(), short)
        })
        .into();
    let mut out = Vec::new();
    let id = format!("{PLUGIN}{}", p.id);
    if matches!(p.state, State::Incompatible { .. }) {
        // Nothing to switch: it cannot run with this game.
        out.push(row(
            &id,
            name,
            OptionKind::Actions(Vec::new()),
            0,
            note,
            description,
        ));
    } else {
        out.push(switch(&id, name, p.on, note, description));
    }
    if let Some(page) = &p.homepage {
        out.push(row(
            &format!("{PAGE}{}", p.id),
            String::new(),
            OptionKind::Actions(vec![m::plugins_open_page().into()]),
            0,
            UiText::data(page).into(),
            m::plugins_page_description(page.as_str()).into(),
        ));
    }
    out
}

/// A plugin's sentence: what it is now, its version and publisher, what it
/// does, how far its songs may go, its page and its whole digest.
fn description(p: &PluginView) -> String {
    let state = match (p.state, p.on) {
        (State::Ready, false) => m::plugins_now_off(),
        (State::Ready, true) => m::plugins_now_on(),
        (State::StoppedWorking, _) => m::plugins_now_stopped(),
        (State::NotAsDeclared, _) => m::plugins_now_not_as_declared(),
        (State::Incompatible { plugin_newer: true }, _) => m::plugins_now_newer(),
        (
            State::Incompatible {
                plugin_newer: false,
            },
            _,
        ) => m::plugins_now_older(),
    };
    let can: Vec<String> = p
        .can
        .iter()
        .map(|c| {
            String::from(match c {
                Can::Search => m::plugins_can_search(),
                Can::Filters => m::plugins_can_filters(),
                Can::Browse => m::plugins_can_browse(),
                Can::Describe => m::plugins_can_describe(),
                Can::Fetch => m::plugins_can_fetch(),
                Can::Folder => m::plugins_can_folder(),
                Can::Url => m::plugins_can_url(),
                Can::Text => m::plugins_can_text(),
                Can::Link => m::plugins_can_link(UiText::data(p.links.join(", "))),
            })
        })
        .collect();
    let reach = match p.reach {
        Reach::Play => m::plugins_reach_play(),
        Reach::Rooms => m::plugins_reach_rooms(),
        Reach::Keep => m::plugins_reach_keep(),
    };
    let page = match &p.homepage {
        Some(h) => m::plugins_page(h.as_str()),
        None => m::plugins_no_page(),
    };
    m::plugins_description(
        state,
        p.version.as_str(),
        p.publisher.as_str(),
        can.join(" · "),
        reach,
        page,
        p.sha256.as_str(),
    )
    .into()
}

fn refused_row(i: usize, r: &RefusedView) -> OptionDef {
    let (note, description) = match &r.why {
        Refusal::NoManifest => (
            m::plugins_refused_no_manifest(),
            m::plugins_refused_no_manifest_description(),
        ),
        Refusal::Manifest(why) => (
            m::plugins_refused_manifest(),
            m::plugins_refused_manifest_description(why.as_str()),
        ),
        Refusal::NotItsFolder => (
            m::plugins_refused_folder(),
            m::plugins_refused_folder_description(),
        ),
        Refusal::NoProgram { platform, path } => (
            m::plugins_refused_program(),
            m::plugins_refused_program_description(platform.as_str(), path.as_str()),
        ),
    };
    row(
        &format!("{REFUSED}{i}"),
        UiText::data(&r.folder).into(),
        OptionKind::Actions(Vec::new()),
        0,
        note.into(),
        description.into(),
    )
}

/// A switch, Off by default: counted at Basic where it is on.
fn switch(id: &str, label: String, on: bool, note: String, description: String) -> OptionDef {
    let mut r = row(
        id,
        label,
        OptionKind::Choices(vec![common::off().into(), common::on().into()]),
        usize::from(on),
        note,
        description,
    );
    r.default = Some(0);
    r
}

fn row(
    id: &str,
    label: String,
    kind: OptionKind,
    index: usize,
    note: String,
    description: String,
) -> OptionDef {
    OptionDef {
        id: id.into(),
        label,
        kind,
        index,
        note,
        description,
        heading: None,
        level: Level::Advanced,
        default: None,
    }
}

/// The plugin a switch's or a page's row is of.
pub fn plugin_of(id: &str) -> Option<&str> {
    id.strip_prefix(PLUGIN).or_else(|| id.strip_prefix(PAGE))
}

/// A section in every state a row can be in (the tests' and the
/// `qps-ploc` walk's).
#[cfg(test)]
pub(crate) fn sample_view() -> PluginsView {
    let plugin = |id: &str, on: bool, state: State| PluginView {
        id: id.into(),
        name: format!("Test {id}"),
        version: "1.0.0".into(),
        publisher: "Tests".into(),
        homepage: Some(format!("https://example.org/{id}")),
        can: vec![
            Can::Search,
            Can::Filters,
            Can::Browse,
            Can::Describe,
            Can::Fetch,
            Can::Folder,
            Can::Url,
            Can::Link,
            Can::Text,
        ],
        links: vec!["example.org/songs".into()],
        reach: Reach::Rooms,
        sha256: "ab".repeat(32),
        on,
        state,
    };
    let mut other = plugin("plain", true, State::Ready);
    other.homepage = None;
    other.reach = Reach::Play;
    PluginsView {
        folder: "/home/plugins".into(),
        looking: false,
        this_computer: true,
        folders: vec!["/a".into(), "/b".into()],
        plugins: vec![
            plugin("good", false, State::Ready),
            other,
            plugin("tired", true, State::StoppedWorking),
            plugin("liar", true, State::NotAsDeclared),
            plugin("future", false, State::Incompatible { plugin_newer: true }),
            plugin(
                "past",
                false,
                State::Incompatible {
                    plugin_newer: false,
                },
            ),
        ],
        refused: vec![
            RefusedView {
                folder: "broken".into(),
                why: Refusal::Manifest("expected `=`".into()),
            },
            RefusedView {
                folder: "empty".into(),
                why: Refusal::NoManifest,
            },
            RefusedView {
                folder: "misnamed".into(),
                why: Refusal::NotItsFolder,
            },
            RefusedView {
                folder: "elsewhere".into(),
                why: Refusal::NoProgram {
                    platform: "aarch64-macos".into(),
                    path: "/home/plugins/elsewhere/bin/aarch64-macos/x".into(),
                },
            },
        ],
    }
}

#[cfg(test)]
#[path = "options_plugins_tests.rs"]
mod tests;
