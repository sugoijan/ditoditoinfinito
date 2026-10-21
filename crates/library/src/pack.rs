//! Finding songs in a file listing, StepMania style.
//!
//! The input is the list of relative paths of an import (a picked folder, a
//! dropped folder or a zip): `/`-separated, relative to the import root, e.g.
//! `My Pack/Song A/song.ssc`. Every directory holding a simfile is one song,
//! as in StepMania's `Songs/<pack>/<song>/` layout, but at any depth, so a
//! single song folder, a pack folder or a folder of packs all import.
//!
//! Asset lookup follows StepMania 5.1 (`Song::TidyUpData`,
//! `NotesLoader::LoadFromDir`): paths are case-insensitive, `#MUSIC`,
//! `#BANNER` and `#BACKGROUND` are relative to the song directory (with `../`
//! reaching pack-level shared files), and missing references fall back to
//! guessing from file names.

use std::collections::BTreeMap;

use ddi_chart::Song;

use crate::legacy::Legacy;
use crate::path::{RelPath, RelPathBuf};

/// Simfile formats the importer reads, in order of preference: when a song
/// directory holds several, StepMania loads `.ssc` before `.sm` before
/// `.dwi` (`NotesLoader::LoadFromDir`; `.sma`, `.bms` and `.ksf`, which sit
/// between and after them there, are not supported).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChartFormat {
    Ssc,
    Sm,
    Dwi,
}

impl ChartFormat {
    /// Format of a file name by its extension, case-insensitively.
    pub fn of(name: &str) -> Option<ChartFormat> {
        match extension(name)?.to_ascii_lowercase().as_str() {
            "ssc" => Some(ChartFormat::Ssc),
            "sm" => Some(ChartFormat::Sm),
            "dwi" => Some(ChartFormat::Dwi),
            _ => None,
        }
    }

    /// Lowercase extension, as `ddi_chart::formats::sm::parse_simfile` takes it.
    pub fn extension(self) -> &'static str {
        match self {
            ChartFormat::Ssc => "ssc",
            ChartFormat::Sm => "sm",
            ChartFormat::Dwi => "dwi",
        }
    }
}

/// A directory that holds a simfile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SongCandidate {
    /// Song directory relative to the import root (`""` for the root itself).
    pub dir: RelPathBuf,
    /// Full path of the chosen simfile.
    pub chart: RelPathBuf,
    pub format: ChartFormat,
    /// Pack (group) name: the directory containing the song directory, or
    /// the caller's fallback when the song sits at or directly under the
    /// import root.
    pub pack: String,
}

impl SongCandidate {
    /// Name of the song directory, or the pack name for a song at the import
    /// root (a zip holding one song's files directly). Used as the title
    /// fallback.
    pub fn dir_name(&self) -> &str {
        match self.dir.file_name() {
            "" => &self.pack,
            name => name,
        }
    }

    /// [`song_id`] of this candidate.
    pub fn id(&self) -> String {
        song_id(&self.pack, self.dir.as_str())
    }

    /// The id and the fallback name (the title when the simfile has none)
    /// an import gives the song: [`SongCandidate::id`] and its folder's
    /// name, except for files at the import's root (loose files picked
    /// without a folder), which are told apart by the simfile's name
    /// (the web's import, `crates/web/src/import`'s `identity`, until it calls
    /// this in step 7.1).
    pub fn identity(&self) -> (String, String) {
        if self.dir.is_empty() {
            let name = self.chart.file_name();
            let name = name.rsplit_once('.').map_or(name, |(s, _)| s);
            (song_id(&self.pack, name), name.to_string())
        } else {
            (self.id(), self.dir_name().to_string())
        }
    }

    /// The pack's folder (`""` for the import root, as for a zip of one
    /// pack's song folders), or `None` for a song at the root itself.
    pub fn pack_dir(&self) -> Option<&RelPath> {
        (!self.dir.is_empty()).then(|| self.dir.parent())
    }
}

/// The pack's banner, as StepMania finds a group banner
/// (`SongManager::AddGroup`, `src/SongManager.cpp` 270–300, 5_1-new): the
/// first image directly in the pack folder, `png` before `jpg`, `jpeg`,
/// `gif` and `bmp` (then `webp`, which browsers also show), else an image
/// beside the folder with the folder's name (`Packs/Foo.png` for
/// `Packs/Foo/`). Case-insensitive, like StepMania's file lookups.
pub fn pack_banner<S: AsRef<str>>(pack_dir: &RelPath, paths: &[S]) -> Option<RelPathBuf> {
    let pack_dir = pack_dir.as_str();
    let ext_rank = |p: &str| {
        let ext = extension(p)?.to_ascii_lowercase();
        IMAGE_EXTENSIONS.iter().position(|e| *e == ext)
    };
    let pick = |matches: &dyn Fn(&str) -> bool| {
        paths
            .iter()
            .map(AsRef::as_ref)
            .filter(|p| !is_ignored(p) && matches(p))
            .filter_map(|p| Some((ext_rank(p)?, p.to_ascii_lowercase(), p)))
            .min()
            .map(|(_, _, p)| RelPathBuf::from(p))
    };
    pick(&|p| parent(p).eq_ignore_ascii_case(pack_dir)).or_else(|| {
        if pack_dir.is_empty() {
            return None;
        }
        pick(&|p| {
            parent(p).eq_ignore_ascii_case(parent(pack_dir))
                && p.rsplit_once('.')
                    .is_some_and(|(stem, _)| stem.eq_ignore_ascii_case(pack_dir))
        })
    })
}

/// Files and directories never looked at: macOS resource forks
/// (`__MACOSX/`, `._name`), other dotfiles and dot-directories (`.git`,
/// `.DS_Store`), and editor backups and autosaves (`.old`, `.bak`, `.ats`,
/// trailing `~`), which StepMania would not load as the song either.
pub fn is_ignored(path: &str) -> bool {
    let mut components = path.split('/').filter(|c| !c.is_empty()).peekable();
    while let Some(c) = components.next() {
        if c.starts_with('.') || c.eq_ignore_ascii_case("__MACOSX") {
            return true;
        }
        if components.peek().is_none() {
            let backup = matches!(
                extension(c).map(str::to_ascii_lowercase).as_deref(),
                Some("old" | "bak" | "ats" | "tmp")
            );
            return backup || c.ends_with('~');
        }
    }
    false
}

