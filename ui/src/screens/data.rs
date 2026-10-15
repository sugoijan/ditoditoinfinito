//! What the theme's screens show, as plain data a shell fills (step 6
//! from the library, the engine's records and the settings; the
//! developer routes from [`super::sample`]).

use ddi_chart::Difficulty;

/// The difficulties in their order (StepMania's `Difficulty` enum).
pub const DIFFICULTIES: [Difficulty; 6] = [
    Difficulty::Beginner,
    Difficulty::Easy,
    Difficulty::Medium,
    Difficulty::Hard,
    Difficulty::Challenge,
    Difficulty::Edit,
];

/// A difficulty's place in [`DIFFICULTIES`].
pub fn difficulty_index(d: Difficulty) -> usize {
    DIFFICULTIES.iter().position(|x| *x == d).unwrap_or(0)
}

/// A difficulty's name, as commands and colours are named
/// (`Difficulty.Hard`).
pub fn difficulty_name(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Beginner => "Beginner",
        Difficulty::Easy => "Easy",
        Difficulty::Medium => "Medium",
        Difficulty::Hard => "Hard",
        Difficulty::Challenge => "Challenge",
        Difficulty::Edit => "Edit",
    }
}

/// A style (a chart layout: `dance-single`, shown as "Single").
#[derive(Clone, Debug, PartialEq)]
pub struct StyleDef {
    pub id: String,
    pub name: String,
}

/// A chart of a song.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartEntry {
    /// Its style's id.
    pub style: String,
    pub difficulty: Difficulty,
    pub meter: u32,
    pub notes: u32,
    /// Its step artist.
    pub credit: String,
}

/// A song as song select shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SongEntry {
    /// Stable within the list (the library's id).
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub pack: String,
    /// The display BPM (the beat pulse's tempo).
    pub bpm: f32,
    /// As shown ("160", "90–180").
    pub bpm_text: String,
    /// Seconds.
    pub length: f32,
    /// Texture sources ([`crate::Cx::texture`]).
    pub banner: Option<String>,
    pub background: Option<String>,
    /// Easiest first within a style.
    pub charts: Vec<ChartEntry>,
}

impl SongEntry {
    /// Its charts in style `style`, easiest first.
    pub fn charts_in(&self, style: &str) -> Vec<&ChartEntry> {
        let mut v: Vec<&ChartEntry> = self.charts.iter().filter(|c| c.style == style).collect();
        v.sort_by_key(|c| (difficulty_index(c.difficulty), c.meter));
        v
    }

    /// The image behind the menus: its background, else its banner.
    pub fn backdrop(&self) -> Option<&str> {
        self.background.as_deref().or(self.banner.as_deref())
    }

    /// "2:26".
    pub fn length_text(&self) -> String {
        clock(self.length)
    }
}

/// Seconds as "m:ss".
pub fn clock(seconds: f32) -> String {
    let s = seconds.max(0.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The chart of `charts` (easiest first) closest to `preferred`: the
/// same difficulty, else the nearest, the easier on a tie (StepMania's
/// `ScreenSelectMusic::SwitchToPreferredDifficulty`, which keeps the
/// first of equal differences in its easiest-first list).
pub fn closest(charts: &[&ChartEntry], preferred: Difficulty) -> Option<usize> {
    let want = difficulty_index(preferred) as i32;
    let mut best: Option<(usize, i32)> = None;
    for (i, c) in charts.iter().enumerate() {
        let d = (difficulty_index(c.difficulty) as i32 - want).abs();
        if best.is_none_or(|(_, b)| d < b) {
            best = Some((i, d));
        }
    }
    best.map(|(i, _)| i)
}

/// Song select's data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectData {
    /// Every style the library has charts in, in the order the tabs show.
    pub styles: Vec<StyleDef>,
    /// In list order, grouped by pack.
    pub songs: Vec<SongEntry>,
    /// What is in effect, for the header ("×2.5", "Reverse", "ITG").
    pub mods: Vec<String>,
    /// The difficulty picked at first (the player's last; Hard when
    /// none).
    pub preferred: Option<Difficulty>,
}

/// A judgement row of results.
#[derive(Clone, Debug, PartialEq)]
pub struct JudgementCount {
    /// As the ruleset names it ("Fantastic").
    pub name: String,
    /// Its colour's name (`Judgement.W1` … `Judgement.Miss`).
    pub color: String,
    pub count: u32,
}

/// One hit of a play: when (seconds into the song) and how far off
/// (milliseconds, negative early); `None` for a miss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub time: f32,
    pub offset: Option<f32>,
    pub lane: u8,
}

