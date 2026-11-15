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
/// new ones; its files (music, art, background changes, lyrics, preview
/// movie) the old ones, except that its music, banner, background and
/// jacket follow what the simfile now names where that is one of
/// `stored` (the song's files as the shell keeps them, by their paths in
/// its folder; matched as simfiles name files, whatever the case), as a
/// library reading the folder again would have them: a name that is no
/// stored file leaves the old one. A chart's own music is kept where the
/// chart at its index still names one.
pub fn entry_after_edit(
    old: &ManifestEntry,
    text: &str,
    chart: RelPathBuf,
    stored: &[String],
) -> Result<ManifestEntry, String> {
    let song: Song = crate::scan::parse_simfile(text, "ssc").map_err(|e| e.to_string())?;
    let (title, artist) = simfile_names(&song, &old.title);
    let meta = EntryMeta {
        title,
        artist,
        chart,
        ..EntryMeta::default()
    };
    let new = summarize(meta, &song);
    let mut entry = ManifestEntry {
        variants_version: new.variants_version,
        title: new.title,
        artist: new.artist,
        chart: new.chart,
        bpm: new.bpm,
        preview_start: new.preview_start,
        preview_length: new.preview_length,
        length: new.length,
        charts: new.charts,
        layouts: new.layouts,
        title_translit: new.title_translit,
        artist_translit: new.artist_translit,
        ..old.clone()
    };
    let named = |name: &Option<String>| -> Option<RelPathBuf> {
        let name = name.as_deref()?.trim().replace('\\', "/");
        stored
            .iter()
            .find(|s| s.eq_ignore_ascii_case(&name))
            .map(|s| RelPathBuf::from(s.as_str()))
    };
    if let Some(music) = named(&song.music) {
        entry.music = music;
    }
    for (field, name) in [
        (&mut entry.banner, &song.banner),
        (&mut entry.background, &song.background),
        (&mut entry.jacket, &song.jacket),
    ] {
        if let Some(file) = named(name) {
            *field = Some(file);
        }
    }
    for c in &mut entry.charts {
        if song.chart_music(c.index).is_some() {
            c.music = old
                .charts
                .iter()
                .find(|o| o.index == c.index)
                .and_then(|o| o.music.clone());
        }
    }
    Ok(entry)
}

/// The entry of a song the editor made new from a music file (the web
/// stores it as an imported song): `id`, read from `text` as stored as
/// `chart`, its music `music`.
pub fn entry_for_new(
    id: &str,
    text: &str,
    chart: RelPathBuf,
    music: RelPathBuf,
) -> Result<ManifestEntry, String> {
    let song: Song = crate::scan::parse_simfile(text, "ssc").map_err(|e| e.to_string())?;
    let (title, artist) = simfile_names(&song, "");
    let meta = EntryMeta {
        id: id.to_string(),
        title,
        artist,
        chart,
        music,
        ..EntryMeta::default()
    };
    Ok(summarize(meta, &song))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SM: &str = "#TITLE:Song;#ARTIST:Me;#MUSIC:song.ogg;#OFFSET:0;#BPMS:0=120;\
        #NOTES:dance-single:x:Easy:3:0,0,0,0,0:1000\n0100\n0010\n0001\n;";

    #[test]
    fn an_edited_song_keeps_its_files_and_lists_its_new_charts() {
        let song = crate::scan::parse_simfile(SM, "sm").unwrap();
        let mut old = summarize(
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
        let movie = crate::manifest::VideoRef {
            name: "preview.mp4".into(),
            format: ddi_platform::video::VideoFormat {
                container: ddi_platform::video::VideoContainer::Unknown,
                codec: ddi_platform::video::VideoCodec::Unknown,
            },
            stored: true,
        };
        old.preview_video = Some(movie.clone());
        let mut edited = song.clone();
        edited.title = "Song (edit)".into();
        let mut hard = edited.charts[0].clone();
        hard.difficulty = ddi_chart::Difficulty::Hard;
        hard.meter = 9;
        hard.notes.truncate(2);
        edited.charts.push(hard);
        edited.timing.bpms[0].bpm = 150.0;
        let text = ddi_chart::formats::sm_write::write_ssc(&edited);
        let e = entry_after_edit(&old, &text, "chart.ssc".into(), &[]).unwrap();
        assert_eq!(e.id, "pack:song");
        assert_eq!(e.title, "Song (edit)");
        assert_eq!(e.chart.as_str(), "chart.ssc");
        assert_eq!(e.music.as_str(), "music.ogg");
        assert_eq!(e.banner.as_ref().map(|b| b.as_str()), Some("banner.png"));
        assert_eq!(e.preview_video, Some(movie), "the preview movie kept");
        assert_eq!(e.bpm, "150");
        assert_eq!(e.charts.len(), 2);
        assert_eq!((e.charts[1].meter, e.charts[1].notes), (9, 2));
        assert!(e.charts.iter().all(|c| c.digest.is_some()));
        assert_ne!(
            e.charts[0].digest, old.charts[0].digest,
            "a new tempo, a new chart"
        );
    }

    /// Review studio-editor-7: the song's music and art follow what the
    /// editor's Song fields now name where that is a file the shell
    /// keeps for the song (desktop reads the folder again; the web's
    /// entry said the old files for good).
    #[test]
    fn an_edited_song_s_files_follow_its_fields_where_they_are_stored() {
        let song = crate::scan::parse_simfile(SM, "sm").unwrap();
        let old = summarize(
            EntryMeta {
                id: "pack:song".into(),
                title: "Song".into(),
                chart: "chart.sm".into(),
                music: "song.ogg".into(),
                banner: Some("banner.png".into()),
                ..EntryMeta::default()
            },
            &song,
        );
        let mut edited = song.clone();
        edited.music = Some("Other.OGG".into());
        edited.banner = Some("gone.png".into());
        edited.background = Some("art\\bg.png".into());
        let text = ddi_chart::formats::sm_write::write_ssc(&edited);
        let stored: Vec<String> = ["song.ogg", "other.ogg", "banner.png", "art/bg.png"]
            .map(String::from)
            .to_vec();
        let e = entry_after_edit(&old, &text, "chart.ssc".into(), &stored).unwrap();
        assert_eq!(e.music.as_str(), "other.ogg", "the file stored, as stored");
        assert_eq!(
            e.banner.as_ref().map(|b| b.as_str()),
            Some("banner.png"),
            "no such file: the old one"
        );
        assert_eq!(e.background.as_ref().map(|b| b.as_str()), Some("art/bg.png"));
        // Nothing known of the stored files: as before.
        let e = entry_after_edit(&old, &text, "chart.ssc".into(), &[]).unwrap();
        assert_eq!(e.music.as_str(), "song.ogg");
    }

    #[test]
    fn a_new_song_with_an_empty_chart_is_listed() {
        let text = "#VERSION:0.83;#TITLE:New;#ARTIST:Me;#MUSIC:tune.ogg;#OFFSET:0;#BPMS:0=120;\
            #NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Medium;#METER:1;#NOTES:\n0000\n;";
        let e = entry_for_new("u-1", text, "chart.ssc".into(), "tune.ogg".into()).unwrap();
        assert_eq!(
            (e.id.as_str(), e.title.as_str(), e.artist.as_str()),
            ("u-1", "New", "Me")
        );
        assert_eq!(e.music.as_str(), "tune.ogg");
        assert_eq!(e.charts.len(), 1);
        assert_eq!((e.charts[0].notes, e.bpm.as_str()), (0, "120"));
    }
}
