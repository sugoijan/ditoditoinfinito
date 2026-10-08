//! Song list entries, shared by bundled songs (`xtask gen-songs` writes them
//! to `songs/index.json`) and imported ones (the web shell builds them at
//! import time), so the song select treats both the same way.
//!
//! The JSON field names and defaults are the wire format of the generated
//! manifest; changing them changes `index.json`.

use ddi_chart::{Layout, Song};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Stable id. For bundled songs also the directory name under `songs/`;
    /// for imported songs [`crate::pack::song_id`].
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Chart file (`.sm`, `.ssc` or `.dwi`) relative to the song directory.
    pub chart: String,
    /// Audio file relative to the song directory.
    pub music: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Images the song's background changes show, relative to the song
    /// directory as the simfile names them
    /// ([`crate::backgrounds::referenced_images`]). Imported songs store
    /// each under `bg/<path>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bg_images: Vec<String>,
    /// Short credit line shown in the song list (full credits live in the
    /// Credits screen).
    #[serde(default)]
    pub credit: String,
    /// Display BPM, `"120"` or `"90-180"`.
    #[serde(default)]
    pub bpm: String,
    #[serde(default)]
    pub preview_start: f64,
    #[serde(default)]
    pub preview_length: f64,
    /// Playable charts sorted by layout, difficulty and meter; `index` is the
    /// position in `Song::charts`.
    #[serde(default)]
    pub charts: Vec<ChartInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChartInfo {
    /// Position in `Song::charts` (the `chart=` URL parameter).
    pub index: usize,
    pub layout: String,
    /// `Difficulty` variant name (`Beginner` … `Edit`).
    pub difficulty: String,
    pub meter: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub credit: String,
    /// Judged notes ([`ddi_chart::Chart::judged_note_count`]).
    #[serde(default)]
    pub notes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub songs: Vec<ManifestEntry>,
}

/// What [`summarize`] cannot read from the parsed simfile: where the files
/// are, and how the song is named and credited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntryMeta {
    pub id: String,
    /// Bundled songs take this from `PROVENANCE.toml`; imported songs from
    /// [`simfile_names`].
    pub title: String,
    pub artist: String,
    pub chart: String,
    pub music: String,
    pub banner: Option<String>,
    pub background: Option<String>,
    pub bg_images: Vec<String>,
    pub credit: String,
}

/// Builds the song list entry for a parsed song: display BPM from the
/// timing, preview window, and the playable charts (those with a built-in
/// [`Layout`]) sorted by layout, difficulty slot, then meter, since file
/// order is arbitrary.
pub fn summarize(meta: EntryMeta, song: &Song) -> ManifestEntry {
    let (lo, hi) = song.timing.bpm_range();
    let bpm = if (lo - hi).abs() < 0.01 {
        format!("{}", lo.round() as i64)
    } else {
        format!("{}-{}", lo.round() as i64, hi.round() as i64)
    };
    let mut indexed: Vec<_> = song
        .charts
        .iter()
        .enumerate()
        .filter(|(_, c)| Layout::builtin(&c.layout).is_some())
        .collect();
    indexed.sort_by(|(_, a), (_, b)| {
        a.layout
            .cmp(&b.layout)
            .then(a.difficulty.cmp(&b.difficulty))
            .then(a.meter.cmp(&b.meter))
    });
    let charts = indexed
        .into_iter()
        .map(|(index, c)| ChartInfo {
            index,
            layout: c.layout.clone(),
            difficulty: format!("{:?}", c.difficulty),
            meter: c.meter,
            name: c.name.clone(),
            credit: c.credit.clone(),
            notes: c.judged_note_count(),
        })
        .collect();
    ManifestEntry {
        id: meta.id,
        title: meta.title,
        artist: meta.artist,
        chart: meta.chart,
        music: meta.music,
        banner: meta.banner,
        background: meta.background,
        bg_images: meta.bg_images,
        credit: meta.credit,
        bpm,
        preview_start: song.preview_start,
        preview_length: song.preview_length,
        charts,
    }
}

/// Artist shown when the simfile names none (StepMania's wording).
pub const UNKNOWN_ARTIST: &str = "Unknown artist";

