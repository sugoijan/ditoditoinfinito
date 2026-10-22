//! The play history (`docs/plans/play-history.md`, step 4): what a play
//! keeps, what is worked out from it, and the bests, platform-free.
//!
//! A play is stored as a [`PlayEntry`] (`ddi_platform::history`): the raw
//! [`PlayRecord`] (never rewritten), the engine's encoded input log and
//! timing as played, and a [`PlaySummary`] worked out from them, which is
//! worked out again ([`rework`]) when the judge has changed since
//! ([`JUDGE_VERSION`]). [`History`] holds the summaries in memory and
//! answers for the bests; [`Plays`] puts it over a [`PlayStore`] for the
//! shells, and [`Reworker`] says when stale plays are worked out again.
//! An export is read back by [`PlayImport`].

mod import;
#[cfg(test)]
mod import_tests;
mod plays;
mod replay;
mod reworker;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};

use ddi_chart::Song;
use ddi_engine::player::{PlayOptions, Results};
use ddi_engine::rules::ddr::DdrClear;
use ddi_engine::rules::ffr::Flag;
use ddi_engine::rules::{FullCombo, Judgement, ScoreView, Tally};
pub use ddi_platform::history::{PlayEntry, PlayStore, Reply, is_play_id, play_id};
use serde::{Deserialize, Serialize};

// The engine's and the chart's history API (steps 1–3).
pub use ddi_chart::ChartDigest;
pub use ddi_engine::JUDGE_VERSION;
pub use ddi_engine::history::{LogError, LoggedInput, PlayLog, PlayTiming, replay};
pub use ddi_engine::rules::RulesetKey;
pub use import::{
    ImportReport, Imported, MAX_LINE, MAX_PLAYS, MAX_PROBLEMS, PlayImport, Refused, read_line,
};
pub use plays::{Plays, Reworked, export_lines, rework_entries};
pub use replay::{
    Ghost, GhostDelta, GhostRun, Replay, ReplayCheck, Replaying, local_time, play_rows, plays_list,
};
pub use reworker::{FOCUS_STEP_MS, PASS_STEP_MS, REWORK_BATCH, ReworkStep, Reworker};

/// The version of [`PlayRecord`]'s and [`PlaySummary`]'s format. The
/// reader keeps reading every past one.
pub const RECORD_VERSION: u32 = 1;

/// Which chart a play was of: the song, its chart's content (a chart
/// whose notes or timing change is another chart), and where it was in
/// the song (to find it again).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChartKey {
    pub song: String,
    pub digest: ChartDigest,
    pub index: u32,
}

/// How a play ended. Quits are not kept for now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ending {
    Finished,
    Failed,
    Quit,
}

/// A play the game played: kept, never counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Autoplay {
    /// Through the tests' fake pad (`auto=pad`).
    #[serde(default)]
    pub pad: bool,
    /// Presses late by this many seconds (negative: early).
    #[serde(default)]
    pub bias: f64,
}

/// Where an offset in effect came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OffsetSource {
    /// The device's profile, calibrated.
    Calibrated,
    /// The device's profile, not calibrated (copied from another).
    Profile,
    /// No profile: the settings' default.
    #[default]
    Default,
}

/// An audio path or a display, as the offsets are kept by.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub label: String,
}

/// The clock and the offsets a play ran with.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockSetup {
    /// Seconds.
    pub audio_offset: f64,
    pub audio_source: OffsetSource,
    pub visual_offset: f64,
    pub visual_source: OffsetSource,
    pub audio: Option<Device>,
    pub display: Option<Device>,
    /// The output latency as the play started, seconds.
    pub output_latency: f64,
    /// Hz.
    pub sample_rate: f64,
    /// The display's refresh rate as the play started, Hz.
    pub refresh_hz: f64,
    /// The output latency's range over the play, seconds.
    pub latency_min: f64,
    pub latency_max: f64,
}

/// How a play was drawn. Settings by their stored names.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrawSetup {
    /// `None`: the built-in arrows.
    pub noteskin: Option<String>,
    /// Dito's note colors and its toggle for a chart's own colors (the
    /// built-in arrows draw in them too).
    pub note_colors: String,
    pub chart_colors: bool,
    /// The values set for the drawn skin's parameters
    /// (`docs/plans/skin-parameters.md`; Dito's when none was chosen).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub skin_params: crate::state::settings::Values,
    pub field_filter: f32,
    pub bg_brightness: f32,
    pub background_frame: String,
    pub video: String,
    /// The timing-error display.
    pub show_deltas: bool,
    /// The graphics backend (`WebGPU`, `WebGL2`, …).
    pub graphics: String,
}

