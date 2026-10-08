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
    pub dir: String,
    /// Full path of the chosen simfile.
    pub chart: String,
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
        match file_name(&self.dir) {
            "" => &self.pack,
            name => name,
        }
    }

    /// [`song_id`] of this candidate.
    pub fn id(&self) -> String {
        song_id(&self.pack, &self.dir)
    }
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
                dir: dir.to_string(),
                chart: chart.to_string(),
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
    pub music: Option<String>,
    pub banner: Option<String>,
    pub background: Option<String>,
    /// Images directly in the song directory that are neither the banner
    /// nor the background, nor recognisably another kind of song image (CD
    /// title, jacket, CD image, disc; see [`resolve_assets`]), sorted.
    /// Candidates for [`classify_images`].
    pub unclassified: Vec<String>,
    /// `(tag, reference)` for `#MUSIC`, `#BANNER` and `#BACKGROUND` values
    /// that name a file the import does not have (a fallback may still
    /// have been found).
    pub missing: Vec<(&'static str, String)>,
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
pub fn resolve_assets<S: AsRef<str>>(song: &Song, dir: &str, all_paths: &[S]) -> ResolvedAssets {
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

    let find = |reference: Option<&str>| -> Option<String> {
        let path = join_relative(dir, reference?)?;
        lookup.get(&path.to_lowercase()).map(|p| p.to_string())
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
        Some(pick.to_string())
    });

    let images = with_ext(IMAGE_EXTENSIONS);
    let guess = |taken: Option<&str>, matches: &dyn Fn(&str) -> bool| -> Option<String> {
        images
            .iter()
            .find(|p| Some(**p) != taken && matches(&stem(p).to_lowercase()))
            .map(|p| p.to_string())
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
    let other_refs = [find(song.jacket.as_deref()), find(song.cd_title.as_deref())];
    let unclassified = images
        .iter()
        .filter(|p| {
            let p = Some(**p);
            p != banner.as_deref()
                && p != background.as_deref()
                && !other_refs.iter().any(|r| r.as_deref() == p)
        })
        .filter(|p| !names_other_image(&stem(p).to_lowercase()))
        .map(|p| p.to_string())
        .collect();
    let missing = [
        ("#MUSIC", song.music.as_deref()),
        ("#BANNER", song.banner.as_deref()),
        ("#BACKGROUND", song.background.as_deref()),
    ]
    .into_iter()
    .filter_map(|(tag, reference)| {
        let reference = reference?.trim();
        (!reference.is_empty() && find(Some(reference)).is_none())
            .then(|| (tag, reference.to_string()))
    })
    .collect();
    ResolvedAssets {
        music,
        banner,
        background,
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
pub fn shared_path(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    let (i, kind) = parts[..parts.len().saturating_sub(1)]
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, part)| shared_kind(part).map(|k| (i, k)))?;
    Some(format!("{kind}/{}", parts[i + 1..].join("/")))
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
pub fn folders_without_simfile<S: AsRef<str>>(paths: &[S]) -> Vec<(String, String)> {
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
        .map(|(dir, file)| (dir.to_string(), file.to_string()))
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

/// Fills a missing banner or background from
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
///    banners).
///
/// So a large wide image becomes the background when it comes first. An
/// image taken for one slot is removed from `unclassified` and never fills
/// the other. StepMania then goes on to classify CD titles (≤ 100×48),
/// jackets (square) and discs in the same loop; those cannot change which
/// image becomes banner or background, since every image is tested for
/// those first, so they are not modelled. Unlike StepMania, candidates come
/// in sorted order rather than file system listing order.
pub fn classify_images(assets: &mut ResolvedAssets, sizes: &[(String, (u32, u32))]) {
    let size_of = |path: &str| sizes.iter().find(|(p, _)| p == path).map(|&(_, size)| size);
    let mut remaining = Vec::with_capacity(assets.unclassified.len());
    for path in std::mem::take(&mut assets.unclassified) {
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
        } else {
            remaining.push(path);
        }
    }
    assets.unclassified = remaining;
}

/// Joins a simfile reference onto the song directory, resolving `.` and
/// `..`. `None` for an empty reference or one that climbs above the import
/// root.
pub(crate) fn join_relative(dir: &str, reference: &str) -> Option<String> {
    let reference = reference.trim().replace('\\', "/");
    if reference.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|c| !c.is_empty()).collect();
    for c in reference.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            c => parts.push(c),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `path` relative to `dir` (both relative to the import root), with `../`
/// where it leaves `dir`: the form manifest entries store asset paths in.
pub fn relative_path(dir: &str, path: &str) -> String {
    let d: Vec<&str> = dir.split('/').filter(|c| !c.is_empty()).collect();
    let p: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    let common = d.iter().zip(&p).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = vec![".."; d.len() - common];
    out.extend(&p[common..]);
    out.join("/")
}

/// Decodes simfile text: UTF-8 (BOM stripped) when valid, otherwise
/// Windows-1252 byte for byte. Simfile syntax is ASCII, so either way the
/// chart parses; only the free text (titles, artists, credits) of files in
/// other legacy encodings comes out wrong. Shift-JIS, common in Japanese
/// packs, is a known limitation: its titles turn into Latin mojibake.
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match core::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| windows_1252(b)).collect(),
    }
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