/// Title and artist of an imported song: `#TITLE` (plus `#SUBTITLE`) and
/// `#ARTIST`, trimmed; an empty title falls back to the song directory's
/// name and an empty artist to [`UNKNOWN_ARTIST`], as StepMania's
/// `Song::TidyUpData` does.
pub fn simfile_names(song: &Song, dir_name: &str) -> (String, String) {
    let title = song.title.trim();
    let subtitle = song.subtitle.trim();
    let title = match (title.is_empty(), subtitle.is_empty()) {
        (true, _) => dir_name.trim().to_string(),
        (false, true) => title.to_string(),
        (false, false) => format!("{title} {subtitle}"),
    };
    let artist = song.artist.trim();
    let artist = if artist.is_empty() {
        UNKNOWN_ARTIST.to_string()
    } else {
        artist.to_string()
    };
    (title, artist)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::formats::sm::parse_sm;

    const SM: &str = "#TITLE:  Song  ;\n#SUBTITLE:(Remix);\n#ARTIST:;\n#BPMS:0=120,16=180;\n#SAMPLESTART:12.5;\n#SAMPLELENGTH:15;\n\
#NOTES:\n     dance-single:\n     :\n     Hard:\n     9:\n     0,0,0,0,0:\n1000\n0100\n0010\n0001\n;\n\
#NOTES:\n     pump-single:\n     :\n     Easy:\n     2:\n     0,0,0,0,0:\n10000\n01000\n00100\n00010\n;\n\
#NOTES:\n     dance-single:\n     :\n     Easy:\n     3:\n     0,0,0,0,0:\n1000\n0000\n0010\n0000\n;\n";

    #[test]
    fn summarize_sorts_and_filters_charts() {
        let song = parse_sm(SM).unwrap();
        let entry = summarize(
            EntryMeta {
                id: "x".into(),
                title: "T".into(),
                artist: "A".into(),
                chart: "song.sm".into(),
                music: "song.ogg".into(),
                ..Default::default()
            },
            &song,
        );
        assert_eq!(entry.bpm, "120-180");
        assert_eq!(entry.preview_start, 12.5);
        assert_eq!(entry.preview_length, 15.0);
        let order: Vec<_> = entry
            .charts
            .iter()
            .map(|c| (c.index, c.difficulty.as_str(), c.notes))
            .collect();
        // pump-single has no built-in layout; Easy sorts before Hard.
        assert_eq!(order, [(2, "Easy", 2), (0, "Hard", 4)]);
    }

    #[test]
    fn names_fall_back_like_stepmania() {
        let song = parse_sm(SM).unwrap();
        assert_eq!(
            simfile_names(&song, "Dir"),
            ("Song (Remix)".to_string(), UNKNOWN_ARTIST.to_string())
        );
        let mut untitled = song.clone();
        untitled.title.clear();
        untitled.artist = "Someone".into();
        assert_eq!(
            simfile_names(&untitled, "My Song"),
            ("My Song".to_string(), "Someone".to_string())
        );
    }

    #[test]
    fn json_shape_is_stable() {
        let entry = ManifestEntry {
            id: "a".into(),
            title: "t".into(),
            artist: "r".into(),
            chart: "c.ssc".into(),
            music: "m.ogg".into(),
            banner: None,
            background: Some("bg.png".into()),
            bg_images: Vec::new(),
            credit: String::new(),
            bpm: "120".into(),
            preview_start: 1.0,
            preview_length: 2.0,
            charts: vec![],
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(
            json,
            r#"{"id":"a","title":"t","artist":"r","chart":"c.ssc","music":"m.ogg","background":"bg.png","credit":"","bpm":"120","preview_start":1.0,"preview_length":2.0,"charts":[]}"#
        );
        // Older manifests without the optional fields still load.
        let minimal: ManifestEntry = serde_json::from_str(
            r#"{"id":"a","title":"t","artist":"r","chart":"c.sm","music":"m.ogg"}"#,
        )
        .unwrap();
        assert_eq!(minimal.charts, vec![]);
        assert_eq!(serde_json::from_str::<ManifestEntry>(&json).unwrap(), entry);
    }
}