/// Picks the simfile among the file names of one directory: best
/// [`ChartFormat`], then the first in sorted order.
pub fn pick_chart<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<(&'a str, ChartFormat)> {
    names
        .into_iter()
        .filter(|n| !is_ignored(n))
        .filter_map(|n| Some((ChartFormat::of(n)?, n)))
        .min()
        .map(|(f, n)| (n, f))
}

/// Finds the songs in an import, sorted by directory.
///
/// `fallback_pack` names the pack of songs at or directly under the import
/// root, e.g. the zip file's stem or "Imported".
pub fn find_songs<S: AsRef<str>>(paths: &[S], fallback_pack: &str) -> Vec<SongCandidate> {
    let mut by_dir: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for p in paths {
        let p = p.as_ref();
        if is_ignored(p) || p.ends_with('/') {
            continue;
        }
        by_dir.entry(parent(p)).or_default().push(p);
    }
    by_dir
        .into_iter()
        .filter_map(|(dir, files)| {
            let (chart, format) = pick_chart(files)?;
            let pack = match file_name(parent(dir)) {
                "" => fallback_pack,
                name => name,
            };
            Some(SongCandidate {
                dir: dir.into(),
                chart: chart.into(),
                format,
                pack: pack.to_string(),
            })
        })
        .collect()
}

/// Audio extensions tried for the music fallback: StepMania's `FT_Sound`
/// (`mp3`, `oga`, `ogg`, `wav`) plus what browsers also decode.
pub const AUDIO_EXTENSIONS: &[&str] = &["ogg", "oga", "opus", "mp3", "wav", "flac"];

/// Image extensions tried for the banner and background fallbacks:
/// StepMania's `FT_Bitmap` (`bmp`, `gif`, `jpeg`, `jpg`, `png`) plus `webp`.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// Asset paths of a song, relative to the import root (not to the song
/// directory, since `../` references leave it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedAssets {
    pub music: Option<RelPathBuf>,
    pub banner: Option<RelPathBuf>,
    pub background: Option<RelPathBuf>,
    /// The jacket (`#JACKET`) and CD title (`#CDTITLE`), found as the
    /// banner is, or by size ([`classify_images`]).
    pub jacket: Option<RelPathBuf>,
    pub cd_title: Option<RelPathBuf>,
    /// The `.lrc` lyrics (`#LYRICSPATH`, else the first `.lrc` in the song
    /// directory, as `Song::TidyUpData` finds them).
    pub lyrics: Option<RelPathBuf>,
    /// The `.ssc` `#PREVIEW` sound file, when it exists.
    pub preview: Option<RelPathBuf>,
    /// The `.ssc` `#PREVIEWVID` movie, when it exists and is a movie.
    pub preview_video: Option<RelPathBuf>,
    /// The `.ssc` charts' own `#MUSIC` that exist, `(chart index, path)`.
    pub chart_music: Vec<(usize, RelPathBuf)>,
    /// Images directly in the song directory that are neither the banner
    /// nor the background, nor recognisably another kind of song image (CD
    /// title, jacket, CD image, disc; see [`resolve_assets`]), sorted.
    /// Candidates for [`classify_images`].
    pub unclassified: Vec<RelPathBuf>,
    /// `(tag, reference)` for `#MUSIC`, `#BANNER`, `#BACKGROUND`,
    /// `#JACKET`, `#CDTITLE`, `#LYRICSPATH`, `#PREVIEW`, `#PREVIEWVID` and charts'
    /// `#MUSIC` (as `#MUSIC (chart n)`) values that name a file the import
    /// does not have (a fallback may still have been found).
    pub missing: Vec<(String, String)>,
}

