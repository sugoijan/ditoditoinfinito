//! Song list entries, shared by bundled songs (`xtask gen-songs` writes them
//! to `songs/index.json`) and imported ones (the web shell builds them at
//! import time), so the song select treats both the same way.
//!
//! The JSON field names and defaults are the wire format of the generated
//! manifest; changing them changes `index.json`. The files an entry names
//! are logical paths ([`RelPathBuf`]), plain strings in the JSON.

use ddi_chart::{DisplayBpm, Layout, Song, SourceFormat};
use ddi_platform::video::VideoFormat;
use serde::{Deserialize, Serialize};

use crate::path::{RelPath, RelPathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Generated-chart metadata version; older imported charts are upgraded
    /// from their stored chart data without requiring another import.
    #[serde(default, skip_serializing_if = "is_zero_version")]
    pub variants_version: u32,
    /// Stable id. For bundled songs also the directory name under `songs/`;
    /// for imported songs [`crate::pack::song_id`].
    pub id: String,
    pub title: String,
    pub artist: String,
    /// Chart file (`.sm`, `.ssc` or `.dwi`) relative to the song directory.
    pub chart: RelPathBuf,
    /// Audio file relative to the song directory.
    pub music: RelPathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<RelPathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<RelPathBuf>,
    /// The jacket (square art) and CD title (a small mark) song select
    /// shows (`#JACKET`, `#CDTITLE`, or found as StepMania finds them).
    /// Missing in songs imported before 2026-10-21.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jacket: Option<RelPathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cd_title: Option<RelPathBuf>,
    /// The `.lrc` lyrics a play shows (`#LYRICSPATH`, or the folder's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<RelPathBuf>,
    /// The `.ssc` `#PREVIEW` sound song select plays instead of the
    /// music's sample.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<RelPathBuf>,
    /// Images the song's background changes show, relative to the song
    /// directory as the simfile names them
    /// ([`crate::backgrounds::referenced_images`]). Imported songs store
    /// each under `bg/<path>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bg_images: Vec<RelPathBuf>,
    /// Movies the song's background changes play, in order of first use
    /// ([`crate::backgrounds::referenced_movies`]), with what each is.
    /// Imported songs store or link each under `bg/<name>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bg_videos: Vec<VideoRef>,
    /// Images and movies the background changes name that the song's
    /// folder lacks, as the simfile writes them
    /// ([`crate::backgrounds::absent_files`]): looked up in the library's
    /// shared folders when the song is played.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bg_shared: Vec<RelPathBuf>,
    /// Scripted animations (neither images nor movies) the background
    /// changes name that exist, in the song's folder or the shared ones,
    /// as written ([`crate::backgrounds::unshown_files`]): StepMania plays
    /// them, so the second layer clears for them
    /// ([`crate::backgrounds::second_layer`]); one found nowhere leaves it
    /// as it is. Missing in songs imported before 2026-10-16.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bg_scripted: Vec<RelPathBuf>,
    /// Images a Dancing☆Onigiri work's back and mask layers show, by the
    /// names its data uses, stored as `layer/<name>`
    /// (`docs/plans/overlays.md`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layer_images: Vec<RelPathBuf>,
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
    /// Seconds to the end of the last note of the playable charts (the
    /// song list's length; the music may run on). 0 in entries from
    /// before it was kept.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub length: f64,
    /// Playable charts sorted by layout, difficulty and meter; `index` is the
    /// position in `Song::charts`.
    #[serde(default)]
    pub charts: Vec<ChartInfo>,
    /// Layouts the song defines itself (Dancing☆Onigiri custom keys) that
    /// its charts use, so the song list and the bindings can show them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layouts: Vec<Layout>,
    /// The title (with its subtitle) and artist as the simfile spells them
    /// in Latin letters (`#TITLETRANSLIT`, `#SUBTITLETRANSLIT`,
    /// `#ARTISTTRANSLIT`), for the search; empty when it gives none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title_translit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist_translit: String,
}

/// A movie of a song's background changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoRef {
    /// Path relative to the song directory, as the schedule matches it.
    pub name: RelPathBuf,
    pub format: VideoFormat,
    /// Whether the import kept it (copied or linked). A copying import
    /// leaves movies out unless asked to keep them; the song background
    /// shows instead.
    #[serde(default = "kept")]
    pub stored: bool,
}

fn kept() -> bool {
    true
}

fn is_zero_version(value: &u32) -> bool {
    *value == 0
}

/// Current metadata for additional chart versions in imported manifests.
pub const VARIANTS_VERSION: u32 = 1;

impl ManifestEntry {
    /// Whether the song is a Dancing☆Onigiri work's (its chart stored as
    /// one, [`crate::danoni::STORED_EXT`]).
    pub fn is_danoni_work(&self) -> bool {
        self.chart.as_str().ends_with(crate::danoni::STORED_EXT)
    }

