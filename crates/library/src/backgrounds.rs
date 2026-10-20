//! Background changes (`#BGCHANGES`, `#BGCHANGES2`) as timed schedules of
//! still images and movies.
//!
//! Follows StepMania 5.1 (`src/Background.cpp`, `src/NotesLoaderSM.cpp` at
//! `825467b`) for what the game can show, which is still images and movies:
//!
//! - Both of StepMania's layers play: the first ([`schedule`]) and the
//!   second, drawn over it ([`second_layer`], whose own rules are listed
//!   there). `#FGCHANGES` overlays do not.
//! - A change naming a `.ini` or `.xml` file (field 2 or 8) is dropped at
//!   load (`SMLoader::LoadFromBGChangesVector`, outside quirks mode).
//! - Before the first change the song's background is shown: StepMania
//!   inserts it at beat −10000.
//! - `-nosongbg-` is not an image but a marker (`SMLoader::TidyUpData`):
//!   it is dropped, and a song with changes but without the marker goes back
//!   to its background at the last beat of its charts, unless a change
//!   already sits at or after that beat, the last change already shows the
//!   song background, or the song has no background.
//! - A change starts when the song reaches its beat on the song's timing
//!   (not a chart's split timing): at the start of a delay on that beat, as
//!   StepMania looks the segment up by beat. The per-song offset, which
//!   shifts the song timing, moves it with the notes. Of several changes on
//!   one beat the last wins.
//! - The file is matched case-insensitively against the song's files, then
//!   against the shared folders in StepMania's order (`SongMovies/<pack>/`,
//!   `SongMovies/`, `RandomMovies/`; [`shared_candidates`]). Movies (by
//!   extension, [`crate::video::is_movie`]) become [`BgImage::Movie`]
//!   segments; scripted animations, folders, `-random-` and missing files
//!   show the song background instead (StepMania would play them, or pick
//!   a random movie).
//! - The transition is field 9 when present, else `CrossFade` when field 4
//!   is a non-zero integer, else a cut. `CrossFade` fades over 1 s,
//!   `CrossFade_Faster` 0.75 s, `CrossFade_Fastest` 0.5 s; StepMania's other
//!   transitions (wipes and slides, 1 s each) become 1 s fades, and an
//!   unknown name is a cut (StepMania reports it and switches at once).
//! - A song with no background changes on either layer and exactly one
//!   movie among its folder's own files plays that movie from beat 0
//!   without looping, until the end (`Song::TidyUpData`: "assume they are
//!   DWI style"; [`implicit_movie`]).
//! - A change to what is already shown is ignored, unless it is a movie
//!   that the change restarts or plays at another rate (StepMania then
//!   skips the transition but runs the effect's `On` command again).
//! - Movies play as StepMania's background effects play them
//!   (`BackgroundEffects/Stretch*.lua`, picked by `LoadFromBGChangesVector`):
//!   field 3 is the rate; field 5 = 0 (no loop) restarts the movie at each
//!   change and holds its last frame at the end, field 6 = 1 (rewind)
//!   restarts it at each change; otherwise the movie resumes where it was
//!   when it last lost focus, and loops ([`movie_position`]). Field 7
//!   names the effect directly. Every change naming a file shares one movie
//!   texture (`RageTextureManager`), so a restart resets it for the later
//!   changes too, and once a change stops it looping it stays that way
//!   ([`movie_loops`]).
//!
//! Deviation: StepMania splits the tag by matching the song folder's file
//! names first, so a name may contain `,` or `=`; the chart importer splits
//! on them, so such names do not resolve here.

use ddi_chart::formats::sm::{sm_float, sm_int};
use ddi_chart::{EffectTime, Song, Tick};

use crate::pack::{IMAGE_EXTENSIONS, extension, join_relative};
use crate::path::{RelPath, RelPathBuf};
use crate::video::is_movie;

/// What a segment shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BgImage {
    /// The song's background image (or nothing, if it has none).
    Song,
    /// An image, as listed in the images given to [`schedule`].
    File(String),
    /// A movie, as listed in the movies given to [`schedule`].
    Movie(String),
    /// The song's opening movie's frame where its change starts, kept as
    /// a still ([`opening_movie`]): what the field shows before the first
    /// change in place of the song's background, and in place of that
    /// movie while it has no frame ([`shown`]). Never in a schedule.
    Still,
    /// Nothing: the second layer before its first change, and where it
    /// would play what the game cannot show ([`second_layer`]).
    Clear,
    /// The plain backdrop, opaque: the second layer's `songbackground`
    /// for a song without a background, where StepMania shows its theme's
    /// fallback graphic ([`second_layer`]). The shell gives it an id.
    Plain,
}

/// A song whose first background change is a movie at or before its first
/// note: that movie's frame where the change starts is the song's
/// background everywhere (song select, the start prompt, the field before
/// the change; the maintainer's decision of 2026-10-16), so the cut to the
/// movie is seamless.
#[derive(Clone, Debug, PartialEq)]
pub struct Opening {
    /// The movie, as its [`BgImage::Movie`] names it.
    pub movie: String,
    /// The schedule segment it starts (always 1: the first change).
    pub segment: usize,
    /// Where the movie is, in movie seconds, when it first shows: 0, or
    /// further in when the change is before the music starts (a negative
    /// beat), at the change's rate ([`movie_position`]).
    pub position: f64,
}

/// Song second at which a play starts drawing: the music is scheduled this
/// far ahead of the start (the web shell's start lookahead takes it from
/// here), so the first frame drawn is at −0.8 s. A change before it is
/// joined part-way, and the opening still is the frame shown then, so the
/// movie runs on from it.
pub const PLAY_START: f64 = -0.8;

/// The song's opening movie ([`Opening`]), from its schedule: `None` when
/// the first change is not a movie, or comes after the earliest first note
/// of the song's playable charts (each on its own timing). Background
/// changes belong to the song, not to a chart (StepMania 5.1 reads
/// `#BGCHANGES` only among the song's tags, `NotesLoaderSSC.cpp`), so one
/// still serves every chart; a chart whose own timing puts its first note
/// before the movie counts like any other.
pub fn opening_movie(song: &Song, segments: &[BgSegment]) -> Option<Opening> {
    let first = segments.get(1)?;
    let BgImage::Movie(movie) = &first.image else {
        return None;
    };
    let first_note = song
        .charts
        .iter()
        .filter(|c| song.layout_of(c).is_some())
        .filter_map(|c| {
            let timing = c.timing(song);
            c.notes
                .iter()
                .map(|n| n.tick)
                .min()
                .map(|t| timing.seconds_at(t))
        })
        .filter(|s| s.is_finite())
        .fold(f64::INFINITY, f64::min);
    if first.seconds > first_note + 1e-9 {
        return None;
    }
    Some(Opening {
        movie: movie.clone(),
        segment: 1,
        position: movie_position(segments, 1, first.seconds.max(PLAY_START)),
    })
}

/// One background, shown from `seconds` on.
#[derive(Clone, Debug, PartialEq)]
pub struct BgSegment {
    /// Song seconds; the first segment starts at `-inf`.
    pub seconds: f64,
    pub image: BgImage,
    /// Crossfade from the previous segment, seconds (0 = cut).
    pub fade: f64,
    /// A movie's playback rate (field 3; 1 for images).
    pub rate: f64,
    pub effect: MovieEffect,
}

/// How a movie plays, after StepMania's background effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MovieEffect {
    /// `StretchNormal`: resumes where it was when it last lost focus; loops.
    #[default]
    Normal,
    /// `StretchRewind`: starts over at each change; loops.
    Rewind,
    /// `StretchNoLoop`: starts over at each change; holds its last frame.
    NoLoop,
}

impl MovieEffect {
    /// Whether the movie starts again at the beginning at each change.
    pub fn restarts(self) -> bool {
        self != MovieEffect::Normal
    }

    /// Whether the movie loops at its end (else its last frame stays).
    pub fn loops(self) -> bool {
        self != MovieEffect::NoLoop
    }

    /// The effect of a change's fields, as `SMLoader::LoadFromBGChangesVector`
    /// picks it: field 7 when given; else field 5 = 0 is `StretchNoLoop`;
    /// else field 6 ≠ 0 is `StretchRewind`.
    fn of(fields: &[String]) -> MovieEffect {
        match fields.get(6).map(|e| e.trim()).filter(|e| !e.is_empty()) {
            Some(e) if e.eq_ignore_ascii_case("StretchNoLoop") => MovieEffect::NoLoop,
            Some(e) if e.eq_ignore_ascii_case("StretchRewind") => MovieEffect::Rewind,
            Some(_) => MovieEffect::Normal,
            None if fields.get(5).is_some_and(|l| sm_int(l) == 0) => MovieEffect::NoLoop,
            None if fields.get(4).is_some_and(|r| sm_int(r) != 0) => MovieEffect::Rewind,
            None => MovieEffect::Normal,
        }
    }
}

/// Which segments to draw at one moment: `current` faded in over `previous`
/// by `mix` (`0..=1`; 1 = only `current`). Indices into the schedule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BgState {
    pub current: usize,
    pub previous: Option<usize>,
    pub mix: f32,
}

/// [`BgState`] resolved to loaded images: ids of the images to draw (`None`
/// for nothing), `current` faded in over `previous` while `mix < 1`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shown {
    pub current: Option<usize>,
    pub previous: Option<usize>,
    pub mix: f32,
}

const NO_SONG_BG: &str = "-nosongbg-";

/// StepMania's `SONG_BACKGROUND_FILE`.
const SONG_BACKGROUND: &str = "songbackground";

/// StepMania's `RANDOM_BACKGROUND_FILE`.
const RANDOM_BACKGROUND: &str = "-random-";

/// Background layers StepMania has (`NUM_BackgroundLayer`, `Song.h`).
pub const LAYERS: usize = 2;

/// Whether `SMLoader::LoadFromBGChangesVector` keeps a change: it drops
/// one whose file (field 2) or second file (field 8) names a `.ini` or
/// `.xml` file (outside quirks mode).
fn loaded(fields: &[String]) -> bool {
    let bad = |f: Option<&String>| {
        f.is_some_and(|f| {
            let f = f.to_lowercase();
            f.contains(".ini") || f.contains(".xml")
        })
    };
    !bad(fields.get(1)) && !bad(fields.get(7))
}

/// Change events of `layer` (0: `#BGCHANGES`, 1: `#BGCHANGES2`):
/// `(beat tick, fields)`, in file order, those StepMania loads.
fn layer_changes(song: &Song, layer: u8) -> impl Iterator<Item = (Tick, &[String])> {
    song.effects
        .iter()
        .filter(move |e| e.kind == "bgchange" && e.layer == layer && e.fields.len() >= 2)
        .filter(|e| loaded(&e.fields))
        .filter_map(|e| match e.at {
            EffectTime::Beat(t) => Some((t, e.fields.as_slice())),
            EffectTime::Seconds(_) => None,
        })
}