/// Resolves the song's `#MUSIC`, `#BANNER` and `#BACKGROUND` against the
/// import's file list.
///
/// References are trimmed, backslashes become `/`, `.` and `..` are
/// resolved against `dir`, and the result is matched case-insensitively
/// (StepMania's file database ignores case; packs authored on Windows rely
/// on it). A missing or dangling reference falls back the way
/// `Song::TidyUpData` does, looking only at files directly in `dir`, in
/// sorted order:
///
/// - music: the first audio file, or the second when the first is named
///   `intro*` and there are several (KSF intro tracks);
/// - banner: an image whose name without extension contains `banner` or
///   ends in `bn` after a space, `-`, `_` or `.` (StepMania only accepts
///   `" bn"`; the wider match covers files like `song-bn.png`, which it would
///   find by size instead);
/// - background: an image whose name contains `background` or ends in `bg`.
///
/// A fallback never picks the image already used for the other slot.
///
/// StepMania's last resort, classifying the remaining images by pixel size,
/// needs image headers: the remaining images are listed in
/// [`ResolvedAssets::unclassified`] for [`classify_images`]. Like StepMania,
/// that list leaves out images referenced by `#JACKET` or `#CDTITLE` and
/// images whose names mark them as another kind (name without extension
/// containing `cdtitle`, `jacket` or `albumart`, starting with `jk_`, or
/// ending in `-cd`, ` disc` or ` title`).
pub fn resolve_assets<S: AsRef<str>>(
    song: &Song,
    dir: &RelPath,
    all_paths: &[S],
) -> ResolvedAssets {
    let mut lookup: BTreeMap<String, &str> = BTreeMap::new();
    let mut children: Vec<&str> = Vec::new();
    for p in all_paths {
        let p = p.as_ref();
        if is_ignored(p) || p.ends_with('/') {
            continue;
        }
        lookup.entry(p.to_lowercase()).or_insert(p);
        if parent(p) == dir {
            children.push(p);
        }
    }
    children.sort_unstable();

    let find = |reference: Option<&str>| -> Option<RelPathBuf> {
        let path = dir.resolve(reference?)?;
        lookup
            .get(&path.as_str().to_lowercase())
            .map(|p| RelPathBuf::from(*p))
    };
    let with_ext = |exts: &[&str]| -> Vec<&str> {
        children
            .iter()
            .copied()
            .filter(|p| {
                extension(p).is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
            })
            .collect()
    };

    let music = find(song.music.as_deref()).or_else(|| {
        let audio = with_ext(AUDIO_EXTENSIONS);
        let first = *audio.first()?;
        let pick = if audio.len() > 1 && stem(first).to_lowercase().starts_with("intro") {
            audio[1]
        } else {
            first
        };
        Some(pick.into())
    });

    let images = with_ext(IMAGE_EXTENSIONS);
    let guess = |taken: Option<&RelPath>, matches: &dyn Fn(&str) -> bool| -> Option<RelPathBuf> {
        images
            .iter()
            .find(|p| Some(RelPath::new(**p)) != taken && matches(&stem(p).to_lowercase()))
            .map(|p| RelPathBuf::from(*p))
    };
    let explicit_banner = find(song.banner.as_deref());
    let explicit_background = find(song.background.as_deref());
    let banner = explicit_banner.or_else(|| {
        guess(explicit_background.as_deref(), &|s| {
            s.contains("banner")
                || s == "bn"
                || [" bn", "-bn", "_bn", ".bn"].iter().any(|e| s.ends_with(e))
        })
    });
    let background = explicit_background.or_else(|| {
        guess(banner.as_deref(), &|s| {
            s.contains("background") || s.ends_with("bg")
        })
    });
    // The jacket and CD title by their tags, else by their names
    // (`Song::TidyUpData`: a jacket's starts with `jk_` or contains
    // `jacket` or `albumart`, a CD title's contains `cdtitle`).
    let taken = |p: &str| {
        let p = Some(RelPath::new(p));
        p != banner.as_deref() && p != background.as_deref()
    };
    let named_image = |matches: &dyn Fn(&str) -> bool| -> Option<RelPathBuf> {
        images
            .iter()
            .filter(|p| taken(p))
            .find(|p| matches(&stem(p).to_lowercase()))
            .map(|p| RelPathBuf::from(*p))
    };
    let jacket = find(song.jacket.as_deref()).or_else(|| {
        named_image(&|s| s.starts_with("jk_") || s.contains("jacket") || s.contains("albumart"))
    });
    let cd_title =
        find(song.cd_title.as_deref()).or_else(|| named_image(&|s| s.contains("cdtitle")));
    let unclassified = images
        .iter()
        .filter(|p| taken(p))
        .filter(|p| {
            let p = Some(RelPath::new(**p));
            p != jacket.as_deref() && p != cd_title.as_deref()
        })
        .filter(|p| !names_other_image(&stem(p).to_lowercase()))
        .map(|p| RelPathBuf::from(*p))
        .collect();
    let lyrics =
        find(song.lyrics_path()).or_else(|| with_ext(&["lrc"]).first().map(|p| (*p).into()));
    let preview = find(song.preview_music());
    let preview_video = find(song.preview_video()).filter(|p| crate::video::is_movie(p.as_str()));
    let chart_music: Vec<(usize, RelPathBuf)> = (0..song.charts.len())
        .filter_map(|i| Some((i, find(song.chart_music(i))?)))
        .collect();
    let mut missing: Vec<(String, String)> = [
        ("#MUSIC", song.music.as_deref()),
        ("#BANNER", song.banner.as_deref()),
        ("#BACKGROUND", song.background.as_deref()),
        ("#JACKET", song.jacket.as_deref()),
        ("#CDTITLE", song.cd_title.as_deref()),
        ("#LYRICSPATH", song.lyrics_path()),
        ("#PREVIEW", song.preview_music()),
        ("#PREVIEWVID", song.preview_video()),
    ]
    .into_iter()
    .filter_map(|(tag, reference)| {
        let reference = reference?.trim();
        (!reference.is_empty() && find(Some(reference)).is_none())
            .then(|| (tag.to_string(), reference.to_string()))
    })
    .collect();
    // Each chart's own music once (generated versions name their
    // original's).
    for i in 0..song.charts.len() {
        let Some(reference) = song.chart_music(i).map(str::trim) else {
            continue;
        };
        if song.effects_chart(i) == i && !reference.is_empty() && find(Some(reference)).is_none() {
            missing.push((format!("#MUSIC (chart {})", i + 1), reference.to_string()));
        }
    }
    ResolvedAssets {
        music,
        banner,
        background,
        jacket,
        cd_title,
        lyrics,
        preview,
        preview_video,
        chart_music,
        unclassified,
        missing,
    }
}

/// StepMania's shared background folders, searched for background-change
/// files a song's folder does not have.
pub const SHARED_FOLDERS: [&str; 2] = ["RandomMovies", "SongMovies"];

/// Whether a folder name marks a shared background folder: one of
/// [`SHARED_FOLDERS`] exactly, or followed by a separator (space, `(`,
/// `-`, `_`, `.`), ignoring case, so that a picked `RandomMovies (low
/// rez).zip`, which imports as a folder of that name, counts, but
/// `RandomMoviesFan` does not.
fn shared_kind(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    SHARED_FOLDERS.iter().copied().find(|f| {
        lower
            .strip_prefix(&f.to_lowercase())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '(', '-', '_', '.']))
    })
}

/// Where `path` sits inside a shared background folder, as
/// `RandomMovies/<rest>` or `SongMovies/<rest>`, using the innermost such
/// folder (a `RandomMovies.zip` usually wraps a `RandomMovies/` folder).
/// `None` outside them, or for the folder itself.
pub fn shared_path(path: &str) -> Option<RelPathBuf> {
    let parts: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    let (i, kind) = parts[..parts.len().saturating_sub(1)]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, part)| shared_kind(part).map(|k| (i, k)))?;
    Some(format!("{kind}/{}", parts[i + 1..].join("/")).into())
}

/// Folders of a StepMania installation that hold sounds but are not songs.
const INSTALL_FOLDERS: [&str; 12] = [
    "Announcers",
    "Appearance",
    "BGAnimations",
    "BackgroundEffects",
    "BackgroundTransitions",
    "Cache",
    "Characters",
    "Data",
    "NoteSkins",
    "Program",
    "Scripts",
    "Themes",
];

