//! StepMania 5.1's random backgrounds (`docs/plans/phase-8-gaps.md`,
//! step 1): the movies of the player's `RandomMovies` folder that play
//! behind a song without background changes, in place of a `-random-`
//! change, and in place of a first-layer file found nowhere. Read from
//! StepMania 5.1 (`src/Background.cpp`: `LoadFromSong`, `LoadFromRandom`,
//! `Layer::CreateRandomBGA`; `src/BackgroundUtil.cpp`:
//! `GetGlobalRandomMoviePaths`, `GetFilterToFileNames`; `src/Song.cpp`:
//! `ReCalculateRadarValuesAndLastSecond`; `Themes/_fallback/metrics.ini`'s
//! `[Background]`, at `825467b`), its default `RandomBackgroundMode`
//! being `RandomMovies`:
//!
//! - **The pool** ([`random_pool`]): the movies directly in
//!   `RandomMovies/<pack>/` when it has any, else in `RandomMovies/`;
//!   only `.ogv`, `.avi`, `.mpg` and `.mpeg`. With a genre, the pack's
//!   `RandomMovies/<pack>/BackgroundMapping.ini` maps it
//!   (`[GenreToSection]`) to a section whose keys are the file names
//!   allowed, unless none of them is there. Shuffled the same way every
//!   play of a song and cut to [`NUM_BACKGROUNDS`]; handed out in turn
//!   ([`Rotation`]), never shuffled again.
//! - **When they change** ([`expand`]): within each time signature, every
//!   [`CHANGE_MEASURES`] of its measures, counted from the later of its
//!   start and the span's; a BPM change on a measure's start adds one.
//!   The first change of the song's first time signature is put at beat
//!   [`START_BEAT`], so it shows from the start (for a `-random-` change
//!   inside that first time signature, it sorts before the changes before
//!   it and is hardly seen: StepMania's own quirk, kept).
//! - **The span** of a song without changes ([`span`]): its first note to
//!   its last, as StepMania finds them, then back to the song background.
//!
//! Deviations: the shuffle is ours (SplitMix64 seeded by the song's id,
//! which both shells give a song alike), not StepMania's generator and
//! hash of the song's folder; the file names are sorted before it, so a
//! song shuffles the same on both shells; names, genres and sections are
//! compared without regard to case. Animations mode (StepMania's global
//! `BGAnimations`) is left out: the stock ones are four flashes.

use std::collections::VecDeque;

use ddi_chart::{Song, SourceFormat, TICKS_PER_BEAT, Tick};

use crate::pack::{extension, fnv1a64};

/// The shared folder random movies come from.
pub const DIR: &str = "RandomMovies";

/// StepMania's `NumBackgrounds` preference (default 10).
pub const NUM_BACKGROUNDS: usize = 10;

/// `[Background] RandomBGChangeMeasures`.
pub const CHANGE_MEASURES: i64 = 4;

/// `[Background] RandomBGStartBeat`.
pub const START_BEAT: i64 = -1000;

/// The extensions `GetGlobalRandomMoviePaths` lists.
const EXTENSIONS: [&str; 4] = ["ogv", "avi", "mpg", "mpeg"];

/// The global `BGAnimations` a stock StepMania has: a change naming one
/// is found there (so it takes no random movie), though not drawn here.
const STOCK_ANIMATIONS: [&str; 4] = [
    "flash",
    "white flash",
    "white reverse flash",
    "yellow flash",
];

/// Whether a first-layer change's file, found nowhere among the song's,
/// takes a random movie (`LoadFromSong`: `CreateBackground` failed): not
/// a stock animation StepMania would find, nor a name it gives a meaning.
pub(super) fn replaces(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && !super::is_special(name)
        && !STOCK_ANIMATIONS
            .iter()
            .any(|a| a.eq_ignore_ascii_case(name))
}

/// Where a pack's genre mapping is, as a shared path.
pub fn mapping_path(pack: &str) -> String {
    format!("{DIR}/{pack}/BackgroundMapping.ini")
}

