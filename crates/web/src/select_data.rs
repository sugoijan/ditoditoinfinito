//! Song select's data from the library, the settings and the play
//! history: `ddi_ui::state::select_view` (shared with desktop since step
//! 7.1 of `docs/plans/desktop.md`) over the web's library, whose banners
//! and backgrounds are bundled files or stored ones; and the import
//! dialog's views.

use std::collections::{HashMap, HashSet};

use ddi_chart::Layout;
use ddi_ui::screens::data::{ImportReportView, ImportView, SelectData};
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
    select_view::library_styles(
        library
            .bundled
            .iter()
            .chain(library.imported.iter().map(|s| &s.entry)),
    )
}

/// An imported song as the list shows it: stored files as `files:` keys,
/// its background its opening movie's still first (`crate::stills`; a
/// background left in a kept folder is not in the store). Bundled songs
/// are [`Listed::bundled`].
fn imported(s: &ImportedSong) -> Listed {
    let entry = &s.entry;
    Listed {
        entry: entry.clone(),
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
    let songs: Vec<Listed> = library
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
        let key = crate::songs::pack_art_key(&pack.name);
        if library.pack_art.contains(&key) {
            pack.banner = Some(format!("files:{key}"));
        }
    }
    data
}

/// The import dialog's state now.
pub(crate) fn import_view(settings: &Settings) -> ImportView {
    let (running, report, _) = crate::import::status();
    ImportView {
        platform: ddi_ui::screens::data::Platform::Web,
        danoni_urls: false,
        danoni_cancel: false,
        danoni: Default::default(),
        folders: crate::web::folders::supported(),
        copy: settings.import_copy,
        videos: settings.import_videos,
        details: settings.import_details,
        running,
        report: report.map(|r| report_view(&r, settings.import_details)),
    }
}

/// An import's report as the dialog shows it (what the song list's
/// import panel said).
pub(crate) fn report_view(r: &ImportReport, details: bool) -> ImportReportView {
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut parts = Vec::new();
    if !r.imported.is_empty() {
        parts.push(format!(
            "Imported {}",
            plural(r.imported.len(), "song", "songs")
        ));
    }
    if !r.noteskins.is_empty() {
        parts.push(format!(
            "stored {} (choose it under Options, Arrows)",
            plural(r.noteskins.len(), "noteskin", "noteskins")
        ));
    }
    if r.shared_images > 0 {
        parts.push(format!(
            "stored {}",
            plural(
                r.shared_images,
                "shared background image",
                "shared background images"
            )
        ));
    }
    if r.shared_videos > 0 {
        parts.push(format!(
            "kept {}",
            plural(
                r.shared_videos,
                "shared background video",
                "shared background videos"
            )
        ));
    }
    if r.shared_skipped > 0 {
        parts.push(format!(
            "left out {} from shared folders",
            plural(r.shared_skipped, "other file", "other files")
        ));
    }
    if r.videos > 0 {
        parts.push(format!(
            "{} will play",
            plural(r.videos, "background video", "background videos")
        ));
    }
    if r.videos_left_out > 0 {
        parts.push(format!(
            "left out {} (\"also store background videos\" is off)",
            plural(r.videos_left_out, "background video", "background videos")
        ));
    }
    if r.removed > 0 {
        parts.push(format!(
            "removed {} no longer in the folder",
            plural(r.removed, "song", "songs")
        ));
    }
    if !r.warnings.is_empty() {
        parts.push(plural(r.warnings.len(), "warning", "warnings"));
    }
    if !r.skipped.is_empty() {
        parts.push(format!("skipped {}", r.skipped.len()));
    }
    if details && !r.notes.is_empty() {
        parts.push(plural(r.notes.len(), "detail", "details"));
    }
    let summary = if parts.is_empty() {
        r.error
            .clone()
            .unwrap_or_else(|| "No simfiles found in what was picked.".into())
    } else {
        let mut text = parts.join("; ");
        if let Some(first) = text.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        text + "."
    };
    let error = r.imported.is_empty()
        && r.noteskins.is_empty()
        && r.shared_images == 0
        && r.shared_videos == 0
        && r.shared_skipped == 0
        && r.videos_left_out == 0
        && r.removed == 0;
    let lines = |items: &[(String, String)]| -> Vec<String> {
        items
            .iter()
            .map(|(what, why)| format!("{what}: {why}"))
            .collect()
    };
    let mut sections = Vec::new();
    for (title, items, shown) in [
        ("Noteskins", &r.noteskins, true),
        ("Not imported", &r.skipped, true),
        ("Imported with problems", &r.warnings, true),
        (
            "Details for pack authors (no difference in play)",
            &r.notes,
            details,
        ),
    ] {
        if shown && !items.is_empty() {
            sections.push((title.to_string(), lines(items)));
        }
    }
    ImportReportView {
        summary,
        error,
        sections,
    }
}

/// "This site stores about 120 MB".
pub(crate) fn format_bytes(b: f64) -> String {
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else {
        format!("{:.0} MB", (b / 1e6).max(1.0))
    }
}