/// Layer-1 change events of `song`.
fn layer_one(song: &Song) -> impl Iterator<Item = (Tick, &[String])> {
    layer_changes(song, 0)
}

/// Both layers' change events, by beat (the first layer's before the
/// second's on one beat): what the song's files are listed by.
fn all_changes(song: &Song) -> Vec<(Tick, &[String])> {
    let mut changes: Vec<(Tick, &[String])> = (0..LAYERS as u8)
        .flat_map(|layer| layer_changes(song, layer))
        .collect();
    changes.sort_by_key(|(t, _)| *t);
    changes
}

fn is_image(path: &str) -> bool {
    extension(path).is_some_and(|e| IMAGE_EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// What the game shows of a background-change file: an image or a movie.
fn is_media(path: &str) -> bool {
    is_image(path) || is_movie(path)
}

/// A reference or path in a comparable form: relative to the song folder,
/// `.`/`..` resolved, `/` separators, lower case.
fn key(reference: &str) -> Option<String> {
    join_relative("song", reference).map(|p| p.to_lowercase())
}

/// The images `song`'s background changes (on either layer) refer to that
/// exist in the import, as `(path relative to dir, path in the import)`:
/// the first with `../` where it leaves `dir`, both in the files' own case. In order of
/// first use, without duplicates, so a caller keeping only some keeps the
/// earliest. `all_paths` are relative to the import root, `dir` is the song
/// folder; `background` (a path in the import) is the song's own background,
/// which is left out because a change to it shows the song background.
pub fn referenced_images<S: AsRef<str>>(
    song: &Song,
    dir: &RelPath,
    all_paths: &[S],
    background: Option<&RelPath>,
) -> Vec<(RelPathBuf, RelPathBuf)> {
    referenced(song, dir, all_paths, background, is_image)
}

/// The movies `song`'s background changes refer to that exist in the
/// import, as [`referenced_images`] lists images; for a song without
/// changes, its [`implicit_movie`].
pub fn referenced_movies<S: AsRef<str>>(
    song: &Song,
    dir: &RelPath,
    all_paths: &[S],
) -> Vec<(RelPathBuf, RelPathBuf)> {
    if !has_changes(song) {
        return implicit_movie(song, dir, all_paths).into_iter().collect();
    }
    referenced(song, dir, all_paths, None, is_movie)
}

/// StepMania's `Song::HasBGChanges`: a change on either background layer.
fn has_changes(song: &Song) -> bool {
    (0..LAYERS as u8).any(|layer| layer_changes(song, layer).next().is_some())
}

/// The movie StepMania plays for a song without background changes: the
/// only movie among the song folder's own files (not in subfolders), as
/// `(path relative to dir, path in the import)`; `None` with none, several,
/// or any change. [`schedule`] plays it from beat 0 without looping.
pub fn implicit_movie<S: AsRef<str>>(
    song: &Song,
    dir: &RelPath,
    all_paths: &[S],
) -> Option<(RelPathBuf, RelPathBuf)> {
    if has_changes(song) {
        return None;
    }
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let mut movies = all_paths.iter().map(AsRef::as_ref).filter(|p| {
        p.strip_prefix(prefix.as_str())
            .is_some_and(|name| !name.contains('/') && is_movie(name))
    });
    let movie = movies.next()?;
    if movies.next().is_some() {
        return None;
    }
    let movie = RelPath::new(movie);
    Some((movie.relative_to(dir), movie.to_owned()))
}

fn referenced<S: AsRef<str>>(
    song: &Song,
    dir: &RelPath,
    all_paths: &[S],
    background: Option<&RelPath>,
    wanted_kind: fn(&str) -> bool,
) -> Vec<(RelPathBuf, RelPathBuf)> {
    let background = background.map(|b| b.as_str().to_lowercase());
    let mut out: Vec<(RelPathBuf, RelPathBuf)> = Vec::new();
    for (_, f) in all_changes(song) {
        let Some(wanted) = dir.resolve(&f[1]).map(|p| p.as_str().to_lowercase()) else {
            continue;
        };
        if background.as_deref() == Some(wanted.as_str()) {
            continue;
        }
        let Some(path) = all_paths
            .iter()
            .map(AsRef::as_ref)
            .find(|p| p.to_lowercase() == wanted && wanted_kind(p))
        else {
            continue;
        };
        if !out.iter().any(|(_, p)| p == path) {
            let path = RelPath::new(path);
            out.push((path.relative_to(dir), path.to_owned()));
        }
    }
    out
}

/// Names StepMania gives a meaning instead of looking up a file.
fn is_special(name: &str) -> bool {
    let name = name.trim();
    [NO_SONG_BG, SONG_BACKGROUND, RANDOM_BACKGROUND]
        .iter()
        .any(|s| name.eq_ignore_ascii_case(s))
}

/// The files `song`'s background changes name on either layer, as written
/// (trimmed), in order of first use, without duplicates or StepMania's
/// special names.
fn change_files(song: &Song) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, f) in all_changes(song) {
        let name = f[1].trim();
        if name.is_empty() || is_special(name) {
            continue;
        }
        if !out.iter().any(|o| key(o) == key(name)) {
            out.push(name.to_string());
        }
    }
    out
}

/// Shared-folder paths StepMania tries, in order, for a background-change
/// file the song's folder does not have
/// (`BackgroundUtil::GetGlobalRandomMoviePaths`): `SongMovies/<pack>/`,
/// `SongMovies/`, `RandomMovies/`. Paths as from
/// [`crate::pack::shared_path`].
pub fn shared_candidates(reference: &str, pack: &str) -> Vec<String> {
    let Some(name) = join_relative("", reference) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(3);
    if !pack.is_empty() {
        out.push(format!("SongMovies/{pack}/{name}"));
    }
    out.push(format!("SongMovies/{name}"));
    out.push(format!("RandomMovies/{name}"));
    out
}

/// Images and movies `song`'s background changes name that its folder
/// `dir` does not have (any file of the import, `all_paths`, relative to the
/// import root), as written: the ones to look up in shared folders. A
/// song-folder file always wins, as in StepMania, even when it is not kept
/// (the song's own background, images beyond what is stored, movies not
/// copied).
pub fn absent_files<S: AsRef<str>>(song: &Song, dir: &RelPath, all_paths: &[S]) -> Vec<RelPathBuf> {
    change_files(song)
        .into_iter()
        .filter(|name| {
            is_media(name)
                && !dir.resolve(name).is_some_and(|w| {
                    let w = w.as_str().to_lowercase();
                    all_paths.iter().any(|p| p.as_ref().to_lowercase() == w)
                })
        })
        .map(RelPathBuf::from)
        .collect()
}

/// Of `names` (from [`absent_files`]), those found among `shared`
/// (shared-folder paths, any case): `(name as written, shared path)`, the
/// first match in StepMania's order. These go in the schedule's images
/// under the name as written.
pub fn shared_images<S: AsRef<str>, T: AsRef<str>>(
    names: &[S],
    pack: &str,
    shared: &[T],
) -> Vec<(RelPathBuf, RelPathBuf)> {
    names
        .iter()
        .map(AsRef::as_ref)
        .filter_map(|name| {
            let found = shared_candidates(name, pack).into_iter().find_map(|c| {
                let c = c.to_lowercase();
                shared
                    .iter()
                    .map(AsRef::as_ref)
                    .find(|s| s.to_lowercase() == c)
            })?;
            Some((name.into(), found.into()))
        })
        .collect()
}

/// The images and movies `song`'s background changes name that exist
/// nowhere: not in its folder `dir` (any file of the import, `all_paths`,
/// relative to the import root), nor in `shared` (shared-folder paths). For
/// the import's warnings. Scripted animations do not count: they are never
/// shown, so whether they exist makes no difference.
pub fn missing_files<S: AsRef<str>, T: AsRef<str>>(
    song: &Song,
    dir: &RelPath,
    all_paths: &[S],
    pack: &str,
    shared: &[T],
) -> Vec<String> {
    fn has<S: AsRef<str>>(list: &[S], wanted: &str) -> bool {
        list.iter().any(|p| p.as_ref().to_lowercase() == wanted)
    }
    change_files(song)
        .into_iter()
        .filter(|name| is_media(name))
        .filter(|name| {
            let local = dir
                .resolve(name)
                .is_some_and(|w| has(all_paths, &w.as_str().to_lowercase()));
            let in_shared = shared_candidates(name, pack)
                .iter()
                .any(|c| has(shared, &c.to_lowercase()));
            !local && !in_shared
        })
        .collect()
}

/// Background-change files the game never shows (scripted animations:
/// anything but images and movies), as written, each with whether it
/// exists in the song's folder `dir` (any file or folder of the import,
/// `all_paths`) or in `shared` (shared-folder paths). For the import's
/// notes to pack authors.
pub fn unshown_files<S: AsRef<str>, T: AsRef<str>>(
    song: &Song,
    dir: &RelPath,
    all_paths: &[S],
    pack: &str,
    shared: &[T],
) -> Vec<(String, bool)> {
    // `wanted` (lower case) is a file, or a folder (a scripted animation).
    fn has<S: AsRef<str>>(list: &[S], wanted: &str) -> bool {
        let folder = format!("{wanted}/");
        list.iter().any(|p| {
            let p = p.as_ref().to_lowercase();
            p == wanted || p.starts_with(&folder)
        })
    }
    change_files(song)
        .into_iter()
        .filter(|name| !is_media(name))
        .map(|name| {
            let local = dir
                .resolve(&name)
                .is_some_and(|w| has(all_paths, &w.as_str().to_lowercase()));
            let in_shared = shared_candidates(&name, pack)
                .iter()
                .any(|c| has(shared, &c.to_lowercase()));
            (name, local || in_shared)
        })
        .collect()
}

/// Seconds of a StepMania background transition (`BackgroundTransitions/`),
/// `None` for an unknown name.
fn transition_seconds(name: &str) -> Option<f64> {
    match name.trim().to_lowercase().as_str() {
        "crossfade" => Some(1.0),
        "crossfade_faster" => Some(0.75),
        "crossfade_fastest" => Some(0.5),
        "fadecenterhorizontal"
        | "fadecentervertical"
        | "fadedown"
        | "fadeleft"
        | "faderight"
        | "fadeup"
        | "slidedown"
        | "slideleft"
        | "slideright"
        | "slideup" => Some(1.0),
        _ => None,
    }
}

/// Beat at which the song ends: the last note (or hold tail) of any chart.
fn last_tick(song: &Song) -> Option<Tick> {
    song.charts
        .iter()
        .flat_map(|c| c.notes.iter().map(|n| n.end_tick()))
        .max()
}

/// One change after `TidyUpData`: the file name and fields, or the song
/// background added at the last beat.
enum Change<'a> {
    Fields(&'a [String]),
    SongBackground,
}

