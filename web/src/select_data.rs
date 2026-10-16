//! Song select's data from the library, the settings and the play
//! history (`ddi_ui::screens::data::SelectData`): every song in every
//! style, packs in list order, each chart with the player's best under
//! the rules the settings choose for it.

use std::collections::HashMap;

use ddi_chart::{Difficulty, Layout};
use ddi_ui::screens::data::{
    Best, ChartEntry, ImportReportView, ImportView, PackEntry, SelectData, SongEntry, StyleDef,
};
use ddi_ui::state::history::{BestKey, History, Lamp, score_value};
use wasm_bindgen::JsValue;

use crate::import::ImportReport;
use crate::settings::Settings;
use crate::songs::{ImportedSong, Library, ManifestEntry, file_key};
use ddi_ui::state::place::MenuPlace;

/// What the bundled songs are listed under.
pub(crate) const BUNDLED_PACK: &str = "Bundled songs";

/// Layouts with charts in the library: the built-in ones in their order,
/// then the ones songs define themselves, by name.
pub(crate) fn library_styles(library: &Library) -> Vec<Layout> {
    let entries = || {
        library
            .bundled
            .iter()
            .chain(library.imported.iter().map(|s| &s.entry))
    };
    let mut ids: Vec<&str> = entries()
        .flat_map(|e| e.charts.iter().map(|c| c.layout.as_str()))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let mut layouts: Vec<Layout> = ids
        .into_iter()
        .filter_map(|id| {
            Layout::builtin(id)
                .or_else(|| entries().find_map(|e| e.layouts.iter().find(|l| l.id == id).cloned()))
        })
        .collect();
    let order = Layout::builtin_ids();
    layouts.sort_by(|a, b| {
        let rank = |l: &Layout| order.iter().position(|b| *b == l.id).unwrap_or(order.len());
        rank(a)
            .cmp(&rank(b))
            .then(a.name.cmp(&b.name))
            .then(a.id.cmp(&b.id))
    });
    layouts
}

fn difficulty(name: &str) -> Difficulty {
    match name {
        "Beginner" => Difficulty::Beginner,
        "Easy" => Difficulty::Easy,
        "Medium" => Difficulty::Medium,
        "Hard" => Difficulty::Hard,
        "Challenge" => Difficulty::Challenge,
        _ => Difficulty::Edit,
    }
}

/// A lamp as colours and commands name it, and written short.
pub(crate) fn lamp_names(l: Lamp) -> (&'static str, &'static str) {
    match l {
        Lamp::Failed => ("Failed", "FAILED"),
        Lamp::Clear => ("Clear", "CLEAR"),
        Lamp::FullCombo => ("FullCombo", "FC"),
        Lamp::GreatFullCombo => ("GreatFullCombo", "GFC"),
        Lamp::PerfectFullCombo => ("PerfectFullCombo", "PFC"),
        Lamp::MarvelousFullCombo => ("MarvelousFullCombo", "MFC"),
    }
}

/// A best's score as the ruleset scores: money, a percentage, EX.
fn score_text(v: &ddi_engine::rules::ScoreView) -> String {
    match (v.money, v.percent, v.ex) {
        (Some(m), _, _) => m.to_string(),
        (None, Some(p), _) => format!("{:.2}%", p * 100.0),
        (None, None, Some(ex)) => format!("EX {ex}"),
        _ => format!("{:.0}", score_value(v)),
    }
}

/// The player's best on a chart under the rules the settings choose for
/// it (the key a play of it records; autoplay never counts; assist plays
/// are apart, not shown here).
fn best_of(
    entry: &ManifestEntry,
    chart: &ddi_library::manifest::ChartInfo,
    settings: &Settings,
    bests: &HashMap<BestKey, ddi_ui::state::history::Best>,
) -> Option<Best> {
    let digest = chart.digest?;
    let danoni = entry.chart.ends_with(ddi_library::danoni::STORED_EXT);
    let key = BestKey {
        digest,
        ruleset: settings.best_key_for(&chart.layout, danoni, chart.excessive),
        assist: false,
    };
    let b = bests.get(&key)?;
    let (lamp, lamp_text) = lamp_names(b.lamp);
    Some(Best {
        grade: b.grade.clone(),
        score: score_text(&b.score),
        lamp: lamp.into(),
        lamp_text: lamp_text.into(),
        pending: b.pending,
        plays: b.plays,
    })
}