/// Results' data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResultsData {
    pub song: SongEntry,
    /// "Hard 13 · Single · ITG rules · ×2.5 Reverse".
    pub line: String,
    pub grade: String,
    /// Percent.
    pub score: f32,
    pub ex: f32,
    pub judgements: Vec<JudgementCount>,
    /// (label, value) pairs under the table ("Held / let go", "13 / 1").
    pub stats: Vec<(String, String)>,
    pub hits: Vec<Hit>,
    /// The judgement windows' edges (ms) the histogram colours by, best
    /// first, with their colours' names.
    pub windows: Vec<(f32, String)>,
    /// Life over the song: (seconds, 0..=1).
    pub life: Vec<(f32, f32)>,
    /// Lane names ("Left", "Down", …).
    pub lanes: Vec<String>,
    /// The song's offset now, ms.
    pub song_offset: i32,
    /// A note under the footer ("Sample play: …").
    pub note: String,
}

impl ResultsData {
    /// The mean and standard deviation of the hits' offsets, ms.
    pub fn timing(&self) -> (f32, f32) {
        let v: Vec<f32> = self.hits.iter().filter_map(|h| h.offset).collect();
        if v.is_empty() {
            return (0.0, 0.0);
        }
        let n = v.len() as f32;
        let mean = v.iter().sum::<f32>() / n;
        let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n;
        (mean, var.sqrt())
    }
}

/// An option's values.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionKind {
    /// Named choices, one chosen.
    Choices(Vec<String>),
    /// A number stepped through `values` (shown as given).
    Stepper(Vec<String>),
}

/// An option row.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionDef {
    pub label: String,
    pub kind: OptionKind,
    pub index: usize,
    /// A note beside a stepper ("400 at this song").
    pub note: String,
    /// What it does, in a sentence.
    pub description: String,
    /// A heading above it ("SCROLL").
    pub heading: Option<String>,
}

impl OptionDef {
    pub fn values(&self) -> &[String] {
        match &self.kind {
            OptionKind::Choices(v) | OptionKind::Stepper(v) => v,
        }
    }

    pub fn value(&self) -> &str {
        self.values().get(self.index).map_or("", String::as_str)
    }
}

/// A section of options.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionSection {
    pub name: String,
    pub options: Vec<OptionDef>,
}

/// Options' data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionsData {
    pub sections: Vec<OptionSection>,
    /// Under the field preview ("Dito · Single · ×2.5 · Reverse").
    pub preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart(d: Difficulty) -> ChartEntry {
        ChartEntry {
            style: "s".into(),
            difficulty: d,
            meter: 1,
            notes: 1,
            credit: String::new(),
        }
    }

    #[test]
    fn the_closest_chart_prefers_the_easier_on_a_tie() {
        let m = chart(Difficulty::Medium);
        let c = chart(Difficulty::Challenge);
        let b = chart(Difficulty::Beginner);
        assert_eq!(closest(&[&m, &c], Difficulty::Hard), Some(0));
        assert_eq!(closest(&[&b, &c], Difficulty::Hard), Some(1));
        assert_eq!(closest(&[&b], Difficulty::Edit), Some(0));
        assert_eq!(closest(&[], Difficulty::Hard), None);
        assert_eq!(clock(146.4), "2:26");
        assert_eq!(clock(108.0), "1:48");
    }
}
