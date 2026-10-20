//! Song select's data from a library, the settings and the play history
//! (`crate::screens::data::SelectData`), for any shell: every song in
//! every style, packs in list order (the bundled songs first), each chart
//! with the player's best under the rules the settings choose for it.
//! Both shells build their list with it (`docs/plans/desktop.md`, 7.3
//! and 7.1): desktop's `songs.rs`, the web's `select_data.rs`.

use std::collections::HashMap;

use ddi_chart::{Difficulty, Layout};
use ddi_library::manifest::{ChartInfo, ManifestEntry};

use super::history::{Best as HistoryBest, BestKey, History, Lamp, score_value};
use super::place::MenuPlace;
use super::settings::Settings;
use crate::screens::data::{
    Best, ChartEntry, ImportView, PackEntry, SelectData, SongEntry, StyleDef,
};

/// What the bundled songs are listed under.
pub const BUNDLED_PACK: &str = "Bundled songs";

/// A song of the library as the list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Listed {
    pub entry: ManifestEntry,
    /// Its pack; `None` for a bundled song.
    pub pack: Option<String>,
    /// The pack's own cover, separately from the song banner.
    pub pack_banner: Option<String>,
    /// Its banner and background as texture sources.
    pub banner: Option<String>,
    pub background: Option<String>,
    /// It can be removed from the list.
    pub removable: bool,
}

/// Layouts with charts in `songs`: the built-in ones in their order,
/// then the ones songs define themselves, by name.
pub fn library_styles<'a>(songs: impl Iterator<Item = &'a ManifestEntry> + Clone) -> Vec<Layout> {
    let mut ids: Vec<&str> = songs
        .clone()
        .flat_map(|e| e.charts.iter().map(|c| c.layout.as_str()))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let mut layouts: Vec<Layout> = ids
        .into_iter()
        .filter_map(|id| {
            Layout::builtin(id).or_else(|| {
                songs
                    .clone()
                    .find_map(|e| e.layouts.iter().find(|l| l.id == id).cloned())
            })
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

/// A lamp as colors and commands name it, and written short.
pub fn lamp_names(l: Lamp) -> (&'static str, &'static str) {
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
pub fn score_text(v: &ddi_engine::rules::ScoreView) -> String {
    match (v.money, v.percent, v.ex) {
        (Some(m), _, _) => m.to_string(),
        (None, Some(p), _) => format!("{:.2}%", p * 100.0),
        (None, None, Some(ex)) => format!("EX {ex}"),
        _ => format!("{:.0}", score_value(v)),
    }
}

/// The player's best on a chart under the rules the settings choose for
/// it. Original charts show unassisted bests; generated charts show their
/// own assisted bests. Autoplay never counts.
fn best_of(
    entry: &ManifestEntry,
    chart: &ChartInfo,
    settings: &Settings,
    bests: &HashMap<BestKey, HistoryBest>,
) -> Option<Best> {
    let digest = chart.digest?;
    let danoni = entry
        .chart
        .as_str()
        .ends_with(ddi_library::danoni::STORED_EXT);
    let key = BestKey {
        digest,
        ruleset: settings.best_key_for(&chart.layout, danoni, chart.excessive),
        assist: chart.derivation.is_some(),
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
        play: b.score_play.clone(),
    })
}

fn song_entry(
    s: &Listed,
    pack: &str,
    settings: &Settings,
    bests: &HashMap<BestKey, HistoryBest>,
) -> SongEntry {
    let entry = &s.entry;
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
        banner: s.banner.clone(),
        background: s.background.clone(),
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
                digest: c.digest,
            })
            .collect(),
        removable: s.removable,
        offset_ms: (settings.song_offset(&entry.id) * 1000.0).round() as i32,
        title_translit: entry.title_translit.clone(),
        artist_translit: entry.artist_translit.clone(),
        credit: entry.credit.clone(),
    }
}