/// The schedule of `song`'s background changes. `images` and `movies` are
/// the files available (paths relative to the song folder, as from
/// [`referenced_images`] and [`referenced_movies`], or names as written
/// for shared ones); a [`BgImage::File`] or [`BgImage::Movie`] carries the
/// matching entry. A movie that cannot play is left out of `movies` by the
/// caller, so it shows the song background. A song without changes plays
/// the one movie given, its [`implicit_movie`], from beat 0 without
/// looping. `has_background`: the song's background image exists.
pub fn schedule<S: AsRef<str>, T: AsRef<str>>(
    song: &Song,
    images: &[S],
    movies: &[T],
    has_background: bool,
) -> Vec<BgSegment> {
    // Stable: changes on the same beat keep their file order.
    let mut changes: Vec<(Tick, Change)> = layer_one(song)
        .map(|(t, f)| (t, Change::Fields(f)))
        .collect();
    changes.sort_by_key(|(t, _)| *t);
    let marker = changes.iter().position(
        |(_, c)| matches!(c, Change::Fields(f) if f[1].trim().eq_ignore_ascii_case(NO_SONG_BG)),
    );
    match marker {
        Some(i) => {
            changes.remove(i);
        }
        None if !changes.is_empty() && has_background => {
            let shows_background = |f: &[String]| {
                let name = f[1].trim();
                name.eq_ignore_ascii_case(SONG_BACKGROUND)
                    || (key(name).is_some()
                        && key(name) == song.background.as_deref().and_then(key))
            };
            if let Some(last) = last_tick(song)
                && let Some((tick, Change::Fields(f))) = changes.last()
                && *tick < last
                && !shows_background(f)
            {
                changes.push((last, Change::SongBackground));
            }
        }
        None => {}
    }

    let mut segments = vec![BgSegment {
        seconds: f64::NEG_INFINITY,
        image: BgImage::Song,
        fade: 0.0,
        rate: 1.0,
        effect: MovieEffect::Normal,
    }];
    if !has_changes(song) {
        if let [movie] = movies {
            segments.push(BgSegment {
                seconds: song.timing.seconds_before_delay(Tick(0)),
                image: BgImage::Movie(movie.as_ref().to_string()),
                fade: 0.0,
                rate: 1.0,
                effect: MovieEffect::NoLoop,
            });
        }
        return segments;
    }
    for (tick, change) in changes {
        let (image, fade, rate, effect) = match change {
            Change::SongBackground => (BgImage::Song, 0.0, 1.0, MovieEffect::Normal),
            Change::Fields(f) => {
                let wanted = key(f[1].trim());
                let matching = |i: &&str| key(i) == wanted;
                let image = match wanted {
                    None => BgImage::Song,
                    Some(_) => {
                        if let Some(i) = images.iter().map(AsRef::as_ref).find(matching) {
                            BgImage::File(i.to_string())
                        } else if let Some(m) = movies.iter().map(AsRef::as_ref).find(matching) {
                            BgImage::Movie(m.to_string())
                        } else {
                            BgImage::Song
                        }
                    }
                };
                let (fade, rate, effect) = change_look(f, &image);
                (image, fade, rate, effect)
            }
        };
        let seconds = change_seconds(song, tick);
        // Of changes at the same moment, the last wins.
        if segments.len() > 1 && segments.last().is_some_and(|s| s.seconds == seconds) {
            segments.pop();
        }
        push_change(&mut segments, seconds, image, fade, rate, effect);
    }
    segments
}

/// Song second at which a change on beat `tick` shows: the song reaches
/// the beat at the start of a delay on it.
fn change_seconds(song: &Song, tick: Tick) -> f64 {
    song.timing.seconds_before_delay(tick)
}

/// A change's transition (seconds of fade, 0 for a cut), and the rate and
/// effect it plays `image` with.
fn change_look(f: &[String], image: &BgImage) -> (f64, f64, MovieEffect) {
    let fade = match f.get(8).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(transition) => transition_seconds(transition).unwrap_or(0.0),
        None if f.get(3).is_some_and(|s| sm_int(s) != 0) => 1.0,
        None => 0.0,
    };
    // Only movies have a rate and an effect that matter. Below 0 StepMania
    // stops on an assertion (`Actor::Update` with a negative delta); here
    // the movie stays still. Above 1000 it is held at 1000, so the clock
    // stays finite.
    let (rate, effect) = match image {
        BgImage::Movie(_) => (
            f.get(2).map_or(1.0, |r| {
                let r = sm_float(r);
                if r.is_finite() {
                    r.clamp(0.0, 1000.0)
                } else {
                    0.0
                }
            }),
            MovieEffect::of(f),
        ),
        _ => (1.0, MovieEffect::Normal),
    };
    (fade, rate, effect)
}

/// Adds a change to `segments` as `BackgroundImpl::Layer::UpdateCurBGChange`
/// switches: a change to what is already shown is none (unless a movie
/// starts over or changes rate: then it has no transition).
fn push_change(
    segments: &mut Vec<BgSegment>,
    seconds: f64,
    image: BgImage,
    fade: f64,
    rate: f64,
    effect: MovieEffect,
) {
    let same = segments.last().is_some_and(|s| {
        s.image == image
            && !(matches!(image, BgImage::Movie(_)) && (effect.restarts() || s.rate != rate))
    });
    if same {
        // The one fading in again: the one fading out is dropped
        // (`m_pFadingBGA == m_pCurrentBGA`).
        cut_fade(segments, seconds);
        return;
    }
    // To the same thing (a movie started over): no transition.
    let fade = if segments.last().is_some_and(|s| s.image == image) {
        0.0
    } else {
        fade
    };
    segments.push(BgSegment {
        seconds,
        image,
        fade,
        rate,
        effect,
    });
}

/// Ends a fade running at `seconds` there, as StepMania does when a
/// layer's current background becomes its fading one too (a change to it,
/// or to one never loaded): what fades out is dropped and what fades in
/// shows alone, playing on (a movie neither starts over nor stops).
fn cut_fade(segments: &mut Vec<BgSegment>, seconds: f64) {
    let Some(last) = segments.last() else {
        return;
    };
    if segments.len() > 1 && last.fade > 0.0 && seconds < last.seconds + last.fade {
        let cut = BgSegment {
            seconds,
            fade: 0.0,
            effect: MovieEffect::Normal,
            ..last.clone()
        };
        segments.push(cut);
    }
}

/// The schedule of `song`'s second background layer (`#BGCHANGES2`), drawn
/// over the first ([`schedule`]) as StepMania 5.1 draws it
/// (`BackgroundImpl::LoadFromSong`, `UpdateCurBGChange`, `DrawPrimitives`):
///
/// - It shows nothing ([`BgImage::Clear`]) until its first change: the
///   song background is put at beat −10000 on the first layer only, and
///   that first change cuts in (there is nothing to fade from).
/// - Its changes are timed, matched and played as the first layer's
///   (transitions, rates, movie effects, the last of a beat winning).
/// - A file StepMania never loads leaves the layer as it is, cutting a
///   fade that is running (`Tried to switch to a background that was never
///   loaded`: the faded-out one is dropped): a missing image or movie,
///   and `-nosongbg-` and `-random-`, which mean nothing here
///   (`SMLoader::TidyUpData` and the random backgrounds only look at the
///   first layer). Nothing is put in their place, and nothing returns at
///   the last beat. A change naming no file is not there at all.
/// - `songbackground` (as written: StepMania compares it exactly) and the
///   song's own `#BACKGROUND` file show the song's background;
///   `songbackground` in a song without one (`has_background` false) the
///   plain backdrop ([`BgImage::Plain`]), opaque, where StepMania shows
///   its theme's fallback graphic (`CreateBackground`).
/// - What StepMania would play but the game cannot show (`unshowable`:
///   movies that cannot play, scripted animations that exist) shows
///   nothing on this layer, so the first shows through, as the first
///   layer shows its song background for them. Any other name that is
///   neither an image nor a movie is a BGAnimation found nowhere: never
///   loaded, as above.
///
/// `images` and `movies` are as for [`schedule`] (images and movies of
/// either layer).
pub fn second_layer<S: AsRef<str>, T: AsRef<str>, U: AsRef<str>>(
    song: &Song,
    images: &[S],
    movies: &[T],
    unshowable: &[U],
    has_background: bool,
) -> Vec<BgSegment> {
    let mut changes: Vec<(Tick, &[String])> = layer_changes(song, 1).collect();
    changes.sort_by_key(|(t, _)| *t);
    let mut segments = vec![BgSegment {
        seconds: f64::NEG_INFINITY,
        image: BgImage::Clear,
        fade: 0.0,
        rate: 1.0,
        effect: MovieEffect::Normal,
    }];
    let song_background = song.background.as_deref().and_then(key);
    for (tick, f) in changes {
        // No file at all: not even added (`LoadFromSong` keeps a change of
        // this layer only when its definition names a file).
        if f[1].is_empty() && f.get(7).is_none_or(|f2| f2.is_empty()) {
            continue;
        }
        let name = f[1].trim();
        let wanted = key(name);
        let matching = |i: &&str| key(i) == wanted;
        let image = if name == SONG_BACKGROUND && !has_background {
            Some(BgImage::Plain)
        } else if name == SONG_BACKGROUND
            || (has_background && wanted.is_some() && wanted == song_background)
        {
            Some(BgImage::Song)
        } else if name.is_empty() || is_special(name) || wanted.is_none() {
            None
        } else if let Some(i) = images.iter().map(AsRef::as_ref).find(matching) {
            Some(BgImage::File(i.to_string()))
        } else if let Some(m) = movies.iter().map(AsRef::as_ref).find(matching) {
            Some(BgImage::Movie(m.to_string()))
        } else if unshowable
            .iter()
            .map(AsRef::as_ref)
            .any(|u| key(u) == wanted)
        {
            Some(BgImage::Clear)
        } else {
            None
        };
        let seconds = change_seconds(song, tick);
        if segments.len() > 1 && segments.last().is_some_and(|s| s.seconds == seconds) {
            segments.pop();
        }
        let Some(image) = image else {
            // Never loaded: the layer stays as it is, a fade cut short.
            cut_fade(&mut segments, seconds);
            continue;
        };
        let (fade, rate, effect) = change_look(f, &image);
        // The first change has nothing to fade from.
        let fade = if segments.len() == 1 { 0.0 } else { fade };
        push_change(&mut segments, seconds, image, fade, rate, effect);
    }
    segments
}

/// Where in its movie segment `index` is at song second `t`, in movie
/// seconds before looping: the time the movie's file has played since it
/// last started over (the earlier segments naming it, each for as long as
/// it was current, at its rate; a restarting one starts the count again)
/// plus the time into this segment at its rate. 0 before the segment
/// starts; for a segment that is not a movie, 0. One layer's schedule:
/// [`movie_position_in`] for both.
pub fn movie_position(segments: &[BgSegment], index: usize, t: f64) -> f64 {
    movie_position_in(&[segments], 0, index, t)
}