/// Folders that hold music but no simfile (`(folder, music file)`): song
/// folders whose simfile is missing. Not reported: folders with a simfile
/// at or below them (a pack folder with a preview track), shared folders,
/// and a StepMania installation's own folders (themes, note skins, …).
pub fn folders_without_simfile<S: AsRef<str>>(paths: &[S]) -> Vec<(RelPathBuf, RelPathBuf)> {
    let mut charts: Vec<&str> = Vec::new();
    let mut music: BTreeMap<&str, &str> = BTreeMap::new();
    for p in paths {
        let p = p.as_ref();
        if is_ignored(p) || p.ends_with('/') || shared_path(p).is_some() {
            continue;
        }
        if ChartFormat::of(p).is_some() {
            charts.push(parent(p));
        } else if extension(p)
            .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
        {
            music.entry(parent(p)).or_insert(p);
        }
    }
    let within = |inner: &str, outer: &str| {
        inner == outer || outer.is_empty() || inner.starts_with(&format!("{outer}/"))
    };
    let install = |dir: &str| {
        dir.split('/')
            .any(|c| INSTALL_FOLDERS.iter().any(|f| c.eq_ignore_ascii_case(f)))
    };
    music
        .into_iter()
        .filter(|(dir, _)| {
            !install(dir) && !charts.iter().any(|c| within(dir, c) || within(c, dir))
        })
        .map(|(dir, file)| (dir.into(), file.into()))
        .collect()
}

/// StepMania's name rules for the song images other than banner and
/// background (`Song::TidyUpData`): CD title, jacket, CD image, disc.
/// `stem` is the lowercased name without extension.
fn names_other_image(stem: &str) -> bool {
    stem.contains("cdtitle")
        || stem.starts_with("jk_")
        || stem.contains("jacket")
        || stem.contains("albumart")
        || stem.ends_with("-cd")
        || stem.ends_with(" disc")
        || stem.ends_with(" title")
}

/// Fills a missing banner, background, CD title or jacket from
/// [`ResolvedAssets::unclassified`] by pixel size, StepMania 5.1's last
/// resort (`Song::TidyUpData`). `sizes` maps image paths (as listed in
/// `unclassified`) to `(width, height)`, e.g. from
/// [`crate::image::image_size`]; images without a size are skipped, as
/// StepMania skips images it cannot load.
///
/// Each image, in order, is tried against these rules, first match wins:
///
/// 1. no background yet, at least 320×240: background;
/// 2. no banner yet, 100–320 wide and 50–240 high: banner;
/// 3. no banner yet, wider than 200 and wider than 2:1: banner (overlarge
///    banners);
/// 4. no CD title yet, at most 100×48: CD title;
/// 5. no jacket yet, square: jacket.
///
/// So a large wide image becomes the background when it comes first. An
/// image taken for one slot is removed from `unclassified` and never fills
/// another. Once banner, background and CD title are all known the
/// search stops, as StepMania's loop does (a jacket is then not looked
/// for). StepMania goes on to classify discs and CD images, which nothing
/// here shows. Unlike StepMania, candidates come in sorted order rather
/// than file system listing order.
pub fn classify_images(assets: &mut ResolvedAssets, sizes: &[(RelPathBuf, (u32, u32))]) {
    let size_of = |path: &RelPath| sizes.iter().find(|(p, _)| p == path).map(|&(_, size)| size);
    let mut remaining = Vec::with_capacity(assets.unclassified.len());
    for path in std::mem::take(&mut assets.unclassified) {
        if assets.banner.is_some() && assets.background.is_some() && assets.cd_title.is_some() {
            remaining.push(path);
            continue;
        }
        let Some((w, h)) = size_of(&path).filter(|&(w, h)| w > 0 && h > 0) else {
            remaining.push(path);
            continue;
        };
        let banner_sized = (100..=320).contains(&w) && (50..=240).contains(&h);
        let overlarge_banner = w > 200 && f64::from(w) / f64::from(h) > 2.0;
        if assets.background.is_none() && w >= 320 && h >= 240 {
            assets.background = Some(path);
        } else if assets.banner.is_none() && (banner_sized || overlarge_banner) {
            assets.banner = Some(path);
        } else if assets.cd_title.is_none() && w <= 100 && h <= 48 {
            assets.cd_title = Some(path);
        } else if assets.jacket.is_none() && w == h {
            assets.jacket = Some(path);
        } else {
            remaining.push(path);
        }
    }
    assets.unclassified = remaining;
}

/// [`RelPath::resolve`] as text: a simfile reference joined onto the song
/// directory, `None` when empty or climbing above the import root.
pub(crate) fn join_relative(dir: &str, reference: &str) -> Option<String> {
    RelPath::new(dir)
        .resolve(reference)
        .map(RelPathBuf::into_string)
}

/// Decodes simfile text: UTF-8 (BOM stripped) when valid, otherwise
/// Windows-1252 byte for byte. Simfile syntax is ASCII, so either way the
/// chart parses; only the free text (titles, artists, credits) of files in
/// other legacy encodings comes out wrong. [`decode_text_with`] also tries
/// Shift-JIS, Korean and Chinese.
pub fn decode_text(bytes: &[u8]) -> String {
    decode_text_with(bytes, crate::legacy::none)
}

/// [`decode_text`], trying the legacy code pages of Japanese, Korean and
/// Chinese packs before Windows-1252: `legacy` decodes strictly (`None`
/// on any invalid sequence; the browser's `TextDecoder` in the app), and
/// [`crate::legacy::decode`] takes the reading that reads as its
/// language. StepMania itself only tries UTF-8 and the system code page,
/// so on a Western system it shows such titles as mojibake; such packs
/// are common enough to do better.
pub fn decode_text_with(bytes: &[u8], legacy: impl Fn(Legacy, &[u8]) -> Option<String>) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match core::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => crate::legacy::decode(bytes, &legacy)
            .map(|(_, s)| s)
            .unwrap_or_else(|| bytes.iter().map(|&b| windows_1252(b)).collect()),
    }
}

