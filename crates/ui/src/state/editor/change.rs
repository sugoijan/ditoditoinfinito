//! What one edit did, and how to take it back: each [`Change`] applied to
//! the song it was made on gives the song after it, and its
//! [`Change::inverse`] applied then gives the song before.

use ddi_chart::{Chart, DisplayBpm, Note, Song, TimingMap};

/// One edit as data.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Change {
    /// Notes of chart `chart` taken out, then others put in.
    Notes {
        chart: usize,
        removed: Vec<Note>,
        added: Vec<Note>,
    },
    /// The song's timing (`chart: None`) or a chart's own (`Some`; `None`
    /// for a chart on the song's).
    Timing {
        chart: Option<usize>,
        before: Option<Box<TimingMap>>,
        after: Option<Box<TimingMap>>,
    },
    /// The song's own fields.
    Song {
        before: Box<SongInfo>,
        after: Box<SongInfo>,
    },
    /// A chart's own fields.
    Chart {
        chart: usize,
        before: Box<ChartInfo>,
        after: Box<ChartInfo>,
    },
    /// A chart put in at `index`, with its tags kept as written: their
    /// places in `SourceInfo::unknown_tags` and names without the
    /// `NOTEDATA[index].` prefix.
    InsertChart {
        index: usize,
        chart: Box<Chart>,
        tags: Vec<(usize, String, String)>,
    },
    /// The chart at `index` taken out.
    RemoveChart {
        index: usize,
        chart: Box<Chart>,
        tags: Vec<(usize, String, String)>,
    },
    /// Several, in order.
    Batch(Vec<Change>),
}

impl Change {
    pub(crate) fn inverse(&self) -> Change {
        match self {
            Change::Notes {
                chart,
                removed,
                added,
            } => Change::Notes {
                chart: *chart,
                removed: added.clone(),
                added: removed.clone(),
            },
            Change::Timing {
                chart,
                before,
                after,
            } => Change::Timing {
                chart: *chart,
                before: after.clone(),
                after: before.clone(),
            },
            Change::Song { before, after } => Change::Song {
                before: after.clone(),
                after: before.clone(),
            },
            Change::Chart {
                chart,
                before,
                after,
            } => Change::Chart {
                chart: *chart,
                before: after.clone(),
                after: before.clone(),
            },
            Change::InsertChart { index, chart, tags } => Change::RemoveChart {
                index: *index,
                chart: chart.clone(),
                tags: tags.clone(),
            },
            Change::RemoveChart { index, chart, tags } => Change::InsertChart {
                index: *index,
                chart: chart.clone(),
                tags: tags.clone(),
            },
            Change::Batch(changes) => {
                Change::Batch(changes.iter().rev().map(Change::inverse).collect())
            }
        }
    }

    /// Applies the change to the song it was made on.
    pub(crate) fn apply(&self, song: &mut Song) {
        match self {
            Change::Notes {
                chart,
                removed,
                added,
            } => {
                let notes = &mut song.charts[*chart].notes;
                // Notes are sorted and one to a cell: each removed one is
                // found by its cell.
                let mut gone = vec![false; notes.len()];
                for n in removed {
                    let at = notes.partition_point(|m| (m.tick, m.lane) < (n.tick, n.lane));
                    let found = notes.get(at).is_some_and(|m| m == n);
                    debug_assert!(found, "removing a note not there: {n:?}");
                    if found {
                        gone[at] = true;
                    }
                }
                let mut i = 0;
                notes.retain(|_| {
                    i += 1;
                    !gone[i - 1]
                });
                for n in added {
                    let at = notes.partition_point(|m| (m.tick, m.lane) < (n.tick, n.lane));
                    notes.insert(at, n.clone());
                }
            }
            Change::Timing {
                chart: None, after, ..
            } => {
                if let Some(after) = after {
                    song.timing = (**after).clone();
                }
            }
            Change::Timing {
                chart: Some(chart),
                after,
                ..
            } => song.charts[*chart].timing = after.as_deref().cloned(),
            Change::Song { after, .. } => after.put(song),
            Change::Chart { chart, after, .. } => after.put(&mut song.charts[*chart]),
            Change::InsertChart { index, chart, tags } => {
                renumber_tags(song, *index, 1);
                song.charts.insert(*index, (**chart).clone());
                let list = &mut song.source.unknown_tags;
                for (at, name, value) in tags {
                    let tag = (format!("NOTEDATA[{index}].{name}"), value.clone());
                    list.insert((*at).min(list.len()), tag);
                }
            }
            Change::RemoveChart { index, .. } => {
                let prefix = format!("NOTEDATA[{index}].");
                song.source
                    .unknown_tags
                    .retain(|(k, _)| !k.starts_with(&prefix));
                song.charts.remove(*index);
                renumber_tags(song, *index + 1, -1);
            }
            Change::Batch(changes) => {
                for c in changes {
                    c.apply(song);
                }
            }
        }
    }
}

/// Moves the tags of charts from `from` on by `by` places.
fn renumber_tags(song: &mut Song, from: usize, by: isize) {
    for (name, _) in &mut song.source.unknown_tags {
        let Some(rest) = name.strip_prefix("NOTEDATA[") else {
            continue;
        };
        let Some((index, tail)) = rest.split_once(']') else {
            continue;
        };
        let Ok(index) = index.parse::<usize>() else {
            continue;
        };
        if index >= from {
            *name = format!("NOTEDATA[{}]{tail}", index.saturating_add_signed(by));
        }
    }
}