/// An input device a play's lanes were pressed with.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum InputDevice {
    Keyboard,
    Touch,
    /// A controller, by its model (`Gamepad.id`).
    Pad {
        model: String,
    },
    Other {
        name: String,
    },
}

/// Every parameter of a play beyond what judging reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaySetup {
    pub clock: ClockSetup,
    pub drawn: DrawSetup,
    pub inputs: Vec<InputDevice>,
}

/// A play as played: never rewritten. The input log and the timing as
/// played are kept beside it, encoded ([`PlayEntry::log`],
/// [`PlayEntry::timing`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayRecord {
    pub version: u32,
    pub chart: ChartKey,
    pub ruleset: RulesetKey,
    pub options: PlayOptions,
    /// The song offset in effect (seconds; the song was played shifted
    /// by it, `Song::shift_notes`).
    pub song_offset: f64,
    #[serde(default)]
    pub setup: PlaySetup,
    /// The settings as stored when it was played.
    #[serde(default)]
    pub settings: serde_json::Value,
    pub ended: Ending,
    #[serde(default)]
    pub autoplay: Option<Autoplay>,
    /// Milliseconds since the Unix epoch.
    pub time: f64,
    #[serde(default)]
    pub app_version: String,
    /// The judge that judged it ([`JUDGE_VERSION`] then; 1 in records
    /// kept before it was written down, when there was no other): its
    /// timing is that judge's.
    #[serde(default = "first_judge")]
    pub judge_version: u32,
}

fn first_judge() -> u32 {
    1
}

/// The lamp a play earns: failed, cleared, or a full combo's kind,
/// better to the end. Under DDR A's rules, LIFE4's clear types too
/// (`docs/plans/rulesets.md`): a clear under the LIFE4 gauge (or RISKY)
/// and single digit perfects, a perfect full combo short of a marvelous
/// one by 1 to 9 Perfects. Under FFR's rules, its flags instead of the
/// full combos' kinds (`ddi_engine::rules::ffr::Flag`, R^3's song icons
/// in its order): a clear, a full combo, then single digit goods to AAA,
/// which only FFR's rules earn and compare among themselves (bests are
/// kept by ruleset).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lamp {
    Failed,
    Clear,
    Life4Clear,
    FullCombo,
    GreatFullCombo,
    PerfectFullCombo,
    SingleDigitPerfects,
    MarvelousFullCombo,
    SingleDigitGoods,
    Omniflag,
    Missflag,
    Averageflag,
    Blackflag,
    Booflag,
    Aaa,
}

impl Lamp {
    pub fn of(r: &Results) -> Lamp {
        if r.failed {
            Lamp::Failed
        } else {
            Lamp::played(r)
        }
    }

    /// The lamp the play's judgements earn, failed or not (the results
    /// show a full combo's kind beside FAILED).
    pub fn played(r: &Results) -> Lamp {
        if let Some(ffr) = &r.ffr {
            return match ffr.flag {
                Flag::Pass => Lamp::Clear,
                Flag::FullCombo => Lamp::FullCombo,
                Flag::SingleDigitGoods => Lamp::SingleDigitGoods,
                Flag::Omniflag => Lamp::Omniflag,
                Flag::Missflag => Lamp::Missflag,
                Flag::Averageflag => Lamp::Averageflag,
                Flag::Blackflag => Lamp::Blackflag,
                Flag::Booflag => Lamp::Booflag,
                Flag::Aaa => Lamp::Aaa,
            };
        }
        let ddr = r.ddr.as_ref();
        match r.full_combo {
            FullCombo::None if ddr.is_some_and(DdrClear::life4) => Lamp::Life4Clear,
            FullCombo::None => Lamp::Clear,
            FullCombo::FC => Lamp::FullCombo,
            FullCombo::GreatFC => Lamp::GreatFullCombo,
            FullCombo::PerfectFC if ddr.is_some() && r.tally.count(Judgement::W2) < 10 => {
                Lamp::SingleDigitPerfects
            }
            FullCombo::PerfectFC => Lamp::PerfectFullCombo,
            FullCombo::MarvelousFC => Lamp::MarvelousFullCombo,
        }
    }

