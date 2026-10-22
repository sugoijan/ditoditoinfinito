//! FFR levels among an import's files or a song folder's
//! (`docs/plans/phase-10.md`, step 2), for both shells: which files may be
//! levels ([`find_levels`]), each read into a song ([`read_level`]) with
//! its list entry ([`entry`]), and read again for a play ([`chart_song`],
//! [`music`]).
//!
//! A level is one file, and one song: a level SWF, holding its chart and
//! its music (`crate::swf`), or a `beatBox` written as text (`.as`, as a
//! decompiler exports a level's actions; `.beatbox`; a `.txt` named for
//! it) with its music beside it (the audio file of the same name, else
//! the folder's only one). Several levels share a folder; the folder is
//! their pack (a level at the import's root goes in the import's pack).
//! A file that turns out not to be a level (a SWF without a `beatBox`, an
//! `.as` of other actions) is no song and nothing to report.
//!
//! The web stores what it read (the `beatBox` as JSON, the music
//! extracted: [`STORED_CHART`]); desktop reads the files in place, the
//! music out of the SWF each time it plays.

use ddi_chart::Song;
use ddi_chart::formats::ffr::{LevelInfo, Value, parse_beatbox_text, song_from_beatbox};

use crate::manifest::{EntryMeta, ManifestEntry, summarize};
use crate::pack::{self, AUDIO_EXTENSIONS};
use crate::path::{RelPath, RelPathBuf};

/// The largest file read as a possible level, bytes (a level is its
/// music: a few megabytes).
pub const MAX_LEVEL: u64 = 64 << 20;
/// The name a stored level's chart has (the `beatBox` as JSON).
pub const STORED_CHART: &str = "chart.beatbox";

/// Whether `path` may be a level: a SWF, an `.as` or `.beatbox` file, or a
/// `.txt` whose name says `beatbox`.
pub fn is_level_file(path: &str) -> bool {
    let p = RelPath::new(path);
    match p.extension().map(str::to_ascii_lowercase).as_deref() {
        Some("swf" | "as" | "beatbox") => true,
        Some("txt") => p.file_stem().to_ascii_lowercase().contains("beatbox"),
        _ => false,
    }
}

/// Whether `path` names a SWF: a level that holds its music, an MP3
/// ([`music`]).
pub fn is_swf_name(path: &RelPath) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("swf"))
}

/// The extension of the audio [`music`] gives for song music file `file`
/// (`mp3` for a SWF's), as the file names it.
pub fn music_extension(file: &RelPath) -> Option<&str> {
    if is_swf_name(file) {
        Some("mp3")
    } else {
        file.extension()
    }
}

/// A file that may be a level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelCandidate {
    /// The file, relative to the import's root.
    pub path: RelPathBuf,
    /// Its folder (the pack's).
    pub dir: RelPathBuf,
    pub pack: String,
    /// `pack::song_id` of the pack and the file's name.
    pub id: String,
    /// The file's name without its extension.
    pub title: String,
    /// A `beatBox` text's music beside it (relative to the root); none
    /// for a SWF, or when the folder has none.
    pub music: Option<RelPathBuf>,
}