/// A chart's tags kept as written: their places, and names without their
/// `NOTEDATA[i].` prefix.
pub(crate) fn chart_tags(song: &Song, index: usize) -> Vec<(usize, String, String)> {
    let prefix = format!("NOTEDATA[{index}].");
    song.source
        .unknown_tags
        .iter()
        .enumerate()
        .filter_map(|(at, (k, v))| {
            k.strip_prefix(&prefix)
                .map(|k| (at, k.to_string(), v.clone()))
        })
        .collect()
}

/// The song's fields the editor sets, apart from its timing and charts.
#[derive(Clone, Debug, PartialEq)]
pub struct SongInfo {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub title_translit: String,
    pub subtitle_translit: String,
    pub artist_translit: String,
    pub genre: String,
    pub credit: String,
    pub music: Option<String>,
    pub banner: Option<String>,
    pub background: Option<String>,
    pub jacket: Option<String>,
    pub cd_title: Option<String>,
    pub preview_start: f64,
    pub preview_length: f64,
    pub display_bpm: DisplayBpm,
}

impl SongInfo {
    pub fn of(song: &Song) -> SongInfo {
        SongInfo {
            title: song.title.clone(),
            subtitle: song.subtitle.clone(),
            artist: song.artist.clone(),
            title_translit: song.title_translit.clone(),
            subtitle_translit: song.subtitle_translit.clone(),
            artist_translit: song.artist_translit.clone(),
            genre: song.genre.clone(),
            credit: song.credit.clone(),
            music: song.music.clone(),
            banner: song.banner.clone(),
            background: song.background.clone(),
            jacket: song.jacket.clone(),
            cd_title: song.cd_title.clone(),
            preview_start: song.preview_start,
            preview_length: song.preview_length,
            display_bpm: song.display_bpm,
        }
    }

    fn put(&self, song: &mut Song) {
        song.title = self.title.clone();
        song.subtitle = self.subtitle.clone();
        song.artist = self.artist.clone();
        song.title_translit = self.title_translit.clone();
        song.subtitle_translit = self.subtitle_translit.clone();
        song.artist_translit = self.artist_translit.clone();
        song.genre = self.genre.clone();
        song.credit = self.credit.clone();
        song.music = self.music.clone();
        song.banner = self.banner.clone();
        song.background = self.background.clone();
        song.jacket = self.jacket.clone();
        song.cd_title = self.cd_title.clone();
        song.preview_start = self.preview_start;
        song.preview_length = self.preview_length;
        song.display_bpm = self.display_bpm;
    }

    /// As a file keeps it: text trimmed (the reader trims every value),
    /// files `None` when empty, numbers finite and not negative, a range
    /// of one value a single one.
    pub(crate) fn tidy(mut self) -> SongInfo {
        for s in [
            &mut self.title,
            &mut self.subtitle,
            &mut self.artist,
            &mut self.title_translit,
            &mut self.subtitle_translit,
            &mut self.artist_translit,
            &mut self.genre,
            &mut self.credit,
        ] {
            *s = s.trim().to_string();
        }
        for f in [
            &mut self.music,
            &mut self.banner,
            &mut self.background,
            &mut self.jacket,
            &mut self.cd_title,
        ] {
            *f = f
                .take()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        self.preview_start = finite_or(self.preview_start, 0.0).max(0.0);
        self.preview_length = finite_or(self.preview_length, 0.0).max(0.0);
        self.display_bpm = tidy_display_bpm(self.display_bpm);
        self
    }
}

/// A chart's own fields the editor sets.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartInfo {
    pub difficulty: ddi_chart::Difficulty,
    pub meter: u32,
    pub name: String,
    pub description: String,
    pub credit: String,
    pub display_bpm: Option<DisplayBpm>,
}

impl ChartInfo {
    pub fn of(chart: &Chart) -> ChartInfo {
        ChartInfo {
            difficulty: chart.difficulty,
            meter: chart.meter,
            name: chart.name.clone(),
            description: chart.description.clone(),
            credit: chart.credit.clone(),
            display_bpm: chart.display_bpm,
        }
    }

    fn put(&self, chart: &mut Chart) {
        chart.difficulty = self.difficulty;
        chart.meter = self.meter;
        chart.name = self.name.clone();
        chart.description = self.description.clone();
        chart.credit = self.credit.clone();
        chart.display_bpm = self.display_bpm;
    }

    /// As a file keeps it: text trimmed, an empty name the description
    /// (`.ssc`'s `#CHARTNAME` falls back to it).
    pub(crate) fn tidy(mut self) -> ChartInfo {
        self.name = self.name.trim().to_string();
        self.description = self.description.trim().to_string();
        self.credit = self.credit.trim().to_string();
        if self.name.is_empty() {
            self.name = self.description.clone();
        }
        self.display_bpm = self
            .display_bpm
            .map(tidy_display_bpm)
            .filter(|b| *b != DisplayBpm::Actual);
        self
    }
}

fn finite_or(v: f64, or: f64) -> f64 {
    if v.is_finite() { v } else { or }
}

fn tidy_display_bpm(bpm: DisplayBpm) -> DisplayBpm {
    match bpm {
        DisplayBpm::Single(v) if !v.is_finite() => DisplayBpm::Actual,
        DisplayBpm::Range(a, b) if !a.is_finite() || !b.is_finite() => DisplayBpm::Actual,
        DisplayBpm::Range(a, b) if a == b => DisplayBpm::Single(a),
        other => other,
    }
}