/// Zip entry names in a legacy code page ([`crate::zip::ZipEntry::legacy_name`]),
/// decided for the whole archive so a folder never splits between two
/// readings: UTF-8 when every name is valid UTF-8 (many tools write it
/// without setting the flag), a legacy code page when together the names
/// read in it ([`crate::legacy::decode`], each name decoding strictly),
/// else CP437 as the format says. Backslashes become `/` after decoding,
/// like the zip reader's names (a Shift-JIS, GBK or Big5 trail byte may be
/// 0x5C).
pub fn decode_legacy_names(
    raws: &[&[u8]],
    legacy: impl Fn(Legacy, &[u8]) -> Option<String>,
) -> Vec<String> {
    let names: Vec<String> = if raws.iter().all(|r| core::str::from_utf8(r).is_ok()) {
        raws.iter()
            .map(|r| String::from_utf8_lossy(r).into_owned())
            .collect()
    } else {
        let joined = raws.join(&b'\n');
        crate::legacy::decode(&joined, &legacy)
            .and_then(|(code_page, _)| {
                raws.iter()
                    .map(|r| {
                        if r.is_ascii() {
                            Some(String::from_utf8_lossy(r).into_owned())
                        } else {
                            legacy(code_page, r)
                        }
                    })
                    .collect::<Option<Vec<String>>>()
            })
            .unwrap_or_else(|| raws.iter().map(|r| crate::zip::decode_cp437(r)).collect())
    };
    names.into_iter().map(|n| n.replace('\\', "/")).collect()
}

/// Whether text decoded as Shift-JIS reads as Japanese rather than as
/// Western text that happens to decode. Windows-1252's accented letters
/// and typographic quotes are Shift-JIS lead bytes and half-width katakana,
/// so a Western text turns into lone kanji or katakana inside Latin words
/// ("Don’t" → "Don稚", "Pokémon" → "Pok駑on", "Ökostrom" → "ﾖkostrom"):
/// such a character rules the text out. Japanese is shown by full-width
/// kana or forms, runs of kanji or of half-width katakana, or a kanji
/// standing on its own; every other non-ASCII character must be one
/// Shift-JIS has (punctuation, symbols, Greek, Cyrillic, NEC's extensions),
/// and nothing may be a control character.
pub fn reads_as_japanese(s: &str) -> bool {
    let kana = |c: char| matches!(c, '\u{3041}'..='\u{30FF}');
    let full = |c: char| matches!(c, '\u{FF01}'..='\u{FF5E}');
    let kanji = |c: char| matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}');
    let half = |c: char| matches!(c, '\u{FF61}'..='\u{FF9F}');
    let symbol = |c: char| {
        matches!(c,
            '\u{00A7}' | '\u{00A8}' | '\u{00B0}' | '\u{00B1}' | '\u{00B4}' | '\u{00B6}'
            | '\u{00D7}' | '\u{00F7}'
            | '\u{0391}'..='\u{03C9}' // Greek
            | '\u{0401}'..='\u{0451}' // Cyrillic
            | '\u{2010}'..='\u{266F}' // punctuation, arrows, maths, box drawing, ★, ♪
            | '\u{3000}'..='\u{3040}' // CJK punctuation
            | '\u{3200}'..='\u{33FF}' // NEC row 13: ㈱, ㍻, ㎞
            | '\u{FFE0}'..='\u{FFE5}'
        )
    };
    let chars: Vec<char> = s.chars().collect();
    let latin = |i: Option<usize>| {
        i.and_then(|i| chars.get(i))
            .is_some_and(|c| c.is_ascii_alphanumeric())
    };
    let mut japanese = false;
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii() {
            if c.is_control() && !matches!(c, '\n' | '\r' | '\t') {
                return false;
            }
            continue;
        }
        if kana(c) || full(c) {
            japanese = true;
        } else if kanji(c) || half(c) {
            let kin = |j: Option<usize>| {
                j.and_then(|j| chars.get(j))
                    .is_some_and(|&o| kana(o) || (kanji(c) && kanji(o)) || (half(c) && half(o)))
            };
            let (prev, next) = (i.checked_sub(1), Some(i + 1));
            if kin(prev) || kin(next) {
                japanese = true;
            } else if latin(prev) || latin(next) {
                return false;
            } else if kanji(c) {
                japanese = true;
            }
        } else if !symbol(c) {
            return false;
        }
    }
    japanese
}

/// Windows-1252 as browsers decode it (WHATWG): Latin-1 except 0x80..=0x9F,
/// whose five unassigned bytes map to the C1 control of the same value.
fn windows_1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// The pack of songs found at or directly under an import's root when
/// the import names none (the web's import of a folder or of loose
/// files; a zip picked alone names it after itself).
pub const LOOSE_PACK: &str = "Imported";

/// Stable id of an imported song, so importing the same folder again
/// replaces it: `u-` and the 16 hex digits of the 64-bit FNV-1a hash of
/// `pack/name` lowercased, where `name` is the last component of `dir`. Only
/// `[a-z0-9-]`, since it appears in URLs; the `u-` prefix keeps it apart
/// from bundled song ids.
pub fn song_id(pack: &str, dir: &str) -> String {
    let key = format!("{}/{}", pack.trim(), file_name(dir)).to_lowercase();
    format!("u-{:016x}", fnv1a64(key.as_bytes()))
}

/// FNV-1a, 64 bits: the imported songs' ids and other short keys.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = Fnv1a64::new();
    h.write(bytes);
    h.finish()
}

/// [`fnv1a64`] over bytes given in parts: the same as over them joined.
#[derive(Clone, Copy, Debug)]
pub struct Fnv1a64(u64);