/// Stable id of an imported song, so importing the same folder again
/// replaces it: `u-` and the 16 hex digits of the 64-bit FNV-1a hash of
/// `pack/name` lowercased, where `name` is the last component of `dir`. Only
/// `[a-z0-9-]`, since it appears in URLs; the `u-` prefix keeps it apart
/// from bundled song ids.
pub fn song_id(pack: &str, dir: &str) -> String {
    let key = format!("{}/{}", pack.trim(), file_name(dir)).to_lowercase();
    format!("u-{:016x}", fnv1a64(key.as_bytes()))
}

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Directory part of a path (`""` at the root).
pub(crate) fn parent(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit_once('/')
        .map_or("", |(dir, _)| dir)
}

/// Last component of a path.
pub(crate) fn file_name(path: &str) -> &str {
    let path = path.trim_end_matches('/');
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// Extension of the last component, without the dot. A leading dot alone
/// (`.ssc`) is not an extension.
pub(crate) fn extension(path: &str) -> Option<&str> {
    let name = file_name(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => Some(ext),
        _ => None,
    }
}

/// Last component without its extension.
fn stem(path: &str) -> &str {
    let name = file_name(path);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
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
            shared_path("StepMania/RandomMovies/MAX-EXTREME/Robot1.png").as_deref(),
            Some("RandomMovies/MAX-EXTREME/Robot1.png")
        );
        // A picked zip imports as a folder named after it.
        assert_eq!(
            shared_path("RandomMovies(low rez)/MAX-EXTREME/Robot1.png").as_deref(),
            Some("RandomMovies/MAX-EXTREME/Robot1.png")
        );
        assert_eq!(
            shared_path("songmovies/Pack/clip.png").as_deref(),
            Some("SongMovies/Pack/clip.png")
        );
        assert_eq!(shared_path("Songs/Pack/Song/bg.png"), None);
        // A file named like the folder is not inside one.
        assert_eq!(shared_path("Pack/RandomMovies.png"), None);
        // A zip wrapping its own folder: the innermost one counts.
        assert_eq!(
            shared_path("RandomMovies/RandomMovies/A/b.png").as_deref(),
            Some("RandomMovies/A/b.png")
        );
        assert_eq!(
            shared_path("RandomMovies (low rez)/RandomMovies/A/b.png").as_deref(),
            Some("RandomMovies/A/b.png")
        );
        // A name that only starts with it is not a shared folder.
        assert_eq!(shared_path("Songs/RandomMoviesFan/Song/bg.png"), None);
        assert_eq!(
            shared_path("RandomMovies-HD/x.png").as_deref(),
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
                "Pack/Orphan".to_string(),
                "Pack/Orphan/track.mp3".to_string()
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
        let r = resolve_assets(&s, "P/S", &paths);
        assert_eq!(r.music.as_deref(), Some("P/S/Audio/Song.OGG"));
        assert_eq!(r.banner.as_deref(), Some("P/shared-bn.png"));
        assert_eq!(r.background.as_deref(), Some("P/S/BG.JPG"));
        assert_eq!(relative_path("P/S", "P/shared-bn.png"), "../shared-bn.png");
        assert_eq!(relative_path("P/S", "P/S/Audio/Song.OGG"), "Audio/Song.OGG");
        assert_eq!(relative_path("", "x.ogg"), "x.ogg");
    }

    #[test]
    fn references_cannot_escape_the_root() {
        let s = song("#MUSIC:../../../etc/passwd;");
        let r = resolve_assets(&s, "S", &["S/a.sm", "etc/passwd"]);
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
        let r = resolve_assets(&s, "S", &paths);
        assert_eq!(r.music.as_deref(), Some("S/main.mp3"));
        assert_eq!(r.banner.as_deref(), Some("S/song-bn.png"));
        assert_eq!(r.background.as_deref(), Some("S/song-bg.png"));
        // The missing `#MUSIC` is reported even though a fallback was found;
        // an empty `#BANNER` names nothing.
        assert_eq!(r.missing, vec![("#MUSIC", "missing.ogg".to_string())]);

        // A single intro-named file is still the music.
        let r = resolve_assets(&song(""), "S", &["S/s.sm", "S/Intro.wav"]);
        assert_eq!(r.music.as_deref(), Some("S/Intro.wav"));

        // Never the same image twice: "banner background.png" is the banner.
        let r = resolve_assets(
            &song(""),
            "S",
            &["S/s.sm", "S/banner background.png", "S/x background.jpg"],
        );
        assert_eq!(r.banner.as_deref(), Some("S/banner background.png"));
        assert_eq!(r.background.as_deref(), Some("S/x background.jpg"));
        let r = resolve_assets(&song(""), "S", &["S/s.sm", "S/banner background.png"]);
        assert_eq!(r.background, None);

        // Nothing at all.
        assert_eq!(
            resolve_assets(&song(""), "S", &["S/s.sm"]),
            ResolvedAssets::default()
        );
    }

    #[test]
    fn unclassified_images() {
        let mut s = song("#JACKET:art.png;");
        s.cd_title = Some("logo.gif".into());
        let r = resolve_assets(
            &s,
            "S",
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
        assert_eq!(r.background.as_deref(), Some("S/song-bg.png"));
        assert_eq!(r.unclassified, ["S/Song.png", "S/zeta.png"]);
    }

    #[test]
    fn classify_by_size() {
        let sizes = |list: &[(&str, (u32, u32))]| -> Vec<(String, (u32, u32))> {
            list.iter().map(|&(p, s)| (p.to_string(), s)).collect()
        };
        // The DWI case: `song.png` banner next to a named background.
        let mut r = ResolvedAssets {
            background: Some("S/song-bg.png".into()),
            unclassified: vec!["S/song.png".into()],
            ..Default::default()
        };
        classify_images(&mut r, &sizes(&[("S/song.png", (256, 80))]));
        assert_eq!(r.banner.as_deref(), Some("S/song.png"));
        assert!(r.unclassified.is_empty());

        // Background first when large; overlarge 2:1+ banner; tiny CD
        // title, square jacket and unknown sizes stay unclassified.
        let mut r = ResolvedAssets {
            unclassified: ["a", "b", "c", "d", "e", "f"].map(String::from).to_vec(),
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
        assert_eq!(r.background.as_deref(), Some("c"));
        // 1024×320 is ≥ 320×240, but the background is taken: banner by ratio.
        assert_eq!(r.banner.as_deref(), Some("d"));
        assert_eq!(r.unclassified, ["a", "b", "e", "f"]);

        // A wide image seen before any background becomes the background,
        // as in StepMania; 300×300 is not banner-shaped.
        let mut r = ResolvedAssets {
            unclassified: vec!["wide".into(), "sq".into()],
            ..Default::default()
        };
        classify_images(&mut r, &sizes(&[("wide", (1024, 320)), ("sq", (300, 300))]));
        assert_eq!(r.background.as_deref(), Some("wide"));
        assert_eq!(r.banner, None);

        // Banner-sized rule: 100–320 × 50–240 (not ratio-limited).
        let mut r = ResolvedAssets {
            unclassified: vec!["b".into()],
            ..Default::default()
        };
        classify_images(&mut r, &sizes(&[("b", (200, 200))]));
        assert_eq!(r.banner.as_deref(), Some("b"));
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
    }
}