    /// The names the import found the background changes' files by, as
    /// [`crate::scan::song_dir_names`] lists a folder's: those with `,` or
    /// `=` in them, for [`ddi_chart::Song::resolve_change_names`] when the
    /// simfile is read again for a play (the folder is not listed then).
    pub fn change_names(&self) -> Vec<String> {
        let names: Vec<&str> = self
            .bg_images
            .iter()
            .chain(self.bg_shared.iter())
            .chain(self.bg_scripted.iter())
            .chain(self.bg_videos.iter().map(|v| &v.name))
            .map(|p| p.as_str())
            .collect();
        crate::scan::song_dir_names(RelPath::new(""), &names)
    }

    /// The music chart `index` (of `Song::charts`) plays: its own, else the
    /// song's.
    pub fn music_for(&self, index: usize) -> &RelPath {
        self.charts
            .iter()
            .find(|c| c.index == index)
            .and_then(|c| c.music.as_deref())
            .unwrap_or(&self.music)
    }

    /// Whether stored chart data should be reread to add missing versions.
    pub fn needs_chart_variants(&self) -> bool {
        self.variants_version < VARIANTS_VERSION
            && (self.is_danoni_work()
                || self
                    .charts
                    .iter()
                    .any(|c| ddi_chart::simplify::without_onigiri(&c.layout).is_some()))
    }
}

/// Enrich a consumer's metadata from the parsed song without changing its
/// admitted source metadata. Parse through [`crate::scan::parse_simfile`] (or
/// add the song's chart variants first). Managed catalogs and admission receipts
/// keep their original-only entries; shells expose this clone in the library.
pub fn with_chart_variants(entry: &ManifestEntry, song: &Song) -> ManifestEntry {
    let mut entry = entry.clone();
    crate::works::fill_chart_keys(&mut entry, song);
    entry
}

pub(crate) fn chart_variants_version(song: &Song) -> u32 {
    let relevant = song.source.format == SourceFormat::Danoni
        || song
            .charts
            .iter()
            .any(|c| ddi_chart::simplify::without_onigiri(&c.layout).is_some());
    if relevant && song.chart_variants_current() {
        VARIANTS_VERSION
    } else {
        0
    }
}