/// Whether a shared path is a pack's genre mapping ([`mapping_path`]),
/// in any case.
pub fn is_mapping(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    matches!(parts.as_slice(), [d, p, f] if d.eq_ignore_ascii_case(DIR)
        && !p.is_empty()
        && f.eq_ignore_ascii_case("BackgroundMapping.ini"))
}

/// The random movies `song` (of id `id`, in pack `pack`) plays, as shared
/// paths (`RandomMovies/…`, as from [`crate::pack::shared_path`]) among
/// the library's `shared` files, in the order they come; `mapping` is the
/// text of the pack's [`mapping_path`] when there is one. None for a
/// Dancing☆Onigiri work, which is explicitly without a background (its
/// back layers are its own).
pub fn random_pool<S: AsRef<str>>(
    song: &Song,
    id: &str,
    pack: &str,
    shared: &[S],
    mapping: Option<&str>,
) -> Vec<String> {
    if song.source.format == SourceFormat::Danoni {
        return Vec::new();
    }
    let allowed = match mapping {
        Some(text) if !song.genre.trim().is_empty() => allowed_names(text, song.genre.trim()),
        _ => Vec::new(),
    };
    let group = format!("{DIR}/{pack}/");
    let top = format!("{DIR}/");
    let dirs: Vec<&str> = if pack.is_empty() {
        vec![top.as_str()]
    } else {
        vec![group.as_str(), top.as_str()]
    };
    for dir in dirs {
        let mut found: Vec<&str> = shared
            .iter()
            .map(AsRef::as_ref)
            .filter(|p| {
                starts_with_ignoring_case(p, dir)
                    && !p[dir.len()..].contains('/')
                    && extension(p)
                        .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
            })
            .collect();
        if !allowed.is_empty() {
            let matching: Vec<&str> = found
                .iter()
                .copied()
                .filter(|p| {
                    let name = &p[dir.len()..];
                    allowed.iter().any(|a| a.eq_ignore_ascii_case(name))
                })
                .collect();
            if !matching.is_empty() {
                found = matching;
            }
        }
        if found.is_empty() {
            continue;
        }
        found.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b)));
        found.dedup();
        shuffle(&mut found, fnv1a64(id.as_bytes()));
        found.truncate(NUM_BACKGROUNDS);
        return found.into_iter().map(str::to_string).collect();
    }
    Vec::new()
}

fn starts_with_ignoring_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.is_char_boundary(prefix.len())
        && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// The file names a `BackgroundMapping.ini` allows for `genre`: the keys
/// of the section its `[GenreToSection]` names for it.
fn allowed_names(text: &str, genre: &str) -> Vec<String> {
    let sections = ini_sections(text);
    let section = sections
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("GenreToSection"))
        .and_then(|(_, keys)| {
            keys.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(genre))
                .map(|(_, v)| v.clone())
        });
    let Some(section) = section else {
        return Vec::new();
    };
    sections
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(&section))
        .map(|(_, keys)| keys.into_iter().map(|(k, _)| k).collect())
        .unwrap_or_default()
}

/// An INI file's sections and their `key=value` lines, as StepMania's
/// `IniFile` reads them (`;`, `#` and `//` lines are comments).
fn ini_sections(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}').trim();
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with('#')
            || line.starts_with("//")
        {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push((name.trim().to_string(), Vec::new()));
        } else if let Some((key, value)) = line.split_once('=')
            && let Some((_, keys)) = out.last_mut()
        {
            keys.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    out
}

/// A Fisher–Yates shuffle on SplitMix64 from `seed`.
fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    for i in (1..items.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

/// The pool handed out in turn (`CreateRandomBGA`): each taken goes to
/// the back.
pub(super) struct Rotation {
    items: VecDeque<String>,
}

impl Rotation {
    pub(super) fn new<S: AsRef<str>>(pool: &[S]) -> Rotation {
        Rotation {
            items: pool.iter().map(|s| s.as_ref().to_string()).collect(),
        }
    }

    pub(super) fn next(&mut self) -> Option<String> {
        let first = self.items.pop_front()?;
        self.items.push_back(first.clone());
        Some(first)
    }
}

/// A change [`expand`] makes: on its tick, a movie of the pool; `on_bpm`
/// for the one a BPM change adds (which keeps the colors of the change
/// it stands for, where the others keep only its effect).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Made {
    pub tick: Tick,
    pub movie: String,
    pub on_bpm: bool,
}