    /// The lamp as colors and commands name it, and written short.
    pub fn names(self) -> (&'static str, &'static str) {
        match self {
            Lamp::Failed => ("Failed", "FAILED"),
            Lamp::Clear => ("Clear", "CLEAR"),
            Lamp::Life4Clear => ("Life4Clear", "LIFE4"),
            Lamp::FullCombo => ("FullCombo", "FC"),
            Lamp::GreatFullCombo => ("GreatFullCombo", "GFC"),
            Lamp::PerfectFullCombo => ("PerfectFullCombo", "PFC"),
            Lamp::SingleDigitPerfects => ("SingleDigitPerfects", "SDP"),
            Lamp::MarvelousFullCombo => ("MarvelousFullCombo", "MFC"),
            Lamp::SingleDigitGoods => ("SingleDigitGoods", "SDG"),
            Lamp::Omniflag => ("Omniflag", "OMNIFLAG"),
            Lamp::Missflag => ("Missflag", "MISSFLAG"),
            Lamp::Averageflag => ("Averageflag", "AVFLAG"),
            Lamp::Blackflag => ("Blackflag", "BLACKFLAG"),
            Lamp::Booflag => ("Booflag", "BOOFLAG"),
            Lamp::Aaa => ("Aaa", "AAA"),
        }
    }

    /// The lamp written out as the results show it; failed and a plain
    /// clear show none.
    pub fn label(self) -> Option<&'static str> {
        match self {
            Lamp::Failed | Lamp::Clear => None,
            Lamp::Life4Clear => Some("LIFE4 CLEAR"),
            Lamp::FullCombo => Some("FULL COMBO"),
            Lamp::GreatFullCombo => Some("GREAT FULL COMBO"),
            Lamp::PerfectFullCombo => Some("PERFECT FULL COMBO"),
            Lamp::SingleDigitPerfects => Some("SINGLE DIGIT PERFECTS"),
            Lamp::MarvelousFullCombo => Some("MARVELOUS FULL COMBO"),
            Lamp::SingleDigitGoods => Some("SINGLE DIGIT GOODS"),
            Lamp::Omniflag => Some("OMNIFLAG"),
            Lamp::Missflag => Some("MISSFLAG"),
            Lamp::Averageflag => Some("AVERAGEFLAG"),
            Lamp::Blackflag => Some("BLACKFLAG"),
            Lamp::Booflag => Some("BOOFLAG"),
            Lamp::Aaa => Some("AAA"),
        }
    }
}

/// What is worked out from a play's raw data, by the judge of
/// `judge_version`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Worked {
    pub judge_version: u32,
    pub score: ScoreView,
    pub grade: String,
    pub lamp: Lamp,
    pub full_combo: FullCombo,
    pub assist: bool,
    pub failed: bool,
    pub tally: Tally,
    pub max_combo: u32,
    pub fast: u32,
    pub slow: u32,
    #[serde(default)]
    pub hits: u32,
    /// Seconds.
    pub mean: f64,
    pub stddev: f64,
    /// Presses that reached the play later than it judged by time
    /// (`Results::late_edges`; not shown yet).
    #[serde(default)]
    pub late_edges: u32,
}

impl Worked {
    pub fn from_results(r: &Results) -> Worked {
        Worked {
            judge_version: JUDGE_VERSION,
            score: r.score,
            grade: r.grade.clone(),
            lamp: Lamp::of(r),
            full_combo: r.full_combo,
            assist: r.assist,
            failed: r.failed,
            tally: r.tally,
            max_combo: r.max_combo,
            fast: r.fast,
            slow: r.slow,
            hits: r.hits,
            mean: r.mean_delta,
            stddev: r.stddev_delta,
            late_edges: r.late_edges,
        }
    }

    /// Worked out by an older judge.
    pub fn is_stale(&self) -> bool {
        self.judge_version < JUDGE_VERSION
    }
}

/// The score a best is chosen by: money where the ruleset counts it,
/// else the percentage, else the EX score.
pub fn score_value(v: &ScoreView) -> f64 {
    v.money
        .map(|m| m as f64)
        .or(v.percent)
        .or(v.ex.map(f64::from))
        .unwrap_or(0.0)
}

