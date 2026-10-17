//! A song's list entry from its folder, read in place
//! (`docs/plans/desktop.md`, section 4): the simfile parsed, its playable
//! layouts, its music, banner and background found as StepMania finds
//! them ([`crate::pack::resolve_assets`], images told apart by size as a
//! last resort), its background changes' images and movies, the notes and
//! warnings an import reports, and its charts' digests (the keys of their
//! bests). The web's import runs the same steps ([`read_song`],
//! [`Reading::finish`], its reads asynchronous) and copies or links the
//! files; desktop lists the songs where they are ([`scan_song`]).
//!
//! Files come through a [`Reader`] over a folder or a zip, by paths
//! relative to its root ('/'-separated, as [`crate::pack::find_songs`]
//! takes them). The entry's files are paths relative to the song's folder
//! as the web's are (its chart, music, banner and background by their own
//! names, `../` where the simfile reaches out of the folder).

use ddi_chart::formats::sm::parse_simfile;
use ddi_platform::video::VideoFormat;

use crate::image::{IMAGE_HEADER_LEN, image_size};
use crate::manifest::{EntryMeta, ManifestEntry, VideoRef, simfile_names, summarize};
use crate::pack::{self, SongCandidate};
use crate::path::{RelPath, RelPathBuf};

/// The most background-change images a song keeps (the web's
/// `songs::MAX_BG_IMAGES`).
pub const MAX_BG_IMAGES: usize = 32;

/// Files of a folder or a zip, by paths relative to its root.
pub trait Reader {
    /// The whole file.
    fn read(&self, path: &RelPath) -> Result<Vec<u8>, String>;
    /// `len` bytes at `at` (fewer at the end).
    fn read_at(&self, path: &RelPath, at: u64, len: usize) -> Result<Vec<u8>, String> {
        let all = self.read(path)?;
        let start = (at as usize).min(all.len());
        Ok(all[start..(start + len).min(all.len())].to_vec())
    }
}

/// A song listed from its folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Scanned {
    /// Its entry: id `pack::song_id`, files relative to the song folder.
    pub entry: ManifestEntry,
    pub pack: String,
    /// The song folder relative to the root.
    pub dir: RelPathBuf,
    /// What the player should know: a file missing with nothing in its
    /// place, movies the game cannot play.
    pub warnings: Vec<String>,
    /// What is worth saying but changes nothing important.
    pub notes: Vec<String>,
}

/// The song of `c` among the root's `paths`, read through `files`;
/// `shared` are the library's shared folders' files (movies and images a
/// pack shares, as `RandomMovies/`), `shift_jis` decodes a simfile that is
/// not UTF-8 (`encoding_rs` on desktop, `TextDecoder` on the web). Its
/// files are named in place, every movie the game can play is kept.
pub fn scan_song(
    files: &dyn Reader,
    paths: &[RelPathBuf],
    c: &SongCandidate,
    shared: &[RelPathBuf],
    shift_jis: &dyn Fn(&[u8]) -> Option<String>,
) -> Result<Scanned, String> {
    let bytes = files.read(&c.chart)?;
    let mut reading = read_song(c, &bytes, paths, shift_jis)?;
    drop(bytes);
    if !reading.images_to_size().is_empty() {
        let sizes: Vec<(RelPathBuf, (u32, u32))> = reading
            .images_to_size()
            .iter()
            .filter_map(|path| {
                let head = files.read_at(path, 0, IMAGE_HEADER_LEN).ok()?;
                Some((path.clone(), image_size(&head)?))
            })
            .collect();
        reading.sized(&sizes);
    }
    let movies = reading
        .movies(paths)
        .into_iter()
        .map(|(relative, path)| {
            let format = sniff_movie(files, &path);
            Movie {
                kept: format.playable(),
                relative,
                path,
                format,
            }
        })
        .collect();
    Ok(reading
        .finish(paths, shared, movies, Naming::InPlace)?
        .scanned)
}