impl Fnv1a64 {
    pub fn new() -> Fnv1a64 {
        Fnv1a64(0xcbf2_9ce4_8422_2325)
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a64 {
    fn default() -> Fnv1a64 {
        Fnv1a64::new()
    }
}

/// Directory part of a path (`""` at the root): [`RelPath::parent`].
pub(crate) fn parent(path: &str) -> &str {
    RelPath::new(path).parent().as_str()
}

/// Last component of a path: [`RelPath::file_name`].
pub(crate) fn file_name(path: &str) -> &str {
    RelPath::new(path).file_name()
}

/// Extension of the last component: [`RelPath::extension`].
pub(crate) fn extension(path: &str) -> Option<&str> {
    RelPath::new(path).extension()
}

/// Last component without its extension: [`RelPath::file_stem`].
fn stem(path: &str) -> &str {
    RelPath::new(path).file_stem()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::formats::sm::parse_sm;

    fn song(header: &str) -> Song {
        parse_sm(&format!(
            "{header}\n#BPMS:0=120;\n#NOTES:\n dance-single:\n :\n Easy:\n 1:\n 0,0,0,0,0:\n1000\n0000\n0000\n0000\n;\n"
        ))
        .unwrap()
    }

    #[test]
    fn shared_folders_are_recognised_by_name_prefix() {
        assert_eq!(
            shared_path("StepMania/RandomMovies/MAX-EXTREME/Robot1.png")
                .as_deref()
                .map(RelPath::as_str),
            Some("RandomMovies/MAX-EXTREME/Robot1.png")
        );
        // A picked zip imports as a folder named after it.
        assert_eq!(
            shared_path("RandomMovies(low rez)/MAX-EXTREME/Robot1.png")
                .as_deref()
                .map(RelPath::as_str),
            Some("RandomMovies/MAX-EXTREME/Robot1.png")
        );
        assert_eq!(
            shared_path("songmovies/Pack/clip.png")
                .as_deref()
                .map(RelPath::as_str),
            Some("SongMovies/Pack/clip.png")
        );
        assert_eq!(shared_path("Songs/Pack/Song/bg.png"), None);
        // A file named like the folder is not inside one.
        assert_eq!(shared_path("Pack/RandomMovies.png"), None);
        // A zip wrapping its own folder: the innermost one counts.
        assert_eq!(
            shared_path("RandomMovies/RandomMovies/A/b.png")
                .as_deref()
                .map(RelPath::as_str),
            Some("RandomMovies/A/b.png")
        );
        assert_eq!(
            shared_path("RandomMovies (low rez)/RandomMovies/A/b.png")
                .as_deref()
                .map(RelPath::as_str),
            Some("RandomMovies/A/b.png")
        );
        // A name that only starts with it is not a shared folder.
        assert_eq!(shared_path("Songs/RandomMoviesFan/Song/bg.png"), None);
        assert_eq!(
            shared_path("RandomMovies-HD/x.png")
                .as_deref()
                .map(RelPath::as_str),
            Some("RandomMovies/x.png")
        );
    }

    #[test]
    fn music_folders_without_a_simfile_are_found() {
        let paths = [
            "Pack/Good/song.sm",
            "Pack/Good/song.ogg",
            "Pack/Good/sounds/hit.ogg",
            "Pack/Orphan/track.mp3",
            "Pack/Orphan/cover.png",
            "Pack/preview.ogg",
            "RandomMovies/clip.ogg",
            "Pack/Art/only.png",
            "Themes/Default/Sounds/start.ogg",
            "NoteSkins/dance/x/hit.wav",
        ];
        assert_eq!(
            folders_without_simfile(&paths),
            vec![(
                RelPathBuf::from("Pack/Orphan"),
                RelPathBuf::from("Pack/Orphan/track.mp3")
            )]
        );
    }

    #[test]
    fn finds_songs_and_packs() {
        let paths = [
            "Packs/My Pack/Song A/song.sm",
            "Packs/My Pack/Song A/song.SSC",
            "Packs/My Pack/Song A/song.ogg",
            "Packs/My Pack/Song B/b.dwi",
            "Packs/My Pack/Song B/a.dwi",
            "Packs/My Pack/banner.png",
            "Solo/x.sm",
            "root.sm",
            "__MACOSX/Packs/My Pack/Song A/._song.sm",
            "Packs/My Pack/Song C/song.sm.old",
            "Packs/My Pack/Song C/song.ats",
            ".hidden/h.sm",
        ];
        let songs = find_songs(&paths, "upload");
        let got: Vec<_> = songs
            .iter()
            .map(|s| (s.dir.as_str(), s.chart.as_str(), s.format, s.pack.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("", "root.sm", ChartFormat::Sm, "upload"),
                (
                    "Packs/My Pack/Song A",
                    "Packs/My Pack/Song A/song.SSC",
                    ChartFormat::Ssc,
                    "My Pack"
                ),
                (
                    "Packs/My Pack/Song B",
                    "Packs/My Pack/Song B/a.dwi",
                    ChartFormat::Dwi,
                    "My Pack"
                ),
                ("Solo", "Solo/x.sm", ChartFormat::Sm, "upload"),
            ]
        );
        assert_eq!(songs[0].dir_name(), "upload");
        assert_eq!(songs[1].dir_name(), "Song A");
    }

    #[test]
    fn ignored_paths() {
        assert!(is_ignored("__MACOSX/a/b.sm"));
        assert!(is_ignored("a/.git/config"));
        assert!(is_ignored("a/._song.ogg"));
        assert!(is_ignored("a/song.sm.OLD"));
        assert!(is_ignored("a/song.sm~"));
        assert!(!is_ignored("a/song.sm"));
        assert!(!is_ignored("a.b/song.old.sm"));
    }

    #[test]
    fn explicit_references_case_insensitive_and_relative() {
        let paths = [
            "P/S/song.sm",
            "P/S/Audio/Song.OGG",
            "P/shared-bn.png",
            "P/S/BG.JPG",
        ];
        let mut s = song("#BANNER:../Shared-BN.png;\n#BACKGROUND:./bg.jpg;");
        // Set directly: the MSD parser treats a backslash as an escape.
        s.music = Some(" audio\\song.ogg ".into());
        let r = resolve_assets(&s, RelPath::new("P/S"), &paths);
        assert_eq!(
            r.music.as_deref().map(RelPath::as_str),
            Some("P/S/Audio/Song.OGG")
        );
        assert_eq!(
            r.banner.as_deref().map(RelPath::as_str),
            Some("P/shared-bn.png")
        );
        assert_eq!(
            r.background.as_deref().map(RelPath::as_str),
            Some("P/S/BG.JPG")
        );
        assert_eq!(
            RelPath::new("P/shared-bn.png").relative_to(RelPath::new("P/S")),
            "../shared-bn.png"
        );
        assert_eq!(
            RelPath::new("P/S/Audio/Song.OGG").relative_to(RelPath::new("P/S")),
            "Audio/Song.OGG"
        );
        assert_eq!(RelPath::new("x.ogg").relative_to(RelPath::new("")), "x.ogg");
    }

    #[test]
    fn references_cannot_escape_the_root() {
        let s = song("#MUSIC:../../../etc/passwd;");
        let r = resolve_assets(&s, RelPath::new("S"), &["S/a.sm", "etc/passwd"]);
        assert_eq!(r.music, None);
    }

    #[test]
    fn fallbacks() {
        let paths = [
            "S/song.sm",
            "S/intro.ogg",
            "S/main.mp3",
            "S/cdtitle.png",
            "S/abnormal.png",
            "S/song-bn.png",
            "S/song-bg.png",
            "S/sub/other.ogg",
            "S/._fake.ogg",
        ];
        let s = song("#MUSIC:missing.ogg;\n#BANNER:;");
        let r = resolve_assets(&s, RelPath::new("S"), &paths);
        assert_eq!(r.music.as_deref().map(RelPath::as_str), Some("S/main.mp3"));
        assert_eq!(
            r.banner.as_deref().map(RelPath::as_str),
            Some("S/song-bn.png")
        );
        assert_eq!(
            r.background.as_deref().map(RelPath::as_str),
            Some("S/song-bg.png")
        );
        // The missing `#MUSIC` is reported even though a fallback was found;
        // an empty `#BANNER` names nothing.
        assert_eq!(
            r.missing,
            vec![("#MUSIC".to_string(), "missing.ogg".to_string())]
        );

        // A single intro-named file is still the music.
        let r = resolve_assets(&song(""), RelPath::new("S"), &["S/s.sm", "S/Intro.wav"]);
        assert_eq!(r.music.as_deref().map(RelPath::as_str), Some("S/Intro.wav"));

        // Never the same image twice: "banner background.png" is the banner.
        let r = resolve_assets(
            &song(""),
            RelPath::new("S"),
            &["S/s.sm", "S/banner background.png", "S/x background.jpg"],
        );
        assert_eq!(
            r.banner.as_deref().map(RelPath::as_str),
            Some("S/banner background.png")
        );
        assert_eq!(
            r.background.as_deref().map(RelPath::as_str),
            Some("S/x background.jpg")
        );
        let r = resolve_assets(
            &song(""),
            RelPath::new("S"),
            &["S/s.sm", "S/banner background.png"],
        );
        assert_eq!(r.background, None);

        // Nothing at all.
        assert_eq!(
            resolve_assets(&song(""), RelPath::new("S"), &["S/s.sm"]),
            ResolvedAssets::default()
        );
    }

    #[test]
    fn unclassified_images() {
        let mut s = song("#JACKET:art.png;");
        s.cd_title = Some("logo.gif".into());
        let r = resolve_assets(
            &s,
            RelPath::new("S"),
            &[
                "S/s.dwi",
                "S/zeta.png",
                "S/Song.png",
                "S/song-bg.png",
                "S/art.png",
                "S/logo.gif",
                "S/my cdtitle.png",
                "S/jk_x.png",
                "S/x-cd.jpg",
                "S/x disc.png",
                "S/sub/deep.png",
            ],
        );
        assert_eq!(r.banner, None);
        assert_eq!(
            r.background.as_deref().map(RelPath::as_str),
            Some("S/song-bg.png")
        );
        assert_eq!(r.unclassified, ["S/Song.png", "S/zeta.png"]);
    }

    #[test]
    fn classify_by_size() {
        let sizes = |list: &[(&str, (u32, u32))]| -> Vec<(RelPathBuf, (u32, u32))> {
            list.iter().map(|&(p, s)| (p.into(), s)).collect()
        };
        // The DWI case: `song.png` banner next to a named background.
        let mut r = ResolvedAssets {
            background: Some("S/song-bg.png".into()),
            unclassified: vec!["S/song.png".into()],
            ..Default::default()
        };
        classify_images(&mut r, &sizes(&[("S/song.png", (256, 80))]));
        assert_eq!(r.banner.as_deref().map(RelPath::as_str), Some("S/song.png"));
        assert!(r.unclassified.is_empty());

        // Background first when large; overlarge 2:1+ banner; a tiny
        // image the CD title, a square one the jacket; unknown sizes stay
        // unclassified.
        let mut r = ResolvedAssets {
            unclassified: ["a", "b", "c", "d", "e", "f"]
                .map(RelPathBuf::from)
                .to_vec(),
            ..Default::default()
        };
        classify_images(
            &mut r,
            &sizes(&[
                ("a", (64, 40)),
                ("b", (300, 300)),
                ("c", (640, 480)),
                ("d", (1024, 320)),
                ("e", (0, 0)),
            ]),
        );
        assert_eq!(r.background.as_deref().map(RelPath::as_str), Some("c"));
        // 1024×320 is ≥ 320×240, but the background is taken: banner by ratio.
        assert_eq!(r.banner.as_deref().map(RelPath::as_str), Some("d"));
        assert_eq!(r.cd_title.as_deref().map(RelPath::as_str), Some("a"));
        assert_eq!(r.jacket.as_deref().map(RelPath::as_str), Some("b"));
        assert_eq!(r.unclassified, ["e", "f"]);
        // Once banner, background and CD title are known, no jacket is
        // looked for (StepMania's loop stops).
        let mut r = ResolvedAssets {
            banner: Some("bn".into()),
            background: Some("bg".into()),
            cd_title: Some("cd".into()),
            unclassified: vec!["sq".into()],
            ..Default::default()
        };
        classify_images(&mut r, &sizes(&[("sq", (300, 300))]));
        assert_eq!(r.jacket, None);

        // A wide image seen before any background becomes the background,
        // as in StepMania; 300×300 is not banner-shaped.
        let mut r = ResolvedAssets {
            unclassified: vec!["wide".into(), "sq".into()],
            ..Default::default()
        };
        classify_images(&mut r, &sizes(&[("wide", (1024, 320)), ("sq", (300, 300))]));
        assert_eq!(r.background.as_deref().map(RelPath::as_str), Some("wide"));
        assert_eq!(r.banner, None);

        // Banner-sized rule: 100–320 × 50–240 (not ratio-limited).
        let mut r = ResolvedAssets {
            unclassified: vec!["b".into()],
            ..Default::default()
        };
        classify_images(&mut r, &sizes(&[("b", (200, 200))]));
        assert_eq!(r.banner.as_deref().map(RelPath::as_str), Some("b"));
    }

    #[test]
    fn pack_banners_like_stepmania() {
        let paths = [
            "Songs/Pack/b.jpg",
            "Songs/Pack/A.PNG",
            "Songs/Pack/._c.png",
            "Songs/Pack/Song/bn.png",
            "Songs/Pack/Song/song.sm",
            "Songs/Other.jpg",
            "Songs/Other/Song/song.sm",
        ];
        assert_eq!(
            pack_banner(RelPath::new("Songs/Pack"), &paths)
                .as_deref()
                .map(RelPath::as_str),
            Some("Songs/Pack/A.PNG")
        );
        assert_eq!(
            pack_banner(RelPath::new("songs/other"), &paths)
                .as_deref()
                .map(RelPath::as_str),
            Some("Songs/Other.jpg")
        );
        assert_eq!(
            pack_banner(RelPath::new(""), &["Song/song.sm", "x.gif"])
                .as_deref()
                .map(RelPath::as_str),
            Some("x.gif")
        );
        assert_eq!(pack_banner(RelPath::new("Songs/None"), &paths), None);
        let c = &find_songs(&paths, "Imported")[0];
        assert_eq!(c.pack_dir().map(RelPath::as_str), Some("Songs/Other"));
    }

    use crate::legacy::tests::decoder as shift_jis;

    #[test]
    fn shift_jis_text_and_names() {
        let (sjis, _, _) = encoding_rs::SHIFT_JIS.encode("#TITLE:恋のソナタ;\n#ARTIST:ＤＪ ★;");
        assert_eq!(
            decode_text_with(&sjis, shift_jis),
            "#TITLE:恋のソナタ;\n#ARTIST:ＤＪ ★;"
        );
        // Without a Shift-JIS decoder: Windows-1252, as before.
        assert!(!decode_text(&sjis).contains('恋'));
        // Western text stays Windows-1252 even where it decodes as
        // Shift-JIS: "Résumé" fails it, "Rés" reads as a kanji but not as
        // Japanese text around Latin-1 punctuation.
        assert_eq!(
            decode_text_with(b"#TITLE:R\xe9sum\xe9;", shift_jis),
            "#TITLE:Résumé;"
        );
        assert_eq!(
            decode_text_with(b"Caf\xe9 \x93x\x94", shift_jis),
            "Café “x”"
        );
    }

    #[test]
    fn western_text_is_not_read_as_shift_jis() {
        for text in [
            "#TITLE:Don’t Stop;",
            "#TITLE:It’s;",
            "#TITLE:Pokémon;",
            "#TITLE:Ökostrom;",
            "#TITLE:Straße;",
            "#TITLE:Ça va;",
            "#CREDIT:© 2005;",
            "#TITLE:10°C;",
            "#TITLE:½;",
        ] {
            let (cp, _, _) = encoding_rs::WINDOWS_1252.encode(text);
            assert_eq!(decode_text_with(&cp, shift_jis), text, "{text}");
        }
        // Japanese with NEC extensions and compatibility kanji.
        for text in [
            "#ARTIST:山﨑;",
            "#TITLE:㈱テスト ㍻;",
            "#TITLE:桜;",
            "#TITLE:ｶﾞﾝﾀﾞﾑ;",
        ] {
            let (sjis, _, unmappable) = encoding_rs::SHIFT_JIS.encode(text);
            if unmappable {
                // encoding_rs writes NEC extensions as IBM's; decode those.
                continue;
            }
            assert_eq!(decode_text_with(&sjis, shift_jis), text, "{text}");
        }
        let nec = b"#TITLE:\x87\x8a\x83e\x83X\x83g;"; // ㈱テスト
        assert_eq!(decode_text_with(nec, shift_jis), "#TITLE:㈱テスト;");
        let ibm = b"#ARTIST:\x8eR\xfa\xb1;"; // 山﨑 (IBM extension)
        assert_eq!(decode_text_with(ibm, shift_jis), "#ARTIST:山﨑;");
    }

    #[test]
    fn zip_names_are_decided_per_archive() {
        // "Mötley" (CP437 0x94) and "Café" (0x82) in one archive: both stay
        // CP437, even though the first alone would decode as Shift-JIS.
        let names: [&[u8]; 2] = [
            b"M\x94tley Pack/Kick/kick.sm",
            b"M\x94tley Pack/Kick/Caf\x82.ogg",
        ];
        assert_eq!(
            decode_legacy_names(&names, shift_jis),
            ["Mötley Pack/Kick/kick.sm", "Mötley Pack/Kick/Café.ogg"]
        );
        let (a, _, _) = encoding_rs::SHIFT_JIS.encode("パック/ソング/song.sm");
        let (b, _, _) = encoding_rs::SHIFT_JIS.encode("パック/ソング/音.ogg");
        assert_eq!(
            decode_legacy_names(&[&a, &b, b"readme.txt"], shift_jis),
            [
                "パック/ソング/song.sm",
                "パック/ソング/音.ogg",
                "readme.txt"
            ]
        );
        assert_eq!(
            decode_legacy_names(&["曲/a.sm".as_bytes()], shift_jis),
            ["曲/a.sm"]
        );
        assert_eq!(
            decode_legacy_names(&[b"a\\b\x82.sm"], crate::legacy::none),
            ["a/bé.sm"]
        );
    }

    #[test]
    fn decode_text_variants() {
        assert_eq!(decode_text(b"\xEF\xBB\xBF#TITLE:x;"), "#TITLE:x;");
        assert_eq!(decode_text("#TITLE:曲;".as_bytes()), "#TITLE:曲;");
        assert_eq!(
            decode_text(b"#TITLE:Caf\xe9 \x93x\x94;"),
            "#TITLE:Café “x”;"
        );
        assert_eq!(decode_text(b"\x81\x9d"), "\u{81}\u{9d}");
    }

    #[test]
    fn song_ids() {
        let a = song_id("My Pack", "Packs/My Pack/Song A");
        assert_eq!(a, song_id("my pack", "Elsewhere/SONG A/"));
        assert_ne!(a, song_id("My Pack", "Song B"));
        assert!(a.starts_with("u-") && a.len() == 18);
        assert!(
            a.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        );
        assert_eq!(song_id("a", ""), format!("u-{:016x}", fnv1a64(b"a/")));
    }

    #[test]
    fn fnv1a64_reference_values() {
        // From the FNV reference test suite.
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
        let mut h = Fnv1a64::new();
        h.write(b"foo");
        h.write(b"");
        h.write(b"bar");
        assert_eq!(h.finish(), fnv1a64(b"foobar"));
    }
}