/// A best's score as the ruleset scores: money, a percentage, EX.
pub fn score_text(v: &ScoreView) -> String {
    match (v.money, v.percent, v.ex) {
        (Some(m), _, _) => m.to_string(),
        (None, Some(p), _) => format!("{:.2}%", p * 100.0),
        (None, None, Some(ex)) => format!("EX {ex}"),
        _ => format!("{:.0}", score_value(v)),
    }
}

/// A field that does not read (a later format, a damaged store) is none:
/// a `worked` then is stale and is worked out again from its record.
fn lenient_some<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(
    d: D,
) -> Result<Option<T>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).ok())
}

/// What is loaded at start for every play: what it was of, and what it
/// came to (`None` until it is worked out again, when it did not read).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaySummary {
    pub version: u32,
    pub chart: ChartKey,
    pub ruleset: RulesetKey,
    /// Milliseconds since the Unix epoch.
    pub time: f64,
    #[serde(default)]
    pub autoplay: bool,
    pub ended: Ending,
    #[serde(default, deserialize_with = "lenient_some")]
    pub worked: Option<Worked>,
    /// The options as played (the plays list shows them; none in
    /// summaries kept before they were, or that do not read).
    #[serde(default, deserialize_with = "lenient_some")]
    pub options: Option<PlayOptions>,
}

impl PlaySummary {
    /// Whether the play counts toward the bests: the player's own, played
    /// to the end or failed. Failed plays count, as StepMania keeps them
    /// among the high scores (graded Failed).
    pub fn counts(&self) -> bool {
        !self.autoplay && self.ended != Ending::Quit
    }

    /// Worked out by an older judge, or not readable.
    pub fn is_stale(&self) -> bool {
        self.worked.as_ref().is_none_or(Worked::is_stale)
    }

    /// The summary of a play from its record, to be worked out.
    pub fn of(record: &PlayRecord) -> PlaySummary {
        PlaySummary {
            version: RECORD_VERSION,
            chart: record.chart.clone(),
            ruleset: record.ruleset.clone(),
            time: record.time,
            autoplay: record.autoplay.is_some(),
            ended: record.ended,
            worked: None,
            options: Some(record.options),
        }
    }

    /// The summary of a play from its record alone, to be worked out
    /// (a summary that was lost or does not read).
    pub fn from_record(record: &str) -> Result<PlaySummary, String> {
        #[derive(Deserialize)]
        struct Keys {
            chart: ChartKey,
            ruleset: RulesetKey,
            time: f64,
            ended: Ending,
            #[serde(default)]
            autoplay: Option<serde_json::Value>,
            #[serde(default)]
            options: Option<serde_json::Value>,
        }
        let k: Keys = serde_json::from_str(record).map_err(|e| e.to_string())?;
        Ok(PlaySummary {
            version: RECORD_VERSION,
            chart: k.chart,
            ruleset: k.ruleset,
            time: k.time,
            autoplay: k.autoplay.is_some_and(|a| !a.is_null()),
            ended: k.ended,
            worked: None,
            options: k.options.and_then(|o| serde_json::from_value(o).ok()),
        })
    }
}

/// A new play: its entry to store and its summary.
pub fn new_entry(
    id: String,
    record: &PlayRecord,
    log: &PlayLog,
    results: &Results,
) -> Result<(PlayEntry, PlaySummary), String> {
    let summary = PlaySummary {
        worked: Some(Worked::from_results(results)),
        ..PlaySummary::of(record)
    };
    let entry = PlayEntry {
        id,
        summary: Some(serde_json::to_string(&summary).map_err(|e| e.to_string())?),
        record: serde_json::to_string(record).map_err(|e| e.to_string())?,
        log: log.encode(),
        timing: PlayTiming::from_results(results).encode(),
    };
    Ok((entry, summary))
}

/// What judging a play again reads from its record (the rest may be of a
/// later format and is left alone).
#[derive(Deserialize)]
struct JudgedBy {
    chart: ChartKey,
    ruleset: RulesetKey,
    #[serde(default)]
    options: PlayOptions,
    #[serde(default)]
    song_offset: f64,
}