/// [`movie_position`] of segment `index` of layer `layer` among `layers`
/// (the first layer's schedule, then the second's). Both layers' changes
/// naming a file share one movie texture (`RageTextureManager`), whose
/// clock each current one advances (`Sprite::Update`): the movie plays on
/// either layer's segments, twice as fast while both show it, and starts
/// over at a restarting change on either. On its own layer the segments up
/// to `index` count, on the others those that start before `t` (or, before
/// the segment starts, before it).
pub fn movie_position_in<L: AsRef<[BgSegment]>>(
    layers: &[L],
    layer: usize,
    index: usize,
    t: f64,
) -> f64 {
    let Some(seg) = layers.get(layer).and_then(|l| l.as_ref().get(index)) else {
        return 0.0;
    };
    if !matches!(seg.image, BgImage::Movie(_)) {
        return 0.0;
    }
    let until = t.max(seg.seconds);
    // When the file plays: (from, to, rate), and when it last started over.
    let mut runs = Vec::new();
    let mut restart = f64::NEG_INFINITY;
    for (l, segments) in layers.iter().enumerate() {
        let segments = segments.as_ref();
        for (j, s) in segments.iter().enumerate() {
            if (l == layer && j > index) || (l != layer && s.seconds > until) {
                break;
            }
            if s.image != seg.image {
                continue;
            }
            if s.effect.restarts() {
                restart = restart.max(s.seconds);
            }
            // Current until the next segment of its layer starts (a
            // crossfade pauses the movie that fades out).
            let end = if l == layer && j == index {
                until
            } else {
                segments.get(j + 1).map_or(until, |n| n.seconds.min(until))
            };
            runs.push((s.seconds, end, s.rate));
        }
    }
    let position: f64 = runs
        .into_iter()
        .map(|(from, to, rate)| (to - from.max(restart)).max(0.0) * rate)
        .sum();
    if position.is_finite() { position } else { 0.0 }
}

/// Whether the movie of segment `index` loops at its end: not once a change
/// naming its file (this one or an earlier one) stopped it looping, as the
/// texture they share keeps `loop(false)`. One layer's schedule:
/// [`movie_loops_in`] for both.
pub fn movie_loops(segments: &[BgSegment], index: usize) -> bool {
    movie_loops_in(&[segments], 0, index, f64::NEG_INFINITY)
}

/// [`movie_loops`] of segment `index` of layer `layer` among `layers` at
/// song second `t`: changes on another layer that started by then (or by
/// the time this one starts) count too, as they set `loop(false)` on the
/// texture this segment shows, mid-segment as well.
pub fn movie_loops_in<L: AsRef<[BgSegment]>>(
    layers: &[L],
    layer: usize,
    index: usize,
    t: f64,
) -> bool {
    let Some(seg) = layers.get(layer).and_then(|l| l.as_ref().get(index)) else {
        return true;
    };
    let until = t.max(seg.seconds);
    !layers.iter().enumerate().any(|(l, segments)| {
        segments
            .as_ref()
            .iter()
            .enumerate()
            .take_while(|&(j, s)| {
                if l == layer {
                    j <= index
                } else {
                    s.seconds <= until
                }
            })
            .any(|(_, s)| s.image == seg.image && !s.effect.loops())
    })
}

/// The movie segments a player keeps decoders for at song second `t`
/// ([`movie_segments_at`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MovieSegments {
    /// Each layer's current segment (`None` for an empty schedule).
    pub currents: [Option<usize>; LAYERS],
    /// `(layer, segment)`: each layer's current segment and its next one
    /// starting within `lead` seconds, when they are movies. A file current
    /// on both layers is decoded once (one texture, one clock, as
    /// StepMania keeps one texture per file): by the segment that started
    /// last (whose decoder was readied for its start), the first layer's
    /// on a tie.
    pub wanted: Vec<(usize, usize)>,
}

/// [`MovieSegments`] of `layers` at song second `t`.
pub fn movie_segments_at<L: AsRef<[BgSegment]>>(layers: &[L], t: f64, lead: f64) -> MovieSegments {
    let mut out = MovieSegments::default();
    let mut nexts = Vec::new();
    for (layer, schedule) in layers.iter().enumerate().take(LAYERS) {
        let schedule = schedule.as_ref();
        if schedule.is_empty() {
            continue;
        }
        let current = state_at(schedule, t).current;
        out.currents[layer] = Some(current);
        if matches!(schedule[current].image, BgImage::Movie(_)) {
            out.wanted.push((layer, current));
        }
        let next = schedule
            .iter()
            .enumerate()
            .skip(current + 1)
            .take_while(|(_, s)| s.seconds - t < lead)
            .find(|(_, s)| matches!(s.image, BgImage::Movie(_)))
            .map(|(i, _)| i);
        nexts.extend(next.map(|i| (layer, i)));
    }
    let seg = |(l, i): (usize, usize)| &layers[l].as_ref()[i];
    if let [a, b] = out.wanted[..]
        && seg(a).image == seg(b).image
    {
        let keep = if seg(b).seconds > seg(a).seconds {
            b
        } else {
            a
        };
        out.wanted = vec![keep];
    }
    out.wanted.extend(nexts);
    out
}

/// What `segments` show at song second `t`.
pub fn state_at(segments: &[BgSegment], t: f64) -> BgState {
    let current = segments.iter().rposition(|s| s.seconds <= t).unwrap_or(0);
    let seg = &segments[current];
    if current == 0 || seg.fade <= 0.0 {
        return BgState {
            current,
            previous: None,
            mix: 1.0,
        };
    }
    let mix = ((t - seg.seconds) / seg.fade).clamp(0.0, 1.0) as f32;
    BgState {
        current,
        previous: (mix < 1.0).then_some(current - 1),
        mix,
    }
}

/// What to draw at song second `t`, given the images loaded so far as
/// `(image, id)`. An image that did not load shows the song background; the
/// song background shows nothing when it did not load. Without a schedule
/// the song background is shown.
///
/// A loaded [`BgImage::Still`] (given only for a song with an
/// [`opening_movie`], whose segment 1 it is taken from) shows before the
/// first change instead of the song background, and, while
/// `opening_plays` (the movie is expected to play: not off, failed or
/// turned off), in place of the opening movie's segment until that has a
/// frame, so the cut into the movie is seamless. A movie that does not
/// play shows the song background, as StepMania would; so do later
/// changes to it (named, or StepMania's return at the last beat).
pub fn shown(
    segments: &[BgSegment],
    t: f64,
    loaded: &[(BgImage, usize)],
    opening_plays: bool,
) -> Shown {
    let find = |w: &BgImage| loaded.iter().find(|(k, _)| k == w).map(|(_, id)| *id);
    let song = || find(&BgImage::Still).or_else(|| find(&BgImage::Song));
    let id = |index: usize| match &segments[index].image {
        BgImage::Song if index == 0 => song(),
        BgImage::Song | BgImage::Still => find(&BgImage::Song),
        BgImage::Clear | BgImage::Plain => None,
        movie @ BgImage::Movie(_) if index == 1 && opening_plays => find(movie).or_else(song),
        file => find(file).or_else(|| find(&BgImage::Song)),
    };
    if segments.is_empty() {
        return Shown {
            current: song(),
            previous: None,
            mix: 1.0,
        };
    }
    let state = state_at(segments, t);
    Shown {
        current: id(state.current),
        previous: state.previous.and_then(id),
        mix: state.mix,
    }
}

/// What the second layer ([`second_layer`]) draws at song second `t`, given
/// the images and movie frames loaded so far as `(image, id)`: `current`
/// fully shown, and while `mix < 1` `previous` drawn over it fading out
/// (StepMania draws a layer's fading background after its current one and
/// fades it out: `BackgroundTransitions/CrossFade.xml`). What is not loaded
/// (an image being decoded, a movie before its first frame or that does
/// not play) shows nothing, nor does [`BgImage::Clear`]; the shell gives
/// [`BgImage::Plain`] an id among `loaded`.
pub fn shown_over(segments: &[BgSegment], t: f64, loaded: &[(BgImage, usize)]) -> Shown {
    if segments.is_empty() {
        return Shown {
            mix: 1.0,
            ..Shown::default()
        };
    }
    let id = |index: usize| match &segments[index].image {
        BgImage::Clear | BgImage::Still => None,
        what => loaded.iter().find(|(k, _)| k == what).map(|(_, id)| *id),
    };
    let state = state_at(segments, t);
    Shown {
        current: id(state.current),
        previous: state.previous.and_then(id),
        mix: state.mix,
    }
}

/// How many of `pending` decoded background images go up to the GPU
/// now: all of them outside play (`at` `None`); in play, at song second
/// `at`, one a frame, and only when no change of `layers` is due within a
/// second (an upload never costs a change its frame).
pub fn uploads_due(layers: &[Vec<BgSegment>], at: Option<f64>, pending: usize) -> usize {
    let count = match at {
        None => pending,
        Some(t) => {
            let next = layers
                .iter()
                .filter_map(|l| l.iter().map(|s| s.seconds).find(|&s| s > t))
                .reduce(f64::min);
            usize::from(next.is_none_or(|s| s - t >= 1.0))
        }
    };
    count.min(pending)
}

/// Whether the song's opening movie (`schedule`'s segment 1) is expected
/// to play: movies are not turned `off`, and it is neither among the
/// movies `failed` this run nor in the play's `trouble` (`(name, why)`
/// each). The opening still stands in for it until its first frame only
/// then ([`shown`]).
pub fn opening_plays(
    schedule: &[BgSegment],
    off: bool,
    failed: &[(String, String)],
    trouble: &[(String, String)],
) -> bool {
    let Some(BgImage::Movie(name)) = schedule.get(1).map(|s| &s.image) else {
        return false;
    };
    !off && !failed.iter().any(|(n, _)| n == name) && !trouble.iter().any(|(n, _)| n == name)
}