/// The files of `paths` that may be levels, by path: not in a song folder
/// of `songs` (`pack::find_songs` of the same paths: that folder is a
/// StepMania song), not ignored (`pack::is_ignored`). `fallback_pack`
/// names the pack of levels at the import's root.
pub fn find_levels<S: AsRef<str>>(
    paths: &[S],
    songs: &[pack::SongCandidate],
    fallback_pack: &str,
) -> Vec<LevelCandidate> {
    let paths: Vec<&str> = paths.iter().map(AsRef::as_ref).collect();
    let song_dirs: std::collections::HashSet<&RelPath> =
        songs.iter().map(|c| c.dir.as_rel_path()).collect();
    let mut out: Vec<LevelCandidate> = paths
        .iter()
        .filter(|p| !pack::is_ignored(p) && is_level_file(p))
        .filter_map(|p| {
            let path = RelPath::new(p);
            let dir = path.parent();
            if song_dirs.contains(dir) {
                return None;
            }
            let pack = match dir.file_name() {
                "" => fallback_pack,
                name => name,
            };
            let title = path.file_stem().to_string();
            let music = (!is_swf_name(path))
                .then(|| music_beside(path, &paths))
                .flatten();
            Some(LevelCandidate {
                path: path.to_rel_path_buf(),
                dir: dir.to_rel_path_buf(),
                pack: pack.to_string(),
                id: pack::song_id(pack, path.file_name()),
                title,
                music,
            })
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// A text level's music: the audio file of its name in its folder
/// (ignoring case), else the folder's only audio file.
fn music_beside(level: &RelPath, paths: &[&str]) -> Option<RelPathBuf> {
    let dir = level.parent();
    let audio: Vec<&RelPath> = paths
        .iter()
        .map(RelPath::new)
        .filter(|p| {
            p.parent() == dir
                && !pack::is_ignored(p.as_str())
                && p.extension()
                    .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .collect();
    let stem = level.file_stem().to_lowercase();
    audio
        .iter()
        .find(|p| p.file_stem().to_lowercase() == stem)
        .or(match audio.as_slice() {
            [only] => Some(only),
            _ => None,
        })
        .map(|p| p.to_rel_path_buf())
}

/// A level read.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelSong {
    pub song: Song,
    /// A SWF's music, an MP3 file; none for a text level (its music is
    /// [`LevelCandidate::music`]).
    pub mp3: Option<Vec<u8>>,
    /// The `beatBox` as JSON (what the web stores, [`STORED_CHART`]).
    pub beatbox: String,
}

/// Candidate `c` read from its bytes: `Ok(None)` when it is not a level
/// (a SWF without a `beatBox`, a text that is not one), an error when it
/// is one that cannot be played (no notes, no music).
pub fn read_level(c: &LevelCandidate, bytes: &[u8]) -> Result<Option<LevelSong>, String> {
    let info = LevelInfo {
        title: c.title.clone(),
        ..LevelInfo::default()
    };
    let (beatbox, mp3) = if is_swf_name(&c.path) {
        if !crate::swf::is_swf(bytes) {
            return Ok(None);
        }
        let level = crate::swf::read(bytes)?;
        let Some(beatbox) = level.beatbox else {
            return Ok(None);
        };
        if level.mp3.is_empty() {
            return Err("the level has no MP3 music".into());
        }
        (beatbox, Some(level.mp3))
    } else {
        let text = pack::decode_text(bytes);
        let beatbox = match parse_beatbox_text(&text) {
            Ok(v) => v,
            // An `.as` or `.txt` of something else: not a level.
            Err(_)
                if !c
                    .path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("beatbox")) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e.to_string()),
        };
        if c.music.is_none() {
            return Err("no audio file beside the beatBox".into());
        }
        (beatbox, None)
    };
    let song = song_from_beatbox(&beatbox, &info).map_err(|e| e.to_string())?;
    Ok(Some(LevelSong {
        song,
        mp3,
        beatbox: beatbox.to_json(),
    }))
}

/// A level's list entry, its files named `chart` and `music` (relative to
/// its folder in place; [`STORED_CHART`] and the stored music's name on
/// the web).
pub fn entry(c: &LevelCandidate, song: &Song, chart: &str, music: &str) -> ManifestEntry {
    let mut entry = summarize(
        EntryMeta {
            id: c.id.clone(),
            title: c.title.clone(),
            chart: chart.into(),
            music: music.into(),
            ..EntryMeta::default()
        },
        song,
    );
    crate::works::fill_chart_keys(&mut entry, song);
    entry
}

/// The names a level's files have in place, relative to its folder:
/// `(chart, music)`.
pub fn names_in_place(c: &LevelCandidate) -> (String, String) {
    let chart = c.path.file_name().to_string();
    let music = c
        .music
        .as_ref()
        .map_or_else(|| chart.clone(), |m| m.file_name().to_string());
    (chart, music)
}

/// Whether a song's chart file (an entry's `chart`) is a level's: read
/// with [`chart_song`], not as a simfile.
pub fn is_level_chart(chart: &RelPath) -> bool {
    is_level_file(chart.as_str())
}

/// A level's song read again from its chart file's bytes (a SWF, a text
/// level, or a stored `beatBox`), titled `title`.
pub fn chart_song(chart: &RelPath, bytes: &[u8], title: &str) -> Result<Song, String> {
    chart_song_and_music(chart, bytes, title).map(|(song, _)| song)
}

/// [`chart_song`], with the music when the chart file holds it (a SWF's
/// MP3, as [`music`] of the same file gives it), the file read once.
pub fn chart_song_and_music(
    chart: &RelPath,
    bytes: &[u8],
    title: &str,
) -> Result<(Song, Option<Vec<u8>>), String> {
    let (beatbox, mp3): (Value, _) = if is_swf_name(chart) {
        let level = crate::swf::read(bytes)?;
        let beatbox = level.beatbox.ok_or("the level has no beatBox")?;
        (beatbox, Some(level.mp3).filter(|m| !m.is_empty()))
    } else {
        let text = pack::decode_text(bytes);
        (parse_beatbox_text(&text).map_err(|e| e.to_string())?, None)
    };
    let info = LevelInfo {
        title: title.to_string(),
        ..LevelInfo::default()
    };
    let song = song_from_beatbox(&beatbox, &info).map_err(|e| e.to_string())?;
    Ok((song, mp3))
}

/// A song's music file's bytes as audio: a SWF's MP3 taken out of it
/// (`mp3` its extension), any other file as it is. With the extension the
/// decoder is given.
pub fn music(file: &RelPath, bytes: Vec<u8>) -> Result<(Vec<u8>, Option<String>), String> {
    let ext = music_extension(file).map(str::to_ascii_lowercase);
    if is_swf_name(file) {
        let mp3 = crate::swf::read(&bytes)?.mp3;
        if mp3.is_empty() {
            return Err("the level has no MP3 music".into());
        }
        return Ok((mp3, ext));
    }
    Ok((bytes, ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes() -> Vec<(f64, &'static str, &'static str)> {
        (0..16)
            .map(|i| {
                (
                    30.0 + 6.0 * f64::from(i),
                    ["L", "D", "U", "R"][i as usize % 4],
                    "red",
                )
            })
            .collect()
    }

    fn find(paths: &[&str], fallback_pack: &str) -> Vec<LevelCandidate> {
        find_levels(
            paths,
            &pack::find_songs(paths, fallback_pack),
            fallback_pack,
        )
    }

    #[test]
    fn levels_are_found_one_a_file_their_folder_their_pack() {
        let paths = [
            "FFR/level_1.swf",
            "FFR/Level_2.SWF",
            "FFR/._level_3.swf",
            "FFR/notes.txt",
            "FFR/my beatbox.txt",
            "FFR/my beatbox.mp3",
            "FFR/Other.mp3",
            "Decompiled/frame_1/DoAction.as",
            "Decompiled/frame_1/song.ogg",
            "Pack/Song/song.sm",
            "Pack/Song/intro.swf",
            "loose.swf",
        ];
        let found = find(&paths, "Imported");
        let got: Vec<(&str, &str, &str, Option<&str>)> = found
            .iter()
            .map(|c| {
                (
                    c.path.as_str(),
                    c.pack.as_str(),
                    c.title.as_str(),
                    c.music.as_ref().map(|m| m.as_str()),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    "Decompiled/frame_1/DoAction.as",
                    "frame_1",
                    "DoAction",
                    Some("Decompiled/frame_1/song.ogg")
                ),
                ("FFR/Level_2.SWF", "FFR", "Level_2", None),
                ("FFR/level_1.swf", "FFR", "level_1", None),
                (
                    "FFR/my beatbox.txt",
                    "FFR",
                    "my beatbox",
                    Some("FFR/my beatbox.mp3")
                ),
                ("loose.swf", "Imported", "loose", None),
            ]
        );
        // One id a file, apart from a song folder of the same name.
        assert_ne!(found[1].id, found[2].id);
        assert_ne!(found[2].id, pack::song_id("FFR", "level_1"));
    }

    #[test]
    fn a_swf_level_is_a_song_with_its_music_inside() {
        let mp3 = vec![0xff, 0xfb, 0x90, 0x64, 1, 2, 3];
        let swf = crate::swf::write_level(&notes(), &mp3, true);
        let c = &find(&["FFR/level_9.swf"], "x")[0];
        let read = read_level(c, &swf).unwrap().unwrap();
        assert_eq!(read.mp3.as_deref(), Some(&mp3[..]));
        assert_eq!(read.song.title, "level_9");
        assert_eq!(read.song.charts[0].notes.len(), 16);
        let (chart, music_name) = names_in_place(c);
        assert_eq!(
            (chart.as_str(), music_name.as_str()),
            ("level_9.swf", "level_9.swf")
        );
        let e = entry(c, &read.song, &chart, &music_name);
        assert_eq!(e.id, c.id);
        assert_eq!(e.title, "level_9");
        assert_eq!(e.bpm, "");
        assert_eq!(e.charts.len(), 1);
        assert!(e.charts[0].digest.is_some());
        assert!(e.preview_length > 0.0 && (e.preview_start - 1.0).abs() < 1e-9);
        // Read again for a play: the same chart; the music out of it.
        let again = chart_song(RelPath::new(&chart), &swf, &e.title).unwrap();
        assert_eq!(again.chart_digest(0), read.song.chart_digest(0));
        let (both, inside) = chart_song_and_music(RelPath::new(&chart), &swf, &e.title).unwrap();
        assert_eq!(both.chart_digest(0), read.song.chart_digest(0));
        assert_eq!(inside.as_deref(), Some(&mp3[..]));
        let (bytes, ext) = music(RelPath::new(&music_name), swf.clone()).unwrap();
        assert_eq!((bytes, ext.as_deref()), (mp3.clone(), Some("mp3")));
        // As the web stores it: the beatBox as JSON.
        let stored = chart_song(RelPath::new(STORED_CHART), read.beatbox.as_bytes(), "t").unwrap();
        assert_eq!(stored.chart_digest(0), read.song.chart_digest(0));
        // Other music passes as it is.
        let (bytes, ext) = music(RelPath::new("a.OGG"), vec![1, 2]).unwrap();
        assert_eq!((bytes, ext.as_deref()), (vec![1, 2], Some("ogg")));
    }

    #[test]
    fn what_is_not_a_level_is_no_song_and_broken_levels_say_why() {
        let c = |p: &str, extra: &[&str]| {
            let mut paths = vec![p];
            paths.extend_from_slice(extra);
            find(&paths, "x").remove(0)
        };
        // A SWF of something else; not a SWF at all; an `.as` of other
        // actions.
        let other = crate::swf::write_level(&[], &[], false);
        assert_eq!(read_level(&c("a.swf", &[]), b"PK\x03\x04").unwrap(), None);
        assert_eq!(
            read_level(&c("a.as", &["a.mp3"]), b"stop(); gotoAndPlay(2);").unwrap(),
            None
        );
        // A level without notes, without music; a text one without audio;
        // a `.beatbox` that does not read.
        assert!(read_level(&c("a.swf", &[]), &other).is_err());
        let silent = crate::swf::write_level(&notes(), &[], false);
        assert!(read_level(&c("a.swf", &[]), &silent).is_err());
        assert!(read_level(&c("a.as", &[]), b"_root.beatBox = [[30,'L']];").is_err());
        assert!(read_level(&c("a.beatbox", &["a.mp3"]), b"not one").is_err());
        let text = read_level(&c("a.as", &["a.mp3"]), b"_root.beatBox = [[30,'L']];")
            .unwrap()
            .unwrap();
        assert_eq!(text.mp3, None);
        assert_eq!(text.song.charts[0].notes.len(), 1);
    }
}