/// `LoadFromRandom` from `start` to `end` (ticks, StepMania's rows): the
/// changes it makes, in the order it makes them, each taking the next
/// movie of `rotation` (none when it is empty).
pub(super) fn expand(song: &Song, start: Tick, end: Tick, rotation: &mut Rotation) -> Vec<Made> {
    let timing = &song.timing;
    let default_signature = [ddi_chart::TimeSignature {
        tick: Tick::ZERO,
        numerator: 4,
        denominator: 4,
    }];
    let signatures = if timing.time_signatures.is_empty() {
        &default_signature[..]
    } else {
        &timing.time_signatures[..]
    };
    let per_measure = |s: &ddi_chart::TimeSignature| {
        (TICKS_PER_BEAT * 4 * s.numerator as i64) / (s.denominator.max(1) as i64)
    };
    let mut out = Vec::new();
    for (i, signature) in signatures.iter().enumerate() {
        let segment_end = signatures.get(i + 1).map_or(end, |s| s.tick);
        let first = signature.tick.max(start);
        let step = (CHANGE_MEASURES * per_measure(signature)).max(1);
        let mut row = first;
        while row < end.min(segment_end) {
            let Some(movie) = rotation.next() else {
                break;
            };
            let tick = if row == first && i == 0 {
                Tick(START_BEAT * TICKS_PER_BEAT)
            } else {
                row
            };
            out.push(Made {
                tick,
                movie,
                on_bpm: false,
            });
            row = Tick(row.0 + step);
        }
    }
    // `RandomBGChangesWhenBPMChanges`: a BPM change on any time
    // signature's measure start, within the span.
    for bpm in &timing.bpms {
        let on_measure = signatures.iter().any(|s| {
            let per = per_measure(s);
            per > 0 && (bpm.tick.0 - s.tick.0) % per == 0
        });
        if !on_measure || bpm.tick < start || bpm.tick >= end {
            continue;
        }
        let Some(movie) = rotation.next() else {
            continue;
        };
        out.push(Made {
            tick: bpm.tick,
            movie,
            on_bpm: true,
        });
    }
    out
}

/// The song's first and last beats as StepMania finds them
/// (`Song::GetFirstBeat`, `GetLastBeat`): each chart's first and last
/// note in seconds on its own timing, then on the song's timing; not a
/// generated chart, an edit when there are other charts, a lights chart
/// or one with nothing past its first row. `None` without such a chart.
pub(crate) fn span(song: &Song) -> Option<(f64, f64)> {
    let originals = song
        .charts
        .iter()
        .filter(|c| c.derivation.is_none())
        .count();
    let mut first = f64::INFINITY;
    let mut last = f64::NEG_INFINITY;
    for chart in song.charts.iter().filter(|c| c.derivation.is_none()) {
        if chart.difficulty == ddi_chart::Difficulty::Edit && originals > 1 {
            continue;
        }
        if chart.layout.starts_with("lights") {
            continue;
        }
        let Some(end) = chart.notes.iter().map(|n| n.end_tick()).max() else {
            continue;
        };
        if end == Tick::ZERO {
            continue;
        }
        let Some(start) = chart.notes.iter().map(|n| n.tick).min() else {
            continue;
        };
        let timing = chart.timing(song);
        first = first.min(timing.seconds_at(start));
        last = last.max(timing.seconds_at(end));
    }
    if !last.is_finite() {
        return None;
    }
    let first = if first < last { first } else { 0.0 };
    Some((song.timing.beat_at(first), song.timing.beat_at(last)))
}

#[cfg(test)]
mod tests;