/// The chart a play was of in `song` (the song as stored, unshifted):
/// where it was, or where a chart of the same content is now.
pub fn find_chart(song: &Song, chart: &ChartKey) -> Option<usize> {
    let at = chart.index as usize;
    if at < song.charts.len() && song.chart_digest(at) == chart.digest {
        return Some(at);
    }
    (0..song.charts.len()).find(|&i| song.chart_digest(i) == chart.digest)
}

/// Judges a stored play again from its log, on `song` (as stored, before
/// any song offset): what it comes to under today's judge.
pub fn rework(entry: &PlayEntry, song: &Song) -> Result<Worked, String> {
    let by: JudgedBy = serde_json::from_str(&entry.record).map_err(|e| e.to_string())?;
    let index = find_chart(song, &by.chart).ok_or("the chart is no longer there")?;
    let log = PlayLog::decode(&entry.log).map_err(|e| e.to_string())?;
    let song = song.shifted(by.song_offset);
    let results = replay(&song, index, &by.ruleset, &by.options, &log);
    Ok(Worked::from_results(&results))
}

/// A best's key: a chart's content, the rules it was played by (without
/// the gauge chosen) and whether notes were removed (assist plays are
/// apart, as StepMania marks them). Not the song: a pack imported again
/// from another folder gets new song ids, and its charts keep their
/// bests.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BestKey {
    pub digest: ChartDigest,
    pub ruleset: RulesetKey,
    pub assist: bool,
}

/// The best of a chart under one ruleset: the best score and the best
/// lamp (from different plays, maybe), how often it was played and when
/// last.
#[derive(Clone, Debug, PartialEq)]
pub struct Best {
    pub score: ScoreView,
    pub grade: String,
    pub lamp: Lamp,
    pub plays: u32,
    /// Milliseconds since the Unix epoch.
    pub last_played: f64,
    /// A play under this key waits to be worked out again: the values
    /// are the old judge's (or none yet) until then.
    pub pending: bool,
    /// The plays the score and the lamp are from.
    pub score_play: String,
    pub lamp_play: String,
}

/// The summaries of every play kept, in memory.
#[derive(Clone, Debug, Default)]
pub struct History {
    plays: BTreeMap<String, PlaySummary>,
    /// Plays whose summary is missing or does not read, until rebuilt from
    /// their records ([`Plays::load`]); listed, never dropped.
    unreadable: Vec<String>,
}

impl History {
    /// From the store's `(id, summary)` pairs (`None`: a play with no
    /// summary); summaries that do not read are listed apart
    /// ([`History::unreadable`]), never dropped.
    pub fn from_summaries(summaries: Vec<(String, Option<String>)>) -> History {
        let mut h = History::default();
        for (id, json) in summaries {
            match json.and_then(|j| serde_json::from_str::<PlaySummary>(&j).ok()) {
                Some(s) => {
                    h.plays.insert(id, s);
                }
                None => h.unreadable.push(id),
            }
        }
        h
    }

    pub fn len(&self) -> usize {
        self.plays.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plays.is_empty() && self.unreadable.is_empty()
    }

    pub fn unreadable(&self) -> &[String] {
        &self.unreadable
    }

    pub fn get(&self, id: &str) -> Option<&PlaySummary> {
        self.plays.get(id)
    }

    /// Whether play `id` is kept (its summary read or not).
    pub fn contains(&self, id: &str) -> bool {
        self.plays.contains_key(id) || self.unreadable.iter().any(|u| u == id)
    }