/// Song select's data now: `songs` in list order (the bundled ones
/// first), `rereadable` saying which packs can be read again.
pub fn select_data(
    songs: &[Listed],
    rereadable: &dyn Fn(&str) -> bool,
    settings: &Settings,
    history: &History,
    place: &MenuPlace,
) -> SelectData {
    // Generation is independent of the display setting so indexes stay
    // stable. Only hide the additional versions from the song list.
    let visible: Vec<Listed> = songs
        .iter()
        .cloned()
        .map(|mut song| {
            song.entry.charts.retain(|c| match c.derivation {
                None => true,
                Some(ddi_chart::simplify::Derivation::WithoutOnigiri { .. }) => {
                    settings.simplify_onigiri
                }
            });
            song
        })
        .collect();
    let songs = visible.as_slice();
    let styles: Vec<StyleDef> = library_styles(songs.iter().map(|s| &s.entry))
        .into_iter()
        .map(|l| StyleDef {
            id: l.id,
            name: l.name,
        })
        .collect();
    let bests = history.bests();
    let mut list = Vec::with_capacity(songs.len());
    let mut packs: Vec<PackEntry> = Vec::new();
    if songs.iter().any(|s| s.pack.is_none()) {
        packs.push(PackEntry {
            name: BUNDLED_PACK.into(),
            banner: None,
            removable: false,
            rereadable: false,
        });
    }
    for s in songs.iter().filter(|s| s.pack.is_none()) {
        list.push(song_entry(s, BUNDLED_PACK, settings, &bests));
    }
    for s in songs.iter() {
        let Some(pack) = &s.pack else { continue };
        // Packs told apart ignoring case, named as first seen.
        let name = match packs
            .iter()
            .find(|p| p.name.to_lowercase() == pack.to_lowercase())
        {
            Some(p) => p.name.clone(),
            None => {
                packs.push(PackEntry {
                    name: pack.clone(),
                    banner: s.pack_banner.clone(),
                    removable: s.removable,
                    rereadable: rereadable(pack),
                });
                pack.clone()
            }
        };
        list.push(song_entry(s, &name, settings, &bests));
    }
    let mods = crate::screens::play::mods_summary(
        &settings.transform,
        settings.appearance,
        settings.scroll_action,
        None,
    );
    // The first visit: the bundled songs open (else the first pack).
    let is_closed = |name: &String| place.closed.as_ref().is_some_and(|c| c.contains(name));
    let open = if place.closed.is_none() {
        packs.first()
    } else {
        // Migrate older lists with several expanded packs, favoring the selected song.
        place
            .song
            .as_ref()
            .and_then(|id| list.iter().find(|s| &s.id == id))
            .and_then(|song| {
                packs
                    .iter()
                    .find(|p| p.name == song.pack && !is_closed(&p.name))
            })
            .or_else(|| packs.iter().find(|p| !is_closed(&p.name)))
    };
    let closed = packs
        .iter()
        .filter(|p| open.is_none_or(|o| o.name != p.name))
        .map(|p| p.name.clone())
        .collect();
    SelectData {
        styles,
        songs: list,
        mods,
        preferred: None,
        packs,
        closed,
        resume: place.song.clone().map(|song| {
            let chart = place
                .chart
                .as_ref()
                .filter(|(id, _)| *id == song)
                .map(|(_, c)| *c);
            (song, chart)
        }),
        resume_pack: place.open.as_ref().and_then(|key| {
            if key == "bundled" {
                Some(BUNDLED_PACK.into())
            } else {
                key.strip_prefix("pack:").map(str::to_string)
            }
        }),
        loading: false,
        style: place.style.clone().or_else(|| Some(settings.style.clone())),
        search: place.search.clone(),
        notices: Vec::new(),
        import: ImportView::default(),
        autoplay: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onigiri_versions_are_optional_but_the_originals_are_always_listed_and_unassisted() {
        use ddi_chart::formats::danoni::{self, Dos, DosFlags};
        use ddi_engine::player::Player;
        let mut song = danoni::import(&Dos::parse(
            "|musicTitle=Test,Artist|musicUrl=test.ogg|blankFrame=0|difData=5,Hard,3.5|left_data=60|space_data=90|right_data=120|",
            DosFlags::default(),
        )).unwrap().songs.remove(0);
        song.add_no_onigiri_charts();
        let mut entry = ddi_library::manifest::summarize(
            ddi_library::manifest::EntryMeta {
                id: "test".into(),
                chart: format!("test.{}", ddi_library::danoni::STORED_EXT).into(),
                ..Default::default()
            },
            &song,
        );
        ddi_library::works::fill_chart_keys(&mut entry, &song);
        let songs = [Listed {
            entry,
            pack: Some("Test".into()),
            pack_banner: None,
            banner: None,
            background: None,
            removable: true,
        }];
        let mut settings = Settings::default();
        let data = select_data(
            &songs,
            &|_| false,
            &settings,
            &History::default(),
            &MenuPlace::default(),
        );
        assert_eq!(data.songs[0].charts.len(), 1);
        assert_eq!(data.songs[0].charts[0].style, "danoni-5");
        settings.simplify_onigiri = true;
        let data = select_data(
            &songs,
            &|_| false,
            &settings,
            &History::default(),
            &MenuPlace::default(),
        );
        assert_eq!(data.songs[0].charts.len(), 2);
        assert!(data.styles.iter().any(|s| s.id == "dance-single"));
        assert!(data.styles.iter().any(|s| s.id == "danoni-5"));
        assert_eq!(
            data.songs[0]
                .charts
                .iter()
                .find(|c| c.style == "danoni-5")
                .unwrap()
                .index,
            0
        );
        for (index, assisted, count) in [(0, false, 3), (1, true, 2)] {
            let player = Player::new(
                &song,
                index,
                settings.ruleset_for(&song.charts[index]),
                settings.play_options(None),
            );
            assert_eq!(player.results().assist, assisted);
            assert_eq!(player.judge().notes().len(), count);
        }
        // Turning the display option back off does not remove a replay's chart.
        settings.simplify_onigiri = false;
        assert!(
            Player::new(
                &song,
                1,
                settings.ruleset_for(&song.charts[1]),
                settings.play_options(None)
            )
            .results()
            .assist
        );
    }

    fn entry(id: &str, layout: &str) -> ManifestEntry {
        let mut e: ManifestEntry = serde_json::from_str(&format!(
            r#"{{"id":"{id}","title":"T {id}","artist":"A","chart":"c.sm","music":"m.ogg","bpm":"120-150",
                "charts":[{{"index":0,"layout":"{layout}","difficulty":"Hard","meter":9}}]}}"#
        ))
        .unwrap();
        e.length = 90.0;
        e
    }

    #[test]
    fn bundled_songs_come_first_and_packs_ignore_case() {
        let listed = |id: &str, pack: Option<&str>| Listed {
            entry: entry(id, "dance-single"),
            pack: pack.map(str::to_string),
            banner: None,
            pack_banner: None,
            background: None,
            removable: pack.is_some(),
        };
        let mut songs = vec![
            listed("u-1", Some("Pack")),
            listed("bundled", None),
            listed("u-2", Some("PACK")),
            listed("u-3", Some("Other")),
        ];
        songs[0].pack_banner = Some("pack-cover.png".into());
        let d = select_data(
            &songs,
            &|p| p == "Other",
            &Settings::default(),
            &History::default(),
            &MenuPlace::default(),
        );
        let ids: Vec<&str> = d.songs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["bundled", "u-1", "u-2", "u-3"]);
        let packs: Vec<(&str, bool)> = d
            .packs
            .iter()
            .map(|p| (p.name.as_str(), p.rereadable))
            .collect();
        assert_eq!(
            packs,
            vec![(BUNDLED_PACK, false), ("Pack", false), ("Other", true)]
        );
        assert_eq!(d.songs[2].pack, "Pack");
        assert_eq!(d.packs[1].banner.as_deref(), Some("pack-cover.png"));
        assert_eq!(d.songs[1].bpm, 120.0);
        assert_eq!(d.songs[1].bpm_text, "120–150");
        // The first visit opens the bundled songs.
        assert_eq!(d.closed, vec!["Pack".to_string(), "Other".to_string()]);
        assert_eq!(d.styles.len(), 1);
    }
}