impl VideoRef {
    /// Whether the game plays it: kept and in a format the decoder has.
    pub fn plays(&self) -> bool {
        self.stored && self.format.playable()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChartInfo {
    /// Provenance of an additional, assisted version of an original chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivation: Option<ddi_chart::simplify::Derivation>,
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
    /// The chart's content ([`ddi_chart::Song::chart_digest`]): what the
    /// play history keeps its bests by, so a song list shows them without
    /// reading the chart. Missing in songs imported before it was kept
    /// (filled when the song's chart is next read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<ddi_chart::ChartDigest>,
    /// A Dancing☆Onigiri chart's own excessive setting (its headers'
    /// `excessiveJdgUse`, `excessiveUse`): part of the key its bests are
    /// kept under. Filled by the import's core (`crate::scan`), and for
    /// older imports when their chart is next read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excessive: Option<bool>,
    /// The chart's own music (`.ssc` per-chart `#MUSIC`) when not the
    /// song's ([`ManifestEntry::music_for`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music: Option<RelPathBuf>,
}

impl ChartInfo {
    /// What the song list shows of `chart`, at `index` in its song, with
    /// its `digest` when known (no excessive setting yet).
    pub fn of(
        index: usize,
        chart: &ddi_chart::Chart,
        digest: Option<ddi_chart::ChartDigest>,
    ) -> ChartInfo {
        ChartInfo {
            derivation: chart.derivation.clone(),
            index,
            layout: chart.layout.clone(),
            difficulty: chart.difficulty.name().to_string(),
            meter: chart.meter,
            name: chart.name.clone(),
            credit: chart.credit.clone(),
            notes: chart.judged_note_count(),
            digest,
            excessive: None,
            music: None,
        }
    }
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
    pub chart: RelPathBuf,
    pub music: RelPathBuf,
    pub banner: Option<RelPathBuf>,
    pub background: Option<RelPathBuf>,
    pub bg_images: Vec<RelPathBuf>,
    pub bg_videos: Vec<VideoRef>,
    pub bg_shared: Vec<RelPathBuf>,
    pub credit: String,
}

/// Builds the song list entry for a parsed song: display BPM from the
/// timing, preview window, and the playable charts (those with a built-in
/// [`Layout`]) sorted by layout, difficulty slot, then meter, since file
/// order is arbitrary.
pub fn summarize(meta: EntryMeta, song: &Song) -> ManifestEntry {
    let danoni = song.source.format == SourceFormat::Danoni;
    let (lo, hi) = if danoni {
        // The timing's 75 BPM is synthetic; the work declares its own.
        match song.display_bpm {
            DisplayBpm::Single(b) => (b, b),
            DisplayBpm::Range(a, b) => (a, b),
            _ => (f64::NAN, f64::NAN),
        }
    } else {
        song.timing.bpm_range()
    };
    let bpm = if !lo.is_finite() || !hi.is_finite() || lo.abs() > 1e6 || hi.abs() > 1e6 {
        String::new()
    } else if (lo - hi).abs() < 0.01 {
        format!("{}", lo.round() as i64)
    } else {
        format!("{}-{}", lo.round() as i64, hi.round() as i64)
    };
    let mut indexed: Vec<_> = song
        .charts
        .iter()
        .enumerate()
        .filter(|(_, c)| song.layout_of(c).is_some())
        .collect();
    if danoni {
        // Works order their charts themselves.
        indexed.sort_by(|(i, a), (j, b)| {
            a.derivation
                .is_some()
                .cmp(&b.derivation.is_some())
                .then(a.layout.cmp(&b.layout))
                .then(i.cmp(j))
        });
    } else {
        indexed.sort_by(|(_, a), (_, b)| {
            a.derivation
                .is_some()
                .cmp(&b.derivation.is_some())
                .then(a.layout.cmp(&b.layout))
                .then(a.difficulty.cmp(&b.difficulty))
                .then(a.meter.cmp(&b.meter))
        });
    }
    let layouts = song
        .layouts
        .iter()
        .filter(|l| indexed.iter().any(|(_, c)| c.layout == l.id))
        .cloned()
        .collect();
    let charts = indexed
        .into_iter()
        .map(|(index, c)| ChartInfo::of(index, c, Some(song.chart_digest(index))))
        .collect();
    ManifestEntry {
        variants_version: chart_variants_version(song),
        id: meta.id,
        title: meta.title,
        artist: meta.artist,
        chart: meta.chart,
        music: meta.music,
        banner: meta.banner,
        background: meta.background,
        jacket: None,
        cd_title: None,
        lyrics: None,
        preview: None,
        bg_images: meta.bg_images,
        bg_videos: meta.bg_videos,
        bg_shared: meta.bg_shared,
        bg_scripted: Vec::new(),
        layer_images: Vec::new(),
        credit: meta.credit,
        layouts,
        bpm,
        preview_start: song.preview_start,
        preview_length: song.preview_length,
        length: song_length(song),
        charts,
        title_translit: [song.title_translit.trim(), song.subtitle_translit.trim()]
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        artist_translit: song.artist_translit.trim().to_string(),
    }
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// Seconds from the start to the end of the last note (a hold's or a
/// roll's end) of the song's playable charts.
pub fn song_length(song: &Song) -> f64 {
    song.charts
        .iter()
        .filter(|c| song.layout_of(c).is_some())
        .filter_map(|c| {
            let timing = c.timing(song);
            c.notes
                .iter()
                .map(|n| n.end_tick())
                .max()
                .map(|t| timing.seconds_at(t))
        })
        .filter(|s| s.is_finite())
        .fold(0.0, f64::max)
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
#NOTES:\n     pump-couple:\n     :\n     Easy:\n     2:\n     0,0,0,0,0:\n10000\n01000\n00100\n00010\n;\n\
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
        assert!(entry.length > 0.0, "{}", entry.length);
        let order: Vec<_> = entry
            .charts
            .iter()
            .map(|c| (c.index, c.difficulty.as_str(), c.notes))
            .collect();
        // pump-couple (two players) has no built-in layout; Easy sorts
        // before Hard.
        assert_eq!(order, [(2, "Easy", 2), (0, "Hard", 4)]);
        // Each with its content's digest, written as hex and read back.
        for c in &entry.charts {
            assert_eq!(c.digest, Some(song.chart_digest(c.index)));
        }
        let json = serde_json::to_string(&entry.charts[0]).unwrap();
        assert!(json.contains(&format!("\"digest\":\"{}\"", song.chart_digest(2))));
        let back: ChartInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry.charts[0]);
        // Entries from before: none.
        let old: ChartInfo =
            serde_json::from_str(r#"{"index":0,"layout":"x","difficulty":"Hard","meter":1}"#)
                .unwrap();
        assert_eq!(old.digest, None);
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
            variants_version: 0,
            id: "a".into(),
            title: "t".into(),
            artist: "r".into(),
            chart: "c.ssc".into(),
            music: "m.ogg".into(),
            banner: None,
            background: Some("bg.png".into()),
            jacket: None,
            cd_title: None,
            lyrics: None,
            preview: None,
            bg_images: Vec::new(),
            bg_videos: Vec::new(),
            bg_shared: Vec::new(),
            bg_scripted: Vec::new(),
            layer_images: Vec::new(),
            credit: String::new(),
            bpm: "120".into(),
            preview_start: 1.0,
            preview_length: 2.0,
            length: 0.0,
            charts: vec![],
            layouts: Vec::new(),
            title_translit: String::new(),
            artist_translit: String::new(),
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