/// A song's simfile read and its files found by name
/// ([`crate::pack::resolve_assets`]), before what only the files
/// themselves tell: images' sizes ([`Reading::images_to_size`]) and what the
/// movies are ([`Reading::movies`]). The web reads those asynchronously
/// (from `Blob`s), desktop as it goes ([`scan_song`]); both then
/// [`Reading::finish`] the same way.
#[derive(Clone, Debug)]
pub struct Reading {
    pub song: ddi_chart::Song,
    /// The simfile as text (decoded once: the web stores it so).
    pub text: String,
    c: SongCandidate,
    assets: pack::ResolvedAssets,
    warnings: Vec<String>,
    notes: Vec<String>,
}

/// A movie a song's background changes name, and what was found of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Movie {
    /// Its name relative to the song folder (as the entry lists it).
    pub relative: RelPathBuf,
    /// Its path relative to the root.
    pub path: RelPathBuf,
    pub format: VideoFormat,
    /// Kept with the song: a movie the game can play, read in place, or
    /// linked or copied on the web (copied only with "also store
    /// background videos").
    pub kept: bool,
}

/// How an entry names the song's files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Naming {
    /// By their paths relative to the song folder (read in place).
    InPlace,
    /// By their role (`chart.sm`, `music.ogg`, `banner.png`,
    /// `background.jpg`), as the web stores them.
    Stored,
}

/// A song [`Reading::finish`]ed: its list entry, and where the files it
/// names are (paths relative to the root) for a caller that copies them.
#[derive(Clone, Debug)]
pub struct Built {
    pub scanned: Scanned,
    pub song: ddi_chart::Song,
    /// The simfile as text.
    pub text: String,
    pub music: RelPathBuf,
    /// `(name in the entry, path)`.
    pub banner: Option<(RelPathBuf, RelPathBuf)>,
    pub background: Option<(RelPathBuf, RelPathBuf)>,
    /// Background-change images, `(relative, path)`.
    pub bg_images: Vec<(RelPathBuf, RelPathBuf)>,
    /// The movies kept, `(relative, path)`.
    pub videos: Vec<(RelPathBuf, RelPathBuf)>,
    /// Background-change movies that will play.
    pub videos_playing: usize,
    /// Movies the game could play that were not kept.
    pub videos_left_out: usize,
}

/// Reads the simfile `chart` (the bytes of `c.chart`) and finds the
/// song's files among `paths` (its pack's, [`pack_paths`]). Fails when
/// the simfile does not parse or has no chart this game plays.
pub fn read_song(
    c: &SongCandidate,
    chart: &[u8],
    paths: &[RelPathBuf],
    shift_jis: &dyn Fn(&[u8]) -> Option<String>,
) -> Result<Reading, String> {
    let mut notes = Vec::new();
    let text = pack::decode_text_with(chart, shift_jis);
    let song = parse_simfile(&text, c.format.extension()).map_err(|e| e.to_string())?;
    let mut unplayed: Vec<&str> = song
        .charts
        .iter()
        .filter(|ch| song.layout_of(ch).is_none())
        .map(|ch| ch.layout.as_str())
        .collect();
    unplayed.sort_unstable();
    unplayed.dedup();
    if song.playable_charts().next().is_none() {
        return Err(if unplayed.is_empty() {
            "the simfile has no charts".into()
        } else {
            format!(
                "no charts in a layout this game plays; found {}",
                unplayed.join(", ")
            )
        });
    }
    if !unplayed.is_empty() {
        notes.push(format!(
            "charts in layouts this game does not play were left out: {}",
            unplayed.join(", ")
        ));
    }
    let assets = pack::resolve_assets(&song, &c.dir, paths);
    Ok(Reading {
        song,
        text,
        c: c.clone(),
        assets,
        warnings: Vec::new(),
        notes,
    })
}