    /// Every play, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &PlaySummary)> {
        self.plays.iter()
    }

    /// The latest play.
    pub fn last(&self) -> Option<(&String, &PlaySummary)> {
        self.plays.iter().next_back()
    }

    pub fn add(&mut self, id: String, summary: PlaySummary) {
        self.unreadable.retain(|u| *u != id);
        self.plays.insert(id, summary);
    }

    fn mark_unreadable(&mut self, id: String) {
        if !self.plays.contains_key(&id) && !self.unreadable.contains(&id) {
            self.unreadable.push(id);
        }
    }

    /// A play worked out again.
    pub fn set_worked(&mut self, id: &str, worked: Worked) -> Option<&PlaySummary> {
        let s = self.plays.get_mut(id)?;
        s.worked = Some(worked);
        Some(s)
    }

    pub fn remove(&mut self, ids: &[String]) {
        for id in ids {
            self.plays.remove(id);
        }
        self.unreadable.retain(|u| !ids.contains(u));
    }

    /// Every play's id (the unreadable ones too).
    pub fn ids(&self) -> Vec<String> {
        self.plays.keys().chain(&self.unreadable).cloned().collect()
    }

    /// The plays of a chart (by content, under any song id).
    pub fn ids_of_chart(&self, digest: ChartDigest) -> Vec<String> {
        self.select(|s| s.chart.digest == digest)
    }

    /// The plays of these songs (a pack's).
    pub fn ids_of_songs(&self, songs: &[String]) -> Vec<String> {
        self.select(|s| songs.contains(&s.chart.song))
    }

    /// The plays of `song` worked out by an older judge (or not
    /// readable), oldest first.
    pub fn stale_of(&self, song: &str) -> Vec<String> {
        self.select(|s| s.is_stale() && s.chart.song == song)
    }

    /// Whether a play of `song` waits to be worked out again.
    pub fn has_stale(&self, song: &str) -> bool {
        self.plays
            .values()
            .any(|s| s.is_stale() && s.chart.song == song)
    }

    /// How many plays wait to be worked out again.
    pub fn stale_count(&self) -> usize {
        self.plays.values().filter(|s| s.is_stale()).count()
    }

    fn select(&self, f: impl Fn(&PlaySummary) -> bool) -> Vec<String> {
        self.plays
            .iter()
            .filter(|(_, s)| f(s))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Every best.
    pub fn bests(&self) -> HashMap<BestKey, Best> {
        self.bests_where(|_| true)
    }

    /// The bests of these charts (a song's, by their digests), whatever
    /// song id they were played under.
    pub fn bests_of_charts(&self, digests: &[ChartDigest]) -> HashMap<BestKey, Best> {
        self.bests_where(|s| digests.contains(&s.chart.digest))
    }

    /// The best of a chart under a ruleset (the gauge chosen aside).
    pub fn best(&self, digest: ChartDigest, ruleset: &RulesetKey, assist: bool) -> Option<Best> {
        let key = BestKey {
            digest,
            ruleset: ruleset.best_key(),
            assist,
        };
        self.bests_where(|s| s.chart.digest == digest).remove(&key)
    }

    /// The best of `song`'s chart `chart` (as stored) under the rules and
    /// the assist the settings play it with: a play's ghost's.
    pub fn best_of_play(
        &self,
        song: &Song,
        chart: usize,
        settings: &crate::state::settings::Settings,
    ) -> Option<Best> {
        let c = song.charts.get(chart)?;
        self.best(
            song.chart_digest(chart),
            &settings.ruleset_for(c).key(),
            c.derivation.is_some() || settings.transform.is_assist(),
        )
    }

    fn bests_where(&self, f: impl Fn(&PlaySummary) -> bool) -> HashMap<BestKey, Best> {
        let mut out: HashMap<BestKey, Best> = HashMap::new();
        // Plays not readable yet: their charts' bests are pending, with
        // the others' values.
        let mut unread = Vec::new();
        for (id, s) in self.plays.iter().filter(|(_, s)| s.counts() && f(s)) {
            let Some(w) = &s.worked else {
                unread.push((s.chart.digest, s.ruleset.best_key()));
                continue;
            };
            let key = BestKey {
                digest: s.chart.digest,
                ruleset: s.ruleset.best_key(),
                assist: w.assist,
            };
            let b = out.entry(key).or_insert_with(|| Best {
                score: w.score,
                grade: w.grade.clone(),
                lamp: w.lamp,
                plays: 0,
                last_played: s.time,
                pending: false,
                score_play: id.clone(),
                lamp_play: id.clone(),
            });
            b.plays += 1;
            b.last_played = b.last_played.max(s.time);
            b.pending |= w.is_stale();
            if score_value(&w.score) > score_value(&b.score) {
                b.score = w.score;
                b.grade = w.grade.clone();
                b.score_play = id.clone();
            }
            if w.lamp > b.lamp {
                b.lamp = w.lamp;
                b.lamp_play = id.clone();
            }
        }
        for (k, b) in out.iter_mut() {
            b.pending |= unread
                .iter()
                .any(|(d, r)| *d == k.digest && *r == k.ruleset);
        }
        out
    }
}