/// What the song list keeps of a parsed song's charts for its bests: their
/// digests and a Dancing☆Onigiri chart's excessive setting.
pub(crate) fn fill_chart_keys(entry: &mut ManifestEntry, song: &ddi_chart::Song) {
    for c in &mut entry.charts {
        if c.index < song.charts.len() {
            c.digest = Some(song.chart_digest(c.index));
            c.excessive = ddi_ui::state::settings::chart_excessive(&song.charts[c.index]);
        }
    }
}

fn song_entry(
    entry: &ManifestEntry,
    imported: Option<&ImportedSong>,
    pack: &str,
    settings: &Settings,
    bests: &HashMap<BestKey, ddi_ui::state::history::Best>,
) -> SongEntry {
    let (banner, background) = match imported {
        None => (
            entry
                .banner
                .as_ref()
                .map(|b| format!("songs/{}/{b}", entry.id)),
            entry
                .background
                .as_ref()
                .map(|b| format!("songs/{}/{b}", entry.id)),
        ),
        Some(s) => (
            entry
                .banner
                .as_ref()
                .map(|b| format!("files:{}", file_key(&entry.id, b))),
            // A background left in a kept folder is not in the store.
            entry
                .background
                .as_ref()
                .filter(|b| !s.links.contains_key(b.as_str()))
                .map(|b| format!("files:{}", file_key(&entry.id, b))),
        ),
    };
    let bpm = entry
        .bpm
        .split(['-', '–'])
        .next()
        .and_then(|b| b.trim().parse::<f32>().ok())
        .unwrap_or(0.0);
    SongEntry {
        id: entry.id.clone(),
        title: entry.title.clone(),
        subtitle: String::new(),
        artist: entry.artist.clone(),
        pack: pack.to_string(),
        bpm,
        bpm_text: entry.bpm.replace('-', "–"),
        length: entry.length as f32,
        banner,
        background,
        charts: entry
            .charts
            .iter()
            .map(|c| ChartEntry {
                style: c.layout.clone(),
                difficulty: difficulty(&c.difficulty),
                meter: c.meter,
                notes: c.notes as u32,
                credit: c.credit.clone(),
                index: c.index,
                name: c.name.clone(),
                best: best_of(entry, c, settings, bests),
            })
            .collect(),
        removable: imported.is_some(),
        offset_ms: (settings.song_offset(&entry.id) * 1000.0).round() as i32,
        title_translit: entry.title_translit.clone(),
        artist_translit: entry.artist_translit.clone(),
        credit: entry.credit.clone(),
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
    let styles: Vec<StyleDef> = library_styles(library)
        .into_iter()
        .map(|l| StyleDef {
            id: l.id,
            name: l.name,
        })
        .collect();
    // Every best once (looked up per chart below).
    let bests = history.bests();
    let mut songs: Vec<SongEntry> = library
        .bundled
        .iter()
        .map(|e| song_entry(e, None, BUNDLED_PACK, settings, &bests))
        .collect();
    let mut packs = Vec::new();
    if !library.bundled.is_empty() {
        packs.push(PackEntry {
            name: BUNDLED_PACK.into(),
            removable: false,
            rereadable: false,
        });
    }
    for s in &library.imported {
        // Packs told apart ignoring case, named as first seen.
        let name = match packs
            .iter()
            .find(|p: &&PackEntry| p.name.to_lowercase() == s.pack.to_lowercase())
        {
            Some(p) => p.name.clone(),
            None => {
                packs.push(PackEntry {
                    name: s.pack.clone(),
                    removable: true,
                    rereadable: false,
                });
                s.pack.clone()
            }
        };
        if s.folder.as_ref().is_some_and(|f| folders.contains_key(f))
            && let Some(p) = packs.iter_mut().find(|p| p.name == name)
        {
            p.rereadable = true;
        }
        songs.push(song_entry(&s.entry, Some(s), &name, settings, &bests));
    }
    let mods = crate::mods::mods_summary(
        &settings.transform,
        settings.appearance,
        settings.scroll_action,
        None,
    );
    // The first visit: the bundled songs open (else the first pack).
    let closed = match &place.closed {
        Some(c) => c.clone(),
        None => packs.iter().skip(1).map(|p| p.name.clone()).collect(),
    };
    SelectData {
        styles,
        songs,
        mods,
        preferred: None,
        packs,
        closed,
        style: place.style.clone().or_else(|| Some(settings.style.clone())),
        search: place.search.clone(),
        notices: Vec::new(),
        import: ImportView::default(),
    }
}

/// The import dialog's state now.
pub(crate) fn import_view(settings: &Settings) -> ImportView {
    let (running, report, _) = crate::import::status();
    ImportView {
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
