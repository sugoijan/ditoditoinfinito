//! A song the chart editor saved (`docs/plans/editor.md`): its list
//! entry made again from the simfile as written, its files as they were.
//! The web keeps an imported song's entry with its stored files; after a
//! save it is this.

use ddi_chart::Song;

use crate::manifest::{EntryMeta, ManifestEntry, simfile_names, summarize};
use crate::path::RelPathBuf;

/// `old`'s entry for the song read from `text` (the `.ssc` the editor
/// wrote, read as the library reads every simfile, generated versions
/// added), stored as `chart`: its title, charts, tempo and length the
/// new ones; its files (music, art, background changes, lyrics) the old
/// ones. A chart's own music is kept where the chart at its index still
/// names one.
pub fn entry_after_edit(
    old: &ManifestEntry,
    text: &str,
    chart: RelPathBuf,
) -> Result<ManifestEntry, String> {
    let song: Song = crate::scan::parse_simfile(text, "ssc").map_err(|e| e.to_string())?;
    let (title, artist) = simfile_names(&song, &old.title);
    let meta = EntryMeta {
        id: old.id.clone(),
        title,
        artist,
        chart,
        music: old.music.clone(),
        banner: old.banner.clone(),
        background: old.background.clone(),
        bg_images: old.bg_images.clone(),
        bg_videos: old.bg_videos.clone(),
        bg_shared: old.bg_shared.clone(),
        credit: old.credit.clone(),
    };
    let mut entry = summarize(meta, &song);
    entry.jacket = old.jacket.clone();
    entry.cd_title = old.cd_title.clone();
    entry.lyrics = old.lyrics.clone();
    entry.preview = old.preview.clone();
    entry.bg_scripted = old.bg_scripted.clone();
    entry.bg_scripts = old.bg_scripts.clone();
    entry.layer_images = old.layer_images.clone();
    for c in &mut entry.charts {
        if song.chart_music(c.index).is_some() {
            c.music = old
                .charts
                .iter()
                .find(|o| o.index == c.index)
                .and_then(|o| o.music.clone());
        }
    }
    crate::works::fill_chart_keys(&mut entry, &song);
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SM: &str = "#TITLE:Song;#ARTIST:Me;#MUSIC:song.ogg;#OFFSET:0;#BPMS:0=120;\
        #NOTES:dance-single:x:Easy:3:0,0,0,0,0:1000\n0100\n0010\n0001\n;";

    #[test]
    fn an_edited_song_keeps_its_files_and_lists_its_new_charts() {
        let song = crate::scan::parse_simfile(SM, "sm").unwrap();
        let old = summarize(
            EntryMeta {
                id: "pack:song".into(),
                title: "Song".into(),
                artist: "Me".into(),
                chart: "chart.sm".into(),
                music: "music.ogg".into(),
                banner: Some("banner.png".into()),
                credit: "x".into(),
                ..EntryMeta::default()
            },
            &song,
        );
        let mut edited = song.clone();
        edited.title = "Song (edit)".into();
        let mut hard = edited.charts[0].clone();
        hard.difficulty = ddi_chart::Difficulty::Hard;
        hard.meter = 9;
        hard.notes.truncate(2);
        edited.charts.push(hard);
        edited.timing.bpms[0].bpm = 150.0;
        let text = ddi_chart::formats::sm_write::write_ssc(&edited);
        let e = entry_after_edit(&old, &text, "chart.ssc".into()).unwrap();
        assert_eq!(e.id, "pack:song");
        assert_eq!(e.title, "Song (edit)");
        assert_eq!(e.chart.as_str(), "chart.ssc");
        assert_eq!(e.music.as_str(), "music.ogg");
        assert_eq!(e.banner.as_ref().map(|b| b.as_str()), Some("banner.png"));
        assert_eq!(e.bpm, "150");
        assert_eq!(e.charts.len(), 2);
        assert_eq!((e.charts[1].meter, e.charts[1].notes), (9, 2));
        assert!(e.charts.iter().all(|c| c.digest.is_some()));
        assert_ne!(
            e.charts[0].digest, old.charts[0].digest,
            "a new tempo, a new chart"
        );
    }
}
