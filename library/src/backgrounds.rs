//! Background changes (`#BGCHANGES`) as a timed schedule of still images
//! and movies.
//!
//! Follows StepMania 5.1 (`src/Background.cpp`, `src/NotesLoaderSM.cpp` at
//! `825467b`) for what the game can show, which is still images and movies:
//!
//! - Only the first layer is played (`#BGCHANGES`; `#BGCHANGES2` and
//!   `#FGCHANGES` overlays are not).
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
//! - A change to what is already shown is ignored.
//!
//! Deviation: StepMania splits the tag by matching the song folder's file
//! names first, so a name may contain `,` or `=`; the chart importer splits
//! on them, so such names do not resolve here.

use ddi_chart::{EffectTime, Song, Tick};

use crate::pack::{IMAGE_EXTENSIONS, extension, join_relative};
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
}

/// One background, shown from `seconds` on.
#[derive(Clone, Debug, PartialEq)]
pub struct BgSegment {
    /// Song seconds; the first segment starts at `-inf`.
    pub seconds: f64,
    pub image: BgImage,
    /// Crossfade from the previous segment, seconds (0 = cut).
    pub fade: f64,
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

/// Layer-1 change events of `song`: `(beat tick, fields)`, in file order.
fn layer_one(song: &Song) -> impl Iterator<Item = (Tick, &[String])> {
    song.effects
        .iter()
        .filter(|e| e.kind == "bgchange" && e.layer == 0 && e.fields.len() >= 2)
        .filter_map(|e| match e.at {
            EffectTime::Beat(t) => Some((t, e.fields.as_slice())),
            EffectTime::Seconds(_) => None,
        })
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

/// StepMania's `StringToInt` (`atoi`): the leading integer, 0 if none.
fn leading_int(s: &str) -> i64 {
    let s = s.trim();
    let end = s
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
        .map_or(s.len(), |(i, _)| i);
    s[..end].parse().unwrap_or(0)
}

/// The images `song`'s background changes refer to that exist in the
/// import, as `(path relative to dir, path in the import)`: the first with
/// `../` where it leaves `dir`, both in the files' own case. In order of
/// first use, without duplicates, so a caller keeping only some keeps the
/// earliest. `all_paths` are relative to the import root, `dir` is the song
/// folder; `background` (a path in the import) is the song's own background,
/// which is left out because a change to it shows the song background.
pub fn referenced_images<S: AsRef<str>>(
    song: &Song,
    dir: &str,
    all_paths: &[S],
    background: Option<&str>,
) -> Vec<(String, String)> {
    referenced(song, dir, all_paths, background, is_image)
}

/// The movies `song`'s background changes refer to that exist in the
/// import, as [`referenced_images`] lists images.
pub fn referenced_movies<S: AsRef<str>>(
    song: &Song,
    dir: &str,
    all_paths: &[S],
) -> Vec<(String, String)> {
    referenced(song, dir, all_paths, None, is_movie)
}

fn referenced<S: AsRef<str>>(
    song: &Song,
    dir: &str,
    all_paths: &[S],
    background: Option<&str>,
    wanted_kind: fn(&str) -> bool,
) -> Vec<(String, String)> {
    let background = background.map(str::to_lowercase);
    let mut changes: Vec<(Tick, &[String])> = layer_one(song).collect();
    changes.sort_by_key(|(t, _)| *t);
    let mut out: Vec<(String, String)> = Vec::new();
    for (_, f) in changes {
        let Some(wanted) = join_relative(dir, &f[1]).map(|p| p.to_lowercase()) else {
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
            out.push((crate::pack::relative_path(dir, path), path.to_string()));
        }
    }
    out
}

/// Names StepMania gives a meaning instead of looking up a file.
fn is_special(name: &str) -> bool {
    let name = name.trim();
    [NO_SONG_BG, SONG_BACKGROUND, "-random-"]
        .iter()
        .any(|s| name.eq_ignore_ascii_case(s))
}

/// The files `song`'s background changes name, as written (trimmed), in
/// order of first use, without duplicates or StepMania's special names.
fn change_files(song: &Song) -> Vec<String> {
    let mut changes: Vec<(Tick, &[String])> = layer_one(song).collect();
    changes.sort_by_key(|(t, _)| *t);
    let mut out: Vec<String> = Vec::new();
    for (_, f) in changes {
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
pub fn absent_files<S: AsRef<str>>(song: &Song, dir: &str, all_paths: &[S]) -> Vec<String> {
    change_files(song)
        .into_iter()
        .filter(|name| {
            is_media(name)
                && !join_relative(dir, name).is_some_and(|w| {
                    let w = w.to_lowercase();
                    all_paths.iter().any(|p| p.as_ref().to_lowercase() == w)
                })
        })
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
) -> Vec<(String, String)> {
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
            Some((name.to_string(), found.to_string()))
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
    dir: &str,
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
            let local = join_relative(dir, name).is_some_and(|w| has(all_paths, &w.to_lowercase()));
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
    dir: &str,
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
            let local =
                join_relative(dir, &name).is_some_and(|w| has(all_paths, &w.to_lowercase()));
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
/// caller, so it shows the song background. `has_background`: the song's
/// background image exists.
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
    }];
    for (tick, change) in changes {
        let (image, fade) = match change {
            Change::SongBackground => (BgImage::Song, 0.0),
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
                let fade = match f.get(8).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    Some(transition) => transition_seconds(transition).unwrap_or(0.0),
                    None if f.get(3).is_some_and(|s| leading_int(s) != 0) => 1.0,
                    None => 0.0,
                };
                (image, fade)
            }
        };
        // The song reaches the beat at the start of a delay on it.
        let seconds = song.timing.seconds_at(tick) - song.timing.delay_at(tick);
        // Of changes at the same moment, the last wins.
        if segments.len() > 1 && segments.last().is_some_and(|s| s.seconds == seconds) {
            segments.pop();
        }
        if segments.last().is_some_and(|s| s.image == image) {
            continue;
        }
        segments.push(BgSegment {
            seconds,
            image,
            fade,
        });
    }
    segments
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
pub fn shown(segments: &[BgSegment], t: f64, loaded: &[(BgImage, usize)]) -> Shown {
    let find = |w: &BgImage| loaded.iter().find(|(k, _)| k == w).map(|(_, id)| *id);
    let id = |image: &BgImage| match image {
        BgImage::Song => find(&BgImage::Song),
        file => find(file).or_else(|| find(&BgImage::Song)),
    };
    if segments.is_empty() {
        return Shown {
            current: find(&BgImage::Song),
            previous: None,
            mix: 1.0,
        };
    }
    let state = state_at(segments, t);
    Shown {
        current: id(&segments[state.current].image),
        previous: state.previous.and_then(|p| id(&segments[p].image)),
        mix: state.mix,
    }
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
        let found = referenced_images(&s, "Pack/Song", &FILES, Some("Pack/Song/bg.png"));
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
            shown(&seg, 2.5, &loaded),
            Shown {
                current: Some(1),
                previous: Some(0),
                mix: 0.5
            }
        );
        // B never loaded: the song background instead.
        assert_eq!(shown(&seg, 5.0, &loaded).current, Some(0));
        // Nothing loaded at all: nothing to draw.
        assert_eq!(shown(&seg, 5.0, &[]).current, None);
        // No schedule (calibration): the song background, if loaded.
        assert_eq!(shown(&[], 1.0, &loaded).current, Some(0));
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
        let absent = absent_files(&s, "Pack/S", &all);
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
                    "MAX-EXTREME/robot1.png".to_string(),
                    "RandomMovies/MAX-EXTREME/Robot1.png".to_string()
                ),
                // The pack's own SongMovies folder comes first.
                (
                    "robot2.png".to_string(),
                    "SongMovies/Pack/robot2.png".to_string()
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
            missing_files(&s, "P/S", &all, "P", &shared),
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
            unshown_files(&s, "P/S", &all, "P", &shared),
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
            "Pack/Song",
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
        assert_eq!(shown(&seg, 4.5, &loaded).current, Some(0));
        let playing = [(BgImage::Song, 0), (BgImage::Movie("movie.avi".into()), 7)];
        assert_eq!(shown(&seg, 5.5, &playing).current, Some(7));
    }

    #[test]
    fn leading_int_reads_like_atoi() {
        assert_eq!(leading_int("1"), 1);
        assert_eq!(leading_int(" 1.000"), 1);
        assert_eq!(leading_int("0.5"), 0);
        assert_eq!(leading_int(""), 0);
        assert_eq!(leading_int("-2x"), -2);
    }
}