impl Reading {
    /// The images whose pixel sizes would tell the banner from the
    /// background (StepMania's last resort), when one of them is still
    /// to find; empty otherwise.
    pub fn images_to_size(&self) -> &[RelPathBuf] {
        if self.assets.banner.is_none() || self.assets.background.is_none() {
            &self.assets.unclassified
        } else {
            &[]
        }
    }

    /// The sizes of (some of) [`Reading::images_to_size`]'s images, `(path,
    /// (width, height))`.
    pub fn sized(&mut self, sizes: &[(RelPathBuf, (u32, u32))]) {
        pack::classify_images(&mut self.assets, sizes);
    }

    /// The movies the background changes name, `(relative, path)`.
    pub fn movies(&self, paths: &[RelPathBuf]) -> Vec<(RelPathBuf, RelPathBuf)> {
        crate::backgrounds::referenced_movies(&self.song, &self.c.dir, paths)
    }

    /// The song's entry, its files named as `naming` says, with the
    /// warnings and notes an import reports; `movies` are
    /// [`Reading::movies`] with what each is and whether it is kept.
    /// Fails when the song has no music.
    pub fn finish(
        self,
        paths: &[RelPathBuf],
        shared: &[RelPathBuf],
        movies: Vec<Movie>,
        naming: Naming,
    ) -> Result<Built, String> {
        let Reading {
            song,
            text,
            c,
            assets,
            mut warnings,
            mut notes,
        } = self;
        let music = assets
            .music
            .clone()
            .ok_or("no audio file found next to the simfile")?;
        // A file the simfile names but the folder lacks is only worth a
        // warning when nothing stands in for it: StepMania quietly uses
        // the folder's audio or images instead (`Song::TidyUpData`), and
        // packs often rename files without updating the simfile.
        for (tag, reference) in &assets.missing {
            let stand_in = match *tag {
                "#BANNER" => assets.banner.is_some(),
                "#BACKGROUND" => assets.background.is_some(),
                _ => true,
            };
            if !stand_in {
                warnings.push(format!(
                    "{tag} names {reference}, which is missing; the song has no {}",
                    tag.trim_start_matches('#').to_lowercase()
                ));
                continue;
            }
            let used = match *tag {
                "#MUSIC" => Some(&music),
                "#BANNER" => assets.banner.as_ref(),
                _ => assets.background.as_ref(),
            };
            if let Some(used) = used {
                notes.push(format!(
                    "{tag} names {reference}, which is missing; {} is used instead",
                    used.file_name()
                ));
            }
        }
        let (id, name) = c.identity();
        let (title, artist) = simfile_names(&song, &name);
        let named = |role: &str, path: &RelPath| -> RelPathBuf {
            match naming {
                Naming::InPlace => path.relative_to(&c.dir),
                Naming::Stored => match extension(path) {
                    Some(ext) => format!("{role}.{ext}").into(),
                    None => role.into(),
                },
            }
        };
        let chart = match naming {
            Naming::InPlace => c.chart.relative_to(&c.dir),
            Naming::Stored => format!("chart.{}", c.format.extension()).into(),
        };
        let music_name = named("music", &music);
        let banner = assets.banner.map(|p| (named("banner", &p), p));
        let background = assets.background.map(|p| (named("background", &p), p));
        let mut bg_images = crate::backgrounds::referenced_images(
            &song,
            &c.dir,
            paths,
            background.as_ref().map(|(_, p)| p.as_rel_path()),
        );
        if bg_images.len() > MAX_BG_IMAGES {
            warnings.push(format!(
                "{} background-change images; only the first {MAX_BG_IMAGES} are kept",
                bg_images.len()
            ));
            bg_images.truncate(MAX_BG_IMAGES);
        }
        let mut bg_videos = Vec::new();
        let mut videos = Vec::new();
        let mut unplayable = Vec::new();
        let mut left_out = Vec::new();
        for m in movies {
            let playable = m.format.playable();
            let kept = playable && m.kept;
            if !playable {
                unplayable.push(format!("{} ({})", m.relative, m.format.describe()));
            } else if !kept {
                left_out.push(m.relative.clone());
            }
            if kept {
                videos.push((m.relative.clone(), m.path));
            }
            bg_videos.push(VideoRef {
                name: m.relative,
                format: m.format,
                stored: kept,
            });
        }
        let videos_playing = bg_videos.iter().filter(|v| v.plays()).count();
        if !unplayable.is_empty() {
            warnings.push(format!(
                "background videos in a format the game cannot play (the song background shows instead): {}",
                unplayable.join(", ")
            ));
        }
        if !left_out.is_empty() {
            notes.push(format!(
                "background videos not stored (\"also store background videos\" is off): {}",
                left_out
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let missing = crate::backgrounds::missing_files(&song, &c.dir, paths, &c.pack, shared);
        let unshown = crate::backgrounds::unshown_files(&song, &c.dir, paths, &c.pack, shared);
        let (present, absent): (Vec<_>, Vec<_>) =
            unshown.into_iter().partition(|(_, exists)| *exists);
        let scripted: Vec<RelPathBuf> = present.iter().map(|(n, _)| n.into()).collect();
        let names = |list: Vec<(String, bool)>| {
            list.into_iter()
                .map(|(n, _)| n)
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !present.is_empty() {
            notes.push(format!(
                "background changes use scripted animations, which are not played (the song background shows instead): {}",
                names(present)
            ));
        }
        if !absent.is_empty() {
            notes.push(format!(
                "background changes name scripted animations that are missing (they would not be played anyway): {}",
                names(absent)
            ));
        }
        if !missing.is_empty() {
            warnings.push(format!(
                "background changes name images or videos that were not found (the song background shows instead): {}",
                missing.join(", ")
            ));
        }
        let mut entry = summarize(
            EntryMeta {
                id,
                title,
                artist,
                chart,
                music: music_name,
                banner: banner.as_ref().map(|(n, _)| n.clone()),
                background: background.as_ref().map(|(n, _)| n.clone()),
                bg_images: bg_images.iter().map(|(r, _)| r.clone()).collect(),
                bg_videos,
                bg_shared: crate::backgrounds::absent_files(&song, &c.dir, paths),
                credit: song.credit.trim().to_string(),
            },
            &song,
        );
        entry.bg_scripted = scripted;
        // The keys the song list finds its charts' bests by (a work's
        // chart's own excessive setting among them).
        for chart in &mut entry.charts {
            if chart.index < song.charts.len() {
                chart.digest = Some(song.chart_digest(chart.index));
                chart.excessive =
                    ddi_engine::rules::danoni::chart_excessive(&song.charts[chart.index]);
            }
        }
        // A non-finite float does not survive JSON (`#SAMPLESTART:1e999`
        // parses to inf, which would not read back and would hide the
        // song).
        for v in [&mut entry.preview_start, &mut entry.preview_length] {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
        Ok(Built {
            scanned: Scanned {
                entry,
                pack: c.pack.clone(),
                dir: c.dir.clone(),
                warnings,
                notes,
            },
            song,
            text,
            music,
            banner,
            background,
            bg_images,
            videos,
            videos_playing,
            videos_left_out: left_out.len(),
        })
    }
}

/// Lowercase extension of a path's file name.
fn extension(path: &RelPath) -> Option<String> {
    let (_, ext) = path.file_name().rsplit_once('.')?;
    (!ext.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// What a movie is ([`crate::video::sniff`], the MP4 index read where its
/// head points).
pub fn sniff_movie(files: &dyn Reader, path: &RelPath) -> VideoFormat {
    use crate::video::{MP4_INDEX_LEN, SNIFF_LEN, mp4_index_at, mp4_index_codec, sniff};
    let head = files.read_at(path, 0, SNIFF_LEN).unwrap_or_default();
    let mut format = sniff(&head);
    if let Some(at) = mp4_index_at(&head)
        && let Ok(index) = files.read_at(path, at, MP4_INDEX_LEN)
    {
        format.codec = mp4_index_codec(&index);
    }
    format
}

/// What a root held: its songs, and the folders whose simfile did not
/// read (`(dir, why)`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RootScan {
    pub songs: Vec<Scanned>,
    pub failed: Vec<(RelPathBuf, String)>,
}

/// Every song among `paths` (the root's files, sorted), packs named as
/// [`pack::find_songs`] names them (`fallback_pack` for songs at or under
/// the root). Two song folders with the same pack and folder names share
/// an id: the first is kept and the other reported, as the web's import
/// does.
pub fn scan_root(
    files: &dyn Reader,
    paths: &[RelPathBuf],
    fallback_pack: &str,
    shared: &[RelPathBuf],
    shift_jis: &dyn Fn(&[u8]) -> Option<String>,
) -> RootScan {
    let mut out = RootScan::default();
    let mut seen = Duplicates::default();
    for c in pack::find_songs(paths, fallback_pack) {
        if let Some(e) = seen.check(&c) {
            out.failed.push((c.dir.clone(), e));
            continue;
        }
        let paths = pack_paths(paths, &c);
        match scan_song(files, paths, &c, shared, shift_jis) {
            Ok(s) => out.songs.push(s),
            Err(e) => out.failed.push((c.dir.clone(), e)),
        }
    }
    out
}

/// The ids an import has seen, and where.
#[derive(Default)]
pub struct Duplicates(std::collections::HashMap<String, RelPathBuf>);

impl Duplicates {
    /// Why `c` is left out (its id was seen), or `None` (first seen now).
    pub fn check(&mut self, c: &SongCandidate) -> Option<String> {
        let (id, _) = c.identity();
        let label = if c.dir.is_empty() {
            c.chart.clone()
        } else {
            c.dir.clone()
        };
        match self.0.get(&id) {
            Some(first) => Some(format!("same pack and folder name as {first}")),
            None => {
                self.0.insert(id, label);
                None
            }
        }
    }
}

/// The paths a song's files may come from: its pack folder's (where a
/// simfile's `../` reaches), out of the root's sorted `paths`, so a scan
/// does not go through the whole library for each song.
pub fn pack_paths<'a>(paths: &'a [RelPathBuf], c: &SongCandidate) -> &'a [RelPathBuf] {
    match c.pack_dir() {
        Some(dir) if !dir.is_empty() => {
            let prefix = format!("{dir}/");
            let start = paths.partition_point(|p| p.as_str() < prefix.as_str());
            let len = paths[start..].partition_point(|p| p.as_str().starts_with(&prefix));
            &paths[start..start + len]
        }
        _ => paths,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    struct Files(BTreeMap<String, Vec<u8>>);

    impl Reader for Files {
        fn read(&self, path: &RelPath) -> Result<Vec<u8>, String> {
            self.0
                .get(path.as_str())
                .cloned()
                .ok_or(format!("no {path}"))
        }
    }

    const SM: &str = "#TITLE:Song;#SUBTITLE:Sub;#ARTIST:Me;#MUSIC:song.ogg;#BANNER:bn.png;\
        #OFFSET:0;#BPMS:0=120;#SAMPLESTART:10;#SAMPLELENGTH:12;\
        #NOTES:dance-single:x:Easy:3:0,0,0,0,0:1000\n0100\n0010\n0001\n1000\n;\
        #NOTES:dance-couple:x:Easy:3:0,0,0,0,0:10001000\n00000000\n00000000\n00000000\n;";

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v.extend([8, 6, 0, 0, 0]);
        v
    }

    #[test]
    fn a_pack_folder_lists_as_the_web_imports_it() {
        let mut f = BTreeMap::new();
        f.insert("Pack/Song A/song.sm".into(), SM.as_bytes().to_vec());
        f.insert("Pack/Song A/SONG.OGG".into(), vec![0; 10]);
        // Named in another case, found anyway (StepMania's lookups).
        f.insert("Pack/Song A/BN.PNG".into(), png(256, 80));
        f.insert("Pack/Song A/bg-wide.png".into(), png(640, 480));
        f.insert("Pack/Song B/b.sm".into(), b"#TITLE:B;#BPMS:0=120;".to_vec());
        f.insert("Pack/notes.txt".into(), Vec::new());
        let paths: Vec<RelPathBuf> = f.keys().map(RelPathBuf::from).collect();
        let files = Files(f);
        let scan = scan_root(&files, &paths, "Fallback", &[], &|_| None);
        assert_eq!(scan.songs.len(), 1);
        let s = &scan.songs[0];
        assert_eq!(s.entry.id, pack::song_id("Pack", "Pack/Song A"));
        assert_eq!(s.pack, "Pack");
        assert_eq!(s.entry.title, "Song Sub");
        assert_eq!(s.entry.chart, "song.sm");
        assert_eq!(s.entry.music, "SONG.OGG");
        assert_eq!(
            s.entry.banner.as_deref().map(RelPath::as_str),
            Some("BN.PNG")
        );
        // The other image, 4:3, is the background.
        assert_eq!(
            s.entry.background.as_deref().map(RelPath::as_str),
            Some("bg-wide.png")
        );
        assert_eq!(s.entry.charts.len(), 1);
        assert!(s.entry.charts[0].digest.is_some());
        assert_eq!(s.entry.preview_start, 10.0);
        assert!(s.notes[0].contains("dance-couple"), "{:?}", s.notes);
        // Song B has no playable chart.
        assert_eq!(scan.failed.len(), 1);
        assert_eq!(scan.failed[0].0, "Pack/Song B");
    }

    #[test]
    fn missing_music_fails_and_a_stand_in_is_noted() {
        let mut f = BTreeMap::new();
        f.insert(
            "S/s.sm".into(),
            SM.replace("song.ogg", "gone.ogg").into_bytes(),
        );
        f.insert("S/other.mp3".into(), vec![0; 4]);
        let paths: Vec<RelPathBuf> = f.keys().map(RelPathBuf::from).collect();
        let scan = scan_root(&Files(f.clone()), &paths, "Mine", &[], &|_| None);
        let s = &scan.songs[0];
        assert_eq!(s.pack, "Mine");
        assert_eq!(s.entry.music, "other.mp3");
        assert!(
            s.notes.iter().any(|n| n.contains("gone.ogg")),
            "{:?}",
            s.notes
        );
        // No banner either: a warning.
        assert!(
            s.warnings.iter().any(|w| w.contains("#BANNER")),
            "{:?}",
            s.warnings
        );
        f.remove("S/other.mp3");
        let paths: Vec<RelPathBuf> = f.keys().map(RelPathBuf::from).collect();
        let scan = scan_root(&Files(f), &paths, "Mine", &[], &|_| None);
        assert!(scan.songs.is_empty());
        assert!(scan.failed[0].1.contains("no audio"));
    }
    #[test]
    fn the_second_layers_files_and_scripted_animations_are_kept() {
        let mut f = BTreeMap::new();
        let sm = SM.replace(
            "#BPMS:0=120;",
            "#BPMS:0=120;#BGCHANGES2:0=over.png=1=0=0=0,2=Spin=1=0=0=0,4=gone=1=0=0=0;",
        );
        f.insert("P/S/s.sm".into(), sm.into_bytes());
        f.insert("P/S/song.ogg".into(), vec![0; 4]);
        f.insert("P/S/over.png".into(), png(64, 48));
        f.insert("P/S/spin/default.xml".into(), Vec::new());
        let paths: Vec<RelPathBuf> = f.keys().map(RelPathBuf::from).collect();
        let scan = scan_root(&Files(f), &paths, "Mine", &[], &|_| None);
        let e = &scan.songs[0].entry;
        assert_eq!(e.bg_images, vec!["over.png"]);
        // The animation that exists (another case: found anyway), not the
        // one found nowhere.
        assert_eq!(e.bg_scripted, vec!["Spin"]);
    }
}