/// The movie being drawn now, for the automatic video setting's rule, as
/// `segment * LAYERS + layer`: among the movies of each layer's `current`
/// segment that are `shown` (by name: their texture holds a frame of this
/// run), the one `guard` named while it is still drawn, so a change on the
/// other layer does not start the rule's warm-up again, else the first;
/// `guard` is set to it.
pub fn drawing_movie(
    layers: &[Vec<BgSegment>],
    current: &[Option<usize>; LAYERS],
    guard: &std::cell::Cell<Option<(usize, usize)>>,
    shown: impl Fn(&str) -> bool,
) -> Option<usize> {
    let drawing: Vec<(usize, usize)> = current
        .iter()
        .enumerate()
        .filter_map(|(layer, current)| {
            let current = (*current)?;
            let BgImage::Movie(name) = &layers.get(layer)?.get(current)?.image else {
                return None;
            };
            shown(name).then_some((layer, current))
        })
        .collect();
    let key = guard
        .get()
        .filter(|k| drawing.contains(k))
        .or_else(|| drawing.first().copied());
    guard.set(key);
    key.map(|(layer, segment)| segment * LAYERS + layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ddi_chart::formats::sm::parse_simfile;

    /// 120 BPM (beat `b` at `b / 2` s); the chart's last note is on beat 32.
    fn song(tags: &str) -> Song {
        let text = format!(
            "#TITLE:t;#OFFSET:0;#BPMS:0=120;#BACKGROUND:bg.png;{tags}\n\
             #NOTES:dance-single::Easy:1::\n0000\n,\n0000\n,\n0000\n,\n0000\n,\n0000\n,\n\
             0000\n,\n0000\n,\n0000\n,\n1000\n;"
        );
        parse_simfile(&text, "sm").unwrap()
    }

    const FILES: [&str; 7] = [
        "Pack/Song/song.sm",
        "Pack/Song/A.png",
        "Pack/Song/bgs/b.JPG",
        "Pack/Song/movie.avi",
        "Pack/shared.png",
        "Pack/Song/notes.txt",
        "Pack/Song/bg.png",
    ];

    const NO_MOVIES: &[&str] = &[];

    fn shown_images(seg: &[BgSegment]) -> Vec<(f64, BgImage, f64)> {
        seg.iter()
            .map(|s| (s.seconds, s.image.clone(), s.fade))
            .collect()
    }

    #[test]
    fn referenced_images_are_matched_like_stepmania() {
        let s = song(
            "#BGCHANGES:16=../shared.png=1=0=0=0,4=a.png=1=1=0=0,8=BGS/B.jpg=1=0=0=0,\
             12=movie.avi=1=0=0=0,20=missing.png=1=0=0=0,24=a.png=1=0=0=0,26=bg.png=1=0=0=0;\
             #BGCHANGES2:2=notes.txt=1=0=0=0;",
        );
        let found = referenced_images(
            &s,
            RelPath::new("Pack/Song"),
            &FILES,
            Some(RelPath::new("Pack/Song/bg.png")),
        );
        let relative: Vec<&str> = found.iter().map(|(r, _)| r.as_str()).collect();
        // In order of first use; the song's own background is left out.
        assert_eq!(relative, vec!["A.png", "bgs/b.JPG", "../shared.png"]);
        assert_eq!(found[1].1, "Pack/Song/bgs/b.JPG");
    }

    #[test]
    fn schedule_follows_stepmania_semantics() {
        // The marker is present, so nothing is added at the end.
        let s = song(
            "#BGCHANGES:4=a.png=1=1=0=0,8=BGS/B.jpg=1=0=0=0,10=b.jpg=1=0=0=0,\
             12=movie.avi=1=1=0=0,14=a.png=1=0.5=0=0,\
             20=bgs/b.jpg=1.000=0=0=0=StretchNoLoop==CrossFade_Fastest==,\
             24=A.png=1=0=0=0=StretchNoLoop==SlideLeft==,\
             28=bgs/b.jpg=1=0=0=0=StretchNoLoop==NoSuchTransition==,99999=-nosongbg-=1=0=0=0;",
        );
        let seg = schedule(&s, &["A.png", "bgs/b.JPG"], NO_MOVIES, true);
        assert_eq!(
            shown_images(&seg),
            vec![
                (f64::NEG_INFINITY, BgImage::Song, 0.0),
                // Field 4 = 1: CrossFade.
                (2.0, BgImage::File("A.png".into()), 1.0),
                // Case does not matter; no crossfade: a cut.
                (4.0, BgImage::File("bgs/b.JPG".into()), 0.0),
                // `b.jpg` at the top level does not exist: the song
                // background. The video also shows the song background,
                // which is already shown, so that change is dropped.
                (5.0, BgImage::Song, 0.0),
                // Field 4 is read as an integer: 0.5 is 0, a cut.
                (7.0, BgImage::File("A.png".into()), 0.0),
                // Field 9 names the transition.
                (10.0, BgImage::File("bgs/b.JPG".into()), 0.5),
                // Wipes and slides become 1 s fades.
                (12.0, BgImage::File("A.png".into()), 1.0),
                // An unknown transition cuts.
                (14.0, BgImage::File("bgs/b.JPG".into()), 0.0),
            ]
        );
    }

    #[test]
    fn without_the_marker_the_song_background_returns_at_the_last_beat() {
        let s = song("#BGCHANGES:4=a.png=1=1=0=0;");
        let seg = schedule(&s, &["A.png"], NO_MOVIES, true);
        assert_eq!(seg.len(), 3);
        assert_eq!(
            (seg[2].seconds, &seg[2].image, seg[2].fade),
            (16.0, &BgImage::Song, 0.0)
        );
        // Not when the song has no background, nor when a change already
        // sits at or after the last beat, nor when the last change already
        // shows the background.
        assert_eq!(schedule(&s, &["A.png"], NO_MOVIES, false).len(), 2);
        let late = song("#BGCHANGES:4=a.png=1=1=0=0,32=a.png=1=0=0=0;");
        assert_eq!(schedule(&late, &["A.png"], NO_MOVIES, true).len(), 2);
        let back = song("#BGCHANGES:4=a.png=1=1=0=0,8=BG.PNG=1=0=0=0;");
        let seg = schedule(&back, &["A.png"], NO_MOVIES, true);
        assert_eq!(seg.len(), 3);
        assert_eq!(seg[2].seconds, 4.0);
    }

    #[test]
    fn a_song_without_changes_shows_its_background() {
        let seg = schedule::<&str, &str>(&song(""), &[], NO_MOVIES, true);
        assert_eq!(seg.len(), 1);
        assert_eq!(seg[0].image, BgImage::Song);
        assert_eq!(state_at(&seg, -5.0).current, 0);
        assert_eq!(state_at(&seg, 99.0).current, 0);
    }

    #[test]
    fn the_last_change_on_a_beat_wins() {
        let s = song("#BGCHANGES:4=a.png=1=0=0=0,8=a.png=1=0=0=0,8=bgs/b.jpg=1=1=0=0;");
        let seg = schedule(&s, &["A.png", "bgs/b.JPG"], NO_MOVIES, true);
        // B crossfades from A, the one actually shown before it.
        assert_eq!(seg[2].image, BgImage::File("bgs/b.JPG".into()));
        assert_eq!(state_at(&seg, 4.5).previous, Some(1));
        assert_eq!(seg.len(), 4);
    }

    #[test]
    fn state_crossfades_and_cuts() {
        let s = song("#BGCHANGES:4=a.png=1=1=0=0,8=bgs/b.jpg=1=0=0=0,99999=-nosongbg-;");
        let seg = schedule(&s, &["A.png", "bgs/b.JPG"], NO_MOVIES, true);
        assert_eq!(
            state_at(&seg, 1.9),
            BgState {
                current: 0,
                previous: None,
                mix: 1.0
            }
        );
        // Halfway through the 1 s crossfade into A.
        assert_eq!(
            state_at(&seg, 2.5),
            BgState {
                current: 1,
                previous: Some(0),
                mix: 0.5
            }
        );
        assert_eq!(state_at(&seg, 3.0).previous, None);
        // A cut has no fade.
        assert_eq!(
            state_at(&seg, 4.0),
            BgState {
                current: 2,
                previous: None,
                mix: 1.0
            }
        );
    }

    #[test]
    fn changes_use_the_song_timing_and_start_with_a_delay() {
        // A BPM change halfway: beat 8 is at 2 s + 4 beats at 240 BPM = 3 s.
        let s = song("#BPMS:0=120,4=240;#BGCHANGES:8=a.png=1=0=0=0;");
        let seg = schedule(&s, &["a.png"], NO_MOVIES, false);
        assert!((seg[1].seconds - 3.0).abs() < 1e-9);
        // A 0.5 s delay on beat 4: the change shows when the delay begins.
        let s = song("#DELAYS:4=0.5;#BGCHANGES:4=a.png=1=0=0=0;");
        let seg = schedule(&s, &["a.png"], NO_MOVIES, false);
        assert!((seg[1].seconds - 2.0).abs() < 1e-9);
    }

    #[test]
    fn shown_falls_back_to_the_song_background() {
        let s = song("#BGCHANGES:4=a.png=1=1=0=0,8=bgs/b.jpg=1=0=0=0,99999=-nosongbg-;");
        let seg = schedule(&s, &["A.png", "bgs/b.JPG"], NO_MOVIES, true);
        let loaded = [(BgImage::Song, 0), (BgImage::File("A.png".into()), 1)];
        // Mid-crossfade from the song background into A.
        assert_eq!(
            shown(&seg, 2.5, &loaded, true),
            Shown {
                current: Some(1),
                previous: Some(0),
                mix: 0.5
            }
        );
        // B never loaded: the song background instead.
        assert_eq!(shown(&seg, 5.0, &loaded, true).current, Some(0));
        // Nothing loaded at all: nothing to draw.
        assert_eq!(shown(&seg, 5.0, &[], true).current, None);
        // No schedule (calibration): the song background, if loaded.
        assert_eq!(shown(&[], 1.0, &loaded, true).current, Some(0));
    }

    #[test]
    fn shared_images_follow_stepmanias_search_order() {
        let s = song(
            "#BGCHANGES:4=MAX-EXTREME/robot1.png=1=0=0=1,6=robot2.png=1=0=0=1,\
             8=local.png=1=0=0=1,10=movie.avi=1=0=0=1,12=-nosongbg-=1=0=0=0;",
        );
        assert_eq!(
            shared_candidates("MAX-EXTREME/robot1.png", "Pack"),
            vec![
                "SongMovies/Pack/MAX-EXTREME/robot1.png",
                "SongMovies/MAX-EXTREME/robot1.png",
                "RandomMovies/MAX-EXTREME/robot1.png"
            ]
        );
        let shared = [
            "RandomMovies/MAX-EXTREME/Robot1.png",
            "RandomMovies/robot2.png",
            "SongMovies/Pack/robot2.png",
            "RandomMovies/local.png",
        ];
        let all = ["Pack/S/s.sm", "Pack/S/local.png", "Pack/S/bg.png"];
        let absent = absent_files(&s, RelPath::new("Pack/S"), &all);
        // The movie is absent too: shared folders hold movies as well.
        assert_eq!(
            absent,
            vec!["MAX-EXTREME/robot1.png", "robot2.png", "movie.avi"]
        );
        assert_eq!(
            shared_images(&absent, "Pack", &shared),
            vec![
                // Case-insensitive, under RandomMovies.
                (
                    RelPathBuf::from("MAX-EXTREME/robot1.png"),
                    RelPathBuf::from("RandomMovies/MAX-EXTREME/Robot1.png")
                ),
                // The pack's own SongMovies folder comes first.
                (
                    RelPathBuf::from("robot2.png"),
                    RelPathBuf::from("SongMovies/Pack/robot2.png")
                ),
                // local.png is in the song folder: never looked up; the
                // movie is in no shared folder.
            ]
        );
        // A shared image then plays like a local one.
        let images = ["MAX-EXTREME/robot1.png"];
        let seg = schedule(&s, &images, NO_MOVIES, true);
        assert_eq!(seg[1].image, BgImage::File("MAX-EXTREME/robot1.png".into()));
    }

    #[test]
    fn missing_files_are_those_found_nowhere() {
        let s = song(
            "#BGCHANGES:4=a.png=1=0=0=1,6=movie.avi=1=0=0=1,8=gone.png=1=0=0=1,\
             10=MAX-EXTREME/robot1.png=1=0=0=1,12=anim=1=0=0=1,14=gone.avi=1=0=0=1,\
             16=-random-=1=0=0=1;",
        );
        let all = [
            "P/S/s.sm",
            "P/S/A.PNG",
            "P/S/movie.avi",
            "P/S/anim/default.xml",
        ];
        let shared = ["RandomMovies/MAX-EXTREME/Robot1.png"];
        assert_eq!(
            missing_files(&s, RelPath::new("P/S"), &all, "P", &shared),
            // The scripted animation and `-random-` never count.
            vec!["gone.png", "gone.avi"]
        );
    }

    #[test]
    fn unshown_files_are_everything_but_images_and_movies() {
        let s = song(
            "#BGCHANGES:4=a.png=1=0=0=1,6=movie.avi=1=0=0=1,8=anim=1=0=0=1,\
             10=gone.avi=1=0=0=1,12=MAX-EXTREME/fire1.avi=1=0=0=1,14=-random-=1=0=0=1;",
        );
        let all = [
            "P/S/s.sm",
            "P/S/A.PNG",
            "P/S/Movie.AVI",
            "P/S/anim/default.xml",
        ];
        let shared = ["RandomMovies/MAX-EXTREME/fire1.avi"];
        assert_eq!(
            unshown_files(&s, RelPath::new("P/S"), &all, "P", &shared),
            vec![("anim".to_string(), true)]
        );
    }

    #[test]
    fn movies_get_their_own_segments() {
        let s = song(
            "#BGCHANGES:4=a.png=1=0=0=0,8=Movie.AVI=1=1=0=0,12=gone.avi=1=0=0=0,\
             16=bgs/clip.mp4=1=0=0=0,20=unplayable.mp4=1=0=0=0;",
        );
        let found = referenced_movies(
            &s,
            RelPath::new("Pack/Song"),
            &[
                "Pack/Song/song.sm",
                "Pack/Song/movie.avi",
                "Pack/Song/bgs/clip.mp4",
                "Pack/Song/unplayable.mp4",
            ],
        );
        let relative: Vec<&str> = found.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(
            relative,
            vec!["movie.avi", "bgs/clip.mp4", "unplayable.mp4"]
        );
        // The caller leaves out what cannot play (unplayable.mp4).
        let seg = schedule(&s, &["A.png"], &["movie.avi", "bgs/clip.mp4"], true);
        assert_eq!(
            shown_images(&seg),
            vec![
                (f64::NEG_INFINITY, BgImage::Song, 0.0),
                (2.0, BgImage::File("A.png".into()), 0.0),
                // Matched case-insensitively; field 4 = 1 crossfades.
                (4.0, BgImage::Movie("movie.avi".into()), 1.0),
                // Missing: the song background.
                (6.0, BgImage::Song, 0.0),
                (8.0, BgImage::Movie("bgs/clip.mp4".into()), 0.0),
                // Cannot play: the song background (already shown at the
                // end of the song: not repeated).
                (10.0, BgImage::Song, 0.0),
            ]
        );
        // Until a movie has a frame, the song background stands in.
        let loaded = [(BgImage::Song, 0), (BgImage::File("A.png".into()), 1)];
        assert_eq!(shown(&seg, 4.5, &loaded, true).current, Some(0));
        let playing = [(BgImage::Song, 0), (BgImage::Movie("movie.avi".into()), 7)];
        assert_eq!(shown(&seg, 5.5, &playing, true).current, Some(7));
    }

    #[test]
    fn an_opening_movie_at_or_before_the_first_note() {
        // The chart's first note is on beat 32 (16 s).
        let at = |tags: &str, movies: &[&str]| {
            let s = song(tags);
            opening_movie(&s, &schedule(&s, &["A.png"], movies, true))
        };
        let opening = |position: f64| {
            Some(Opening {
                movie: "movie.avi".into(),
                segment: 1,
                position,
            })
        };
        assert_eq!(
            at("#BGCHANGES:1=movie.avi=1=0=0=1;", &["movie.avi"]),
            opening(0.0)
        );
        // On the first note's beat still counts; after it, not.
        assert_eq!(
            at("#BGCHANGES:32=movie.avi=1=0=0=1;", &["movie.avi"]),
            opening(0.0)
        );
        assert_eq!(at("#BGCHANGES:33=movie.avi=1=0=0=1;", &["movie.avi"]), None);
        // The first change must be the movie: an image first, a movie that
        // cannot play, or none at all keeps the song's background.
        assert_eq!(
            at(
                "#BGCHANGES:1=a.png=1=0=0=1,2=movie.avi=1=0=0=1;",
                &["movie.avi"]
            ),
            None
        );
        assert_eq!(at("#BGCHANGES:1=movie.avi=1=0=0=1;", &[]), None);
        assert_eq!(at("", &[]), None);
        // A change before the play's first frame (beat −8, −4 s; the
        // first frame is at −0.8 s) is joined part way, at its rate: the
        // movie is 3.2 s in at 1×, 6.4 s in at 2×. One after it (beat −1,
        // −0.5 s) starts from the movie's beginning.
        let position = |tags: &str| at(tags, &["movie.avi"]).map(|o| o.position);
        let near = |p: Option<f64>, want: f64| p.is_some_and(|p| (p - want).abs() < 1e-9);
        assert!(near(position("#BGCHANGES:-8=movie.avi=1=0=0=1;"), 3.2));
        assert!(near(position("#BGCHANGES:-8=movie.avi=2=0=0=1;"), 6.4));
        assert_eq!(
            at("#BGCHANGES:-1=movie.avi=1=0=0=1;", &["movie.avi"]),
            opening(0.0)
        );
        // A song without changes and with one movie plays it from beat 0.
        let s = song("");
        let seg = schedule(&s, &[] as &[&str], &["movie.avi"], true);
        assert_eq!(opening_movie(&s, &seg), opening(0.0));
        // The earliest first note of any chart, on its own timing: a second
        // chart whose first note is on beat 1 (0.5 s) puts it before a
        // movie on beat 4.
        let two = parse_simfile(
            "#TITLE:t;#OFFSET:0;#BPMS:0=120;#BGCHANGES:4=movie.avi=1=0=0=1;\n\
             #NOTES:dance-single::Easy:1::\n0000\n0000\n0000\n0000\n,\n1000\n0000\n0000\n0000\n;\n\
             #NOTES:dance-single::Hard:1::\n0000\n1000\n0000\n0000\n;",
            "sm",
        )
        .unwrap();
        let seg = schedule(&two, &[] as &[&str], &["movie.avi"], false);
        assert_eq!(opening_movie(&two, &seg), None);
    }

    #[test]
    fn the_still_stands_in_for_the_song_background_before_the_movie() {
        // The movie on beat 4 (2 s), an image on beat 8, the song's
        // background again on beat 32 (16 s).
        let s = song("#BGCHANGES:4=movie.avi=1=0=0=1,8=a.png=1=0=0=0;");
        let seg = schedule(&s, &["A.png"], &["movie.avi"], true);
        assert!(opening_movie(&s, &seg).is_some());
        let still = [
            (BgImage::Song, 0),
            (BgImage::File("A.png".into()), 1),
            (BgImage::Still, 2),
        ];
        // Before the change, the still; at the change, still the still until
        // the movie has a frame; then the movie.
        assert_eq!(shown(&seg, 1.0, &still, true).current, Some(2));
        assert_eq!(shown(&seg, 2.5, &still, true).current, Some(2));
        let playing = [&still[..], &[(BgImage::Movie("movie.avi".into()), 3)]].concat();
        assert_eq!(shown(&seg, 2.5, &playing, true).current, Some(3));
        // The return to the song background at the last beat shows the
        // song's own background, as StepMania does.
        assert_eq!(shown(&seg, 17.0, &still, true).current, Some(0));
        // A movie that does not play (off, failed, turned off): the song
        // background from the change on, the still only before it.
        assert_eq!(shown(&seg, 1.0, &still, false).current, Some(2));
        assert_eq!(shown(&seg, 2.5, &still, false).current, Some(0));
        // Without a still, the song background throughout.
        assert_eq!(shown(&seg, 1.0, &still[..2], true).current, Some(0));
        assert_eq!(shown(&seg, 2.5, &still[..2], true).current, Some(0));
    }

    #[test]
    fn movies_resume_restart_and_change_rate_like_stepmania() {
        // 120 BPM: beat b at b/2 s. intro.avi (normal) at 4, image at 8,
        // intro.avi again at 12 (resumes), at 16 at half rate (a new
        // segment, no fade), loop.avi rewinding at 20 and again at 24 (each
        // starts over), end.avi without looping at 28.
        let s = song(
            "#BGCHANGES:4=intro.avi=1.000=1=0=1,8=a.png=1=0=0=0,12=intro.avi=1=0=0=1,\
             16=intro.avi=0.5=0=0=1,20=loop.avi=1=0=1=1,24=loop.avi=1=0=1=1,\
             28=end.avi=2=0=0=0,30=end.avi==0=0=0;",
        );
        let movies = ["intro.avi", "loop.avi", "end.avi"];
        let seg = schedule(&s, &["A.png"], &movies, true);
        let kinds: Vec<(f64, BgImage, f64, f64, MovieEffect)> = seg
            .iter()
            .map(|s| (s.seconds, s.image.clone(), s.fade, s.rate, s.effect))
            .collect();
        let movie = |n: &str| BgImage::Movie(n.into());
        use MovieEffect::*;
        assert_eq!(
            kinds,
            vec![
                (f64::NEG_INFINITY, BgImage::Song, 0.0, 1.0, Normal),
                (2.0, movie("intro.avi"), 1.0, 1.0, Normal),
                (4.0, BgImage::File("A.png".into()), 0.0, 1.0, Normal),
                (6.0, movie("intro.avi"), 0.0, 1.0, Normal),
                // Same movie, another rate: kept, without a transition.
                (8.0, movie("intro.avi"), 0.0, 0.5, Normal),
                (10.0, movie("loop.avi"), 0.0, 1.0, Rewind),
                // Same movie restarting: kept.
                (12.0, movie("loop.avi"), 0.0, 1.0, Rewind),
                (14.0, movie("end.avi"), 0.0, 2.0, NoLoop),
                // An empty rate is StringToFloat's 0: a frozen movie.
                (15.0, movie("end.avi"), 0.0, 0.0, NoLoop),
                // Back to the song background at the last beat (32).
                (16.0, BgImage::Song, 0.0, 1.0, Normal),
            ]
        );
        let at = |i: usize, t: f64| movie_position(&seg, i, t);
        // The first segment from 0; before it starts, 0.
        assert_eq!(at(1, 1.0), 0.0);
        assert_eq!(at(1, 3.0), 1.0);
        // Resumes after the 2 s it played (2..4), whatever came between.
        assert_eq!(at(3, 6.0), 2.0);
        assert!(movie_loops(&seg, 3));
        assert!(!movie_loops(&seg, 8), "end.avi does not loop");
        assert_eq!(at(3, 7.5), 3.5);
        // Then 2 more (6..8), and on at half rate.
        assert_eq!(at(4, 9.0), 4.5);
        // Rewinding: from 0 at each change.
        assert_eq!(at(5, 11.0), 1.0);
        assert_eq!(at(6, 13.0), 1.0);
        // No loop: from 0, at double rate; then frozen at 0.
        assert_eq!(at(7, 14.5), 1.0);
        assert_eq!(at(8, 15.5), 0.0);
        // Not a movie.
        assert_eq!(at(2, 5.0), 0.0);
        assert_eq!(at(99, 5.0), 0.0);
    }

    #[test]
    fn one_file_shares_its_clock_across_effects() {
        // StepMania's shared texture: bg.avi plays 0..4 s (normal), a
        // rewinding change at 8 s starts it over (plays 8..10), and the
        // normal change at 16 s resumes from there, at 2 s.
        let s = song(
            "#BGCHANGES:0=bg.avi=1=0=0=1,8=a.png=1=0=0=0,16=bg.avi=1=0=1=1,\
             20=a.png=1=0=0=0,32=bg.avi=1=0=0=1,40=other.avi=1=0=0=0,44=bg.avi=1=0=0=1;",
        );
        let seg = schedule(&s, &["A.png"], &["bg.avi", "other.avi"], true);
        let index = |t: f64| seg.iter().rposition(|s| s.seconds <= t).unwrap();
        assert_eq!(movie_position(&seg, index(16.0), 16.0), 2.0);
        // other.avi stops looping; bg.avi keeps looping (another file).
        assert!(!movie_loops(&seg, index(20.5)));
        assert!(movie_loops(&seg, index(22.5)));
    }

    #[test]
    fn a_song_without_changes_plays_its_only_movie() {
        let s = song("");
        let one = [
            "P/S/s.sm",
            "P/S/Song.AVI",
            "P/S/bg.png",
            "P/S/sub/other.avi",
        ];
        // Only the folder's own files count.
        assert_eq!(
            implicit_movie(&s, RelPath::new("P/S"), &one),
            Some(("Song.AVI".into(), "P/S/Song.AVI".into()))
        );
        assert_eq!(referenced_movies(&s, RelPath::new("P/S"), &one).len(), 1);
        let two = ["P/S/s.sm", "P/S/a.avi", "P/S/b.mpg"];
        assert_eq!(implicit_movie(&s, RelPath::new("P/S"), &two), None);
        // Any change, on either layer, turns the rule off.
        let layer2 = song("#BGCHANGES2:0=bg.png=1=0=0=0;");
        assert_eq!(implicit_movie(&layer2, RelPath::new("P/S"), &one), None);
        assert!(referenced_movies(&layer2, RelPath::new("P/S"), &one).is_empty());
        // From beat 0, without looping, and no return to the background.
        let seg = schedule(&s, &[] as &[&str], &["Song.AVI"], true);
        assert_eq!(
            shown_images(&seg),
            vec![
                (f64::NEG_INFINITY, BgImage::Song, 0.0),
                (0.0, BgImage::Movie("Song.AVI".into()), 0.0),
            ]
        );
        assert_eq!(seg[1].effect, MovieEffect::NoLoop);
        assert!(!movie_loops(&seg, 1));
        // Without a playable movie, just the background.
        assert_eq!(schedule(&s, &[] as &[&str], NO_MOVIES, true).len(), 1);
    }

    /// `(seconds, image, fade)` of a schedule, for the second layer.
    fn layer2(tags: &str, images: &[&str], movies: &[&str], unshowable: &[&str]) -> Vec<BgSegment> {
        second_layer(&song(tags), images, movies, unshowable, true)
    }

    #[test]
    fn the_second_layer_starts_empty_and_cuts_in() {
        // Field 4 = 1 asks for a crossfade, but the layer has nothing to
        // fade from (`m_pCurrentBGA` is null: no transition runs).
        let seg = layer2(
            "#BGCHANGES2:4=a.png=1=1=0=0,8=bgs/b.jpg=1=1=0=0;",
            &["A.png", "bgs/b.JPG"],
            &[],
            &[],
        );
        assert_eq!(
            shown_images(&seg),
            vec![
                (f64::NEG_INFINITY, BgImage::Clear, 0.0),
                (2.0, BgImage::File("A.png".into()), 0.0),
                (4.0, BgImage::File("bgs/b.JPG".into()), 1.0),
            ]
        );
        let loaded = [
            (BgImage::Song, 0),
            (BgImage::File("A.png".into()), 1),
            (BgImage::File("bgs/b.JPG".into()), 2),
        ];
        // Nothing before the first change (not the song background: that
        // is the first layer's at beat −10000).
        let nothing = Shown {
            current: None,
            previous: None,
            mix: 1.0,
        };
        assert_eq!(shown_over(&seg, 1.0, &loaded), nothing);
        assert_eq!(shown_over(&seg, 2.0, &loaded).current, Some(1));
        // Halfway through the crossfade: B shown, A over it fading out.
        assert_eq!(
            shown_over(&seg, 4.5, &loaded),
            Shown {
                current: Some(2),
                previous: Some(1),
                mix: 0.5
            }
        );
        // An image not decoded yet shows nothing on this layer.
        assert_eq!(shown_over(&seg, 5.0, &loaded[..2]).current, None);
        // No schedule (calibration): nothing.
        assert_eq!(shown_over(&[], 5.0, &loaded), nothing);
    }

    #[test]
    fn the_second_layer_ignores_what_stepmania_never_loads() {
        // A missing image, `-nosongbg-`, `-random-`, a missing movie:
        // StepMania reports "Tried to switch to a background that was never
        // loaded" and the layer keeps what it shows; an empty name is not
        // even added; no song background is put at the last beat (32).
        let seg = layer2(
            "#BGCHANGES2:4=a.png=1=0=0=0,6=gone.png=1=0=0=0,8=-nosongbg-=1=0=0=0,\
             10=-random-=1=0=0=0,12==1=0=0=0,14=gone.avi=1=0=0=0;",
            &["A.png"],
            &[],
            &[],
        );
        assert_eq!(
            shown_images(&seg),
            vec![
                (f64::NEG_INFINITY, BgImage::Clear, 0.0),
                (2.0, BgImage::File("A.png".into()), 0.0),
            ]
        );
        // The marker on the second layer means nothing to the first: it
        // still returns to the song background at the last beat.
        let s = song("#BGCHANGES:4=a.png=1=0=0=0;#BGCHANGES2:8=-nosongbg-=1=0=0=0;");
        let first = schedule(&s, &["A.png"], NO_MOVIES, true);
        assert_eq!(
            first.last().map(|s| (s.seconds, &s.image)),
            Some((16.0, &BgImage::Song))
        );
        // Of changes on one beat the last wins, even one never loaded: the
        // layer then stays as it was before that beat.
        let seg = layer2(
            "#BGCHANGES2:4=a.png=1=0=0=0,8=bgs/b.jpg=1=0=0=0,8=gone.png=1=0=0=0;",
            &["A.png", "bgs/b.JPG"],
            &[],
            &[],
        );
        assert_eq!(seg.len(), 2);
        assert_eq!(seg[1].image, BgImage::File("A.png".into()));
        // A change without a file is not there: it does not take its
        // beat from the one before it.
        let seg = layer2(
            "#BGCHANGES2:4=a.png=1=0=0=0,4==1=0=0=0;",
            &["A.png"],
            &[],
            &[],
        );
        assert_eq!(seg.len(), 2);
    }

    #[test]
    fn a_change_never_loaded_cuts_a_running_fade() {
        // B crossfades in over 1 s from 4 s; a missing file at 4.5 s drops
        // the faded-out A (`m_pFadingBGA = m_pCurrentBGA`): B alone from
        // then on. One at 6 s, after the fade, changes nothing.
        let seg = layer2(
            "#BGCHANGES2:4=a.png=1=0=0=0,8=bgs/b.jpg=1=1=0=0,9=gone.png=1=0=0=0,\
             12=gone.png=1=0=0=0;",
            &["A.png", "bgs/b.JPG"],
            &[],
            &[],
        );
        assert_eq!(
            shown_images(&seg),
            vec![
                (f64::NEG_INFINITY, BgImage::Clear, 0.0),
                (2.0, BgImage::File("A.png".into()), 0.0),
                (4.0, BgImage::File("bgs/b.JPG".into()), 1.0),
                (4.5, BgImage::File("bgs/b.JPG".into()), 0.0),
            ]
        );
        let loaded = [
            (BgImage::File("A.png".into()), 1),
            (BgImage::File("bgs/b.JPG".into()), 2),
        ];
        assert_eq!(shown_over(&seg, 4.25, &loaded).previous, Some(1));
        assert_eq!(
            shown_over(&seg, 4.75, &loaded),
            Shown {
                current: Some(2),
                previous: None,
                mix: 1.0
            }
        );
        // A movie cut that way plays on: it neither starts over nor stops.
        let seg = layer2(
            "#BGCHANGES2:4=a.png=1=0=0=0,8=movie.avi=1=1=1=0,9=gone.png=1=0=0=0;",
            &["A.png"],
            &["movie.avi"],
            &[],
        );
        assert_eq!(seg.len(), 4);
        assert_eq!(movie_position(&seg, 3, 5.0), 1.0);
        assert!(!movie_loops(&seg, 3), "it still does not loop");
    }

    #[test]
    fn the_second_layer_shows_the_song_background_and_clears_for_the_unshowable() {
        let seg = layer2(
            "#BGCHANGES2:2=a.png=1=0=0=0,4=songbackground=1=0=0=0,6=a.png=1=0=0=0,\
             8=BG.png=1=0=0=0,10=a.png=1=0=0=0,12=SongBackground=1=0=0=0,\
             14=anim=1=0=0=0,16=a.png=1=0=0=0,18=bad.mkv=1=0=0=0,20=a.png=1=0=0=0,\
             22=movie.avi=2=0=1=1,24=nowhere=1=0=0=0;",
            &["A.png"],
            &["movie.avi"],
            &["bad.mkv", "anim"],
        );
        assert_eq!(
            shown_images(&seg),
            vec![
                (f64::NEG_INFINITY, BgImage::Clear, 0.0),
                (1.0, BgImage::File("A.png".into()), 0.0),
                // `songbackground`, and the song's `#BACKGROUND` by name.
                (2.0, BgImage::Song, 0.0),
                (3.0, BgImage::File("A.png".into()), 0.0),
                (4.0, BgImage::Song, 0.0),
                (5.0, BgImage::File("A.png".into()), 0.0),
                // Compared exactly: `SongBackground` is a missing file.
                // A scripted animation that exists and a movie that cannot
                // play: the layer clears. A BGAnimation found nowhere
                // (`nowhere`, at 12 s) is never loaded: nothing changes.
                (7.0, BgImage::Clear, 0.0),
                (8.0, BgImage::File("A.png".into()), 0.0),
                (9.0, BgImage::Clear, 0.0),
                (10.0, BgImage::File("A.png".into()), 0.0),
                (11.0, BgImage::Movie("movie.avi".into()), 0.0),
            ]
        );
        assert_eq!((seg[10].rate, seg[10].effect), (2.0, MovieEffect::Rewind));
        let loaded = [(BgImage::Song, 0), (BgImage::File("A.png".into()), 1)];
        assert_eq!(shown_over(&seg, 2.5, &loaded).current, Some(0));
        assert_eq!(shown_over(&seg, 7.5, &loaded).current, None);
        // Before its first frame the movie shows nothing.
        assert_eq!(shown_over(&seg, 11.5, &loaded).current, None);
        let playing = [&loaded[..], &[(BgImage::Movie("movie.avi".into()), 5)]].concat();
        assert_eq!(shown_over(&seg, 11.5, &playing).current, Some(5));
        // A song without a background: `songbackground` is the plain
        // backdrop (StepMania's fallback graphic), its file a missing one.
        let s = song(
            "#BGCHANGES2:2=a.png=1=0=0=0,4=songbackground=1=0=0=0,6=a.png=1=0=0=0,8=bg.png=1=0=0=0;",
        );
        let seg = second_layer(&s, &["A.png"], NO_MOVIES, NO_MOVIES, false);
        assert_eq!(seg[2].image, BgImage::Plain);
        let plain = [(BgImage::Plain, 9), (BgImage::File("A.png".into()), 1)];
        assert_eq!(shown_over(&seg, 2.5, &plain).current, Some(9));
        assert_eq!(seg.len(), 4);
    }

    #[test]
    fn a_change_to_what_fades_in_cuts_the_fade() {
        // B fades in over A from 4 s; B again at 4.5 s: StepMania's fading
        // background becomes its current one and is dropped, so A goes
        // (`m_pFadingBGA == m_pCurrentBGA`), on either layer.
        let tags = "#BGCHANGES:4=a.png=1=0=0=0,8=bgs/b.jpg=1=1=0=0,9=bgs/b.jpg=1=0=0=0,\
                    12=bgs/b.jpg=1=0=0=0,99999=-nosongbg-;\
                    #BGCHANGES2:4=a.png=1=0=0=0,8=bgs/b.jpg=1=1=0=0,9=bgs/b.jpg=1=0=0=0;";
        let s = song(tags);
        let images = ["A.png", "bgs/b.JPG"];
        let want = |seg: &[BgSegment]| {
            assert_eq!(
                shown_images(&seg[1..]),
                vec![
                    (2.0, BgImage::File("A.png".into()), 0.0),
                    (4.0, BgImage::File("bgs/b.JPG".into()), 1.0),
                    (4.5, BgImage::File("bgs/b.JPG".into()), 0.0),
                ]
            );
            assert_eq!(state_at(seg, 4.75).previous, None);
        };
        want(&schedule(&s, &images, NO_MOVIES, true));
        want(&second_layer(&s, &images, NO_MOVIES, NO_MOVIES, true));
    }

    #[test]
    fn ini_and_xml_changes_are_dropped_at_load() {
        // `LoadFromBGChangesVector` returns false for them: the change is
        // not there at all, on either layer (the image before stays).
        let tags = "#BGCHANGES:4=a.png=1=0=0=0,8=anim.xml=1=0=0=0,10=a.png=1=0=0=0=StretchNormal=b.INI,\
                    99999=-nosongbg-;#BGCHANGES2:4=a.png=1=0=0=0,8=Foo.XML/x=1=0=0=0;";
        let s = song(tags);
        let first = schedule(&s, &["A.png"], NO_MOVIES, true);
        assert_eq!(first.len(), 2);
        let second = second_layer(&s, &["A.png"], NO_MOVIES, NO_MOVIES, true);
        assert_eq!(second.len(), 2);
        let all = ["P/S/s.sm", "P/S/a.png", "P/S/anim.xml"];
        assert!(unshown_files(&s, RelPath::new("P/S"), &all, "P", NO_MOVIES).is_empty());
        // A song whose only change is dropped has none (`HasBGChanges`):
        // its one movie plays.
        let only = song("#BGCHANGES:4=anim.xml=1=0=0=0;");
        let files = ["P/S/s.sm", "P/S/song.avi"];
        assert!(implicit_movie(&only, RelPath::new("P/S"), &files).is_some());
    }

    #[test]
    fn the_second_layers_files_are_listed_with_the_first() {
        // A song whose only changes are on the second layer: its files are
        // stored and looked up like the first layer's, in order of first
        // use across both; the first layer shows the song background
        // throughout (no implicit movie: `HasBGChanges` counts both).
        let s = song(
            "#BGCHANGES2:2=bgs/b.jpg=1=0=0=0,6=movie.avi=1=0=0=0,8=robot.png=1=0=0=0;\
             #BGCHANGES:4=a.png=1=0=0=0,99999=-nosongbg-;",
        );
        let relative =
            |v: Vec<(RelPathBuf, RelPathBuf)>| v.into_iter().map(|(r, _)| r).collect::<Vec<_>>();
        assert_eq!(
            relative(referenced_images(
                &s,
                RelPath::new("Pack/Song"),
                &FILES,
                None
            )),
            vec!["bgs/b.JPG", "A.png"]
        );
        assert_eq!(
            relative(referenced_movies(&s, RelPath::new("Pack/Song"), &FILES)),
            vec!["movie.avi"]
        );
        assert_eq!(
            absent_files(&s, RelPath::new("Pack/Song"), &FILES),
            vec!["robot.png"]
        );
        let only2 = song("#BGCHANGES2:2=bgs/b.jpg=1=0=0=0;");
        let first = schedule(&only2, &["bgs/b.JPG"], &["movie.avi"], true);
        assert_eq!(
            shown_images(&first),
            vec![(f64::NEG_INFINITY, BgImage::Song, 0.0)]
        );
        assert!(referenced_movies(&only2, RelPath::new("Pack/Song"), &FILES).is_empty());
    }

    #[test]
    fn a_movie_on_both_layers_shares_one_clock() {
        // bg.avi on the first layer from 0 s (normal); on the second from
        // 2 s to 4 s: both advance the shared texture, so it plays twice
        // as fast meanwhile. A rewinding change on the second layer at
        // 6 s starts it over for the first layer too.
        let s = song(
            "#BGCHANGES:0=bg.avi=1=0=0=1,99999=-nosongbg-;\
             #BGCHANGES2:4=bg.avi=1=0=0=1,8=a.png=1=0=0=0,12=bg.avi=1=0=1=1,14=a.png=1=0=0=0;",
        );
        let movies = ["bg.avi"];
        let layers = [
            schedule(&s, &["A.png"], &movies, true),
            second_layer(&s, &["A.png"], &movies, NO_MOVIES, true),
        ];
        assert_eq!(layers[0].len(), 2);
        assert_eq!(layers[1].len(), 5);
        let first = |t: f64| movie_position_in(&layers, 0, 1, t);
        assert_eq!(first(1.0), 1.0);
        assert_eq!(first(3.0), 4.0);
        assert_eq!(first(5.0), 7.0);
        // The second layer's segment sees the same clock.
        assert_eq!(movie_position_in(&layers, 1, 1, 3.0), 4.0);
        // Started over at 6 s by the second layer.
        assert_eq!(first(6.5), 1.0);
        assert_eq!(movie_position_in(&layers, 1, 3, 6.5), 1.0);
        // Before the second layer's segment starts: where it will be then.
        assert_eq!(movie_position_in(&layers, 1, 1, 1.0), 2.0);
        // On one layer alone, as before.
        assert_eq!(movie_position(&layers[0], 1, 5.0), 5.0);
        // A change that stops it looping on one layer stops it on the
        // other from then on.
        let s = song(
            "#BGCHANGES:0=bg.avi=1=0=0=1,8=a.png=1=0=0=0,12=bg.avi=1=0=0=1,99999=-nosongbg-;\
             #BGCHANGES2:4=bg.avi=1=0=0=0;",
        );
        let layers = [
            schedule(&s, &["A.png"], &movies, true),
            second_layer(&s, &["A.png"], &movies, NO_MOVIES, true),
        ];
        // Layer 1's run loops until layer 2's no-loop change at 2 s sets
        // `loop(false)` on the texture it shows, mid-segment.
        assert!(movie_loops_in(&layers, 0, 1, 0.0));
        assert!(movie_loops_in(&layers, 0, 1, 1.9));
        assert!(!movie_loops_in(&layers, 0, 1, 2.0));
        assert!(!movie_loops_in(&layers, 1, 1, 2.0));
        assert!(!movie_loops_in(&layers, 0, 3, 6.0));
    }

    #[test]
    fn a_file_current_on_both_layers_is_decoded_once() {
        // bg.avi on the first layer from 0 s; on the second from 2 s, and
        // a rewinding change to it at 6 s; other.avi next on the first.
        let s = song(
            "#BGCHANGES:0=bg.avi=1=0=0=1,16=other.avi=1=0=0=1,99999=-nosongbg-;\
             #BGCHANGES2:4=bg.avi=1=0=0=1,8=a.png=1=0=0=0,12=bg.avi=1=0=1=1;",
        );
        let movies = ["bg.avi", "other.avi"];
        let layers = [
            schedule(&s, &["A.png"], &movies, true),
            second_layer(&s, &["A.png"], &movies, NO_MOVIES, true),
        ];
        let at = |t: f64| movie_segments_at(&layers, t, 2.0);
        // Before the second layer's: the first's, and the second's readied.
        assert_eq!(at(1.0).wanted, vec![(0, 1), (1, 1)]);
        // Both current: once, by the one that started last.
        assert_eq!(at(3.0).wanted, vec![(1, 1)]);
        assert_eq!(at(3.0).currents, [Some(1), Some(1)]);
        // The second layer shows a.png: the first's again, the restart
        // readied; then the restart decodes it for both, other.avi readied.
        assert_eq!(at(4.5).wanted, vec![(0, 1), (1, 3)]);
        assert_eq!(at(6.5).wanted, vec![(1, 3), (0, 2)]);
    }
}
