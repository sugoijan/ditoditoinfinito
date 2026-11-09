//! Song select's data from the library, the settings and the play
//! history: `ddi_ui::state::select_view` (shared with desktop since step
//! 7.1 of `docs/plans/desktop.md`) over the web's library, whose banners
//! and backgrounds are bundled files or stored ones; and the import
//! dialog's views.

use std::collections::{HashMap, HashSet};

use ddi_chart::Layout;
use ddi_ui::screens::data::{ImportReportView, ImportView, SelectData, SharedView};
use ddi_ui::state::history::History;
use ddi_ui::state::select_view::{self, Listed};
use wasm_bindgen::JsValue;

use crate::import::ImportReport;
use crate::settings::Settings;
use crate::songs::{ImportedSong, Library, file_key};
use ddi_ui::state::place::MenuPlace;

/// Layouts with charts in the library: the built-in ones in their order,
/// then the ones songs define themselves, by name.
pub(crate) fn library_styles(library: &Library) -> Vec<Layout> {
    select_view::library_styles(library.entries())
}

/// An imported song as the list shows it: stored files as `files:` keys,
/// its background its opening movie's still first (`crate::stills`; a
/// background left in a kept folder is not in the store). Bundled songs
/// are [`Listed::bundled`].
fn imported(s: &ImportedSong) -> Listed<'_> {
    let entry = &s.entry;
    // A share's song (`crate::share`): its pictures by URL, not removable.
    if crate::share::is_shared_id(&entry.id) {
        let url = |p: &ddi_library::path::RelPathBuf| crate::share::file_url(&entry.id, p.as_str());
        return Listed {
            entry,
            pack: Some(s.pack.clone()),
            pack_banner: None,
            banner: entry.banner.as_ref().and_then(url),
            background: entry.background.as_ref().and_then(url),
            jacket: entry.jacket.as_ref().and_then(url),
            cd_title: entry.cd_title.as_ref().and_then(url),
            removable: false,
        };
    }
    Listed {
        entry,
        pack: Some(s.pack.clone()),
        pack_banner: None,
        banner: entry
            .banner
            .as_ref()
            .map(|b| format!("files:{}", file_key(&entry.id, b))),
        background: s
            .still_file()
            .map(|key| format!("files:{key}"))
            .or_else(|| {
                entry
                    .background
                    .as_ref()
                    .filter(|b| !s.links.contains_key(b.as_str()))
                    .map(|b| format!("files:{}", file_key(&entry.id, b)))
            }),
        jacket: entry
            .jacket
            .as_ref()
            .map(|b| format!("files:{}", file_key(&entry.id, b))),
        cd_title: entry
            .cd_title
            .as_ref()
            .map(|b| format!("files:{}", file_key(&entry.id, b))),
        removable: true,
    }
}

/// Song select's data now.
pub(crate) fn select_data(
    library: &Library,
    settings: &Settings,
    history: &History,
    folders: &HashMap<String, JsValue>,
    place: &MenuPlace,
) -> SelectData {
    let songs: Vec<Listed<'_>> = library
        .bundled
        .iter()
        .map(Listed::bundled)
        .chain(library.imported.iter().map(imported))
        .collect();
    // A pack (told apart ignoring case) can be read again when one of its
    // songs came from a folder still known.
    let rereadable_packs: HashSet<String> = library
        .imported
        .iter()
        .filter(|s| s.folder.as_ref().is_some_and(|f| folders.contains_key(f)))
        .map(|s| s.pack.to_lowercase())
        .collect();
    let rereadable = |pack: &str| rereadable_packs.contains(&pack.to_lowercase());
    let mut data = select_view::select_data(&songs, &rereadable, settings, history, place);
    for pack in &mut data.packs {
        let Some(name) = &pack.name else { continue };
        let key = crate::songs::pack_art_key(name);
        if library.pack_art.contains(&key) {
            pack.banner = Some(format!("files:{key}"));
        }
    }
    data
}

/// What song select says of the share it shows: its pack's name and
/// the time it has left.
pub(crate) fn shared_view(share: &crate::share::Share) -> SharedView {
    SharedView::new(
        share.pack.clone(),
        share.expires_at,
        js_sys::Date::now() / 1000.0,
    )
}

/// The import dialog's state now.
pub(crate) fn import_view(settings: &Settings) -> ImportView {
    let (running, report, _) = crate::import::status();
    ImportView {
        danoni_urls: false,
        danoni_cancel: false,
        danoni: Default::default(),
        folders: crate::web::folders::supported(),
        copy: settings.import_copy,
        videos: settings.import_videos,
        details: settings.import_details,
        running: running.map(Into::into),
        report: report.map(|r| report_view(&r, settings.import_details)),
    }
}

/// An import's report as the dialog shows it: its values, worded by
/// `ddi_ui::screens::import_report` (the details for pack authors with
/// `details`).
pub(crate) fn report_view(r: &ImportReport, details: bool) -> ImportReportView {
    ImportReportView::Files {
        report: r.clone(),
        details,
    }
}
