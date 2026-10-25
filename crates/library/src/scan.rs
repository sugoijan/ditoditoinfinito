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

use ddi_platform::video::VideoFormat;

use crate::image::{IMAGE_HEADER_LEN, image_size};
use crate::manifest::{EntryMeta, ManifestEntry, VideoRef, simfile_names, summarize};
use crate::pack::{self, MissingTag, SongCandidate};
use crate::path::{RelPath, RelPathBuf};

/// The most background-change images a song keeps (the web's
/// `songs::MAX_BG_IMAGES`).
pub const MAX_BG_IMAGES: usize = 32;

/// Files of a song's scripted backgrounds and foregrounds kept, at most
/// (`docs/plans/scripted-backgrounds.md`, decision 8).
pub const MAX_SCRIPT_FILES: usize = 64;

/// Reads a simfile for the library or a play, appending any generated
/// chart versions. The format importer itself preserves the source charts.
pub fn parse_simfile(
    text: &str,
    extension: &str,
) -> Result<ddi_chart::Song, ddi_chart::formats::ParseError> {
    let mut song = ddi_chart::formats::sm::parse_simfile(text, extension)?;
    song.add_chart_versions();
    Ok(song)
}

/// The files and folders under the song folder `dir` among `paths`
/// (relative to the root), relative to `dir`: what StepMania matches a
/// background change's file names against (`SMLoader::GetSongDirFiles`;
/// [`ddi_chart::Song::resolve_change_names`]). Only names with `,` or `=`
/// in them change how changes split, so only those are listed.
pub fn song_dir_names<S: AsRef<str>>(dir: &RelPath, paths: &[S]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for p in paths {
        let Some(rel) = RelPath::new(p.as_ref()).strip_prefix(dir) else {
            continue;
        };
        let rel = rel.as_str();
        // The file and each folder it is in.
        let mut end = rel.len();
        while rel[..end].contains([',', '=']) {
            names.push(rel[..end].to_string());
            match rel[..end].rfind('/') {
                Some(i) => end = i,
                None => break,
            }
        }
    }
    names.sort_unstable();
    names.dedup();
    names
}

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
/// pack shares, as `RandomMovies/`), `legacy` decodes a simfile that is
/// not UTF-8 (`encoding_rs` on desktop, `TextDecoder` on the web). Its
/// files are named in place, every movie the game can play is kept.
pub fn scan_song(
    files: &dyn Reader,
    paths: &[RelPathBuf],
    c: &SongCandidate,
    shared: &[RelPathBuf],
    legacy: crate::legacy::Decoder,
) -> Result<Scanned, String> {
    let bytes = files.read(&c.chart)?;
    let mut reading = read_song(c, &bytes, paths, legacy)?;
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
        .movies()
        .iter()
        .map(|(relative, path)| {
            let format = sniff_movie(files, path);
            Movie {
                kept: format.playable(),
                relative: relative.clone(),
                path: path.clone(),
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
    /// [`Reading::movies`].
    movies: Vec<(RelPathBuf, RelPathBuf)>,
    /// Whether a background change plays the `#PREVIEWVID` movie too.
    preview_in_backgrounds: bool,
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
    /// The jacket, CD title, `.lrc` lyrics and `#PREVIEW` sound, `(name in
    /// the entry, path)`.
    pub jacket: Option<(RelPathBuf, RelPathBuf)>,
    pub cd_title: Option<(RelPathBuf, RelPathBuf)>,
    pub lyrics: Option<(RelPathBuf, RelPathBuf)>,
    pub preview: Option<(RelPathBuf, RelPathBuf)>,
    /// The charts' own music files other than the song's, `(name in the
    /// entry, path)` (the entry's charts name theirs).
    pub chart_music: Vec<(RelPathBuf, RelPathBuf)>,
    /// Background-change images, `(relative, path)`.
    pub bg_images: Vec<(RelPathBuf, RelPathBuf)>,
    /// The scripted backgrounds' and foregrounds' files, `(relative,
    /// path)` ([`crate::manifest::ManifestEntry::bg_scripts`]).
    pub script_files: Vec<(RelPathBuf, RelPathBuf)>,
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
    legacy: crate::legacy::Decoder,
) -> Result<Reading, String> {
    let mut notes = Vec::new();
    let text = pack::decode_text_with(chart, legacy);
    let mut song = parse_simfile(&text, c.format.extension()).map_err(|e| e.to_string())?;
    debug_assert!(paths.is_sorted(), "a song's paths are sorted");
    song.resolve_change_names(&song_dir_names(&c.dir, paths_under(paths, &c.dir)));
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
    let mut movies = crate::backgrounds::referenced_movies(&song, &c.dir, paths);
    let mut preview_in_backgrounds = false;
    if let Some(p) = &assets.preview_video {
        preview_in_backgrounds = movies.iter().any(|(_, q)| q == p);
        if !preview_in_backgrounds {
            movies.push((p.relative_to(&c.dir), p.clone()));
        }
    }
    Ok(Reading {
        song,
        text,
        c: c.clone(),
        assets,
        movies,
        preview_in_backgrounds,
        warnings: Vec::new(),
        notes,
    })
}

impl Reading {
    /// The images whose pixel sizes would tell the banner, the background
    /// and the CD title (and a jacket) apart (StepMania's last resort),
    /// when one of the first three is still to find; empty otherwise.
    pub fn images_to_size(&self) -> &[RelPathBuf] {
        let a = &self.assets;
        if a.banner.is_none() || a.background.is_none() || a.cd_title.is_none() {
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

    /// The movies the background changes name, then the `#PREVIEWVID`
    /// movie when it is not one of them, `(relative, path)`.
    pub fn movies(&self) -> &[(RelPathBuf, RelPathBuf)] {
        &self.movies
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
            movies: _,
            preview_in_backgrounds,
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
            // The file that stands in, when one does.
            let used = match tag {
                MissingTag::Jacket
                | MissingTag::CdTitle
                | MissingTag::LyricsPath
                | MissingTag::PreviewVid => {
                    notes.push(format!("{tag} names {reference}, which is missing"));
                    continue;
                }
                MissingTag::Preview => {
                    notes.push(format!(
                        "{tag} names {reference}, which is missing; the music's sample plays instead"
                    ));
                    continue;
                }
                MissingTag::ChartMusic(_) => {
                    notes.push(format!(
                        "{tag} names {reference}, which is missing; the song's music plays instead"
                    ));
                    continue;
                }
                MissingTag::Music => Some(&music),
                MissingTag::Banner => assets.banner.as_ref(),
                MissingTag::Background => assets.background.as_ref(),
            };
            let Some(used) = used else {
                warnings.push(format!(
                    "{tag} names {reference}, which is missing; the song has no {}",
                    tag.to_string().trim_start_matches('#').to_lowercase()
                ));
                continue;
            };
            notes.push(format!(
                "{tag} names {reference}, which is missing; {} is used instead",
                used.file_name()
            ));
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
        let jacket = assets.jacket.map(|p| (named("jacket", &p), p));
        let cd_title = assets.cd_title.map(|p| (named("cdtitle", &p), p));
        let lyrics = assets.lyrics.map(|p| (named("lyrics", &p), p));
        let preview = assets.preview.map(|p| (named("preview", &p), p));
        // Each chart's own music file once, in order of first use; the
        // song's music file is the song's.
        let mut chart_music: Vec<(RelPathBuf, RelPathBuf)> = Vec::new();
        let mut music_of: Vec<(usize, RelPathBuf)> = Vec::new();
        for (i, p) in assets.chart_music {
            let name = if p == music {
                music_name.clone()
            } else if let Some((n, _)) = chart_music.iter().find(|(_, q)| *q == p) {
                n.clone()
            } else {
                let n = named(&format!("music-{}", chart_music.len() + 1), &p);
                chart_music.push((n.clone(), p));
                n
            };
            music_of.push((i, name));
        }
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
        // The preview movie, kept as the background movies are; listed
        // with them only when a background change plays it too.
        let mut preview_video = None;
        for m in movies {
            let playable = m.format.playable();
            let kept = playable && m.kept;
            if assets.preview_video.as_ref() == Some(&m.path) {
                preview_video = Some(VideoRef {
                    name: m.relative.clone(),
                    format: m.format.clone(),
                    stored: kept,
                });
                if !preview_in_backgrounds {
                    if playable && !kept {
                        notes.push(format!(
                            "the preview video {} was not stored (\"also store background videos\" is off)",
                            m.relative
                        ));
                    }
                    if !playable {
                        notes.push(format!(
                            "#PREVIEWVID names {}, in a format the game cannot play",
                            m.relative
                        ));
                    }
                    if kept {
                        videos.push((m.relative.clone(), m.path));
                    }
                    continue;
                }
            }
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
        // The song's folders play (scripts and StepMania 3.9's formats);
        // animations found only in the shared folders do not.
        let scripts = crate::backgrounds::script_folders(&song, &c.dir, paths);
        let mut script_files = crate::backgrounds::script_files(&c.dir, &scripts, paths);
        if script_files.len() > MAX_SCRIPT_FILES {
            warnings.push(format!(
                "{} files in scripted backgrounds; only the first {MAX_SCRIPT_FILES} are kept",
                script_files.len()
            ));
            script_files.truncate(MAX_SCRIPT_FILES);
        }
        let is_script = |name: &str| {
            let name = name.trim().to_lowercase();
            scripts.iter().any(|s| s.as_str().to_lowercase() == name)
        };
        let present: Vec<(String, bool)> =
            present.into_iter().filter(|(n, _)| !is_script(n)).collect();
        let names = |list: Vec<(String, bool)>| {
            list.into_iter()
                .map(|(n, _)| n)
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !present.is_empty() {
            notes.push(format!(
                "background changes use animations from shared folders, which are not played (the song background shows instead): {}",
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
        entry.jacket = jacket.as_ref().map(|(n, _)| n.clone());
        entry.cd_title = cd_title.as_ref().map(|(n, _)| n.clone());
        entry.lyrics = lyrics.as_ref().map(|(n, _)| n.clone());
        entry.preview = preview.as_ref().map(|(n, _)| n.clone());
        entry.preview_video = preview_video;
        entry.bg_scripts = script_files.iter().map(|(r, _)| r.clone()).collect();
        // The keys the song list finds its charts' bests by (a work's
        // chart's own excessive setting among them).
        for chart in &mut entry.charts {
            if chart.index < song.charts.len() {
                chart.digest = Some(song.chart_digest(chart.index));
                chart.excessive =
                    ddi_engine::rules::danoni::chart_excessive(&song.charts[chart.index]);
                let own = song.effects_chart(chart.index);
                chart.music = music_of
                    .iter()
                    .find(|(i, _)| *i == own)
                    .map(|(_, n)| n.clone())
                    .filter(|n| *n != entry.music);
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
            jacket,
            cd_title,
            lyrics,
            preview,
            chart_music,
            bg_images,
            script_files,
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
    legacy: crate::legacy::Decoder,
) -> RootScan {
    let mut out = RootScan::default();
    let mut seen = Duplicates::default();
    for c in pack::find_songs(paths, fallback_pack) {
        if let Some(e) = seen.check(&c) {
            out.failed.push((c.dir.clone(), e));
            continue;
        }
        let paths = pack_paths(paths, &c);
        match scan_song(files, paths, &c, shared, legacy) {
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
        Some(dir) => paths_under(paths, dir),
        None => paths,
    }
}

/// The paths under folder `dir` (all for the root) out of sorted `paths`.
fn paths_under<'a>(paths: &'a [RelPathBuf], dir: &RelPath) -> &'a [RelPathBuf] {
    if dir.is_empty() {
        return paths;
    }
    let prefix = format!("{dir}/");
    let start = paths.partition_point(|p| p.as_str() < prefix.as_str());
    let len = paths[start..].partition_point(|p| p.as_str().starts_with(&prefix));
    &paths[start..start + len]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn pump_simfiles_gain_center_free_charts_after_their_originals() {
        let chart = |steps: &str, row: &str| {
            format!(
                "#NOTEDATA:;#STEPSTYPE:{steps};#DIFFICULTY:Hard;#METER:8;\
                 #NOTES:{row}\n{row};"
            )
        };
        let text = format!(
            "#VERSION:0.83;#TITLE:Pump;#BPMS:0=120;#TICKCOUNTS:0=4;{}{}{}{}",
            chart("pump-single", "11100"),
            chart("dance-single", "1001"),
            chart("pump-halfdouble", "110001"),
            chart("pump-double", "0010000101"),
        );
        // As a song imported before was listed: its originals only.
        let original = ddi_chart::formats::sm::parse_ssc(&text).unwrap();
        let mut old = summarize(Default::default(), &original);
        old.variants_version = 1;
        assert!(old.needs_chart_variants());
        let song = parse_simfile(&text, "ssc").unwrap();
        assert_eq!(song.charts[..4], original.charts[..]);
        let made: Vec<_> = song.charts[4..]
            .iter()
            .map(|c| (c.derivation.as_ref().map(|d| d.source()), c.layout.as_str()))
            .collect();
        assert_eq!(
            made,
            [
                (Some(0), "dance-single"),
                (Some(2), "dance-single"),
                (Some(3), "dance-double"),
                (Some(0), "danoni-5"),
            ]
        );
        let entry = summarize(Default::default(), &song);
        assert!(!entry.needs_chart_variants());
        let upgraded = crate::manifest::with_chart_variants(&old, &song);
        assert!(!upgraded.needs_chart_variants());
        assert_eq!(&upgraded.charts[..old.charts.len()], old.charts);
        assert_eq!(upgraded.charts.len(), 8);
        for c in &upgraded.charts {
            assert_eq!(c.digest, Some(song.chart_digest(c.index)));
        }
        assert!(upgraded.charts[4..].iter().all(|c| c.derivation.is_some()));
    }

    #[test]
    fn exported_danoni_simfiles_keep_originals_and_gain_versioned_charts() {
        for (mode, lanes, onigiri, target) in [
            ("5", 5, 4, "dance-single"),
            ("7", 7, 3, "dance-solo"),
            ("9A", 9, 4, "dance-double"),
            ("9B", 9, 4, "techno-single8"),
        ] {
            let mut rows = vec![vec![b'0'; lanes]; 4];
            rows[0][0] = b'2';
            rows[0][onigiri] = b'1';
            rows[1][onigiri] = b'2';
            rows[2][0] = b'3';
            rows[2][onigiri] = b'3';
            rows[3][lanes - 2] = b'1';
            let notes = rows
                .into_iter()
                .map(|r| String::from_utf8(r).unwrap())
                .collect::<Vec<_>>()
                .join("\n");
            let text = format!(
                "#VERSION:0.83;#TITLE:Export;#BPMS:0=120;#NOTEDATA:;\
                #STEPSTYPE:danoni-{mode};#CHARTNAME:Hard;#DIFFICULTY:Hard;#METER:8;\
                #OFFSET:0.25;#BPMS:0=150;#SCROLLS:0=0.8,2=1.2;#NOTES:{notes};"
            );
            let original = ddi_chart::formats::sm::parse_ssc(&text).unwrap();
            let raw_entry = summarize(Default::default(), &original);
            assert_eq!(raw_entry.variants_version, 0);
            assert!(raw_entry.needs_chart_variants());
            let song = parse_simfile(&text, "ssc").unwrap();
            // 5 keys is also converted to pump single, after.
            let made = if mode == "5" { 3 } else { 2 };
            assert_eq!(song.charts.len(), made, "{mode}");
            assert_eq!(song.charts[0], original.charts[0]);
            assert_eq!(song.chart_digest(0), original.chart_digest(0));
            assert_eq!(song.charts[1].layout, target);
            assert_eq!(song.charts[1].notes.len(), 2);
            assert_eq!(song.charts[1].timing, original.charts[0].timing);
            assert!(
                song.charts[1].danoni.is_none(),
                "SSC keeps its ruleset semantics"
            );
            let entry = summarize(Default::default(), &song);
            assert_eq!(entry.charts[0].index, 0);
            assert_eq!(entry.charts[1].index, 1);
            if mode == "5" {
                assert_eq!(song.charts[2].layout, "pump-single");
                assert_eq!(song.charts[2].notes.len(), 4);
                assert!(!song.charts[2].pump_game(), "its holds stay its own");
            }
            assert!(!entry.needs_chart_variants());
            let admitted_json = serde_json::to_string(&raw_entry).unwrap();
            let enriched = crate::manifest::with_chart_variants(&raw_entry, &song);
            assert_eq!(enriched.charts, entry.charts);
            assert!(!enriched.needs_chart_variants());
            assert_eq!(&enriched.charts[..raw_entry.charts.len()], raw_entry.charts);
            assert_eq!(serde_json::to_string(&raw_entry).unwrap(), admitted_json);
            assert_eq!(
                crate::manifest::with_chart_variants(&enriched, &song),
                enriched
            );
        }
    }

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
        let scan = scan_root(&files, &paths, "Fallback", &[], &crate::legacy::none);
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
        let scan = scan_root(&Files(f.clone()), &paths, "Mine", &[], &crate::legacy::none);
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
        let scan = scan_root(&Files(f), &paths, "Mine", &[], &crate::legacy::none);
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
        let scan = scan_root(&Files(f), &paths, "Mine", &[], &crate::legacy::none);
        let e = &scan.songs[0].entry;
        assert_eq!(e.bg_images, vec!["over.png"]);
        // The animation that exists (another case: found anyway), not the
        // one found nowhere.
        assert_eq!(e.bg_scripted, vec!["Spin"]);
    }

    #[test]
    fn the_preview_video_is_kept_apart_from_the_background_movies() {
        let ssc = |bg: &str| {
            format!(
                "#VERSION:0.83;#TITLE:S;#MUSIC:song.ogg;#PREVIEWVID:Preview.MP4;#BPMS:0=120;\
                 {bg}\n#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Easy;#METER:1;\
                 #NOTES:\n1000\n0000\n0000\n0000\n;\n"
            )
        };
        let mp4 = crate::video::tests::mp4_file(b"avc1", true);
        let mut f = BTreeMap::new();
        f.insert("P/S/song.ogg".into(), vec![0; 4]);
        f.insert("P/S/preview.mp4".into(), mp4.clone());
        f.insert("P/S/bga.mp4".into(), mp4);
        // Background changes naming another movie: listed apart.
        f.insert(
            "P/S/s.ssc".into(),
            ssc("#BGCHANGES:0=bga.mp4=1=0=0=0;").into_bytes(),
        );
        let paths: Vec<RelPathBuf> = f.keys().map(RelPathBuf::from).collect();
        let scan = scan_root(&Files(f.clone()), &paths, "Mine", &[], &crate::legacy::none);
        let e = &scan.songs[0].entry;
        let preview = e.preview_video.as_ref().expect("a preview video");
        assert_eq!(preview.name.as_str(), "preview.mp4");
        assert!(preview.stored && preview.format.playable());
        let names: Vec<&str> = e.bg_videos.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["bga.mp4"]);
        // A background change playing it too: in both.
        f.insert(
            "P/S/s.ssc".into(),
            ssc("#BGCHANGES:0=preview.mp4=1=0=0=0;").into_bytes(),
        );
        let scan = scan_root(&Files(f.clone()), &paths, "Mine", &[], &crate::legacy::none);
        let e = &scan.songs[0].entry;
        assert!(e.preview_video.is_some());
        assert_eq!(e.bg_videos.len(), 1);
        // Missing: a note, none kept.
        f.remove("P/S/preview.mp4");
        let paths: Vec<RelPathBuf> = f.keys().map(RelPathBuf::from).collect();
        let scan = scan_root(&Files(f), &paths, "Mine", &[], &crate::legacy::none);
        let s = &scan.songs[0];
        assert!(s.entry.preview_video.is_none());
        assert!(
            s.notes.iter().any(|n| n.contains("#PREVIEWVID")),
            "{:?}",
            s.notes
        );
    }

    #[test]
    fn jacket_cd_title_lyrics_preview_and_chart_music_are_found() {
        let ssc = "#VERSION:0.83;#TITLE:S;#MUSIC:song.ogg;#JACKET:art/Cover.PNG;\
                   #PREVIEW:cut.ogg;#BPMS:0=120;\n\
                   #NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Easy;#METER:1;\
                   #MUSIC:easy.ogg;#NOTES:\n1000\n0000\n0000\n0000\n;\n\
                   #NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:9;\
                   #MUSIC:Song.ogg;#NOTES:\n1000\n0000\n0000\n0000\n;\n\
                   #NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Challenge;#METER:10;\
                   #MUSIC:gone.ogg;#NOTES:\n1000\n0000\n0000\n0000\n;\n";
        let mut f = BTreeMap::new();
        f.insert("P/S/s.ssc".into(), ssc.as_bytes().to_vec());
        f.insert("P/S/song.ogg".into(), vec![0; 4]);
        f.insert("P/S/easy.ogg".into(), vec![0; 4]);
        f.insert("P/S/cut.ogg".into(), vec![0; 4]);
        f.insert("P/S/art/cover.png".into(), png(512, 512));
        f.insert("P/S/my cdtitle.png".into(), png(200, 60));
        f.insert("P/S/words.lrc".into(), b"[00:01]la".to_vec());
        f.insert("P/S/bn.png".into(), png(256, 80));
        f.insert("P/S/bg.png".into(), png(640, 480));
        let paths: Vec<RelPathBuf> = f.keys().map(RelPathBuf::from).collect();
        let scan = scan_root(&Files(f.clone()), &paths, "Mine", &[], &crate::legacy::none);
        let s = &scan.songs[0];
        let e = &s.entry;
        assert_eq!(
            e.jacket.as_deref().map(RelPath::as_str),
            Some("art/cover.png")
        );
        assert_eq!(
            e.cd_title.as_deref().map(RelPath::as_str),
            Some("my cdtitle.png")
        );
        assert_eq!(e.lyrics.as_deref().map(RelPath::as_str), Some("words.lrc"));
        assert_eq!(e.preview.as_deref().map(RelPath::as_str), Some("cut.ogg"));
        let music: Vec<(usize, Option<&str>)> = e
            .charts
            .iter()
            .map(|c| (c.index, c.music.as_deref().map(RelPath::as_str)))
            .collect();
        // Hard names the song's music (in other capitals): none of its own.
        assert_eq!(music, vec![(0, Some("easy.ogg")), (1, None), (2, None)]);
        assert_eq!(e.music_for(0), "easy.ogg");
        assert_eq!(e.music_for(2), "song.ogg");
        assert!(
            s.notes.iter().any(|n| n.contains("gone.ogg")),
            "{:?}",
            s.notes
        );
        // Stored on the web by role.
        let c = &pack::find_songs(&paths, "Mine")[0];
        let reading = read_song(c, ssc.as_bytes(), &paths, &crate::legacy::none).unwrap();
        let built = reading
            .finish(&paths, &[], Vec::new(), Naming::Stored)
            .unwrap();
        let e = &built.scanned.entry;
        assert_eq!(e.jacket.as_deref().map(RelPath::as_str), Some("jacket.png"));
        assert_eq!(
            e.cd_title.as_deref().map(RelPath::as_str),
            Some("cdtitle.png")
        );
        assert_eq!(e.lyrics.as_deref().map(RelPath::as_str), Some("lyrics.lrc"));
        assert_eq!(
            e.preview.as_deref().map(RelPath::as_str),
            Some("preview.ogg")
        );
        assert_eq!(e.music_for(0), "music-1.ogg");
        assert_eq!(
            built.chart_music,
            vec![("music-1.ogg".into(), "P/S/easy.ogg".into())]
        );
    }

    #[test]
    fn background_files_with_commas_and_equals_are_found_whole() {
        let mut f = BTreeMap::new();
        let changes = "#BGCHANGES:0=a,b.png=1=0=0=1,2=x=y/c.png=1=0=0=1,4=plain.png=1=0=0=1;";
        let sm = SM.replace("#BPMS:0=120;", &format!("#BPMS:0=120;{changes}"));
        f.insert("P/S/s.sm".into(), sm.clone().into_bytes());
        f.insert("P/S/song.ogg".into(), vec![0; 4]);
        f.insert("P/S/A,B.png".into(), png(64, 48));
        f.insert("P/S/x=y/c.png".into(), png(64, 48));
        f.insert("P/S/plain.png".into(), png(64, 48));
        let paths: Vec<RelPathBuf> = f.keys().map(RelPathBuf::from).collect();
        assert_eq!(
            song_dir_names(RelPath::new("P/S"), &paths),
            vec!["A,B.png", "x=y", "x=y/c.png"]
        );
        let scan = scan_root(&Files(f), &paths, "Mine", &[], &crate::legacy::none);
        let e = &scan.songs[0].entry;
        assert_eq!(e.bg_images, vec!["A,B.png", "x=y/c.png", "plain.png"]);
        assert!(
            !scan.songs[0]
                .warnings
                .iter()
                .any(|w| w.contains("not found")),
            "{:?}",
            scan.songs[0].warnings
        );
        // Read again for a play, the entry's names split it the same way.
        let mut song = parse_simfile(&sm, "sm").unwrap();
        song.resolve_change_names(&e.change_names());
        let files: Vec<&str> = song
            .effects
            .iter()
            .filter(|e| e.kind == "bgchange")
            .map(|e| e.fields[1].as_str())
            .collect();
        assert_eq!(files, vec!["A,B.png", "x=y/c.png", "plain.png"]);
    }

    #[test]
    fn scripted_backgrounds_keep_their_folders_files() {
        let mut f = BTreeMap::new();
        let sm = SM.replace(
            "#BPMS:0=120;",
            "#BPMS:0=120;#BGCHANGES:0=anim=1=0=0=0,2=old=1=0=0=0;#FGCHANGES:4=FG=1=0=0=0;",
        );
        f.insert("P/S/s.sm".into(), sm.into_bytes());
        f.insert("P/S/song.ogg".into(), vec![0; 4]);
        f.insert(
            "P/S/Anim/default.lua".into(),
            b"return Def.Actor{}".to_vec(),
        );
        f.insert("P/S/Anim/x.png".into(), png(8, 8));
        f.insert("P/S/fg/default.lua".into(), b"return Def.Actor{}".to_vec());
        f.insert("P/S/old/BGAnimation.ini".into(), Vec::new());
        let paths: Vec<RelPathBuf> = f.keys().map(RelPathBuf::from).collect();
        let scan = scan_root(&Files(f), &paths, "Mine", &[], &crate::legacy::none);
        let s = &scan.songs[0];
        // StepMania 3.9's folders are kept as the scripts' are.
        assert_eq!(
            s.entry.bg_scripts,
            vec![
                "Anim/default.lua",
                "Anim/x.png",
                "fg/default.lua",
                "old/BGAnimation.ini"
            ]
        );
        assert!(
            !s.notes.iter().any(|n| n.contains("not played")),
            "{:?}",
            s.notes
        );
    }
}
